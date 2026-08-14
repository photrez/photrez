// SPDX-License-Identifier: AGPL-3.0-or-later
// Text tool pointer wiring (mirrors pointerTools/shapeTool.ts).
//
// Flow (plan §8.3, the "most-often-forgotten" dispatcher step):
//   pointerdown on empty        → create temp text layer + open edit session
//   pointerdown on text layer   → re-edit session on that layer (no new layer)
//   drag beyond threshold       → grow the box into area mode (boxWidth)
//   pointerup                   → session stays open for typing; history
//                                 commits at session CLOSE (not at pointerup)
//                                 so one undo step spans the whole edit.
//   session close (commit)      → "Add Text" / "Edit Text"; empty new layer
//                                 is deleted with NO history entry.
import { hitTestLayers, type LayerInfo } from "@/viewport/layerHitTest";
import { trySetPointerCapture, tryReleasePointerCapture } from "../../tools/pointerCapture";
import { showToast } from "../../Toast";
import type { PointerToolContext } from "./pointerToolContext";
import type { EditorAccessors } from "./pointerToolContext";
import type { LayerNode } from "@/engine/types";
import type { DocumentEngine } from "@/engine/document";
import { type TextData, DEFAULT_TEXT_DATA } from "@/engine/textTypes";
import type { TextEditSession } from "../../tools/editorState";

/** Drag beyond this many doc px switches point text into area-box mode. */
const MIN_AREA_PX = 3;

/** Default name for a freshly created text layer (auto-replaced by typed content). */
const DEFAULT_TEXT_LAYER_NAME = "Text";

/** Mutable drag state (plain object, like shapeTool). */
export interface TextPointerState {
  start: { x: number; y: number } | null;
  isDragging: boolean;
  reset: () => void;
}

/** Minimal editor surface needed for session commit/cancel (tool-switch safe). */
export type TextSessionEditor = Pick<
  EditorAccessors,
  "workspace" | "textEditSession" | "setTextEditSession" | "scheduler"
> & { renderer?: EditorAccessors["renderer"] };

/**
 * Minimal editor surface for opening a session on an existing layer by id
 * (layer-panel double-click re-edit, plan §7.3).
 */
export type TextSessionOpener = TextSessionEditor &
  Pick<EditorAccessors, "setActiveTool" | "setSelectedLayerId">;

// ── Pending-content flush registry ──────────────────────────────────────────
// The edit overlay debounces live re-raster (R4) but commits can also come
// from OUTSIDE the overlay (click-away pointerdown, tool-switch auto-commit).
// Those paths must see the user's latest keystrokes — otherwise a new temp
// layer with typed-but-not-yet-pushed content hits the empty-commit cleanup
// and gets deleted. The overlay registers a flush callback; every external
// commit/cancel flushes it first.
/**
 * A flush callback registered by the edit overlay. Receives the session's
 * engine when the caller resolved it first (commit/cancel), so external
 * commits on a document switch push into the SOURCE document rather than the
 * now-active one.
 */
type PendingTextFlush = (engine?: DocumentEngine | null) => void;

// Per-layer flush registry (keyed by session layerId, NOT a single process-wide
// singleton). Best practice: scope the pending-content flush to the session it
// belongs to so concurrent/switched documents can't clobber each other's flush
// (the old single `let` could be overwritten if two overlays ever registered).
const pendingTextFlushByLayer = new Map<string, PendingTextFlush>();

/** The overlay registers its pending-content flush for the open session's layer. */
export function setPendingTextFlush(layerId: string | null, fn: PendingTextFlush | null): void {
  if (layerId == null) return;
  if (fn) pendingTextFlushByLayer.set(layerId, fn);
  else pendingTextFlushByLayer.delete(layerId);
}

