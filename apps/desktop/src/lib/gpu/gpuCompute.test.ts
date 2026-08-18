import { describe, it, expect } from "vitest";
import { invertRgba, invertRgbaCpu, isGpuComputeAvailable, adjustRgba, adjustRgbaCpu } from "./gpuCompute";
import { applyBasicAdjustmentToPixelsTs, type BasicAdjustment } from "@/engine/layerAdjustments";

describe("gpuCompute invert (fallback path)", () => {
  it("isGpuComputeAvailable is false under jsdom (no navigator.gpu)", () => {
    expect(isGpuComputeAvailable()).toBe(false);
  });

  it("invertRgbaCpu inverts RGB and preserves alpha", () => {
    const px = new Uint8Array([10, 20, 30, 255, 0, 0, 0, 0]);
    expect(Array.from(invertRgbaCpu(px))).toEqual([245, 235, 225, 255, 255, 255, 255, 0]);
  });

  it("invertRgba falls back to CPU when GPU is absent and matches CPU result", async () => {
    const px = new Uint8Array([10, 20, 30, 255]);
    const res = await invertRgba(px);
    expect(res.usedGpu).toBe(false);
    expect(Array.from(res.data)).toEqual([245, 235, 225, 255]);
  });

  it("does not mutate the caller's input buffer", async () => {
    const px = new Uint8Array([10, 20, 30, 255]);
    const before = Array.from(px);
    await invertRgba(px);
    expect(Array.from(px)).toEqual(before);
  });
});

describe("gpuCompute adjust (B/C/S fallback path)", () => {
  const adj: BasicAdjustment = { brightness: 20, contrast: -40, saturation: 60 };

  it("adjustRgbaCpu matches the pure-TS reference bit-exact", () => {
    const px = new Uint8ClampedArray([100, 150, 200, 255, 0, 0, 0, 0, 255, 128, 64, 128]);
    const ref = applyBasicAdjustmentToPixelsTs(px, adj);
    expect(Array.from(adjustRgbaCpu(px, adj))).toEqual(Array.from(ref));
  });

  it("adjustRgba falls back to CPU when GPU is absent and matches the reference", async () => {
    const px = new Uint8ClampedArray([100, 150, 200, 255, 0, 0, 0, 0]);
    const res = await adjustRgba(px, adj);
    expect(res.usedGpu).toBe(false);
    const ref = applyBasicAdjustmentToPixelsTs(px, adj);
    expect(Array.from(res.data)).toEqual(Array.from(ref));
  });

  it("does not mutate the caller's input buffer", async () => {
    const px = new Uint8ClampedArray([100, 150, 200, 255]);
    const before = Array.from(px);
    await adjustRgba(px, adj);
    expect(Array.from(px)).toEqual(before);
  });
});
