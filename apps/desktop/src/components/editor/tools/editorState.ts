import { createSignal } from "solid-js";
import type { LayerNode, DocumentTabSummary, Transform2D, DocumentModel, SelectionState, ShapeKind } from "@/engine/types";
import type { TextStrokeAlign } from "@/engine/textTypes";
import type { ToolId } from "./toolTypes";

export interface TextEditSession {
  /** Text layer being edited (new or existing). */
  layerId: string;
  /** Doc-space anchor (top-left of the text box). */
  docX: number;
  docY: number;
  boxMode: "point" | "area";
  /** Area box width (0 when point mode). */
  boxWidth: number;
  /** Area box height (0 when point mode). */
  boxHeight: number;
  /** True when the layer was created by this pointer gesture (temp layer). */
  isNewLayer: boolean;
  /** True while active drag gesture is in progress (pointer-events: none on overlay). */
  isDragging?: boolean;
  /** Snapshot taken BEFORE the gesture — committed on session close. */
  preSnapshot: DocumentModel;
}

export interface LayerTransformSession {
  documentId: string;
  layerId: string;
  originalSnapshot: DocumentModel;
  originalTransform: Transform2D;
  mode: "resize" | "rotate";
  lockRatio: boolean;
  startedAt: number;
}

export function createEditorState() {
  const [activeTool, setActiveTool] = createSignal<ToolId>("move");
  const [fgColor, setFgColor] = createSignal("#E15A17");
  const [bgColor, setBgColor] = createSignal("#FFFFFF");
  // Color-picker "pick from canvas" mode: when a color-picker dialog is
  // open (non-modal), a click on the canvas samples into the active target.
  const [colorPickerOpen, setColorPickerOpen] = createSignal(false);
  const [colorPickerTarget, setColorPickerTarget] = createSignal<"foreground" | "background">("foreground");
  const [zoom, setZoom] = createSignal(1.0);
  const [pan, setPan] = createSignal({ x: 0, y: 0 });

  // Sync state signals
  const [documents, setDocuments] = createSignal<DocumentTabSummary[]>([]);
  const [activeDocumentId, setActiveDocumentId] = createSignal<string | null>(null);
  const [layers, setLayers] = createSignal<LayerNode[]>([]);
  const [activeLayerId, setActiveLayerId] = createSignal<string | null>(null);
  const [hoveredLayerId, setHoveredLayerId] = createSignal<string | null>(null);
  const [hoverHandle, setHoverHandle] = createSignal<string | null>(null);
  const [docWidth, setDocWidth] = createSignal(800);
  const [docHeight, setDocHeight] = createSignal(600);
  const [viewportWidth, setViewportWidth] = createSignal(800);
  const [viewportHeight, setViewportHeight] = createSignal(600);

  const [moveAutoSelect, setMoveAutoSelect] = createSignal(true);
  const [moveSnapEnabled, setMoveSnapEnabled] = createSignal(true);
  const [snapToLayersEnabled, setSnapToLayersEnabled] = createSignal(true);
  const [snapToCanvasEnabled, setSnapToCanvasEnabled] = createSignal(true);
  const [showTransformControls, setShowTransformControls] = createSignal(true);
  const [cropInteractionMode, setCropInteractionMode] = createSignal<"modern" | "classic">("modern");
  const [hoverPos, setHoverPos] = createSignal<{ x: number; y: number } | null>(null);
  const [layerTransformSession, setLayerTransformSession] = createSignal<LayerTransformSession | null>(null);

  // Single source of truth for aspect-ratio locking across the app:
  // the PropertiesPanel "Constrain proportions" toggle, the TransformOptionBar
  // "Ratio" toggle, and on-canvas resize drag all read/write this. Default ON
  // (keep aspect ratio by default; Shift inverts on canvas drag).
  const [constrainRatio, setConstrainRatio] = createSignal(true);

  const [brushSize, setBrushSize] = createSignal(20);
  const [brushHardness, setBrushHardness] = createSignal(0.8);
  const [brushOpacity, setBrushOpacity] = createSignal(1);
  const [eraserSize, setEraserSize] = createSignal(32);
  const [eraserHardness, setEraserHardness] = createSignal(1);
  const [eraserOpacity, setEraserOpacity] = createSignal(1);
  const [brushFlow, setBrushFlow] = createSignal(1);
  const [brushSmoothing, setBrushSmoothing] = createSignal(0);
  const [eraserFlow, setEraserFlow] = createSignal(1);
  const [eraserSmoothing, setEraserSmoothing] = createSignal(0);
  const [brushPresetId, setBrushPresetId] = createSignal<string | null>(null);
  const [eraserPresetId, setEraserPresetId] = createSignal<string | null>(null);

  // Fill / Gradient tool settings
  const [fillTolerance, setFillTolerance] = createSignal(32);
  const [fillContiguous, setFillContiguous] = createSignal(true);
  const [gradientType, setGradientType] = createSignal<"linear" | "radial">("linear");
  const [gradientPreset, setGradientPreset] = createSignal<"fg-bg" | "fg-transparent">("fg-bg");
  const [gradientDragLine, setGradientDragLine] = createSignal<{
    start: { x: number; y: number };
    end: { x: number; y: number };
    type: "linear" | "radial";
    angle: number;
    distance: number;
  } | null>(null);

  // Shape tool: deterministic defaults per session (fill = fgColor, stroke = none).
  // No cross-session "last-used" persistence (research pain point).
  const [shapeKind, setShapeKind] = createSignal<ShapeKind>("rect");
  const [shapeFillEnabled, setShapeFillEnabled] = createSignal(true);
  const [shapeStrokeEnabled, setShapeStrokeEnabled] = createSignal(false);
  const [shapeStrokeColor, setShapeStrokeColor] = createSignal("#000000");
  const [shapeStrokeWidth, setShapeStrokeWidth] = createSignal(4);
  const [shapeRadius, setShapeRadius] = createSignal(0);
  const [shapeArrowHead, setShapeArrowHead] = createSignal(false);

  // Text tool: deterministic session defaults per plan §4/§5 (color = editor
  // foreground at create time, read from fgColor — no separate color signal).
  const [textFontFamily, setTextFontFamily] = createSignal("Arial");
  const [textFontSize, setTextFontSize] = createSignal(48);
  const [textFontWeight, setTextFontWeight] = createSignal(400);
  const [textFontItalic, setTextFontItalic] = createSignal(false);
  const [textAlign, setTextAlign] = createSignal<"left" | "center" | "right">("left");
  const [textStrokeWidth, setTextStrokeWidth] = createSignal(0);
  const [textStrokeColor, setTextStrokeColor] = createSignal("#000000");
  const [textStrokeAlign, setTextStrokeAlign] = createSignal<TextStrokeAlign>("outside");
  const [textUnderline, setTextUnderline] = createSignal(false);
  const [textStrikethrough, setTextStrikethrough] = createSignal(false);
  const [textUppercase, setTextUppercase] = createSignal(false);
  // Active text edit session (new layer temp or re-edit of an existing layer).
  // Non-null while the edit overlay is open; cleared on commit/cancel.
  const [textEditSession, setTextEditSession] = createSignal<TextEditSession | null>(null);

  const [selectedLayerId, setSelectedLayerId] = createSignal<string | null>(null);
  const [selection, setSelection] = createSignal<SelectionState | null>(null);
  const [selectionEditMode, setSelectionEditMode] = createSignal(false);
  const [selectionShape, setSelectionShape] = createSignal<"rect" | "ellipse">("rect");
  const [selectionConstraintMode, setSelectionConstraintMode] = createSignal<"normal" | "ratio" | "size">("normal");
  const [selectionRatioW, setSelectionRatioW] = createSignal(1);
  const [selectionRatioH, setSelectionRatioH] = createSignal(1);
  const [selectionSizeW, setSelectionSizeW] = createSignal(100);
  const [selectionSizeH, setSelectionSizeH] = createSignal(100);
  const [showResizeDialog, setShowResizeDialog] = createSignal(false);
  const [showExportDialog, setShowExportDialog] = createSignal(false);
  const [showPrintDialog, setShowPrintDialog] = createSignal(false);
  const [loadingMessage, setLoadingMessage] = createSignal<string | null>(null);
  const [renamingLayerId, setRenamingLayerId] = createSignal<string | null>(null);
  const [renameLayerName, setRenameLayerName] = createSignal("");
  const [chromeVisible, setChromeVisible] = createSignal(true);

  // Transform mini undo/redo state (like crop's mini undo/redo)
  const [transformUndoStack, setTransformUndoStack] = createSignal<{ transform: Transform2D }[]>([]);
  const [transformRedoStack, setTransformRedoStack] = createSignal<{ transform: Transform2D }[]>([]);

  const commitTransformState = (transform: Transform2D) => {
    setTransformUndoStack(prev => [...prev, { transform: { ...transform } }]);
    setTransformRedoStack([]);
  };

  const canTransformUndo = () => transformUndoStack().length > 0;
  const canTransformRedo = () => transformRedoStack().length > 0;

  const undoTransform = (): { transform: Transform2D } | null => {
    const stack = transformUndoStack();
    if (stack.length === 0) return null;
    const entry = stack[stack.length - 1];
    setTransformUndoStack(prev => prev.slice(0, -1));
    return entry;
  };

  const redoTransform = (): { transform: Transform2D } | null => {
    const stack = transformRedoStack();
    if (stack.length === 0) return null;
    const entry = stack[stack.length - 1];
    setTransformRedoStack(prev => prev.slice(0, -1));
    return entry;
  };

  const undoTransformWithCurrent = (currentTransform: Transform2D): { transform: Transform2D } | null => {
    const entry = undoTransform();
    if (entry) {
      setTransformRedoStack(prev => [...prev, { transform: { ...currentTransform } }]);
    }
    return entry;
  };

  const redoTransformWithCurrent = (currentTransform: Transform2D): { transform: Transform2D } | null => {
    const entry = redoTransform();
    if (entry) {
      setTransformUndoStack(prev => [...prev, { transform: { ...currentTransform } }]);
    }
    return entry;
  };

  const clearTransformStacks = () => {
    setTransformUndoStack([]);
    setTransformRedoStack([]);
  };

  return {
    activeTool, setActiveTool,
    selectedLayerId, setSelectedLayerId,
    selection, setSelection,
    selectionEditMode, setSelectionEditMode,
    selectionConstraintMode, setSelectionConstraintMode,
    selectionRatioW, setSelectionRatioW,
    selectionRatioH, setSelectionRatioH,
    selectionSizeW, setSelectionSizeW,
    selectionSizeH, setSelectionSizeH,
    selectionShape, setSelectionShape,
    fgColor, setFgColor,
    bgColor, setBgColor,
    colorPickerOpen, setColorPickerOpen,
    colorPickerTarget, setColorPickerTarget,
    zoom, setZoom,
    pan, setPan,
    documents, setDocuments,
    activeDocumentId, setActiveDocumentId,
    layers, setLayers,
    activeLayerId, setActiveLayerId,
    hoveredLayerId, setHoveredLayerId,
    hoverHandle, setHoverHandle,
    docWidth, setDocWidth,
    docHeight, setDocHeight,
    viewportWidth, setViewportWidth,
    viewportHeight, setViewportHeight,
    moveAutoSelect, setMoveAutoSelect,
    moveSnapEnabled, setMoveSnapEnabled,
    snapToLayersEnabled, setSnapToLayersEnabled,
    snapToCanvasEnabled, setSnapToCanvasEnabled,
    showTransformControls, setShowTransformControls,
    cropInteractionMode, setCropInteractionMode,
    hoverPos, setHoverPos,
    layerTransformSession, setLayerTransformSession,
    constrainRatio, setConstrainRatio,
    brushSize, setBrushSize,
    brushHardness, setBrushHardness,
    brushOpacity, setBrushOpacity,
    eraserSize, setEraserSize,
    eraserHardness, setEraserHardness,
    eraserOpacity, setEraserOpacity,
    brushFlow, setBrushFlow,
    brushSmoothing, setBrushSmoothing,
    eraserFlow, setEraserFlow,
    eraserSmoothing, setEraserSmoothing,
    brushPresetId, setBrushPresetId,
    eraserPresetId, setEraserPresetId,
    fillTolerance, setFillTolerance,
    fillContiguous, setFillContiguous,
    gradientType, setGradientType,
    gradientPreset, setGradientPreset,
    gradientDragLine, setGradientDragLine,
    shapeKind, setShapeKind,
    shapeFillEnabled, setShapeFillEnabled,
    shapeStrokeEnabled, setShapeStrokeEnabled,
    shapeStrokeColor, setShapeStrokeColor,
    shapeStrokeWidth, setShapeStrokeWidth,
    shapeRadius, setShapeRadius,
    shapeArrowHead, setShapeArrowHead,
    textFontFamily, setTextFontFamily,
    textFontSize, setTextFontSize,
    textFontWeight, setTextFontWeight,
    textFontItalic, setTextFontItalic,
    textAlign, setTextAlign,
    textStrokeWidth, setTextStrokeWidth,
    textStrokeColor, setTextStrokeColor,
    textStrokeAlign, setTextStrokeAlign,
    textUnderline, setTextUnderline,
    textStrikethrough, setTextStrikethrough,
    textUppercase, setTextUppercase,
    textEditSession, setTextEditSession,
    showResizeDialog, setShowResizeDialog,
    showExportDialog, setShowExportDialog,
    showPrintDialog, setShowPrintDialog,
    loadingMessage, setLoadingMessage,
    renamingLayerId, setRenamingLayerId,
    renameLayerName, setRenameLayerName,
    chromeVisible, setChromeVisible,
    // Transform mini undo/redo
    commitTransformState,
    canTransformUndo,
    canTransformRedo,
    undoTransform,
    redoTransform,
    undoTransformWithCurrent,
    redoTransformWithCurrent,
    clearTransformStacks,
  };
}
