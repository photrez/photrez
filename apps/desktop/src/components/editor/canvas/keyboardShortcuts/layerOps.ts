// SPDX-License-Identifier: AGPL-3.0-or-later
import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";
import { flattenAllLayers, mergeActiveLayerDown, stampVisibleLayers } from "../../layers/layerOperations";
import { showToast } from "../../Toast";
import type { KeyboardShortcutContext } from "./context";

/**
 * Layer operations: stamp visible, merge/flatten, duplicate, new layer,
 * reorder (incl. to top/bottom), flip, delete layer, layer opacity (0-9).
 * Also blocks destructive layer ops during an active transform session.
 *
 * `key`/`ctrl` are computed once by the caller (shared with tool shortcuts).
 */
export function handleLayerOpsKey(
  ctx: KeyboardShortcutContext,
  e: KeyboardEvent,
  engine: DocumentEngine,
  history: CommandHistory,
  key: string,
  ctrl: boolean,
): boolean {
  const { editor } = ctx;
  const { renderer, scheduler, layerTransformSession } = editor;

  // Block destructive layer operations during an active transform session:
  // they would commit to global history, but Ctrl+Z during a transform only
  // reaches the session's local undo stack, making them un-undoable.
  // (Flip / Ctrl+G is intentionally allowed — it mutates the transform and
  // is captured by the session's mini undo stack.)
  if (layerTransformSession()) {
    if (
      (ctrl && e.shiftKey && e.altKey && key === "e") ||  // Stamp Visible
      (ctrl && key === "e") ||                            // Merge / Flatten
      (ctrl && key === "j") ||                            // Duplicate layer
      (ctrl && e.shiftKey && key === "n") ||              // New layer
      (ctrl && (e.key === "]" || e.key === "["))          // Reorder (incl. shift)
    ) {
      e.preventDefault();
      e.stopPropagation();
      return true;
    }
  }

  // Stamp Visible: Ctrl+Shift+Alt+E — composite all visible layers into a new top layer
  if (ctrl && e.shiftKey && e.altKey && key === "e") {
    e.preventDefault();
    e.stopPropagation();
    if (!engine.getActiveLayerId()) {
      showToast("No layer selected", "warn");
    } else if (stampVisibleLayers(engine, history, renderer)) {
      scheduler.requestRender();
    } else {
      showToast("Nothing to stamp", "warn");
    }
    return true;
  }

  if (ctrl && key === "e") {
    e.preventDefault();
    e.stopPropagation();

    const activeId = engine.getActiveLayerId();
    if (e.shiftKey) {
      if (flattenAllLayers(engine, history, renderer)) {
        scheduler.requestRender();
      } else {
        showToast("Could not flatten layers", "warn");
      }
    } else if (activeId) {
      if (mergeActiveLayerDown(engine, history, renderer, activeId)) {
        scheduler.requestRender();
      } else {
        showToast("Could not merge layers", "warn");
      }
    } else {
      showToast("No layer selected", "warn");
    }

    return true;
  }

  if (ctrl && key === "j") {
    e.preventDefault();
    e.stopPropagation();
    const activeId = engine.getActiveLayerId();
    if (activeId) {
      history.commit(engine.snapshot(), "Duplicate Layer");
      try {
        const dup = engine.duplicateLayer(activeId);
        if (dup.imageBitmap) {
          renderer.uploadImage(dup.id, dup.imageBitmap);
        }
        scheduler.requestRender();
      } catch (err) {
        showToast(`Cannot duplicate layer: ${(err as Error).message}`, "error");
      }
    }
    return true;
  }

  // Layer: Ctrl+Shift+N - Add new layer
  if (ctrl && e.shiftKey && key === "n") {
    e.preventDefault();
    e.stopPropagation();
    history.commit(engine.snapshot(), "New Layer");
    try {
      engine.addLayer(`Layer ${engine.getLayers().length + 1}`);
      scheduler.requestRender();
    } catch (err) {
      showToast(`Cannot add layer: ${(err as Error).message}`, "error");
    }
    return true;
  }

  // Layer: Ctrl+] - Move active layer up in stack (towards top, index 0)
  if (ctrl && !e.shiftKey && e.key === "]") {
    e.preventDefault();
    e.stopPropagation();
    const activeId = engine.getActiveLayerId();
    if (activeId) {
      const idx = engine.getLayers().findIndex((l) => l.id === activeId);
      if (idx > 0) {
        history.commit(engine.snapshot(), "Reorder Layer");
        engine.reorderLayer(idx, idx - 1);
        scheduler.requestRender();
      }
    }
    return true;
  }

  // Layer: Ctrl+[ - Move active layer down in stack (towards bottom)
  if (ctrl && !e.shiftKey && e.key === "[") {
    e.preventDefault();
    e.stopPropagation();
    const activeId = engine.getActiveLayerId();
    if (activeId) {
      const stack = engine.getLayers();
      const idx = stack.findIndex((l) => l.id === activeId);
      if (idx >= 0 && idx < stack.length - 1) {
        history.commit(engine.snapshot(), "Reorder Layer");
        engine.reorderLayer(idx, idx + 1);
        scheduler.requestRender();
      }
    }
    return true;
  }

  // Layer: Ctrl+Shift+] - Move active layer to top of stack
  if (ctrl && e.shiftKey && e.key === "]") {
    e.preventDefault();
    e.stopPropagation();
    const activeId = engine.getActiveLayerId();
    if (activeId) {
      const idx = engine.getLayers().findIndex((l) => l.id === activeId);
      if (idx > 0) {
        history.commit(engine.snapshot(), "Reorder Layer");
        engine.reorderLayer(idx, 0);
        scheduler.requestRender();
      }
    }
    return true;
  }

  // Layer: Ctrl+Shift+[ - Move active layer to bottom of stack
  if (ctrl && e.shiftKey && e.key === "[") {
    e.preventDefault();
    e.stopPropagation();
    const activeId = engine.getActiveLayerId();
    if (activeId) {
      const stack = engine.getLayers();
      const idx = stack.findIndex((l) => l.id === activeId);
      if (idx >= 0 && idx < stack.length - 1) {
        history.commit(engine.snapshot(), "Reorder Layer");
        engine.reorderLayer(idx, stack.length - 1);
        scheduler.requestRender();
      }
    }
    return true;
  }

  // Layer: Ctrl+G - Flip horizontal, Ctrl+Shift+G - Flip vertical
  if (ctrl && key === "g") {
    e.preventDefault();
    e.stopPropagation();
    const activeId = engine.getActiveLayerId();
    if (!activeId) {
      showToast("No layer selected", "warn");
      return true;
    }
    if (activeId) {
      const layer = engine.getLayer(activeId);
      if (layer && !layer.locked) {
        history.commit(engine.snapshot(), "Flip Layer");
        engine.flipLayer(activeId, e.shiftKey ? "v" : "h");
        scheduler.requestRender();
      }
    }
    return true;
  }

  // Layer: Delete / Backspace - Delete active layer
  // (Selection tool handles this earlier when in selection mode.)
  if (e.key === "Delete" || e.key === "Backspace") {
    e.preventDefault();
    e.stopPropagation();
    const activeId = engine.getActiveLayerId();
    if (!activeId) {
      showToast("No layer selected", "warn");
      return true;
    }
    if (activeId && engine.getLayers().length > 1) {
      history.commit(engine.snapshot(), "Delete Layer");
      engine.deleteLayer(activeId);
      scheduler.requestRender();
    }
    return true;
  }

  // Layer: 0-9 (no modifier) - Set active layer opacity
  // 0 = 100%, 1 = 10%, 2 = 20%, ..., 9 = 90%
  if (!ctrl && !e.shiftKey && !e.altKey && e.key.length === 1 && e.key >= "0" && e.key <= "9") {
    const activeId = engine.getActiveLayerId();
    if (activeId) {
      const layer = engine.getLayer(activeId);
      if (layer && !layer.locked) {
        e.preventDefault();
        e.stopPropagation();
        const digit = e.key.charCodeAt(0) - 48;
        const opacity = digit === 0 ? 1.0 : digit / 10;
        if (layer.opacity === opacity) return true;
        history.commit(engine.snapshot(), "Layer Opacity");
        engine.setLayerOpacity(activeId, opacity);
        scheduler.requestRender();
        return true;
      }
    }
  }

  return false;
}
