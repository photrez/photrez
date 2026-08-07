// apps/desktop/src/__tests__/colorPickerCanvas.test.tsx
//
// Wiring guard for the color-picker "pick from canvas" feature:
// when a non-modal color picker is open, a click on the canvas samples
// the pixel into the picker's active target (foreground/background) — a
// single click, no pipette/extra button. Uses the REAL onCanvasPointerDown
// hook (same harness as eyedropper-regression.test.tsx) so it catches
// a silently-no-op dispatcher, not just a mocked branch.

import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { describe, it, expect, vi, afterEach } from "vitest";
import { createMockEngine } from "./test-builders";
import { createMockEditorParams, createPointerTools, makePointerEvent } from "./pointerRoutingHarness";

vi.mock("../components/editor/dialogs/DialogProvider", () => ({
  useDialog: () => ({ confirm: vi.fn().mockResolvedValue(false) }),
}));

describe("color picker: canvas click samples into target", () => {
  afterEach(() => vi.restoreAllMocks());

  it("updates fgColor on canvas click when picker open (foreground)", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("move");
    mockEngine.samplePixel = vi.fn((): [number, number, number, number] => [10, 20, 30, 255]);

    mockUseEditor(signals);

    const { tools, dispose: disposeTools } = createPointerTools({
      getCanvasContainerRef: () => document.createElement("div"),
      getCanvasRef: () => document.createElement("canvas"),
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => false,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      commitBrushStroke: vi.fn(),
    });

    (signals as any).setColorPickerOpen(true);
    (signals as any).setColorPickerTarget("foreground");

    tools.onCanvasPointerDown(makePointerEvent());

    expect(mockEngine.samplePixel).toHaveBeenCalled();
    expect((signals as any).fgColor()).toBe("#0a141e");

    disposeTools();
    dispose();
  });

  it("updates bgColor when target is background", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("move");
    mockEngine.samplePixel = vi.fn((): [number, number, number, number] => [40, 50, 60, 255]);

    mockUseEditor(signals);

    const { tools, dispose: disposeTools } = createPointerTools({
      getCanvasContainerRef: () => document.createElement("div"),
      getCanvasRef: () => document.createElement("canvas"),
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => false,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      commitBrushStroke: vi.fn(),
    });

    (signals as any).setColorPickerOpen(true);
    (signals as any).setColorPickerTarget("background");

    tools.onCanvasPointerDown(makePointerEvent());

    expect(mockEngine.samplePixel).toHaveBeenCalled();
    expect((signals as any).bgColor()).toBe("#28323c");

    disposeTools();
    dispose();
  });

  it("ignores canvas click when picker is closed", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("move");
    mockEngine.samplePixel = vi.fn((): [number, number, number, number] => [10, 20, 30, 255]);

    mockUseEditor(signals);

    const { tools, dispose: disposeTools } = createPointerTools({
      getCanvasContainerRef: () => document.createElement("div"),
      getCanvasRef: () => document.createElement("canvas"),
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => false,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      commitBrushStroke: vi.fn(),
    });

    (signals as any).setColorPickerOpen(false);

    tools.onCanvasPointerDown(makePointerEvent());

    expect(mockEngine.samplePixel).not.toHaveBeenCalled();
    expect((signals as any).fgColor()).toBe("#000000");

    disposeTools();
    dispose();
  });

  it("drag-samples on pointer move while button held", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("move");
    mockEngine.samplePixel = vi.fn((): [number, number, number, number] => [200, 100, 50, 255]);

    mockUseEditor(signals);

    const { tools, dispose: disposeTools } = createPointerTools({
      getCanvasContainerRef: () => document.createElement("div"),
      getCanvasRef: () => document.createElement("canvas"),
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => false,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      commitBrushStroke: vi.fn(),
    });

    (signals as any).setColorPickerOpen(true);
    (signals as any).setColorPickerTarget("foreground");

    tools.onCanvasPointerMove(makePointerEvent({ buttons: 1 }));

    expect(mockEngine.samplePixel).toHaveBeenCalled();
    expect((signals as any).fgColor()).toBe("#c86432");

    disposeTools();
    dispose();
  });
});
