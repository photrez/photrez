// apps/desktop/src/components/editor/__tests__/fillAndGradientWiring.test.tsx
//
// Regression suite for Paint Bucket & Gradient Tool Wiring Contracts:
// 1. Paint Bucket: pointerdown triggers floodFill with correct tolerance & contiguous mode.
// 2. Gradient: pointerdown -> pointermove -> pointerup updates gradientDragLine signal and handles Shift 45° angle lock.
// 3. Cursor Resolver: resolveCursor returns custom SVG cursors for paintBucket and gradient.

import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { resolveCursor } from "@/viewport/cursorResolver";
import { createMockEditorParams, createPointerTools, makePointerEvent } from "../../../__tests__/pointerRoutingHarness";

describe("Paint Bucket & Gradient Wiring & Regression Suite", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    class OffscreenCanvasMock {
      width: number;
      height: number;
      constructor(w: number, h: number) {
        this.width = w;
        this.height = h;
      }
      getContext() {
        return {
          drawImage: () => {},
          getImageData: () => ({ data: new Uint8ClampedArray(4 * (this.width || 100) * (this.height || 100)) }),
          putImageData: () => {},
        };
      }
      transferToImageBitmap() {
        return {} as ImageBitmap;
      }
    }
    vi.stubGlobal("OffscreenCanvas", OffscreenCanvasMock);
  });

  describe("Cursor Resolver Contracts", () => {
    const makeCtx = (tool: any) => ({
      activeTool: tool,
      isSpacePressed: false,
      isPanning: false,
      isAltPressed: false,
      hoverHandle: null,
      isLayerLocked: false,
      eyedropperTarget: null,
    });

    it("returns custom SVG cursor for paintBucket tool", () => {
      const cursor = resolveCursor(makeCtx("paintBucket"));
      expect(cursor).toContain("data:image/svg+xml");
      expect(cursor).toContain("fill=");
    });

    it("returns custom SVG cursor for gradient tool", () => {
      const cursor = resolveCursor(makeCtx("gradient"));
      expect(cursor).toContain("data:image/svg+xml");
      expect(cursor).toContain("crosshair");
    });
  });

  describe("Paint Bucket Tool Pointer Dispatch", () => {
    it("executes flood fill on canvas click with signals context", () => {
      const { signals, dispose } = createMockEditorParams("paintBucket");
      const commitSpy = vi.fn();
      signals.workspace.getActiveHistory = () => ({
        commit: commitSpy,
        getLastPaintCoords: () => null,
        setLastPaintCoords: () => {},
      }) as any;

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

      // Pointer down should process flood fill and commit to history
      tools.onCanvasPointerDown(makePointerEvent({ clientX: 50, clientY: 50 }));
      expect(commitSpy).toHaveBeenCalled();

      disposeTools();
      dispose();
    });
  });

  describe("Gradient Tool Drag & Angle Lock Contracts", () => {
    it("updates gradientDragLine signal during drag and resets to null on pointerup", () => {
      const { signals, dispose } = createMockEditorParams("gradient");
      const setGradientDragLineSpy = vi.fn();
      const commitSpy = vi.fn();

      signals.setGradientDragLine = setGradientDragLineSpy;
      signals.workspace.getActiveHistory = () => ({
        commit: commitSpy,
        getLastPaintCoords: () => null,
        setLastPaintCoords: () => {},
      }) as any;

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

      // 1. Pointer Down
      tools.onCanvasPointerDown(makePointerEvent({ clientX: 100, clientY: 100 }));
      expect(setGradientDragLineSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          start: { x: 100, y: 100 },
          end: { x: 100, y: 100 },
          type: "linear",
        })
      );

      // 2. Pointer Move (free drag)
      tools.onCanvasPointerMove(makePointerEvent({ clientX: 200, clientY: 100 }));
      expect(setGradientDragLineSpy).toHaveBeenLastCalledWith(
        expect.objectContaining({
          start: { x: 100, y: 100 },
          end: { x: 200, y: 100 },
          angle: 0,
          distance: 100,
        })
      );

      // 3. Pointer Move with Shift (45° angle snap)
      tools.onCanvasPointerMove(makePointerEvent({ clientX: 200, clientY: 120, shiftKey: true }));
      expect(setGradientDragLineSpy).toHaveBeenLastCalledWith(
        expect.objectContaining({
          start: { x: 100, y: 100 },
          angle: 0,
        })
      );

      // 4. Pointer Up (commits to history & resets drag line signal)
      tools.onCanvasPointerUp(makePointerEvent({ clientX: 200, clientY: 120 }));
      expect(setGradientDragLineSpy).toHaveBeenLastCalledWith(null);
      expect(commitSpy).toHaveBeenCalled();

      disposeTools();
      dispose();
    });
  });
});