/**
 * Flush any pending overlay content so commits see the latest text. Keyed by
 * `layerId` (stable for the whole session), so the doc-switch commit — which
 * uses a wrapper whose getActiveEngine returns the SOURCE engine — still finds
 * the flush registered by the overlay on that layer (@bug 2026-08-09 B2b).
 * When `engine` is provided the overlay pushes into THAT engine.
 */
export function flushPendingText(layerId: string | null, engine?: DocumentEngine | null): void {
  if (layerId == null) return;
  const fn = pendingTextFlushByLayer.get(layerId);
  if (fn) {
    pendingTextFlushByLayer.delete(layerId);
    fn(engine);
  }
}

/**
 * Re-anchor an open session's preSnapshot after undo/redo (or any engine
 * restore): the user is now looking at an OLDER state, so the next commit
 * must diff against what they SEE, not the state captured at session open —
 * otherwise the undo step would jump the history back past the stale operand.
 * No-op when the session is closed or its layer was deleted by the restore.
 */
export function syncTextSessionBase(editor: TextSessionEditor): void {
  if (typeof editor.textEditSession !== "function") return;
  const session = editor.textEditSession();
  if (!session || !session.preSnapshot) return;
  const engine = editor.workspace.getActiveEngine();
  if (!engine) return;
  const layer = engine.getLayer(session.layerId);
  // The layer this session points at was removed by undo/redo (or another
  // command). Drop the orphan session instead of leaving a dangling overlay
  // pointed at a deleted layer — otherwise the edit box renders at (0,0)
  // (@bug 2026-08-14, sibling of the click-away stray-session bug).
  if (!layer) {
    editor.setTextEditSession(null);
    return;
  }
  // Re-anchor both the pre-snapshot (next commit diffs against what the user
  // now sees) and the overlay position (docX/docY) to the restored layer
  // transform, so the edit box stays glued to the text after undo/redo.
  editor.setTextEditSession({
    ...session,
    preSnapshot: engine.snapshot(),
    docX: layer.transform.x,
    docY: layer.transform.y,
  });
}

// Uniform accessor contract: every editor signal is treated as optionally
// present (harness/test mocks may not implement the full EditorAccessors), so
// all reads use the same `isFn` guard with a sensible default. This removes the
// previous inconsistency where some fields were guarded and others assumed
// present.
function buildSessionTextData(editor: EditorAccessors): TextData {
  const isFn = (v: unknown): v is () => unknown => typeof v === "function";
  return {
    content: "",
    fontFamily: isFn(editor.textFontFamily) ? editor.textFontFamily() : "Arial",
    fontSize: isFn(editor.textFontSize) ? editor.textFontSize() : 48,
    fontWeight: isFn(editor.textFontWeight) ? editor.textFontWeight() : 400,
    fontStyle: isFn(editor.textFontItalic) && editor.textFontItalic() ? "italic" : "normal",
    color: isFn(editor.fgColor) ? editor.fgColor() : "#000000",
    align: isFn(editor.textAlign) ? editor.textAlign() : "left",
    // New text inherits the canonical defaults (single source of truth in
    // textTypes.ts) instead of hardcoding 1.2 / 0 here.
    lineHeight: DEFAULT_TEXT_DATA.lineHeight,
    letterSpacing: DEFAULT_TEXT_DATA.letterSpacing,
    boxMode: "point",
    boxWidth: 0,
    boxHeight: 0,
    stroke: {
      width: isFn(editor.textStrokeWidth) ? editor.textStrokeWidth() : 0,
      color: isFn(editor.textStrokeColor) ? editor.textStrokeColor() : "#000000",
    },
    underline: isFn(editor.textUnderline) ? editor.textUnderline() : false,
    strikethrough: isFn(editor.textStrikethrough) ? editor.textStrikethrough() : false,
    uppercase: isFn(editor.textUppercase) ? editor.textUppercase() : false,
  };
}

