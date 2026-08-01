// SPDX-License-Identifier: AGPL-3.0-or-later
import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";
import { fillActiveLayerWithColor } from "../../layers/layerOperations";
import { showToast } from "../../Toast";
import type { KeyboardShortcutContext } from "./context";

/**
 * Alt+Delete/Alt+Backspace → fill active layer with foreground color.
 * Ctrl+Delete/Ctrl+Backspace → fill active layer with background color.
 * F2 → rename active layer.
 * Plain Delete/Backspace falls through to delete-layer / delete-selection
 * behavior handled later (selection tool / layer ops).
 */
export function handleLayerFillKey(
  ctx: KeyboardShortcutContext,
  e: KeyboardEvent,
  engine: DocumentEngine,
  history: CommandHistory,
): boolean {
  const { editor } = ctx;
  const { renderer, scheduler, layerTransformSession, activeTool, fgColor, bgColor } = editor;

  if (e.key === "Delete" || e.key === "Backspace") {
    // Block during active transform session: fill/delete commit to global
    // history, but Ctrl+Z during a transform session only reaches the
    // session's local undo stack, making the action un-undoable.
    if (layerTransformSession()) {
      e.preventDefault();
      e.stopPropagation();
      return true;
    }
    // Same rationale for the crop tool: the crop mini-undo stack owns
    // Ctrl+Z while a crop rect is live, so a destructive fill/delete here
    // would not be reachable from the crop undo path.
    if (activeTool() === "crop") {
      e.preventDefault();
      e.stopPropagation();
      return true;
    }
    const resolveColor = (c: string | (() => string)) =>
      typeof c === "function" ? c() : c;
    if (e.altKey) {
      e.preventDefault();
      e.stopPropagation();
      if (!engine.getActiveLayerId()) {
        showToast("No editable layer selected", "warn");
      } else if (fillActiveLayerWithColor(engine, history, renderer, resolveColor(fgColor))) {
        scheduler.requestRender();
      } else {
        showToast("Could not fill layer", "warn");
      }
      return true;
    }
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      e.stopPropagation();
      if (!engine.getActiveLayerId()) {
        showToast("No editable layer selected", "warn");
      } else if (fillActiveLayerWithColor(engine, history, renderer, resolveColor(bgColor))) {
        scheduler.requestRender();
      } else {
        showToast("Could not fill layer", "warn");
      }
      return true;
    }
  }

  // F2: Rename active layer
  if (e.key === "F2") {
    e.preventDefault();
    e.stopPropagation();
    const activeId = engine.getActiveLayerId();
    if (activeId) {
      const layer = engine.getLayer(activeId);
      editor.setRenamingLayerId(activeId);
      editor.setRenameLayerName(layer?.name ?? "");
    }
    return true;
  }

  return false;
}
