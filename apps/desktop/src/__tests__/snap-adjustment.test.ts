import { describe, it, expect } from "vitest";
import { computeSnapAdjustment, SnapRect } from "../viewport/smartGuides";

describe("computeSnapAdjustment", () => {
  it("returns zero delta and no lines when no target is within threshold", () => {
    const moving = { x: 0, y: 0, w: 50, h: 50 };
    const targets = [{ x: 500, y: 500, w: 50, h: 50 }];
    const result = computeSnapAdjustment(moving, targets, 5);
    expect(result.dx).toBe(0);
    expect(result.dy).toBe(0);
    expect(result.lines).toEqual([]);
  });

  it("snaps moving left edge to target left edge", () => {
    const moving = { x: 98, y: 100, w: 50, h: 50 };
    const targets = [{ x: 100, y: 200, w: 50, h: 50 }];
    const result = computeSnapAdjustment(moving, targets, 5);
    expect(result.dx).toBe(2);
    expect(result.dy).toBe(0);
    expect(result.lines.length).toBe(1);
    expect(result.lines[0].x1).toBe(100);
    expect(result.lines[0].x2).toBe(100);
  });

  it("snaps moving center to target center", () => {
    const moving = { x: 75, y: 100, w: 50, h: 50 };
    const targets = [{ x: 75, y: 200, w: 50, h: 50 }];
    const result = computeSnapAdjustment(moving, targets, 5);
    expect(result.dx).toBe(0);
    expect(result.dy).toBe(0);
    expect(result.lines.length).toBeGreaterThan(0);
  });

  it("snaps moving center to canvas horizontal center (synthetic vertical line)", () => {
    // doc width 1000; moving at x=298 with w=200 → right edge = 498
    // vertical center line: x=500 spanning full height (target via Infinity sentinels)
    // dx=2 to push right edge from 498 to 500
    const moving = { x: 298, y: 0, w: 200, h: 200 };
    const targets = [
      { x: 500, y: -Infinity, w: 0, h: Infinity },
    ];
    const result = computeSnapAdjustment(moving, targets, 5);
    expect(result.dx).toBe(2);
    expect(result.dy).toBe(0);
    expect(result.lines.length).toBe(1);
    expect(result.lines[0].x1).toBe(500);
  });

  it("snaps moving top edge to target top edge", () => {
    const moving = { x: 100, y: 98, w: 50, h: 50 };
    const targets = [{ x: 200, y: 100, w: 50, h: 50 }];
    const result = computeSnapAdjustment(moving, targets, 5);
    expect(result.dx).toBe(0);
    expect(result.dy).toBe(2);
    expect(result.lines.length).toBe(1);
    expect(result.lines[0].y1).toBe(100);
    expect(result.lines[0].y2).toBe(100);
  });

  it("picks the nearest target when multiple are within threshold (X axis)", () => {
    // left = 0, distance 2
    // left 2, distance 2
    // left 4, distance 4
    const moving = { x: 0, y: 0, w: 50, h: 50 };
    const targets = [
      { x: 2, y: 200, w: 50, h: 50 },
      { x: 4, y: 200, w: 50, h: 50 },
    ];
    const result = computeSnapAdjustment(moving, targets, 5);
    expect(result.dx).toBe(2);
    expect(result.lines.length).toBe(1);
    expect(result.lines[0].x1).toBe(2);
  });

  it("emits at most one line per axis (0, 1, or 2 total)", () => {
    const moving = { x: 98, y: 98, w: 50, h: 50 };
    const targets = [{ x: 100, y: 100, w: 50, h: 50 }];
    const result = computeSnapAdjustment(moving, targets, 5);
    expect(result.lines.length).toBeLessThanOrEqual(2);
  });

  it("respects custom threshold (no snap when distance >= threshold)", () => {
    const moving = { x: 95, y: 100, w: 50, h: 50 };
    const targets = [{ x: 100, y: 200, w: 50, h: 50 }];
    const result = computeSnapAdjustment(moving, targets, 3);
    expect(result.dx).toBe(0);
    expect(result.dy).toBe(0);
    expect(result.lines).toEqual([]);
  });

  it("uses default threshold of 5 when none provided", () => {
    // left = 96, distance to 100 is 4
    const moving = { x: 96, y: 0, w: 50, h: 50 };
    const targets = [{ x: 100, y: 0, w: 50, h: 50 }];
    const result = computeSnapAdjustment(moving, targets);
    expect(result.dx).toBe(4);
    expect(result.lines.length).toBeGreaterThanOrEqual(1);
    expect(result.lines[0].x1).toBe(100);
  });

  it("emits a vertical guide for a horizontal-axis snap and vice versa", () => {
    const xOnly = computeSnapAdjustment(
      { x: 98, y: 0, w: 50, h: 50 },
      [{ x: 100, y: 200, w: 50, h: 50 }],
      5,
    );
    expect(xOnly.lines.length).toBeGreaterThan(0);
    expect(xOnly.lines[0].x1).toBe(xOnly.lines[0].x2);

    const yOnly = computeSnapAdjustment(
      { x: 0, y: 98, w: 50, h: 50 },
      [{ x: 200, y: 100, w: 50, h: 50 }],
      5,
    );
    expect(yOnly.lines.length).toBeGreaterThan(0);
    expect(yOnly.lines[0].y1).toBe(yOnly.lines[0].y2);
  });

  it("produces finite guide-line endpoints when snapping to synthetic center line", () => {
    const moving = { x: 298, y: 0, w: 200, h: 200 };
    const targets = [
      { x: 500, y: -Infinity, w: 0, h: Infinity },
    ];
    const result = computeSnapAdjustment(moving, targets, 5);
    expect(result.lines.length).toBe(1);
    expect(Number.isFinite(result.lines[0].y1)).toBe(true);
    expect(Number.isFinite(result.lines[0].y2)).toBe(true);
  });

  it("snaps layer 10px from canvas edge (within 12px threshold)", () => {
    const moving = { x: 10, y: 100, w: 50, h: 50 };
    const targets: SnapRect[] = [
      { x: 0, y: 0, w: 1000, h: 800, snapThreshold: 12, snapPriority: 3 },
    ];
    const result = computeSnapAdjustment(moving, targets, 5);
    expect(result.dx).toBe(-10);
  });

  it("snaps layer 11px from canvas edge (within 12px threshold)", () => {
    const moving = { x: 11, y: 100, w: 50, h: 50 };
    const targets: SnapRect[] = [
      { x: 0, y: 0, w: 1000, h: 800, snapThreshold: 12, snapPriority: 3 },
    ];
    const result = computeSnapAdjustment(moving, targets, 5);
    expect(result.dx).toBe(-11);
  });

  it("does NOT snap layer 13px from canvas edge (outside 12px threshold)", () => {
    const moving = { x: 13, y: 100, w: 50, h: 50 };
    const targets: SnapRect[] = [
      { x: 0, y: 0, w: 1000, h: 800, snapThreshold: 12, snapPriority: 3 },
    ];
    const result = computeSnapAdjustment(moving, targets, 5);
    expect(result.dx).toBe(0);
  });

  it("canvas edge wins over layer edge when both are candidates", () => {
    const moving = { x: 7, y: 100, w: 50, h: 50 };
    const targets: SnapRect[] = [
      { x: 0, y: 0, w: 1000, h: 800, snapThreshold: 12, snapPriority: 3 },
      { x: -48, y: 100, w: 50, h: 50 },
    ];
    const result = computeSnapAdjustment(moving, targets, 5);
    expect(result.dx).toBe(-7);
  });

  it("canvas center snap works within its 6px threshold", () => {
    const moving = { x: 100, y: 0, w: 50, h: 50 };
    const result = computeSnapAdjustment(moving, [
      { x: 120, y: -Infinity, w: 0, h: Infinity, snapThreshold: 6, snapPriority: 2 },
    ], 5);
    expect(result.dx).toBe(-5);
  });

  it("canvas center does NOT snap beyond its 6px threshold", () => {
    const moving = { x: 100, y: 0, w: 50, h: 50 };
    const result = computeSnapAdjustment(moving, [
      { x: 132, y: -Infinity, w: 0, h: Infinity, snapThreshold: 6, snapPriority: 2 },
    ], 5);
    expect(result.dx).toBe(0);
  });

  it("backward compat: bare SnapRect uses default threshold and priority", () => {
    const moving = { x: 97, y: 100, w: 50, h: 50 };
    const targets: SnapRect[] = [
      { x: 100, y: 200, w: 50, h: 50 },
    ];
    const result = computeSnapAdjustment(moving, targets, 5);
    expect(result.dx).toBe(3);
    expect(result.lines.length).toBe(1);
  });

  it("snaps moving layer left edge to target right edge (abutting / zero-gap docking)", () => {
    // Target 1: x=0, w=100 -> right edge is 100
    // Moving: x=102, w=50 -> distance 2 to 100 -> snaps dx=-2
    // Both layers also align on Y (y=50, top-top align)
    const moving = { x: 102, y: 50, w: 50, h: 50, kind: "layer" as const };
    const targets: SnapRect[] = [
      { x: 0, y: 50, w: 100, h: 50, kind: "layer" },
    ];
    const result = computeSnapAdjustment(moving, targets, 8);
    expect(result.dx).toBe(-2);
    expect(result.dy).toBe(0);
    expect(result.lines.length).toBe(2); // 1 X docking line + 1 Y top-align line
    expect(result.lines[0].kind).toBe("layer");
    expect(result.lines[0].color).toBe("var(--guide-layer, #E03183)");
  });

  it("equal spacing (gap snapping): snaps moving layer when placed after two layers with matching gap", () => {
    // Layer A: x=0, w=100 (right=100)
    // Layer B: x=120, w=100 (left=120, right=220) -> gap is 20px
    // Moving Layer M: w=100, placed near x=240+2 = 242 (ideal x is 220 + 20 = 240)
    const moving = { x: 242, y: 50, w: 100, h: 50, kind: "layer" as const };
    const targets: SnapRect[] = [
      { x: 0, y: 50, w: 100, h: 50, kind: "layer" },
      { x: 120, y: 50, w: 100, h: 50, kind: "layer" },
    ];
    const result = computeSnapAdjustment(moving, targets, 8);
    expect(result.dx).toBe(-2); // 242 - 2 = 240
    const gapLines = result.lines.filter((l) => l.kind === "gap");
    expect(gapLines.length).toBe(2);
    expect(gapLines[0].color).toBe("var(--guide-gap, #F59E0B)");
    expect(gapLines[1].color).toBe("var(--guide-gap, #F59E0B)");
  });

  it("equal spacing (gap snapping): snaps vertical stack when placed below two layers with matching vertical gap", () => {
    // Layer A: y=0, h=100 (bottom=100)
    // Layer B: y=130, h=100 (top=130, bottom=230) -> gap is 30px
    // Moving Layer M: h=100, placed near y=263 (ideal y is 230 + 30 = 260)
    const moving = { x: 50, y: 263, w: 50, h: 100, kind: "layer" as const };
    const targets: SnapRect[] = [
      { x: 50, y: 0, w: 50, h: 100, kind: "layer" },
      { x: 50, y: 130, w: 50, h: 100, kind: "layer" },
    ];
    const result = computeSnapAdjustment(moving, targets, 8);
    expect(result.dy).toBe(-3); // 263 - 3 = 260
    const gapLines = result.lines.filter((l) => l.kind === "gap");
    expect(gapLines.length).toBe(2);
    expect(gapLines[0].color).toBe("var(--guide-gap, #F59E0B)");
  });

  it("closest-target priority: closest layer target (2px away) wins over faraway canvas edge (8px away)", () => {
    // Canvas: x=0, w=1000, snapPriority: 3, snapThreshold: 12
    // Layer Target: x=200, w=100 (right edge 300), snapPriority: 1, snapThreshold: 8
    // Moving: x=302, w=50 -> distance to Layer is 2px (left edge 302 -> 300), distance to Canvas is 302px (not candidate), or if Moving.x is near canvas edge:
    // Moving at x=8 (distance 8px to Canvas left edge 0, but distance 2px to a Layer at x=10)
    const moving = { x: 8, y: 100, w: 50, h: 50, kind: "layer" as const };
    const targets: SnapRect[] = [
      { x: 0, y: 0, w: 1000, h: 800, snapThreshold: 12, snapPriority: 3, kind: "canvas" },
      { x: 10, y: 100, w: 50, h: 50, snapThreshold: 8, snapPriority: 1, kind: "layer" },
    ];
    const result = computeSnapAdjustment(moving, targets, 8);
    // Distance to Layer Target left edge (10) is 2px (dx = +2). Distance to Canvas left edge (0) is 8px (dx = -8).
    // Physically closest (Layer, 2px) wins!
    expect(result.dx).toBe(2);
    expect(result.lines[0].kind).toBe("layer");
  });

  it("equal spacing (middle placement): snaps moving layer centered between two layers with equidistant gaps", () => {
    // Layer A: x=0, w=100 (right=100)
    // Layer B: x=240, w=100 (left=240)
    // Total gap = 140px. Moving layer M has w=60.
    // Remaining space = 140 - 60 = 80px -> equidistant gap is 40px each side.
    // Ideal M.x = 100 + 40 = 140px.
    // Moving placed at x=142 (2px off).
    const moving = { x: 142, y: 50, w: 60, h: 50, kind: "layer" as const };
    const targets: SnapRect[] = [
      { x: 0, y: 50, w: 100, h: 50, kind: "layer" },
      { x: 240, y: 50, w: 100, h: 50, kind: "layer" },
    ];
    const result = computeSnapAdjustment(moving, targets, 8);
    expect(result.dx).toBe(-2); // 142 - 2 = 140
    const gapLines = result.lines.filter((l) => l.kind === "gap");
    expect(gapLines.length).toBe(2);
    expect(gapLines[0].label).toBe("40px");
    expect(gapLines[1].label).toBe("40px");
  });
});

