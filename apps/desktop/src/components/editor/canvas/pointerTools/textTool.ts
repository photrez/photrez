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
import type { TextData } from "@/engine/textTypes";
import type { TextEditSession } from "../../tools/editorState";

/** Drag beyond this many doc px switches point text into area-box mode. */
const MIN_AREA_PX = 3;

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
>;

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
let pendingTextFlush: (() => void) | null = null;

/** The overlay registers its pending-content flush while a session is open. */
export function setPendingTextFlush(fn: (() => void) | null): void {
  pendingTextFlush = fn;
}

/** Flush any pending overlay content so commits see the latest text. */
export function flushPendingText(): void {
  if (pendingTextFlush) {
    const fn = pendingTextFlush;
    pendingTextFlush = null;
    fn();
  }
}

function buildSessionTextData(editor: EditorAccessors): TextData {
  return {
    content: "",
    fontFamily: editor.textFontFamily(),
    fontSize: editor.textFontSize(),
    fontWeight: editor.textFontWeight(),
    fontStyle: editor.textFontItalic() ? "italic" : "normal",
    color: editor.fgColor(),
    align: editor.textAlign(),
    lineHeight: 1.4,
    letterSpacing: 0,
    boxMode: "point",
    boxWidth: 0,
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

  // Persist any pending session before starting a new interaction (click-away
  // commit). Empty new layers are removed by commitTextSession's cleanup.
  if (editor.textEditSession()) {
    commitTextSession(ctx.editor);
  }

  // Hit test (alpha-aware, same sampler as move tool) — existing text layer
  // starts a re-edit session instead of creating a duplicate on top.
  const allLayers = [...engine.getLayers()];
  const hit = hitTestLayers(coords, allLayers as LayerInfo[], (id, x, y) => engine.sampleLayerAlpha(id, x, y));
  if (hit) {
    const hitLayer = engine.getLayer(hit.id);
    const hitData = layerTextData(hitLayer);
    if (hitLayer && hitData) {
      const pre = engine.snapshot();
      openSession(editor, {
        layerId: hitLayer.id,
        docX: coords.x,
        docY: coords.y,
        boxMode: hitData.boxMode,
        boxWidth: hitData.boxWidth,
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
    const layer = engine.addTextLayer("Text", buildSessionTextData(editor));
    engine.transformLayer(layer.id, { x: coords.x, y: coords.y });
    const bitmap = engine.getLayerImageBitmap(layer.id);
    if (bitmap) editor.renderer?.uploadImage(layer.id, bitmap);
    openSession(editor, {
      layerId: layer.id,
      docX: coords.x,
      docY: coords.y,
      boxMode: "point",
      boxWidth: 0,
      isNewLayer: true,
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
 * (boxMode "area", boxWidth = |dx|), re-rasterizing live. Point text stays put.
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
  const width = Math.max(1, Math.abs(coords.x - state.start.x));
  const isArea = Math.abs(coords.x - state.start.x) >= MIN_AREA_PX;
  if (!isArea || width === session.boxWidth) return true;

  const layer = engine.getLayer(session.layerId);
  const textData = layerTextData(layer);
  if (!textData) return true;

  engine.updateTextData(session.layerId, { ...textData, boxMode: "area", boxWidth: width });
  editor.setTextEditSession({ ...session, boxMode: "area", boxWidth: width });
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
  if (!editor.textEditSession()) {
    state.reset();
    return true;
  }
  tryReleasePointerCapture(ctx.getCanvasRef(), e.pointerId);
  state.isDragging = false;
  editor.scheduler.requestRender();
  return true;
}

/**
 * Session close (Ctrl+Enter / click-away / tool switch). New temp layers with
 * empty content are DELETED with no history entry (empty-commit cleanup);
 * otherwise the pre-gesture snapshot is committed as one undo step. Re-edits
 * that changed nothing are closed WITHOUT a commit (no ghost undo entry).
 */
export function commitTextSession(editor: TextSessionEditor): void {
  // External commits (click-away, tool switch) must see the latest typed
  // content, not the debounced-stale layer state.
  flushPendingText();
  const session = editor.textEditSession();
  if (!session) return;
  const engine = editor.workspace.getActiveEngine();
  const history = editor.workspace.getActiveHistory();
  if (!engine || !history) return;

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
    history.commit(session.preSnapshot, "Add Text");
    editor.setTextEditSession(null);
    editor.scheduler.requestRender();
    return;
  }

  // Re-edit of an existing layer: no-op guard — identical textData → no commit.
  const preLayer = session.preSnapshot.layers?.find((l) => l.id === session.layerId);
  const preData = layerTextData(preLayer as LayerNode);
  if (preData && textData && textDataEquals(preData, textData)) {
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
  flushPendingText();
  const session = editor.textEditSession();
  if (!session) return;
  const engine = editor.workspace.getActiveEngine();
  if (session.isNewLayer && engine) engine.deleteLayer(session.layerId);
  editor.setTextEditSession(null);
  editor.scheduler.requestRender();
}
