// apps/desktop/src/__tests__/eyedropper-regression.test.tsx
//
// Full-chain regression guard for the eyedropper tool. Unlike
// pointerToolRouting.test.tsx (which mocks `@/viewport/input-handler`
// to spy on routing) and input-handler-eyedropper.test.ts (which calls
// handlePointerDown directly), this test drives the REAL
// onCanvasPointerDown hook with the REAL handlePointerDown and asserts
// the foreground-color SIGNAL is actually mutated with the sampled pixel.
//
// It catches BOTH original failure modes at once:
//   1. a dispatcher early-return that silently no-ops eyedropper
//      (the 2026-07-11 bug)
//   2. a removed/changed eyedropper branch inside handlePointerDown

import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { describe, it, expect, vi, afterEach } from "vitest";
import { createMockEngine } from "./test-builders";
import { createMockEditorParams, createPointerTools, makePointerEvent } from "./pointerRoutingHarness";

vi.mock("../components/editor/dialogs/DialogProvider", () => ({
  useDialog: () => ({ confirm: vi.fn().mockResolvedValue(false) }),
}));

describe("eyedropper: real hook → real handler → fgColor signal", () => {
  afterEach(() => vi.restoreAllMocks());

  it("updates fgColor with the sampled pixel on canvas click", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("eyedropper");
    // Real engine whose samplePixel returns a deterministic color.
    mockEngine.samplePixel = vi.fn((): [number, number, number, number] => [128, 255, 64, 255]);

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

    tools.onCanvasPointerDown(makePointerEvent());

    // The sampled color [128,255,64] must flow through the real
    // handlePointerDown → setFgColor into the fgColor signal.
    expect(mockEngine.samplePixel).toHaveBeenCalled();
    expect((signals as any).fgColor()).toBe("#80ff40");

    disposeTools();
    dispose();
  });
});
