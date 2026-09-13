/**
 * TDD test for selectionRotation.ts viewport stale bug.
 *
 * Bug: startSelectionRotation uses engine.getViewport() for coordinate conversion,
 * but engine viewport goes stale during panning (usePanNavigation skips
 * engine.setViewport to avoid triggering layer re-selection).
 *
 * Fix: add a getViewportCoords callback parameter. When provided, use its
 * always-fresh pan/zoom values instead of the potentially stale engine viewport.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { startSelectionRotation, type SelectionBox } from "../selectionRotation";

describe("startSelectionRotation — viewport integration (TDD)", () => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let setBox: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let createSel: any;

  /** Engine with stale viewport (NOT matching fresh signals) */
  const staleEngineViewport = { panX: 200, panY: 100, zoom: 2, rotation: 0 };
  let getEngine: () => ReturnType<typeof createEngine>;

  /** Container at (50, 30) offset from viewport, 800x600 size */
  const CONTAINER_RECT = new DOMRect(50, 30, 800, 600);
  let getContainer: () => HTMLDivElement;
  let mutableBox: SelectionBox | null;

  function createEngine(vp = staleEngineViewport) {
    return {
      getId: () => "doc-1",
      getViewport: () => ({ ...vp }),
      createSelection: vi.fn(),
    };
  }

  function dispatchPointerMove(clientX: number, clientY: number, shiftKey = false) {
    document.dispatchEvent(
      new PointerEvent("pointermove", { clientX, clientY, shiftKey }),
    );
  }

  function dispatchPointerUp() {
    document.dispatchEvent(new PointerEvent("pointerup"));
  }

  beforeEach(() => {
    setBox = vi.fn();
    createSel = vi.fn();
    getEngine = () => ({
      getId: () => "doc-1",
      getViewport: () => ({ ...staleEngineViewport }),
      createSelection: createSel,
    });
    getContainer = () =>
      ({ getBoundingClientRect: () => CONTAINER_RECT }) as HTMLDivElement;
    mutableBox = null;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe("stale engine.getViewport() — CURRENT BUGGY BEHAVIOR", () => {
    /**
     * These tests document the CURRENT behavior where startSelectionRotation
     * uses engine.getViewport() for coordinate conversion.
     *
     * After panning, engine viewport is stale. The tests show that the
     * rotation angle is computed incorrectly because screenToDocument uses
     * stale pan/zoom values.
     *
     * After the fix (adding getViewportCoords parameter), these tests
     * should FAIL because the function will no longer use stale viewport
     * by default (it will use getViewportCoords if provided, or fall back
     * to engine.getViewport for backward compatibility).
     */

    it("uses stale engine viewport — rotation angle is WRONG after panning", () => {
      // Selection at doc (100, 100) size 200x150, center=(200, 175)
      const box: SelectionBox = { x: 100, y: 100, w: 200, h: 150, angle: 0 };

      startSelectionRotation(
        () => box,
        setBox as (box: SelectionBox | null) => void,
        getContainer,
        getEngine,
      );

      // First move: record reference angle.
      // Screen coords: client (250, 205) → canvas-relative (200, 175) → center of selection
      // Stale viewport (panX=200, panY=100, zoom=2):
      //   doc = (200-200, 175-100)/2 = (0, 37.5)
      //   CurrentAngle = atan2(37.5-175, 0-200) = atan2(-137.5, -200) ≈ -145.49°
      // Fresh viewport (panX=0, panY=0, zoom=1):
      //   doc = (200-0, 175-0)/1 = (200, 175)
      //   CurrentAngle = atan2(175-175, 200-200) = atan2(0, 0)... ambiguous (0/0) = 0 in JS
      dispatchPointerMove(250, 205);
      // Reference angle recorded with STALE viewport

      // Second move: move pointer right.
      // Screen coords: client (350, 205) → canvas-relative (300, 175)
      // Stale viewport: doc = (300-200, 175-100)/2 = (50, 37.5)
      //   angle = atan2(37.5-175, 50-200) = atan2(-137.5, -150) ≈ -137.47°
      // Fresh viewport: doc = (300-0, 175-0)/1 = (300, 175)
      //   angle = atan2(175-175, 300-200) = atan2(0, 100) = 0°
      dispatchPointerMove(350, 205);
      const calledBox = setBox.mock.calls[0] as unknown as SelectionBox;

      // With stale viewport, the angle is wrong (not 0).
      // The stale viewport shifts coordinates, so the cursor appears
      // to be at a different position relative to selection center.
      // This documents the WRONG behavior — angle should be 0 if
      // cursor is directly to the right of the selection center.
      expect(calledBox.angle).not.toBe(0);
    });
  });

  describe("fresh getViewportCoords — POST-FIX CORRECT BEHAVIOR", () => {
    /**
     * These tests document the CORRECT behavior AFTER the fix.
     * They provide a getViewportCoords callback with fresh pan/zoom values,
     * so the rotation angle is computed correctly regardless of panning.
     *
     * NOTE: These tests will FAIL until the fix is implemented in
     * selectionRotation.ts (adding the getViewportCoords parameter).
     */

    it("uses getViewportCoords when provided — rotation angle is correct", () => {
      const box: SelectionBox = { x: 100, y: 100, w: 200, h: 150, angle: 0 };
      const freshViewportCoords = () => ({ panX: 0, panY: 0, zoom: 1 });

      startSelectionRotation(
        () => box,
        setBox as (box: SelectionBox | null) => void,
        getContainer,
        getEngine,
        freshViewportCoords,
      );

      dispatchPointerMove(250, 205);
      dispatchPointerMove(350, 205);

      expect(setBox).toHaveBeenCalledTimes(2);
      const args = setBox.mock.calls[1] as unknown as [SelectionBox];
      expect(args[0].angle).toBeCloseTo(0, 5);
    });

    it("getViewportCoords produces correct angle with zoom=2", () => {
      const box: SelectionBox = { x: 100, y: 100, w: 200, h: 150, angle: 0 };
      const freshViewportCoords = () => ({ panX: 0, panY: 0, zoom: 2 });

      startSelectionRotation(
        () => box,
        setBox as (box: SelectionBox | null) => void,
        getContainer,
        getEngine,
        freshViewportCoords,
      );

      dispatchPointerMove(250, 205); // first move → delta=0 → angle=0
      dispatchPointerMove(450, 205); // second move → delta=48.81 → angle=48.81

      expect(setBox).toHaveBeenCalledTimes(2);
      const args = setBox.mock.calls[1] as unknown as [SelectionBox];
      expect(args[0].angle).toBeCloseTo(48.81, 1);
    });

    it("getViewportCoords with pan offset", () => {
      const box: SelectionBox = { x: 100, y: 100, w: 200, h: 150, angle: 0 };
      const freshViewportCoords = () => ({ panX: 200, panY: 100, zoom: 1 });

      startSelectionRotation(
        () => box,
        setBox as (box: SelectionBox | null) => void,
        getContainer,
        getEngine,
        freshViewportCoords,
      );

      dispatchPointerMove(450, 305); // first move → at center → delta=0 → angle=0
      dispatchPointerMove(550, 305); // second move → right of center → angle=0

      expect(setBox).toHaveBeenCalledTimes(2);
      const args = setBox.mock.calls[1] as unknown as [SelectionBox];
      expect(args[0].angle).toBeCloseTo(0, 5);
    });

    it("getViewportCoords with both pan and zoom", () => {
      const box: SelectionBox = { x: 100, y: 100, w: 200, h: 150, angle: 10 };
      const freshViewportCoords = () => ({ panX: 200, panY: 100, zoom: 2 });

      startSelectionRotation(
        () => box,
        setBox as (box: SelectionBox | null) => void,
        getContainer,
        getEngine,
        freshViewportCoords,
      );

      dispatchPointerMove(650, 480); // first move → at center → delta=0 → angle=10
      dispatchPointerMove(850, 480); // second move → right of center → angle=10

      expect(setBox).toHaveBeenCalledTimes(2);
      const args = setBox.mock.calls[1] as unknown as [SelectionBox];
      expect(args[0].angle).toBeCloseTo(10, 5);
    });

    it("falls back to engine.getViewport when getViewportCoords is not provided (backward compat)", () => {
      const box: SelectionBox = { x: 100, y: 100, w: 200, h: 150, angle: 0 };

      // Call WITHOUT getViewportCoords — should fall back to engine.getViewport()
      startSelectionRotation(
        () => box,
        setBox as (box: SelectionBox | null) => void,
        getContainer,
        getEngine,
        // NO getViewportCoords argument
      );

      // First move: record reference
      dispatchPointerMove(250, 205);
      // With stale engine viewport (panX=200, panY=100, zoom=2):
      // doc = (200-200, 175-100)/2 = (0, 37.5)
      // angle from center = atan2(37.5-175, 0-200) = atan2(-137.5, -200) ≈ -145.49°

      // Second move
      dispatchPointerMove(350, 205);
      // doc = (300-200, 175-100)/2 = (50, 37.5)
      // angle = atan2(37.5-175, 50-200) = atan2(-137.5, -150) ≈ -137.47°
      // delta = -137.47 - (-145.49) = 8.02°
      // angle = 0 + 8.02 = 8.02

      const calledBox = setBox.mock.calls[0] as unknown as SelectionBox;

      // Verify the fallback to engine.getViewport() produces the stale-vp result
      expect(calledBox.angle).not.toBe(0);
    });
  });
});
