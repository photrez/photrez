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
  | "window.minimize"
  | "window.toggle-maximize"
  | "window.close"
  | "help.about";

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
  "window.minimize",
  "window.toggle-maximize",
  "window.close",
  "help.about",
]);

// ── Save queue — lives in saveState.ts (shared with the autosave timer) ──

export function isEditorCommand(value: string): value is EditorCommand {
  return EDITOR_COMMANDS.has(value);
}

export function dispatchEditorCommand(command: EditorCommand): void {
  window.dispatchEvent(new CustomEvent<EditorCommand>(EDITOR_COMMAND_EVENT, { detail: command }));
}

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
      return (editor.activeTool() === "crop" && (editor.canCropUndo() || editor.canModernCropUndo()))
        || editor.workspace.getActiveHistory()?.canUndo() === true;
    }
    if (command === "edit.redo") {
      if (editor.layerTransformSession()) return true;
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

  const uploadActiveLayerBitmap = () => {
    const engine = editor.workspace.getActiveEngine();
    const activeId = engine?.getActiveLayerId();
    if (!engine || !activeId) return;
    const layer = engine.getLayer(activeId);
    if (layer?.imageBitmap) editor.renderer.uploadImage(layer.id, layer.imageBitmap);
  };

  const cancelActiveTransformSession = (): boolean => {
    const engine = editor.workspace.getActiveEngine();
    if (!cancelLayerTransformSession(editor.layerTransformSession(), engine)) return false;
    editor.setLayerTransformSession(null);
    editor.scheduler.requestRender();
    return true;
  };

  const restoreHistorySnapshot = (direction: "undo" | "redo") => {
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
      const snapshot = direction === "undo"
        ? history.undo(engine.snapshot())
        : history.redo(engine.snapshot());
      if (!snapshot) {
        return;
      }


      engine.restore(snapshot);

      // An open text session must re-anchor its preSnapshot: the user now
      // sees an OLDER state, so the session's next commit diffs against it.
      syncTextSessionBase({
        workspace: editor.workspace,
        textEditSession: editor.textEditSession,
        setTextEditSession: editor.setTextEditSession,
        scheduler: editor.scheduler,
      });

      const restoredLayer = engine.getLayers()[0];

      for (const layer of engine.getLayers()) {
        if (layer.imageBitmap) editor.renderer.uploadImage(layer.id, layer.imageBitmap);
      }
      // Notify workspace to trigger UI sync (layers, history panel, adjustments, etc.)
      editor.workspace.notifyVisualChange();
      editor.scheduler.requestRender();
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
                : ext === "webp" ? "webp" : "png";
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
                : ext === "webp" ? "webp" : "png";
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
        history.commit(engine.snapshot(), "Cut");
        SelectionOperations.cutSelection(engine);
        uploadActiveLayerBitmap();
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
      case "edit.select-all":
        editor.workspace.getActiveEngine()?.selectAll();
        editor.scheduler.requestRender();
        break;
      case "edit.deselect":
        editor.workspace.getActiveEngine()?.clearSelection();
        editor.setSelectionEditMode(false);
        editor.scheduler.requestRender();
        break;
      case "edit.invert-selection":
        editor.workspace.getActiveEngine()?.invertSelection();
        editor.setSelectionEditMode(false);
        editor.scheduler.requestRender();
        break;
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
