import { createContext, useContext, onMount, onCleanup, createEffect, createSignal, batch, JSX } from "solid-js";
import { WorkspaceManager } from "@/engine/workspace";
import { WebGL2Backend } from "@/renderer/webgl2";
import { RenderScheduler } from "@/renderer/scheduler";
import { LayerNode, DocumentTabSummary, SelectionState, type Transform2D } from "@/engine/types";
import { Accessor, Setter } from "solid-js";
import { createEditorState, LayerTransformSession } from "../tools/editorState";
import type { ToolId } from "../tools/toolTypes";
import { createCropState, CropPreview, CropFillSource } from "../cropState";
import {
  createModernCropState,
  type ModernCropFrame,
  type ModernCropImageTransform,
  type ModernCropSnapshot,
} from "../modernCropState";
import { setupWorkspaceSync } from "../canvas/workspaceSync";
import { openImage, openSingleFile, loadProjectFile } from "../editorOpenImage";
import { runStartupOpenChain } from "./startupOpenChain";
import { autosaveDirtyDocs, listAutosaves, clearAllAutosaves, setAutosaveStatus, createAutosaveTimerDebouncer } from "../autoSave";
import { scheduleSave } from "../saveState";
import { ask } from "@tauri-apps/plugin-dialog";
import { ViewportCamera } from "../../../viewport/viewportCamera";
import { DragControllerProvider } from "../DragController";
import { showToast as showToastImpl } from "../Toast";
import { runToolSwitchCleanup } from "../tools/toolLifecycle";
import { DialogProvider } from "../dialogs/DialogProvider";
import type { HistoryItem } from "@/engine/history";
import { cancelLayerTransformSession, commitLayerTransformSession } from "../transformSession";
import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";


export interface EditorContextValue {
  workspace: WorkspaceManager;
  renderer: WebGL2Backend;
  scheduler: RenderScheduler;
  camera: ViewportCamera;
  setViewportState: (next: { x: number; y: number; zoom: number }) => void;
  syncFromCamera: () => void;
  
  openImage: () => Promise<void>;
  
  // UI Signals
  activeTool: Accessor<ToolId>;
  setActiveTool: Setter<ToolId>;
  
  fgColor: Accessor<string>;
  setFgColor: Setter<string>;
  
  bgColor: Accessor<string>;
  setBgColor: Setter<string>;

  colorPickerOpen: Accessor<boolean>;
  setColorPickerOpen: Setter<boolean>;
  colorPickerTarget: Accessor<"foreground" | "background">;
  setColorPickerTarget: Setter<"foreground" | "background">;
  
  zoom: Accessor<number>;
  setZoom: Setter<number>;
  
  pan: Accessor<{ x: number; y: number }>;
  setPan: Setter<{ x: number; y: number }>;

  syncViewport: () => void;

  // Derived / Sync signals
  documents: Accessor<DocumentTabSummary[]>;
  activeDocumentId: Accessor<string | null>;
  layers: Accessor<LayerNode[]>;
  activeLayerId: Accessor<string | null>;
  selectedLayerId: Accessor<string | null>;
  setSelectedLayerId: Setter<string | null>;
  selection: Accessor<SelectionState | null>;
  setSelection: Setter<SelectionState | null>;
  selectionEditMode: Accessor<boolean>;
  setSelectionEditMode: Setter<boolean>;
  selectionConstraintMode: Accessor<"normal" | "ratio" | "size">;
  setSelectionConstraintMode: Setter<"normal" | "ratio" | "size">;
  selectionRatioW: Accessor<number>;
  setSelectionRatioW: Setter<number>;
  selectionRatioH: Accessor<number>;
  setSelectionRatioH: Setter<number>;
  selectionSizeW: Accessor<number>;
  setSelectionSizeW: Setter<number>;
  selectionSizeH: Accessor<number>;
  setSelectionSizeH: Setter<number>;
  selectionShape: Accessor<"rect" | "ellipse">;
  setSelectionShape: Setter<"rect" | "ellipse">;
  hoveredLayerId: Accessor<string | null>;
  setHoveredLayerId: Setter<string | null>;
  hoverHandle: Accessor<string | null>;
  setHoverHandle: Setter<string | null>;
  docWidth: Accessor<number>;
  docHeight: Accessor<number>;
  viewportWidth: Accessor<number>;
  setViewportWidth: Setter<number>;
  viewportHeight: Accessor<number>;
  setViewportHeight: Setter<number>;