function textDataEquals(a: TextData, b: TextData): boolean {
  return (
    a.content === b.content
    && a.fontFamily === b.fontFamily
    && a.fontSize === b.fontSize
    && a.fontWeight === b.fontWeight
    && a.fontStyle === b.fontStyle
    && a.color === b.color
    && a.align === b.align
    && a.lineHeight === b.lineHeight
    && a.letterSpacing === b.letterSpacing
    && a.boxMode === b.boxMode
    && a.boxWidth === b.boxWidth
    && (a.boxHeight ?? 0) === (b.boxHeight ?? 0)
    && (a.stroke?.width ?? 0) === (b.stroke?.width ?? 0)
    && (a.stroke?.color ?? "#000000") === (b.stroke?.color ?? "#000000")
    && !!a.underline === !!b.underline
    && !!a.strikethrough === !!b.strikethrough
    && !!a.uppercase === !!b.uppercase
  );
}

function layerTextData(layer: LayerNode | null | undefined): TextData | null {
  return layer && layer.type === "text" && layer.textData ? layer.textData : null;
}

type SessionOpener = Pick<EditorAccessors, "setTextEditSession" | "setSelectedLayerId">;

function openSession(
  editor: SessionOpener,
  session: TextEditSession,
  selectId?: string,
): void {
  editor.setTextEditSession(session);
  if (selectId) editor.setSelectedLayerId(selectId);
}

/**
 * Open a re-edit session on an existing text layer by id — the layer-panel
 * double-click path (plan §7.3). Mirrors the hit-test re-edit branch of
 * startTextPointer but is driven by the layer id instead of a canvas pointer
 * event: switches to the text tool, selects the layer, and opens a session
 * anchored at the layer's current doc position. Any pending session is
 * committed first (click-away pattern). Returns false when the layer isn't
 * editable text (caller falls back to default row behavior, e.g. rename).
 */
export function openTextEditSession(
  editor: TextSessionOpener,
  layerId: string,
): boolean {
  const engine = editor.workspace.getActiveEngine();
  if (!engine) return false;
  const layer = engine.getLayer(layerId);
  const data = layerTextData(layer);
  if (!layer || !data) return false;

  // Persist any pending session before starting a new interaction (click-away
  // commit). Empty new layers are removed by commitTextSession's cleanup.
  if (editor.textEditSession()) {
    commitTextSession(editor);
  }

  const pre = engine.snapshot();
  editor.setActiveTool("text");
  openSession(editor, {
    layerId: layer.id,
    docX: layer.transform.x,
    docY: layer.transform.y,
    boxMode: data.boxMode,
    boxWidth: data.boxWidth,
    boxHeight: data.boxHeight ?? 0,
    isNewLayer: false,
    preSnapshot: pre,
  }, layer.id);
  editor.scheduler.requestRender();
  return true;
}

/**
 * pointerdown: hit → re-edit existing text layer; otherwise create a temp text
 * layer at the click point (point mode). Commits any PREVIOUS session first so
 * clicking elsewhere while editing persists the pending text as one step.
 */
