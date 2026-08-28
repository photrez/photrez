import { describe, it, expect } from "vitest";
import { applyRustTilesToSurface, isPristineOpaqueWhite } from "../rustShadow";

class FakeImageData {
  data: Uint8ClampedArray;
  width: number;
  height: number;
  constructor(d: Uint8ClampedArray, w: number, h: number) {
    this.data = new Uint8ClampedArray(d);
    this.width = w;
    this.height = h;
  }
}

describe("C3 canonical seam", () => {
  const WHITE = [255, 255, 255, 255];
  const TILE = (x: number, y: number, fill: number[]) => ({
    x, y, w: 2, h: 2,
    data: [...fill, ...fill, ...fill, ...fill],
  });

  function fakeCtx() {
    const calls: { x: number; y: number; dims: string; firstPx: number[] }[] = [];
    return {
      calls,
      putImageData(img: { width: number; height: number; data: Uint8ClampedArray }, x: number, y: number) {
        calls.push({ x, y, dims: `${img.width}x${img.height}`, firstPx: [img.data[0], img.data[1], img.data[2], img.data[3]] });
      },
    };
  }

  it("applyRustTilesToSurface: putImageData per tile with exact coords/order/dims", () => {
    const ctx = fakeCtx();
    const tiles = [TILE(256, 256, [10, 20, 30, 255]), TILE(512, 512, [1, 2, 3, 255])];
    applyRustTilesToSurface(ctx as never, tiles, FakeImageData);
    expect(ctx.calls.length).toBe(2);
    expect(ctx.calls[0]).toMatchObject({ x: 256, y: 256, dims: "2x2" });
    expect(ctx.calls[0].firstPx).toEqual([10, 20, 30, 255]);
    expect(ctx.calls[1]).toMatchObject({ x: 512, y: 512, dims: "2x2" });
    expect(ctx.calls[1].firstPx).toEqual([1, 2, 3, 255]);
  });

  it("applyRustTilesToSurface: empty tile list = no calls (safe no-op)", () => {
    const ctx = fakeCtx();
    applyRustTilesToSurface(ctx as never, []);
    expect(ctx.calls.length).toBe(0);
  });

  it("isPristineOpaqueWhite: accepts pure white opaque buffer", () => {
    expect(isPristineOpaqueWhite([...WHITE, ...WHITE, ...WHITE])).toBe(true);
  });

  it("isPristineOpaqueWhite: rejects transparent / tinted / non-opaque bytes", () => {
    expect(isPristineOpaqueWhite([...WHITE, 0, 0, 0, 255])).toBe(false);
    expect(isPristineOpaqueWhite([254, 255, 255, 255])).toBe(false);
    expect(isPristineOpaqueWhite([255, 255, 255, 254])).toBe(false);
    expect(isPristineOpaqueWhite([])).toBe(true); // vacuous
  });

  it("stability: identical inputs produce identical call sequences across runs", () => {
    const tiles = [TILE(300, 300, [9, 9, 9, 255])];
    const run = () => {
      const ctx = fakeCtx();
    applyRustTilesToSurface(ctx as never, tiles, FakeImageData);
      return JSON.stringify(ctx.calls);
    };
    expect(run()).toBe(run());
  });
});
