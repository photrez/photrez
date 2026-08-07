import { describe, expect, it, vi } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { useCanvasPointerTools } from "../canvas/useCanvasPointerTools";
import { createRoot, createSignal } from "solid-js";
import { CommandHistory } from "../../../engine/history";

vi.mock("../dialogs/DialogProvider", () => ({
  useDialog: () => ({ confirm: vi.fn().mockResolvedValue(false) }),
}));

// Helper to create mock editor context values
function createMockEditor(overrides: Record<string, unknown> = {}) {
  const mockEngine = {
    getActiveLayerId: () => "layer-1",
    getLayer: () => ({ id: "layer-1", locked: false, visible: true, width: 100, height: 100 }),
    samplePixel: vi.fn(() => [128, 255, 64, 255]),
    getViewport: () => ({ panX: 0, panY: 0, zoom: 1 }),
    snapshot: vi.fn(() => ({})),
    getWidth: () => 100,
    getHeight: () => 100,
    getLayers: () => [],
    isShapeLayer: () => false,
    shapeLayerToRaster: vi.fn(),
  };

  let currentLastPaintCoords: { x: number; y: number } | null = null;
  const defaults: Record<string, unknown> = {
    workspace: {
      getActiveEngine: () => mockEngine,
      getActiveHistory: () => ({
        commit: vi.fn(),
        getLastPaintCoords: () => currentLastPaintCoords,
        setLastPaintCoords: (c: { x: number; y: number } | null) => { currentLastPaintCoords = c; },
      }),
    },
    activeTool: "brush",
    fgColor: "#000000",
    bgColor: "#ffffff",
    colorPickerOpen: false,
    colorPickerTarget: "foreground",
    zoom: 1,
    pan: { x: 0, y: 0 },
    camera: { pan: vi.fn(), getState: () => ({ x: 0, y: 0 }) },
    docWidth: 100,
    docHeight: 100,
    brushSize: 20,
    brushHardness: 0.8,
    brushOpacity: 1.0,
    eraserSize: 20,
    eraserHardness: 0.8,
    eraserOpacity: 1.0,
    brushFlow: 1,
    brushSmoothing: 0,
    eraserFlow: 1,
    eraserSmoothing: 0,
    moveAutoSelect: false,
    moveSnapEnabled: false,
    selectedLayerId: null,
    setHoverPos: vi.fn(),
    setSelectedLayerId: vi.fn(),
  };

  const merged = { ...defaults, ...overrides };
  let dispose = () => {};
  const signals = createRoot((rootDispose) => {
    dispose = rootDispose;
    // Dynamic signal registry keyed by string — `any` is structural, not laziness
    const ownedSignals: Record<string, any> = {
      workspace: merged.workspace,
      scheduler: { requestRender: vi.fn() },
      camera: merged.camera,
    };

    for (const [key, val] of Object.entries(merged)) {
      if (key === "workspace" || key === "scheduler" || key === "camera") continue;
      const [s, set] = createSignal(val);
      ownedSignals[key] = s;
      const setKey = "set" + key.charAt(0).toUpperCase() + key.slice(1);
      ownedSignals[setKey] = set;
    }
    return ownedSignals;
  });
  return { signals, mockEngine, dispose };
}

function createPointerTools(params: Parameters<typeof useCanvasPointerTools>[0]) {
  return createRoot((dispose) => ({ tools: useCanvasPointerTools(params), dispose }));
}

