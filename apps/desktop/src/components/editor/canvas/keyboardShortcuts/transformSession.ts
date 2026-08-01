// SPDX-License-Identifier: AGPL-3.0-or-later
import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";
import { cancelLayerTransformSession, commitLayerTransformSession } from "../../transformSession";
import type { KeyboardShortcutContext } from "./context";

/**
 * Layer transform session keyboard shortcuts (takes precedence over
 * crop/tool shortcuts). Ctrl+Z/Y = transform mini undo/redo within the
 * active session; Enter = commit; Escape = cancel.
 */
export function handleTransformSessionKey(
  ctx: KeyboardShortcutContext,
  e: KeyboardEvent,
  engine: DocumentEngine,
  history: CommandHistory,
): boolean {
  const { editor } = ctx;
  const { layerTransformSession, undoTransformWithCurrent, redoTransformWithCurrent, scheduler } = editor;

  if (layerTransformSession()) {
    const ctrl = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();

    // Ctrl+Z / Ctrl+Y: transform mini undo/redo within the active session.
    // Each pointerDown for a resize/rotate gesture saves a snapshot to the
    // mini undo stack, so these keys revert/restore individual gestures.
    if (ctrl && key === "z" && !e.shiftKey) {
      const session = layerTransformSession();
      const layer = engine.getLayer(session!.layerId);
      if (layer) {
        const entry = undoTransformWithCurrent(layer.transform);
        if (entry) {
          e.preventDefault();
          e.stopPropagation();
          engine.transformLayer(layer.id, entry.transform);
          scheduler.requestRender();
          return true;
        }
        // Mini undo stack empty — fall through so useEditorCommands.ts
        // can cancel the session (existing Ctrl+Z behavior).
      }
    }

    if ((ctrl && key === "y") || (ctrl && e.shiftKey && key === "z")) {
      const session = layerTransformSession();
      const layer = engine.getLayer(session!.layerId);
      if (layer) {
        const entry = redoTransformWithCurrent(layer.transform);
        if (entry) {
          e.preventDefault();
          e.stopPropagation();
          engine.transformLayer(layer.id, entry.transform);
          scheduler.requestRender();
          return true;
        }
        // Mini redo stack empty — fall through to general redo handler.
      }
    }

    if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      if (commitLayerTransformSession(layerTransformSession(), engine, history)) {
        editor.clearTransformStacks();
        editor.setLayerTransformSession(null);
        scheduler.requestRender();
      }
      return true;
    }

    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      if (cancelLayerTransformSession(layerTransformSession(), engine)) {
        editor.clearTransformStacks();
        editor.setLayerTransformSession(null);
        scheduler.requestRender();
      }
      return true;
    }
  }

  return false;
}
