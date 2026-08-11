import { useContext, onMount, onCleanup, createEffect, createSignal, batch, JSX } from "solid-js";
import { WorkspaceManager } from "@/engine/workspace";
import { WebGL2Backend } from "@/renderer/webgl2";
import { RenderScheduler } from "@/renderer/scheduler";
import { createEditorState } from "../tools/editorState";
import type { ToolId } from "../tools/toolTypes";
import { createCropState } from "../cropState";
import { createModernCropState } from "../modernCropState";
import { setupWorkspaceSync } from "../canvas/workspaceSync";
import { openImage, openSingleFile, loadProjectFile } from "../editorOpenImage";
import { runStartupOpenChain } from "./startupOpenChain";
import { listAutosaves, clearAllAutosaves, setAutosaveStatus, createAutosaveTimerDebouncer } from "../autoSave";
import { scheduleSave } from "../saveState";
import { ask } from "@tauri-apps/plugin-dialog";
import { ViewportCamera } from "../../../viewport/viewportCamera";
import { DragControllerProvider } from "../DragController";
import { showToast as showToastImpl } from "../Toast";
import { runToolSwitchCleanup } from "../tools/toolLifecycle";
import { DialogProvider } from "../dialogs/DialogProvider";
import { cancelLayerTransformSession, commitLayerTransformSession } from "../transformSession";
import { commitTextSession, syncTextSessionBase } from "../canvas/pointerTools/textTool";
import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import {
  EditorCoreContext,
  EditorCoreProvider,
  useEditorCore,
  type EditorCoreValue,
} from "./contexts/EditorCoreContext";
import {
  ToolSettingsContext,
  ToolSettingsProvider,
  useToolSettings,
  type ToolSettingsValue,
} from "./contexts/ToolSettingsContext";
import {
  DocumentStateContext,
  DocumentStateProvider,
  useDocumentState,
  type DocumentStateValue,
} from "./contexts/DocumentStateContext";
import {
  DialogChromeContext,
  DialogChromeProvider,
  useDialogChrome,
  type DialogChromeValue,
} from "./contexts/DialogChromeContext";
import {
  HistoryDockContext,
  HistoryDockProvider,
  useHistoryDock,
  type HistoryDockValue,
} from "./contexts/HistoryDockContext";

/**
 * Combined editor context facade (review #19: god context split into 5 domain
 * contexts). The facade keeps the legacy single-surface API for existing
 * consumers; new code can use the domain hooks (useEditorCore, useToolSettings,
 * useDocumentState, useDialogChrome, useHistoryDock) directly.
 */
export type EditorContextValue =
  EditorCoreValue & ToolSettingsValue & DocumentStateValue & DialogChromeValue & HistoryDockValue;

export function useEditor(): EditorContextValue {
  const core = useContext(EditorCoreContext);
  const tool = useContext(ToolSettingsContext);
  const document = useContext(DocumentStateContext);
  const dialog = useContext(DialogChromeContext);
  const dock = useContext(HistoryDockContext);
  if (!core || !tool || !document || !dialog || !dock) {
    throw new Error("useEditor must be used within an EditorProvider");
  }
  return { ...core, ...tool, ...document, ...dialog, ...dock };
}

interface EditorDebugEnv {
  DEV?: boolean;
  MODE?: string;
  VITE_PHOTREZ_DEBUG_EDITOR?: string;
}

export function shouldExposeEditorDebugHandle(env: EditorDebugEnv = import.meta.env): boolean {
  return env.DEV === true || env.MODE === "test" || env.VITE_PHOTREZ_DEBUG_EDITOR === "1";
}