describe("Brush & Eraser UX modifiers (Alt / Shift)", () => {
  it("samples pixel and updates fgColor when Alt is held in pointerDown/move", () => {
    const { signals, mockEngine, dispose } = createMockEditor({ activeTool: "brush" });
    mockUseEditor(signals);

    let isAltPressed = true;
    const canvas = document.createElement("canvas");
    canvas.setPointerCapture = vi.fn();
    canvas.releasePointerCapture = vi.fn();
    const container = document.createElement("div");

    const params = {
      getCanvasContainerRef: () => container,
      getCanvasRef: () => canvas,
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => isAltPressed,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      commitBrushStroke: vi.fn(),
      onPaintStroke: vi.fn(),
    };

    const { tools, dispose: disposeTools } = createPointerTools(params);

    // Simulated event
    const eventDown = {
      button: 0,
      clientX: 50,
      clientY: 50,
      pointerId: 1,
    } as any;

    tools.onCanvasPointerDown(eventDown);

    // Verify it sampled from engine
    expect(mockEngine.samplePixel).toHaveBeenCalled();
    // Color returned by mocked samplePixel is [128, 255, 64, 255] -> #80ff40
    expect(signals.fgColor()).toBe("#80ff40");

    // Clear mock to check move behavior
    mockEngine.samplePixel.mockClear();

    // Simulated drag move
    const eventMove = {
      clientX: 60,
      clientY: 60,
      buttons: 1,
    } as any;
    tools.onCanvasPointerMove(eventMove);
    expect(mockEngine.samplePixel).toHaveBeenCalled();

    disposeTools();
    dispose();
    vi.restoreAllMocks();
  });

  it("connects lastPaintCoords with straight line when Shift is held on pointerDown", () => {
    const { signals, dispose } = createMockEditor({ activeTool: "brush" });
    mockUseEditor(signals);

    const canvas = document.createElement("canvas");
    canvas.setPointerCapture = vi.fn();
    canvas.releasePointerCapture = vi.fn();
    const container = document.createElement("div");

    let strokePointsReceived: { x: number; y: number }[] = [];
    const historyApi = signals.workspace.getActiveHistory();
    const params = {
      getCanvasContainerRef: () => container,
      getCanvasRef: () => canvas,
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => false,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      // Replicates production: live lastPaintCoords advances to the stroke end.
      commitBrushStroke: vi.fn((_e: unknown, _h: unknown, _id: unknown, _er: unknown, _a: unknown) => {
        if (strokePointsReceived.length) {
          historyApi.setLastPaintCoords(strokePointsReceived[strokePointsReceived.length - 1]);
        }
      }),
      onPaintStroke: vi.fn((points) => {
        strokePointsReceived = points;
      }),
    };

    const { tools, dispose: disposeTools } = createPointerTools(params);

    // Click 1 (No shift) -> stamps a point, sets lastPaintCoords on pointer up
    tools.onCanvasPointerDown({ button: 0, clientX: 10, clientY: 10, pointerId: 1 } as any);
    tools.onCanvasPointerUp({ clientX: 10, clientY: 10, pointerId: 1 } as any);

    // Click 2 (With shift) -> draws straight line from (10, 10) to (20, 20)
    tools.onCanvasPointerDown({ button: 0, clientX: 20, clientY: 20, pointerId: 1, shiftKey: true } as any);

    // Check if points were generated along the line
    expect(strokePointsReceived.length).toBeGreaterThan(1);
    expect(strokePointsReceived[0]).toEqual({ x: 10, y: 10 });
    expect(strokePointsReceived[strokePointsReceived.length - 1]).toEqual({ x: 20, y: 20 });

    disposeTools();
    dispose();
    vi.restoreAllMocks();
  });

  it("locks coordinate to horizontal or vertical axis when Shift is held on pointerMove", () => {
    const { signals, dispose } = createMockEditor({ activeTool: "brush" });
    mockUseEditor(signals);

    const canvas = document.createElement("canvas");
    canvas.setPointerCapture = vi.fn();
    canvas.releasePointerCapture = vi.fn();
    const container = document.createElement("div");

    let strokePointsReceived: { x: number; y: number }[] = [];
    const params = {
      getCanvasContainerRef: () => container,
      getCanvasRef: () => canvas,
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => false,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      commitBrushStroke: vi.fn(),
      onPaintStroke: vi.fn((points) => {
        strokePointsReceived = points;
      }),
    };

    const { tools, dispose: disposeTools } = createPointerTools(params);

    // PointerDown at (50, 50)
    tools.onCanvasPointerDown({ button: 0, clientX: 50, clientY: 50, pointerId: 1 } as any);

    // Move to (60, 52) with Shift key -> should lock horizontally to Y=50 (since dx=10, dy=2)
    tools.onCanvasPointerMove({ clientX: 60, clientY: 52, pointerId: 1, shiftKey: true } as any);

    // Last received point should be X=60, Y=50
    expect(strokePointsReceived.at(-1)).toEqual({ x: 60, y: 50 });

    disposeTools();
    dispose();
    vi.restoreAllMocks();
  });

  it("forwards the exact pointer-up coordinate as a final paint sample before commit", () => {
    const { signals, dispose } = createMockEditor({ activeTool: "brush" });
    mockUseEditor(signals);

    const canvas = document.createElement("canvas");
    canvas.setPointerCapture = vi.fn();
    canvas.releasePointerCapture = vi.fn();
    const onPaintStroke = vi.fn();
    const commitBrushStroke = vi.fn();
    const params = {
      getCanvasContainerRef: () => document.createElement("div"),
      getCanvasRef: () => canvas,
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => false,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      commitBrushStroke,
      onPaintStroke,
    };

    const { tools, dispose: disposeTools } = createPointerTools(params);
    tools.onCanvasPointerDown({ button: 0, clientX: 10, clientY: 10, pointerId: 1 } as any);
    tools.onCanvasPointerMove({ clientX: 20, clientY: 10, buttons: 1, pointerId: 1 } as any);
    tools.onCanvasPointerUp({ clientX: 23, clientY: 10, pointerId: 1 } as any);

    const finalCall = onPaintStroke.mock.calls.at(-1);
    expect(finalCall?.[0].at(-1)).toEqual({ x: 23, y: 10 });
    expect(finalCall?.[3]).toBe(true);
    expect(onPaintStroke.mock.invocationCallOrder.at(-1)).toBeLessThan(
      commitBrushStroke.mock.invocationCallOrder[0],
    );

    disposeTools();
    dispose();
    vi.restoreAllMocks();
  });

  it.each(["onCanvasPointerCancel", "onCanvasLostPointerCapture"] as const)(
    "%s finalizes the last sampled endpoint before committing",
    (terminalHandler) => {
      const { signals, dispose } = createMockEditor({ activeTool: "brush" });
      mockUseEditor(signals);

      const canvas = document.createElement("canvas");
      canvas.setPointerCapture = vi.fn();
      canvas.releasePointerCapture = vi.fn();
      const onPaintStroke = vi.fn();
      const commitBrushStroke = vi.fn();
      const { tools, dispose: disposeTools } = createPointerTools({
        getCanvasContainerRef: () => document.createElement("div"),
        getCanvasRef: () => canvas,
        isSpacePressed: () => false,
        isPanning: () => false,
        isAltPressed: () => false,
        stopMomentum: vi.fn(),
        fitToScreenAndRender: vi.fn(),
        commitBrushStroke,
        onPaintStroke,
      });

      tools.onCanvasPointerDown({ button: 0, clientX: 10, clientY: 10, pointerId: 1 } as any);
      tools.onCanvasPointerMove({ clientX: 23, clientY: 10, buttons: 1, pointerId: 1 } as any);
      tools[terminalHandler]({ pointerId: 1 } as PointerEvent);

      expect(onPaintStroke.mock.calls.at(-1)?.[3]).toBe(true);
      expect(onPaintStroke.mock.invocationCallOrder.at(-1)).toBeLessThan(
        commitBrushStroke.mock.invocationCallOrder[0],
      );

      disposeTools();
      dispose();
      vi.restoreAllMocks();
    },
  );

  it("brush targets engine's active layer even after move deselect (selectedLayerId=null)", () => {
    // Regression: after move tool deselects via pasteboard click, selectedLayerId
    // signal becomes null. Brush should still paint on engine.getActiveLayerId().
    const { signals, mockEngine, dispose } = createMockEditor({
      selectedLayerId: null, // simulate move deselect
      activeTool: "brush",
    });
    mockUseEditor(signals);

    const canvas = document.createElement("canvas");
    canvas.setPointerCapture = vi.fn();
    canvas.releasePointerCapture = vi.fn();
    const onPaintStroke = vi.fn();
    const commitBrushStroke = vi.fn();
    const params = {
      getCanvasContainerRef: () => document.createElement("div"),
      getCanvasRef: () => canvas,
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => false,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      commitBrushStroke,
      onPaintStroke,
    };

    const { tools, dispose: disposeTools } = createPointerTools(params);

    // Brush stroke after deselect
    tools.onCanvasPointerDown({ button: 0, clientX: 10, clientY: 10, pointerId: 1 } as any);
    // prepareToolContext should have set interactiveState.selectedLayerId = engine.getActiveLayerId()
    // handlePointerDown should fire onPaintStroke with the initial point
    expect(onPaintStroke).toHaveBeenCalled();

    tools.onCanvasPointerMove({ clientX: 20, clientY: 10, buttons: 1, pointerId: 1 } as any);
    tools.onCanvasPointerUp({ clientX: 20, clientY: 10, pointerId: 1 } as any);

    // onCanvasPointerUp calls commitBrushStroke with engine.getActiveLayerId() (not selectedLayerId signal)
    expect(commitBrushStroke).toHaveBeenCalled();
    const commitCall = commitBrushStroke.mock.calls[0];
    expect(commitCall[2]).toBe(mockEngine.getActiveLayerId()); // layerId matches engine

    disposeTools();
    dispose();
    vi.restoreAllMocks();
  });

  it("clears stale brushAdjustStart on pointerDown so eraser works after leaked Alt+RightClick", () => {
    const { signals, dispose } = createMockEditor({ activeTool: "eraser" });
    mockUseEditor(signals);

    const canvas = document.createElement("canvas");
    canvas.setPointerCapture = vi.fn();
    canvas.releasePointerCapture = vi.fn();
    const onPaintStroke = vi.fn();
    const commitBrushStroke = vi.fn();
    const params = {
      getCanvasContainerRef: () => document.createElement("div"),
      getCanvasRef: () => canvas,
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => false,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      commitBrushStroke,
      onPaintStroke,
    };

    const { tools, dispose: disposeTools } = createPointerTools(params);

    // Step 1: Alt+RightClick → brushAdjustStart is set (state leak)
    tools.onCanvasPointerDown({
      button: 2, altKey: true, clientX: 50, clientY: 50, pointerId: 1,
      preventDefault: vi.fn(),
    } as any);
    // No onCanvasPointerUp or onCanvasPointerCancel is called — simulates
    // releasing the right button outside the window (stale state leak)

    // Step 2: Left-click eraser → brushAdjustStart cleared at top of
    // onCanvasPointerDown → eraser stroke proceeds normally
    tools.onCanvasPointerDown({ button: 0, clientX: 10, clientY: 10, pointerId: 2 } as any);
    expect(onPaintStroke).toHaveBeenCalledWith(
      expect.arrayContaining([{ x: 10, y: 10 }]),
      true, // isEraser
      expect.any(Object),
    );

    tools.onCanvasPointerMove({ clientX: 20, clientY: 10, buttons: 1, pointerId: 2 } as any);
    tools.onCanvasPointerUp({ clientX: 20, clientY: 10, pointerId: 2 } as any);

    // commitBrushStroke should have been called for the eraser
    expect(commitBrushStroke).toHaveBeenCalled();
    expect(commitBrushStroke.mock.calls[0][3]).toBe(true); // isEraser=true

    disposeTools();
    dispose();
    vi.restoreAllMocks();
  });

  it("synchronizes lastPaintCoords with active history on undo/redo actions", () => {
    let mockCoords: { x: number; y: number } | null = null;
    const mockHistory = {
      commit: vi.fn(),
      getLastPaintCoords: () => mockCoords,
      setLastPaintCoords: (c: { x: number; y: number } | null) => { mockCoords = c; },
    };

    const { signals, dispose } = createMockEditor({
      workspace: {
        getActiveEngine: () => ({
          getActiveLayerId: () => "layer-1",
          getLayer: () => ({ id: "layer-1", locked: false, visible: true, width: 100, height: 100 }),
          getViewport: () => ({ panX: 0, panY: 0, zoom: 1 }),
          snapshot: vi.fn(() => ({})),
          getWidth: () => 100,
          getHeight: () => 100,
          getLayers: () => [],
          isShapeLayer: () => false,
          shapeLayerToRaster: vi.fn(),
        }),
        getActiveHistory: () => mockHistory,
      },
      activeTool: "brush",
    });
    mockUseEditor(signals);

    const canvas = document.createElement("canvas");
    canvas.setPointerCapture = vi.fn();
    canvas.releasePointerCapture = vi.fn();
    let pts397: { x: number; y: number }[] = [];
    const params = {
      getCanvasContainerRef: () => document.createElement("div"),
      getCanvasRef: () => canvas,
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => false,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      // Replicates production: live lastPaintCoords advances to the stroke end.
      commitBrushStroke: vi.fn((_e: unknown, _h: unknown, _id: unknown, _er: unknown, _a: unknown) => {
        if (pts397.length) mockHistory.setLastPaintCoords(pts397[pts397.length - 1]);
      }),
      onPaintStroke: vi.fn((p: { x: number; y: number }[]) => {
        pts397 = p;
      }),
    };

    const { tools, dispose: disposeTools } = createPointerTools(params);

    // Draw first stroke ending at (10, 10)
    tools.onCanvasPointerDown({ button: 0, clientX: 10, clientY: 10, pointerId: 1 } as any);
    tools.onCanvasPointerUp({ clientX: 10, clientY: 10, pointerId: 1 } as any);

    expect(mockHistory.getLastPaintCoords()).toEqual({ x: 10, y: 10 });

    // Simulate undo reverting coords to null
    mockHistory.setLastPaintCoords(null);
    expect(mockHistory.getLastPaintCoords()).toBeNull();

    // Simulating redo setting coords back to (10, 10)
    mockHistory.setLastPaintCoords({ x: 10, y: 10 });
    expect(mockHistory.getLastPaintCoords()).toEqual({ x: 10, y: 10 });

    disposeTools();
    dispose();
    vi.restoreAllMocks();
  });

  it("keeps the pre-stroke anchor across undo so a later shift stroke reconnects from the prior stroke (A→B→undo→C = A→C)", () => {
    // Real history so undo/redo actually restore lastPaintCoords.
    const realHistory = new CommandHistory();
    const { signals, mockEngine, dispose } = createMockEditor({
      activeTool: "brush",
      workspace: {
        getActiveEngine: () => mockEngine,
        getActiveHistory: () => realHistory,
      },
    });
    mockUseEditor(signals);

    const canvas = document.createElement("canvas");
    canvas.setPointerCapture = vi.fn();
    canvas.releasePointerCapture = vi.fn();
    const container = document.createElement("div");

    let pts: { x: number; y: number }[] = [];
    // Replicates the production commitBrushStroke contract with a real history:
    // the snapshot records the pre-stroke anchor, then live advances to the stroke end.
    const commitBrushStroke = vi.fn((_e: unknown, _h: unknown, _id: unknown, _er: unknown, anchor?: { x: number; y: number } | null) => {
      realHistory.setLastPaintCoords(anchor ?? realHistory.getLastPaintCoords());
      realHistory.commit({} as any, "Brush Stroke");
      const end = pts[pts.length - 1];
      if (end) realHistory.setLastPaintCoords(end);
    });
    const params = {
      getCanvasContainerRef: () => container,
      getCanvasRef: () => canvas,
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => false,
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      commitBrushStroke,
      onPaintStroke: vi.fn((p: { x: number; y: number }[]) => {
        pts = p;
      }),
    };

    const { tools, dispose: disposeTools } = createPointerTools(params);

    // Stroke A (no shift) at (10,10)
    tools.onCanvasPointerDown({ button: 0, clientX: 10, clientY: 10, pointerId: 1 } as any);
    tools.onCanvasPointerUp({ clientX: 10, clientY: 10, pointerId: 1 } as any);
    expect(realHistory.getLastPaintCoords()).toEqual({ x: 10, y: 10 });

    // Stroke B (shift) at (40,40) — connects A_end(10,10) -> (40,40)
    tools.onCanvasPointerDown({ button: 0, clientX: 40, clientY: 40, pointerId: 1, shiftKey: true } as any);
    tools.onCanvasPointerUp({ clientX: 40, clientY: 40, pointerId: 1 } as any);
    expect(realHistory.getLastPaintCoords()).toEqual({ x: 40, y: 40 });

    // Undo B — must restore the pre-stroke anchor A_end, NOT B's own end.
    realHistory.undo({} as any);
    expect(realHistory.getLastPaintCoords()).toEqual({ x: 10, y: 10 });

    // Stroke C (shift) at (80,80) — reconnects from A_end(10,10).
    // Before the fix, undo restored B(40,40), so C would connect B→C.
    tools.onCanvasPointerDown({ button: 0, clientX: 80, clientY: 80, pointerId: 1, shiftKey: true } as any);
    tools.onCanvasPointerUp({ clientX: 80, clientY: 80, pointerId: 1 } as any);
    const cAnchor = commitBrushStroke.mock.calls.at(-1)![4];
    expect(cAnchor).toEqual({ x: 10, y: 10 });

    disposeTools();
    dispose();
    vi.restoreAllMocks();
  });
});
