import { describe, it, expect, vi } from "vitest";
import { handlePointerDown, handlePointerMove, handlePointerUp } from "../viewport/input-handler";
import { createMockEngine, createMockHistory, createToolContext } from "./test-builders";

describe("repro: move tool with Background layer selected emits no snap lines", () => {
  it("input-handler move path: background selected → no moveLayerSilent, no onSnapLines during drag", () => {
    const engine = createMockEngine(["snapshot", "moveLayerSilent", "flushChangeNotification", "getLayer"]);
    const bgLayer = {
      id: "bg-1",
      name: "Background",
      type: "raster" as const,
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      locked: false,
      lockPosition: true,
      lockRotation: true,
      isBackground: true,
      visible: true,
      opacity: 1,
      blendMode: "normal",
      width: 800,
      height: 600,
      imageBitmap: null,
    };
    vi.mocked(engine.getLayer).mockReturnValue(bgLayer as never);
    const history = createMockHistory();
    const onSnapLines = vi.fn();
    const onComputeSnap = vi.fn(() => ({ dx: 0, dy: 0, lines: [] }));
    const context = createToolContext({
      selectedLayerId: "bg-1",
      isDragging: true,
      dragStart: { x: 0, y: 0 },
      onSnapLines,
      onComputeSnap,
    });

    handlePointerDown("move", 50, 50, engine, history, vi.fn(), context);
    handlePointerMove("move", 200, 150, engine, vi.fn(), context);

    expect(engine.moveLayerSilent).not.toHaveBeenCalled();
    expect(onSnapLines).not.toHaveBeenCalled();
    expect(onComputeSnap).not.toHaveBeenCalled();

    handlePointerUp("move", 200, 150, engine, history, vi.fn(), context);
  });
});
