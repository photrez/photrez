// Unit tests for mergeTilesToRects (Fase 1.5): mergeSparseRects pattern.
// Horizontal runs first, then vertical merge of identical-x-extent adjacent
// rows. Output must be y-sorted and every input tile must land in exactly one
// output rect with its original pixel geometry preserved.
import { describe, it, expect } from "vitest";
import { mergeTilesToRects, type TileRect } from "@/lib/paint/paintTileSurface";

const T = 256;
function tile(tx: number, ty: number): TileRect {
  return {
    tx,
    ty,
    key: `${tx},${ty}`,
    x: tx * T,
    y: ty * T,
    w: T,
    h: T,
  };
}
function grid(coords: Array<[number, number]>): TileRect[] {
  return coords.map(([tx, ty]) => tile(tx, ty));
}

describe("mergeTilesToRects", () => {
  it("empty input -> empty output", () => {
    expect(mergeTilesToRects([])).toEqual([]);
  });

  it("single tile stays a single rect", () => {
    const out = mergeTilesToRects(grid([[3, 2]]));
    expect(out).toHaveLength(1);
    expect(out[0]).toMatchObject({ x: 3 * T, y: 2 * T, w: T, h: T });
    expect(out[0].tiles).toHaveLength(1);
  });

  it("horizontal run merges into one wide rect", () => {
    const out = mergeTilesToRects(grid([[0, 0], [1, 0], [2, 0]]));
    expect(out).toHaveLength(1);
    expect(out[0]).toMatchObject({ x: 0, y: 0, w: 3 * T, h: T });
    expect(out[0].tiles).toHaveLength(3);
  });

  it("vertical column merges into one tall rect (same x-extent)", () => {
    const out = mergeTilesToRects(grid([[5, 0], [5, 1], [5, 2]]));
    expect(out).toHaveLength(1);
    expect(out[0]).toMatchObject({ x: 5 * T, y: 0, w: T, h: 3 * T });
  });

  it("full rectangular block collapses to ONE rect", () => {
    const coords: Array<[number, number]> = [];
    for (let ty = 0; ty < 4; ty++) for (let tx = 0; tx < 7; tx++) coords.push([tx, ty]);
    const out = mergeTilesToRects(grid(coords));
    expect(out).toHaveLength(1);
    expect(out[0]).toMatchObject({ x: 0, y: 0, w: 7 * T, h: 4 * T });
    expect(out[0].tiles).toHaveLength(28);
  });

  it("L-shape produces two rects (vertical merge only for equal x-extent)", () => {
    // Row 0: tiles 0,1,2 ; Row 1: tile 0
    const out = mergeTilesToRects(grid([[0, 0], [1, 0], [2, 0], [0, 1]]));
    expect(out).toHaveLength(2);
    // y-sorted: row 0 first
    expect(out[0].y).toBe(0);
    expect(out[0].w).toBe(3 * T);
    expect(out[1]).toMatchObject({ x: 0, y: T, w: T, h: T });
  });

  it("checkerboard keeps tiles separate", () => {
    const out = mergeTilesToRects(grid([[0, 0], [2, 0], [0, 2], [2, 2]]));
    expect(out).toHaveLength(4);
  });

  it("two disjoint columns merge independently without cross-contamination", () => {
    // Column A rows 0-1 at tx=0; Column B rows 0-1 at tx=2; gap at tx=1.
    const out = mergeTilesToRects(grid([[0, 0], [0, 1], [2, 0], [2, 1]]));
    expect(out).toHaveLength(2);
    const xs = out.map((r) => r.x).sort((a, b) => a - b);
    expect(xs).toEqual([0, 2 * T]);
    expect(out.every((r) => r.h === 2 * T)).toBe(true);
  });

  it("every input tile appears in exactly one output rect (partition property)", () => {
    const coords: Array<[number, number]> = [
      [0, 0], [1, 0], [4, 0], [0, 1], [4, 1], [4, 2], [5, 2],
    ];
    const out = mergeTilesToRects(grid(coords));
    const seen = new Set<string>();
    let totalArea = 0;
    for (const r of out) {
      totalArea += r.w * r.h;
      for (const t of r.tiles) {
        expect(seen.has(t.key)).toBe(false); // no double-counting
        seen.add(t.key);
        // tile geometry consistent with its rect placement
        expect(t.x).toBeGreaterThanOrEqual(r.x);
        expect(t.y).toBeGreaterThanOrEqual(r.y);
        expect(t.x + t.w).toBeLessThanOrEqual(r.x + r.w);
        expect(t.y + t.h).toBeLessThanOrEqual(r.y + r.h);
      }
    }
    expect(seen.size).toBe(coords.length);
    // rects never overlap: total member area == sum of rect areas
    const memberArea = out.reduce((s, r) => s + r.tiles.reduce((a, t) => a + t.w * t.h, 0), 0);
    expect(totalArea).toBe(memberArea);
  });

  it("splits oversized merged rects by whole tiles (maxRectPixels cap)", () => {
    // A solid 16x16 tile block would merge to one 4096x4096 rect; cap at
    // 16 tiles (4x4 tiles worth of area) forces splitting.
    const coords: Array<[number, number]> = [];
    for (let ty = 0; ty < 16; ty++) for (let tx = 0; tx < 16; tx++) coords.push([tx, ty]);
    const out = mergeTilesToRects(grid(coords), 16 * T * T);
    expect(out.length).toBeGreaterThan(1);
    for (const r of out) {
      expect(r.w * r.h).toBeLessThanOrEqual(16 * T * T);
    }
    // Partition property holds after splitting.
    const seen = new Set<string>();
    for (const r of out) for (const t of r.tiles) {
      expect(seen.has(t.key)).toBe(false);
      seen.add(t.key);
    }
    expect(seen.size).toBe(256);
    // Union covers the same extent.
    const x0 = Math.min(...out.map((r) => r.x));
    const y0 = Math.min(...out.map((r) => r.y));
    const x1 = Math.max(...out.map((r) => r.x + r.w));
    const y1 = Math.max(...out.map((r) => r.y + r.h));
    expect([x0, y0, x1, y1]).toEqual([0, 0, 16 * T, 16 * T]);
  });

  it("dense mega-stroke regression: many-tile solid block produces multiple bounded rects", () => {
    // User repro 2026-08-23: dense sweep -> rects=1 covering nearly full
    // canvas -> 86MB compose buffer, zero yields. With the cap this must
    // never happen again.
    const coords: Array<[number, number]> = [];
    for (let ty = 0; ty < 15; ty++) for (let tx = 0; tx < 27; tx++) coords.push([tx, ty]);
    const out = mergeTilesToRects(grid(coords)); // default cap 4096*4096
    expect(out.length).toBeGreaterThanOrEqual(2);
    for (const r of out) {
      expect(r.w * r.h).toBeLessThanOrEqual(4096 * 4096);
    }
  });
});
