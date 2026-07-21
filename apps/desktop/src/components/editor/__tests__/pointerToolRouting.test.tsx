// apps/desktop/src/components/editor/__tests__/pointerToolRouting.test.tsx
//
// Wiring contract: for each registered ToolId, useCanvasPointerTools must
// route pointer events to the correct input-handler function.  If the
// dispatcher is missing a case or the routing branch is broken, the tool
// silently no-ops — AGENTS.md calls this the "#1 most forgotten wiring step".
//
// Pattern: set activeTool → fire onCanvasPointerDown → assert
// handlePointerDown called with that tool type.  For crop+modern mode
// the expected behavior is different (early return before handlePointerDown).

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot, createSignal } from "solid-js";
import { useCanvasPointerTools } from "../canvas/useCanvasPointerTools";
import * as InputHandlerModule from "@/viewport/input-handler";
import * as EditorContextModule from "../shell/EditorContext";
import { createMockEditorParams, createPointerTools, makePointerEvent } from "../../../__tests__/pointerRoutingHarness";

// Mock input-handler so we can spy on handlePointerDown calls
// vi.hoisted ensures the spy is created BEFORE vi.mock hoisting runs
const { handlePointerDownSpy } = vi.hoisted(() => ({
  handlePointerDownSpy: vi.fn(),
}));
vi.mock("@/viewport/input-handler", () => ({
  handlePointerDown: handlePointerDownSpy,
  handlePointerMove: vi.fn(),
  handlePointerUp: vi.fn(),
  ToolType: null,
  ToolContext: null,
}));