  // Move Tool options
  moveAutoSelect: Accessor<boolean>;
  setMoveAutoSelect: Setter<boolean>;
  moveSnapEnabled: Accessor<boolean>;
  setMoveSnapEnabled: Setter<boolean>;
  showTransformControls: Accessor<boolean>;
  setShowTransformControls: Setter<boolean>;

  // Crop interaction mode
  cropInteractionMode: Accessor<"modern" | "classic">;
  setCropInteractionMode: Setter<"modern" | "classic">;

  // Crop Tool options
  cropRect: Accessor<{ x: number; y: number; w: number; h: number } | null>;
  setCropRect: Setter<{ x: number; y: number; w: number; h: number } | null>;
  cropMode: Accessor<"free" | "ratio" | "size">;
  setCropMode: Setter<"free" | "ratio" | "size">;
  cropGuideMode: Accessor<"none" | "thirds" | "grid" | "diagonal" | "golden">;
  setCropGuideMode: Setter<"none" | "thirds" | "grid" | "diagonal" | "golden">;
  cropDeletePixels: Accessor<boolean>;
  setCropDeletePixels: Setter<boolean>;
  cropFillEnabled: Accessor<boolean>;
  setCropFillEnabled: Setter<boolean>;
  cropFillSource: Accessor<CropFillSource>;
  setCropFillSource: Setter<CropFillSource>;
  cropFillCustomColor: Accessor<string>;
  setCropFillCustomColor: Setter<string>;
  cropAspect: Accessor<{ w: number; h: number } | null>;
  setCropAspect: Setter<{ w: number; h: number } | null>;
  cropSizeTarget: Accessor<{ w: number; h: number } | null>;
  setCropSizeTarget: Setter<{ w: number; h: number } | null>;
  cropSizeUnit: Accessor<"px" | "cm" | "mm" | "in">;
  setCropSizeUnit: Setter<"px" | "cm" | "mm" | "in">;
  cropRotation: Accessor<number>;
  setCropRotation: Setter<number>;
  hiddenCropPreview: Accessor<CropPreview | null>;
  setHiddenCropPreview: Setter<CropPreview | null>;
  modernCropFrame: Accessor<ModernCropFrame | null>;
  setModernCropFrame: Setter<ModernCropFrame | null>;
  modernCropImageTransform: Accessor<ModernCropImageTransform>;
  setModernCropImageTransform: Setter<ModernCropImageTransform>;
  resetModernCrop: () => void;
  commitModernCropState: () => void;
  canModernCropUndo: Accessor<boolean>;
  canModernCropRedo: Accessor<boolean>;
  undoModernCrop: () => ModernCropSnapshot | null;
  redoModernCrop: () => ModernCropSnapshot | null;
  commitCropState: (rect: { x: number; y: number; w: number; h: number }, rotation: number) => void;
  canCropUndo: Accessor<boolean>;
  canCropRedo: Accessor<boolean>;
  undoLastCrop: () => { rect: { x: number; y: number; w: number; h: number }; rotation: number } | null;
  redoCrop: () => { rect: { x: number; y: number; w: number; h: number }; rotation: number } | null;
  clearCropStacks: () => void;

  // Rotate cursor hover position (screen-space)
  hoverPos: Accessor<{ x: number; y: number } | null>;
  setHoverPos: Setter<{ x: number; y: number } | null>;

