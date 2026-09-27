import { describe, expect, it } from "vitest";
import { clampRegionToLayer, computeDirtyRegion } from "./regionProducer";

describe("regionProducer unit", () => {
  it("empty bbox returns null", () => expect(computeDirtyRegion(null, null)).toBeNull());
  it("zero width returns null", () => expect(computeDirtyRegion({ x: 0, y: 0, w: 0, h: 10 }, null)).toBeNull());
  it("negative height returns null", () => expect(computeDirtyRegion({ x: 0, y: 0, w: 10, h: -1 }, null)).toBeNull());
  it("NaN returns null", () => expect(computeDirtyRegion({ x: NaN, y: 0, w: 5, h: 5 }, null)).toBeNull());
  it("intersects non-inverted selection bounds", () => expect(
    computeDirtyRegion({ x: 4.2, y: 6.8, w: 20, h: 12 }, { x: 10, y: 8, w: 8, h: 6 }),
  ).toEqual({ x: 10, y: 8, w: 8, h: 6 }));
  it("rejects invalid selection bounds", () => expect(
    computeDirtyRegion({ x: 0, y: 0, w: 5, h: 5 }, { x: 0, y: 0, w: Infinity, h: 5 }),
  ).toBeNull());
  it("disjoint selection bounds return null", () => expect(
    computeDirtyRegion({ x: 0, y: 0, w: 5, h: 5 }, { x: 10, y: 10, w: 5, h: 5 }),
  ).toBeNull());
  it("rejects computed output when x+w or y+h overflows", () => {
    expect(computeDirtyRegion({ x: Number.MAX_VALUE, y: 0, w: Number.MAX_VALUE, h: 5 }, null)).toBeNull();
    expect(computeDirtyRegion({ x: 0, y: Number.MAX_VALUE, w: 5, h: Number.MAX_VALUE }, null)).toBeNull();
  });
  it("clamps oversize to layer", () => expect(
    clampRegionToLayer({ x: -4, y: -4, w: 600, h: 600 }, 512, 512),
  ).toEqual({ x: 0, y: 0, w: 512, h: 512 }));
  it("rejects zero layer dimensions", () => expect(() => clampRegionToLayer({ x: 0, y: 0, w: 1, h: 1 }, 0, 512)).toThrow(RangeError));
  it("rejects zero or negative region dimensions", () => {
    expect(() => clampRegionToLayer({ x: 0, y: 0, w: 0, h: 1 }, 512, 512)).toThrow(RangeError);
    expect(() => clampRegionToLayer({ x: 0, y: 0, w: 1, h: -1 }, 512, 512)).toThrow(RangeError);
    expect(() => clampRegionToLayer({ x: 600, y: 600, w: 1, h: 1 }, 512, 512)).toThrow(RangeError);
  });
  it("rejects an Infinity-overflow width", () => expect(() => clampRegionToLayer({ x: 0, y: 0, w: Infinity, h: 8 }, 512, 512)).toThrow(RangeError));
  it("rejects an Infinity-overflow x plus width sum", () => expect(() => clampRegionToLayer({ x: Number.MAX_VALUE, y: 0, w: Number.MAX_VALUE, h: 8 }, 512, 512)).toThrow(RangeError));
  it("rejects NaN layer height", () => expect(() => clampRegionToLayer({ x: 0, y: 0, w: 1, h: 1 }, 512, NaN)).toThrow(RangeError));
  it("rejects an Infinity layer dimension instead of clamping to a bogus rect", () => expect(() => clampRegionToLayer({ x: 0, y: 0, w: 1, h: 1 }, Infinity, 512)).toThrow(RangeError));
});
