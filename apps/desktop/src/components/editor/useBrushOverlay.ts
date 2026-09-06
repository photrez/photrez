import { createEffect, onCleanup, createSignal } from "solid-js";
import { useEditor } from "./shell/EditorContext";
import { useDialog } from "./dialogs/DialogProvider";
import type { DocumentEngine } from "@/engine/document";
import type { DocumentModel } from "@/engine/types";
import type { CommandHistory } from "@/engine/history";
import {
  tilesInRect,
  mergeTilesToRects,
  PAINT_TILE_SIZE,
  type TileKeyed,
} from "@/lib/paint/paintTileSurface";
import { syncFacadeVersionFromPixel } from "@/lib/protocol/facadeRegistry";
import { getPaintToolBlockReason, resolveEraserFill, type PaintToolSettings } from "./brushToolState";
import { commitPaintBitmap } from "./paintCommitCommand";
import { mapPaintPointToLayerLocal } from "./paintStrokeCoordinates";
import { showToast } from "./Toast";
import { isFacadeOwnedLayer } from "@/engine/document";
import { applyBasicAdjustmentToColor, inverseBasicAdjustmentToColor } from "@/engine/layerAdjustments";
import { isRustShadowEnabled, runShadowForCommit, applyRustTilesToSurface, isPristineOpaqueWhite, rehydratePaintSurfaceFromRust } from "@/lib/rustShadow";
import {
  getBrushDabSpacing,
  getBrushTip,
  getEffectiveFlowMultiplier,
  type DirtyRect,
  emptyDirtyRect,
  expandDirtyRect,
  clampDirtyRect,
  parsePaintColor,
} from "./brushTipMask";
import { createDabProducer, type DabProducer } from "./brushDabProducer";

// ── C4 pilot (R2 flagged active-layer): Rust owns the canonical pixel buffer ──
// Bug 2 fix: there is no TS-side "seeded" flag. Each layer is seeded into Rust
// ONCE via `rust_pixels_init` (idempotent, namespaced by docId+layerId), then
// every committed stroke sends ONLY its dirty region via `rust_pixels_write_region`
// (which replaces those canonical pixels and returns before/after tiles). Seeding
// state lives entirely in Rust, so reopening or switching documents can never
// inherit stale seeded state.

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
  /** Dab producer (TS reference impl or Rust BrushStrokeEngine behind
   *  photrez.rustDabs). Owns the spacing/carry state machine. */
  producer: DabProducer;
  /** Accumulated dirty region for this stroke. */
  dirtyRect: DirtyRect;
}

// ── Strategy D: async-deferred C4 canonical commit (production analog of the
// WebGL2 PBO readback validated in RESPONSE.md). Brush surface is Canvas2D, so
// the literal PBO readback is replaced by an async `surface.readRect` + Tauri IPC
// + `history.commit`. A per-document queue serializes commits so stroke N+1 never
// overtakes stroke N (canonical state + history ordering preserved). The pointerup
// handler returns immediately after enqueue — block = bbox bookkeeping only.
const c4CommitQueues = new Map<string, Promise<void>>();

interface C4SurfaceLike {
  context: OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D;
  readRect: (x: number, y: number, w: number, h: number) => ImageData;
  pixelEpoch: number;
  pixelVersion: number;
}
interface C4CommitJob {
  docId: string;
  layerId: string;
  dx0: number; dy0: number; dw: number; dh: number;
  w: number; h: number;
  surface: C4SurfaceLike;
  engine: DocumentEngine;
  history: CommandHistory;
  requestRender: () => void;
  beforePatches: TileKeyed<ImageData>[];
  effectiveIsEraser: boolean;
  seq: number;
}

