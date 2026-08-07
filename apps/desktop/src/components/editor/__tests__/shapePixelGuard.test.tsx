// apps/desktop/src/components/editor/__tests__/shapePixelGuard.test.tsx
//
// Wiring contract for the pixel-tool guard (Task 8, shape-tool plan): a
// brush/eraser stroke on a shape layer must NOT start — it shows a confirm
// dialog instead. "Confirm → convert → proceed"; reject → no-op. A stroke on
// a non-shape layer proceeds normally without the dialog.

import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createMockEditorParams, createPointerTools, makePointerEvent } from "../../../__tests__/pointerRoutingHarness";

// Mutable confirm holder so each test resolves the dialog differently.
const { confirmSpy } = vi.hoisted(() => ({ confirmSpy: vi.fn() }));
vi.mock("../dialogs/DialogProvider", () => ({
  useDialog: () => ({ confirm: confirmSpy }),
}));

// Mock input-handler so we can assert the stroke path wraps normally for a
// non-shape layer (guard must not swallow ordinary strokes).
const { handlePointerDownSpy } = vi.hoisted(() => ({ handlePointerDownSpy: vi.fn() }));
vi.mock("@/viewport/input-handler", () => ({
  handlePointerDown: handlePointerDownSpy,
  handlePointerMove: vi.fn(),
  handlePointerUp: vi.fn(),
  ToolType: null,
  ToolContext: null,
}));

beforeEach(() => {
  handlePointerDownSpy.mockClear();
  confirmSpy.mockReset();
});

afterEach(() => {
  vi.restoreAllMocks();
});

function createTool(signals: any) {
  mockUseEditor(signals);
  const { tools, dispose } = createPointerTools({
    getCanvasContainerRef: () => document.createElement("div"),
    getCanvasRef: () => document.createElement("canvas"),
    isSpacePressed: () => false,
    isPanning: () => false,
    isAltPressed: () => false,
    stopMomentum: vi.fn(),
    fitToScreenAndRender: vi.fn(),
    commitBrushStroke: vi.fn(),
  });
  return { tools, dispose };
}

async function flush() {
  await Promise.resolve();
  await Promise.resolve();
}

describe("pixel-tool guard on shape layers", () => {
  it("brush on a shape layer with confirm=true converts to raster, no stroke", async () => {
    const { signals, mockEngine } = createMockEditorParams("brush");
    mockEngine.isShapeLayer = vi.fn(() => true);
    mockEngine.shapeLayerToRaster = vi.fn();
    confirmSpy.mockResolvedValue(true);

    const history = (signals.workspace as any).getActiveHistory();
    const { tools, dispose } = createTool(signals);

    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
    await flush();

    expect(confirmSpy).toHaveBeenCalledTimes(1);
    expect(mockEngine.shapeLayerToRaster).toHaveBeenCalledWith("layer-1");
    expect(history.commit).toHaveBeenCalled();
    expect(handlePointerDownSpy).not.toHaveBeenCalled();

    dispose();
  });

  it("brush on a shape layer with confirm=false is a no-op", async () => {
    const { signals, mockEngine } = createMockEditorParams("brush");
    mockEngine.isShapeLayer = vi.fn(() => true);
    mockEngine.shapeLayerToRaster = vi.fn();
    confirmSpy.mockResolvedValue(false);

    const history = (signals.workspace as any).getActiveHistory();
    const { tools, dispose } = createTool(signals);

    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
    await Promise.resolve();

    expect(confirmSpy).toHaveBeenCalledTimes(1);
    expect(mockEngine.shapeLayerToRaster).not.toHaveBeenCalled();
    expect(history.commit).not.toHaveBeenCalled();
    expect(handlePointerDownSpy).not.toHaveBeenCalled();

    dispose();
  });

  it("brush on a NON-shape layer does not trigger the dialog and strokes proceed", () => {
    const { signals, mockEngine } = createMockEditorParams("brush");
    mockEngine.isShapeLayer = vi.fn(() => false);
    mockEngine.shapeLayerToRaster = vi.fn();
    const history = (signals.workspace as any).getActiveHistory();

    const { tools, dispose } = createTool(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));

    expect(confirmSpy).not.toHaveBeenCalled();
    expect(mockEngine.shapeLayerToRaster).not.toHaveBeenCalled();
    expect(history.commit).not.toHaveBeenCalled();
    expect(handlePointerDownSpy).toHaveBeenCalled();

    dispose();
  });
});