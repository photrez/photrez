import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { ViewportState } from "@/engine/types";
import { startSelectionRotation, type SelectionBox } from "../selectionRotation";

vi.mock("@/viewport/coords", () => ({
  screenToDocument: (x: number, y: number) => ({ x, y }),
}));

describe("startSelectionRotation", () => {
  let setBox: ReturnType<typeof vi.fn>;
  let getContainer: () => HTMLDivElement;
  let getEngine: () => { getViewport(): ViewportState; createSelection(x: number, y: number, w: number, h: number, angle?: number, shape?: "rect" | "ellipse"): void } | null;

  beforeEach(() => {
    setBox = vi.fn();
    getContainer = () =>
      ({ getBoundingClientRect: () => new DOMRect(0, 0, 800, 600) }) as HTMLDivElement;
    getEngine = () => ({ getViewport: () => ({}) as ViewportState, createSelection: vi.fn() });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("returns early when no selection box — no listeners registered", () => {
    const addSpy = vi.spyOn(document, "addEventListener");
    startSelectionRotation(() => null, setBox as (box: SelectionBox | null) => void, getContainer, getEngine);
    expect(addSpy).not.toHaveBeenCalled();
  });

  it("returns early when engine is null — no listeners registered", () => {
    const addSpy = vi.spyOn(document, "addEventListener");
    startSelectionRotation(
      () => ({ x: 0, y: 0, w: 100, h: 100, angle: 0 }),
      setBox as (box: SelectionBox | null) => void,
      getContainer,
      () => null,
    );
    expect(addSpy).not.toHaveBeenCalled();
  });

  it("registers pointermove and pointerup on document when box exists", () => {
    const addSpy = vi.spyOn(document, "addEventListener");
    startSelectionRotation(
      () => ({ x: 0, y: 0, w: 100, h: 100, angle: 0 }),
      setBox as (box: SelectionBox | null) => void,
      getContainer,
      getEngine,
    );
    expect(addSpy).toHaveBeenCalledWith("pointermove", expect.any(Function));
    expect(addSpy).toHaveBeenCalledWith("pointerup", expect.any(Function), true);
  });

  it("removes listeners on pointerup", () => {
    const removeSpy = vi.spyOn(document, "removeEventListener");
    startSelectionRotation(
      () => ({ x: 0, y: 0, w: 100, h: 100, angle: 0 }),
      setBox as (box: SelectionBox | null) => void,
      getContainer,
      getEngine,
    );
    document.dispatchEvent(new PointerEvent("pointerup"));
    expect(removeSpy).toHaveBeenCalledWith("pointermove", expect.any(Function));
    expect(removeSpy).toHaveBeenCalledWith("pointerup", expect.any(Function), true);
  });

  it("first pointermove records reference angle and preserves existing angle (no jump)", () => {
    startSelectionRotation(
      () => ({ x: 100, y: 100, w: 200, h: 150, angle: 10 }),
      setBox as (box: SelectionBox | null) => void,
      getContainer,
      getEngine,
    );
    // Move pointer above the box center (200,175) — at (200, 75)
    document.dispatchEvent(new PointerEvent("pointermove", { clientX: 200, clientY: 75 }));
    expect(setBox).toHaveBeenCalledWith(
      expect.objectContaining({ x: 100, y: 100, w: 200, h: 150 }),
    );
    const calledBox = setBox.mock.calls[0][0] as SelectionBox;
    // First move records reference angle → delta=0 → angle stays at 10° (existing)
    // OLD BUG: hardcoded initialAngle=180 gave angle=-270 normalized to 90° → JUMP!
    expect(calledBox.angle).toBe(10);
  });

  it("second pointermove applies delta on top of existing angle", () => {
    startSelectionRotation(
      () => ({ x: 100, y: 100, w: 200, h: 150, angle: 5 }),
      setBox as (box: SelectionBox | null) => void,
      getContainer,
      getEngine,
    );
    // First move: record reference (200, 75) → angle stays at 5
    document.dispatchEvent(new PointerEvent("pointermove", { clientX: 200, clientY: 75 }));
    expect(setBox.mock.calls[0][0].angle).toBe(5);

    // Second move: move right to (300, 175) — angle from center = atan2(0, 100) = 0°
    document.dispatchEvent(new PointerEvent("pointermove", { clientX: 300, clientY: 175 }));
    const calledBox = setBox.mock.calls[1][0] as SelectionBox;
    // referenceAngle was -90, currentAngle = 0, delta = 90, angle = 5 + 90 = 95
    expect(calledBox.angle).toBeCloseTo(95, 5);
  });

  it("snaps to 15-degree increments when Shift is held (applied to final angle)", () => {
    startSelectionRotation(
      () => ({ x: 100, y: 100, w: 200, h: 150, angle: 7 }),
      setBox as (box: SelectionBox | null) => void,
      getContainer,
      getEngine,
    );
    // First move: record reference (200, 75) → -90°
    document.dispatchEvent(new PointerEvent("pointermove", { clientX: 200, clientY: 75 }));
    // Second move: move right to (300, 175) → 0°, delta = 90°
    document.dispatchEvent(
      new PointerEvent("pointermove", { clientX: 300, clientY: 175, shiftKey: true }),
    );
    const calledBox = setBox.mock.calls[1][0] as SelectionBox;
    // angle = 7 + 90 = 97 → rounded to 15 → 90 → snapped to 90
    expect(calledBox.angle % 15).toBe(0);
    expect(calledBox.angle).toBeCloseTo(90, 5);
  });

  it("rotates counter-clockwise (negative delta)", () => {
    startSelectionRotation(
      () => ({ x: 100, y: 100, w: 200, h: 150, angle: 0 }),
      setBox as (box: SelectionBox | null) => void,
      getContainer,
      getEngine,
    );
    // First move: record reference (200, 75) → -90°
    document.dispatchEvent(new PointerEvent("pointermove", { clientX: 200, clientY: 75 }));
    // Second move: move left to (100, 75) → angle from center = atan2(-100, -100) = -135°
    document.dispatchEvent(new PointerEvent("pointermove", { clientX: 100, clientY: 75 }));
    const calledBox = setBox.mock.calls[1][0] as SelectionBox;
    // delta = -135 - (-90) = -45, angle = 0 + (-45) = -45
    expect(calledBox.angle).toBeCloseTo(-45, 5);
  });

  it("commits the rotated angle to engine on pointerup", () => {
    const createSel = vi.fn();
    const engine = { getViewport: () => ({}) as ViewportState, createSelection: createSel };
    // Use a mutable variable so getSelectionBox reflects the latest angle
    let mutableAngle = 0;
    startSelectionRotation(
      () => ({ x: 100, y: 100, w: 200, h: 150, angle: mutableAngle }),
      (box) => { if (box) { mutableAngle = box.angle; (setBox as any)(box); } },
      getContainer,
      () => engine,
    );
    // First move: record reference (200, 75) → -90°
    document.dispatchEvent(new PointerEvent("pointermove", { clientX: 200, clientY: 75 }));
    // Second move: (300, 175) → 0°, delta = 90° → angle = 0 + 90 = 95...
    // Actually box.angle was 0, mutableAngle is 0. delta = 90.
    // angle = 0 + 90 = 90 (normalized)
    document.dispatchEvent(new PointerEvent("pointermove", { clientX: 300, clientY: 175 }));
    // pointerup should commit the rotated angle
    document.dispatchEvent(new PointerEvent("pointerup"));
    // mutableAngle should now be 90 (box.angle(0) + delta(90))
    expect(createSel).toHaveBeenCalledWith(100, 100, 200, 150, 90, undefined);
  });
});
