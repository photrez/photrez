// SPDX-License-Identifier: AGPL-3.0-or-later
// Text edit overlay — the live typing surface for a text edit session.
//
// A <textarea> positioned over the canvas at doc→screen coords, synced with
// zoom/pan; transparent background, accent border. While editing the canvas keeps
// rendering the live raster (debounced re-raster on every input), so the result is
// WYSIWYG without a placeholder.
//
// Session contract:
//   - Mounts while textEditSession() is non-null (new temp layer or re-edit).
//   - Every input re-rasterizes the layer live (debounced 50ms) via
//     engine.updateTextData → renderer.uploadImage.
//   - Ctrl/Cmd+Enter commits the session (one undo step); Escape cancels.
//   - IME-safe: composition events do not push/commit until compositionend,
//     and Enter during composition inserts the IME candidate, not a commit.
import { Show, createEffect, createMemo, createSignal, onCleanup, untrack, type JSX } from "solid-js";
import { useEditor } from "./shell/EditorContext";
import { commitTextSession, cancelTextSession, setPendingTextFlush } from "./canvas/pointerTools/textTool";
import { useI18n } from "@/i18n/I18nProvider";
import { isFacadeOwnedLayer, type DocumentEngine } from "@/engine/document";
import { rasterizeText } from "@/engine/textRasterizer";
import {
  clearTransformPreview,
  commitFacadeParams,
  isFacadeEnabled,
  peekFacade,
  setTransformPreview,
} from "@/lib/protocol/facadeRegistry";
import { routeNumericTransform } from "./layers/transformRouting";
import { commitRoutedTextParams, sameParams } from "./layers/paramsRouting";
import { showToast } from "./Toast";

/**
 * Small minimum width so a one-character or empty text box still offers a
 * usable click/caret target (the old 160px stretched short labels to 2x
 * their width). jsdom's cssstyle can't round-trip `ch`, so the constant is
 * asserted in tests instead of the computed style.
 */
export const OVERLAY_MIN_WIDTH = "2ch";
import type { TextData } from "@/engine/textTypes";

/** Debounce window for the live re-raster; each push rasterizes a fresh bitmap. */
const RERASTER_DEBOUNCE_MS = 50;