export function startTextPointer(
  ctx: PointerToolContext,
  e: PointerEvent,
  state: TextPointerState,
): boolean {
  const { editor } = ctx;
  if (editor.activeTool() !== "text") return false;
  const engine = editor.workspace.getActiveEngine();
  if (!engine) return true;

  const coords = ctx.getDocCoords(e);
  state.start = { x: coords.x, y: coords.y };
  state.isDragging = true;

  // Hit test (alpha-aware, same sampler as move tool) — existing text layer
  // starts a re-edit session instead of creating a duplicate on top.
  const allLayers = [...engine.getLayers()];
  const hit = hitTestLayers(coords, allLayers as LayerInfo[], (id, x, y) => engine.sampleLayerAlpha(id, x, y));

  const activeSession = editor.textEditSession();
  if (activeSession) {
    const activeLayer = engine.getLayer(activeSession.layerId);
    const boxW = activeSession.boxWidth > 0 ? activeSession.boxWidth : Math.max(160, activeLayer?.width ?? 160);
    const boxH = activeSession.boxHeight > 0 ? activeSession.boxHeight : Math.max(40, activeLayer?.height ?? 40);
    const isInsideActiveBox = activeLayer && (
      coords.x >= activeSession.docX - 15 &&
      coords.x <= activeSession.docX + boxW + 15 &&
      coords.y >= activeSession.docY - 15 &&
      coords.y <= activeSession.docY + boxH + 15
    );

    // Clicking on the SAME text layer or inside its bounding box: keep session active!
    if ((hit && hit.id === activeSession.layerId) || isInsideActiveBox) {
      trySetPointerCapture(ctx.getCanvasRef(), e.pointerId);
      return true;
    }
    // Clicking away: commit the pending session and FINISH the edit (exit text
    // mode). Do not fall through to start a new empty session — that left a
    // stray empty text box on the canvas and turned "click = done" into
    // "click = start another text". A second click starts a fresh text.
    commitTextSession(ctx.editor);
    return true;
  }
  // Opening a NEW or re-edit text session from a canvas pointerdown. Prevent
  // the browser's default mousedown focus shift (to <body>/canvas) from
  // stealing focus from the overlay <textarea> — without this, the user must
  // click the text field a second time before they can type (@ux 2026-08-14).
  // `?.` because some callers (e.g. the double-click handler) pass a MouseEvent
  // mock without preventDefault; real pointer/mouse events always have it.
  e.preventDefault?.();

  if (hit) {
    const hitLayer = engine.getLayer(hit.id);
    const hitData = layerTextData(hitLayer);
    if (hitLayer && hitData) {
      const pre = engine.snapshot();
      openSession(editor, {
        layerId: hitLayer.id,
        // Anchor the overlay at the LAYER's box origin, never the click point:
        // transform.x/y is the raster box's top-left corner (drawLayerToContext)
        // and the overlay positions its textarea at docX/docY. Using the click
        // point displaced the edit box from the real text whenever the click
        // landed anywhere inside the box (box-hittable rule) but not at its
        // origin — the text visibly "jumped" while editing and snapped back on
        // commit (@bug 2026-08-09 B1). Matches openTextEditSession.
        docX: hitLayer.transform.x,
        docY: hitLayer.transform.y,
        boxMode: hitData.boxMode,
        boxWidth: hitData.boxWidth,
        boxHeight: hitData.boxHeight ?? 0,
        isNewLayer: false,
        preSnapshot: pre,
      }, hitLayer.id);
      trySetPointerCapture(ctx.getCanvasRef(), e.pointerId);
      return true;
    }
    // Hit a non-text layer → fall through and create text on top.
  }

  // Create temp text layer.
  const pre = engine.snapshot();
  try {
    const layer = engine.addTextLayer(DEFAULT_TEXT_LAYER_NAME, buildSessionTextData(editor));
    engine.transformLayer(layer.id, { x: coords.x, y: coords.y });
    const bitmap = engine.getLayerImageBitmap(layer.id);
    if (bitmap) editor.renderer?.uploadImage(layer.id, bitmap);
    openSession(editor, {
      layerId: layer.id,
      docX: coords.x,
      docY: coords.y,
      boxMode: "point",
      boxWidth: 0,
      boxHeight: 0,
      isNewLayer: true,
      isDragging: true,
      preSnapshot: pre,
    }, layer.id);
  } catch (err) {
    showToast(`Text failed: ${err instanceof Error ? err.message : "Unknown error"}`, "error");
    state.reset();
    return true;
  }
  trySetPointerCapture(ctx.getCanvasRef(), e.pointerId);
  editor.scheduler.requestRender();
  return true;
}

/**
 * pointermove: drag beyond MIN_AREA_PX turns the temp layer into an area box
 * (boxMode "area", boxWidth = |dx|, boxHeight = |dy|), re-rasterizing live. Point text stays put.
 */
