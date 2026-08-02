import { createEffect, onCleanup, createSignal } from "solid-js";
import { useEditor } from "./shell/EditorContext";
import { useDialog } from "./dialogs/DialogProvider";
import type { DocumentEngine } from "@/engine/document";
import type { DocumentModel } from "@/engine/types";
import type { CommandHistory } from "@/engine/history";
import { getPaintToolBlockReason, resolveEraserFill, type PaintToolSettings } from "./brushToolState";
import { commitPaintBitmap } from "./paintCommitCommand";
import { mapPaintPointToLayerLocal } from "./paintStrokeCoordinates";
import { showToast } from "./Toast";
import { applyBasicAdjustmentToColor, inverseBasicAdjustmentToColor } from "@/engine/layerAdjustments";
import {
  getBrushDabSpacing,
  getBrushTip,
  interpolateDabs,
  getEffectiveFlowMultiplier,
  type DirtyRect,
  emptyDirtyRect,
  expandDirtyRect,
  clampDirtyRect,
  parsePaintColor,
} from "./brushTipMask";

// ── Hold timer (time-based endpoint dab) ──
// During slow strokes or holds where interpolateDabs produces 0 dabs
// (movement < spacing), this RAF timer forces a dab at the last cursor
// position every DAB_HOLD_MS so the brush always "reaches" the cursor.
// Shares lastDabTime with all dab types (interpolated, terminal) to
// prevent double-fire. Runs from pointerdown to pointerup.
const DAB_HOLD_MS = 150;
let lastDabTime = 0;
let holdRaf: number | null = null;
let holdActive = false;
let holdTipExtent = 1;

interface Dab {
  x: number;
  y: number;
  alpha: number;
}

interface PaintStrokeSession {
  layerId: string;
  isEraser: boolean;
  settingsKey: string;
  color: string;
  /** Tip size (brush/eraser diameter) — stored directly to avoid parsing from settingsKey. */
  tipSize: number;
  /** Tip hardness 0..1 — stored directly to avoid parsing from settingsKey. */
  tipHardness: number;
  /** Positions of all dabs rendered so far (for GPU-accelerated drawImage composite). */
  dabPositions: Dab[];
  /** How many dabs have already been rendered to the overlay (incremental drawing). */
  dabsRendered: number;
  lastPoint: { x: number; y: number } | null;
  spacingCarry: number;
  /** Accumulated dirty region for this stroke. */
  dirtyRect: DirtyRect;
}

