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
      { x: 50, y: 80 },
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
    (mockEngine as any).addShapeLayer = vi.fn(() => ({ id: "shape-1", type: "shape" }));
    (mockEngine as any).updateShapeParams = vi.fn();
    (mockEngine as any).isShapeLayer = vi.fn(() => true);
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
});