export function trackTextPointer(
  ctx: PointerToolContext,
  e: PointerEvent,
  state: TextPointerState,
): boolean {
  const { editor } = ctx;
  if (editor.activeTool() !== "text" || !state.isDragging || !state.start) return false;

  const engine = editor.workspace.getActiveEngine();
  const session = editor.textEditSession();
  if (!engine || !session || !session.isNewLayer) return true;

  const coords = ctx.getDocCoords(e);
  // The box spans from the MIN(left, right) and MIN(top, bottom) drag edges.
  const left = Math.min(state.start.x, coords.x);
  const top = Math.min(state.start.y, coords.y);
  const width = Math.max(1, Math.abs(coords.x - state.start.x));
  const height = Math.max(1, Math.abs(coords.y - state.start.y));
  const isArea = width >= MIN_AREA_PX || height >= MIN_AREA_PX;
  if (!isArea || (width === session.boxWidth && height === session.boxHeight)) return true;

  const layer = engine.getLayer(session.layerId);
  const textData = layerTextData(layer);
  if (!textData) return true;

  engine.transformLayer(session.layerId, { x: left, y: top });
  engine.updateTextData(session.layerId, { ...textData, boxMode: "area", boxWidth: width, boxHeight: height });
  editor.setTextEditSession({ ...session, docX: left, docY: top, boxMode: "area", boxWidth: width, boxHeight: height, isDragging: true });
  const bitmap = engine.getLayerImageBitmap(session.layerId);
  if (bitmap) editor.renderer?.uploadImage(session.layerId, bitmap);
  editor.scheduler.requestRender();
  return true;
}

/**
 * pointerup: release capture; the session stays open for typing (the edit
 * overlay consumes textEditSession). History is committed at session close —
 * one undo step covers the whole create+type edit.
 */
export function applyTextPointer(
  ctx: PointerToolContext,
  e: PointerEvent,
  state: TextPointerState,
): boolean {
  const { editor } = ctx;
  if (editor.activeTool() !== "text" || !state.isDragging) return false;
  const currentSession = editor.textEditSession();
  if (!currentSession) {
    state.reset();
    return true;
  }
  editor.setTextEditSession({ ...currentSession, isDragging: false });
  tryReleasePointerCapture(ctx.getCanvasRef(), e.pointerId);
  state.isDragging = false;
  editor.scheduler.requestRender();

  // Focus is owned by TextEditOverlay: it focuses the textarea via its own ref
  // on session start (prevSessionKey change), so no global DOM query is needed
  // here. The previous `document.querySelector("[data-text-edit-overlay]")` +
  // `setTimeout(10)` was removed — refs are the best-practice way to reach the
  // overlay's textarea.

  return true;
}

/**
 * Session close (Ctrl+Enter / click-away / tool switch). New temp layers with
 * empty content are DELETED with no history entry (empty-commit cleanup);
 * otherwise the pre-gesture snapshot is committed as one undo step. Re-edits
 * that changed nothing are closed WITHOUT a commit (no ghost undo entry).
 */
