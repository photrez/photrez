// apps/desktop/src/components/editor/__tests__/textPixelGuard.test.tsx
//
// Wiring contract for the text-layer pixel guard (plan Task 8, mirror of
// shapePixelGuard): a brush/eraser/paintBucket/gradient stroke on a TEXT layer
// must NOT act directly on the bitmap — it shows a confirm dialog instead.
// "Confirm → convert → proceed"; reject → no-op. A stroke on a non-text layer
// proceeds normally without the dialog.

import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createMockEditorParams, createPointerTools, makePointerEvent } from "../../../__tests__/pointerRoutingHarness";

// Mutable confirm holder so each test resolves the dialog differently.
const { confirmSpy } = vi.hoisted(() => ({ confirmSpy: vi.fn() }));
vi.mock("../dialogs/DialogProvider", () => ({
  useDialog: () => ({ confirm: confirmSpy }),
}));

// Mock input-handler so we can assert the stroke path wraps normally for a
// non-text layer (guard must not swallow ordinary strokes).
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

describe("pixel-tool guard on text layers", () => {
  it("brush on a text layer with confirm=true converts to raster, no stroke", async () => {
    const { signals, mockEngine } = createMockEditorParams("brush");
    mockEngine.isTextLayer = vi.fn(() => true);
    mockEngine.textLayerToRaster = vi.fn();
    confirmSpy.mockResolvedValue(true);

    const history = (signals.workspace as any).getActiveHistory();
    const { tools, dispose } = createTool(signals);

    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
    await flush();

    expect(confirmSpy).toHaveBeenCalledTimes(1);
    expect(confirmSpy.mock.calls[0][0].title).toBe("Convert text to pixels?");
    expect(mockEngine.textLayerToRaster).toHaveBeenCalledWith("layer-1");
    expect(history.commit).toHaveBeenCalledWith(expect.anything(), "Convert Text to Pixels");
    expect(handlePointerDownSpy).not.toHaveBeenCalled();

    dispose();
  });

  it("brush on a text layer with confirm=false is a no-op", async () => {
    const { signals, mockEngine } = createMockEditorParams("brush");
    mockEngine.isTextLayer = vi.fn(() => true);
    mockEngine.textLayerToRaster = vi.fn();
    confirmSpy.mockResolvedValue(false);

    const history = (signals.workspace as any).getActiveHistory();
    const { tools, dispose } = createTool(signals);

    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
    await flush();

    expect(confirmSpy).toHaveBeenCalledTimes(1);
    expect(mockEngine.textLayerToRaster).not.toHaveBeenCalled();
    expect(history.commit).not.toHaveBeenCalled();
    expect(handlePointerDownSpy).not.toHaveBeenCalled();

    dispose();
  });

  it("brush on a NON-text layer does not trigger the dialog and strokes proceed", () => {
    const { signals, mockEngine } = createMockEditorParams("brush");
    mockEngine.isTextLayer = vi.fn(() => false);
    mockEngine.textLayerToRaster = vi.fn();
    const history = (signals.workspace as any).getActiveHistory();

    const { tools, dispose } = createTool(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));

    expect(confirmSpy).not.toHaveBeenCalled();
    expect(mockEngine.textLayerToRaster).not.toHaveBeenCalled();
    expect(history.commit).not.toHaveBeenCalled();
    expect(handlePointerDownSpy).toHaveBeenCalled();

    dispose();
  });

  it("paintBucket on a text layer shows the dialog and converts on confirm (no silent fill)", async () => {
    const { signals, mockEngine } = createMockEditorParams("paintBucket");
    mockEngine.isTextLayer = vi.fn(() => true);
    mockEngine.textLayerToRaster = vi.fn();
    confirmSpy.mockResolvedValue(true);

    const history = (signals.workspace as any).getActiveHistory();
    const { tools, dispose } = createTool(signals);

    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
    await flush();

    expect(confirmSpy).toHaveBeenCalledTimes(1);
    expect(mockEngine.textLayerToRaster).toHaveBeenCalledWith("layer-1");
    expect(history.commit).toHaveBeenCalled();
    expect(mockEngine.setLayerImageBitmap).not.toHaveBeenCalled();

    dispose();
  });

  it("gradient on a text layer shows the dialog and is a no-op on reject", async () => {
    const { signals, mockEngine } = createMockEditorParams("gradient");
    mockEngine.isTextLayer = vi.fn(() => true);
    mockEngine.textLayerToRaster = vi.fn();
    confirmSpy.mockResolvedValue(false);

    const history = (signals.workspace as any).getActiveHistory();
    const { tools, dispose } = createTool(signals);

    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
    await flush();

    expect(confirmSpy).toHaveBeenCalledTimes(1);
    expect(mockEngine.textLayerToRaster).not.toHaveBeenCalled();
    expect(history.commit).not.toHaveBeenCalled();

    dispose();
  });

  it("paintBucket on a NON-text layer bypasses the dialog and proceeds to fill", () => {
    const { signals, mockEngine } = createMockEditorParams("paintBucket");
    mockEngine.isTextLayer = vi.fn(() => false);
    const readBitmapSpy = vi.spyOn(mockEngine, "getLayerImageBitmap");

    const { tools, dispose } = createTool(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));

    expect(confirmSpy).not.toHaveBeenCalled();
    // applyPaintBucketFill ran and read the layer bitmap (guard did not swallow it)
    expect(readBitmapSpy).toHaveBeenCalled();

    dispose();
  });
});
