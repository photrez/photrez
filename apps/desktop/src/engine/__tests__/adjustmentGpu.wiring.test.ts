import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { bakeAdjustmentToBitmapGpu } from "@/engine/layerAdjustments";
import * as gpuCompute from "@/lib/gpu/gpuCompute";
import type { BasicAdjustment } from "@/engine/layerAdjustments";

// Mirror the other GPU wiring tests: stub OffscreenCanvas so the engine pixel
// pass runs in jsdom. getImageData returns a seeded buffer; putImageData
// captures the final ImageData so we can assert the adjusted pixels were written.
let currentSourcePixels: Uint8ClampedArray | null = null;
let lastPutImageData: { data: Uint8ClampedArray } | null = null;

function setupOffscreenCanvasMock() {
  const MockConstructor = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    this.getContext = vi.fn(() => ({
      drawImage: vi.fn(),
      putImageData: vi.fn((img: any) => {
        lastPutImageData = img;
      }),
      getImageData: vi.fn(() => ({
        data: currentSourcePixels ?? new Uint8ClampedArray(w * h * 4),
      })),
    }));
    this.transferToImageBitmap = vi.fn(
      () =>
        ({ width: this.width, height: this.height, close: vi.fn() } as unknown as ImageBitmap),
    );
  };
  vi.stubGlobal("OffscreenCanvas", MockConstructor as unknown as typeof OffscreenCanvas);
}

describe("bakeAdjustmentToBitmapGpu WGSL wiring", () => {
  beforeEach(() => setupOffscreenCanvasMock());
  afterEach(() => {
    currentSourcePixels = null;
    lastPutImageData = null;
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("routes layer pixels through adjustRgba (GPU module) and bakes the adjusted result", async () => {
    const adj: BasicAdjustment = { brightness: 20, contrast: -40, saturation: 60 };
    currentSourcePixels = new Uint8ClampedArray([100, 150, 200, 255, 0, 0, 0, 0]);
    const adjusted = new Uint8Array([180, 90, 210, 255, 0, 0, 0, 0]);

    // Spy on the GPU module to prove the bake actually calls it.
    const adjustSpy = vi.spyOn(gpuCompute, "adjustRgba").mockImplementation(async (data) => {
      expect(Array.from(data.slice(0, 4))).toEqual([100, 150, 200, 255]);
      return { data: adjusted, usedGpu: true };
    });

    const bitmap = await bakeAdjustmentToBitmapGpu({} as ImageBitmap, 2, 1, adj);

    expect(adjustSpy).toHaveBeenCalledTimes(1); // PROVES the wiring to the GPU module
    expect(Array.from(lastPutImageData!.data)).toEqual(Array.from(adjusted)); // written back
    expect(bitmap).toBeDefined();
  });

  it("produces a bitmap via the real CPU fallback when no GPU spy is installed", async () => {
    const adj: BasicAdjustment = { brightness: 0, contrast: 0, saturation: 0 };
    currentSourcePixels = new Uint8ClampedArray([10, 20, 30, 255]);
    const bitmap = await bakeAdjustmentToBitmapGpu({} as ImageBitmap, 1, 1, adj);
    expect(bitmap).toBeDefined();
  });
});