  // Transform Session
  layerTransformSession: Accessor<LayerTransformSession | null>;
  setLayerTransformSession: Setter<LayerTransformSession | null>;

  // Aspect-ratio lock (single source of truth: PropertiesPanel + TransformOptionBar + canvas drag)
  constrainRatio: Accessor<boolean>;
  setConstrainRatio: Setter<boolean>;

  // Paint tool settings
  brushSize: Accessor<number>;
  setBrushSize: Setter<number>;
  brushHardness: Accessor<number>;
  setBrushHardness: Setter<number>;
  brushOpacity: Accessor<number>;
  setBrushOpacity: Setter<number>;
  eraserSize: Accessor<number>;
  setEraserSize: Setter<number>;
  eraserHardness: Accessor<number>;
  setEraserHardness: Setter<number>;
  eraserOpacity: Accessor<number>;
  setEraserOpacity: Setter<number>;
  brushFlow: Accessor<number>;
  setBrushFlow: Setter<number>;
  brushSmoothing: Accessor<number>;
  setBrushSmoothing: Setter<number>;
  eraserFlow: Accessor<number>;
  setEraserFlow: Setter<number>;
  eraserSmoothing: Accessor<number>;
  setEraserSmoothing: Setter<number>;
  brushPresetId: Accessor<string | null>;
  setBrushPresetId: Setter<string | null>;
  eraserPresetId: Accessor<string | null>;
  setEraserPresetId: Setter<string | null>;

  // Fill / Gradient tool settings
  fillTolerance: Accessor<number>;
  setFillTolerance: Setter<number>;
  fillContiguous: Accessor<boolean>;
  setFillContiguous: Setter<boolean>;
  gradientType: Accessor<"linear" | "radial">;
  setGradientType: Setter<"linear" | "radial">;
  gradientPreset: Accessor<"fg-bg" | "fg-transparent">;
  setGradientPreset: Setter<"fg-bg" | "fg-transparent">;
  gradientDragLine: Accessor<{
    start: { x: number; y: number };
    end: { x: number; y: number };
    type: "linear" | "radial";
    angle: number;
    distance: number;
  } | null>;
  setGradientDragLine: Setter<{
    start: { x: number; y: number };
    end: { x: number; y: number };
    type: "linear" | "radial";
    angle: number;
    distance: number;
  } | null>;

  showResizeDialog: Accessor<boolean>;
  setShowResizeDialog: Setter<boolean>;
  showExportDialog: Accessor<boolean>;
  setShowExportDialog: Setter<boolean>;
  showPrintDialog: Accessor<boolean>;
  setShowPrintDialog: Setter<boolean>;

  loadingMessage: Accessor<string | null>;
  setLoadingMessage: Setter<string | null>;
  renamingLayerId: Accessor<string | null>;
  setRenamingLayerId: Setter<string | null>;
  renameLayerName: Accessor<string>;
  setRenameLayerName: Setter<string>;
  chromeVisible: Accessor<boolean>;
  setChromeVisible: Setter<boolean>;

  // Feature flag: GPU camera image transform for Modern Crop
  useGPUCameraForModernCrop: Accessor<boolean>;
  setUseGPUCameraForModernCrop: Setter<boolean>;

  // Toast notifications
  showToast: (message: string, severity?: "info" | "warn" | "error") => void;

  // History panel UI
  historyItems: Accessor<HistoryItem[]>;
  activeHistoryIndex: Accessor<number>;
  navigateHistory: (index: number) => void;
  rightDockPanel: Accessor<"layers" | "history">;
  setRightDockPanel: Setter<"layers" | "history">;

