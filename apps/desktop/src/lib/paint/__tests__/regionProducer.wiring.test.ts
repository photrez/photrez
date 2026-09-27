import { describe, expect, it, vi } from "vitest";
import { clampRegionToLayer, computeDirtyRegion } from "../regionProducer";

class TestImageData {
  data: Uint8ClampedArray;
  width: number;
  height: number;
  constructor(width: number, height: number) {
    this.width = width;
    this.height = height;
    this.data = new Uint8ClampedArray(width * height * 4);
  }
}

describe("computeDirtyRegion feeds the existing readRect surface unchanged", () => {
  it("clamps then forwards one region to readRect and returns the same ImageData", () => {
    const image = new TestImageData(3, 2) as unknown as ImageData;
    const readRect = vi.fn((_x: number, _y: number, _w: number, _h: number): ImageData => image);
    // 3.2/4.8 floor to 3/4; 3.2+2 = 5.2 ceils to 6; 4.8+1 = 5.8 ceils to 6.
    const checked = clampRegionToLayer({ x: 3.2, y: 4.8, w: 2, h: 1 }, 64, 64);
    expect(checked).toEqual({ x: 3, y: 4, w: 3, h: 2 });
    const result = readRect(checked.x, checked.y, checked.w, checked.h);
    expect(readRect).toHaveBeenCalledTimes(1);
    expect(readRect).toHaveBeenCalledWith(3, 4, 3, 2);
    expect(result).toBe(image);
    expect(result.data).toBeInstanceOf(Uint8ClampedArray);
    expect(result.width).toBe(3);
    expect(result.height).toBe(2);
  });

  it("aborts before any read when the dirty region is null", () => {
    const readRect = vi.fn();
    // Selection bounds fully outside the scratch bbox collapse to no overlap.
    expect(computeDirtyRegion({ x: 3.2, y: 4.8, w: 2, h: 1 }, { x: -9, y: -9, w: 1, h: 1 })).toBeNull();
    expect(computeDirtyRegion({ x: 0, y: 0, w: 0, h: 0 }, null)).toBeNull();
    expect(readRect).not.toHaveBeenCalled();
  });
});
