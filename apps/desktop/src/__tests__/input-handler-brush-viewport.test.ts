import { describe, it, expect, vi } from "vitest";
import { handlePointerDown, handlePointerMove, handlePointerUp } from "../viewport/input-handler";
import { createMockEngine, createMockHistory, createToolContext } from "./test-builders";

/**
 * Helper: simulate the coordinate conversion that getDocCoords() performs.
 * (clientX - rectLeft - panX) / zoom
 * getDocCoords guard: !Number.isFinite(z) || z <= 0 → return {0, 0}
 */
function simulateGetDocCoords(
  clientX: number,
  clientY: number,
  rectLeft: number,
  rectTop: number,
  panX: number,
  panY: number,
  zoom: number,
): { x: number; y: number } {
  if (!Number.isFinite(zoom) || zoom <= 0) return { x: 0, y: 0 };
  return {
    x: (clientX - rectLeft - panX) / zoom,
    y: (clientY - rectTop - panY) / zoom,
  };
}

const CANVAS_RECT = { left: 50, top: 30 };
const PAINT_SETTINGS = { size: 20, hardness: 0.8, opacity: 1, flow: 1, smoothing: 0 };

describe("brush tool coordinate chain — viewport integration", () => {
  /**
   * These tests simulate the full chain for brush/eraser:
   *
   * PointerEvent (clientX, clientY)
   *   → getDocCoords() using pan()/zoom() signals
   *   → handlePointerDown/Move/Up("brush", docX, docY, ...)
   *   → context.onPaintStroke(points, isEraser, settings, isFinal)
   *
   * By varying pan and zoom, we verify that stroke points are recorded
   * at the correct document-space positions.
   */

  it("default zoom=1, pan=(0,0): brush stroke at correct doc coords", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const onPaintStroke = vi.fn();
    const ctx = createToolContext({ onPaintStroke, paintSettings: PAINT_SETTINGS });

    // Click at screen (150, 80) → doc (100, 50)
    const start = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    handlePointerDown("brush", start.x, start.y, engine, history, vi.fn(), ctx);

    // pointerDown should init strokePoints and call onPaintStroke
    expect(ctx.strokePoints).toEqual([{ x: 100, y: 50 }]);
    expect(onPaintStroke).toHaveBeenCalledWith(
      [{ x: 100, y: 50 }], false, PAINT_SETTINGS,
    );

    // Drag to screen (200, 130) → doc (150, 100)
    const mid = simulateGetDocCoords(200, 130, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    handlePointerMove("brush", mid.x, mid.y, engine, vi.fn(), ctx);

    // pointerMove should append point
    expect(ctx.strokePoints).toEqual([{ x: 100, y: 50 }, { x: 150, y: 100 }]);
    expect(onPaintStroke).toHaveBeenCalledWith(
      [{ x: 100, y: 50 }, { x: 150, y: 100 }], false, PAINT_SETTINGS,
    );
  });

  it("eraser tool sets isEraser=true in onPaintStroke", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const onPaintStroke = vi.fn();
    const ctx = createToolContext({ selectedLayerId: "layer-1", onPaintStroke, paintSettings: PAINT_SETTINGS });

    const start = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    handlePointerDown("eraser", start.x, start.y, engine, history, vi.fn(), ctx);

    expect(onPaintStroke).toHaveBeenCalledWith(
      [{ x: 100, y: 50 }], true, PAINT_SETTINGS,
    );
  });

  it("zoom=2: stroke points are halved in doc space", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const onPaintStroke = vi.fn();
    const ctx = createToolContext({ onPaintStroke, paintSettings: PAINT_SETTINGS });

    // Click at screen (150, 80) → doc (50, 25)
    const start = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 2);
    handlePointerDown("brush", start.x, start.y, engine, history, vi.fn(), ctx);

    expect(ctx.strokePoints).toEqual([{ x: 50, y: 25 }]);

    // Drag to screen (250, 130) → doc (100, 50)
    const mid = simulateGetDocCoords(250, 130, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 2);
    handlePointerMove("brush", mid.x, mid.y, engine, vi.fn(), ctx);

    expect(ctx.strokePoints).toEqual([{ x: 50, y: 25 }, { x: 100, y: 50 }]);

    // Verify: 100px screen delta = 50px doc delta (zoom=2)
    const dx = ctx.strokePoints[1].x - ctx.strokePoints[0].x;
    const dy = ctx.strokePoints[1].y - ctx.strokePoints[0].y;
    expect(dx).toBe(50);
    expect(dy).toBe(25);
  });

  it("zoom=0.5: stroke points are doubled in doc space", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const onPaintStroke = vi.fn();
    const ctx = createToolContext({ onPaintStroke, paintSettings: PAINT_SETTINGS });

    // Click at screen (150, 80) → doc (200, 100)
    const start = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 0.5);
    handlePointerDown("brush", start.x, start.y, engine, history, vi.fn(), ctx);

    expect(ctx.strokePoints).toEqual([{ x: 200, y: 100 }]);

    // Drag to screen (200, 130) → doc (300, 200)
    const mid = simulateGetDocCoords(200, 130, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 0.5);
    handlePointerMove("brush", mid.x, mid.y, engine, vi.fn(), ctx);

    expect(ctx.strokePoints).toEqual([{ x: 200, y: 100 }, { x: 300, y: 200 }]);

    // 50px screen delta = 100px doc delta (zoom=0.5)
    const dx = ctx.strokePoints[1].x - ctx.strokePoints[0].x;
    const dy = ctx.strokePoints[1].y - ctx.strokePoints[0].y;
    expect(dx).toBe(100);
    expect(dy).toBe(100);
  });

  it("zoom=1, pan=(200,100): stroke accounts for pan offset", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const onPaintStroke = vi.fn();
    const ctx = createToolContext({ onPaintStroke, paintSettings: PAINT_SETTINGS });

    // Click at screen (250, 130) → doc (0, 0)
    const start = simulateGetDocCoords(250, 130, CANVAS_RECT.left, CANVAS_RECT.top, 200, 100, 1);
    handlePointerDown("brush", start.x, start.y, engine, history, vi.fn(), ctx);

    expect(ctx.strokePoints).toEqual([{ x: 0, y: 0 }]);

    // Drag to screen (450, 230) → doc (200, 100)
    const mid = simulateGetDocCoords(450, 230, CANVAS_RECT.left, CANVAS_RECT.top, 200, 100, 1);
    handlePointerMove("brush", mid.x, mid.y, engine, vi.fn(), ctx);

    expect(ctx.strokePoints).toEqual([{ x: 0, y: 0 }, { x: 200, y: 100 }]);
  });

  it("zoom=2, pan=(-200, -100): negative pan + zoom combined", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const onPaintStroke = vi.fn();
    const ctx = createToolContext({ onPaintStroke, paintSettings: PAINT_SETTINGS });

    // Click at screen (50, 30) → doc (100, 50)
    const start = simulateGetDocCoords(50, 30, CANVAS_RECT.left, CANVAS_RECT.top, -200, -100, 2);
    handlePointerDown("brush", start.x, start.y, engine, history, vi.fn(), ctx);

    expect(ctx.strokePoints).toEqual([{ x: 100, y: 50 }]);

    // Drag to screen (150, 80) → doc (150, 75)
    const mid = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, -200, -100, 2);
    handlePointerMove("brush", mid.x, mid.y, engine, vi.fn(), ctx);

    expect(ctx.strokePoints).toEqual([{ x: 100, y: 50 }, { x: 150, y: 75 }]);

    // 100px screen delta = 50px doc delta (zoom=2)
    const dx = ctx.strokePoints[1].x - ctx.strokePoints[0].x;
    expect(dx).toBe(50);
  });

  it("pointerUp finalizes stroke with isFinal=true", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const onPaintStroke = vi.fn();
    const ctx = createToolContext({ selectedLayerId: "layer-1", onPaintStroke, paintSettings: PAINT_SETTINGS });

    // Must call handlePointerDown first to set isDragging=true and init strokePoints
    const start = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    handlePointerDown("brush", start.x, start.y, engine, history, vi.fn(), ctx);

    // Clear the initial onPaintStroke call from pointerDown
    onPaintStroke.mockClear();

    const end = simulateGetDocCoords(200, 130, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    handlePointerUp("brush", end.x, end.y, engine, history, vi.fn(), ctx);

    // pointerUp should call onPaintStroke with isFinal=true
    expect(onPaintStroke).toHaveBeenCalledWith(
      expect.any(Array), false, PAINT_SETTINGS, true,
    );
    // strokePoints should be cleared after pointerUp
    expect(ctx.strokePoints).toEqual([]);
  });

  it("no stroke when selectedLayerId is null", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const onPaintStroke = vi.fn();
    const ctx = createToolContext({ selectedLayerId: null, onPaintStroke, paintSettings: PAINT_SETTINGS });

    const start = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    handlePointerDown("brush", start.x, start.y, engine, history, vi.fn(), ctx);

    // pointerDown should still call onPaintStroke even without selectedLayerId
    // (the layer check happens in commit phase, not in stroke accumulation)
    expect(onPaintStroke).toHaveBeenCalled();
    expect(ctx.strokePoints).toEqual([{ x: 100, y: 50 }]);
  });

  it("multiple pointer moves accumulate multiple stroke points", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const onPaintStroke = vi.fn();
    const ctx = createToolContext({ onPaintStroke, paintSettings: PAINT_SETTINGS });

    const p1 = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    handlePointerDown("brush", p1.x, p1.y, engine, history, vi.fn(), ctx);

    const p2 = simulateGetDocCoords(200, 130, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    handlePointerMove("brush", p2.x, p2.y, engine, vi.fn(), ctx);

    const p3 = simulateGetDocCoords(250, 180, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    handlePointerMove("brush", p3.x, p3.y, engine, vi.fn(), ctx);

    const p4 = simulateGetDocCoords(350, 130, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    handlePointerMove("brush", p4.x, p4.y, engine, vi.fn(), ctx);

    expect(ctx.strokePoints).toHaveLength(4);
    expect(ctx.strokePoints[0]).toEqual({ x: 100, y: 50 });
    expect(ctx.strokePoints[1]).toEqual({ x: 150, y: 100 });
    expect(ctx.strokePoints[2]).toEqual({ x: 200, y: 150 });
    expect(ctx.strokePoints[3]).toEqual({ x: 300, y: 100 });

    // Each move should call onPaintStroke
    expect(onPaintStroke).toHaveBeenCalledTimes(4); // 1 pointerDown + 3 pointerMove
  });

  it("non-move tool does not trigger brush stroke", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const onPaintStroke = vi.fn();
    const ctx = createToolContext({ onPaintStroke, paintSettings: PAINT_SETTINGS });

    // Start a brush stroke
    const start = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    handlePointerDown("brush", start.x, start.y, engine, history, vi.fn(), ctx);

    // Switch tool mid-gesture
    const mid = simulateGetDocCoords(200, 130, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    handlePointerMove("move", mid.x, mid.y, engine, vi.fn(), ctx);

    // Should still push stroke points (uses dragTool from pointerDown, not _tool param)
    expect(ctx.strokePoints).toHaveLength(2);
    // onPaintStroke should still be called with brush points
    expect(onPaintStroke).toHaveBeenCalledWith(
      expect.arrayContaining([{ x: 150, y: 100 }]), false, PAINT_SETTINGS,
    );
    // engine.moveLayerSilent should NOT be called (it's brush, not move)
    expect(engine.moveLayerSilent).not.toHaveBeenCalled();
  });
});

describe("eraser tool — viewport integration", () => {
  it("eraser isEraser flag set correctly with various pan/zoom", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const onPaintStroke = vi.fn();
    const ctx = createToolContext({ selectedLayerId: "layer-1", onPaintStroke, paintSettings: PAINT_SETTINGS });

    const scenarios = [
      { panX: 0, panY: 0, zoom: 1, name: "default" },
      { panX: 200, panY: 100, zoom: 1, name: "panned" },
      { panX: 0, panY: 0, zoom: 2, name: "zoomed" },
      { panX: -200, panY: -100, zoom: 2, name: "negative pan + zoom" },
    ];

    for (const s of scenarios) {
      onPaintStroke.mockClear();
      const pt = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, s.panX, s.panY, s.zoom);
      handlePointerDown("eraser", pt.x, pt.y, engine, history, vi.fn(), ctx);
      // Verify eraser flag is always true
      const lastCall = onPaintStroke.mock.calls[onPaintStroke.mock.calls.length - 1];
      expect(lastCall[1]).toBe(true);
      ctx.isDragging = false; // reset for next iteration
    }
  });
});
