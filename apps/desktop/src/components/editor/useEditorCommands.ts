import { batch, onCleanup, onMount } from "solid-js";
import { registerShortcut } from "./keyboardRegistry";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { isEditableTarget } from "@/lib/dom";
import { isTauriRuntime, runTauriWindowAction } from "@/lib/desktop";
import { WorkspaceManager } from "@/engine/workspace";
import { MAX_OPEN_DOCUMENTS } from "@/engine/types";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { useEditor } from "./shell/EditorContext";
import { useLayerActions } from "./layers/useLayerActions";
import { cancelLayerTransformSession } from "./transformSession";
import { syncTextSessionBase } from "./canvas/pointerTools/textTool";
import { useDialog } from "./dialogs/DialogProvider";
import { showToast } from "./Toast";
import { showSaveDialog, writeFileBytes, showSaveDialogAllFormats, setTrustedPaths, ipcErrorMessage } from "@/tauri/native";
import { serializeAndSaveProject } from "./projectSerialize";
import { addRecentFile } from "@/lib/recentFiles";
import { easeOutCubic } from "@/viewport/easing";
import { encodeComposite, getSavedQuality, setSavedQuality, type ExportFormat } from "./exportDocument";
import { saveProgress, setSaveProgress, cancelPendingSaveDismiss, scheduleSaveDismiss, scheduleSave } from "./saveState";
import { cancelAutosave } from "./autoSave";
import { hasFacadeOwnedLayers } from "@/engine/document";
import {
  commitFacadeClearSelection,
  commitFacadeInvertSelection,
  commitFacadeSelectAll,
  mirrorRestoredSelection,
  mirrorSelectionCommand,
  syncFacadeVersionFromPixel,
} from "@/lib/protocol/facadeRegistry";
import { isNativeAuthority } from "@/lib/protocol/bridge";
import { repushCanonicalDocument } from "@/lib/protocol/canonicalSeed";
import { runFacadeExternalHandoff } from "./facadeHistoryHandoff";
import { selectionUploadRect } from "./canvas/keyboardShortcuts/selectionTool";
import { historyBridgeEnabled, restoreSnapshotBitmapsByToken } from "@/engine/history";
import { observeHistoryCursorParity } from "@/engine/historyCursorParity";
import { bitmapStoreFor } from "@/engine/bitmapStore";
import { projectRustTiles } from "./rustTileProjection";


export const NATIVE_MENU_EVENT = "photrez://native-menu";
export const EDITOR_COMMAND_EVENT = "photrez://editor-command";

export type EditorCommand =
  | "file.new"
  | "file.open"
  | "file.save"
  | "file.save-as"
  | "file.export"
  | "file.print"
  | "edit.undo"
  | "edit.redo"
  | "edit.cut"
  | "edit.copy"
  | "edit.paste"
  | "edit.select-all"
  | "edit.deselect"
  | "edit.invert-selection"
  | "image.resize"
  | "layer.new"
  | "layer.duplicate"
  | "layer.delete"
  | "layer.select-all"
  | "layer.merge-down"
  | "layer.flatten"
  | "layer.stamp-visible"
  | "view.zoom-in"
  | "view.zoom-out"
  | "view.actual-size"
  | "view.fit-canvas"
  | "view.zoom-to-selection"
  | "view.toggle-side-panels"
  | "view.toggle-right-dock-layout"
  | "view.toggle-snap"
  | "view.toggle-snap-layers"
  | "view.toggle-snap-canvas"
  | "window.minimize"
  | "window.toggle-maximize"
  | "window.close"
  | "help.about" | "app.settings";

const EDITOR_COMMANDS: ReadonlySet<string> = new Set<EditorCommand>([
  "file.new",
  "file.open",
  "file.save",
  "file.save-as",
  "file.export",
  "file.print",
  "edit.undo",
  "edit.redo",
  "edit.cut",
  "edit.copy",
  "edit.paste",
  "edit.select-all",
  "edit.deselect",
  "edit.invert-selection",
  "image.resize",
  "layer.new",
  "layer.duplicate",
  "layer.delete",
  "layer.select-all",
  "layer.merge-down",
  "layer.flatten",
  "layer.stamp-visible",
  "view.zoom-in",
  "view.zoom-out",
  "view.actual-size",
  "view.fit-canvas",
  "view.zoom-to-selection",
  "view.toggle-side-panels",
  "view.toggle-right-dock-layout",
  "view.toggle-snap",
  "view.toggle-snap-layers",
  "view.toggle-snap-canvas",
  "window.minimize",
  "window.toggle-maximize",
  "window.close",
  "help.about",
  "app.settings",
]);

// ── Save queue — lives in saveState.ts (shared with the autosave timer) ──

export function isEditorCommand(value: string): value is EditorCommand {
  return EDITOR_COMMANDS.has(value);
}

export function dispatchEditorCommand(command: EditorCommand): void {
  window.dispatchEvent(new CustomEvent<EditorCommand>(EDITOR_COMMAND_EVENT, { detail: command }));
}

/**
 * Monotonic counter for snapshot-history undo/redo dispatches that perform the
 * async bitmap re-attach. Bumped at dispatch start; a stale async re-attach that
 * resolves after a NEWER op has started must not win (it would overwrite the
 * live model with an older snapshot's bitmap). This is the in-flight guard for
 * the `rust_pixels_undo_snapshot`/`rust_pixels_redo_snapshot` re-attach.
 */
let snapshotHistoryOps = 0;