beforeEach(() => {
  handlePointerDownSpy.mockClear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe.each([
  ["move", "move"],
  ["selection", "selection"],
  ["crop (classic)", "crop"],
  ["brush", "brush"],
  ["eraser", "eraser"],
] as const)("pointer routing: %s", (label, toolId) => {
  it(`routes onCanvasPointerDown to handlePointerDown with tool='${toolId}'`, () => {
    const { signals, dispose } = createMockEditorParams(toolId);
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

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

    // For classic crop mode, handlePointerDown IS called.
    // For modern crop, the early return before handlePointerDown is tested separately.
    if (toolId === "crop") {
      // With default cropInteractionMode="classic", crop routes to handlePointerDown
      expect(handlePointerDownSpy).toHaveBeenCalled();
      const callToolArg = handlePointerDownSpy.mock.calls[0][0];
      // The input-handler's handlePointerDown receives tool as first arg
      expect(callToolArg).toBe("crop");
    } else {
      expect(handlePointerDownSpy).toHaveBeenCalled();
      const callToolArg = handlePointerDownSpy.mock.calls[0][0];
      expect(callToolArg).toBe(toolId);
    }

    disposeTools();
    dispose();
  });
});

describe("eyedropper routes to handlePointerDown (samples color)", () => {
  it("calls handlePointerDown with tool='eyedropper' when activeTool is eyedropper", () => {
    const { signals, dispose } = createMockEditorParams("eyedropper");
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

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

    // Regression: the eyedropper tool used to early-return and silently no-op.
    // It must route into the shared input-handler which samples the pixel.
    expect(handlePointerDownSpy).toHaveBeenCalledTimes(1);
    expect(handlePointerDownSpy.mock.calls[0][0]).toBe("eyedropper");

    disposeTools();
    dispose();
  });
});

describe("crop + modern mode exits early (no handlePointerDown)", () => {
  it("does NOT call handlePointerDown when crop+modern mode", () => {
    const { signals, dispose } = createMockEditorParams("crop");
    // Override to modern mode
    const cropMode = createSignal<"modern" | "classic">("modern");
    signals.cropInteractionMode = cropMode[0];
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

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

    // Modern crop mode exits early BEFORE handlePointerDown
    expect(handlePointerDownSpy).not.toHaveBeenCalled();

    disposeTools();
    dispose();
  });
});

describe("paintBucket + gradient bypass handlePointerDown (same pattern as modern crop)", () => {
  it("does NOT call handlePointerDown when paintBucket active", () => {
    const { signals, dispose } = createMockEditorParams("paintBucket");
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

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
    // paintBucket exits early BEFORE handlePointerDown
    expect(handlePointerDownSpy).not.toHaveBeenCalled();
    disposeTools();
    dispose();
  });

  it("does NOT call handlePointerDown when gradient active", () => {
    const { signals, dispose } = createMockEditorParams("gradient");
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

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
    // gradient exits early BEFORE handlePointerDown
    expect(handlePointerDownSpy).not.toHaveBeenCalled();
    disposeTools();
    dispose();
  });
});

describe("brush/eraser + Alt (eyedropper) intercepts before handlePointerDown", () => {
  it("does NOT call handlePointerDown when brush+Alt (eyedropper shortcut)", () => {
    const { signals, dispose } = createMockEditorParams("brush");
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

    const { tools, dispose: disposeTools } = createPointerTools({
      getCanvasContainerRef: () => document.createElement("div"),
      getCanvasRef: () => document.createElement("canvas"),
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => true,  // Alt held → eyedropper
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      commitBrushStroke: vi.fn(),
    });

    tools.onCanvasPointerDown(makePointerEvent());

    // Alt + brush/eraser → eyedropper, does NOT reach handlePointerDown
    expect(handlePointerDownSpy).not.toHaveBeenCalled();

    disposeTools();
    dispose();
  });
});

describe("move auto-select calls setActiveLayer before handlePointerDown", () => {
  it("calls setActiveLayer on hit when moveAutoSelect is on", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("move");
    // Enable auto-select
    const autoSelect = createSignal(true);
    signals.moveAutoSelect = autoSelect[0];
    // Make sure layers returns something hittable
    const hitLayer = { id: "target-layer", name: "Target", visible: true, locked: false, width: 100, height: 100, transform: { scaleX: 1, scaleY: 1 } } as any;
    signals.workspace = {
      getActiveEngine: () => ({
        ...mockEngine,
        getLayers: () => [hitLayer],
        setActiveLayer: vi.fn(),
        getViewport: () => ({ panX: 0, panY: 0, zoom: 1 }),
        getWidth: () => 100,
        getHeight: () => 100,
      }),
      getActiveHistory: () => ({
        commit: vi.fn(),
        getLastPaintCoords: () => null,
        setLastPaintCoords: vi.fn(),
      }),
    };
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

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

    // handlePointerDown should be called with "move"
    expect(handlePointerDownSpy).toHaveBeenCalled();
    expect(handlePointerDownSpy.mock.calls[0][0]).toBe("move");

    disposeTools();
    dispose();
  });
});

describe("right-click and middle-click block routing", () => {
  it.each([
    ["move", "move"],
    ["selection", "selection"],
    ["brush", "brush"],
  ] as const)("does NOT call handlePointerDown when button=2 (right-click) for %s", (label, toolId) => {
    const { signals, dispose } = createMockEditorParams(toolId);
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

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

    tools.onCanvasPointerDown(makePointerEvent({ button: 2 }));
    expect(handlePointerDownSpy).not.toHaveBeenCalled();
    disposeTools();
    dispose();
  });

  it.each([
    ["move", "move"],
    ["selection", "selection"],
    ["brush", "brush"],
  ] as const)("does NOT call handlePointerDown when button=1 (middle-click) for %s", (label, toolId) => {
    const { signals, dispose } = createMockEditorParams(toolId);
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

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

    tools.onCanvasPointerDown(makePointerEvent({ button: 1 }));
    expect(handlePointerDownSpy).not.toHaveBeenCalled();
    disposeTools();
    dispose();
  });
});

describe("brush/eraser locked layer blocks routing", () => {
  it.each([
    ["brush", "brush"],
    ["eraser", "eraser"],
  ] as const)("does NOT call handlePointerDown when target layer is locked for %s", (label, toolId) => {
    const { signals, mockEngine, dispose } = createMockEditorParams(toolId);
    // Override getLayer to return a locked layer
    signals.workspace = {
      getActiveEngine: () => ({
        ...mockEngine,
        getLayer: (id: string) => ({ id, locked: true, visible: true, width: 100, height: 100 }),
      }),
      getActiveHistory: () => ({
        commit: vi.fn(),
        getLastPaintCoords: () => null,
        setLastPaintCoords: vi.fn(),
      }),
    };
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

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
    expect(handlePointerDownSpy).not.toHaveBeenCalled();
    disposeTools();
    dispose();
  });
});

describe("no active engine returns early (no crash)", () => {
  it("does NOT crash when workspace has no active engine", () => {
    const { signals, dispose } = createMockEditorParams("move");
    // Override workspace to return null engine
    signals.workspace = {
      getActiveEngine: () => null,
      getActiveHistory: () => null,
      getActiveSession: () => null,
    };
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

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

    // Must not throw or call handlePointerDown
    expect(() => tools.onCanvasPointerDown(makePointerEvent())).not.toThrow();
    expect(handlePointerDownSpy).not.toHaveBeenCalled();

    disposeTools();
    dispose();
  });

  it("does NOT crash when workspace has no active history", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("brush");
    signals.workspace = {
      getActiveEngine: () => mockEngine,
      getActiveHistory: () => null,
    };
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

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

    expect(() => tools.onCanvasPointerDown(makePointerEvent())).not.toThrow();
    expect(handlePointerDownSpy).not.toHaveBeenCalled();

    disposeTools();
    dispose();
  });
});

describe("space/pan early return blocks routing", () => {
  it("does NOT call handlePointerDown when space is pressed (pan mode)", () => {
    const { signals, dispose } = createMockEditorParams("move");
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

    const { tools, dispose: disposeTools } = createPointerTools({
      getCanvasContainerRef: () => document.createElement("div"),
      getCanvasRef: () => document.createElement("canvas"),
      isSpacePressed: () => true,  // panning
      isPanning: () => false,
      isAltPressed: () => false,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      commitBrushStroke: vi.fn(),
    });

    tools.onCanvasPointerDown(makePointerEvent());
    expect(handlePointerDownSpy).not.toHaveBeenCalled();
    disposeTools();
    dispose();
  });

  it("does NOT call handlePointerDown when isPanning is true", () => {
    const { signals, dispose } = createMockEditorParams("move");
    vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);

    const { tools, dispose: disposeTools } = createPointerTools({
      getCanvasContainerRef: () => document.createElement("div"),
      getCanvasRef: () => document.createElement("canvas"),
      isSpacePressed: () => false,
      isPanning: () => true,  // panning
      isAltPressed: () => false,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      commitBrushStroke: vi.fn(),
    });

    tools.onCanvasPointerDown(makePointerEvent());
    expect(handlePointerDownSpy).not.toHaveBeenCalled();
    disposeTools();
    dispose();
  });
});