export function TextEditOverlay() {
  const { t } = useI18n();
  const {
    workspace,
    renderer,
    scheduler,
    zoom,
    pan,
    layers,
    textEditSession,
    setTextEditSession,
  } = useEditor();

  let textareaRef: HTMLTextAreaElement | undefined;
  let pushTimer: ReturnType<typeof setTimeout> | undefined;
  // Detach for the resize gesture currently listening on window. Unmount must run
  // it: the pointer/key listeners outlive the component and would otherwise keep
  // handling events (a leaked capture-phase Escape handler swallows the textarea's
  // own Escape) for the rest of the session.
  let activeResizeDetach: (() => void) | null = null;
  let composing = false;
  let prevSessionKey: string | null = null;
  const [value, setValue] = createSignal("");

  const session = () => textEditSession();
  const layer = createMemo(() => {
    const s = session();
    if (!s) return null;
    // Read through the reactive layers() signal (same pattern as ShapeOptionBar)
    // so the binding updates immediately on engine change.
    const found = layers().find((l) => l.id === s.layerId);
    return found && found.type === "text" && found.textData ? found : null;
  });
  const textData = createMemo<TextData | null>(() => layer()?.textData ?? null);

  const editorForSession = () => ({ workspace, renderer, textEditSession: session, setTextEditSession, scheduler });

  // External value sync: when the session/layer changes (new session, undo,
  // option-bar edit), adopt the layer's content into the textarea. Focus +
  // select-all happen ONLY on session start (double-click re-edit UX) —
  // never on keystrokes, or typing would keep re-selecting all text.
  createEffect(() => {
    const s = session();
    const td = textData();
    const engine = workspace.getActiveEngine();
    if (!s) {
      // Session closed: drop the registered flush (so a stale overlay never
      // pushes after commit) and reset the session key so re-editing the SAME
      // layer in a later session still gets focus+select-all. Also unhide the
      // layer in the compositor (it was hidden so the canvas raster did not
      // double-draw under the textarea).
      setPendingTextFlush(prevSessionKey, null);
      if (engine && typeof engine.setRenderHiddenLayerId === "function") {
        engine.setRenderHiddenLayerId(null);
      }
      prevSessionKey = null;
      return;
    }
    if (!td) return;
    // Live Canvas Text Architecture:
    // Keep the layer 2D Canvas bitmap VISIBLE in the compositor. The textarea
    // text is styled `color: transparent`, so the user sees the real 2D Canvas
    // text underneath while typing with zero visual drift or commit jump.
    if (engine && typeof engine.getLayer === "function" && engine.getLayer(s.layerId)) {
      if (typeof engine.setRenderHiddenLayerId === "function") {
        engine.setRenderHiddenLayerId(null);
      }
    }
    // Adopt the engine content when it differs from the textarea. Reading
    // value() UNTRACKED is essential: a tracked read would make this effect
    // re-run on every keystroke and clobber the typed text back to the (still
    // debounced-stale) engine content. This effect must react only to session
    // and layer changes — never to our own typing.
    untrack(() => {
      if (td.content !== value()) setValue(td.content);
    });
    if (prevSessionKey !== s.layerId) {
      prevSessionKey = s.layerId;
      if (textareaRef) {
        textareaRef.focus();
        textareaRef.select();
      }
      queueMicrotask(() => {
        if (textareaRef) {
          textareaRef.focus();
          textareaRef.select();
        }
      });
    }
    // Register the pending-content flush so external commits (click-away,
    // tool switch) see the latest typed text before empty-commit cleanup runs.
    // The callback receives the SESSION's engine when commit/cancel resolved
    // it first (doc-switch path): the active engine may already be a DIFFERENT
    // document, and pushing there would drop the last keystrokes (@bug B2b).
    setPendingTextFlush(s.layerId, (engineOverride) => {
      if (pushTimer) {
        clearTimeout(pushTimer);
        pushTimer = undefined;
      }
      const content = value();
      pushContent(content, engineOverride);
      // Session close is the commit boundary for an owned layer. Cancel does not
      // go through here (see cancelTextSession), so this only ever runs with the
      // intent to keep the typing.
      commitRoutedContent(s.layerId, content, engineOverride);
    });
  });

  onCleanup(() => {
    if (pushTimer) clearTimeout(pushTimer);
    activeResizeDetach?.();
    // A resize gesture torn down by unmount must not leave its preview entry set
    // or a facade transform slot active: both outlive the component and a leaked
    // slot refuses every later numeric commit of that document. Gated on an open
    // session so another layer's or document's gesture is left alone.
    const s = textEditSession();
    const engine = workspace.getActiveEngine() as Partial<DocumentEngine> | null;
    if (s) {
      clearTransformPreview();
      if (typeof engine?.getId === "function") peekFacade(engine.getId())?.cancelTransform();
    }
  });

  const pushContent = (content: string, engineOverride?: DocumentEngine | null) => {
    const s = session();
    if (!s) return;
    // Engine override: commit/cancel pass the SESSION's engine (the doc-switch
    // wrapper) so pending text lands in the source document even after the
    // active document has changed (@bug B2b). `null`/undefined → active engine.
    const engine = engineOverride ?? workspace.getActiveEngine();
    if (!engine) return;
    const l = engine.getLayer(s.layerId);
    if (!l || l.type !== "text" || !l.textData) return;
    if (isFacadeEnabled() && isFacadeOwnedLayer(s.layerId)) {
      // The native arm owns this layer's textData and engine.updateTextData has no
      // ownership guard: writing the model here would let the two copies drift
      // until some later projection clobbered one of them. Rasterize host-side so
      // the frame still shows the typed text; the content itself reaches the
      // native side once, at session close (commitRoutedContent below).
      renderer?.uploadImage(s.layerId, rasterizeText({ ...l.textData, content }).imageBitmap);
      scheduler.requestRender();
      return;
    }
    engine.updateTextData(s.layerId, { ...l.textData, content });
    const bitmap = engine.getLayerImageBitmap(s.layerId);
    if (bitmap) renderer?.uploadImage(s.layerId, bitmap);
    scheduler.requestRender();
  };

  // One SetLayerParams command per session close for an owned layer. Nothing is
  // written to the model here: the arm stores the content and the projection
  // installs it, and the local re-raster regenerates the glyphs (and the box dims,
  // which the projection never writes) from the value it settled on.
  const commitRoutedContent = (
    layerId: string,
    content: string,
    engineOverride?: DocumentEngine | null,
  ) => {
    const engine = engineOverride ?? workspace.getActiveEngine();
    if (!engine) return;
    const layer = engine.getLayer(layerId);
    if (!layer || layer.type !== "text" || !layer.textData) return;
    if (layer.textData.content === content) return;
    commitRoutedTextParams(engine, layerId, { content }, { renderer, scheduler });
  };

  const schedulePush = (content: string) => {
    if (pushTimer) clearTimeout(pushTimer);
    pushTimer = setTimeout(() => {
      pushTimer = undefined;
      pushContent(content);
    }, RERASTER_DEBOUNCE_MS);
  };

  // Point mode: the raster commits a single unwrapped line, but a <textarea>
  // with width:auto computes to the browser default (~20ch), so long point
  // text WRAPS while typing and reflows to one line on commit — WYSIWYG drift
  // (@bug 2026-08-09 B8). Measure the current content and set an explicit
  // width so the box tracks the real glyphs (and the caret stays visible as
  // the line grows). jsdom has no canvas layout → getContext() returns null →
  // fall back to "auto" (tests and non-canvas hosts keep the old behavior).
  const pointWidthPx = createMemo<number | null>(() => {
    const s = session();
    const td = textData();
    if (!s || !td) return null;
    if (s.boxMode === "area" && s.boxWidth > 0) return null; // area: fixed width
    try {
      const probe = document.createElement("canvas");
      const probeCtx = probe.getContext("2d");
      if (!probeCtx) return null;
      const z = zoom();
      probeCtx.font = `${td.fontStyle === "italic" ? "italic " : ""}${td.fontWeight} ${td.fontSize * z}px "${td.fontFamily}", sans-serif`;
      // Parity with the rasterizer (B3): Chrome/WebView2 measureText INCLUDES
      // letterSpacing, so apply the same zoom-scaled spacing before measuring
      // or the width underestimates and the caret clips at the right edge.
      if ("letterSpacing" in probeCtx) {
        (probeCtx as CanvasRenderingContext2D & { letterSpacing?: string }).letterSpacing =
          `${td.letterSpacing * z}px`;
      }
      // Measure each line individually so newlines (\n) take max line width,
      // not total concatenated length. Apply uppercase transform if set.
      const rawContent = value().length > 0 ? value() : "Type text...";
      const textToMeasure = td.uppercase ? rawContent.toUpperCase() : rawContent;
      const lines = textToMeasure.split("\n");
      let maxW = 0;
      for (const line of lines) {
        const lw = probeCtx.measureText(line || " ").width;
        if (lw > maxW) maxW = lw;
      }
      return Math.max(1, Math.ceil(maxW) + 8); // +4px per side = 1px padding + 1px border + 2px safety
    } catch {
      return null;
    }
  });

  // Doc→screen positioning (same math as the artboard border: pan + doc*zoom).
  const overlayStyle = createMemo<JSX.CSSProperties>(() => {
    const s = session();
    const td = textData();
    if (!s || !td) return {};
    const z = zoom();
    const p = pan();
    const layerNode = layer();
    const tf = layerNode?.transform;
    const mirrored = tf && (tf.rotation !== 0 || tf.scaleX !== 1 || tf.scaleY !== 1 || tf.flipH || tf.flipV);
    const measured = pointWidthPx();
    // Stroke parity with the raster (B-stroke): the raster pads the canvas by
    // strokePad = stroke.width*effScale on every side and starts glyph ink at
    // PADDING + strokePad, so a thick outline shifts the text down-right and
    // grows the frame. Mirror that here: pad = 1px + stroke.width*z, width +=
    // 2*stroke.width*z. CSS -webkit-text-stroke, like the canvas stroke, is
    // centered on the glyph outline and the fill covers its inner half — the
    // raster compensates with lineWidth = strokePad*2, so the visible stroke
    // is stroke.width doc px → the CSS value must be 2× stroke.width*z.
    const strokePx = td.stroke && td.stroke.width > 0 ? td.stroke.width * z : 0;
    const sizePad = `${1 + strokePx}px`;
    const widthPx =
      s.boxMode === "area" && s.boxWidth > 0
        ? `${s.boxWidth * z + 2 * strokePx}px`
        : measured !== null
          ? `${measured + 2 * strokePx}px`
          : "auto";
    // W3C Baseline Parity Equation:
    // Outer top is locked at docY so the orange border frame matches SelectionTransformOverlay 100%.
    // Inside <textarea>, CSS line-height adds topLeading = (lineHeight - 1.0) * 0.5 * fontPx above line 0.
    // Setting margin-top: -topLeading pulls the text content UP so line 0 alphabetic baseline matches
    // 2D Canvas (textRasterizer.ts) with 0.000000px shift:
    //   textarea_baseline = docY + 1px(border) + strokePx(padding) - topLeading(margin) + topLeading(leading) + fontAscent
    //                     = docY + 1px + strokePx + fontAscent (EXACT 100% MATCH WITH 2D CANVAS!)
    const fontPx = td.fontSize * z;
    const topLeading = (td.lineHeight - 1.0) * 0.5 * fontPx;
    const lineTotalH = Math.max(32, rows() * fontPx * td.lineHeight + 2 * strokePx + 4);
    const areaH = s.boxMode === "area" && s.boxHeight > 0 ? s.boxHeight * z + 2 * strokePx : 0;
    const heightPx = `${Math.max(areaH, lineTotalH)}px`;

    return {
      position: "absolute",
      left: `${p.x + s.docX * z}px`,
      top: `${p.y + s.docY * z}px`,
      "margin-top": `-${topLeading}px`,
      // border-box: width includes border + padding.
      boxSizing: "border-box",
      width: widthPx,
      minWidth: OVERLAY_MIN_WIDTH,
      height: heightPx,
      minHeight: heightPx,
      "font-family": `"${td.fontFamily}", sans-serif`,
      "font-size": `${fontPx}px`,
      "font-weight": td.fontWeight,
      "font-style": td.fontStyle,
      color: "transparent",
      "-webkit-text-fill-color": "transparent",
      "text-align": td.align,
      "line-height": td.lineHeight,
      "letter-spacing": `${td.letterSpacing * z}px`,
      "text-decoration": [td.underline ? "underline" : "", td.strikethrough ? "line-through" : ""].filter(Boolean).join(" ") || "none",
      "text-transform": td.uppercase ? "uppercase" : "none",
      "-webkit-text-stroke": td.stroke && td.stroke.width > 0 ? `${2 * strokePx}px ${td.stroke.color}` : "0px",
      "paint-order": td.stroke && td.stroke.align === "outside" ? "stroke fill" : "fill stroke",
      background: "transparent",
      resize: "none",
      overflow: "hidden",
      border: "1px solid var(--color-editor-accent, #E15A17)",
      boxShadow: "0 0 0 1px rgba(0, 0, 0, 0.8), 0 0 0 3px rgba(255, 255, 255, 0.4)",
      outline: "none",
      padding: `${strokePx}px ${strokePx}px ${1 + strokePx}px ${strokePx}px`,
      "caret-color": "var(--color-editor-accent, #E15A17)",
      "white-space": "pre-wrap",
      "word-break": "break-word",
      "pointer-events": s.isDragging ? "none" : "auto",
      opacity: layerNode?.opacity ?? 1,
      // Mirror the layer's rotation/scale/flip so the edit box matches the
      // rendered frame. The engine composites around the layer center, so the
      // overlay rotates about its center too.
      transform: mirrored
        ? `rotate(${tf.rotation}deg) scale(${tf.scaleX * (tf.flipH ? -1 : 1)}, ${tf.scaleY * (tf.flipV ? -1 : 1)})`
        : undefined,
      "transform-origin": "center",
      "z-index": 60,
    };
  });

  // Auto-grow rows: while editing, the layer is hidden from the compositor,
  // so this textarea is the ONLY visible rendering of the text. A fixed
  // rows=2 default would clip every line past the second (invisible typing).
  // Estimate the visible rows from content: explicit newlines plus a cheap
  // soft-wrap estimate against the known box width.
  const rows = createMemo(() => {
    const s = session();
    const td = textData();
    if (!s || !td) return 2;
    const z = zoom();
    const widthPx = Math.max(160, s.boxMode === "area" && s.boxWidth > 0 ? s.boxWidth * z : 320);
    const perLine = Math.max(10, Math.floor(widthPx / Math.max(6, td.fontSize * z * 0.5)));
    let n = 0;
    for (const line of value().split("\n")) n += Math.max(1, Math.ceil(line.length / perLine));
    // No cap: the overlay IS the only visible rendering while editing (the
    // layer is hidden), so capping rows would clip long content out of sight.
    return Math.max(1, n);
  });

  // Floating shortcut badge position (centered below the editing box)
  const shortcutBadgeStyle = createMemo<JSX.CSSProperties>(() => {
    const s = session();
    const td = textData();
    if (!s || !td) return {};
    const z = zoom();
    const p = pan();
    const layerNode = layer();
    const tf = layerNode?.transform;
    const mirrored = tf && (tf.rotation !== 0 || tf.scaleX !== 1 || tf.scaleY !== 1 || tf.flipH || tf.flipV);
    const measured = pointWidthPx();
    const strokePx = td.stroke && td.stroke.width > 0 ? td.stroke.width * z : 0;
    const boxW = s.boxMode === "area" && s.boxWidth > 0
      ? s.boxWidth * z + 2 * strokePx
      : (measured ?? 160) + 2 * strokePx;
    // Position below the top-left origin + actual box height
    const boxH = s.boxMode === "area" && s.boxHeight > 0
      ? s.boxHeight * z + 2 * strokePx
      : Math.max(32, rows() * td.fontSize * z * td.lineHeight);
    return {
      position: "absolute",
      left: `${p.x + s.docX * z + boxW / 2}px`,
      top: `${p.y + s.docY * z + boxH + 12}px`,
      transform: mirrored ? `rotate(${tf.rotation}deg) translateX(-50%)` : "translateX(-50%)",
      "transform-origin": "center",
      "z-index": 61,
      "pointer-events": "none",
    };
  });

  // Top-right drag dimension badge during area box creation
  const dragDimensionStyle = createMemo<JSX.CSSProperties>(() => {
    const s = session();
    const td = textData();
    if (!s || !td) return {};
    const z = zoom();
    const p = pan();
    const strokePx = td.stroke && td.stroke.width > 0 ? td.stroke.width * z : 0;
    const boxW = s.boxWidth > 0 ? s.boxWidth * z + 2 * strokePx : 160;
    return {
      position: "absolute",
      left: `${p.x + s.docX * z + boxW}px`,
      top: `${p.y + s.docY * z - 24}px`,
      transform: "translateX(-100%)",
      "z-index": 62,
      "pointer-events": "none",
    };
  });

  // Interactive resize handle start handler for Area mode
  const handleResizeStart = (
    e: PointerEvent,
    corner: "tl" | "tr" | "bl" | "br"
  ) => {
    e.stopPropagation();
    e.preventDefault();
    const s = session();
    const td = textData();
    const startLayer = layer();
    const engine = workspace.getActiveEngine();
    if (!s || !td || !startLayer || !engine) return;

    const startClientX = e.clientX;
    const startClientY = e.clientY;
    const startDocX = s.docX;
    const startDocY = s.docY;
    const startW = s.boxWidth;
    const startH = s.boxHeight > 0 ? s.boxHeight : Math.round(rows() * td.fontSize * td.lineHeight);
    const startTransform = { ...startLayer.transform };

    // Routed only when the facade is on AND the native arm already owns this
    // layer: every per-frame legacy write below would otherwise either throw
    // (transformLayer rejects an owned layer) or mutate the TS model behind the
    // native one's back (updateTextData has no ownership guard). The temp layer
    // of a CREATE gesture is never owned, so creating text is unaffected.
    const routed = isFacadeEnabled() && isFacadeOwnedLayer(s.layerId);

    // Box geometry for a pointer position, shared by pointermove and pointerup so
    // the value committed at release cannot drift from the last previewed one.
    const boxFor = (clientX: number, clientY: number) => {
      const z = zoom();
      if (z <= 0) return null;
      const dx = (clientX - startClientX) / z;
      const dy = (clientY - startClientY) / z;

      let newW = startW;
      let newH = startH;
      let newX = startDocX;
      let newY = startDocY;

      if (corner === "br" || corner === "tr") {
        newW = Math.max(20, startW + dx);
      }
      if (corner === "bl" || corner === "tl") {
        newW = Math.max(20, startW - dx);
        newX = startDocX + (startW - newW);
      }
      if (corner === "br" || corner === "bl") {
        newH = Math.max(20, startH + dy);
      }
      if (corner === "tr" || corner === "tl") {
        newH = Math.max(20, startH - dy);
        newY = startDocY + (startH - newH);
      }

      newW = Math.round(newW);
      newH = Math.round(newH);
      newX = Math.round(newX);
      newY = Math.round(newY);
      return { x: newX, y: newY, w: newW, h: newH };
    };

    const targetEl = e.currentTarget as HTMLElement;
    try {
      targetEl.setPointerCapture(e.pointerId);
    } catch {}

    // Back to the pre-gesture pixels and box. The routed frames only touched the
    // renderer preview, so the texture has to be re-uploaded from the model's own
    // bitmap, and the session has to be put back at the box it started from.
    const restoreRouted = () => {
      const current = engine.getLayer(s.layerId);
      if (current?.imageBitmap) renderer?.uploadImage(s.layerId, current.imageBitmap);
      clearTransformPreview();
      // peekFacade, never getFacade: teardown must not create a facade for a
      // document that has none. Releasing the slot defensively is cheap and a
      // slot leaked by any earlier gesture permanently refuses later numeric
      // edits of that document.
      peekFacade(engine.getId())?.cancelTransform();
      setTextEditSession({ ...s });
      scheduler.requestRender();
    };

    const detach = () => {
      activeResizeDetach = null;
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
      if (routed) {
        window.removeEventListener("pointercancel", onPointerCancel);
        window.removeEventListener("keydown", onKeyDown, true);
      }
    };

    const onPointerMove = (moveEv: PointerEvent) => {
      moveEv.stopPropagation();
      const box = boxFor(moveEv.clientX, moveEv.clientY);
      if (!box) return;

      if (routed) {
        // No model write and no command per frame. The size travels WITH the
        // transform because the renderer draws the quad from the RenderState
        // width/height, and the model keeps its old size until release.
        setTransformPreview([
          {
            layerId: s.layerId,
            transform: { ...startTransform, x: box.x, y: box.y },
            width: box.w,
            height: box.h,
          },
        ]);
        setTextEditSession({ ...s, docX: box.x, docY: box.y, boxMode: "area", boxWidth: box.w, boxHeight: box.h });
        // Reflow the glyphs host-side: no native arm rasterizes text, and the
        // model bitmap must not move per frame, so the frame rasterizes the
        // target box straight to the renderer.
        const raster = rasterizeText({ ...td, boxMode: "area", boxWidth: box.w, boxHeight: box.h });
        renderer?.uploadImage(s.layerId, raster.imageBitmap);
        scheduler.requestRender();
        return;
      }

      engine.transformLayer(s.layerId, { x: box.x, y: box.y });
      engine.updateTextData(s.layerId, {
        ...td,
        boxMode: "area",
        boxWidth: box.w,
        boxHeight: box.h,
      });
      setTextEditSession({
        ...s,
        docX: box.x,
        docY: box.y,
        boxMode: "area",
        boxWidth: box.w,
        boxHeight: box.h,
      });
      const bitmap = engine.getLayerImageBitmap(s.layerId);
      if (bitmap) renderer?.uploadImage(s.layerId, bitmap);
      scheduler.requestRender();
    };

    const finishRouted = async (box: { x: number; y: number; w: number; h: number }) => {
      const target: TextData = { ...td, boxMode: "area", boxWidth: box.w, boxHeight: box.h };
      try {
        // One native transform entry through the shared numeric funnel: no TS
        // history entry and no engine mutator. A no-op is legitimate, because a
        // corner that does not move the origin (bottom-right) changes only the
        // box.
        const status = await routeNumericTransform(
          engine,
          s.layerId,
          { x: box.x, y: box.y },
          { requestRender: () => scheduler.requestRender(), notifyVisualChange: () => {} },
        );
        // routeNumericTransform has already surfaced its own failure toast.
        if (status === "error") {
          restoreRouted();
          return;
        }
        // One native params entry. This is the command whose projection
        // (DocumentEngine.applyFacadeSnapshot) writes textData back into the model.
        const result = await commitFacadeParams(engine, [s.layerId], { textData: target });
        if (result.status !== "applied") {
          throw new Error(`text params were not applied (${result.status})`);
        }
        // A command reports success even when the arm restates no layer (an id the
        // engine does not hold); the projection then wrote nothing and the local
        // write below would move the box while the engine kept the old one. The
        // box fields are what this commit changes, so they are the equality signal.
        const settledTd = engine.getLayer(s.layerId)?.textData;
        if (
          !sameParams(
            settledTd && {
              boxMode: settledTd.boxMode,
              boxWidth: settledTd.boxWidth,
              boxHeight: settledTd.boxHeight,
            },
            { boxMode: target.boxMode, boxWidth: target.boxWidth, boxHeight: target.boxHeight },
          )
        ) {
          throw new Error("the native engine does not hold this layer");
        }
        // The committed box reflows here: neither native arm rasterizes text, so
        // the bitmap AND the layer's box dims are regenerated from the textData
        // the projection just wrote. This is the same call the legacy branch
        // makes per frame, which is what keeps the two end states equal.
        // Ownership note: the value written is the one commitFacadeParams just
        // projected from the native restatement, so this is not the unguarded
        // model write the routed path exists to avoid - it re-rasterizes what the
        // native arm already owns.
        engine.updateTextData(s.layerId, target);
        clearTransformPreview();
        scheduler.requestRender();
      } catch (err) {
        restoreRouted();
        showToast(`Cannot resize text: ${err instanceof Error ? err.message : String(err)}`, "error");
      }
    };

    const onPointerUp = (upEv: PointerEvent) => {
      upEv.stopPropagation();
      detach();
      try {
        targetEl.releasePointerCapture(upEv.pointerId);
      } catch {}
      if (!routed) return;
      const box = boxFor(upEv.clientX, upEv.clientY);
      if (!box) {
        // Box math yields nothing (zoom <= 0): release the preview and the slot
        // rather than leaving the last previewed frame on screen and a slot that
        // would refuse every later numeric commit of this document.
        restoreRouted();
        return;
      }
      void finishRouted(box);
    };

    // Routed-only exits. The legacy path had none: its model already reflects
    // every frame, so there is nothing to roll back on cancel.
    const onPointerCancel = (ev: PointerEvent) => {
      ev.stopPropagation();
      detach();
      restoreRouted();
    };

    // Capture phase so a mid-resize Escape abandons the RESIZE and is consumed
    // before the textarea's own handler runs cancelTextSession() - closing the
    // whole edit session is a different (and much bigger) action.
    const onKeyDown = (ev: KeyboardEvent) => {
      if (ev.key !== "Escape") return;
      ev.preventDefault();
      ev.stopPropagation();
      detach();
      restoreRouted();
    };

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
    activeResizeDetach = detach;
    if (routed) {
      window.addEventListener("pointercancel", onPointerCancel);
      window.addEventListener("keydown", onKeyDown, true);
    }
  };

  // Corner handle position generator for Area mode
  const cornerHandleStyle = (corner: "tl" | "tr" | "bl" | "br"): JSX.CSSProperties => {
    const s = session();
    const td = textData();
    if (!s || !td || s.boxMode !== "area" || s.boxWidth <= 0) return { display: "none" };
    const z = zoom();
    const p = pan();
    const strokePx = td.stroke && td.stroke.width > 0 ? td.stroke.width * z : 0;
    const boxW = s.boxWidth * z + 2 * strokePx;
    const estimatedHeight = s.boxHeight > 0
      ? s.boxHeight * z + 2 * strokePx
      : Math.max(32, rows() * td.fontSize * z * td.lineHeight);

    const isRight = corner === "tr" || corner === "br";
    const isBottom = corner === "bl" || corner === "br";

    const cursorMap = {
      tl: "nwse-resize",
      tr: "nesw-resize",
      bl: "nesw-resize",
      br: "nwse-resize",
    };

    return {
      position: "absolute",
      left: `${p.x + s.docX * z + (isRight ? boxW : 0) - 4}px`,
      top: `${p.y + s.docY * z + (isBottom ? estimatedHeight : 0) - 4}px`,
      width: "8px",
      height: "8px",
      "z-index": 61,
      "pointer-events": "auto",
      cursor: cursorMap[corner],
    };
  };

  return (
    <Show when={session() && textData()}>
      <textarea
        ref={textareaRef}
        data-text-edit-overlay
        aria-label="Edit text"
        placeholder={t("tools.options.typeText", "Type text...")}
        rows={rows()}
        value={value()}
        spellcheck={false}
        onPointerDown={(e) => e.stopPropagation()}
        onPointerUp={(e) => e.stopPropagation()}
        onPointerMove={(e) => e.stopPropagation()}
        onPointerCancel={(e) => e.stopPropagation()}
        onLostPointerCapture={(e) => e.stopPropagation()}
        onCompositionStart={() => { composing = true; }}
        onCompositionEnd={(e) => {
          composing = false;
          const v = e.currentTarget.value;
          setValue(v);
          schedulePush(v);
        }}
        onScroll={(e) => { e.currentTarget.scrollTop = 0; }}
        onKeyUp={(e) => { e.currentTarget.scrollTop = 0; }}
        onInput={(e) => {
          e.currentTarget.scrollTop = 0;
          const v = e.currentTarget.value;
          setValue(v);
          if (!composing) schedulePush(v);
        }}
        onKeyDown={(e) => {
          if (e.key === "Escape" && !composing) {
            e.preventDefault();
            e.stopPropagation();
            cancelTextSession(editorForSession());
            return;
          }
          // NOTE: Ctrl/Cmd+Z is intentionally NOT intercepted here — it falls
          // through to the textarea's native in-field undo. Both global key
          // handlers (useCanvasKeyboard capture + useEditorCommands bubble)
          // already early-return while a text-edit session is active or focus
          // is in a TEXTAREA, so the event never reaches document-level undo.
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && !composing) {
            e.preventDefault();
            e.stopPropagation();
            // Flush any pending debounce so the committed layer matches what
            // the user sees (empty-commit cleanup then sees real content).
            if (pushTimer) {
              clearTimeout(pushTimer);
              pushTimer = undefined;
            }
            pushContent(value());
            commitTextSession(editorForSession());
          }
        }}
        style={overlayStyle()}
        class="placeholder:opacity-50 placeholder:text-editor-text-dim"
      />

      {/* 4 Corner handles for Area Text Mode */}
      <Show when={session()?.boxMode === "area" && (session()?.boxWidth ?? 0) > 0}>
        <div
          data-text-handle="tl"
          style={cornerHandleStyle("tl")}
          onPointerDown={(e) => handleResizeStart(e, "tl")}
          class="rounded-[1px] border border-black/90 bg-editor-accent shadow-xs select-none"
        />
        <div
          data-text-handle="tr"
          style={cornerHandleStyle("tr")}
          onPointerDown={(e) => handleResizeStart(e, "tr")}
          class="rounded-[1px] border border-black/90 bg-editor-accent shadow-xs select-none"
        />
        <div
          data-text-handle="bl"
          style={cornerHandleStyle("bl")}
          onPointerDown={(e) => handleResizeStart(e, "bl")}
          class="rounded-[1px] border border-black/90 bg-editor-accent shadow-xs select-none"
        />
        <div
          data-text-handle="br"
          style={cornerHandleStyle("br")}
          onPointerDown={(e) => handleResizeStart(e, "br")}
          class="rounded-[1px] border border-black/90 bg-editor-accent shadow-xs select-none"
        />
      </Show>

      {/* Floating shortcut badge below the active edit box */}
      <div
        data-text-shortcut-badge
        style={shortcutBadgeStyle()}
        class="flex items-center gap-1.5 whitespace-nowrap rounded-[3px] border border-editor-field-border/80 bg-editor-panel/95 px-2 py-0.5 text-[10px] text-editor-text-dim shadow-md backdrop-blur-xs select-none"
      >
        <span><strong class="font-semibold text-editor-text">↵</strong> {t("tools.textActions.newline", "Newline")}</span>
        <span class="opacity-40">·</span>
        <span><strong class="font-semibold text-editor-accent">Ctrl+Enter</strong> {t("tools.textActions.commit", "Commit")}</span>
        <span class="opacity-40">·</span>
        <span><strong class="font-semibold text-editor-text">Esc</strong> {t("common.cancel", "Cancel")}</span>
      </div>

      {/* Live drag dimension badge while creating an area text box */}
      <Show when={session()?.isNewLayer && session()?.boxMode === "area" && (session()?.boxWidth ?? 0) > 0}>
        <div
          data-text-drag-dimension
          style={dragDimensionStyle()}
          class="flex items-center gap-1 whitespace-nowrap rounded-[3px] border border-black/60 bg-editor-accent px-1.5 py-0.5 text-[10px] font-semibold text-white shadow-sm select-none"
        >
          W: {session()?.boxWidth} px
        </div>
      </Show>
    </Show>
  );
}