/** Test-only: await all in-flight C4 deferred commits (validation flush). */
export async function flushC4Commits(): Promise<void> {
  await Promise.allSettled([...c4CommitQueues.values()]);
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

  async function c4CoreCommit(job: C4CommitJob): Promise<void> {
    const { docId, layerId, dx0, dy0, dw, dh, w, h, surface, engine, history, requestRender, beforePatches, effectiveIsEraser } = job;
    const sctx = surface.context;
    const runCore = async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      await rehydratePaintSurfaceFromRust(docId, layerId, surface as never);
      let layerReady = true;
      try { await invoke("rust_pixels_get_epoch", { docId, layerId }); } catch { layerReady = false; }
      if (!layerReady) {
        const seed = surface.readRect(0, 0, w, h);
        await invoke("rust_pixels_init", { docId, layerId, width: w, height: h, bytes: new Uint8Array(seed.data.buffer, seed.data.byteOffset, seed.data.byteLength) });
      }
      const region = surface.readRect(dx0, dy0, dw, dh);
      const res = (await invoke("rust_pixels_write_region", {
        docId, layerId, x: dx0, y: dy0, w: dw, h: dh,
        rgba: new Uint8Array(region.data.buffer, region.data.byteOffset, region.data.byteLength),
      })) as { before: { x: number; y: number; w: number; h: number; data: number[] }[]; after: { x: number; y: number; w: number; h: number; data: number[] }[]; epoch: number; version: number };
      applyRustTilesToSurface(sctx, res.after);
      surface.pixelEpoch = res.epoch;
      surface.pixelVersion = res.version;
      syncFacadeVersionFromPixel(docId, res.version);
      const afterPatches: { x: number; y: number; width: number; height: number; data: Uint8ClampedArray }[] = [];
      const rectUploads: { x: number; y: number; width: number; height: number; data: Uint8ClampedArray }[] = [];
      for (const t of res.after) {
        const data = new Uint8ClampedArray(t.w * t.h * 4);
        data.set(t.data);
        afterPatches.push({ x: t.x, y: t.y, width: t.w, height: t.h, data });
        rectUploads.push({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) });
      }
      history.commit(engine.snapshot(), effectiveIsEraser ? "Eraser" : "Brush Stroke", {
        layerId, surfaceWidth: w, surfaceHeight: h,
        before: beforePatches.map((p) => ({ x: p.tx * PAINT_TILE_SIZE, y: p.ty * PAINT_TILE_SIZE, width: p.value.width, height: p.value.height, data: p.value.data })),
        after: afterPatches,
      });
      queueOrUploadTiles(layerId, w, h, rectUploads);
      requestRender();
      try {
        if (localStorage.getItem("photrez.c4Audit") === "1") {
          const dirtyRectBytes = dw * dh * 4;
          const responseBytes = res.after.reduce((s, t) => s + t.w * t.h * 4, 0);
          console.info(`[c4-audit] dirtyRectBytes=${dirtyRectBytes} tileCount=${res.after.length} responseBytes=${responseBytes}`);
        }
      } catch { /* audit never breaks commit */ }
    };
    try {
      await runCore();
    } catch (err) {
      // surface already holds the composited after-pixels; mirror them to TS via
      // history.commit + tile upload (no cachedTileScratch dependency).
      console.warn("[c4] async deferred commit failed — synchronous fallback:", err);
      try {
        const region = surface.readRect(dx0, dy0, dw, dh);
        const afterData = new Uint8ClampedArray(region.data);
        const afterPatches = [{ x: dx0, y: dy0, width: dw, height: dh, data: afterData }];
        const rectUploads = [{ x: dx0, y: dy0, width: dw, height: dh, data: afterData }];
        applyRustTilesToSurface(sctx, [{ x: dx0, y: dy0, w: dw, h: dh, data: Array.from(afterData) }]);
        history.commit(engine.snapshot(), effectiveIsEraser ? "Eraser" : "Brush Stroke", {
          layerId, surfaceWidth: w, surfaceHeight: h,
          before: beforePatches.map((p) => ({ x: p.tx * PAINT_TILE_SIZE, y: p.ty * PAINT_TILE_SIZE, width: p.value.width, height: p.value.height, data: p.value.data })),
          after: afterPatches,
        });
        queueOrUploadTiles(layerId, w, h, rectUploads);
        requestRender();
      } catch (err2) {
        console.error("[c4] fallback also failed — pixels may be dropped for", docId, layerId, err2);
      }
    } finally {
      // DEV-only: async-queue ordering probe (gate verification — tree-shaken in prod).
      if ((import.meta as any).env?.DEV) {
        const w = window as unknown as Record<string, any>;
        (w.__c4DeferredDone = (w.__c4DeferredDone ?? 0) + 1);
        (w.__c4DoneSeq = w.__c4DoneSeq ?? []).push(job.seq);
      }
    }
  }

  function enqueueC4Commit(job: C4CommitJob): void {
    const key = `${job.docId}:${job.layerId}`;
    const prev = c4CommitQueues.get(key) ?? Promise.resolve();
    const next = prev.then(() => c4CoreCommit(job)).catch(() => {});
    c4CommitQueues.set(key, next);
    next.finally(() => { if (c4CommitQueues.get(key) === next) c4CommitQueues.delete(key); }).catch(() => {});
    // DEV-only: async-queue ordering probe (gate verification — tree-shaken in prod).
    if ((import.meta as any).env?.DEV) {
      const w = window as unknown as Record<string, any>;
      w.__c4Seq = (w.__c4Seq ?? 0) + 1;
      job.seq = w.__c4Seq;
      (w.__c4DeferredEnq = (w.__c4DeferredEnq ?? 0) + 1);
      (w.__c4EnqSeq = w.__c4EnqSeq ?? []).push(job.seq);
    }
  }

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

  // ── Context-loss upload handling (design §cancellation/context-loss) ──
  // Scratch/overlay are 2D canvases — GL loss never destroys stroke data. When the
  // WebGL context is down, uploads are HELD (not dropped) and flushed on restore.
  let glUploadsPending: { layerId: string; w: number; h: number; tiles: unknown[] }[] = [];
  let glLost = false;
  const onWebGLContextLost = () => { glLost = true; };
  const onWebGLContextRestored = () => {
    glLost = false;
    const r = renderer as { uploadSurfaceTiles?: (...a: unknown[]) => void } | undefined;
    if (!r?.uploadSurfaceTiles) { glUploadsPending = []; return; }
    const pending = glUploadsPending;
    glUploadsPending = [];
    for (const u of pending) {
      try { r.uploadSurfaceTiles(u.layerId, u.w, u.h, u.tiles); } catch { /* drop on repeated failure; next full resync recovers */ }
    }
    scheduler.requestRender();
  };
  if (typeof window !== "undefined") {
    window.addEventListener("webglcontextlost", onWebGLContextLost, true);
    window.addEventListener("webglcontextrestored", onWebGLContextRestored, true);
  }
  onCleanup(() => {
    if (typeof window !== "undefined") {
      window.removeEventListener("webglcontextlost", onWebGLContextLost, true);
      window.removeEventListener("webglcontextrestored", onWebGLContextRestored, true);
    }
  });

  function queueOrUploadTiles(layerId: string, w: number, h: number, tiles: unknown[]): boolean {
    // Returns true when the upload was handed to the renderer now; false when
    // it was queued because the GL context is currently lost.
    if (glLost) {
      glUploadsPending.push({ layerId, w, h, tiles });
      return false;
    }
    try {
      (renderer as { uploadSurfaceTiles?: (...a: unknown[]) => void }).uploadSurfaceTiles?.(layerId, w, h, tiles);
      return true;
    } catch {
      glUploadsPending.push({ layerId, w, h, tiles });
      return false;
    }
  }

  // ── Stroke cancellation (pointercancel / Escape) ──
  // Discards the active session: surface was NEVER mutated pre-commit, so discard
  // is a pure preview teardown. Eraser strokes must restore layer visibility (the
  // stroke start uploaded a 1×1 transparent texture to hide the WebGL layer).
  function cancelActiveStroke(): boolean {
    const hadStroke = paintSession !== null || prevStrokePointCount > 0;
    stopHoldTimer();
    strokeGen++; // invalidate pending RAF composites and any in-flight async commit
    if (compositeRaf !== null) {
      cancelAnimationFrame(compositeRaf);
      compositeRaf = null;
    }
    compositePending = false;
    const engine = workspace.getActiveEngine();
    const layerId = engine?.getActiveLayerId();
    const layer = engine && layerId ? engine.getLayer(layerId) : null;
    if (paintSession?.isEraser && layerId && layer?.imageBitmap) {
      try {
        renderer.uploadImage(layerId, layer.imageBitmap);
        scheduler.requestRender();
      } catch { /* context lost — restored resync re-uploads */ }
    }
    if (overlayCtx && overlayCanvasRef) {
      overlayCtx.clearRect(0, 0, overlayCanvasRef.width, overlayCanvasRef.height);
    }
    prevStrokePointCount = 0;
    paintSession = null;
    return hadStroke;
  }

  function isStrokeActive(): boolean {
    return paintSession !== null;
  }

  // ── Tile-commit scratch buffer ──
  // Dirty-rect-sized GPU-backed canvas for dab rasterization. The paint surface
  // is software-backed (willReadFrequently), where per-dab drawImage costs
  // ~0.5-1ms CPU each (measured 2026-08-22: 300+ slow dabs = 230-410ms);
  // drawing on a GPU canvas then copying the scratch into touched tiles keeps
  // raw-dab semantics at GPU speed.
  let cachedTileScratch: OffscreenCanvas | null = null;
  let cachedTileScratchCtx: OffscreenCanvasRenderingContext2D | null = null;

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

    // Gate A: brush isolation — facade-owned layer cannot be painted via legacy path
    if (isFacadeOwnedLayer(activeId)) {
      showToast("This layer is owned by Rust facade — legacy brush blocked", "warn");
      return;
    }

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
        producer: createDabProducer(settings.size),
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
        // Anchor the producer (emits nothing); initial stamp as before.
        paintSession.producer.update(localPt.x, localPt.y);
        paintSession.dabPositions.push({ x: localPt.x, y: localPt.y, alpha: alphaScale });
        paintSession.dirtyRect = expandDirtyRect(paintSession.dirtyRect, localPt.x, localPt.y, tipExtent);
      } else {
        const dabCount = paintSession.producer.update(localPt.x, localPt.y);
        paintSession.spacingCarry = paintSession.producer.carry();
        const view = dabCount > 0 ? paintSession.producer.view() : null;
        if (view && dabCount > 0) {
          for (let i = 0; i < dabCount; i++) {
            const dabX = view[i * 2];
            const dabY = view[i * 2 + 1];
            paintSession.dabPositions.push({ x: dabX, y: dabY, alpha: alphaScale });
            paintSession.dirtyRect = expandDirtyRect(paintSession.dirtyRect, dabX, dabY, tipExtent);
          }
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
      // NOTE: no drag-feedback outline here. The old rgba(0,0,0,0.25)
      // circle stroked on the overlay was visible as a black halo around
      // the eraser DURING the stroke (baked into the live view; the final
      // composite rebuild only cleaned it up at release). Position feedback
      // is provided by the BrushCursorOverlay SVG ring instead — drawn in
      // screen space, never committed to the layer.
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
    // DEV-only: pointerup synchronous-blocking probe (gate verification — tree-shaken in prod).
    if ((import.meta as any).env?.DEV) {
      (window as unknown as Record<string, any>).__cpT0 = _t0;
      (window as unknown as Record<string, any>).__cpSize = (window as unknown as Record<string, any>).__probeSize ?? 0;
    }
    if (isFacadeOwnedLayer(layerId)) {
      showToast("This layer is owned by Rust facade — legacy brush commit blocked", "warn");
      prevStrokePointCount = 0;
      paintSession = null;
      return;
    }
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

    // ── Fase 1 tile-commit path (flag photrez.tileCommit=1) ───────────────
    // Dabs go straight onto the engine's persistent paint surface; only
    // touched tiles are uploaded. Skips the O(canvas) full-size
    // createImageBitmap entirely. Guards: lockTransparency needs a full-layer
    // destination-in (legacy path); a confirmed bake preBake needs the
    // snapshot-restore semantics of the legacy path.
    let useTileCommit = true;
    // Graduated default-ON (T-BRUSH-TILECOMMIT-GRAD): explicit opt-out via "0".
    // Legacy single-PATCH path retained for lockTransparency/preBake guards.
    // Rollback: localStorage.setItem("photrez.tileCommit", "0").
    try { useTileCommit = localStorage.getItem("photrez.tileCommit") !== "0"; } catch { useTileCommit = true; }
    if (useTileCommit && hasDirt && !layer.lockTransparency && !(preBake && preBake.layerId === layerId)) {
      const surface = (engine as {
        getPaintSurface?: (id: string) => {
          context: OffscreenCanvasRenderingContext2D;
          snapshotTile: (tile: { x: number; y: number; w: number; h: number }) => TileKeyed<ImageData>;
          restoreTile: (patch: TileKeyed<ImageData>) => void;
          readRect: (x: number, y: number, w: number, h: number) => ImageData;
          pixelEpoch: number;
          pixelVersion: number;
        } | null;
      }).getPaintSurface?.(layerId) ?? null;
      // Hybrid threshold: per-tile API calls scale with touched area, so past
      // this many tiles the legacy single-PATCH path is strictly faster
      // (measured 2026-08-22 @6.9K: 300-tile stroke = 700ms vs legacy ~8ms).
      // Small/medium strokes — the common case — keep the tile path.
      if (surface) {
        const beforePatches: TileKeyed<ImageData>[] = [];
        const afterPatches: { x: number; y: number; width: number; height: number; data: Uint8ClampedArray }[] = [];
        const rectUploads: { x: number; y: number; width: number; height: number; data: Uint8ClampedArray }[] = [];
        let histCommitted = false;
        let c4Deferred = false;
        let perfTiles = 0, perfRects = 0, perfYields = 0, perfDabs = 0;
        let tP0 = 0, tP2 = 0, tP3 = 0, tP4 = 0;
        try {
        const tiles = tilesInRect(dirty.x0, dirty.y0, dirty.x1, dirty.y1, w, h);
        perfTiles = tiles.length;
        const rects = mergeTilesToRects(tiles);
        perfRects = rects.length;
        // Fase 1.5 (2026-08-23): no tile-count fallback. Merged
        // rects (mergeSparseRects pattern) keep GPU calls O(rects) instead of
        // O(tiles); budgeted yielding caps any single frame at
        // ~7ms of commit work instead of one blocking mega-upload. History
        // patches stay per-tile so undo/redo remain sub-3ms at every brush
        // size (display granularity != history granularity).
        //   docs/plans/2026-08-23-dirty-region-research.md
        tP0 = performance.now();
        const sctx = surface.context;
        const gen = strokeGen;
        const BUDGET_MS = 7;

        // Dab rasterization happens ONCE on the GPU-backed scratch before
        // the rect loop (the software surface costs ~0.5-1ms CPU PER dab,
        // measured 2026-08-22 — same reason as the original tile path).
        let scratchReady = false;
        let dx0 = 0, dy0 = 0, dw = 0, dh = 0;
        // R2 Step2 DEV-only forensics carriers (no production effect; populated only when shadow flag on)
        let scratchSnap: ImageData | null = null;
        let emulFull: Uint8ClampedArray | null = null;
        let foreGeo: { dx0: number; dy0: number; dw: number; dh: number } | null = null;
        let foreA: Uint8ClampedArray | Uint8Array | null = null;
        let foreTipW = 0, foreTipH = 0, foreTipDiameter = 0;
        let c3Ready = false;
        let c3Brush = 0;
        let c3Hardness = 0;
        let c3Color: [number, number, number] = [225, 90, 23];
        const foreRects: { x: number; y: number; w: number; h: number; rx: number; ry: number }[] = [];
        const foreRectImgs: ImageData[] = [];
        const forePerDabSnaps: Uint8ClampedArray[] = [];
        if (!effectiveIsEraser) {
          const tip = getBrushTip({ size: paintSession.tipSize, hardness: paintSession.tipHardness, curve: "soft" });
          if (tip) {
            dx0 = dirty.x0;
            dy0 = dirty.y0;
            dw = Math.max(1, dirty.x1 - dx0);
            dh = Math.max(1, dirty.y1 - dy0);
            if (!cachedTileScratch || cachedTileScratch.width !== dw || cachedTileScratch.height !== dh) {
              cachedTileScratch = new OffscreenCanvas(dw, dh);
              cachedTileScratchCtx = cachedTileScratch.getContext("2d");
            }
            const sc = cachedTileScratchCtx!;
            sc.clearRect(0, 0, dw, dh);
            const dabColor = inverseBasicAdjustmentToColor(
              paintSession.color,
              layer.basicAdjustment ?? { brightness: 0, contrast: 0, saturation: 0 },
            );
            const rawTip = getTipCanvas(tip, dabColor);
            const r = tip.diameter / 2;
            for (let i = 0; i < paintSession.dabPositions.length; i++) {
              const d = paintSession.dabPositions[i];
              sc.globalAlpha = d.alpha;
              sc.drawImage(rawTip, 0, 0, rawTip.width, rawTip.height,
                Math.round(d.x - r) - dx0, Math.round(d.y - r) - dy0, tip.diameter, tip.diameter);
              // R2 Step2 final forensic: per-dab scratch state (ladder config only)
              if (isRustShadowEnabled() && paintSession.tipSize === 256 && Math.abs(paintSession.tipHardness - 0.8) < 1e-9 && paintSession.dabPositions.length <= 16) {
                try { forePerDabSnaps.push(Uint8ClampedArray.from(sc.getImageData(0, 0, dw, dh).data)); } catch {}
              }
            }
            sc.globalAlpha = 1;
            scratchReady = true;
            // ── R2 Step2 DEV forensics: capture production scratch + build Rust-formula emulation over SAME dirty rect ──
            if (isRustShadowEnabled()) {
              try {
                scratchSnap = sc.getImageData(0, 0, dw, dh);
                foreA = rawTip.getContext("2d")!.getImageData(0, 0, rawTip.width, rawTip.height).data;
                foreTipW = rawTip.width; foreTipH = rawTip.height; foreTipDiameter = tip.diameter;
                c3Ready = true; c3Brush = tip.diameter; c3Hardness = paintSession.tipHardness;
                { const pcv = parsePaintColor(dabColor); c3Color = [pcv.r, pcv.g, pcv.b]; }
                const A2 = foreA;
                const md2 = (c: number, a: number) => ((c * a + 127) / 255) | 0;
                emulFull = new Uint8ClampedArray(dw * dh * 4);
                for (let i = 0; i < emulFull.length; i += 4) { emulFull[i] = 255; emulFull[i + 1] = 255; emulFull[i + 2] = 255; emulFull[i + 3] = 255; }
                for (const d of paintSession.dabPositions) {
                  const ox = Math.round(d.x - r) - dx0, oy = Math.round(d.y - r) - dy0;
                  const x0m = Math.max(0, ox), y0m = Math.max(0, oy);
                  const x1m = Math.min(dw, ox + tip.diameter), y1m = Math.min(dh, oy + tip.diameter);
                  for (let py = y0m; py < y1m; py++) {
                    for (let px = x0m; px < x1m; px++) {
                      const ti = ((py - oy) * rawTip.width + (px - ox)) * 4;
                      const sa = A2[ti + 3], inv = 255 - sa;
                      const di = (py * dw + px) * 4;
                      for (let ch = 0; ch < 3; ch++) emulFull[di + ch] = Math.min(255, md2(A2[ti + ch], sa) + md2(255, inv));
                      emulFull[di + 3] = 255;
                    }
                  }
                }
                foreGeo = { dx0, dy0, dw, dh };
              } catch { /* forensics must never break commit */ }
            }
          }
        }

        let yields = 0;
        let frameStart = performance.now();

        // Phase A: capture ALL before-patches FIRST - the single-crossing
        // stamp below paints the ENTIRE dirty rect at once, so any snapshot
        // taken after it would capture painted pixels (2026-08-23 undo
        // corruption bug: rect 2's "before" contained rect 1's paint).
        for (const rect of rects) {
          for (const t of rect.tiles) beforePatches.push(surface.snapshotTile(t));
        }

        // ── R2 Canonical C3 (flag-gated, fresh-white-docs only): Rust patches replace TS raster ──
  let c3Applied = false;
  let c3Flag = false;
        try { c3Flag = localStorage.getItem("photrez.canonicalCommit") === "1"; } catch {}
        if (c3Flag && !effectiveIsEraser && scratchReady && isRustShadowEnabled() && c3Ready) {
          try {
            // pristine guard: every before-patch must be opaque white (C3 scope = fresh blank docs)
            let pristine = true;
            for (const p of beforePatches) {
              if (!isPristineOpaqueWhite(p.value.data)) { pristine = false; break; }
            }
            if (pristine) {
              const { invoke } = await import("@tauri-apps/api/core");
              const res = await invoke("paint_parity_shadow", {
                req: {
                  w, h, prep_white: true, eraser: false,
                  brush: c3Brush, hardness: c3Hardness,
                  dabs: paintSession.dabPositions.map((d) => ({ x: d.x, y: d.y, alpha: d.alpha })),
                  tip_w: c3Brush, tip_h: c3Brush, tip_data: [],
                  canonical_tip: true, tip_color: c3Color,
                },
                opts: { include_tiles: true },
              }) as { tiles: { x: number; y: number; w: number; h: number; data: number[] }[] };
              // surface seam (existing putImageData path, via tested helper)
              applyRustTilesToSurface(sctx, res.tiles);
              for (const t of res.tiles) {
                const data = new Uint8ClampedArray(t.w * t.h * 4);
                data.set(t.data);
                afterPatches.push({ x: t.x, y: t.y, width: t.w, height: t.h, data });
                rectUploads.push({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) });
              }
              c3Applied = true;
              console.info(`[c3] canonical patches applied: ${res.tiles.length} tiles`);
            } else {
              console.info("[c3] skipped: base not pristine white (outside C3 scope)");
            }
          } catch (err) {
            console.warn("[c3] canonical path failed — falling back to legacy:", err);
          }
        }

        // ── C4 pilot (R2 flagged active-layer): Rust owns canonical pixels ──
        // Every eligible committed stroke sends ONLY its dirty region via
        // `rust_pixels_write_region`, which replaces those canonical pixels in Rust
        // (no dab re-composite: the surface already holds the composited after-pixels)
        // and returns the exact pre-stroke (`before`) and (`after`) tiles. The TS
        // cache is synced from Rust's returned `after` (single source of truth).
        // OFF unless photrez.rustPixels === "1". Mutually exclusive with
        // the C3 canonical path by *mode* — gated on c3Flag (photrez.canonicalCommit),
        // not on c3Applied — so the two modes never both apply to the same stroke.
        // c3Applied is set true by C4's own commit below; that must NOT disable
        // later C4 strokes, so the C4 entry guard uses c3Flag, not c3Applied.
        const rustPixelsFlag = (() => {
          try { return localStorage.getItem("photrez.rustPixels") === "1"; } catch { return false; }
        })();
        if (rustPixelsFlag && !c3Flag && !effectiveIsEraser && scratchReady) {
          // C4 (Strategy D): async-deferred dirty-region commit. The pointerup
          // handler returns immediately after enqueue; the canonical write, tile
          // rehydration, and history.commit run off-path via a per-document queue
          // (stroke N+1 never overtakes N). Brush surface is Canvas2D, so this is
          // the faithful analog of the WebGL2 PBO readback validated in RESPONSE.md
          // (pointerup block <1ms, no busy-wait, ordered).
          c3Applied = true;
          c4Deferred = true;
          histCommitted = true;
          const docId = workspace.getActiveDocumentId() ?? "";
          const _ce0 = performance.now();
          enqueueC4Commit({
            docId,
            layerId,
            dx0, dy0, dw, dh,
            w, h,
            surface: surface as unknown as C4SurfaceLike,
            engine,
            history,
            requestRender: () => scheduler.requestRender(),
            beforePatches,
            effectiveIsEraser,
            seq: 0,
          });
          if ((import.meta as any).env?.DEV) {
            (window as unknown as Record<string, any>).__c4EnqueueMs = performance.now() - _ce0;
          }
        }

        // Phase B: ONE GPU->CPU crossing for the whole dirty rect (benchmarked
        // 2026-08-23 via agent-browser Chrome: per-tile crossings cost
        // ~55ms EACH at 300 tiles = 16.9s total; one full-rect draw =
        // 97ms). Transparent scratch regions preserve existing surface
        // pixels under source-over, same as the old clipped stamps.
        if (!c3Applied && !effectiveIsEraser && scratchReady) {
          sctx.drawImage(cachedTileScratch!, 0, 0, dw, dh, dx0, dy0, dw, dh);
        }

        // Phase C: per-rect readback -> per-tile patches + upload entries.
        if (!c3Applied) {
        for (let ri = 0; ri < rects.length; ri++) {
          const rect = rects[ri];
          const rt = rect.tiles;
          if (effectiveIsEraser) {
            // Overlay holds the erased result — replace-copy the rect's tiles.
            // (Software->software draws, disjoint per rect - safe after all
            // snapshots were taken in Phase A.)
            sctx.save();
            sctx.globalCompositeOperation = "source-over";
            for (const t of rt) {
              sctx.clearRect(t.x, t.y, t.w, t.h);
              sctx.drawImage(overlayCanvasRef, t.x, t.y, t.w, t.h, t.x, t.y, t.w, t.h);
            }
            sctx.restore();
          }

          // ONE readback per rect feeds everything: per-tile upload entries
          // (texSubImage2D per-tile measured faster than one giant rect
          // upload: 41ms vs 73ms) + compact per-tile after-patches.
          const rectImg = surface.readRect(rect.x, rect.y, rect.w, rect.h);
          for (const t of rt) {
            const col = t.x - rect.x;
            const row0 = t.y - rect.y;
            const data = new Uint8ClampedArray(t.w * t.h * 4); // compact copy per patch entry
            for (let row = 0; row < t.h; row++) {
              data.set(
                rectImg.data.subarray((row0 + row) * rect.w * 4 + col * 4, (row0 + row) * rect.w * 4 + col * 4 + t.w * 4),
                row * t.w * 4,
              );
            }
            afterPatches.push({ x: t.x, y: t.y, width: t.w, height: t.h, data });
            rectUploads.push({ x: t.x, y: t.y, width: t.w, height: t.h, data });
            if (isRustShadowEnabled()) { foreRects.push({ x: t.x, y: t.y, w: t.w, h: t.h, rx: rect.x, ry: rect.y }); foreRectImgs.push(rectImg); }
          }

          if (ri < rects.length - 1 && performance.now() - frameStart > BUDGET_MS) {
            await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
            if (gen !== strokeGen) return; // superseded by a newer stroke
            yields++;
            frameStart = performance.now();
          }
        }
        } // end !c3Applied (Phase C skipped when canonical patches were applied)
        perfYields = yields;

        const _p2 = performance.now();
        tP2 = _p2;
        const _dabN = paintSession.dabPositions.length;
        perfDabs = _dabN;
        const imperative = {
          layerId,
          surfaceWidth: w,
          surfaceHeight: h,
          before: beforePatches.map((p) => ({ x: p.tx * PAINT_TILE_SIZE, y: p.ty * PAINT_TILE_SIZE, width: p.value.width, height: p.value.height, data: p.value.data })),
          after: afterPatches,
        };
        // Imperative is entry-owned (tile-memento model): history stores it
        // with this commit and replays its before/after tiles on undo/redo.
        if (!c4Deferred) {
          history.commit(engine.snapshot(), effectiveIsEraser ? "Eraser" : "Brush Stroke", imperative, true);
          histCommitted = true;
        }
        tP3 = performance.now();
        // ── R2 Step2 DEV forensics assembly (inside try scope): scratch / surface / tsAfter boundaries ──
        if (isRustShadowEnabled() && scratchSnap && emulFull && foreGeo && foreA && foreTipW) {
          try {
            const gx = foreGeo.dx0, gy = foreGeo.dy0, gw = foreGeo.dw, gh = foreGeo.dh;
            // (a) scratch readback vs straight-unpremul emulation of stacked stamps over TRANSPARENT
            const emulT = new Uint8ClampedArray(gw * gh * 4);
            {
              const half = foreTipDiameter / 2;
              for (const d of paintSession.dabPositions) {
                const ox = Math.round(d.x - half) - gx, oy = Math.round(d.y - half) - gy;
                const x0m = Math.max(0, ox), y0m = Math.max(0, oy);
                const x1m = Math.min(gw, ox + foreTipDiameter), y1m = Math.min(gh, oy + foreTipDiameter);
                for (let py = y0m; py < y1m; py++) {
                  for (let px = x0m; px < x1m; px++) {
                    const ti = ((py - oy) * foreTipW + (px - ox)) * 4;
                    const sa = foreA[ti + 3];
                    if (sa === 0) continue;
                    const di = (py * gw + px) * 4;
                    const pmR = ((foreA[ti] * sa + 127) / 255) | 0;
                    const pmG = ((foreA[ti + 1] * sa + 127) / 255) | 0;
                    const pmB = ((foreA[ti + 2] * sa + 127) / 255) | 0;
                    emulT[di]     = Math.round(pmR * 255 / sa);
                    emulT[di + 1] = Math.round(pmG * 255 / sa);
                    emulT[di + 2] = Math.round(pmB * 255 / sa);
                    emulT[di + 3] = sa;
                  }
                }
              }
            }
            let sMax = 0, sSum = 0, sN = 0; const sChMx = [0, 0, 0, 0];
            let sFirst: { x: number; y: number; channel: string } | null = null;
            let tMax = 0, tSum = 0, tN = 0;
            let tFirst: { x: number; y: number; channel: string } | null = null;
            for (let i = 0; i < foreRects.length; i++) {
              const rr = foreRects[i], img = foreRectImgs[i];
              // (b) surface output vs white-base Rust-formula emulation
              for (let py = 0; py < rr.h; py++) {
                for (let px = 0; px < rr.w; px++) {
                  const si = ((rr.y - gy + py) * gw + (rr.x - gx + px)) * 4;
                  const di = (py * rr.w + px) * 4;
                  for (let ch = 0; ch < 4; ch++) {
                    const d = Math.abs(emulFull[si + ch] - img.data[di + ch]);
                    if (d > sMax) sMax = d;
                    sSum += d; if (d > sChMx[ch]) sChMx[ch] = d;
                    if (d > 0 && !sFirst) sFirst = { x: rr.x + px, y: rr.y + py, channel: ["R", "G", "B", "A"][ch] };
                  }
                }
              }
              // (c) tsAfter slicing fidelity for tiles of this rect
              const colB = rr.x - rr.rx, rowB = rr.y - rr.ry;
              for (const p of afterPatches) {
                if (p.x !== rr.x || p.y !== rr.y || p.width !== rr.w || p.height !== rr.h) continue;
                for (let row = 0; row < p.height; row++) {
                  const srcOff = ((rowB + row) * img.width + colB) * 4;
                  const dstOff = row * p.width * 4;
                  for (let k = 0; k < p.width * 4; k++) {
                    const d = Math.abs(img.data[srcOff + k] - p.data[dstOff + k]);
                    if (d > tMax) tMax = d;
                    tSum += d;
                    if (d > 0 && !tFirst) tFirst = { x: p.x + (((dstOff + k) / 4) | 0) % p.width, y: p.y + row, channel: ["R", "G", "B", "A"][k & 3] };
                  }
                }
                tN += p.width * p.height * 4;
                break;
              }
            }
            let tEmMax = 0; { let s = 0; for (let i = 0; i < emulT.length; i++) { const d = Math.abs(emulT[i] - scratchSnap.data[i]); if (d > tEmMax) tEmMax = d; s += d; } var tEmMean = +(s / emulT.length).toFixed(3); }
            (window as unknown as Record<string, unknown>).__photrezScratchForensics = {
              geo: foreGeo,
              rects: foreRects.slice(),
              dabs: paintSession.dabPositions.map((d) => ({ x: d.x, y: d.y, alpha: d.alpha })),
              scratchPerDab: forePerDabSnaps,
              scratchBytes: Array.from(scratchSnap.data),
              scratchVsEmulatedTransparent: { max: tEmMax, mean: tEmMean },
              stampToSurface: { max: sMax, mean: +(sSum / Math.max(1, sN)).toFixed(3), chMax: sChMx, first: sFirst },
              readbackVsTsAfter: { max: tMax, mean: +(tSum / Math.max(1, tN)).toFixed(3), first: tFirst },
            };
          } catch { /* forensics never breaks commit */ }
        }
      } catch (err) {
          if (!histCommitted) {
            // Pre-H failure: surface must return to pristine — restore every
            // snapshotted tile, drop the session, no history entry.
            for (const p of beforePatches) {
              try { surface.restoreTile(p); } catch { /* best-effort; snapshot itself failed for this tile */ }
            }
            if (overlayCtx && overlayCanvasRef) overlayCtx.clearRect(0, 0, overlayCanvasRef.width, overlayCanvasRef.height);
            prevStrokePointCount = 0;
            paintSession = null;
            showToast(`Brush commit failed — stroke discarded (${err instanceof Error ? err.message : "unknown error"})`, "error");
            scheduler.requestRender();
            return;
          }
          throw err; // post-commit code is outside the try; safeguard only
        }
        let uploadedNow = false;
        if (!c4Deferred) {
          uploadedNow = queueOrUploadTiles(layerId, w, h, rectUploads);
          tP4 = performance.now();
          scheduler.requestRender();
        }
        // ── R2 Step1 Shadow (candidate-only, bounded, reversible) ──
        // TS remains canonical; Rust never mutates document/history/GPU.
        // Flag OFF (default) => no work; flag ON => fire-and-forget candidate, logs to window.__photrezShadowLog.
        const _shadowSession = paintSession;
        try {
          if (isRustShadowEnabled() && _shadowSession && afterPatches.length) {
            const tsAfter = new Map(afterPatches.map((p) => [`${p.x / 256},${p.y / 256}`, p.data]));
            const tipObj = getBrushTip({ size: _shadowSession.tipSize, hardness: _shadowSession.tipHardness, curve: "soft" });
            const tipCanvas = tipObj
              ? (getTipCanvas(tipObj, inverseBasicAdjustmentToColor(_shadowSession.color, layer.basicAdjustment ?? { brightness: 0, contrast: 0, saturation: 0 })) as unknown as OffscreenCanvas)
              : null;
            const dabs = _shadowSession.dabPositions.map((d) => ({ x: d.x, y: d.y, alpha: d.alpha }));
            void runShadowForCommit({ w, h, brush: _shadowSession.tipSize, hardness: _shadowSession.tipHardness, dabs, tip: tipCanvas, eraser: effectiveIsEraser, prepWhite: true, tsAfter, t0: _t0 }).catch(() => {});
          }
        } catch {}
        overlayCtx.clearRect(0, 0, w, h);
        prevStrokePointCount = 0;
        paintSession = null;
        // DEV-only: record pointerup synchronous-blocking sample (entry→return of commitBrushStroke).
        if ((import.meta as any).env?.DEV) {
          const wd = window as unknown as Record<string, any>;
          const t1 = performance.now();
          const pending = (wd.__c4DeferredEnq ?? 0) - (wd.__c4DeferredDone ?? 0);
          (wd.__photrezCommitProbe = wd.__photrezCommitProbe ?? { samples: [] }).samples.push({
            syncMs: t1 - (wd.__cpT0 ?? t1),
            c4EnqueueMs: wd.__c4EnqueueMs ?? 0,
            size: wd.__cpSize ?? 0,
            deferredPendingAtReturn: pending,
            t: t1,
          });
        }
        // Diagnosis gate: phases only when the whole commit is slow. Upload time
        // is only meaningful when it ran now (not queued behind a lost context).
        const _dtT = tP4 - _t0;
        if (_dtT > 16) {
          console.warn(
            `[perf] tile-commit phases: surface=${(tP0 - _t0).toFixed(1)} dabs=${(tP2 - tP0).toFixed(1)} hist=${(tP3 - tP2).toFixed(1)} upload=${uploadedNow ? (tP4 - tP3).toFixed(1) : "queued"}ms total=${_dtT.toFixed(1)} tiles=${perfTiles} rects=${perfRects} yields=${perfYields} dab#=${perfDabs}`,
          );
        }
        return;
      } else {
        console.info("[perf] commitBrushStroke(tile): no paint surface for layer, legacy fallback");
      }
    } else if (useTileCommit) {
      console.info(
        "[perf] commitBrushStroke(tile): guard fallback",
        JSON.stringify({ hasDirt, lockTransparency: layer.lockTransparency, preBakeHit: preBake?.layerId === layerId }),
      );
    }

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
    /** Discard the active stroke (pointercancel / Escape): no surface mutation, no history entry. Returns true when a stroke was actually discarded. */
    cancelActiveStroke,
    /** True while a stroke gesture is live (between pointerdown and commit/cancel). */
    isStrokeActive,
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
            // Pre-touch the overlay canvas at the ACTIVE LAYER's size and force
            // its GPU backing now, so the FIRST brush stroke doesn't pay
            // resize + allocation + cold-raster (fraction-second hitch on large
            // canvases). Mirrors the commit-canvas warm below.
            try {
              const lyr = id ? eng!.getLayer(id) : undefined;
              if (lyr && lyr.width && lyr.height && (el.width !== lyr.width || el.height !== lyr.height)) {
                el.width = lyr.width;
                el.height = lyr.height;
              }
              if (bmp) {
                el.getContext("2d")?.drawImage(bmp, 0, 0);
                el.getContext("2d")?.clearRect(0, 0, el.width, el.height);
              }
            } catch {
              // Layer/bitmap not ready — first stroke simply pays the cold cost.
            }
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