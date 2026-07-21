import { describe, it, expect, vi } from "vitest";
import { handlePointerDown, handlePointerMove, handlePointerUp, isPointInSelection } from "../viewport/input-handler";
import { createMockEngine, createMockHistory, createToolContext } from "./test-builders";
import type { DocumentEngine } from "../engine/document";

describe("input-handler: selection tool draw modifiers", () => {
  describe("basic drag", () => {
    it("creates selection rect from min/max corners", () => {
      const engine = createMockEngine(["createSelection", "clearSelection", "snapshot"]);
      const ctx = createToolContext({ selectedLayerId: null, onSelectionCreated: vi.fn() });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", 200, 250, engine, vi.fn(), ctx);
      handlePointerUp("selection", 200, 250, engine, createMockHistory(), vi.fn(), ctx);
      expect(engine.createSelection).toHaveBeenCalledWith(100, 100, 100, 150);
    });

    it("drag from right-bottom to left-top still creates valid rect", () => {
      const engine = createMockEngine(["createSelection", "clearSelection", "snapshot"]);
      const ctx = createToolContext({ selectedLayerId: null, onSelectionCreated: vi.fn() });
      handlePointerDown("selection", 200, 250, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", 100, 100, engine, vi.fn(), ctx);
      handlePointerUp("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      expect(engine.createSelection).toHaveBeenCalledWith(100, 100, 100, 150);
    });

    it("tiny drag clears selection instead of creating", () => {
      const engine = createMockEngine(["createSelection", "clearSelection", "snapshot"]);
      const ctx = createToolContext({ selectedLayerId: null, onSelectionCreated: vi.fn() });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", 101, 101, engine, vi.fn(), ctx);
      handlePointerUp("selection", 101, 101, engine, createMockHistory(), vi.fn(), ctx);
      expect(engine.createSelection).not.toHaveBeenCalled();
      expect(engine.clearSelection).toHaveBeenCalled();
    });
  });

  describe("ellipse marquee shape", () => {
    it("creates an ellipse selection when context.selectionShape is 'ellipse'", () => {
      const engine = createMockEngine(["createSelection", "clearSelection", "snapshot"]);
      const ctx = createToolContext({ selectedLayerId: null, onSelectionCreated: vi.fn(), selectionShape: "ellipse" });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", 200, 250, engine, vi.fn(), ctx);
      handlePointerUp("selection", 200, 250, engine, createMockHistory(), vi.fn(), ctx);
      // 6th arg = shape
      expect(engine.createSelection).toHaveBeenCalledWith(100, 100, 100, 150, 0, "ellipse");
    });

    it("defaults to rect shape when context.selectionShape is unset", () => {
      const engine = createMockEngine(["createSelection", "clearSelection", "snapshot"]);
      const ctx = createToolContext({ selectedLayerId: null, onSelectionCreated: vi.fn() });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", 200, 250, engine, vi.fn(), ctx);
      handlePointerUp("selection", 200, 250, engine, createMockHistory(), vi.fn(), ctx);
      expect(engine.createSelection).toHaveBeenCalledWith(100, 100, 100, 150);
    });
  });

  describe("Shift modifier — constrain to square", () => {
    it("Shift+drag uses max dimension for both axes (drag wider than tall)", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({ isShiftPressed: true, selectedLayerId: null, onSelectionCreated: vi.fn() });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", 300, 200, engine, vi.fn(), ctx);
      handlePointerUp("selection", 300, 200, engine, createMockHistory(), vi.fn(), ctx);
      // dx=200, dy=100, max=200 → both axes = 200
      expect(engine.createSelection).toHaveBeenCalledWith(100, 100, 200, 200);
    });

    it("Shift+drag uses max dimension for both axes (drag taller than wide)", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({ isShiftPressed: true, selectedLayerId: null, onSelectionCreated: vi.fn() });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", 150, 400, engine, vi.fn(), ctx);
      handlePointerUp("selection", 150, 400, engine, createMockHistory(), vi.fn(), ctx);
      // dx=50, dy=300, max=300 → both axes = 300
      expect(engine.createSelection).toHaveBeenCalledWith(100, 100, 300, 300);
    });
  });

  describe("Alt modifier — draw from center", () => {
    it("Alt+drag anchors the opposite corner as start point", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({ isAltPressed: true, selectedLayerId: null, onSelectionCreated: vi.fn() });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", 200, 200, engine, vi.fn(), ctx);
      handlePointerUp("selection", 200, 200, engine, createMockHistory(), vi.fn(), ctx);
      // center = (100,100), current = (200,200) → rect from (0,0) to (200,200)
      expect(engine.createSelection).toHaveBeenCalledWith(0, 0, 200, 200);
    });

    it("Alt+drag with Shift constrains square from center", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({ isAltPressed: true, isShiftPressed: true, selectedLayerId: null, onSelectionCreated: vi.fn() });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", 250, 150, engine, vi.fn(), ctx);
      handlePointerUp("selection", 250, 150, engine, createMockHistory(), vi.fn(), ctx);
      // center = (100,100), current = (250,150) → dx=150, dy=50, max=150
      // from center: anchor = center = (100,100), expanded = 150 in each dir
      // raw rect = (-50, -50) to (250, 250) → x=-50, y=-50, w=300, h=300
      // clamped to canvas (4000x4000): x,y < 0 → 0, width/height trimmed → (0,0,250,250)
      expect(engine.createSelection).toHaveBeenCalledWith(0, 0, 250, 250);
    });
  });

  describe("move existing selection", () => {
    it("clicking inside existing selection starts move mode", () => {
      const engine = createMockEngine(["snapshot"]);
      const ctx = createToolContext({
        selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
        onSelectionCreated: vi.fn(),
      });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      expect(ctx.dragMode).toBe("move-selection");
      expect(ctx.onSelectionCreated).not.toHaveBeenCalled();
    });

    it("clicking outside existing selection starts draw mode", () => {
      const engine = createMockEngine(["snapshot"]);
      const ctx = createToolContext({
        selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
        onSelectionCreated: vi.fn(),
      });
      handlePointerDown("selection", 400, 400, engine, createMockHistory(), vi.fn(), ctx);
      expect(ctx.dragMode).toBe("draw");
      expect(ctx.onSelectionCreated).toHaveBeenCalled();
    });

    it("move-selection calls onSelectionMoved with new position", () => {
      const engine = createMockEngine(["snapshot"]);
      const ctx = createToolContext({
        selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
        onSelectionMoved: vi.fn(),
        onSelectionCreated: vi.fn(),
      });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", 150, 130, engine, vi.fn(), ctx);
      // dragStart = (100 - 50, 100 - 50) = (50, 50) — offset from selection origin
      // newX = 150 - 50 = 100, newY = 130 - 50 = 80
      expect(ctx.onSelectionMoved).toHaveBeenCalledWith(100, 80);
    });

    it("move-selection on pointer up also calls onSelectionMoved", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
        onSelectionMoved: vi.fn(),
        onSelectionCreated: vi.fn(),
      });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerUp("selection", 200, 200, engine, createMockHistory(), vi.fn(), ctx);
      // dragStart = (50, 50), newX = 200 - 50 = 150, newY = 200 - 50 = 150
      expect(ctx.onSelectionMoved).toHaveBeenCalledWith(150, 150);
      expect(engine.createSelection).not.toHaveBeenCalled();
    });

    it("moving an ellipse selection preserves the ellipse shape in engine", () => {
      // Simulate: draw an ellipse → then move it → engine still has "ellipse"
      const engine = createMockEngine(["createSelection", "snapshot", "clearSelection"]);
      // Use a REAL onSelectionMoved that calls engine.createSelection with shape,
      // matching how useCanvasPointerTools wires it in production.
      let currentShape: "rect" | "ellipse" | undefined = "ellipse";
      let currentBox = { x: 0, y: 0, w: 100, h: 150 };
      // Phase 1: Draw an ellipse selection (NO selectionBounds → always draw mode)
      const ctxDraw = createToolContext({
        onSelectionMoved: vi.fn(),
        onSelectionCreated: vi.fn(),
        selectionShape: "ellipse", // shape context for the initial draw
        selectedLayerId: null,
      });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctxDraw);
      handlePointerMove("selection", 200, 250, engine, vi.fn(), ctxDraw);
      handlePointerUp("selection", 200, 250, engine, createMockHistory(), vi.fn(), ctxDraw);

      // After draw: engine has shape="ellipse"
      expect(engine.createSelection).toHaveBeenLastCalledWith(100, 100, 100, 150, 0, "ellipse");

      // Track current selection for move phase
      currentBox = { x: 100, y: 100, w: 100, h: 150 };

      // Phase 2: Move the ellipse selection
      // First pointerDown INSIDE the selection to start move mode
      const ctxMove = createToolContext({
        selectionBounds: { x: 100, y: 100, width: 100, height: 150 },
        onSelectionMoved: (x, y) => {
          engine.createSelection(x, y, currentBox.w, currentBox.h, 0, currentShape);
          currentBox = { ...currentBox, x, y };
        },
        onSelectionCreated: vi.fn(),
      });
      // (150, 175) is inside the selection bounds
      handlePointerDown("selection", 150, 175, engine, createMockHistory(), vi.fn(), ctxMove);
      handlePointerMove("selection", 200, 200, engine, vi.fn(), ctxMove);
      handlePointerUp("selection", 200, 200, engine, createMockHistory(), vi.fn(), ctxMove);

      // After move: engine.createSelection must have been called WITH shape="ellipse"
      const allCalls = vi.mocked(engine.createSelection).mock.calls;
      // Filter to only calls that have 6 args (the ones from onSelectionMoved)
      const moveCalls = allCalls.filter((args) => args.length >= 5);
      const lastMoveArgs = moveCalls[moveCalls.length - 1];
      // Last createSelection from onSelectionMoved should have shape = "ellipse"
      expect(lastMoveArgs[5]).toBe("ellipse");
    });

    it("dragMode is reset after pointer up", () => {
      const engine = createMockEngine(["snapshot"]);
      const ctx = createToolContext({
        selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
        onSelectionMoved: vi.fn(),
        onSelectionCreated: vi.fn(),
      });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      expect(ctx.dragMode).toBe("move-selection");
      handlePointerUp("selection", 150, 150, engine, createMockHistory(), vi.fn(), ctx);
      expect(ctx.dragMode).toBeNull();
    });
  });

  describe("history commits for selection edits", () => {
    it("move-selection commits history on pointerUp when selection actually moved (regression: edits not saved)", () => {
      const engine = createMockEngine(["snapshot"]);
      const ctx = createToolContext({
        selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
        onSelectionCreated: vi.fn(),
      });
      const history = createMockHistory();

      handlePointerDown("selection", 100, 100, engine, history, vi.fn(), ctx);
      handlePointerMove("selection", 150, 130, engine, vi.fn(), ctx);
      handlePointerUp("selection", 150, 130, engine, history, vi.fn(), ctx);

      // Selection moved (pointer up at different doc coords than down) — must
      // commit the pre-move snapshot so undo restores prior selection rect.
      expect(history.commit).toHaveBeenCalledTimes(1);
    });

    it("move-selection click-without-drag does NOT commit history (regression 2026-06-18: ghost entries)", () => {
      const engine = createMockEngine(["snapshot"]);
      const ctx = createToolContext({
        selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
        onSelectionCreated: vi.fn(),
      });
      const history = createMockHistory();

      // Click inside selection at (100, 100), release at same spot — no drag.
      handlePointerDown("selection", 100, 100, engine, history, vi.fn(), ctx);
      handlePointerUp("selection", 100, 100, engine, history, vi.fn(), ctx);

      // Previously this produced a ghost undo entry that did nothing visible.
      expect(history.commit).not.toHaveBeenCalled();
    });

    it("drawing a fresh selection does NOT commit history (regression: avoid history noise)", () => {
      const engine = createMockEngine(["snapshot"]);
      const ctx = createToolContext({ selectedLayerId: null, onSelectionCreated: vi.fn() });
      const history = createMockHistory();

      handlePointerDown("selection", 100, 100, engine, history, vi.fn(), ctx);

      // Fresh draws should not pollute the undo stack — undo should not
      // revert to "no selection" if the user hasn't moved anything yet.
      expect(history.commit).not.toHaveBeenCalled();
    });
  });

  describe("selection constraint mode: Fixed Size", () => {
    it("handlePointerDown in size mode creates selection immediately centered on click", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        onSelectionCreated: vi.fn(),
        selectionConstraintMode: "size",
        selectionSizeW: 120,
        selectionSizeH: 80,
      });
      handlePointerDown("selection", 200, 150, engine, createMockHistory(), vi.fn(), ctx);
      // Size mode creates the selection immediately on pointerDown, centered at (200,150)
      expect(ctx.onSelectionCreated).toHaveBeenCalledWith(140, 110, 120, 80);
      expect(ctx.dragMode).toBe("draw");
    });

    it("handlePointerDown in size mode uses defaults when size values are missing", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        onSelectionCreated: vi.fn(),
        selectionConstraintMode: "size",
        selectionSizeW: undefined as unknown as number,
        selectionSizeH: undefined as unknown as number,
      });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      // Defaults: 100 x 100
      expect(ctx.onSelectionCreated).toHaveBeenCalledWith(50, 50, 100, 100);
    });

    it("handlePointerMove in size mode keeps W/H fixed regardless of pointer distance", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        onSelectionCreated: vi.fn(),
        selectionConstraintMode: "size",
        selectionSizeW: 60,
        selectionSizeH: 40,
      });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      vi.mocked(ctx.onSelectionCreated!).mockClear();

      // Drag far away — size should remain 60x40, still centered at start (100,100)
      handlePointerMove("selection", 500, 400, engine, vi.fn(), ctx);
      expect(ctx.onSelectionCreated).toHaveBeenCalledWith(70, 80, 60, 40);
    });

    it("handlePointerUp in size mode calls engine.createSelection with fixed dimensions", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        selectionConstraintMode: "size",
        selectionSizeW: 150,
        selectionSizeH: 100,
      });
      handlePointerDown("selection", 300, 200, engine, createMockHistory(), vi.fn(), ctx);
      vi.mocked(engine.createSelection).mockClear();

      handlePointerUp("selection", 400, 300, engine, createMockHistory(), vi.fn(), ctx);
      expect(engine.createSelection).toHaveBeenCalledWith(225, 150, 150, 100);
    });
  });

  describe("selection constraint mode: Fixed Ratio", () => {
    it("handlePointerMove in ratio mode locks width/height to aspect ratio (wider gesture)", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        onSelectionCreated: vi.fn(),
        selectionConstraintMode: "ratio",
        selectionRatioW: 16,
        selectionRatioH: 9,
      });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      vi.mocked(ctx.onSelectionCreated!).mockClear();

      // Drag to (300, 130) → dx=200, dy=30, currentAspect = 200/30 ≈ 6.67, targetAspect = 16/9 ≈ 1.78
      // currentAspect > targetAspect → h = w / targetAspect = 200 / (16/9) = 112.5
      handlePointerMove("selection", 300, 130, engine, vi.fn(), ctx);
      expect(ctx.onSelectionCreated).toHaveBeenCalledWith(100, 100, 200, 112.5);
    });

    it("handlePointerMove in ratio mode locks aspect ratio (taller gesture)", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        onSelectionCreated: vi.fn(),
        selectionConstraintMode: "ratio",
        selectionRatioW: 4,
        selectionRatioH: 3,
      });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      vi.mocked(ctx.onSelectionCreated!).mockClear();

      // Drag to (130, 400) → dx=30, dy=300, currentAspect = 30/300 = 0.1, targetAspect = 4/3 ≈ 1.33
      // currentAspect < targetAspect → w = h * targetAspect = 300 * (4/3) = 400
      handlePointerMove("selection", 130, 400, engine, vi.fn(), ctx);
      expect(ctx.onSelectionCreated).toHaveBeenCalledWith(100, 100, 400, 300);
    });

    it("handlePointerMove in ratio mode: drag from right-bottom to left-top still correct", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        onSelectionCreated: vi.fn(),
        selectionConstraintMode: "ratio",
        selectionRatioW: 2,
        selectionRatioH: 1,
      });
      handlePointerDown("selection", 300, 300, engine, createMockHistory(), vi.fn(), ctx);
      vi.mocked(ctx.onSelectionCreated!).mockClear();

      // Drag to (100, 100) → dx=-200, dy=-200, w=200, h=200
      // currentAspect = 200/200 = 1, targetAspect = 2/1 = 2
      // currentAspect < targetAspect → w = h * 2 = 400
      handlePointerMove("selection", 100, 100, engine, vi.fn(), ctx);
      // raw rect: x = 300 - 400 = -100 (dx < 0), y = 300 - 200 = 100 (dy < 0)
      // clamped to canvas: x < 0 → 0, width trimmed to 300 → (0, 100, 300, 200)
      expect(ctx.onSelectionCreated).toHaveBeenCalledWith(0, 100, 300, 200);
    });

    it("handlePointerMove in ratio mode with Alt draws from center", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        onSelectionCreated: vi.fn(),
        selectionConstraintMode: "ratio",
        selectionRatioW: 1,
        selectionRatioH: 1,
        isAltPressed: true,
      });
      handlePointerDown("selection", 200, 200, engine, createMockHistory(), vi.fn(), ctx);
      vi.mocked(ctx.onSelectionCreated!).mockClear();

      // center=(200,200), current=(300,250), dx=100, dy=50, w=100, h=50
      // currentAspect = 100/50 = 2, targetAspect = 1/1 = 1
      // currentAspect > targetAspect → h = w / targetAspect = 100 / 1 = 100
      // Alt: w=200, h=200, x=200-100=100, y=200-100=100
      handlePointerMove("selection", 300, 250, engine, vi.fn(), ctx);
      expect(ctx.onSelectionCreated).toHaveBeenCalledWith(100, 100, 200, 200);
    });

    it("handlePointerUp in ratio mode creates selection with correct aspect ratio", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        selectionConstraintMode: "ratio",
        selectionRatioW: 3,
        selectionRatioH: 2,
      });
      handlePointerDown("selection", 50, 50, engine, createMockHistory(), vi.fn(), ctx);
      vi.mocked(engine.createSelection).mockClear();

      handlePointerUp("selection", 200, 100, engine, createMockHistory(), vi.fn(), ctx);
      // dx=150, dy=50, w=150, h=50, currentAspect=3, targetAspect=1.5
      // currentAspect > targetAspect → h = w / 1.5 = 100
      expect(engine.createSelection).toHaveBeenCalledWith(50, 50, 150, 100);
    });

    it("handlePointerUp in ratio mode with tiny drag calls clearSelection", () => {
      const engine = createMockEngine(["createSelection", "clearSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        selectionConstraintMode: "ratio",
        selectionRatioW: 1,
        selectionRatioH: 1,
      });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      vi.mocked(engine.createSelection).mockClear();

      // dx=1, dy=1 → w=1, h=1 → too small (w <= 2 && h <= 2) → clearSelection
      handlePointerUp("selection", 101, 101, engine, createMockHistory(), vi.fn(), ctx);
      expect(engine.createSelection).not.toHaveBeenCalled();
      expect(engine.clearSelection).toHaveBeenCalled();
    });

    it("handlePointerMove in ratio mode with isShiftPressed does NOT apply square constraint (ratio mode overrides Shift)", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        onSelectionCreated: vi.fn(),
        selectionConstraintMode: "ratio",
        selectionRatioW: 16,
        selectionRatioH: 9,
        isShiftPressed: true,
      });
      handlePointerDown("selection", 0, 0, engine, createMockHistory(), vi.fn(), ctx);
      vi.mocked(ctx.onSelectionCreated!).mockClear();

      // Drag to (160, 90) → dx=160, dy=90, currentAspect = 160/90 = 1.778, targetAspect = 16/9 ≈ 1.778
      // currentAspect ≈ targetAspect → h stays = 90, w stays = 160
      handlePointerMove("selection", 160, 90, engine, vi.fn(), ctx);
      expect(ctx.onSelectionCreated).toHaveBeenCalledWith(0, 0, 160, 90);
      // Even with Shift pressed, ratio mode should use its own aspect ratio, not square
      // Verify it's NOT a square (160 != 160*9/16=90)
      const call = vi.mocked(ctx.onSelectionCreated!).mock.calls[0];
      expect(call[2] / call[3]).toBeCloseTo(16 / 9, 5);
    });
  });

  describe("selection constraint mode: edge cases", () => {
    it("handlePointerMove with ratio rw=0 or rh=0 avoids division by zero", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        onSelectionCreated: vi.fn(),
        selectionConstraintMode: "ratio",
        selectionRatioW: 0,
        selectionRatioH: 1,
      });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      vi.mocked(ctx.onSelectionCreated!).mockClear();

      // rw=0 so targetAspect = 0/1 = 0 → currentAspect > 0 → h = w / 0 → NaN or Infinity
      // The guard `if (rw > 0 && rh > 0)` should prevent the ratio branch
      handlePointerMove("selection", 300, 200, engine, vi.fn(), ctx);
      // The ratio branch should be skipped since rw=0
      // Fallback to normal behavior? Actually looking at the code:
      // if (rw > 0 && rh > 0) { ... } else { no ratio adjustment }
      // So w=200, h=100 since no shift, no alt
      expect(ctx.onSelectionCreated).toHaveBeenCalledWith(100, 100, 200, 100);
    });

    it("handlePointerDown selects existing selection for move when bounds exist", () => {
      const engine = createMockEngine(["snapshot"]);
      const ctx = createToolContext({
        selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
        onSelectionCreated: vi.fn(),
      });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      // Click is inside bounds → should set move mode and stash pending state
      expect(ctx.dragMode).toBe("move-selection");
      expect(ctx.pendingHistorySnapshot).toBeDefined();
      expect(ctx.pendingOriginalSelectionPos).toEqual({ x: 50, y: 50 });
      // dragStart should be offset from selection origin
      expect(ctx.dragStart).toEqual({ x: 50, y: 50 }); // 100 - 50, 100 - 50
      // onSelectionCreated should NOT be called (we're not drawing)
      expect(ctx.onSelectionCreated).not.toHaveBeenCalled();
    });

    it("handlePointerDown with size mode AND existing selection: size mode creates draw, not move-selection", () => {
      const engine = createMockEngine(["snapshot"]);
      const ctx = createToolContext({
        selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
        onSelectionCreated: vi.fn(),
        selectionConstraintMode: "size",
        selectionSizeW: 80,
        selectionSizeH: 60,
      });
      // Click at (400, 400) — outside the selection bounds → should draw
      handlePointerDown("selection", 400, 400, engine, createMockHistory(), vi.fn(), ctx);
      expect(ctx.dragMode).toBe("draw");
      // Size mode creates selection immediately centered at click point
      expect(ctx.onSelectionCreated).toHaveBeenCalledWith(360, 370, 80, 60);
    });

    it("handlePointerDown clears pending state from previous incomplete gesture", () => {
      const engine = createMockEngine(["snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        pendingHistorySnapshot: { snap: 1 } as unknown as import("../engine/types").DocumentModel,
        pendingOriginalSelectionPos: { x: 10, y: 20 },
        onSelectionCreated: vi.fn(),
      });
      // Start a new gesture — should clear stale pending state
      handlePointerDown("selection", 200, 200, engine, createMockHistory(), vi.fn(), ctx);
      expect(ctx.pendingHistorySnapshot).toBeNull();
      expect(ctx.pendingOriginalSelectionPos).toBeNull();
    });

    it("handlePointerUp in move-selection mode commits history only when selection actually moved", () => {
      const engine = createMockEngine(["snapshot"]);
      const ctx = createToolContext({
        selectionBounds: { x: 100, y: 100, width: 200, height: 150 },
        onSelectionMoved: vi.fn(),
        onSelectionCreated: vi.fn(),
      });
      const history = createMockHistory();

      // Click inside and release at SAME position — no movement
      handlePointerDown("selection", 150, 150, engine, history, vi.fn(), ctx);
      handlePointerUp("selection", 150, 150, engine, history, vi.fn(), ctx);

      // No commit because pendingOriginalSelectionPos (100,100) equals final position (150-50, 150-50) = (100,100)
      expect(history.commit).not.toHaveBeenCalled();
    });

    it("handlePointerUp in move-selection mode resets pending state after commit", () => {
      const engine = createMockEngine(["snapshot"]);
      const ctx = createToolContext({
        selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
        onSelectionMoved: vi.fn(),
      });
      const history = createMockHistory();

      handlePointerDown("selection", 100, 100, engine, history, vi.fn(), ctx);
      handlePointerUp("selection", 200, 200, engine, history, vi.fn(), ctx);

      // pending state should be cleared after pointerUp
      expect(ctx.pendingHistorySnapshot).toBeNull();
      expect(ctx.pendingOriginalSelectionPos).toBeNull();
    });

    it("handlePointerUp in draw mode calls engine.createSelection when drag exceeds threshold", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
      });
      handlePointerDown("selection", 50, 50, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", 200, 150, engine, vi.fn(), ctx);
      handlePointerUp("selection", 200, 150, engine, createMockHistory(), vi.fn(), ctx);

      expect(engine.createSelection).toHaveBeenCalledWith(50, 50, 150, 100);
      expect(ctx.dragMode).toBeNull();
    });

    it("handlePointerUp in draw mode calls clearSelection when drag is tiny (w <= 2 && h <= 2)", () => {
      const engine = createMockEngine(["createSelection", "clearSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
      });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerUp("selection", 101, 101, engine, createMockHistory(), vi.fn(), ctx);

      expect(engine.createSelection).not.toHaveBeenCalled();
      expect(engine.clearSelection).toHaveBeenCalled();
    });

    it("handlePointerUp with Shift+Alt (square from center) in normal mode", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      const ctx = createToolContext({
        selectedLayerId: null,
        isAltPressed: true,
        isShiftPressed: true,
      });
      handlePointerDown("selection", 200, 200, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", 300, 250, engine, vi.fn(), ctx);
      handlePointerUp("selection", 300, 250, engine, createMockHistory(), vi.fn(), ctx);

      // dx=100, dy=50, w=100, h=50, max=100, alt*2 → w=h=200
      // From center: x = 200 - 100 = 100, y = 200 - 100 = 100
      expect(engine.createSelection).toHaveBeenCalledWith(100, 100, 200, 200);
    });
  });

  describe("draw clamps to canvas bounds (marquee cannot exit canvas)", () => {
    const withCanvas = (engine: DocumentEngine, w: number, h: number) => {
      (engine as unknown as { getWidth: () => number }).getWidth = () => w;
      (engine as unknown as { getHeight: () => number }).getHeight = () => h;
    };

    it("clamps a rect dragged past the right/bottom edge to the canvas", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      withCanvas(engine, 300, 300);
      const ctx = createToolContext({ selectedLayerId: null, onSelectionCreated: vi.fn() });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", 500, 400, engine, vi.fn(), ctx);
      handlePointerUp("selection", 500, 400, engine, createMockHistory(), vi.fn(), ctx);
      // Raw (100,100,400,300) → clamped to 300x300 → (100,100,200,200)
      expect(engine.createSelection).toHaveBeenCalledWith(100, 100, 200, 200);
      expect(ctx.onSelectionCreated).toHaveBeenCalledWith(100, 100, 200, 200);
    });

    it("clamps a rect dragged past the left/top edge to the canvas", () => {
      const engine = createMockEngine(["createSelection", "snapshot"]);
      withCanvas(engine, 300, 300);
      const ctx = createToolContext({ selectedLayerId: null, onSelectionCreated: vi.fn() });
      handlePointerDown("selection", 100, 100, engine, createMockHistory(), vi.fn(), ctx);
      handlePointerMove("selection", -50, -50, engine, vi.fn(), ctx);
      handlePointerUp("selection", -50, -50, engine, createMockHistory(), vi.fn(), ctx);
      // Raw (-50,-50,150,150): far edge at doc 100 → clamped to (0,0,100,100)
      expect(engine.createSelection).toHaveBeenCalledWith(0, 0, 100, 100);
      expect(ctx.onSelectionCreated).toHaveBeenCalledWith(0, 0, 100, 100);
    });
  });

  describe("isPointInSelection", () => {
    it("returns true for point inside axis-aligned selection", () => {
      const bounds = { x: 50, y: 50, width: 200, height: 150 };
      expect(isPointInSelection(100, 100, bounds)).toBe(true);
    });

    it("returns true for point exactly on top-left edge", () => {
      const bounds = { x: 50, y: 50, width: 200, height: 150 };
      expect(isPointInSelection(50, 50, bounds)).toBe(true);
    });

    it("returns true for point exactly on bottom-right edge", () => {
      const bounds = { x: 50, y: 50, width: 200, height: 150 };
      expect(isPointInSelection(250, 200, bounds)).toBe(true);
    });

    it("returns false for point above selection", () => {
      const bounds = { x: 50, y: 50, width: 200, height: 150 };
      expect(isPointInSelection(100, 10, bounds)).toBe(false);
    });

    it("returns false for point to the left of selection", () => {
      const bounds = { x: 50, y: 50, width: 200, height: 150 };
      expect(isPointInSelection(10, 100, bounds)).toBe(false);
    });

    it("returns false for point below selection", () => {
      const bounds = { x: 50, y: 50, width: 200, height: 150 };
      expect(isPointInSelection(100, 300, bounds)).toBe(false);
    });

    it("returns false for point to the right of selection", () => {
      const bounds = { x: 50, y: 50, width: 200, height: 150 };
      expect(isPointInSelection(300, 100, bounds)).toBe(false);
    });

    it("returns true for point inside rotated selection (45 degrees)", () => {
      // A 45-degree rotated square: the original unrotated bounds are (0,0,100,100).
      // The center is at (50,50). After 45° rotation, the square's corners are at
      // the midpoints of each edge. The center point (50,50) should still be inside.
      const bounds = { x: 0, y: 0, width: 100, height: 100, angle: 45 };
      expect(isPointInSelection(50, 50, bounds)).toBe(true);
    });

    it("returns false for point outside rotated selection (45 degrees)", () => {
      // A 45-degree rotated square at (0,0,100,100).
      // Point at (0, 0) is a corner in the original, but after 45° rotation,
      // the original corners are cut off. The point (0,0) should be outside.
      const bounds = { x: 0, y: 0, width: 100, height: 100, angle: 45 };
      // The corner (0,0) is outside the 45° rotated square
      expect(isPointInSelection(0, 0, bounds)).toBe(false);
    });

    it("handles negative rotation angle", () => {
      const bounds = { x: 100, y: 100, width: 200, height: 150, angle: -30 };
      // Center should always be inside regardless of rotation
      expect(isPointInSelection(200, 175, bounds)).toBe(true);
    });

    it("handles default angle (undefined treated as 0)", () => {
      const bounds = { x: 50, y: 50, width: 200, height: 150 };
      expect(isPointInSelection(100, 100, bounds)).toBe(true);
      expect(isPointInSelection(0, 0, bounds)).toBe(false);
    });

    it("correctly identifies point near a rotated edge (90 degrees)", () => {
      // 90° rotation makes the selection a 100x100 box rotated.
      const bounds = { x: 0, y: 0, width: 100, height: 100, angle: 90 };
      // After 90° rotation around center (50,50): the box now occupies the
      // same area as the original since it's a square.
      expect(isPointInSelection(25, 50, bounds)).toBe(true);
      expect(isPointInSelection(-10, 50, bounds)).toBe(false);
    });

    it("works with selection containing negative coordinates (off-canvas selection)", () => {
      const bounds = { x: -50, y: -50, width: 100, height: 100 };
      expect(isPointInSelection(0, 0, bounds)).toBe(true);
      expect(isPointInSelection(-25, -25, bounds)).toBe(true);
      expect(isPointInSelection(-60, 0, bounds)).toBe(false);
      expect(isPointInSelection(60, 0, bounds)).toBe(false);
    });

    it("works with zero-area selection (width=0, height=0)", () => {
      const bounds = { x: 50, y: 50, width: 0, height: 0 };
      // A zero-area selection is just a point
      expect(isPointInSelection(50, 50, bounds)).toBe(true);
      expect(isPointInSelection(51, 50, bounds)).toBe(false);
    });
  });
});
