// SPDX-License-Identifier: AGPL-3.0-or-later
import type { ToolContext } from "@/viewport/input-handler";
import { getActivePaintToolSettings } from "../../brushToolState";
import { getLayerAabb } from "@/viewport/transformGeometry";
import { computeSnapAdjustment, type SnapRect } from "@/viewport/smartGuides";
import { createCropRectFromDocumentPoints } from "../../cropToolActions";
import type { PointerToolContext } from "./pointerToolContext";

/**
 * Populate the shared ToolContext before dispatching a pointer event.
 * Reads current editor signals (colors, paint settings, selection
 * constraint, snap targets) into the mutable interactiveState object.
 */
export function prepareToolContext(
  ctx: PointerToolContext,
  interactiveState: ToolContext,
): void {
  const { editor } = ctx;
  const {
    workspace,
    fgColor,
    bgColor,
    activeTool,
    selectedLayerId,
    selectionConstraintMode,
    selectionRatioW,
    selectionRatioH,
    selectionSizeW,
    selectionSizeH,
    selectionShape,
    zoom,
    moveSnapEnabled,
    setFgColor,
    setBgColor,
    setHoverHandle,
    brushSize,
    brushHardness,
    brushOpacity,
    brushFlow,
    brushSmoothing,
    eraserSize,
    eraserHardness,
    eraserOpacity,
    eraserFlow,
    eraserSmoothing,
  } = editor;
  const { selectionBox, setSelectionBoxSignal, setHudInfo, setSnapLines } = ctx;

  const engine = workspace.getActiveEngine();
  interactiveState.fgColor = fgColor();
  interactiveState.bgColor = bgColor();
  // For the move tool only: if the UI signal says no layer is selected
  // (pasteboard/canvas deselect), don't operate on a stale engine layer.
  // Other tools (brush, eraser, selection, crop) should use the engine's
  // active layer as-is since they don't depend on the UI selection state.
  const engineLayerId = engine ? engine.getActiveLayerId() : null;
  if (activeTool() === "move" && selectedLayerId() === null) {
    interactiveState.selectedLayerId = null;
  } else {
    interactiveState.selectedLayerId = engineLayerId;
  }
  interactiveState.isAltPressed = ctx.isAltPressed();
  interactiveState.setFgColor = setFgColor;
  interactiveState.setBgColor = setBgColor;
  interactiveState.selectionConstraintMode = typeof selectionConstraintMode === "function" ? selectionConstraintMode() : "normal";
  interactiveState.selectionRatioW = typeof selectionRatioW === "function" ? selectionRatioW() : 1;
  interactiveState.selectionRatioH = typeof selectionRatioH === "function" ? selectionRatioH() : 1;
  interactiveState.selectionSizeW = typeof selectionSizeW === "function" ? selectionSizeW() : 100;
  interactiveState.selectionSizeH = typeof selectionSizeH === "function" ? selectionSizeH() : 100;
  interactiveState.onSelectionCreated = (x, y, w, h) => {
    const currentShape = typeof selectionShape === "function" ? selectionShape() : undefined;
    setSelectionBoxSignal({ x, y, w, h, angle: 0, shape: currentShape });
    // Show W×H HUD during selection draw drag
    const sp = interactiveState.screenPos;
    if (sp) {
      setHudInfo({
        mode: "resize",
        clientX: sp.x,
        clientY: sp.y,
        width: w,
        height: h,
        deltaX: 0,
        deltaY: 0,
        scalePercent: 0,
        angle: 0,
        snapActive: false,
      });
    }
  };
  interactiveState.selectionBounds = selectionBox() ? {
    x: selectionBox()!.x,
    y: selectionBox()!.y,
    width: selectionBox()!.w,
    height: selectionBox()!.h,
    angle: selectionBox()!.angle ?? 0,
  } : null;
  interactiveState.onSelectionMoved = (x, y) => {
    const box = selectionBox();
    const eng = workspace.getActiveEngine();
    if (box && eng) {
      // Clamp to document bounds so selection can't be moved completely off-canvas
      const docW = eng.getWidth();
      const docH = eng.getHeight();
      const clampedX = Math.max(-box.w + 1, Math.min(docW - 1, x));
      const clampedY = Math.max(-box.h + 1, Math.min(docH - 1, y));
      setSelectionBoxSignal({ ...box, x: clampedX, y: clampedY });
      eng.createSelection(clampedX, clampedY, box.w, box.h, box.angle, box.shape);
    }
    // Show ΔX ΔY HUD during selection move
    const sp = interactiveState.screenPos;
    const orig = interactiveState.pendingOriginalSelectionPos;
    if (sp && orig) {
      setHudInfo({
        mode: "move",
        clientX: sp.x,
        clientY: sp.y,
        deltaX: x - orig.x,
        deltaY: y - orig.y,
        width: 0,
        height: 0,
        scalePercent: 0,
        angle: 0,
        snapActive: false,
      });
    }
  };
  interactiveState.onSelectionRotated = (angle: number) => {
    const box = selectionBox();
    if (box) {
      setSelectionBoxSignal({ ...box, angle });
    }
  };
  interactiveState.onRotateStart = (centerX: number, centerY: number) => {
    interactiveState.dragMode = "rotate-selection";
    interactiveState.rotateCenter = { x: centerX, y: centerY };
    interactiveState.rotateStartAngle = 0;
    interactiveState.selectionAngle = selectionBox()?.angle ?? 0;
  };
  interactiveState.onCropCreated = (x, y, w, h) => {
    const nextRect = createCropRectFromDocumentPoints(
      interactiveState.dragStart,
      interactiveState.dragCurrent
    );
    if (nextRect) {
      editor.setHiddenCropPreview(null);
      editor.setCropRotation(0);
      editor.setCropRect(nextRect);
    }
  };
  interactiveState.onHoverHandle = setHoverHandle;

  interactiveState.paintSettings = getActivePaintToolSettings(activeTool(), {
    brushSize: brushSize(),
    brushHardness: brushHardness(),
    brushOpacity: brushOpacity(),
    brushFlow: brushFlow(),
    brushSmoothing: brushSmoothing(),
    eraserSize: eraserSize(),
    eraserHardness: eraserHardness(),
    eraserOpacity: eraserOpacity(),
    eraserFlow: eraserFlow(),
    eraserSmoothing: eraserSmoothing(),
  });
  interactiveState.brushSize = interactiveState.paintSettings.size;
  interactiveState.brushHardness = interactiveState.paintSettings.hardness;
  interactiveState.brushOpacity = interactiveState.paintSettings.opacity;

  const activeEngineForTargets = workspace.getActiveEngine();
  const movingId = activeEngineForTargets ? activeEngineForTargets.getActiveLayerId() : null;
  const docW = activeEngineForTargets ? activeEngineForTargets.getWidth() : 0;
  const docH = activeEngineForTargets ? activeEngineForTargets.getHeight() : 0;

  const layerTargets: SnapRect[] = activeEngineForTargets
    ? activeEngineForTargets.getLayers()
      .filter((l) => l.visible && l.id !== movingId)
      .map((l) => {
        const aabb = getLayerAabb(l.transform, l.width, l.height);
        return { x: aabb.x, y: aabb.y, w: aabb.width, h: aabb.height };
      })
    : [];

  const snapTargets: SnapRect[] = [
    { x: 0, y: 0, w: docW, h: docH, snapThreshold: 12, snapPriority: 3 },
    { x: docW / 2, y: -Infinity, w: 0, h: Infinity, snapThreshold: 6, snapPriority: 2 },
    { x: -Infinity, y: docH / 2, w: Infinity, h: 0, snapThreshold: 6, snapPriority: 2 },
    ...layerTargets,
  ];

  if (moveSnapEnabled()) {
    interactiveState.onComputeSnap = (rect: SnapRect) => {
      if (!activeEngineForTargets) {
        setSnapLines([]);
        return { dx: 0, dy: 0, lines: [] };
      }
      return computeSnapAdjustment(rect, snapTargets, 5, zoom());
    };
  } else {
    interactiveState.onComputeSnap = undefined;
    setSnapLines([]);
  }
  interactiveState.onSnapLines = (lines) => setSnapLines(lines);
  interactiveState.onPaintStroke = ctx.onPaintStroke;
}
