import { describe, it, expect, vi } from "vitest";
import { handlePointerDown, handlePointerMove, handlePointerUp } from "../viewport/input-handler";
import { screenToDocument } from "../viewport/coords";
import { createMockEngine, createMockHistory, createToolContext } from "./test-builders";

/**
 * Helper: simulate the full coordinate chain that getDocCoords() performs.
 * 
 * getDocCoords (useCanvasPointerTools.ts, the fixed signal-based path):
 *   (clientX - rect.left - pan().x) / zoom()
 * 
 * screenToDocument (coords.ts, the old engine-based path):
 *   (clientX - canvasRect.left - viewport.panX) / viewport.zoom
 * 
 * Both formulas are mathematically identical. The signal-based path is the
 * correct one because pan()/zoom() signals stay fresh during panning while
 * engine.getViewport() goes stale (see usePanNavigation.ts which skips
 * engine.setViewport to avoid triggering layer re-selection).
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

/** A fixed canvas rect used across all tests to ensure deterministic results. */
const CANVAS_RECT = { left: 50, top: 30 };

describe("move tool coordinate chain — viewport integration", () => {
  /**
   * These tests simulate the FULL chain that occurs in the app:
   * 
   * PointerEvent (clientX, clientY)
   *   → getDocCoords() using pan()/zoom() signals
   *   → handlePointerDown("move", docX, docY, ...)
   *   → handlePointerMove("move", docX, docY, ...)
   *   → engine.moveLayerSilent(layerId, newX, newY)
   * 
   * By varying pan and zoom, we verify that the move tool correctly
   * translates screen-space gestures to document-space layer positions.
   * This prevents regression where engine.getViewport() (which goes stale
   * during panning) was used instead of always-fresh signals.
   */

  it("default zoom=1, pan=(0,0): move tool translates screen delta to layer position correctly", () => {
    // Setup: layer at transform(100, 50), canvas at screen (50, 30)
    const engine = createMockEngine();
    const history = createMockHistory();
    const context = createToolContext();

    // Simulate: click at screen (150, 80) → canvas-relative (100, 50) → doc (100, 50)
    // getDocCoords: (150 - 50 - 0) / 1 = 100, (80 - 30 - 0) / 1 = 50
    const doc = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    expect(doc).toEqual({ x: 100, y: 50 });

    handlePointerDown("move", doc.x, doc.y, engine, history, vi.fn(), context);
    // dragStart = doc - layer.transform = (100 - 100, 50 - 50) = (0, 0)

    // Drag to screen (200, 130) → canvas-relative (150, 100) → doc (150, 100)
    const doc2 = simulateGetDocCoords(200, 130, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 1);
    expect(doc2).toEqual({ x: 150, y: 100 });

    handlePointerMove("move", doc2.x, doc2.y, engine, vi.fn(), context);
    // new position = doc - dragStart = (150 - 0, 100 - 0) = (150, 100)

    expect(engine.moveLayerSilent).toHaveBeenCalledWith("layer-1", 150, 100);
  });

  it("zoom=2, pan=(0,0): screen delta 50px moves layer 25px in doc space", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const context = createToolContext();

    // Click at screen (150, 80) → canvas-relative (100, 50) → doc (50, 25)
    const doc = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 2);
    expect(doc).toEqual({ x: 50, y: 25 });

    handlePointerDown("move", doc.x, doc.y, engine, history, vi.fn(), context);
    // dragStart: (50 - 100, 25 - 50) = (-50, -25)

    // Drag 50px screen = 25px doc
    const doc2 = simulateGetDocCoords(200, 130, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 2);
    expect(doc2).toEqual({ x: 75, y: 50 });

    handlePointerMove("move", doc2.x, doc2.y, engine, vi.fn(), context);
    // new position = doc - dragStart = (75 - (-50), 50 - (-25)) = (125, 75)

    expect(engine.moveLayerSilent).toHaveBeenCalledWith("layer-1", 125, 75);
  });

  it("zoom=0.5, pan=(0,0): screen delta 50px moves layer 100px in doc space", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const context = createToolContext();

    // Click at screen (150, 80) → canvas-relative (100, 50) → doc (200, 100)
    const doc = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 0.5);
    expect(doc).toEqual({ x: 200, y: 100 });

    handlePointerDown("move", doc.x, doc.y, engine, history, vi.fn(), context);
    // dragStart: (200 - 100, 100 - 50) = (100, 50)

    // Drag 50px screen = 100px doc
    const doc2 = simulateGetDocCoords(200, 130, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 0.5);
    expect(doc2).toEqual({ x: 300, y: 200 });

    handlePointerMove("move", doc2.x, doc2.y, engine, vi.fn(), context);
    // new position = doc - dragStart = (300 - 100, 200 - 50) = (200, 150)

    expect(engine.moveLayerSilent).toHaveBeenCalledWith("layer-1", 200, 150);
  });

  it("zoom=1, pan=(200, 100): screen delta accounts for pan offset", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const context = createToolContext();

    // Canvas is panned 200px right, 100px down.
    // Click at screen (250, 130) → canvas-relative (200, 100) → (200-200, 100-100) = (0, 0)
    const doc = simulateGetDocCoords(250, 130, CANVAS_RECT.left, CANVAS_RECT.top, 200, 100, 1);
    expect(doc).toEqual({ x: 0, y: 0 });

    handlePointerDown("move", doc.x, doc.y, engine, history, vi.fn(), context);
    // dragStart: (0 - 100, 0 - 50) = (-100, -50)

    // Drag to screen (300, 180) → canvas-relative (250, 150) → (250-200, 150-100) = (50, 50)
    const doc2 = simulateGetDocCoords(300, 180, CANVAS_RECT.left, CANVAS_RECT.top, 200, 100, 1);
    expect(doc2).toEqual({ x: 50, y: 50 });

    handlePointerMove("move", doc2.x, doc2.y, engine, vi.fn(), context);
    // new position = doc - dragStart = (50 - (-100), 50 - (-50)) = (150, 100)

    expect(engine.moveLayerSilent).toHaveBeenCalledWith("layer-1", 150, 100);
  });

  it("zoom=2, pan=(-200, -100): negative pan + zoom combined", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const context = createToolContext();

    // Canvas panned -200 left, -100 up. Zoom=2.
    // Click at screen (50, 30) → canvas-relative (0, 0) → (0 - (-200), 0 - (-100))/2 = (100, 50)
    const doc = simulateGetDocCoords(50, 30, CANVAS_RECT.left, CANVAS_RECT.top, -200, -100, 2);
    expect(doc).toEqual({ x: 100, y: 50 });

    handlePointerDown("move", doc.x, doc.y, engine, history, vi.fn(), context);
    // dragStart: (100 - 100, 50 - 50) = (0, 0)

    // Drag 50px screen = 25px doc
    const doc2 = simulateGetDocCoords(100, 80, CANVAS_RECT.left, CANVAS_RECT.top, -200, -100, 2);
    expect(doc2).toEqual({ x: 125, y: 75 });

    handlePointerMove("move", doc2.x, doc2.y, engine, vi.fn(), context);
    // new position = doc - 0 = (125, 75)

    expect(engine.moveLayerSilent).toHaveBeenCalledWith("layer-1", 125, 75);
  });

  it("zoom=4, pan=(300, 150): extreme zoom + large pan", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    const context = createToolContext();

    // Click at screen (350, 180) → canvas-relative (300, 150) → (300-300, 150-150)/4 = (0, 0)
    const doc = simulateGetDocCoords(350, 180, CANVAS_RECT.left, CANVAS_RECT.top, 300, 150, 4);
    expect(doc).toEqual({ x: 0, y: 0 });

    handlePointerDown("move", doc.x, doc.y, engine, history, vi.fn(), context);
    // dragStart: (0 - 100, 0 - 50) = (-100, -50)

    // Drag 200px screen = 50px doc
    const doc2 = simulateGetDocCoords(550, 380, CANVAS_RECT.left, CANVAS_RECT.top, 300, 150, 4);
    expect(doc2).toEqual({ x: 50, y: 50 });

    handlePointerMove("move", doc2.x, doc2.y, engine, vi.fn(), context);
    // new position = doc - dragStart = (50 - (-100), 50 - (-50)) = (150, 100)

    expect(engine.moveLayerSilent).toHaveBeenCalledWith("layer-1", 150, 100);
  });

  it("zoom=0 guard: any screen delta maps to doc (0,0) → layer stays at original position", () => {
    // getDocCoords guard: !Number.isFinite(z) || z <= 0 → return {0, 0}
    // This means with zoom=0, ALL pointer events produce doc coords (0,0)
    // regardless of screen position. The layer should end up at its
    // original transform because no effective movement can be computed.
    const engine = createMockEngine();
    const history = createMockHistory();
    const context = createToolContext();

    // Even a large screen delta produces (0,0) with zoom=0
    const docPointerDown = simulateGetDocCoords(50, 30, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 0);
    expect(docPointerDown).toEqual({ x: 0, y: 0 });

    handlePointerDown("move", docPointerDown.x, docPointerDown.y, engine, history, vi.fn(), context);
    // dragStart: (0 - 100, 0 - 50) = (-100, -50)

    // Drag 350px screen away — still (0,0) because zoom=0
    const docPointerMove = simulateGetDocCoords(400, 380, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, 0);
    expect(docPointerMove).toEqual({ x: 0, y: 0 });

    handlePointerMove("move", docPointerMove.x, docPointerMove.y, engine, vi.fn(), context);
    // new position = (0 - (-100), 0 - (-50)) = (100, 50) — same as original!

    expect(engine.moveLayerSilent).toHaveBeenCalledWith("layer-1", 100, 50);
  });

  it("zoom negative guard: returns {0,0} like zoom=0", () => {
    // Same guard: z <= 0 → return {0,0}
    const engine = createMockEngine();
    const history = createMockHistory();
    const context = createToolContext();

    const doc = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, -1);
    expect(doc).toEqual({ x: 0, y: 0 });

    handlePointerDown("move", doc.x, doc.y, engine, history, vi.fn(), context);
    expect(engine.moveLayerSilent).not.toHaveBeenCalled();
  });

  it("zoom NaN guard: returns {0,0} like zoom=0", () => {
    // Same guard: !Number.isFinite(z) → return {0,0}
    const engine = createMockEngine();
    const history = createMockHistory();
    const context = createToolContext();

    const doc = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 0, 0, NaN);
    expect(doc).toEqual({ x: 0, y: 0 });

    handlePointerDown("move", doc.x, doc.y, engine, history, vi.fn(), context);
    expect(engine.moveLayerSilent).not.toHaveBeenCalled();
  });

  it("wrap-around: screenToDocument + handlePointerDown set correct dragStart offset", () => {
    // Pure integration from screenToDocument to handlePointerDown:
    // Verify that the document coordinates produced by screenToDocument
    // match what handlePointerDown expects for dragStart calculation.

    const engine = createMockEngine();
    const history = createMockHistory();
    const context = createToolContext();
    const canvasRect = new DOMRect(CANVAS_RECT.left, CANVAS_RECT.top, 800, 600);

    const testCases = [
      { panX: 0, panY: 0, zoom: 1, clientX: 150, clientY: 80, expectedDoc: { x: 100, y: 50 } },
      { panX: 200, panY: 100, zoom: 1, clientX: 250, clientY: 130, expectedDoc: { x: 0, y: 0 } },
      { panX: 0, panY: 0, zoom: 2, clientX: 150, clientY: 80, expectedDoc: { x: 50, y: 25 } },
      { panX: -200, panY: -100, zoom: 2, clientX: 50, clientY: 30, expectedDoc: { x: 100, y: 50 } },
      { panX: 300, panY: 150, zoom: 4, clientX: 350, clientY: 180, expectedDoc: { x: 0, y: 0 } },
    ];

    for (const tc of testCases) {
      // Use the same screenToDocument that coords.ts exports
      const doc = screenToDocument(
        tc.clientX, tc.clientY, canvasRect,
        { panX: tc.panX, panY: tc.panY, zoom: tc.zoom, rotation: 0 },
      );
      expect(doc).toEqual(tc.expectedDoc);

      handlePointerDown("move", doc.x, doc.y, engine, history, vi.fn(), context);
      // dragStart = doc - layer.transform(100, 50)
      expect(context.dragStart).toEqual({
        x: tc.expectedDoc.x - 100,
        y: tc.expectedDoc.y - 50,
      });

      // Reset for next iteration
      context.isDragging = false;
    }
  });

  it("locked layer: pointerDown does not move layer regardless of viewport", () => {
    const engine = createMockEngine();
    const history = createMockHistory();
    // Override getLayer to return a locked layer
    vi.mocked(engine.getLayer).mockReturnValue({
      id: "layer-1",
      name: "Layer 1",
      type: "raster" as const,
      transform: { x: 100, y: 50, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      locked: true,
      visible: true,
      opacity: 1,
      blendMode: "normal",
      width: 200,
      height: 150,
      imageBitmap: null,
    });

    const context = createToolContext();
    const doc = simulateGetDocCoords(150, 80, CANVAS_RECT.left, CANVAS_RECT.top, 200, 100, 2);
    handlePointerDown("move", doc.x, doc.y, engine, history, vi.fn(), context);

    // dragStart should be raw doc coords (not offset by layer transform)
    expect(context.dragStart).toEqual({ x: doc.x, y: doc.y });
    expect(context.pendingHistorySnapshot).toBeNull();
    expect(engine.moveLayerSilent).not.toHaveBeenCalled();
  });
});