export function useEditorCommands(onToggleSidePanels: () => void) {
  const editor = useEditor();
  const dialog = useDialog();
  const layerActions = useLayerActions();

  const requiresDocument = (command: EditorCommand) => (
    command === "file.save"
    || command === "file.save-as"
    || command === "file.export"
    || command === "file.print"
    || command === "edit.undo"
    || command === "edit.redo"
    || command.startsWith("edit.")
    || command === "image.resize"
    || command.startsWith("layer.")
    || command === "view.zoom-in"
    || command === "view.zoom-out"
    || command === "view.actual-size"
    || command === "view.fit-canvas"
    || command === "view.zoom-to-selection"
  );

  const isEnabled = (command: EditorCommand): boolean => {
    if (command === "file.new") return !editor.workspace.isFull();
    if (requiresDocument(command) && !editor.activeDocumentId()) return false;
    if (command === "edit.undo") {
      // When a transform session exists, undo is always available:
      // mini undo first (revert individual gesture), then cancel-session fallback.
      if (editor.layerTransformSession()) return true;
      // Facade-owned content keeps its history in Rust.
      if (hasFacadeOwnedLayers()) return true;
      return (editor.activeTool() === "crop" && (editor.canCropUndo() || editor.canModernCropUndo()))
        || editor.workspace.getActiveHistory()?.canUndo() === true;
    }
    if (command === "edit.redo") {
      if (editor.layerTransformSession()) return true;
      if (hasFacadeOwnedLayers()) return true;
      return (editor.activeTool() === "crop" && (editor.canCropRedo() || editor.canModernCropRedo()))
        || editor.workspace.getActiveHistory()?.canRedo() === true;
    }
    const engine = editor.workspace.getActiveEngine();
    if (command === "edit.cut" || command === "edit.copy") {
      const activeId = engine?.getActiveLayerId();
      return Boolean(engine?.getSelection() && activeId && engine.getLayerImageBitmap(activeId));
    }
    if (command === "edit.paste") return SelectionOperations.hasClipboard();
    if (command === "edit.deselect") return engine?.getSelection() !== null;
    if (command === "layer.new") return Boolean(engine);
    if (command === "layer.duplicate") return Boolean(engine?.getActiveLayerId());
    if (command === "layer.delete") {
      return Boolean(engine && engine.getActiveLayerId() && engine.getLayers().length > 1);
    }
    if (command === "layer.merge-down") {
      if (!engine) return false;
      const activeId = engine.getActiveLayerId();
      const activeIndex = engine.getLayers().findIndex((layer) => layer.id === activeId);
      return activeIndex >= 0 && activeIndex < engine.getLayers().length - 1;
    }
    if (command === "layer.flatten") return (engine?.getLayers().length ?? 0) > 1;
    return true;
  };

  const uploadActiveLayerBitmap = (dirty?: { x: number; y: number; width: number; height: number }) => {
    const engine = editor.workspace.getActiveEngine();
    const activeId = engine?.getActiveLayerId();
    if (!engine || !activeId) return;
    const layer = engine.getLayer(activeId);
    if (layer?.imageBitmap) {
      if (dirty) editor.renderer.uploadImage(layer.id, layer.imageBitmap, dirty);
      else editor.renderer.uploadImage(layer.id, layer.imageBitmap);
    }
  };

  const cancelActiveTransformSession = (): boolean => {
    const engine = editor.workspace.getActiveEngine();
    if (!cancelLayerTransformSession(editor.layerTransformSession(), engine)) return false;
    editor.setLayerTransformSession(null);
    editor.scheduler.requestRender();
    return true;
  };

  const restoreHistorySnapshot = async (direction: "undo" | "redo") => {
    // Try transform mini undo/redo first when session is active.
    // Each pointerDown for resize/rotate saves a snapshot to the mini undo
    // stack, so Ctrl+Z reverts individual gestures within the session.
    if (editor.layerTransformSession()) {
      const engine = editor.workspace.getActiveEngine();
      if (engine) {
        const session = editor.layerTransformSession()!;
        const layer = engine.getLayer(session.layerId);
        if (layer) {
          if (direction === "undo") {
            const entry = editor.undoTransformWithCurrent(layer.transform);
            if (entry) {
              engine.transformLayer(layer.id, entry.transform);
              editor.scheduler.requestRender();
              editor.workspace.notifyVisualChange();
              return;
            }
          } else {
            const entry = editor.redoTransformWithCurrent(layer.transform);
            if (entry) {
              engine.transformLayer(layer.id, entry.transform);
              editor.scheduler.requestRender();
              editor.workspace.notifyVisualChange();
              return;
            }
          }
        }
      }
    }

    if (cancelActiveTransformSession()) {
      return;
    }

    if (editor.activeTool() === "crop") {
      // Try modern crop undo/redo first (current interaction mode)
      if (editor.cropInteractionMode() === "modern") {
        const state = direction === "undo"
          ? (editor.canModernCropUndo() ? editor.undoModernCrop() : null)
          : (editor.canModernCropRedo() ? editor.redoModernCrop() : null);
        if (state) {
          editor.setModernCropFrame({ ...state.frame });
          editor.setModernCropImageTransform({ ...state.transform });
          return;
        }
      } else {
        // Classic crop undo/redo
        const state = direction === "undo"
          ? (editor.canCropUndo() ? editor.undoLastCrop() : null)
          : (editor.canCropRedo() ? editor.redoCrop() : null);
        if (state) {
          editor.setCropRect(state.rect);
          editor.setCropRotation(state.rotation);
          return;
        }
      }
    }

    // ── Facade (Rust-owned) history first ────────────────────
    // Transforms/addLayers created under photrez.facade=1 have NO TS history
    // entries (the facade gate blocks legacy mutation), so their undo/redo can
    // only come from Rust. Rust Undo on an EMPTY stack is a no-op success, so
    // we detect that via lastHistoryDeltaWasEmpty and fall through to the
    // legacy TS history for pre-facade entries. TRANSITIONAL behavior: while
    // any facade-owned layer exists, the gate blocks engine.restore() of TS
    // entries whose snapshots contain facade layers — such entries stay pinned
    // until facade layers are removed. This is not a final history
    // architecture.
    let handoffFellThrough = false;
    if (hasFacadeOwnedLayers()) {
      // Facade (Rust-owned) history handoff. Returns true when this branch fully
      // handled the step (caller must return); false to fall through to the
      // legacy TS history store. The branch clears the pending-external barrier
      // (the wedge) and, for facade-owned layers, does NOT complete the external
      // step's model restore (engine.restore() throws E_FACADE_OWNED - mixed
      // history constraint, tracked separately). It must not claim the restore
      // finished.
      // No selection mirror needed here: the facade-owned handoff returns before engine.restore and selection is unchanged on both sides (host applyFacadeSnapshot writes layers+dims only; the native walker restores layers+doc_size and selection arms commit no history entry), so host and shadow stay consistent by construction.
      if (await runFacadeExternalHandoff(editor, direction)) return;
      // Handoff attempted but fell through (Rust had no entry; TS does the restore).
      // The native heal re-push below fires only in this case, AFTER engine.restore,
      // so the re-pushed canonical shadow reflects the post-restore state.
      handoffFellThrough = true;
    }

    try {
      const engine = editor.workspace.getActiveEngine();
      const history = editor.workspace.getActiveHistory();
      if (!engine || !history) {
        return;
      }

      const canRestore = direction === "undo" ? history.canUndo() : history.canRedo();
      if (!canRestore) {
        return;
      }
      // ── AUTHORITY CONVERGENCE ──
      // Rust ProtocolEngine is the single logical undo/redo executor.
      // TS history.canUndo()/canRedo() is used as pre-check (TS cursor ==
      // Rust cursor by the cursor invariant). The actual Rust undo/redo is
      // fired by the pop below, once per popped entry, before either restore
      // path runs - not by the tile/metadata split that follows.
      // [perf] Issue C instrumentation: quantify snapshot vs restore cost on
      // large canvases before optimizing.
      const perfT0 = performance.now();
      const snapshot = direction === "undo"
        ? history.undo(engine.snapshot())
        : history.redo(engine.snapshot());
      const perfTHist = performance.now();
      if (!snapshot) {
        return;
      }
      // Hazard #1: whether the popped TS entry is a Snapshot-typed entry. The
      // Rust snapshot cursor only ever steps for Snapshot entries, so a MIXED
      // [External, Snapshot] stream can leave the cursor pointing at a Snapshot
      // while TS restores an External state. Gating the re-attach on this never
      // lets an External step read the wrong-step Snapshot from Rust.
      const isSnapshotEntry =
        typeof history.isLastPoppedSnapshotEntry === "function" &&
        history.isLastPoppedSnapshotEntry();

      // ── Tile path: paint entries carry tile patches ──
      // Pixels are restored via surface patches + per-tile uploads; the model
      // is identical for pure-paint entries, so engine.restore (and its
      // full-texture re-upload) is intentionally skipped.
      // This fast-path early-returns WITHOUT touching model.selection: a paint
      // entry's snapshot carries the same selection the live model already has
      // (a paint op never mutates selection), so there is nothing to mirror to
      // the native shadow here. Selection-changing entries go through the
      // engine.restore path below, which does mirror (mirrorRestoredSelection).
      const patches = direction === "undo"
        ? history.consumeLastUndoPatches()
        : history.consumeLastRedoPatches();
      if (patches) {
        let tiles = direction === "undo" ? patches.before : patches.after;
        // ── Rust is authoritative for every entry it owns ──
        // A Rust-owned entry (rust_pixels_write_region recorded it) is a cursor
        // token, not a second copy of the pixels: this branch always takes the
        // Rust tiles, which drains the twin in lockstep. For an entry Rust does
        // NOT own, photrez.rustPixels still arms the Rust fetch, and the local
        // memento remains the fallback.
        //
        // The cursor STEP is a separate concern and belongs to the pop above
        // (history.undo()/redo()): it fires exactly once per popped entry, for
        // tile and metadata entries alike, chained behind any earlier step so two
        // rapid presses cannot interleave their round-trips. This gate is the
        // FETCH and stays exactly as it was - it decides whether Rust's pixels
        // are this entry's source of truth, which is a different question from
        // whether Rust recorded a step to consume.
        const activeDocId = editor.workspace.getActiveDocumentId() ?? "";
        const rustOwned = patches.rustOwned === true;
        const rustPixelsFlag = (() => {
          try { return localStorage.getItem("photrez.rustPixels") === "1"; } catch { return false; }
        })();
        const projected = await projectRustTiles({
          takeCursorStep: () => history.takeLastCursorStep(),
          getEngine: () => editor.workspace.getActiveEngine(),
          layerId: patches.layerId,
          rustOwned,
          rustPixelsFlag,
          fallbackTiles: tiles,
        });
        const rustRes = projected.step;
        tiles = projected.tiles;
        if (rustOwned && (!rustRes || rustRes.tiles.length === 0)) {
          console.warn("[paint] Rust-owned step returned no tiles — refusing the stale memento", patches.layerId);
        }
        editor.renderer.uploadSurfaceTiles?.(patches.layerId, patches.surfaceWidth, patches.surfaceHeight, tiles);
        // Imperative paint entries replay pixels via tiles above, but the model
        // snapshot (captured pre/post action) also carries non-pixel state a paint
        // op mutates — Fill Layer clears `basicAdjustment` so the adjustment bakes
        // into the fill colour. The full engine.restore(snapshot) is skipped for
        // perf (pure-paint model is otherwise identical), so sync only the
        // specific non-pixel field here. `snapshot` carries the correct model for
        // both directions: undo restores the pre-action value, redo the post-action
        // (cleared) value.
        const snapLayer = snapshot.layers.find((l) => l.id === patches.layerId);
        const liveLayer = snapLayer ? engine.getLayer(patches.layerId) : null;
        if (snapLayer && liveLayer) {
          const a = snapLayer.basicAdjustment;
          const b = liveLayer.basicAdjustment;
          const same = (a === undefined && b === undefined) ||
            (a !== undefined && b !== undefined && a.brightness === b.brightness && a.contrast === b.contrast && a.saturation === b.saturation);
          if (!same) {
            liveLayer.basicAdjustment = a ? { ...a } : undefined;
            engine.notifyVisualChange();
          }
        }
        // Bitmap sync for imperative undo/redo.  The paint-tile
        // fast-path skips engine.restore() for performance, but operations like
        // Fill Layer and Adjustment Bake replace layer.imageBitmap.  On undo,
        // the snapshot carries the pre-operation bitmap reference; on redo, the
        // post-operation bitmap.  Sync it so export/save/eyedropper read the
        // correct pixels.  The bitmap is an immutable ImageBitmap reference —
        // no copy needed.
        if (snapLayer && liveLayer) {
          const snapBitmap = snapLayer.imageBitmap;
          const liveBitmap = liveLayer.imageBitmap;
          if (snapBitmap && snapBitmap !== liveBitmap) {
            liveLayer.imageBitmap = snapBitmap;
          }
          // Bitmap repair: the authoritative tiles just uploaded changed pixels,
          // so `layer.imageBitmap` is now stale - it still holds the pixels from
          // before this step. It must NOT be stamped with the post-step Rust
          // epoch: `ensureBitmapCurrent` (engine/document.ts) short-circuits on
          // a matching epoch, so the stamp would permanently disable the repair
          // and export/save would ship the reverted pixels. The snapshot cannot
          // supply them either - a paint commit writes the post-stroke surface
          // into the model bitmap BEFORE history.commit, so the popped entry
          // holds that same object and the snap/live comparison above is a
          // no-op. Re-derive through the designated repair instead: the surface
          // was just synced to the authoritative tiles above, and the repair
          // falls back to a full Rust canonical read when no surface cache is
          // present. It stamps only an epoch it actually rebuilt for.
          if (rustRes && rustRes.tiles.length > 0) {
            await engine.ensureBitmapCurrent(activeDocId, patches.layerId);
          } else if (tiles.length > 0 && liveLayer.imageBitmap) {
            // No authoritative Rust pixels were applied, so the same staleness
            // applies with no Rust repair available: the epoch probe rejects
            // ("layer not initialized") and ensureBitmapCurrent returns early.
            // The model bitmap is then the only pixel source of truth and the
            // entry's memento tiles are the only record of what this step
            // changed. A paint op writes just those tiles, so restoring their
            // bytes over the current bitmap yields the exact pre-step (undo) /
            // post-step (redo) pixels. No epoch is stamped: with no Rust store
            // there is none to report, and inventing one would make a later
            // ensureBitmapCurrent skip a real repair.
            const canvas = new OffscreenCanvas(liveLayer.width, liveLayer.height);
            const ctx = canvas.getContext("2d");
            if (ctx) {
              ctx.drawImage(liveLayer.imageBitmap as CanvasImageSource, 0, 0);
              for (const t of tiles) {
                ctx.putImageData(new ImageData(new Uint8ClampedArray(t.data), t.width, t.height), t.x, t.y);
              }
              liveLayer.imageBitmap = canvas.transferToImageBitmap();
              // The cached paint surface is derived from the bitmap we just
              // replaced, so it is now drifted; drop it rather than let a later
              // paint op composite onto the stale copy.
              engine.invalidatePaintSurface(patches.layerId);
            }
          }
        }
        const perfDone = performance.now();
        console.info(
          `[perf] ${direction}(tiles): upload=${(perfDone - perfTHist).toFixed(1)}ms total=${(perfDone - perfT0).toFixed(1)}ms tiles=${tiles.length}`,
        );
        // CURSOR INVARIANT: the pop above already stepped the Rust cursor for
        // this entry when Rust recorded one (see CommandHistory.stepRustCursor),
        // for tile and metadata entries alike. Nothing to sync here.
        //
        // The probe reads `rust_pixels_history_tip` over the SAME registry mutex the
        // step holds, so it must not be issued before the step's move lands or it
        // reads the PRE-step cursor and reports a divergence for a step that moved
        // exactly once. The metadata branch below already sequences this way
        // (`await takeLastCursorStep()`); the tile branch has to do it by hand,
        // because `projectRustTiles` only CLAIMS the step when its fetch gate
        // (`rustOwned || photrez.rustPixels`) is open - and a bridge-ON TS-owned
        // tile pop leaves that gate closed while still firing a step. Hence
        // `lastCursorStepSettled()`: it sequences without claiming, so the next
        // pop's leading `take()` still drops the unconsumed step. A step that
        // rejects still settles, so this await cannot fail the undo path.
        await history.lastCursorStepSettled()?.catch(() => {});
        observeHistoryCursorParity(
          editor.workspace.getActiveDocumentId() ?? "",
          history.getUndoCount(),
          direction,
        );
        editor.scheduler.requestRender();
        return;
      }

      // Upload narrow: remember which bitmap object each layer held before the
      // restore. Snapshots reuse the immutable bitmap reference, so a layer
      // whose object is identical afterwards has identical pixels and its
      // texture already matches. Only added or swapped layers re-upload.
      const beforeBitmaps = new Map(engine.getLayers().map((l) => [l.id, l.imageBitmap ?? null]));
      engine.restore(snapshot);
      // Native-authority heal re-push: the handoff fell through to the legacy TS
      // restore, so the native engine was NOT updated by a Rust undo/redo this step.
      // Re-push the FULL canonical document now (AFTER restore) so the native shadow
      // reflects the post-restore state - that timing is the point: firing it inside
      // confirmExternalCursor would carry the PRE-restore snapshot. Barrier-registered
      // (repushCanonicalDocument) so the next facade command's syncFromEngine flushes
      // it before dispatch. Gated by native authority; flag off => no-op.
      if (handoffFellThrough && isNativeAuthority()) {
        const docId = editor.workspace.getActiveDocumentId() ?? "";
        void repushCanonicalDocument(docId, engine).catch((e) =>
          console.warn("[canonical-repush] restore-history heal re-push failed", e),
        );
      }

      // Undo/redo mirror: engine.restore replaced model.selection from the popped
      // snapshot, but the native shadow's selection is only touched by a native
      // selection command, so without this the shadow keeps the pre-undo rect and
      // the next native selectAll/invert reads a stale selection. Covers BOTH
      // directions (this function serves undo and redo). The funnel no-ops when
      // the facade flag is off, so the default path stays byte-identical. Placed
      // here so model.selection is final for this step; the heal re-push above
      // refreshes only the canonical shadow document, not the engine's own active
      // selection.
      mirrorRestoredSelection(engine);

      // ── Snapshot-token re-attach (bridge-ON only, Snapshot-typed entries) ──
      // The Rust snapshot cursor is the single undo/redo authority when the
      // history bridge is on. Ask it for the before/after snapshot and
      // re-attach the layer ImageBitmap by its stable token — the SAME bitmap
      // object, never a detached one. Runs BEFORE the upload loop below so the
      // renderer uploads the token-resolution bitmap. Safe no-op when off.
      // Hazard #1: gated on `isSnapshotEntry` so an External (plain metadata /
      // non-snapshot) undo/redo NEVER reads the Rust snapshot cursor — in a
      // mixed [External, Snapshot] stream the cursor can point at a Snapshot
      // entry while TS restored an External state, and redo_snapshot would
      // return a WRONG-step bitmap. External steps are Model-A authoritative.
      if (historyBridgeEnabled() && isSnapshotEntry) {
        const docId = editor.workspace.getActiveDocumentId() ?? "";
        // Monotonic op counter: register this dispatch by bumping the counter so
        // any async bitmap re-attach that started earlier can detect a newer op
        // and drop itself. `engine.restore(snapshot)` above already applied the
        // synchronous model; this re-attach only swaps the ImageBitmap object to
        // the token-resolved one. A stale re-attach arriving after a newer
        // undo/redo overwrites the new model's bitmap with an old one — guarded.
        const opStart = ++snapshotHistoryOps;
        // Hazard #2: capture the bitmap each live layer held immediately after
        // engine.restore. If a non-undo producer (Fill/bake) replaces a layer's
        // bitmap during the invoke round-trip, the current bitmap will differ
        // from this capture and the re-attach DROPS itself (never overwrites
        // the producer's fresh bitmap with the pre-undo one).
        const expectedBitmaps = new Map<
          string,
          { imageBitmap: ImageBitmap | null; baseImageBitmap: ImageBitmap | null }
        >();
        for (const layer of engine.getLayers()) {
          expectedBitmaps.set(layer.id, {
            imageBitmap: layer.imageBitmap ?? null,
            baseImageBitmap: layer.baseImageBitmap ?? null,
          });
        }
        await restoreSnapshotBitmapsByToken(
          direction,
          docId,
          (token) => bitmapStoreFor(docId).get(token),
          (layerId, bitmap, field = "imageBitmap") => {
            // Stale-op guard: a newer undo/redo started after this dispatch, so
            // the live model is newer. Drop the re-attach (return false) instead
            // of replacing the newer state with this (older) snapshot's bitmap.
            if (snapshotHistoryOps !== opStart) return false;
            const layer = engine.getLayer(layerId);
            if (!layer) return false;
            // Hazard #2 drop-check: if a NEWER producer replaced the layer's
            // bitmap since dispatch, the model is newer than this undo's
            // snapshot — re-attaching would overwrite it. Only proceed when the
            // current bitmap still matches what engine.restore set.
            const expected = expectedBitmaps.get(layerId);
            const current = field === "baseImageBitmap" ? layer.baseImageBitmap : layer.imageBitmap;
            // Finding 3: FAIL-CLOSED. If there is no expected bitmap for this
            // layer (e.g. a layer the restored model did not have), do NOT apply
            // the re-attach — only proceed when we positively verified the model
            // still holds the bitmap this undo/redo set. A missing expected is
            // treated as "not verified", never as "matches".
            if (!expected || current !== expected[field]) return false;
            // Hazard #3: assign to the SAME field the token was registered from
            // (a base-only layer re-attaches to baseImageBitmap, never imageBitmap).
            if (field === "baseImageBitmap") layer.baseImageBitmap = bitmap;
            else layer.imageBitmap = bitmap;
            return true;
          },
          (token) => bitmapStoreFor(docId).getField(token),
          // Finding 2: verify the returned snapshot's layer-id set matches the
          // restored model's layer set before re-attaching (skip on mismatch).
          () => engine.getLayers().map((l) => l.id),
          // Finding-B: verify the returned payload's per-layer epoch/pixelVersion
          // + token against the restored model's stack entry before re-attaching.
          // `snapshot` is the EXACT DocumentModel the stack entry holds (restore
          // clears bitmapEpoch on the ENGINE copy but not on this object), so its
          // per-layer bitmapEpoch (@@ 0) is the ground truth the payload epoch /
          // pixelVersion were derived from at record time. A payload from a DIFFERENT
          // step (future bitmap-mutating producer) fails this gate and is dropped.
          (layerId) => {
            const l = snapshot.layers.find((x) => x.id === layerId);
            if (!l) return null;
            // Size-gate inputs: the CURRENT restored model's dims (live engine
            // layer, falling back to the stack snapshot layer). Read-only -
            // dims stay model-owned, never written by the re-attach.
            const restored = engine.getLayer(layerId);
            return {
              epoch: l.bitmapEpoch ?? 0,
              pixelVersion: l.bitmapEpoch ?? 0,
              imageBitmap: l.imageBitmap ?? null,
              baseImageBitmap: l.baseImageBitmap ?? null,
              width: restored?.width ?? l.width,
              height: restored?.height ?? l.height,
            };
          },
        );
      }

      // An open text session must re-anchor its preSnapshot: the user now
      // sees an OLDER state, so the session's next commit diffs against it.
      syncTextSessionBase({
        workspace: editor.workspace,
        textEditSession: editor.textEditSession,
        setTextEditSession: editor.setTextEditSession,
        scheduler: editor.scheduler,
      });

      const restoredLayer = engine.getLayers()[0];

      // Pixel-layer allowlist: null = unknown, keep the identity-map narrow
      // below; [] = provably metadata-only, skip every upload even when the
      // restored bitmap object differs; [ids] = only those layers are upload
      // candidates (the identity check still applies to each).
      const pixelLayerIds =
        typeof history.getLastPoppedPixelLayerIds === "function"
          ? history.getLastPoppedPixelLayerIds()
          : null;
      for (const layer of engine.getLayers()) {
        if (!layer.imageBitmap) continue;
        if (pixelLayerIds !== null && !pixelLayerIds.includes(layer.id)) continue;
        // Identity-map narrow: an unchanged layer keeps the same bitmap object
        // through the restore, so its texture already matches. A layer with no
        // before entry was re-added and always re-uploads.
        if (beforeBitmaps.get(layer.id) !== layer.imageBitmap) editor.renderer.uploadImage(layer.id, layer.imageBitmap);
      }
      // Notify workspace to trigger UI sync (layers, history panel, adjustments, etc.)
      editor.workspace.notifyVisualChange();
      editor.scheduler.requestRender();
      const perfDone = performance.now();
      console.info(
        `[perf] ${direction}(snapshot): hist=${(perfTHist - perfT0).toFixed(1)}ms restore+upload=${(perfDone - perfTHist).toFixed(1)}ms total=${(perfDone - perfT0).toFixed(1)}ms`,
      );
      // DIAGNOSTIC ONLY: measure whether this host's undo stack sits at the same
      // history position as the Rust cursor, which nothing else compares. The
      // verdict is never branched on - the only output is one console.warn on
      // divergence, and the probe itself is still fired and forgotten. What is
      // awaited here is NOT the probe: it is this entry's cursor step, which the
      // pop above already fired and which must land before the read, or the probe
      // would pair this step's depth with the PREVIOUS step's cursor. That await
      // is the same one IPC round-trip the tile path above already waits on, and
      // a no-op when this pop owned no Rust entry.
      await history.takeLastCursorStep()?.catch(() => {});
      observeHistoryCursorParity(
        editor.workspace.getActiveDocumentId() ?? "",
        history.getUndoCount(),
        direction,
      );
    } catch (error) {
      showToast(`${direction === "undo" ? "Undo" : "Redo"} failed: ${error instanceof Error ? error.message : "unknown error"}`, "error");
    }
  };

  const execute = (command: EditorCommand) => {
    if (!isEnabled(command)) return;

    switch (command) {
      case "file.new": {
        if (editor.workspace.isFull()) {
          showToast(`Workspace full: close a document first (max ${MAX_OPEN_DOCUMENTS})`, "error");
          break;
        }
        void (async () => {
          try {
            const result = await dialog.newDocument();
            if (result) {
              const id = `doc-${crypto.randomUUID()}`;
              editor.workspace.addDocument(
                WorkspaceManager.createBlankDocument(id, result.name, result.width, result.height, { backgroundColor: result.backgroundColor }),
              );
              editor.scheduler.requestRender();
            }
          } catch (err) {
            console.error("file.new dialog failed:", err);
            showToast("Failed to create new document", "error");
          }
        })();
        break;
      }
      case "file.open":
        void editor.openImage();
        break;
      case "file.save": {
        const session = editor.workspace.getActiveSession();
        if (!session) break;

        // New/unsaved → redirect to Save As
        if (!session.sourcePath) {
          execute("file.save-as");
          break;
        }

        const engine = session.engine;
        const ext = session.sourcePath.split(".").pop()?.toLowerCase();
        const layerCount = engine.getLayers().length;

        // Multi-layer on flat image format → redirect to Save As
        if (layerCount > 1 && ext !== "ptz") {
          execute("file.save-as");
          break;
        }

        // Cancel any in-flight autosave so it neither blocks this save nor
        // collides with the SaveWorkerPool (autosave is skipped, not an error).
        cancelAutosave();

        scheduleSave(async () => {
          cancelPendingSaveDismiss();
          try {
            if (ext === "ptz") {
              const ctrl = new AbortController();
              setSaveProgress({ phase: "encoding", label: "Saving project...", fraction: 0, cancel: () => ctrl.abort() });
              const preSaveSnapshot = engine.snapshot();
              const totalLayers = engine.getLayers().length;
              setSaveProgress({ phase: "writing", label: "Writing to disk...", fraction: 0.9, cancel: () => ctrl.abort() });
              await serializeAndSaveProject(engine, session.sourcePath!, {
                signal: ctrl.signal,
                onEncodeProgress: (done, total) => {
                  setSaveProgress({ phase: "encoding", label: `Encoding ${done}/${total} layers`, fraction: total > 0 ? done / total : 0, cancel: () => ctrl.abort() });
                },
              });
              engine.clearDirty(preSaveSnapshot);
              session.dirty = engine.isDirty();
              setSaveProgress({ phase: "done", label: "Saved", fraction: 1 });
            } else {
              const format: ExportFormat = ext === "jpg" || ext === "jpeg" ? "jpeg"
                : ext === "webp" ? "webp" : ext === "tiff" ? "tiff" : "png";
              // Lossy formats: prompt quality only the FIRST time this format is
              // saved; the choice is persisted per-format so later saves of the
              // same format write directly. Cancel aborts the save (anti-accidental
              // guard). PNG is lossless and always saves directly.
              let quality = 92;
              if (format === "jpeg" || format === "webp") {
                const saved = getSavedQuality(format);
                if (saved === null) {
                  const chosen = await dialog.quality({
                    title: `Save ${format.toUpperCase()} Quality`,
                    format,
                    defaultQuality: 92,
                  });
                  if (chosen === null) return; // user cancelled → abort save
                  setSavedQuality(format, chosen);
                  quality = chosen;
                } else {
                  quality = saved;
                }
              }
              setSaveProgress({ phase: "encoding", label: `Saving ${format.toUpperCase()}...`, fraction: 0 });
              const preSaveSnapshot = engine.snapshot();
              const bytes = await encodeComposite(engine, format, quality);
              await writeFileBytes(session.sourcePath!, bytes);
              engine.clearDirty(preSaveSnapshot);
              session.dirty = engine.isDirty();
              setSaveProgress({ phase: "done", label: "Saved", fraction: 1 });
            }
            addRecentFile(session.sourcePath!, session.displayName);
            editor.workspace.notifyVisualChange();
            editor.scheduler.requestRender();
          } catch (err) {
            setSaveProgress({ phase: "error", label: "Save failed", fraction: 0 });
            showToast(`Failed to save: ${ipcErrorMessage(err)}`, "error");
          } finally {
            scheduleSaveDismiss();
          }
        });
        break;
      }
      case "file.save-as": {
        const session = editor.workspace.getActiveSession();
        if (!session) break;

        // Same preemption as file.save — manual save wins over autosave.
        cancelAutosave();

        scheduleSave(async () => {
          cancelPendingSaveDismiss();
          try {
            const engine = session.engine;
            const baseName = session.displayName.replace(/\.[^.]+$/, "");
            const defaultName = `${baseName}.ptz`;

            // Use all-format dialog
            const path = await showSaveDialogAllFormats(defaultName);
            if (!path) return;

            const ext = path.split(".").pop()?.toLowerCase();
            const addRecent = (p: string) => {
              addRecentFile(p, p.split(/[/\\]/).pop() || session.displayName);
            };

            if (ext === "ptz") {
              // Project save — working doc switches to .ptz
              const ctrl = new AbortController();
              setSaveProgress({ phase: "encoding", label: "Saving project...", fraction: 0, cancel: () => ctrl.abort() });
              const preSaveSnapshot = engine.snapshot();
              setSaveProgress({ phase: "writing", label: "Writing to disk...", fraction: 0.9, cancel: () => ctrl.abort() });
              await serializeAndSaveProject(engine, path, {
                signal: ctrl.signal,
                onEncodeProgress: (done, total) => {
                  setSaveProgress({ phase: "encoding", label: `Encoding ${done}/${total} layers`, fraction: total > 0 ? done / total : 0, cancel: () => ctrl.abort() });
                },
              });
              engine.clearDirty(preSaveSnapshot);
              session.dirty = engine.isDirty();
              session.sourcePath = path;
              session.displayName = path.split(/[/\\]/).pop() || session.displayName;
              addRecent(path);
              setSaveProgress({ phase: "done", label: "Saved", fraction: 1 });
            } else {
              // Flat format save
              const format: ExportFormat = ext === "jpg" || ext === "jpeg" ? "jpeg"
                : ext === "webp" ? "webp" : ext === "tiff" ? "tiff" : "png";
              const layerCount = engine.getLayers().length;

              // Warning for multi-layer documents
              if (layerCount > 1) {
                const result = await dialog.confirmWithCheckbox({
                  title: `Save as ${format.toUpperCase()}?`,
                  message: `This will flatten ${layerCount} layers into a single image.\n\nIndividual layers cannot be recovered after closing this document.`,
                  checkboxLabel: "Also save a project backup (.ptz) to preserve layers",
                  checkboxChecked: true,
                  confirmLabel: `Save as ${format.toUpperCase()}`,
                  cancelLabel: "Cancel",
                });
                if (!result.confirmed) return;

                // Save .ptz backup if checkbox was checked
                if (result.checked) {
                  const ctrl = new AbortController();
                  setSaveProgress({ phase: "encoding", label: "Saving project backup...", fraction: 0, cancel: () => ctrl.abort() });
                  const preSaveSnapshot = engine.snapshot();
                  const backupPath = path.replace(/\.[^.]+$/, ".ptz");
                  // The sibling .ptz is a new path — approve it for Rust file-IO.
                  await setTrustedPaths([backupPath]);
                  setSaveProgress({ phase: "writing", label: "Writing backup to disk...", fraction: 0.9, cancel: () => ctrl.abort() });
                  await serializeAndSaveProject(engine, backupPath, {
                    signal: ctrl.signal,
                    onEncodeProgress: (done, total) => {
                      setSaveProgress({ phase: "encoding", label: `Backup encoding ${done}/${total} layers`, fraction: total > 0 ? done / total : 0, cancel: () => ctrl.abort() });
                    },
                  });
                  engine.clearDirty(preSaveSnapshot);
                  session.dirty = engine.isDirty();
                  addRecent(backupPath);
                }
              }

              // Quality dialog for JPEG/WebP (PNG uses lossless default)
              let quality = 100;
              if (format === "jpeg" || format === "webp") {
                const chosen = await dialog.quality({
                  title: `Save as ${format.toUpperCase()} Quality`,
                  format,
                  defaultQuality: 92,
                });
                if (chosen === null) return; // user cancelled quality dialog
                quality = chosen;
                setSavedQuality(format, chosen); // remember last-used for quick saves
              }
              setSaveProgress({ phase: "encoding", label: `Saving ${format.toUpperCase()}...`, fraction: 0 });
              const preSaveSnapshot = engine.snapshot();
              const bytes = await encodeComposite(engine, format, quality);
              await writeFileBytes(path, bytes);
              engine.clearDirty(preSaveSnapshot);
              session.dirty = engine.isDirty();

              // Working doc switches to flat format
              session.sourcePath = path;
              session.displayName = path.split(/[/\\]/).pop() || session.displayName;
              addRecent(path);
              setSaveProgress({ phase: "done", label: "Saved", fraction: 1 });
            }
            editor.workspace.notifyVisualChange();
            editor.scheduler.requestRender();
          } catch (err) {
            setSaveProgress({ phase: "error", label: "Save failed", fraction: 0 });
            showToast(`Failed to save: ${ipcErrorMessage(err)}`, "error");
          } finally {
            scheduleSaveDismiss();
          }
        });
        break;
      }
      case "file.export":
        if (editor.activeDocumentId()) editor.setShowExportDialog(true);
        break;
      case "file.print":
        if (editor.activeDocumentId()) editor.setShowPrintDialog(true);
        break;
      case "edit.undo":
        restoreHistorySnapshot("undo");
        break;
      case "edit.redo":
        restoreHistorySnapshot("redo");
        break;
      case "edit.cut": {
        const engine = editor.workspace.getActiveEngine();
        const history = editor.workspace.getActiveHistory();
        if (!engine?.getSelection() || !history) break;
        // The cut is recorded by Rust as one canonical pixel write, so the
        // history entry (and its rustOwned cursor token) is committed inside the
        // operation. Read the rect first: the mutation below clears the
        // selection it maps from.
        const dirty = selectionUploadRect(engine);
        SelectionOperations.cutSelection(engine, history, editor.renderer);
        uploadActiveLayerBitmap(dirty ?? undefined);
        editor.scheduler.requestRender();
        break;
      }
      case "edit.copy": {
        const engine = editor.workspace.getActiveEngine();
        if (engine) SelectionOperations.copySelection(engine);
        break;
      }
      case "edit.paste": {
        const engine = editor.workspace.getActiveEngine();
        const history = editor.workspace.getActiveHistory();
        if (!engine || !history) break;
        history.commit(engine.snapshot(), "Paste");
        SelectionOperations.pasteSelection(engine);
        uploadActiveLayerBitmap();
        editor.scheduler.requestRender();
        break;
      }
      case "edit.select-all": {
        const engine = editor.workspace.getActiveEngine();
        engine?.selectAll();
        if (engine) mirrorSelectionCommand(engine, () => commitFacadeSelectAll(engine as never));
        editor.scheduler.requestRender();
        break;
      }
      case "edit.deselect": {
        const engine = editor.workspace.getActiveEngine();
        engine?.clearSelection();
        if (engine) mirrorSelectionCommand(engine, () => commitFacadeClearSelection(engine as never));
        editor.setSelectionEditMode(false);
        if (editor.activeTool() === "move") {
          editor.setSelectedLayerIds([]);
          editor.setSelectedLayerId(null);
          editor.workspace.getActiveEngine()?.setActiveLayer(null);
        }
        editor.scheduler.requestRender();
        break;
      }
      case "edit.invert-selection": {
        const engine = editor.workspace.getActiveEngine();
        engine?.invertSelection();
        if (engine) mirrorSelectionCommand(engine, () => commitFacadeInvertSelection(engine as never));
        editor.setSelectionEditMode(false);
        editor.scheduler.requestRender();
        break;
      }
      case "image.resize":
        if (editor.activeDocumentId()) editor.setShowResizeDialog(true);
        break;
      case "layer.new":
        layerActions.handleAddLayer();
        break;
      case "layer.duplicate":
        layerActions.handleDuplicateActiveLayer();
        break;
      case "layer.delete":
        layerActions.handleDeleteActiveLayer();
        break;
      case "layer.select-all": {
        const engine = editor.workspace.getActiveEngine();
        if (engine) {
          const nonBg = engine.getLayers().filter((l) => !l.isBackground).map((l) => l.id);
          if (nonBg.length > 0) {
            editor.setSelectedLayerIds(nonBg);
            if (nonBg[0]) engine.setActiveLayer(nonBg[0]);
            editor.scheduler.requestRender();
            editor.workspace.notifyVisualChange();
          }
        }
        break;
      }
      case "layer.merge-down":
        layerActions.handleMergeActiveLayerDown();
        break;
      case "layer.flatten":
        layerActions.handleFlattenAllLayers();
        break;
      case "layer.stamp-visible":
        layerActions.handleStampVisible();
        break;
      case "view.zoom-in":
      case "view.zoom-out":
      case "view.actual-size": {
        const viewport = editor.camera.getViewportSize();
        const currentZoom = editor.camera.getState().zoom;
        const factor = command === "view.zoom-in"
          ? 1.25
          : command === "view.zoom-out"
            ? 0.8
            : 1 / currentZoom;
        // Animated zoom for keyboard shortcuts (150ms - snappy and smooth)
        editor.camera.animateZoomToPoint(
          factor,
          viewport.width / 2,
          viewport.height / 2,
          150,
          easeOutCubic
        );
        // Note: syncFromCamera() and scheduler.requestRender() are handled by camera animation callbacks
        break;
      }
      case "view.fit-canvas": {
        const engine = editor.workspace.getActiveEngine();
        if (!engine) break;
        engine.fitToScreen(editor.viewportWidth(), editor.viewportHeight());
        const vp = engine.getViewport();
        // Directly update camera + signals to bypass lastVp cache in syncViewport.
        // After panning, the engine viewport may already equal the fit values from
        // initial load, causing syncViewport to bail early (lastVp comparison).
        editor.camera.setState({ x: vp.panX, y: vp.panY, zoom: vp.zoom });
        batch(() => {
          editor.setZoom(vp.zoom);
          editor.setPan({ x: vp.panX, y: vp.panY });
        });
        // Center modern crop frame at new viewport center in document coordinates
        // + reset offset so the image isn't shifted by stale offsetX/Y
        if (editor.cropInteractionMode() === "modern") {
          const z = vp.zoom;
          const p = { x: vp.panX, y: vp.panY };
          editor.setModernCropFrame((prev) => {
            if (!prev) return null;
            const docCenterX = (editor.viewportWidth() / 2 - p.x) / z;
            const docCenterY = (editor.viewportHeight() / 2 - p.y) / z;
            return {
              ...prev,
              x: Math.round(docCenterX - prev.w / 2),
              y: Math.round(docCenterY - prev.h / 2),
            };
          });
          editor.setModernCropImageTransform((prev) => ({ ...prev, offsetX: 0, offsetY: 0 }));
        }
        editor.scheduler.requestRender();
        break;
      }
      case "view.zoom-to-selection": {
        const engine = editor.workspace.getActiveEngine();
        if (!engine) break;
        engine.zoomToSelection(editor.viewportWidth(), editor.viewportHeight());
        const vp = engine.getViewport();
        editor.camera.setState({ x: vp.panX, y: vp.panY, zoom: vp.zoom });
        batch(() => {
          editor.setZoom(vp.zoom);
          editor.setPan({ x: vp.panX, y: vp.panY });
        });
        editor.scheduler.requestRender();
        break;
      }
      case "view.toggle-side-panels":
        onToggleSidePanels();
        break;
      case "view.toggle-right-dock-layout":
        editor.setRightDockLayout(editor.rightDockLayout() === "side-by-side" ? "stacked" : "side-by-side");
        break;
      case "view.toggle-snap":
        editor.setMoveSnapEnabled((prev) => !prev);
        break;
      case "view.toggle-snap-layers":
        editor.setSnapToLayersEnabled((prev) => !prev);
        break;
      case "view.toggle-snap-canvas":
        editor.setSnapToCanvasEnabled((prev) => !prev);
        break;
      case "window.minimize":
        void runTauriWindowAction("minimize");
        break;
      case "window.toggle-maximize":
        void runTauriWindowAction("toggleMaximize");
        break;
      case "window.close":
        void runTauriWindowAction("close");
        break;
      case "help.about":
        void dialog.about();
        break;
      case "app.settings":
        void dialog.settings();
        break;
    }
  };

  onMount(() => {
    // ── Register keyboard shortcuts (conflict detection) ──
    registerShortcut("Ctrl+Z", "useEditorCommands");
    registerShortcut("Ctrl+Shift+Z", "useEditorCommands");
    registerShortcut("Ctrl+Y", "useEditorCommands");
    registerShortcut("Ctrl+N", "useEditorCommands");
    registerShortcut("Ctrl+O", "useEditorCommands");
    registerShortcut("Ctrl+Shift+S", "useEditorCommands");
    registerShortcut("Ctrl+S", "useEditorCommands");
    registerShortcut("Ctrl+Alt+E", "useEditorCommands");
    registerShortcut("Ctrl+P", "useEditorCommands");
    registerShortcut("Ctrl+1", "useEditorCommands");
    registerShortcut("Ctrl+Alt+0", "useEditorCommands");
    registerShortcut("Ctrl+=", "useEditorCommands");
    registerShortcut("Ctrl+-", "useEditorCommands");

    const handleKeyDown = (event: KeyboardEvent) => {
      // Ctrl+P must never fall through to the WebView2 default browser print.
      // Prevent default unconditionally — even while a modal is open or an
      // input is focused — so the app print dialog is the only print surface.
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "p") {
        event.preventDefault();
        if (!document.querySelector('[aria-modal="true"]')) execute("file.print");
        return;
      }

      if (document.querySelector('[aria-modal="true"]')) return;

      if (
        event.defaultPrevented
        || isEditableTarget(event.target)
        || isEditableTarget(document.activeElement)
      ) return;

      const commandKey = event.ctrlKey || event.metaKey;
      const key = event.key.toLowerCase();
      let command: EditorCommand | null = null;

      if (commandKey && event.shiftKey && key === "z") command = "edit.redo";
      else if (commandKey && key === "z") command = "edit.undo";
      else if (commandKey && key === "y") command = "edit.redo";
      else if (commandKey && key === "n") command = "file.new";
      else if (commandKey && key === "o") command = "file.open";
      else if (commandKey && event.shiftKey && key === "s") command = "file.save-as";
      else if (commandKey && key === "s") command = "file.save";
      else if (commandKey && event.altKey && key === "e") command = "file.export";
      else if (commandKey && key === "1") command = "view.actual-size";
      else if (commandKey && event.altKey && key === "0") command = "view.zoom-to-selection";
      else if (commandKey && (key === "=" || key === "+")) command = "view.zoom-in";
      else if (commandKey && (key === "-" || key === "_")) command = "view.zoom-out";

      if (command) {
        event.preventDefault();
        execute(command);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    const handleDispatchedCommand = (event: Event) => {
      const command = (event as CustomEvent<unknown>).detail;
      if (typeof command === "string" && isEditorCommand(command)) execute(command);
    };
    window.addEventListener(EDITOR_COMMAND_EVENT, handleDispatchedCommand);

    let disposed = false;
    let unlisten: (() => void) | undefined;
    if (isTauriRuntime()) {
      // Use async IIFE + try/catch instead of .catch() on the chained promise
      // to prevent unhandledrejection on the original listen() promise.
      // The .then().catch() chain only catches on the chained promise,
      // not on the original Promise returned by listen().
      void (async () => {
        try {
          const disposeListener = await listen<string>(NATIVE_MENU_EVENT, (event) => {
            if (isEditorCommand(event.payload)) execute(event.payload);
          });
          if (disposed) disposeListener();
          else unlisten = disposeListener;
        } catch (error: unknown) {
          console.warn("Failed to register native menu listener:", error);
        }
      })();
    }

    onCleanup(() => {
      disposed = true;
      unlisten?.();
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener(EDITOR_COMMAND_EVENT, handleDispatchedCommand);
    });
  });

  return {
    execute,
    isEnabled,
    undo: () => execute("edit.undo"),
    redo: () => execute("edit.redo"),
  };
}
