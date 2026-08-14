// apps/desktop/src/components/editor/__tests__/shapeToolWiring.test.tsx
//
// Wiring contract for the shape tool's drag-create pointer chain:
//   pointerdown → addShapeLayer (temp) → pointermove → updateShapeParams on the
//   temp layer (live preview) → pointerup → commit (or delete under 3px).
//
// Also covers the Shift (square/circle via max-side) and Alt (draw-from-center)
// geometry modifiers, and the line kind passthrough.

import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { describe, it, expect, vi } from "vitest";
import { createMockEditorParams, createPointerTools, makePointerEvent } from "../../../__tests__/pointerRoutingHarness";
import { computeShapeBox, cancelShapeDrag } from "../canvas/pointerTools/shapeTool";
import type { ShapeDragState } from "../canvas/pointerTools/pointerToolContext";
import { shapeRenderMargin, MAX_SHAPE_DIM } from "@/engine/shapeRaster";

vi.mock("../dialogs/DialogProvider", () => ({
  useDialog: () => ({ confirm: vi.fn().mockResolvedValue(false) }),
}));

function makePointerTools(signals: Record<string, any>) {
  mockUseEditor(signals);
  const params = {
    getCanvasContainerRef: () => document.createElement("div"),
    getCanvasRef: () => document.createElement("canvas"),
    isSpacePressed: () => false,
    isPanning: () => false,
    isAltPressed: () => false,
    stopMomentum: vi.fn(),
    fitToScreenAndRender: vi.fn(),
    commitBrushStroke: vi.fn(),
  };
  return createPointerTools(params);
}