  // Transform mini undo/redo
  commitTransformState: (transform: Transform2D) => void;
  canTransformUndo: () => boolean;
  canTransformRedo: () => boolean;
  undoTransform: () => { transform: Transform2D } | null;
  redoTransform: () => { transform: Transform2D } | null;
  undoTransformWithCurrent: (currentTransform: Transform2D) => { transform: Transform2D } | null;
  redoTransformWithCurrent: (currentTransform: Transform2D) => { transform: Transform2D } | null;
  clearTransformStacks: () => void;

  // Side dock state
  rightDockOpen: Accessor<boolean>;
  setRightDockOpen: (open: boolean) => void;

  // Right dock layout & inspector tabs
  rightDockLayout: Accessor<"side-by-side" | "stacked">;
  setRightDockLayout: (layout: "side-by-side" | "stacked") => void;
  inspectorTab: Accessor<"library" | "adjust" | "presets">;
  setInspectorTab: Setter<"library" | "adjust" | "presets">;
  adjustSubTab: Accessor<"properties" | "adjustments">;
  setAdjustSubTab: Setter<"properties" | "adjustments">;
}


const EditorContext = createContext<EditorContextValue>();

interface EditorDebugEnv {
  DEV?: boolean;
  MODE?: string;
  VITE_PHOTREZ_DEBUG_EDITOR?: string;
}

export function shouldExposeEditorDebugHandle(env: EditorDebugEnv = import.meta.env): boolean {
  return env.DEV === true || env.MODE === "test" || env.VITE_PHOTREZ_DEBUG_EDITOR === "1";
}

export function useEditor(): EditorContextValue {
  const context = useContext(EditorContext);
  if (!context) {
    throw new Error("useEditor must be used within an EditorProvider");
  }
  return context;
}

export function EditorProvider(props: {
  workspace: WorkspaceManager;
  renderer: WebGL2Backend;
  scheduler: RenderScheduler;
  camera?: ViewportCamera;
  rightDockOpen?: Accessor<boolean>;
  setRightDockOpen?: (open: boolean) => void;
  // Feature flag signal can be owned by the caller (EditorShell creates it so
  // the RenderScheduler callback closure can read it) and passed in; when
  // absent the provider owns a fresh signal.
  useGPUCameraForModernCrop?: Accessor<boolean>;
  setUseGPUCameraForModernCrop?: Setter<boolean>;
  children: JSX.Element;
}) {
  const camera = props.camera || new ViewportCamera();
  const editorState = createEditorState();
  const [gpuCameraSignal, setGpuCameraSignal] = createSignal(true);
  const useGPUCameraForModernCrop = props.useGPUCameraForModernCrop ?? gpuCameraSignal;
  const setUseGPUCameraForModernCrop = props.setUseGPUCameraForModernCrop ?? setGpuCameraSignal;
  const cropState = createCropState();
  const modernCropState = createModernCropState();

  const [historyItems, setHistoryItems] = createSignal<HistoryItem[]>([]);
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

  // Clear transient hover state when transform controls are hidden.
  // Prevents stale handle cursor from persisting in the browser after
  // the overlay SVG is unmounted (cursor ghosting bug).
  createEffect(() => {
    if (!editorState.showTransformControls()) {
      editorState.setHoverHandle(null);
      editorState.setHoverPos(null);
    }
  });

  const value: EditorContextValue = {
    workspace: props.workspace,
    renderer: props.renderer,
    scheduler: props.scheduler,
    camera,
    setViewportState,
    syncFromCamera,
    openImage: handleOpenImage,
    ...editorState,
    ...cropState,
    ...modernCropState,
    syncViewport,
    useGPUCameraForModernCrop,
    setUseGPUCameraForModernCrop,
    showToast: (message, severity = "info") => showToastImpl(message, severity),
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
    (window as unknown as { __photrezEditor: EditorContextValue }).__photrezEditor =
      value;
  }

  return (
    <EditorContext.Provider value={value}>
      <DialogProvider>
        <DragControllerProvider>
          {props.children}
        </DragControllerProvider>
      </DialogProvider>
    </EditorContext.Provider>
  );
}