export function EditorProvider(props: {
  workspace: WorkspaceManager;
  renderer: WebGL2Backend;
  scheduler: RenderScheduler;
  camera?: ViewportCamera;
  rightDockOpen?: () => boolean;
  setRightDockOpen?: (open: boolean) => void;
  // Feature flag signal can be owned by the caller (EditorShell creates it so
  // the RenderScheduler callback closure can read it) and passed in; when
  // absent the provider owns a fresh signal.
  useGPUCameraForModernCrop?: () => boolean;
  setUseGPUCameraForModernCrop?: (next: boolean | ((prev: boolean) => boolean)) => void;
  children: JSX.Element;
}) {
  const camera = props.camera || new ViewportCamera();
  const editorState = createEditorState();
  const [gpuCameraSignal, setGpuCameraSignal] = createSignal(true);
  const useGPUCameraForModernCrop = props.useGPUCameraForModernCrop ?? gpuCameraSignal;
  const setUseGPUCameraForModernCrop = props.setUseGPUCameraForModernCrop ?? setGpuCameraSignal;
  const cropState = createCropState();
  const modernCropState = createModernCropState();

  const [historyItems, setHistoryItems] = createSignal<import("@/engine/history").HistoryItem[]>([]);
  const [activeHistoryIndex, setActiveHistoryIndex] = createSignal(0);
  const [rightDockPanel, setRightDockPanel] = createSignal<"layers" | "history">("layers");

  type RightDockLayout = "side-by-side" | "stacked";
  function readRightDockLayout(): RightDockLayout {
    if (typeof localStorage === "undefined") return "side-by-side";
    const stored = localStorage.getItem("photrez.rightDockLayout");
    if (stored === "side-by-side" || stored === "stacked") return stored;
    return "side-by-side";
  }

  const [rightDockLayoutState, setRightDockLayoutState] = createSignal<RightDockLayout>(readRightDockLayout());
  const setRightDockLayout = (layout: RightDockLayout) => {
    setRightDockLayoutState(layout);
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("photrez.rightDockLayout", layout);
    }
  };

  const [inspectorTab, setInspectorTab] = createSignal<"library" | "adjust" | "presets">("adjust");
  const [adjustSubTab, setAdjustSubTab] = createSignal<"properties" | "adjustments">("properties");

  const [localRightDockOpen, setLocalRightDockOpen] = createSignal(true);
  const rightDockOpen = props.rightDockOpen || localRightDockOpen;
  const setRightDockOpen = props.setRightDockOpen || setLocalRightDockOpen;

  const navigateHistory = (index: number) => {
    const engine = props.workspace.getActiveEngine();
    const history = props.workspace.getActiveHistory();
    if (!engine || !history) return;

    const activeIndex = history.getUndoCount();
    const lastIndex = activeIndex + history.getRedoCount();
    if (!Number.isInteger(index) || index < 0 || index > lastIndex) return;
    const diff = index - activeIndex;

    if (diff === 0) return;

    if (
      editorState.layerTransformSession()
      && cancelLayerTransformSession(editorState.layerTransformSession(), engine)
    ) {
      editorState.setLayerTransformSession(null);
    }

    const previousLayerIds = new Set(engine.getLayers().map((layer) => layer.id));
    let currentSnapshot = engine.snapshot();
    const steps = Math.abs(diff);
    for (let step = 0; step < steps; step++) {
      const nextSnapshot = diff < 0
        ? history.undo(currentSnapshot)
        : history.redo(currentSnapshot);
      if (!nextSnapshot) break;
      currentSnapshot = nextSnapshot;
    }

    engine.restore(currentSnapshot);

    const restoredLayerIds = new Set(engine.getLayers().map((layer) => layer.id));
    for (const layerId of previousLayerIds) {
      if (!restoredLayerIds.has(layerId)) props.renderer.destroyTexture(layerId);
    }
    for (const layer of engine.getLayers()) {
      if (layer.imageBitmap) props.renderer.uploadImage(layer.id, layer.imageBitmap);
    }
    syncTextSessionBase({
      workspace: props.workspace,
      textEditSession: editorState.textEditSession,
      setTextEditSession: editorState.setTextEditSession,
      scheduler: props.scheduler,
    });
    props.scheduler.requestRender();
    props.workspace.notifyVisualChange();
  };

  const setViewportState = (next: { x: number; y: number; zoom: number }) => {
    camera.setState(next);
    batch(() => {
      editorState.setZoom(next.zoom);
      editorState.setPan({ x: next.x, y: next.y });
    });
    const engine = props.workspace.getActiveEngine();
    if (engine) {
      engine.setViewport({
        panX: next.x,
        panY: next.y,
        zoom: next.zoom,
      });
    }
  };

  const syncFromCamera = () => {
    // Don't sync to engine during animation - it triggers workspace sync which calls camera.setState() and cancels the animation
    if (camera.isAnimating()) {
      // Only update UI signals during animation
      const state = camera.getState();
      batch(() => {
        editorState.setZoom(state.zoom);
        editorState.setPan({ x: state.x, y: state.y });
      });
      return;
    }

    // Full sync when not animating
    const state = camera.getState();
    batch(() => {
      editorState.setZoom(state.zoom);
      editorState.setPan({ x: state.x, y: state.y });
    });
    const engine = props.workspace.getActiveEngine();
    if (engine) {
      engine.setViewport({
        panX: state.x,
        panY: state.y,
        zoom: state.zoom,
      });
    }
  };

  const { syncState, syncViewport } = setupWorkspaceSync({
    workspace: props.workspace,
    camera,
    setDocuments: editorState.setDocuments,
    setActiveDocumentId: editorState.setActiveDocumentId,
    setLayers: editorState.setLayers,
    setActiveLayerId: editorState.setActiveLayerId,
    setSelectedLayerId: editorState.setSelectedLayerId,
    setSelection: editorState.setSelection,
    setSelectionEditMode: editorState.setSelectionEditMode,
    setDocWidth: editorState.setDocWidth,
    setDocHeight: editorState.setDocHeight,
    setZoom: editorState.setZoom,
    setPan: editorState.setPan,
    scheduler: props.scheduler,
    setHistoryItems,
    setActiveHistoryIndex,
  });

  const handleOpenImage = () => openImage({
    workspace: props.workspace,
    renderer: props.renderer,
    scheduler: props.scheduler,
    onError: (message) => showToastImpl(message, "error"),
    onLoading: (message) => editorState.setLoadingMessage(message),
  });

  onMount(() => {
    try {
      syncState();
    } catch (e) {
      console.error("Workspace sync failed during bootstrap:", e);
    }

    // -- Startup open chain: CLI file first, then crash recovery --
    // Serialized (review #35) so the recovery dialog can never race the CLI
    // open; each step is isolated so one failure never blocks the next.
    if (isTauriRuntime()) {
      void runStartupOpenChain({
        getPendingOpenPath: () => invoke<{ path: string | null }>("get_pending_open_path"),
        openSingleFile: (path) => openSingleFile(path, {
          workspace: props.workspace,
          renderer: props.renderer,
          scheduler: props.scheduler,
          onError: (msg) => showToastImpl(msg, "error"),
          onLoading: (msg) => editorState.setLoadingMessage(msg),
        }),
        listAutosaves,
        askRecover: (count) =>
          ask(
            `${count} document(s) were auto-saved before the last session ended. Recover them?`,
            { title: "Recover unsaved work?" },
          ),
        recoverAutosave: (e) => loadProjectFile(e.path, {
          workspace: props.workspace,
          renderer: props.renderer,
          scheduler: props.scheduler,
          onError: (msg) => showToastImpl(msg, "error"),
          onLoading: (msg) => editorState.setLoadingMessage(msg),
        }, e.displayName),
        clearAutosaves: clearAllAutosaves,
        onError: (msg) => showToastImpl(msg, "error"),
        onRecovered: (count) => showToastImpl(`Recovered ${count} auto-saved document(s)`, "info"),
        onRecoverFailed: (e, msg) => showToastImpl(`Failed to recover ${e.displayName}: ${msg}`, "error"),
      });

      // Periodic auto-save (debounced 60s) of dirty documents.
      // Runs through the shared save queue as a LOW-PRIORITY job: it is
      // skipped entirely when a manual save is running or queued, and a
      // manual save preempts an in-flight autosave via cancelAutosave().
      const debouncedAutosave = createAutosaveTimerDebouncer(
        props.workspace,
        (msg) => showToastImpl(msg, "error"),
      );
      const autosaveTimer = setInterval(() => {
        scheduleSave(() => debouncedAutosave(), true);
      }, 60000);
      // Reset status from "saved" back to "idle" after 3 seconds so the indicator
      // does not perpetually show "Saved" until the next cycle.
      const statusResetTimer = setInterval(() => {
        setAutosaveStatus((prev) => (prev === "saved" ? "idle" : prev));
      }, 4000);
      onCleanup(() => {
        clearInterval(autosaveTimer);
        clearInterval(statusResetTimer);
      });
    }
  });

  let prevActiveLayerId: string | null = null;
  createEffect(() => {
    const id = editorState.activeLayerId();
    const sel = editorState.selectedLayerId();
    if (id && id !== prevActiveLayerId) {
      editorState.setSelectedLayerId(id);
    }
    prevActiveLayerId = id;
  });

  // Tool switch cleanup is registered per ToolId in toolLifecycle.ts.
  // That makes new tool additions declare their cleanup behavior at compile time.
  let prevActiveTool: ToolId | null = null;
  createEffect(() => {
    const tool = editorState.activeTool();
    if (prevActiveTool !== null && tool !== prevActiveTool) {
      // Auto-commit an active text edit session when LEAVING the text tool.
      // Guarded on prevActiveTool === "text": the double-click re-edit path
      // switches INTO the text tool and opens a fresh session synchronously —
      // committing whatever session is open here would instantly close it.
      if (prevActiveTool === "text" && editorState.textEditSession()) {
        commitTextSession({
          workspace: props.workspace,
          textEditSession: editorState.textEditSession,
          setTextEditSession: editorState.setTextEditSession,
          scheduler: props.scheduler,
        });
      }
      // Auto-commit an active transform session before switching tools.
      // Otherwise the transform changes would silently persist without a
      // history entry (the session is cleared but the engine transform is
      // kept), and Ctrl+Z could never revert them.
      const session = editorState.layerTransformSession();
      if (session) {
        const engine = props.workspace.getActiveEngine();
        const history = props.workspace.getActiveHistory();
        commitLayerTransformSession(session, engine, history);
        editorState.clearTransformStacks();
      }
      runToolSwitchCleanup(prevActiveTool, tool, {
        setHoverHandle: editorState.setHoverHandle,
        setHoverPos: editorState.setHoverPos,
        setLayerTransformSession: editorState.setLayerTransformSession,
        setSelectionEditMode: editorState.setSelectionEditMode,
      });
    }
    prevActiveTool = tool;
  });

  // Auto-commit an active text edit session when the ACTIVE DOCUMENT changes
  // (tab click, drag-to-switch tab, open/close). The session belongs to the
  // PREVIOUS document — the commit must run against THAT document's engine
  // and history, otherwise the pending text sticks to the wrong document or
  // leaves a ghost mutation.
  let prevDocId: string | null = null;
  createEffect(() => {
    const id = props.workspace.getActiveDocumentId();
    const prev = prevDocId;
    if (prev !== null && id !== prev && editorState.textEditSession()) {
      const ws = props.workspace;
      commitTextSession({
        workspace: {
          ...ws,
          getActiveEngine: () => ws.getEngine(prev),
          getActiveHistory: () => ws.getHistory(prev),
        } as typeof ws,
        textEditSession: editorState.textEditSession,
        setTextEditSession: editorState.setTextEditSession,
        scheduler: props.scheduler,
      });
    }
    prevDocId = id;
  });

  // Clear transient hover state when transform controls are hidden.
  // Prevents stale handle cursor from persisting in the browser after
  // the overlay SVG is unmounted (cursor ghosting bug).
  createEffect(() => {
    if (!editorState.showTransformControls()) {
      editorState.setHoverHandle(null);
      editorState.setHoverPos(null);
    }
  });

  // --- Domain values (review #19: one context per domain) ---

  const coreValue: EditorCoreValue = {
    workspace: props.workspace,
    renderer: props.renderer,
    scheduler: props.scheduler,
    camera,
    setViewportState,
    syncFromCamera,
    syncViewport,
    openImage: handleOpenImage,
    useGPUCameraForModernCrop,
    setUseGPUCameraForModernCrop,
    showToast: (message, severity = "info") => showToastImpl(message, severity),
  };

  const toolValue: ToolSettingsValue = {
    activeTool: editorState.activeTool,
    setActiveTool: editorState.setActiveTool,
    fgColor: editorState.fgColor,
    setFgColor: editorState.setFgColor,
    bgColor: editorState.bgColor,
    setBgColor: editorState.setBgColor,
    moveAutoSelect: editorState.moveAutoSelect,
    setMoveAutoSelect: editorState.setMoveAutoSelect,
    moveSnapEnabled: editorState.moveSnapEnabled,
    setMoveSnapEnabled: editorState.setMoveSnapEnabled,
    showTransformControls: editorState.showTransformControls,
    setShowTransformControls: editorState.setShowTransformControls,
    cropInteractionMode: editorState.cropInteractionMode,
    setCropInteractionMode: editorState.setCropInteractionMode,
    layerTransformSession: editorState.layerTransformSession,
    setLayerTransformSession: editorState.setLayerTransformSession,
    constrainRatio: editorState.constrainRatio,
    setConstrainRatio: editorState.setConstrainRatio,
    brushSize: editorState.brushSize,
    setBrushSize: editorState.setBrushSize,
    brushHardness: editorState.brushHardness,
    setBrushHardness: editorState.setBrushHardness,
    brushOpacity: editorState.brushOpacity,
    setBrushOpacity: editorState.setBrushOpacity,
    eraserSize: editorState.eraserSize,
    setEraserSize: editorState.setEraserSize,
    eraserHardness: editorState.eraserHardness,
    setEraserHardness: editorState.setEraserHardness,
    eraserOpacity: editorState.eraserOpacity,
    setEraserOpacity: editorState.setEraserOpacity,
    brushFlow: editorState.brushFlow,
    setBrushFlow: editorState.setBrushFlow,
    brushSmoothing: editorState.brushSmoothing,
    setBrushSmoothing: editorState.setBrushSmoothing,
    eraserFlow: editorState.eraserFlow,
    setEraserFlow: editorState.setEraserFlow,
    eraserSmoothing: editorState.eraserSmoothing,
    setEraserSmoothing: editorState.setEraserSmoothing,
    brushPresetId: editorState.brushPresetId,
    setBrushPresetId: editorState.setBrushPresetId,
    eraserPresetId: editorState.eraserPresetId,
    setEraserPresetId: editorState.setEraserPresetId,
    fillTolerance: editorState.fillTolerance,
    setFillTolerance: editorState.setFillTolerance,
    fillContiguous: editorState.fillContiguous,
    setFillContiguous: editorState.setFillContiguous,
    gradientType: editorState.gradientType,
    setGradientType: editorState.setGradientType,
    gradientPreset: editorState.gradientPreset,
    setGradientPreset: editorState.setGradientPreset,
    gradientDragLine: editorState.gradientDragLine,
    setGradientDragLine: editorState.setGradientDragLine,
    shapeKind: editorState.shapeKind,
    setShapeKind: editorState.setShapeKind,
    shapeFillEnabled: editorState.shapeFillEnabled,
    setShapeFillEnabled: editorState.setShapeFillEnabled,
    shapeStrokeEnabled: editorState.shapeStrokeEnabled,
    setShapeStrokeEnabled: editorState.setShapeStrokeEnabled,
    shapeStrokeColor: editorState.shapeStrokeColor,
    setShapeStrokeColor: editorState.setShapeStrokeColor,
    shapeStrokeWidth: editorState.shapeStrokeWidth,
    setShapeStrokeWidth: editorState.setShapeStrokeWidth,
    shapeRadius: editorState.shapeRadius,
    setShapeRadius: editorState.setShapeRadius,
    shapeArrowHead: editorState.shapeArrowHead,
    setShapeArrowHead: editorState.setShapeArrowHead,
    textFontFamily: editorState.textFontFamily,
    setTextFontFamily: editorState.setTextFontFamily,
    textFontSize: editorState.textFontSize,
    setTextFontSize: editorState.setTextFontSize,
    textFontWeight: editorState.textFontWeight,
    setTextFontWeight: editorState.setTextFontWeight,
    textFontItalic: editorState.textFontItalic,
    setTextFontItalic: editorState.setTextFontItalic,
    textAlign: editorState.textAlign,
    setTextAlign: editorState.setTextAlign,
    textStrokeWidth: editorState.textStrokeWidth,
    setTextStrokeWidth: editorState.setTextStrokeWidth,
    textStrokeColor: editorState.textStrokeColor,
    setTextStrokeColor: editorState.setTextStrokeColor,
    textStrokeAlign: editorState.textStrokeAlign,
    setTextStrokeAlign: editorState.setTextStrokeAlign,
    textEditSession: editorState.textEditSession,
    setTextEditSession: editorState.setTextEditSession,
    commitTransformState: editorState.commitTransformState,
    canTransformUndo: editorState.canTransformUndo,
    canTransformRedo: editorState.canTransformRedo,
    undoTransform: editorState.undoTransform,
    redoTransform: editorState.redoTransform,
    undoTransformWithCurrent: editorState.undoTransformWithCurrent,
    redoTransformWithCurrent: editorState.redoTransformWithCurrent,
    clearTransformStacks: editorState.clearTransformStacks,
    ...cropState,
    ...modernCropState,
  };

  const documentValue: DocumentStateValue = {
    zoom: editorState.zoom,
    setZoom: editorState.setZoom,
    pan: editorState.pan,
    setPan: editorState.setPan,
    documents: editorState.documents,
    activeDocumentId: editorState.activeDocumentId,
    layers: editorState.layers,
    activeLayerId: editorState.activeLayerId,
    selectedLayerId: editorState.selectedLayerId,
    setSelectedLayerId: editorState.setSelectedLayerId,
    selection: editorState.selection,
    setSelection: editorState.setSelection,
    selectionEditMode: editorState.selectionEditMode,
    setSelectionEditMode: editorState.setSelectionEditMode,
    selectionConstraintMode: editorState.selectionConstraintMode,
    setSelectionConstraintMode: editorState.setSelectionConstraintMode,
    selectionRatioW: editorState.selectionRatioW,
    setSelectionRatioW: editorState.setSelectionRatioW,
    selectionRatioH: editorState.selectionRatioH,
    setSelectionRatioH: editorState.setSelectionRatioH,
    selectionSizeW: editorState.selectionSizeW,
    setSelectionSizeW: editorState.setSelectionSizeW,
    selectionSizeH: editorState.selectionSizeH,
    setSelectionSizeH: editorState.setSelectionSizeH,
    selectionShape: editorState.selectionShape,
    setSelectionShape: editorState.setSelectionShape,
    hoveredLayerId: editorState.hoveredLayerId,
    setHoveredLayerId: editorState.setHoveredLayerId,
    hoverHandle: editorState.hoverHandle,
    setHoverHandle: editorState.setHoverHandle,
    hoverPos: editorState.hoverPos,
    setHoverPos: editorState.setHoverPos,
    docWidth: editorState.docWidth,
    docHeight: editorState.docHeight,
    viewportWidth: editorState.viewportWidth,
    setViewportWidth: editorState.setViewportWidth,
    viewportHeight: editorState.viewportHeight,
    setViewportHeight: editorState.setViewportHeight,
  };

  const dialogValue: DialogChromeValue = {
    colorPickerOpen: editorState.colorPickerOpen,
    setColorPickerOpen: editorState.setColorPickerOpen,
    colorPickerTarget: editorState.colorPickerTarget,
    setColorPickerTarget: editorState.setColorPickerTarget,
    showResizeDialog: editorState.showResizeDialog,
    setShowResizeDialog: editorState.setShowResizeDialog,
    showExportDialog: editorState.showExportDialog,
    setShowExportDialog: editorState.setShowExportDialog,
    showPrintDialog: editorState.showPrintDialog,
    setShowPrintDialog: editorState.setShowPrintDialog,
    loadingMessage: editorState.loadingMessage,
    setLoadingMessage: editorState.setLoadingMessage,
    renamingLayerId: editorState.renamingLayerId,
    setRenamingLayerId: editorState.setRenamingLayerId,
    renameLayerName: editorState.renameLayerName,
    setRenameLayerName: editorState.setRenameLayerName,
    chromeVisible: editorState.chromeVisible,
    setChromeVisible: editorState.setChromeVisible,
  };

  const dockValue: HistoryDockValue = {
    historyItems,
    activeHistoryIndex,
    navigateHistory,
    rightDockPanel,
    setRightDockPanel,
    rightDockOpen,
    setRightDockOpen,
    rightDockLayout: rightDockLayoutState,
    setRightDockLayout,
    inspectorTab,
    setInspectorTab,
    adjustSubTab,
    setAdjustSubTab,
  };

  // Expose editor on window only in dev/test builds for E2E introspection.
  if (typeof window !== "undefined" && shouldExposeEditorDebugHandle()) {
    (window as unknown as { __photrezEditor: EditorContextValue }).__photrezEditor = {
      ...coreValue,
      ...toolValue,
      ...documentValue,
      ...dialogValue,
      ...dockValue,
    };
  }

  return (
    <EditorCoreProvider value={coreValue}>
      <ToolSettingsProvider value={toolValue}>
        <DocumentStateProvider value={documentValue}>
          <DialogChromeProvider value={dialogValue}>
            <HistoryDockProvider value={dockValue}>
              <DialogProvider>
                <DragControllerProvider>
                  {props.children}
                </DragControllerProvider>
              </DialogProvider>
            </HistoryDockProvider>
          </DialogChromeProvider>
        </DocumentStateProvider>
      </ToolSettingsProvider>
    </EditorCoreProvider>
  );
}
