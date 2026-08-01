import { describe, it, expect, vi } from "vitest";
import { handlePointerDown, handlePointerMove, handlePointerUp } from "../viewport/input-handler";
import type { DocumentEngine } from "../engine/document";
import type { CommandHistory } from "../engine/history";
import type { SnapResult } from "../viewport/smartGuides";

function makeEngine(): {
  engine: DocumentEngine;
  history: CommandHistory;
  moveLayerSilent: ReturnType<typeof vi.fn>;
  flushChangeNotification: ReturnType<typeof vi.fn>;
  commit: ReturnType<typeof vi.fn>;
} {
  let currentX = 50;
  let currentY = 50;
  const moveLayerSilent = vi.fn((_id: string, x: number, y: number) => {
    currentX = x;
    currentY = y;
  });
  const flushChangeNotification = vi.fn();
  const commit = vi.fn();
  const engine = {
    getLayer: (id: string) => ({
      id,
      name: "L1",
      type: "raster" as const,
      visible: true,
      opacity: 1,
      locked: false,
      blendMode: "normal" as const,
      transform: { x: currentX, y: currentY, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      width: 100,
      height: 100,
      imageBitmap: null,
    }),
    moveLayerSilent,
    flushChangeNotification,
    snapshot: () => ({}),
  } as unknown as DocumentEngine;

  const history = { commit } as unknown as CommandHistory;

  return { engine, history, moveLayerSilent, flushChangeNotification, commit };
}

describe("input-handler snap wiring", () => {
  it("applies snap delta and emits guide lines on move when Alt is not pressed", () => {
    const { engine, history, moveLayerSilent, commit } = makeEngine();
    const snap: SnapResult = { dx: 3, dy: 0, lines: [{ x1: 100, y1: 0, x2: 100, y2: 200 }] };
    const onComputeSnap = vi.fn(() => snap);
    const onSnapLines = vi.fn();
    const ctx = {
      fgColor: "#000",
      bgColor: "#fff",
      brushSize: 20,
      brushHardness: 0.8,
      brushOpacity: 1,
      paintSettings: { size: 20, hardness: 0.8, opacity: 1, flow: 1, smoothing: 0 },
      selectedLayerId: "L1",
      isDragging: false,
      dragStart: { x: 0, y: 0 },
      dragCurrent: { x: 0, y: 0 },
      strokePoints: [],
      dragTool: null,
      isAltPressed: false,
      isShiftPressed: false,
      onComputeSnap,
      onSnapLines,
    };

    handlePointerDown("move", 50, 50, engine, history, () => {}, ctx);
    handlePointerMove("move", 55, 50, engine, () => {}, ctx);
    handlePointerUp("move", 55, 50, engine, history, () => {}, ctx);

    // Move tool history is owned by useCanvasLayerDrag.onPointerUp,
    // not by input-handler.handlePointerUp. Verify snap + move still work.
    expect(commit).not.toHaveBeenCalled();
    expect(onComputeSnap).toHaveBeenCalledWith({ x: 55, y: 50, w: 100, h: 100 });
    expect(moveLayerSilent).toHaveBeenCalledWith("L1", 58, 50);
    expect(onSnapLines).toHaveBeenCalledWith(snap.lines);
  });

  it("skips snap and clears guide lines while Alt is pressed", () => {
    const { engine, history, moveLayerSilent } = makeEngine();
    const onComputeSnap = vi.fn((): SnapResult => ({ dx: 3, dy: 0, lines: [{ x1: 100, y1: 0, x2: 100, y2: 200 }] }));
    const onSnapLines = vi.fn();
    const ctx = {
      fgColor: "#000",
      bgColor: "#fff",
      brushSize: 20,
      brushHardness: 0.8,
      brushOpacity: 1,
      paintSettings: { size: 20, hardness: 0.8, opacity: 1, flow: 1, smoothing: 0 },
      selectedLayerId: "L1",
      isDragging: false,
      dragStart: { x: 0, y: 0 },
      dragCurrent: { x: 0, y: 0 },
      strokePoints: [],
      dragTool: null,
      isAltPressed: true,
      isShiftPressed: false,
      onComputeSnap,
      onSnapLines,
    };

    handlePointerDown("move", 50, 50, engine, history, () => {}, ctx);
    handlePointerMove("move", 55, 50, engine, () => {}, ctx);

    expect(onComputeSnap).not.toHaveBeenCalled();
    expect(moveLayerSilent).toHaveBeenCalledWith("L1", 55, 50);
    expect(onSnapLines).toHaveBeenCalledWith([]);
  });

  it("clears snap lines on pointer up", () => {
    const { engine, history } = makeEngine();
    const lines = [{ x1: 100, y1: 0, x2: 100, y2: 200 }];
    const onSnapLines = vi.fn();
    const ctx = {
      fgColor: "#000",
      bgColor: "#fff",
      brushSize: 20,
      brushHardness: 0.8,
      brushOpacity: 1,
      paintSettings: { size: 20, hardness: 0.8, opacity: 1, flow: 1, smoothing: 0 },
      selectedLayerId: "L1",
      isDragging: false,
      dragStart: { x: 0, y: 0 },
      dragCurrent: { x: 0, y: 0 },
      strokePoints: [],
      dragTool: null,
      isAltPressed: false,
      isShiftPressed: false,
      onComputeSnap: () => ({ dx: 0, dy: 0, lines }),
      onSnapLines,
    };

    handlePointerDown("move", 50, 50, engine, history, () => {}, ctx);
    handlePointerMove("move", 50, 50, engine, () => {}, ctx);

    expect(onSnapLines).toHaveBeenCalledWith(lines);

    onSnapLines.mockClear();

    handlePointerUp("move", 50, 50, engine, history, () => {}, ctx);

    expect(onSnapLines).toHaveBeenCalledTimes(1);
    expect(onSnapLines).toHaveBeenCalledWith([]);
  });

  it("does not call onComputeSnap when no layer is selected", () => {
    const { engine, history, moveLayerSilent } = makeEngine();
    const onComputeSnap = vi.fn();
    const ctx = {
      fgColor: "#000",
      bgColor: "#fff",
      brushSize: 20,
      brushHardness: 0.8,
      brushOpacity: 1,
      paintSettings: { size: 20, hardness: 0.8, opacity: 1, flow: 1, smoothing: 0 },
      selectedLayerId: null,
      isDragging: false,
      dragStart: { x: 0, y: 0 },
      dragCurrent: { x: 0, y: 0 },
      strokePoints: [],
      dragTool: null,
      isAltPressed: false,
      isShiftPressed: false,
      onComputeSnap,
      onSnapLines: vi.fn(),
    };

    handlePointerDown("move", 50, 50, engine, history, () => {}, ctx);
    handlePointerMove("move", 55, 50, engine, () => {}, ctx);

    expect(onComputeSnap).not.toHaveBeenCalled();
    expect(moveLayerSilent).not.toHaveBeenCalled();
  });
});
