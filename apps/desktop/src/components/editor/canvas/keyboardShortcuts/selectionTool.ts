// SPDX-License-Identifier: AGPL-3.0-or-later
import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import {
  commitFacadeClearSelection,
  commitFacadeInvertSelection,
  mirrorSelectionCommand,
} from "@/lib/protocol/facadeRegistry";
import type { KeyboardShortcutContext } from "./context";

/**
 * Layer-local pixel rect covered by the live selection, clamped to the
 * active layer bounds. Inverted selections change the whole layer, so there
 * is no sub-rect to report (null = full upload). Call BEFORE the mutating
 * op: cut/delete clear the selection state the mapping reads.
 * Rounding matches the pixel writer in SelectionOperations, which clears
 * [round(x), round(x) + round(w)): rounding the far edge instead would
 * shrink the rect by a pixel on fractional bounds and leave stale pixels.
 */
export function selectionUploadRect(
  engine: DocumentEngine,
): { x: number; y: number; width: number; height: number } | null {
  const sel = engine.getSelection();
  if (!sel || sel.inverted) return null;
  const activeId = engine.getActiveLayerId();
  if (!activeId) return null;
  const layer = engine.getLayer(activeId);
  if (!layer) return null;
  const aabb = SelectionOperations.selectionToLayerAabb(sel, layer.transform, layer.width, layer.height);
  const sx = Math.round(aabb.x);
  const sy = Math.round(aabb.y);
  const w = Math.round(aabb.width);
  const h = Math.round(aabb.height);
  const x = Math.max(0, Math.min(layer.width, sx));
  const y = Math.max(0, Math.min(layer.height, sy));
  const width = Math.max(0, Math.min(layer.width, sx + w) - x);
  const height = Math.max(0, Math.min(layer.height, sy + h) - y);
  if (width === 0 || height === 0) return null;
  return { x, y, width, height };
}

/**
 * Selection tool keyboard shortcuts: Ctrl+D deselect, Ctrl+I invert,
 * Ctrl+T toggle transform/edit mode, Escape cancel, Ctrl+X cut,
 * Ctrl+C copy, Ctrl+V paste, Delete/Backspace delete selection pixels.
 */
export function handleSelectionToolKey(
  ctx: KeyboardShortcutContext,
  e: KeyboardEvent,
  engine: DocumentEngine,
  history: CommandHistory,
): boolean {
  const { editor, options } = ctx;
  const { scheduler, renderer, setSelectionEditMode, selectionEditMode } = editor;

  if (editor.activeTool() !== "selection") return false;

  // Ctrl+D: Deselect
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "d") {
    e.preventDefault();
    e.stopPropagation();
    engine.clearSelection();
    mirrorSelectionCommand(engine, () => commitFacadeClearSelection(engine as never));
    setSelectionEditMode(false);
    options.onSelectionChange?.();
    scheduler.requestRender();
    return true;
  }

  // Ctrl+I: Invert selection
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "i") {
    e.preventDefault();
    e.stopPropagation();
    engine.invertSelection();
    mirrorSelectionCommand(engine, () => commitFacadeInvertSelection(engine as never));
    setSelectionEditMode(false);
    options.onSelectionChange?.();
    scheduler.requestRender();
    return true;
  }

  // Ctrl+T: Toggle transform/edit mode (show resize/rotate handles)
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "t") {
    e.preventDefault();
    e.stopPropagation();
    if (engine.getSelection()) {
      setSelectionEditMode(!selectionEditMode());
      scheduler.requestRender();
    }
    return true;
  }

  // Escape: Cancel drawing / deselect
  if (e.key === "Escape") {
    e.preventDefault();
    engine.clearSelection();
    mirrorSelectionCommand(engine, () => commitFacadeClearSelection(engine as never));
    setSelectionEditMode(false);
    options.onSelectionChange?.();
    scheduler.requestRender();
    return true;
  }

  // Ctrl+X: Cut selection
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "x") {
    e.preventDefault();
    e.stopPropagation();
    if (engine.getSelection()) {
      // Commit pre-action snapshot so the cut is undoable AND redoable.
      // Without this, the post-cut state was never pushed to the undo
      // stack and redo had no entry to replay.
      history.commit(engine.snapshot(), "Cut");
      const dirty = selectionUploadRect(engine);
      SelectionOperations.cutSelection(engine);
      // Re-upload the modified layer's bitmap to the renderer so the
      // canvas reflects the cut immediately (otherwise the GPU texture
      // still holds the pre-cut pixels until the next texture refresh).
      const activeId = engine.getActiveLayerId();
      if (activeId) {
        const layer = engine.getLayer(activeId);
        if (layer?.imageBitmap) {
          if (dirty) renderer.uploadImage(layer.id, layer.imageBitmap, dirty);
          else renderer.uploadImage(layer.id, layer.imageBitmap);
        }
      }
      options.onSelectionChange?.();
      scheduler.requestRender();
    }
    return true;
  }

  // Ctrl+C: Copy selection
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c") {
    e.preventDefault();
    e.stopPropagation();
    if (engine.getSelection()) {
      SelectionOperations.copySelection(engine);
      scheduler.requestRender();
    }
    return true;
  }

  // Ctrl+V: Paste selection
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "v") {
    e.preventDefault();
    e.stopPropagation();
    // Commit pre-action snapshot so the new layer is undoable/redoable.
    history.commit(engine.snapshot(), "Paste");
    SelectionOperations.pasteSelection(engine);
    // Re-upload the newly-pasted layer's bitmap to the renderer. After
    // pasteSelection, engine.getActiveLayerId() points at the new
    // "Pasted Layer" (addLayer sets activeLayerId to the new layer).
    const pastedId = engine.getActiveLayerId();
    if (pastedId) {
      const pastedLayer = engine.getLayer(pastedId);
      if (pastedLayer?.imageBitmap) {
        renderer.uploadImage(pastedLayer.id, pastedLayer.imageBitmap);
      }
    }
    options.onSelectionChange?.();
    scheduler.requestRender();
    return true;
  }

  // Delete / Backspace: Delete selection pixels
  if (e.key === "Delete" || e.key === "Backspace") {
    e.preventDefault();
    e.stopPropagation();
    const sel = engine.getSelection();
    if (sel) {
      // Commit pre-action snapshot so the deletion is undoable/redoable.
      history.commit(engine.snapshot(), "Delete Pixels");
      const dirty = selectionUploadRect(engine);
      SelectionOperations.deleteSelection(engine);
      // Re-upload the modified layer's bitmap to the renderer so the
      // canvas reflects the deletion immediately.
      const activeId = engine.getActiveLayerId();
      if (activeId) {
        const layer = engine.getLayer(activeId);
        if (layer?.imageBitmap) {
          if (dirty) renderer.uploadImage(layer.id, layer.imageBitmap, dirty);
          else renderer.uploadImage(layer.id, layer.imageBitmap);
        }
      }
      options.onSelectionChange?.();
      scheduler.requestRender();
    }
    return true;
  }

  return false;
}
