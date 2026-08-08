import { createContext, useContext, type JSX } from "solid-js";
import type { Accessor, Setter } from "solid-js";
import type { ToolId } from "../../tools/toolTypes";
import type { LayerTransformSession } from "../../tools/editorState";
import type { Transform2D } from "@/engine/types";
import type { CropPreview, CropFillSource } from "../../cropState";
import type {
  ModernCropFrame,
  ModernCropImageTransform,
  ModernCropSnapshot,
} from "../../modernCropState";

/**
 * Tool configuration + tool-local undo stacks (brush/eraser/fill/gradient/move/
 * crop/modern-crop + transform session + crop mini undo/redo).
 * Domain context of EditorProvider (see EditorContext.tsx for composition).
 */
export interface ToolSettingsValue {
  activeTool: Accessor<ToolId>;
  setActiveTool: Setter<ToolId>;

  fgColor: Accessor<string>;
  setFgColor: Setter<string>;
  bgColor: Accessor<string>;
  setBgColor: Setter<string>;

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
  commitCropState: (rect: { x: number; y: number; w: number; h: number }, rotation: number) => void;
  canCropUndo: Accessor<boolean>;
  canCropRedo: Accessor<boolean>;
  undoLastCrop: () => { rect: { x: number; y: number; w: number; h: number }; rotation: number } | null;
  redoCrop: () => { rect: { x: number; y: number; w: number; h: number }; rotation: number } | null;
  clearCropStacks: () => void;

  // Modern Crop tool state
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

  // Shape tool settings
  shapeKind: Accessor<"rect" | "ellipse" | "line">;
  setShapeKind: Setter<"rect" | "ellipse" | "line">;
  shapeFillEnabled: Accessor<boolean>;
  setShapeFillEnabled: Setter<boolean>;
  shapeStrokeEnabled: Accessor<boolean>;
  setShapeStrokeEnabled: Setter<boolean>;
  shapeStrokeColor: Accessor<string>;
  setShapeStrokeColor: Setter<string>;
  shapeStrokeWidth: Accessor<number>;
  setShapeStrokeWidth: Setter<number>;
  shapeRadius: Accessor<number>;
  setShapeRadius: Setter<number>;
  shapeArrowHead: Accessor<boolean>;
  setShapeArrowHead: Setter<boolean>;

  // Text tool settings
  textFontFamily: Accessor<string>;
  setTextFontFamily: Setter<string>;
  textFontSize: Accessor<number>;
  setTextFontSize: Setter<number>;
  textFontWeight: Accessor<number>;
  setTextFontWeight: Setter<number>;
  textFontItalic: Accessor<boolean>;
  setTextFontItalic: Setter<boolean>;
  textAlign: Accessor<"left" | "center" | "right">;
  setTextAlign: Setter<"left" | "center" | "right">;

  // Transform mini undo/redo
  commitTransformState: (transform: Transform2D) => void;
  canTransformUndo: () => boolean;
  canTransformRedo: () => boolean;
  undoTransform: () => { transform: Transform2D } | null;
  redoTransform: () => { transform: Transform2D } | null;
  undoTransformWithCurrent: (currentTransform: Transform2D) => { transform: Transform2D } | null;
  redoTransformWithCurrent: (currentTransform: Transform2D) => { transform: Transform2D } | null;
  clearTransformStacks: () => void;
}

export const ToolSettingsContext = createContext<ToolSettingsValue>();

export function ToolSettingsProvider(props: { value: ToolSettingsValue; children: JSX.Element }) {
  return (
    <ToolSettingsContext.Provider value={props.value}>
      {props.children}
    </ToolSettingsContext.Provider>
  );
}

export function useToolSettings(): ToolSettingsValue {
  const ctx = useContext(ToolSettingsContext);
  if (!ctx) {
    throw new Error("useToolSettings must be used within an EditorProvider");
  }
  return ctx;
}
