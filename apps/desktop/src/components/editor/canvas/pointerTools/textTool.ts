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

let pendingTextFlush: PendingTextFlush | null = null;

/** The overlay registers its pending-content flush while a session is open. */
export function setPendingTextFlush(fn: PendingTextFlush | null): void {
  pendingTextFlush = fn;
}

/**
 * Flush any pending overlay content so commits see the latest text. When
 * `engine` is provided (the session's engine, resolved by the caller) the
 * overlay pushes into THAT engine — required on a doc switch, where the
 * overlay's own workspace would read the wrong (newly active) engine and
 * silently drop the last debounce-window characters (@bug 2026-08-09 B2b).
 * `null` means "no override available" and falls back to the active engine.
 */
export function flushPendingText(engine?: DocumentEngine | null): void {
  if (pendingTextFlush) {
    const fn = pendingTextFlush;
    pendingTextFlush = null;
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
  if (!engine.getLayer(session.layerId)) return;
  editor.setTextEditSession({ ...session, preSnapshot: engine.snapshot() });
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
    lineHeight: 1.2,
    letterSpacing: 0,
    boxMode: "point",
    boxWidth: 0,
    boxHeight: 0,
    stroke: {
      // Guarded: optional editor accessors keep this tool usable from harnesses
      // (and older call sites) that predate the stroke feature.
      width: typeof editor.textStrokeWidth === "function" ? editor.textStrokeWidth() : 0,
      color: typeof editor.textStrokeColor === "function" ? editor.textStrokeColor() : "#000000",
    },
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
    // Clicking on the SAME text layer that is already in edit session: keep session active!
    if (hit && hit.id === activeSession.layerId) {
      trySetPointerCapture(ctx.getCanvasRef(), e.pointerId);
      return true;
    }
    // Clicking away: commit any pending session before starting a new interaction.
    // Empty new layers are removed by commitTextSession's cleanup.
    commitTextSession(ctx.editor);
  }
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
  flushPendingText(engine);
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
  flushPendingText(engine);
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
