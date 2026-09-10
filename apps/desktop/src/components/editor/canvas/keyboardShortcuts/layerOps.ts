import type { DocumentEngine } from "@/engine/document";
import { isFacadeOwnedLayer } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";
import { flattenAllLayers, mergeActiveLayerDown, stampVisibleLayers, mergeSelectedLayers, duplicateMultipleLayers } from "../../layers/layerOperations";
import { commitFacadeOpacity, commitFacadeReorder, isFacadeEnabled } from "@/lib/protocol/facadeRegistry";
import { showToast } from "../../Toast";
import type { KeyboardShortcutContext } from "./context";

/**
 * Layer operations: stamp visible, merge/flatten, duplicate, new layer,
 * reorder (incl. to top/bottom), flip, delete layer, layer opacity (0-9).
 * Also blocks destructive layer ops during an active transform session.
 *
 * `key`/`ctrl` are computed once by the caller (shared with tool shortcuts).
 */
// NOTE: routing is intentionally SYNCHRONOUS. The async facade/opacity work it
// triggers (handleAddLayer / handleDeleteActiveLayer / commitFacadeOpacity) runs
// fire-and-forget and completes on a microtask - the same best-effort, non-blocking
// pattern the history shim uses. Keeping the routing decision synchronous means the
// keyboard handler behaves byte-identically to before the async facade migration:
// tool switches and layer-op routing stay immediate, and event consumption is decided
// before any microtask yield. Tests that assert the *result* of the async work await it.
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

  // Reorder via the Reorder command arm when the active layer is facade-owned
  // (fire-and-forget, mirroring commitFacadeOpacity). The destination index is
  // the legacy toIndex (post-removal insertion index); the command arm uses the
  // same indexing so it passes through unchanged. Falls through to the
  // byte-identical legacy TS path (history commit BEFORE mutation) on non-facade /
  // empty status.
  const routeReorder = (activeId: string, idx: number, toIdx: number): void => {
    if (toIdx < 0 || toIdx >= engine.getLayers().length) return;
    if (isFacadeEnabled() && isFacadeOwnedLayer(activeId)) {
      void commitFacadeReorder(engine as never, activeId, toIdx)
        .then((res) => {
          if (res.status === "applied" || res.status === "noop" || res.status === "empty") {
            scheduler.requestRender();
            editor.workspace.notifyVisualChange();
          } else {
            history.commit(engine.snapshot(), "Reorder Layer");
            engine.reorderLayer(idx, toIdx);
            scheduler.requestRender();
          }
        })
        .catch((err) => showToast(`Cannot reorder layer: ${(err as Error).message}`, "error"));
      return;
    }
    history.commit(engine.snapshot(), "Reorder Layer");
    engine.reorderLayer(idx, toIdx);
    scheduler.requestRender();
  };

  // Block destructive layer operations during an active transform session:
  // they would commit to global history, but Ctrl+Z during a transform only
  // reaches the session's local undo stack, making them un-undoable.
  // (Flip / Ctrl+G is intentionally allowed - it mutates the transform and
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

  // Select All Layers: Ctrl+Alt+A
  if (ctrl && e.altKey && (key === "a" || e.code === "KeyA")) {
    e.preventDefault();
    e.stopPropagation();
    const nonBg = engine.getLayers().filter((l) => !l.isBackground).map((l) => l.id);
    if (nonBg.length > 0) {
      editor.setSelectedLayerIds(nonBg);
      if (nonBg[0]) engine.setActiveLayer(nonBg[0]);
      scheduler.requestRender();
      editor.workspace.notifyVisualChange();
    }
    return true;
  }

  // Free Transform: Ctrl+T - switch to Move tool and enable transform handles
  if (ctrl && !e.shiftKey && !e.altKey && (key === "t" || e.code === "KeyT")) {
    e.preventDefault();
    e.stopPropagation();
    editor.setActiveTool("move");
    editor.setShowTransformControls(true);
    scheduler.requestRender();
    return true;
  }

  // Stamp Visible: Ctrl+Shift+Alt+E - composite all visible layers into a new top layer
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
    const multiIds = editor.selectedLayerIds ? editor.selectedLayerIds() : [];

    if (e.shiftKey) {
      if (flattenAllLayers(engine, history, renderer)) {
        scheduler.requestRender();
      } else {
        showToast("Could not flatten layers", "warn");
      }
    } else if (multiIds.length > 1) {
      if (mergeSelectedLayers(engine, history, renderer, multiIds)) {
        const nextActive = engine.getActiveLayerId();
        editor.setSelectedLayerId(nextActive);
        scheduler.requestRender();
      } else {
        showToast("Could not merge layers", "warn");
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
    const multiIds = editor.selectedLayerIds ? editor.selectedLayerIds() : [];

    if (multiIds.length > 1) {
      const created = duplicateMultipleLayers(engine, history, renderer, multiIds);
      if (created.length > 0) {
        editor.setSelectedLayerIds(created);
        scheduler.requestRender();
      }
      return true;
    }

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
  // Routes through the migrated add funnel (handleAddLayer), which mirrors the
  // panel/menu path: flag ON -> seedFacadeFromEngine -> facade.addLayer ->
  // applyFacadeSnapshot; flag OFF -> byte-identical legacy TS addLayer.
  if (ctrl && e.shiftKey && key === "n") {
    e.preventDefault();
    e.stopPropagation();
    ctx.layerActions.handleAddLayer();
    return true;
  }

  // Layer: Ctrl+] - Move active layer up in stack (towards top, index 0)
  if (ctrl && !e.shiftKey && e.key === "]") {
    e.preventDefault();
    e.stopPropagation();
    const activeId = engine.getActiveLayerId();
    if (activeId) {
      const idx = engine.getLayers().findIndex((l) => l.id === activeId);
      if (idx > 0) routeReorder(activeId, idx, idx - 1);
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
      if (idx >= 0 && idx < stack.length - 1) routeReorder(activeId, idx, idx + 1);
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
      if (idx > 0) routeReorder(activeId, idx, 0);
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
      if (idx >= 0 && idx < stack.length - 1) routeReorder(activeId, idx, stack.length - 1);
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

  // Layer: Delete / Backspace - Delete active layer(s)
  // Routes through the shared delete funnel (handleDeleteActiveLayer), which
  // mirrors the panel/menu Delete path: flag ON + facade-owned -> facade delete
  // (EditorClient) projected into the engine; flag OFF / non-owned -> the same
  // legacy TS delete the panel uses. (Selection-tool delete is handled earlier.)
  //
  // NOTE - routing the keyboard through this funnel is NOT byte-identical to the
  // old keyboard-only delete on the flag-OFF shipped path. The four deltas vs
  // the previous keyboard handler are deliberate consistency improvements that
  // align keyboard DELETE with the panel Delete:
  //   (a) history entry now uses recordSnapshotHistory (snapshotType "snapshot")
  //       instead of history.commit - equivalent TS undo/redo; under the history
  //       bridge gate the Rust record switches from record_external to
  //       record_snapshot (the default-OFF path is unaffected).
  //   (b) one extra engine.snapshot() per delete (before + after capture).
  //   (c) guard order: deleting the Background in a 1-layer doc now shows the
  //       "Cannot delete the Background layer" toast where the old keyboard path
  //       was silent - this makes keyboard DELETE consistent with PANEL delete.
  //   (d) active-layer source is now the activeLayerId() signal (single source of
  //       truth) rather than engine.getActiveLayerId().
  //   (e) the funnel calls cancelActiveTransformSession() first on Delete
  //       (reachable from keyboard since Delete is not in the transform-guard
  //       block list) - this matches the panel Delete path.
  //   (f) the text-session cancel now runs before delete (single + multi). The
  //       old keyboard path could delete a layer mid-text-session and leave a
  //       dangling session that Escape could resurrect; the funnel cancels it
  //       first, matching the panel Delete path.
  if (e.key === "Delete" || e.key === "Backspace") {
    e.preventDefault();
    e.stopPropagation();
    ctx.layerActions.handleDeleteActiveLayer();
    return true;
  }

  // Layer: 0-9 (no modifier) - Set active layer opacity
  // 0 = 100%, 1 = 10%, 2 = 20%, ..., 9 = 90%
  // Routes through the opacity funnel (commitFacadeOpacity): facade-owned layers
  // (flag ON) commit via one SetOpacity command + authoritative projection;
  // otherwise falls through to the byte-identical legacy TS path below.
  if (!ctrl && !e.shiftKey && !e.altKey && e.key.length === 1 && e.key >= "0" && e.key <= "9") {
    const activeId = engine.getActiveLayerId();
    if (activeId) {
      const layer = engine.getLayer(activeId);
      if (layer && !layer.locked) {
        e.preventDefault();
        e.stopPropagation();
        const digit = e.key.charCodeAt(0) - 48;
        const opacity = digit === 0 ? 1.0 : digit / 10;
        if (layer.opacity === opacity) return true; // no-op guard (matches legacy)
        // Single [activeId]: route is facade (all owned) or legacy (none owned).
        // mixed-rejected is unreachable for one id, so it is not handled here.
        // Fire-and-forget: routing returns true synchronously; the projection lands
        // on a microtask (invisible in production, awaited by tests).
        void commitFacadeOpacity(engine as never, [activeId], opacity)
          .then((res) => {
            if (res.status === "applied" || res.status === "noop") {
              scheduler.requestRender();
              editor.workspace.notifyVisualChange();
            } else {
              // status === "legacy" / "empty": byte-identical legacy TS path.
              history.commit(engine.snapshot(), "Layer Opacity");
              engine.setLayerOpacity(activeId, opacity);
              scheduler.requestRender();
            }
          })
          .catch((err) => {
            // A facade-owned opacity commit can throw (Rust Err / version conflict /
            // facade-not-ready). Match the add/delete funnel error handling: surface a
            // toast and bail instead of leaving an unhandled rejection.
            const msg = (err as Error).message;
            if (msg.includes("E_VERSION_MISMATCH")) {
              showToast("Version conflict - retrying", "warn");
            } else {
              showToast(`Cannot set opacity: ${msg}`, "error");
            }
          });
        return true;
      }
    }
  }

  return false;
}
