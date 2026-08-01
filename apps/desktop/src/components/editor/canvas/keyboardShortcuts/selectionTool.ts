// SPDX-License-Identifier: AGPL-3.0-or-later
import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import type { KeyboardShortcutContext } from "./context";

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
      SelectionOperations.cutSelection(engine);
      // Re-upload the modified layer's bitmap to the renderer so the
      // canvas reflects the cut immediately (otherwise the GPU texture
      // still holds the pre-cut pixels until the next texture refresh).
      const activeId = engine.getActiveLayerId();
      if (activeId) {
        const layer = engine.getLayer(activeId);
        if (layer?.imageBitmap) {
          renderer.uploadImage(layer.id, layer.imageBitmap);
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
      SelectionOperations.deleteSelection(engine);
      // Re-upload the modified layer's bitmap to the renderer so the
      // canvas reflects the deletion immediately.
      const activeId = engine.getActiveLayerId();
      if (activeId) {
        const layer = engine.getLayer(activeId);
        if (layer?.imageBitmap) {
          renderer.uploadImage(layer.id, layer.imageBitmap);
        }
      }
      options.onSelectionChange?.();
      scheduler.requestRender();
    }
    return true;
  }

  return false;
}