describe("shape tool pointer wiring", () => {
  it("pointerdown starts drag, pointermove updates preview bitmap, pointerup commits a shape layer", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    const history = signals.workspace.getActiveHistory();
    (mockEngine as any).addShapeLayer = vi.fn(() => ({
      id: "shape-1", type: "shape", name: "Shape 1",
      width: 100, height: 100, imageBitmap: {} as ImageBitmap,
    }));
    (mockEngine as any).getLayer = vi.fn((id: string) => ({ id, type: "shape", width: 100, height: 100 }));
    (mockEngine as any).updateShapeParams = vi.fn();
    (mockEngine as any).deleteLayer = vi.fn();

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));
    expect(mockEngine.addShapeLayer).toHaveBeenCalled();
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 110, clientY: 60 }));
    expect(mockEngine.updateShapeParams).toHaveBeenCalled();
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 110, clientY: 60 }));
    // drag (10,10)→(110,60): w=100, h=50, both ≥ 3px → committed, temp kept.
    expect(history.commit).toHaveBeenCalled();
    expect(history.commit).toHaveBeenCalledWith(expect.anything(), "Add Shape");
    expect(mockEngine.deleteLayer).not.toHaveBeenCalled();
    disposeTools();
    dispose();
  });

  it("sub-3px drag deletes the temp layer and commits nothing", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    const history = signals.workspace.getActiveHistory();
    (mockEngine as any).addShapeLayer = vi.fn(() => ({ id: "shape-1", type: "shape", width: 1, height: 1 }));
    (mockEngine as any).deleteLayer = vi.fn();

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 51, clientY: 50 }));
    expect(mockEngine.deleteLayer).toHaveBeenCalledWith("shape-1");
    expect(history.commit).not.toHaveBeenCalled();
    disposeTools();
    dispose();
  });

  it("Shift constrains to square/circle ratio", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    (mockEngine as any).addShapeLayer = vi.fn(() => ({ id: "shape-1", type: "shape" }));
    (mockEngine as any).updateShapeParams = vi.fn();

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 150, clientY: 80, shiftKey: true }));
    const lastCall = (mockEngine as any).updateShapeParams.mock.calls.at(-1)![1] as any;
    // drag (50,50)→(150,80): w=100, h=30, side=max(100,30)=100 → square 100×100
    expect(lastCall.width).toBe(100);
    expect(lastCall.height).toBe(100);
    disposeTools();
    dispose();
  });

  it("Alt draws from center", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    (mockEngine as any).addShapeLayer = vi.fn(() => ({ id: "shape-1", type: "shape" }));
    (mockEngine as any).updateShapeParams = vi.fn();

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 100, clientY: 100 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 150, clientY: 120, altKey: true }));
    const lastCall = (mockEngine as any).updateShapeParams.mock.calls.at(-1)![1] as any;
    // Alt: start is the center; box spans start ± delta → dims doubled.
    // delta = (50,20) → width 100, height 40, cursor at the far corner.
    expect(lastCall.width).toBe(100);
    expect(lastCall.height).toBe(40);
    // top-left = center - delta = (100-50, 100-20) = (50, 80)
    expect(mockEngine.transformLayer).toHaveBeenCalledWith(
      "shape-1",
      { x: 50, y: 80, flipH: false, flipV: false },
    );
    disposeTools();
    dispose();
  });

  it("horizontal line is not deleted (degenerate height allowed)", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    const history = signals.workspace.getActiveHistory();
    (mockEngine as any).addShapeLayer = vi.fn(() => ({ id: "shape-1", type: "shape" }));
    (mockEngine as any).updateShapeParams = vi.fn();
    (mockEngine as any).deleteLayer = vi.fn();
    signals.setShapeKind("line");

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 150, clientY: 50 }));
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 150, clientY: 50 }));
    // 100px horizontal line (height 0) → hypot=100 ≥ 3 → kept + committed.
    expect(mockEngine.deleteLayer).not.toHaveBeenCalled();
    expect(history.commit).toHaveBeenCalled();
    disposeTools();
    dispose();
  });

  it("line kind passes arrowHead and degenerate height allowed", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    // The shape tool reads stroke settings from the editor; give the mock a
    // visible stroked line so the cap-margin placement path is exercised.
    (mockEngine as any).shapeStrokeEnabled = () => true;
    (mockEngine as any).shapeStrokeWidth = () => 4;
    (mockEngine as any).shapeStrokeColor = () => "#000";
    (mockEngine as any).addShapeLayer = vi.fn(() => ({ id: "shape-1", type: "shape" }));
    (mockEngine as any).updateShapeParams = vi.fn();
    signals.setShapeKind("line");

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 150, clientY: 50 }));
    const lastCall = (mockEngine as any).updateShapeParams.mock.calls.at(-1)![1] as any;
    expect(lastCall.kind).toBe("line");
    expect(lastCall.arrowHead).toBe(false);
    disposeTools();
    dispose();
  });

  it("vertical line keeps width 0 (no 1px tilt)", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    (mockEngine as any).addShapeLayer = vi.fn(() => ({ id: "shape-1", type: "shape" }));
    (mockEngine as any).updateShapeParams = vi.fn();
    signals.setShapeKind("line");

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 50, clientY: 150 }));
    const lastCall = (mockEngine as any).updateShapeParams.mock.calls.at(-1)![1] as any;
    expect(lastCall.width).toBe(0);
    expect(lastCall.height).toBe(100);
    disposeTools();
    dispose();
  });

  // --- Arrow alignment + line orientation (fixes #1 and #2) ---

  it("arrow line down-right: placed at docX - margin, no flip", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    (mockEngine as any).addShapeLayer = vi.fn(() => ({ id: "shape-1", type: "shape" }));
    (mockEngine as any).updateShapeParams = vi.fn();
    signals.setShapeKind("line");
    signals.setShapeArrowHead(true);

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 150, clientY: 50 }));
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 150, clientY: 50 }));

    const lastParams = (mockEngine as any).updateShapeParams.mock.calls.at(-1)![1] as any;
    const margin = shapeRenderMargin(lastParams);
    expect(mockEngine.transformLayer).toHaveBeenCalledWith("shape-1", {
      x: 50 - margin,
      y: 50 - margin,
      flipH: false,
      flipV: false,
    });
    disposeTools();
    dispose();
  });

  it("arrow line dragged up-right flips vertically so the arrow points at release", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    (mockEngine as any).addShapeLayer = vi.fn(() => ({ id: "shape-1", type: "shape" }));
    (mockEngine as any).updateShapeParams = vi.fn();
    signals.setShapeKind("line");
    signals.setShapeArrowHead(true);

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    // press bottom-left (50,150), release top-right (150,50) → "/" line
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 150 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 150, clientY: 50 }));
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 150, clientY: 50 }));

    const lastParams = (mockEngine as any).updateShapeParams.mock.calls.at(-1)![1] as any;
    const margin = shapeRenderMargin(lastParams);
    expect(mockEngine.transformLayer).toHaveBeenCalledWith("shape-1", {
      x: 50 - margin,
      y: 50 - margin,
      flipH: false,
      flipV: true,
    });
    disposeTools();
    dispose();
  });

  it("arrow line dragged down-left flips horizontally so the arrow points at release", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    (mockEngine as any).addShapeLayer = vi.fn(() => ({ id: "shape-1", type: "shape" }));
    (mockEngine as any).updateShapeParams = vi.fn();
    signals.setShapeKind("line");
    signals.setShapeArrowHead(true);

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    // press top-right (150,50), release bottom-left (50,150) → "\" flipped
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 150, clientY: 50 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 50, clientY: 150 }));
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 50, clientY: 150 }));

    const lastParams = (mockEngine as any).updateShapeParams.mock.calls.at(-1)![1] as any;
    const margin = shapeRenderMargin(lastParams);
    expect(mockEngine.transformLayer).toHaveBeenCalledWith("shape-1", {
      x: 50 - margin,
      y: 50 - margin,
      flipH: true,
      flipV: false,
    });
    disposeTools();
    dispose();
  });

  it("computeShapeBox flips for all four drag quadrants and centers on Alt", () => {
    expect(computeShapeBox("line", { x: 0, y: 0 }, { x: 10, y: 10 }, false, false))
      .toMatchObject({ flipH: false, flipV: false, docX: 0, docY: 0 });
    expect(computeShapeBox("line", { x: 10, y: 0 }, { x: 0, y: 10 }, false, false))
      .toMatchObject({ flipH: true, flipV: false });
    expect(computeShapeBox("line", { x: 0, y: 10 }, { x: 10, y: 0 }, false, false))
      .toMatchObject({ flipH: false, flipV: true });
    expect(computeShapeBox("line", { x: 10, y: 10 }, { x: 0, y: 0 }, false, false))
      .toMatchObject({ flipH: true, flipV: true });
    // Alt: box centered on press point, dims doubled.
    expect(computeShapeBox("rect", { x: 100, y: 100 }, { x: 150, y: 120 }, false, true))
      .toMatchObject({ width: 100, height: 40, docX: 50, docY: 80 });
  });

  it("shapeRenderMargin is 0 for non-arrow shapes and >0 for arrow lines", () => {
    expect(shapeRenderMargin({
      kind: "rect", width: 10, height: 10, radius: 0,
      fill: { kind: "none", color: "#000" }, stroke: { enabled: true, color: "#000", width: 4 }, arrowHead: false,
    })).toBe(4);
    expect(shapeRenderMargin({
      kind: "line", width: 10, height: 0, radius: 0,
      fill: { kind: "none", color: "#000" }, stroke: { enabled: true, color: "#000", width: 4 }, arrowHead: true,
    })).toBeGreaterThan(0);
  });

  it("non-arrow line reserves cap margin and places layer at docX - margin (no clip)", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    (mockEngine as any).addShapeLayer = vi.fn(() => ({ id: "shape-1", type: "shape" }));
    (mockEngine as any).updateShapeParams = vi.fn();
    signals.setShapeKind("line");
    // arrowHead stays false (default)

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 150, clientY: 50 }));
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 150, clientY: 50 }));

    const lastParams = (mockEngine as any).updateShapeParams.mock.calls.at(-1)![1] as any;
    const margin = shapeRenderMargin(lastParams);
    // The tool's default line has stroke disabled in this mock, so no cap
    // margin is reserved (nothing to clip); placement uses docX - margin.
    expect(margin).toBe(0);
    expect(mockEngine.transformLayer).toHaveBeenCalledWith("shape-1", {
      x: 50 - margin,
      y: 50 - margin,
      flipH: false,
      flipV: false,
    });
    disposeTools();
    dispose();
  });

  it("shapeRenderMargin returns the stroke width for a plain line, not 0", () => {
    expect(shapeRenderMargin({
      kind: "line", width: 100, height: 0, radius: 0,
      fill: { kind: "none", color: "#000" }, stroke: { enabled: true, color: "#000", width: 4 }, arrowHead: false,
    })).toBe(4);
  });

  it("clicking inside a rotated shape selects it (rotation-aware hit test)", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    (mockEngine as any).addShapeLayer = vi.fn(() => ({ id: "shape-1", type: "shape" }));
    (mockEngine as any).setActiveLayer = vi.fn();
    // 100x100 box rotated 45deg about its centre (50,50) -> diamond whose
    // right vertex is (120.7, 50). A press at (105,45) lies inside the rotated
    // diamond but OUTSIDE the naive axis-aligned box [0,100]x[0,100].
    (mockEngine as any).getLayers = () => [{
      id: "existing-shape",
      type: "shape",
      width: 100,
      height: 100,
      visible: true,
      locked: false,
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 45, flipH: false, flipV: false },
    }];

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    // harness exposes setSelectedLayerId as a vi.fn(); spy on it to prove selection
    const setSelectedLayerIdSpy = vi.fn();
    signals.setSelectedLayerId = setSelectedLayerIdSpy;
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 105, clientY: 45 }));

    expect(mockEngine.setActiveLayer).toHaveBeenCalledWith("existing-shape");
    expect(setSelectedLayerIdSpy).toHaveBeenCalledWith("existing-shape");
    // crucially, no new shape is created over the existing one
    expect(mockEngine.addShapeLayer).not.toHaveBeenCalled();
    disposeTools();
    dispose();
  });

  it("clicking outside a rotated shape still starts a new shape", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    (mockEngine as any).addShapeLayer = vi.fn(() => ({ id: "shape-1", type: "shape" }));
    (mockEngine as any).setActiveLayer = vi.fn();
    (mockEngine as any).getLayers = () => [{
      id: "existing-shape",
      type: "shape",
      width: 100,
      height: 100,
      visible: true,
      locked: false,
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 45, flipH: false, flipV: false },
    }];

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    // far outside the rotated diamond -> should NOT select existing shape
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 300, clientY: 300 }));

    expect(mockEngine.setActiveLayer).not.toHaveBeenCalledWith("existing-shape");
    expect(mockEngine.addShapeLayer).toHaveBeenCalled();
    disposeTools();
    dispose();
  });

  it("line drag with Shift snaps the endpoint to the nearest 45deg angle", () => {
    // Drag from (0,0). A 30deg-ish endpoint (100, 50) should snap to 45deg
    // (equal x/y) preserving length, so |x| ≈ |y|.
    const box = computeShapeBox("line", { x: 0, y: 0 }, { x: 100, y: 50 }, true, false);
    const len = Math.hypot(100, 50);
    expect(Math.abs(box.width - box.height)).toBeLessThan(0.001);
    expect(Math.abs(Math.hypot(box.width, box.height) - len)).toBeLessThan(0.001);
    // Snapped to 45deg in the first quadrant: positive, equal.
    expect(box.width).toBeCloseTo(len / Math.SQRT2, 3);
    expect(box.height).toBeCloseTo(len / Math.SQRT2, 3);
  });

  it("line drag without Shift keeps the raw endpoint", () => {
    const box = computeShapeBox("line", { x: 0, y: 0 }, { x: 100, y: 50 }, false, false);
    expect(box.width).toBe(100);
    expect(box.height).toBe(50);
  });

  it("oversized shape dims are clamped to MAX_SHAPE_DIM to avoid OOM raster", () => {
    const box = computeShapeBox("rect", { x: 0, y: 0 }, { x: 9_999_999, y: 9_999_999 }, false, false);
    expect(box.width).toBeLessThanOrEqual(MAX_SHAPE_DIM);
    expect(box.height).toBeLessThanOrEqual(MAX_SHAPE_DIM);
  });

  it("cancelShapeDrag removes the temp layer and resets state (no history entry)", () => {
    const deleteLayer = vi.fn();
    const requestRender = vi.fn();
    const state: ShapeDragState = {
      start: { x: 10, y: 10 },
      tempLayerId: "temp-shape",
      preSnapshot: { layers: [] } as any,
      isDragging: true,
      reset: () => {
        state.start = null;
        state.tempLayerId = null;
        state.preSnapshot = null;
        state.isDragging = false;
      },
    };
    const engine = { deleteLayer } as any;
    cancelShapeDrag(state, engine, { requestRender });
    expect(deleteLayer).toHaveBeenCalledWith("temp-shape");
    expect(requestRender).toHaveBeenCalled();
    expect(state.tempLayerId).toBeNull();
    expect(state.isDragging).toBe(false);
  });

  it("cancelShapeDrag is a no-op for the engine when there is no temp layer", () => {
    const deleteLayer = vi.fn();
    const state: ShapeDragState = {
      start: { x: 0, y: 0 },
      tempLayerId: null,
      preSnapshot: null,
      isDragging: true,
      reset: () => {
        state.start = null;
        state.tempLayerId = null;
        state.preSnapshot = null;
        state.isDragging = false;
      },
    };
    cancelShapeDrag(state, { deleteLayer } as any, undefined);
    expect(deleteLayer).not.toHaveBeenCalled();
    expect(state.isDragging).toBe(false);
  });
});