export function useBrushOverlay() {
  const {
    workspace, renderer, scheduler, fgColor, bgColor, docWidth, docHeight,
    activeTool, brushSize, brushHardness,
    eraserSize, eraserHardness,
  } = useEditor();
  const dialog = useDialog();

  let overlayCanvasRef: HTMLCanvasElement | null = null;
  let overlayCtx: CanvasRenderingContext2D | null = null;
  let prevStrokePointCount = 0;
  let strokeGen = 0;

  let paintSession: PaintStrokeSession | null = null;

  // Bake-on-paint WYSIWYG: the first stroke on an adjusted layer prompts to
  // bake the adjustment into the layer's pixels so the brush shows the exact
  // picked color (every color, not just the reachable gamut). `bakeDecisionSig`
  // remembers the per-(layer,adjustment) choice ("Paint as-is" sets it so we
  // never re-prompt); `bakePromptPending` blocks re-triggering / painting while
  // the modal is resolving. `preBake` captures the pre-bake snapshot so the
  // stroke's undo restores the adjustment.
  const [bakeDecisionSig, setBakeDecisionSig] = createSignal<string | null>(null);
  let bakePromptPending = false;
  let preBake: { layerId: string; snapshot: DocumentModel } | null = null;

  // ── Cached commit buffer ──
  // Reuses OffscreenCanvas across commits to avoid 107MB allocation per stroke end.
  // Cleared between strokes via clearRect. Reallocated only when layer dimensions change.
  let cachedCommitCanvas: OffscreenCanvas | null = null;
  let cachedCommitCtx: OffscreenCanvasRenderingContext2D | null = null;

  function startHoldTimer() {
    if (holdRaf !== null) return;
    holdActive = true;

    function tick() {
      if (!holdActive) return;
      const session = paintSession;
      if (session && session.lastPoint) {
        const now = performance.now();
        if (now - lastDabTime >= DAB_HOLD_MS && session.dabPositions.length > 0) {
          const lp = session.lastPoint;
          const lastDab = session.dabPositions.at(-1);
          // Skip if the last dab in the session is already at the cursor
          // position (within 1px) — user is holding still and the position
          // already has a dab. We just update lastDabTime so the timer
          // doesn't immediately fire when the user moves again.
          const sameAsLastDab = lastDab &&
            Math.abs(lastDab.x - lp.x) < 1 &&
            Math.abs(lastDab.y - lp.y) < 1;
          if (!sameAsLastDab) {
            // Push full-alpha dab immediately — no transparency, no
            // gradual fade-in. The user sees the dab at the cursor
            // position as soon as the hold timer fires.
            session.dabPositions.push({ x: lp.x, y: lp.y, alpha: compositeAlpha });
            session.dirtyRect = expandDirtyRect(session.dirtyRect, lp.x, lp.y, holdTipExtent);
            // Composite so user sees the dab immediately
            // Brush: synchronous (fast incremental drawImage, ~5ms per dab)
            // Eraser: RAF-scheduled (redraws all dabs + layer, can be slow)
            if (session.isEraser) {
              scheduleComposite();
            } else {
              const eng = workspace.getActiveEngine();
              const id = eng?.getActiveLayerId();
              const l = id ? eng?.getLayer(id) : null;
              if (eng && id && l) performComposite(eng, id, l, false);
            }
          }
          lastDabTime = now;
        }
      }
      holdRaf = requestAnimationFrame(tick);
    }

    holdRaf = requestAnimationFrame(tick);
  }

  function stopHoldTimer() {
    holdActive = false;
    if (holdRaf !== null) {
      cancelAnimationFrame(holdRaf);
      holdRaf = null;
    }
  }

  // ── Tip canvas cache ──
  // Pre-renders brush tip with paint color so we can use GPU-accelerated
  // drawImage instead of CPU mask loops (critical for large brushes).
  const tipCanvasCache = new Map<string, OffscreenCanvas | HTMLCanvasElement>();

  // ── Pre-warm tip cache on settings change ──
  // Rasterizes the brush tip AND generates the tip canvas BEFORE the user
  // clicks (during pointerdown). Eliminates the 400+ms first-stroke delay.
  // Uses setTimeout(300) debounce so heavy rasterization (e.g. 2000px brush =
  // 4M Float32Array elements) never fires mid-drag and blocks the main thread.
  createEffect(() => {
    const tool = activeTool();
    if (tool !== "brush" && tool !== "eraser") return;

    // Pick correct settings based on active tool
    const size = tool === "eraser" ? eraserSize() : brushSize();
    const hardness = tool === "eraser" ? eraserHardness() : brushHardness();
    const color = fgColor();

    // Debounce: only pre-warm after user stops dragging for 300ms.
    const id = setTimeout(() => {
      // Step 1: pre-warm Float32Array mask cache (getCachedBrushTip internally)
      const tip = getBrushTip({ size, hardness, curve: "soft" });
      if (!tip) return;

      // Step 2: pre-warm tip canvas cache (getTipCanvas internally)
      getTipCanvas(tip, color);
    }, 300);

    // Cleanup: cancel pending timeout if settings change again before it fires
    onCleanup(() => clearTimeout(id));
  });

  function getTipCanvas(tip: import("./brushTipMask").BrushTip, color: string): OffscreenCanvas | HTMLCanvasElement {
    const _t0 = performance.now();
    // Canvas size matches the data array resolution. For large brushes
    // (diameter > 256), the data is downsampled and the browser upscales
    // via drawImage destination dimensions — visually identical since
    // the brush alpha profile is smooth.
    const sz = tip.dataSize;
    const key = `tip:${tip.diameter}:${color}`;
    const cached = tipCanvasCache.get(key);
    if (cached) {
      tipCanvasCache.delete(key);
      tipCanvasCache.set(key, cached);
      return cached;
    }

    let canvas: OffscreenCanvas | HTMLCanvasElement;
    try {
      canvas = new OffscreenCanvas(sz, sz);
    } catch {
      // Fallback for environments without OffscreenCanvas (jsdom tests)
      canvas = document.createElement("canvas");
      canvas.width = sz;
      canvas.height = sz;
    }
    const ctx = canvas.getContext("2d") as OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D;
    const paint = parsePaintColor(color);
    const imgData = ctx.createImageData(sz, sz);
    const d = imgData.data;
    for (let i = 0; i < tip.data.length; i++) {
      const a = Math.round(tip.data[i] * 255);
      d[i * 4]     = paint.r;
      d[i * 4 + 1] = paint.g;
      d[i * 4 + 2] = paint.b;
      d[i * 4 + 3] = a;
    }
    ctx.putImageData(imgData, 0, 0);

    // LRU: max 16 entries
    if (tipCanvasCache.size >= 16) {
      const firstKey = tipCanvasCache.keys().next().value;
      if (firstKey !== undefined) tipCanvasCache.delete(firstKey);
    }
    tipCanvasCache.set(key, canvas);
    const _dt = performance.now() - _t0;
    if (_dt > 1) console.warn(`[perf] getTipCanvas: ${_dt.toFixed(1)}ms (sz=${sz}, d=${tip.diameter}, cache=${cached ? "HIT" : "MISS"})`);
    return canvas;
  }

  // ── Preview tip canvas (downscaled for smooth overlay composite) ──
  // For large brushes (diameter > 256px), the full-resolution tip canvas
  // is 4+ million pixels (e.g., 2000×2000). drawImage of this canvas
  // blocks the main thread for 5-15ms per dab, causing frame drops in
  // the RAF-throttled composite. The preview tip canvas is scaled down
  // to at most PREVIEW_MAX_SIZE px so drawImage is ~0.1ms.
  // The browser upscales the preview to the correct visual size when
  // drawn with destination dimensions — slightly blurry preview during
  // drag, crisp final composite on pointerUp.
  const PREVIEW_MAX_SIZE = 256;

  function getPreviewTipCanvas(tip: import("./brushTipMask").BrushTip, color: string): OffscreenCanvas | HTMLCanvasElement {
    const scale = Math.min(1, PREVIEW_MAX_SIZE / tip.diameter);
    if (scale >= 1) return getTipCanvas(tip, color); // no downscale needed

    const key = `preview:${tip.diameter}:${color}`;
    const cached = tipCanvasCache.get(key);
    if (cached) {
      tipCanvasCache.delete(key);
      tipCanvasCache.set(key, cached);
      return cached;
    }

    const fullCanvas = getTipCanvas(tip, color);
    const pw = Math.round(tip.diameter * scale);

    let preview: OffscreenCanvas | HTMLCanvasElement;
    try {
      preview = new OffscreenCanvas(pw, pw);
    } catch {
      preview = document.createElement("canvas");
      preview.width = pw;
      preview.height = pw;
    }
    const pCtx = preview.getContext("2d") as OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D;
    pCtx.drawImage(fullCanvas, 0, 0, pw, pw);

    // LRU: same cache as getTipCanvas — preview entries are small (~256KB vs 16MB)
    if (tipCanvasCache.size >= 16) {
      const firstKey = tipCanvasCache.keys().next().value;
      if (firstKey !== undefined) tipCanvasCache.delete(firstKey);
    }
    tipCanvasCache.set(key, preview);
    return preview;
  }

  // ── RAF throttle state ───────────────────────────────────────────
  // Stamping (mask accumulation) is always synchronous.
  // Composite runs at most once per RAF frame.
  // On isFinal, composite runs synchronously so commitBrushStroke can read the overlay.
  let compositeRaf: number | null = null;
  let compositeAlpha = 0;
  let compositePending = false;

  function getPaintSessionKey(settings: PaintToolSettings, color: string): string {
    return [
      Math.round(settings.size),
      Math.round(settings.hardness * 100),
      Math.round(settings.opacity * 100),
      Math.round(settings.flow * 100),
      color,
    ].join(":");
  }

  function onPaintStroke(
    points: { x: number; y: number }[],
    isEraser: boolean,
    settings: PaintToolSettings,
    isFinal = false,
  ) {
    const _t0 = performance.now();
    const activeEngine = workspace.getActiveEngine();
    if (!activeEngine) return;
    const activeId = activeEngine.getActiveLayerId();
    if (!activeId) return;

    const layer = activeEngine.getLayer(activeId);
    if (!layer) return;

    // ── Resolve eraser behavior for background layers ──
    // Eraser on a Background layer paints with the
    // background color (source-over) instead of erasing to transparent.
    // Non-background layers still erase to transparent (destination-out).
    // NOTE: resolveEraserFill is ONLY called for isEraser=true. For brush
    // (isEraser=false) it returns bogus defaults — the function was designed
    // solely for the eraser case and was never tested with isEraser=false.
    const eraserFill = isEraser ? resolveEraserFill(layer, true, bgColor()) : null;
    const effectiveIsEraser = eraserFill ? eraserFill.isEraser : false;
    const effectiveColor = eraserFill ? eraserFill.color : fgColor();

    // Block check: use effectiveIsEraser so background layer eraser
    // (which paints with bgColor) is not blocked by lockTransparency.
    const blockedReason = getPaintToolBlockReason(layer, effectiveIsEraser);
    if (blockedReason) return;

    if (!overlayCanvasRef) return;
    if (!overlayCtx) {
      overlayCtx = overlayCanvasRef.getContext("2d");
    }
    if (!overlayCtx) return;

    if (overlayCanvasRef.width !== layer.width || overlayCanvasRef.height !== layer.height) {
      overlayCanvasRef.width = layer.width;
      overlayCanvasRef.height = layer.height;
    }
    // brush tip pipeline owns both soft and hard edges. The previous
    // ctx.lineCap=round shortcut for hardness>=1 produced browser-dependent
    // AA and bypassed the mask engine entirely.
    const settingsKey = getPaintSessionKey(settings, effectiveColor);
    const needsReset =
      !paintSession ||
      paintSession.layerId !== activeId ||
      paintSession.isEraser !== effectiveIsEraser ||
      paintSession.settingsKey !== settingsKey ||
      prevStrokePointCount === 0;

    if (needsReset) {
      // Bake-on-paint WYSIWYG gate: the first stroke on an adjusted layer with
      // a bitmap prompts to bake the adjustment into pixels so the brush shows
      // the exact picked color. The dialog is async (modal), so the stroke is
      // deferred to the next pointerdown; baking happens on confirm.
      const adj = layer.basicAdjustment;
      const bakeKey = adj ? `${activeId}:${adj.brightness},${adj.contrast},${adj.saturation}` : null;
      if (adj && bakeDecisionSig() !== bakeKey && layer.imageBitmap) {
        if (!bakePromptPending) {
          bakePromptPending = true;
          dialog
            .confirm({
              title: "Apply adjustment to layer?",
              message:
                "Painting on an adjusted layer shows the shader-adjusted color. Apply the adjustment to the layer now so your brush color appears exactly as picked?",
              confirmLabel: "Apply & Paint",
              cancelLabel: "Paint as-is",
            })
            .then(async (confirmed) => {
              bakePromptPending = false;
              if (confirmed) {
                const snap = activeEngine.snapshot();
                const bakeResult = await activeEngine.commitBasicAdjustment(activeId, renderer);
                // Loud fallback: the GPU bake was available but failed, so we
                // silently dropped to the slow CPU loop — surface it so a
                // regression to the 150–400ms hitch isn't invisible.
                if (bakeResult === "cpu" && typeof renderer?.bakeLayerToBitmap === "function") {
                  showToast(
                    "Layer adjustment bake fell back to CPU — painting may stutter on large layers.",
                    "warn",
                  );
                }
                const bakedLayer = activeEngine.getLayer(activeId);
                if (bakedLayer?.imageBitmap) renderer.uploadImage(activeId, bakedLayer.imageBitmap);
                scheduler.requestRender();
                // One undo restores the pre-bake model (adjustment still applied).
                preBake = { layerId: activeId, snapshot: snap };
              } else {
                // Remember the choice so we don't re-prompt for this adjustment.
                setBakeDecisionSig(bakeKey);
              }
            });
        }
        // Defer the stroke — the modal ended the gesture; the user clicks again.
        return;
      }

      // Invalidate any in-flight commit from the previous stroke so its
      // createImageBitmap result can't clobber this stroke's live overlay.
      strokeGen++;

      paintSession = {
        layerId: activeId,
        isEraser: effectiveIsEraser,
        settingsKey,
        color: effectiveColor,
        tipSize: settings.size,
        tipHardness: settings.hardness,
        dabPositions: [],
        dabsRendered: 0,
        lastPoint: null,
        spacingCarry: 0,
        dirtyRect: emptyDirtyRect(),
      };
      lastDabTime = performance.now();
      startHoldTimer();

      // Eraser: make the active layer invisible in WebGL so erased areas
      // reveal the correct checkerboard + layers behind through overlay holes.
      // Upload a 1×1 transparent texture — the WebGL renderer stretches it
      // across the full layer dimensions (texCoord is normalized 0–1), so
      // every pixel samples the transparent corner pixel.
      if (effectiveIsEraser) {
        try {
          const emptyCanvas = new OffscreenCanvas(1, 1);
          emptyCanvas.getContext("2d"); // needed before transferToImageBitmap
          const emptyBitmap = emptyCanvas.transferToImageBitmap();
          renderer.uploadImage(activeId, emptyBitmap);
          emptyBitmap.close();

          // Force a WebGL re-render so the 1×1 transparent texture takes
          // effect immediately. Without this, the layer remains visible in the
          // WebGL composite (pointer handler suppresses requestRender for paint
          // tools via NOOP), and destination-out overlay holes are invisible
          // because they reveal the same layer content underneath.
          scheduler.requestRender();

          // Seed overlay with full layer content. Destination-out dabs on
          // subsequent composites will cut holes, revealing the WebGL result
          // (other layers + checkerboard) underneath.
          if (layer.imageBitmap) {
            overlayCtx.globalCompositeOperation = "source-over";
            overlayCtx.drawImage(layer.imageBitmap, 0, 0);
          }
        } catch (err) {
          console.error("[eraser] preview init failed:", err);
        }
      }
    }

    if (!paintSession) return;

    const tip = getBrushTip({ size: settings.size, hardness: settings.hardness, curve: "soft" });
    const spacing = getBrushDabSpacing(settings.size, settings.hardness, settings.flow);
    const alphaScale = settings.opacity * settings.flow * getEffectiveFlowMultiplier(settings.hardness);
    // Track full dirty extents: the tip's pixel data spans its full diameter,
    // so we expand by half the (possibly enlarged) raster diameter.
    const tipExtent = Math.ceil(tip.diameter / 2) + 1;
    holdTipExtent = tipExtent;

    const startIndex = needsReset ? 0 : prevStrokePointCount;
    for (let i = startIndex; i < points.length; i++) {
      const pt = points[i];
      const localPt = mapPaintPointToLayerLocal(pt, layer);

      if (!paintSession.lastPoint) {
        paintSession.dabPositions.push({ x: localPt.x, y: localPt.y, alpha: alphaScale });
        paintSession.dirtyRect = expandDirtyRect(paintSession.dirtyRect, localPt.x, localPt.y, tipExtent);
      } else {
        const result = interpolateDabs(paintSession.lastPoint, localPt, spacing, paintSession.spacingCarry);
        paintSession.spacingCarry = result.carry;
        for (const dab of result.dabs) {
          paintSession.dabPositions.push({ x: dab.x, y: dab.y, alpha: alphaScale });
          paintSession.dirtyRect = expandDirtyRect(paintSession.dirtyRect, dab.x, dab.y, tipExtent);
        }
        if (result.dabs.length > 0) {
          lastDabTime = performance.now();
        }
      }
      paintSession.lastPoint = localPt;
      paintSession.dirtyRect = expandDirtyRect(paintSession.dirtyRect, localPt.x, localPt.y, tipExtent);
    }

    if (isFinal && paintSession.lastPoint) {
      const lp = paintSession.lastPoint;
      const last = paintSession.dabPositions.at(-1);
      // Always push terminal dab at the final position with full alpha.
      // Skip only if the LAST dab is a full-alpha interpolated dab
      // already at this exact position. Endpoint dabs (12% alpha) should
      // NOT suppress the terminal dab — the user would see the cursor
      // ahead of the paint trail if only the faint dab remains.
      if (!last ||
          Math.abs(last.x - lp.x) > 0.001 ||
          Math.abs(last.y - lp.y) > 0.001 ||
          last.alpha < alphaScale * 0.9) {
        paintSession.dabPositions.push({ x: lp.x, y: lp.y, alpha: alphaScale });
      }
    }

    // ── Store composite snapshot state ──
    compositeAlpha = alphaScale;

    // ── Composite ──
    // RAF-throttled for both brush AND eraser. For large brushes (2000px),
    // drawImage of the 2000×2000 tip canvas onto the layer-res overlay blocks
    // the main thread for 20-50ms per dab. Synchronous composite inside the
    // pointer event handler prevents the cursor overlay from updating
    // (BrushCursorOverlay.handleMove runs AFTER onCanvasPointerMove in the
    // bubble phase), causing the cursor to appear choppy ("patah patah")
    // during fast drag. RAF throttle bounds composite to 1× per frame,
    // keeping the cursor smooth at the cost of ~1 frame preview lag.
    if (isFinal) {
      stopHoldTimer();
      compositeNow(activeEngine, activeId, layer);
    } else {
      scheduleComposite();
    }

    prevStrokePointCount = points.length;
    const _dt = performance.now() - _t0;
    if (_dt > 3) console.warn(`[perf] onPaintStroke: ${_dt.toFixed(1)}ms (${points.length}pts, ${isFinal ? "final" : "move"}, ${paintSession?.dabPositions.length ?? 0}dabs)`);
  }

  /** Perform composite via GPU-accelerated drawImage from pre-rendered tip canvas */
  function performComposite(
    engine: DocumentEngine,
    layerId: string,
    layer: NonNullable<ReturnType<DocumentEngine["getLayer"]>>,
    final: boolean,
  ) {
    const _t0 = performance.now();
    const session = paintSession;
    if (!session) return;
    // Bail early if no new dabs to draw (e.g., guard skipped push at same position)
    if (!final && session.dabsRendered >= session.dabPositions.length) return;
    const alpha = compositeAlpha;

    const tip = getBrushTip({
      size: session.tipSize,
      hardness: session.tipHardness,
      curve: "soft",
    });

    const dirty = clampDirtyRect(session.dirtyRect, overlayCanvasRef?.width ?? 1, overlayCanvasRef?.height ?? 1);
    const hasDirty = dirty.x1 > dirty.x0 && dirty.y1 > dirty.y0 && session.dabPositions.length > 0;
    if (!hasDirty) return;

    // ── Preview tip canvas ──
    // Non-final strokes: use downscaled preview tip so drawImage of
    // 2000×2000 tip canvas doesn't block main thread for 5-15ms per dab.
    // The browser upscales the preview to the correct visual size via
    // destination dimensions in drawImage (slightly blurry drag preview,
    // crisp final composite on pointerUp).
    // WYSIWYG: store the inverse-adjusted color so the shader reproduces the
    // picked color on display. The overlay is a plain canvas (not
    // shader-adjusted), so the preview draws the *displayed* color directly
    // (apply(inverse(picked)) ≈ picked when in gamut) to stay pixel-identical
    // to the committed result — no color pop at release.
    const adj = layer.basicAdjustment;
    const commitDabColor = adj ? inverseBasicAdjustmentToColor(session.color, adj) : session.color;
    const previewDabColor = adj ? applyBasicAdjustmentToColor(commitDabColor, adj) : session.color;
    const dabColor = !final ? previewDabColor : commitDabColor;
    const compositeTipCanvas = !final ? getPreviewTipCanvas(tip, dabColor) : getTipCanvas(tip, dabColor);
    const cw = compositeTipCanvas.width;
    const ch = compositeTipCanvas.height;
    const tipRadius = tip.diameter / 2;

    const drawDab = (dab: Dab) => {
      overlayCtx!.globalAlpha = dab.alpha;
      overlayCtx!.drawImage(
        compositeTipCanvas,
        0, 0, cw, ch,
        Math.round(dab.x - tipRadius), Math.round(dab.y - tipRadius),
        tip.diameter, tip.diameter,
      );
    };

    if (session.isEraser) {
      // ── Eraser: live preview via destination-out ──
      // The overlay was seeded with layer content at stroke start. Apply new
      // dabs with destination-out compositing to cut holes — through those
      // holes the user sees the WebGL composited result (checkerboard + layers
      // behind the active layer). This gives a correct "real transparency"
      // preview that matches the final commit.
      if (final) {
        // Rebuild the overlay from scratch: non-final composites draw a
        // drag-feedback outline (rgba(0,0,0,0.25) circle) that would
        // otherwise be baked into the committed bitmap by commitBrushStroke
        // (which copies the overlay directly) — the reported "black edge"
        // on eraser strokes. Reconstructing from the layer content + ALL
        // dabs (no outline) guarantees a clean commit.
        overlayCtx!.globalCompositeOperation = "source-over";
        overlayCtx!.clearRect(0, 0, overlayCanvasRef!.width, overlayCanvasRef!.height);
        if (layer.imageBitmap) overlayCtx!.drawImage(layer.imageBitmap, 0, 0);
        overlayCtx!.globalCompositeOperation = "destination-out";
        for (let i = 0; i < session.dabPositions.length; i++) {
          drawDab(session.dabPositions[i]);
        }
        overlayCtx!.globalAlpha = 1;
        overlayCtx!.globalCompositeOperation = "source-over";
        session.dabsRendered = session.dabPositions.length;
        return;
      }
      const startFrom = session.dabsRendered;
      overlayCtx!.globalCompositeOperation = "destination-out";
      for (let i = startFrom; i < session.dabPositions.length; i++) {
        drawDab(session.dabPositions[i]);
      }
      overlayCtx!.globalAlpha = 1;
      overlayCtx!.globalCompositeOperation = "source-over";
      session.dabsRendered = session.dabPositions.length;

      // Draw a thin circle outline at the current (last) dab position as
      // explicit visual feedback for the eraser position, so the user
      // always sees SOMETHING even when the layer has no content to erase.
      // SKIP on final composite: the outline would pollute the overlay
      // and force commitBrushStroke to reconstruct from scratch instead
      // of reading the clean overlay directly.
      if (!final) {
        const lastDab = session.dabPositions[session.dabPositions.length - 1];
        if (lastDab) {
          overlayCtx!.strokeStyle = "rgba(0,0,0,0.25)";
          overlayCtx!.lineWidth = 1;
          overlayCtx!.beginPath();
          overlayCtx!.arc(Math.round(lastDab.x), Math.round(lastDab.y), tipRadius - 0.5, 0, Math.PI * 2);
          overlayCtx!.stroke();
        }
      }
    } else {
      // ── Brush: incremental drawImage (skip already-rendered dabs) ──
      // Hanya draw dabs BARU sejak composite terakhir. Tidak perlu clear
      // overlay karena source-over accumulation sudah benar.
      const startFrom = session.dabsRendered;
      if (startFrom === 0 && session.dabPositions.length > 0) {
        // First composite this stroke: clear dirty rect then draw all
        const subW = dirty.x1 - dirty.x0;
        const subH = dirty.y1 - dirty.y0;
        overlayCtx!.clearRect(dirty.x0, dirty.y0, subW, subH);
      }
      for (let i = startFrom; i < session.dabPositions.length; i++) {
        drawDab(session.dabPositions[i]);
      }
      overlayCtx!.globalAlpha = 1;
      session.dabsRendered = session.dabPositions.length;

      if (layer.lockTransparency && layer.imageBitmap) {
        overlayCtx!.globalCompositeOperation = "destination-in";
        overlayCtx!.drawImage(layer.imageBitmap, 0, 0);
        overlayCtx!.globalCompositeOperation = "source-over";
      }
    }
    const _dt = performance.now() - _t0;
    if (_dt > 2) console.warn(`[perf] performComposite: ${_dt.toFixed(1)}ms (${session.isEraser ? "eraser" : "brush"}, dabs=${session.dabPositions.length}, final=${final})`);
  }

  /** Schedule composite via RAF — at most 1× per frame */
  function scheduleComposite() {
    if (compositePending) return;
    compositePending = true;
    if (compositeRaf !== null) return;

    compositeRaf = requestAnimationFrame(() => {
      const _t0 = performance.now();
      compositeRaf = null;
      if (!compositePending) return;
      compositePending = false;

      const eng = workspace.getActiveEngine();
      const id = eng?.getActiveLayerId();
      const l = id ? eng?.getLayer(id) : null;
      if (!eng || !id || !l) return;

      performComposite(eng, id, l, false);
      const _dt = performance.now() - _t0;
      if (_dt > 2) console.warn(`[perf] RAF-scheduleComposite: ${_dt.toFixed(1)}ms`);
    });
  }

  /** Run composite synchronously — cancels any pending RAF. Used for isFinal events. */
  function compositeNow(
    engine: DocumentEngine,
    layerId: string,
    layer: NonNullable<ReturnType<DocumentEngine["getLayer"]>>,
  ) {
    if (compositeRaf !== null) {
      cancelAnimationFrame(compositeRaf);
      compositeRaf = null;
    }
    compositePending = false;
    performComposite(engine, layerId, layer, true);
  }

  async function commitBrushStroke(engine: DocumentEngine, history: CommandHistory, layerId: string, isEraser: boolean, anchor?: { x: number; y: number } | null) {
    const _t0 = performance.now();
    if (prevStrokePointCount === 0) return;
    if (!overlayCanvasRef) return;
    const w = overlayCanvasRef.width;
    const h = overlayCanvasRef.height;
    if (w === 0 || h === 0) return;

    if (!overlayCtx) {
      overlayCtx = overlayCanvasRef.getContext("2d");
    }
    if (!overlayCtx) return;

    const layer = engine.getLayer(layerId);
    if (!layer) return;

    // ── Resolve eraser behavior for background layers ──
    // Must match the same resolveEraserFill call in onPaintStroke so the
    // commit path matches the composite path (destination-out vs source-over).
    // NOTE: resolveEraserFill is ONLY called for isEraser=true (see onPaintStroke).
    const eraserFill = isEraser ? resolveEraserFill(layer, true, bgColor()) : null;
    const effectiveIsEraser = eraserFill ? eraserFill.isEraser : false;

    // Reuse cached commit buffer to avoid 107MB allocation per commit.
    if (!paintSession) return;
    // Only reallocate when layer dimensions change.
    if (!cachedCommitCanvas || cachedCommitCanvas.width !== w || cachedCommitCanvas.height !== h) {
      cachedCommitCanvas = new OffscreenCanvas(w, h);
      cachedCommitCtx = cachedCommitCanvas.getContext("2d")!;
    }
    const sCtx = cachedCommitCtx!;
    sCtx.clearRect(0, 0, w, h);
    const dirty = clampDirtyRect(paintSession.dirtyRect, w, h);
    const hasDirt = dirty.x1 > dirty.x0 && dirty.y1 > dirty.y0 && paintSession.dabPositions.length > 0;

    if (effectiveIsEraser) {
      // Eraser: overlay already holds the erased result (seeded with layer
      // content, cut via destination-out during the stroke).
      sCtx.drawImage(overlayCanvasRef, 0, 0);
    } else {
      // Brush: commit RAW dabs (session.color, un-adjusted). The shader applies
      // basicAdjustment uniformly, so the stored raw dab displays identically
      // to the preview (which draws the adjustment-applied dab color). This
      // keeps the layer non-destructive — basicAdjustment stays a live param.
      if (layer.imageBitmap) sCtx.drawImage(layer.imageBitmap, 0, 0);
        const tip = getBrushTip({ size: paintSession.tipSize, hardness: paintSession.tipHardness, curve: "soft" });
        if (tip && hasDirt) {
          // Store the inverse-adjusted color so the shader re-applies the
          // layer's basicAdjustment and the stroke displays as the picked color
          // (WYSIWYG). With no adjustment this returns the color unchanged.
          const dabColor = inverseBasicAdjustmentToColor(
            paintSession.color,
            layer.basicAdjustment ?? { brightness: 0, contrast: 0, saturation: 0 },
          );
          const rawTip = getTipCanvas(tip, dabColor);
        const r = tip.diameter / 2;
        for (let i = 0; i < paintSession.dabPositions.length; i++) {
          const d = paintSession.dabPositions[i];
          sCtx.globalAlpha = d.alpha;
          sCtx.drawImage(rawTip, 0, 0, rawTip.width, rawTip.height,
            Math.round(d.x - r), Math.round(d.y - r), tip.diameter, tip.diameter);
        }
        sCtx.globalAlpha = 1;
        if (layer.lockTransparency && layer.imageBitmap) {
          sCtx.globalCompositeOperation = "destination-in";
          sCtx.drawImage(layer.imageBitmap, 0, 0);
          sCtx.globalCompositeOperation = "source-over";
        }
      }
    }

    try {
      const gen = ++strokeGen;
      const newBitmap = await createImageBitmap(cachedCommitCanvas);
      if (gen !== strokeGen) {
        newBitmap.close();
        return;
      }
      const currentEngine = workspace.getActiveEngine();
      if (currentEngine !== engine || !currentEngine.getLayer(layerId)) {
        newBitmap.close();
        overlayCtx.clearRect(0, 0, w, h);
        prevStrokePointCount = 0;
        paintSession = null;
        return;
      }
      // Restore the pre-stroke anchor so the history snapshot (taken inside
      // commitPaintBitmap, below) captures it deterministically — independent
      // of when this async commit actually fires vs. any live `lastPaintCoords`
      // mutation. Fallback keeps cancel-path behavior unchanged when no anchor
      // is supplied.
      history.setLastPaintCoords(anchor ?? history.getLastPaintCoords());
      commitPaintBitmap(
        { engine, history, uploader: renderer, requestRender: () => scheduler.requestRender() },
        {
          layerId,
          bitmap: newBitmap,
          label: effectiveIsEraser ? "Eraser" : "Brush Stroke",
          dirtyRect: hasDirt
            ? { x: dirty.x0, y: dirty.y0, width: dirty.x1 - dirty.x0, height: dirty.y1 - dirty.y0 }
            : undefined,
          // Attach the pre-bake snapshot (if this stroke followed a confirmed
          // bake) so a single undo restores the live adjustment.
          snapshot: preBake && preBake.layerId === layerId ? preBake.snapshot : undefined,
        },
      );
      // Advance live `lastPaintCoords` to the stroke end so the next Shift
      // stroke connects from here.
      if (paintSession && paintSession.dabPositions.length > 0) {
        const lastDab = paintSession.dabPositions[paintSession.dabPositions.length - 1];
        history.setLastPaintCoords({ x: lastDab.x, y: lastDab.y });
      }
      if (preBake && preBake.layerId === layerId) preBake = null;
      // Eraser: defer overlay clear to after the next render so the user
      // never sees a flash where the overlay clears before WebGL re-renders
      // with the committed texture (from uploadImage above).
      // Brush: clear immediately (existing behavior — overlay shows strokes on
      // transparent bg, so the flash of the original layer is barely visible).
      if (effectiveIsEraser) {
        requestAnimationFrame(() => overlayCtx?.clearRect(0, 0, w, h));
      } else {
        overlayCtx.clearRect(0, 0, w, h);
      }
      prevStrokePointCount = 0;
      paintSession = null;
    } catch (err) {
      showToast(`Brush stroke failed: ${err instanceof Error ? err.message : "unknown error"}`, "error");
      paintSession = null;
    }
    const _dt = performance.now() - _t0;
    if (_dt > 5) console.warn(`[perf] commitBrushStroke: ${_dt.toFixed(1)}ms (w=${w}, h=${h})`);
  }

  return {
    onPaintStroke,
    commitBrushStroke,
    setOverlayCanvasRef: (el: HTMLCanvasElement | null) => {
      overlayCanvasRef = el;
      overlayCtx = el ? el.getContext("2d") : null;
      if (el) {
        el.width = docWidth();
        el.height = docHeight();
        // Eagerly allocate the commit scratch canvas at layer size so the first
        // brush stroke doesn't pay the 107MB OffscreenCanvas allocation + first
        // GPU readback on the paint-commit path (was a ~46ms spike).
        const cw = el.width, ch = el.height;
        if (!cachedCommitCanvas || cachedCommitCanvas.width !== cw || cachedCommitCanvas.height !== ch) {
          cachedCommitCanvas = new OffscreenCanvas(cw, ch);
          cachedCommitCtx = cachedCommitCanvas.getContext("2d");
          // Warm the commit-context GPU path off the paint path. The first
          // brush stroke's cold cost is (a) uploading the 27MP base bitmap into
          // this 2D context's texture cache (drawImage) and (b) the first
          // full-canvas readback (createImageBitmap). Replaying both here, at
          // layer activation when the user isn't painting, makes the first real
          // commit a cache HIT (~7ms instead of ~30ms).
          const cctx = cachedCommitCtx;
          const warmCanvas = cachedCommitCanvas;
          const ric: (cb: () => void) => void =
            typeof requestIdleCallback === "function"
              ? (cb) => requestIdleCallback(() => cb())
              : (cb) => setTimeout(cb, 1);
          ric(() => {
            const eng = workspace.getActiveEngine();
            const id = eng?.getActiveLayerId() ?? null;
            const bmp = id ? eng!.getLayer(id)?.imageBitmap : undefined;
            if (!cctx || !bmp) return;
            try {
              cctx.drawImage(bmp, 0, 0);
              if (typeof createImageBitmap === "function") {
                createImageBitmap(warmCanvas).then((b) => b.close()).catch(() => {});
              }
              cctx.clearRect(0, 0, cw, ch);
            } catch {
              // Layer/bitmap not ready — first commit simply pays the cold cost.
            }
          });
        }
      } else {
        paintSession = null;
        cachedCommitCanvas = null;
        cachedCommitCtx = null;
      }
    },
    getOverlayCanvasRef: () => overlayCanvasRef,
    clearPrevStrokePointCount: () => {
      stopHoldTimer();
      prevStrokePointCount = 0;
      paintSession = null;
    },
  };
}