export function commitTextSession(editor: TextSessionEditor): void {
  const session = editor.textEditSession();
  if (!session) return;
  const engine = editor.workspace.getActiveEngine();
  const history = editor.workspace.getActiveHistory();
  if (!engine || !history) return;

  // Flush any pending overlay content into THIS session's engine so the
  // commit sees the latest keystrokes. The doc-switch path passes a workspace
  // wrapper whose getActiveEngine returns the SOURCE engine — the overlay's
  // own workspace would read the now-active engine (@bug 2026-08-09 B2b).
  flushPendingText(session.layerId, engine);
  // Unhide the edited layer in the session's engine. The overlay's cleanup
  // clears the CURRENT engine's hidden id — on a doc switch that leaves the
  // layer invisible in the source document until some later session touches
  // it (@bug 2026-08-09 B2a).
  if (typeof engine.setRenderHiddenLayerId === "function") {
    engine.setRenderHiddenLayerId(null);
  }

  const layer = engine.getLayer(session.layerId);
  const textData = layerTextData(layer);

  if (session.isNewLayer) {
    // Empty-commit cleanup: a click that never typed anything must not leave
    // an empty layer behind (research R1: no placeholder, empty-commit cleanup).
    if (!textData || textData.content.trim() === "") {
      engine.deleteLayer(session.layerId);
      editor.setTextEditSession(null);
      editor.scheduler.requestRender();
      return;
    }
    // Auto-sync text layer name to typed content if it still has default name
    const trimmed = textData.content.trim();
    if (layer && trimmed.length > 0 && (!layer.name || layer.name === DEFAULT_TEXT_LAYER_NAME || layer.name.startsWith(`${DEFAULT_TEXT_LAYER_NAME} `))) {
      const autoName = trimmed.length > 24 ? `${trimmed.slice(0, 24)}...` : trimmed;
      layer.name = autoName;
    }
    history.commit(session.preSnapshot, "Add Text");
    editor.setTextEditSession(null);
    editor.scheduler.requestRender();
    return;
  }

  // Re-edit of an existing layer: no-op guard — identical textData → no commit.
  const preLayer = session.preSnapshot.layers?.find((l) => l.id === session.layerId);
  const preData = layerTextData(preLayer as LayerNode);
  if (!textData) {
    // The layer was deleted externally while the session was open (e.g. the
    // layer-panel Delete). Close the session with NO commit: the deletion
    // already recorded its own undo step, and committing the stale preSnapshot
    // here would push a ghost "Edit Text" entry whose snapshot re-adds the
    // deleted layer on undo (@bug 2026-08-09 B6).
    editor.setTextEditSession(null);
    return;
  }
  if (preData && textDataEquals(preData, textData)) {
    editor.setTextEditSession(null);
    return;
  }
  history.commit(session.preSnapshot, "Edit Text");
  editor.setTextEditSession(null);
  editor.scheduler.requestRender();
}

/**
 * Cancel (Escape / pointercancel / lost capture): remove a temp (new) layer
 * and close the session with no history entry. Existing-layer edits are just
 * closed (nothing mutated so far).
 */
export function cancelTextSession(editor: TextSessionEditor): void {
  // Defensive: the cancel handlers run for every tool; mocks/tests that lack
  // the text session accessor must not crash (no-op when absent).
  if (typeof editor.textEditSession !== "function") return;
  const session = editor.textEditSession();
  if (!session) return;
  const engine = editor.workspace.getActiveEngine();
  // Flush with the session's engine (same rationale as commitTextSession: on
  // a doc switch the wrapper provides the source engine).
  flushPendingText(session.layerId, engine);
  if (session.isNewLayer && engine) {
    engine.deleteLayer(session.layerId);
  } else if (engine && session.preSnapshot && engine.getLayer(session.layerId)) {
    // Re-edit cancel: typing already live-mutated the layer through the
    // debounced push — roll the engine back to the pre-session snapshot so
    // "cancel" is a true cancel (no ghost mutation, no undo entry). The
    // viewport stays untouched, and the restored bitmap is re-uploaded
    // because the compositor may still hold the post-typing raster texture.
    // Guarded on the layer still existing: if it was deleted externally
    // (layer-panel Delete), restoring the pre-snapshot would RESURRECT the
    // deleted layer (@bug 2026-08-09 B6).
    engine.restore(session.preSnapshot, { restoreViewport: false });
    const bitmap = engine.getLayerImageBitmap(session.layerId);
    if (bitmap && editor.renderer) editor.renderer.uploadImage(session.layerId, bitmap);
  }
  // Unhide in the session's engine (the overlay cleanup only clears the
  // current engine — see commitTextSession @bug B2a).
  if (engine && typeof engine.setRenderHiddenLayerId === "function") {
    engine.setRenderHiddenLayerId(null);
  }
  editor.setTextEditSession(null);
  editor.scheduler.requestRender();
}
