import { describe, it, expect, vi, beforeEach } from "vitest";
import type { LayerNode } from "../types";

function makeLayer(over: Partial<LayerNode> = {}): LayerNode {
  return {
    id: "l1",
    name: "Layer",
    visible: true,
    opacity: 1,
    width: 100,
    height: 100,
    transform: { x: 0, y: 0 },
    imageBitmap: {} as ImageBitmap,
    ...over,
  } as LayerNode;
}

describe("performPixelSampling", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("composites a single opaque layer via the reusable scratch canvas", async () => {
    const mockCtx = {
      clearRect: vi.fn(),
      drawImage: vi.fn(),
      getImageData: vi.fn(() => ({ data: new Uint8ClampedArray([10, 20, 30, 255]) })),
    };
    vi.stubGlobal(
      "OffscreenCanvas",
      class {
        width = 1;
        height = 1;
        getContext() {
          return mockCtx;
        }
      },
    );

    const { performPixelSampling } = await import("../pixelSample");
    const layer = makeLayer({ imageBitmap: {} as ImageBitmap });
    const result = performPixelSampling([layer], 100, 100, 10, 10);

    expect(result).toEqual([10, 20, 30, 1]);
    // Reused across the single sample — getContext called once, not per call.
    expect(mockCtx.clearRect).toHaveBeenCalled();
  });

  it("returns amber fallback when OffscreenCanvas is unavailable", async () => {
    vi.stubGlobal("OffscreenCanvas", undefined);

    // Fresh module so the lazy ctx reflects the missing global.
    vi.resetModules();
    const mod = await import("../pixelSample");
    const layer = makeLayer({ imageBitmap: {} as ImageBitmap });
    const result = mod.performPixelSampling([layer], 100, 100, 10, 10);

    expect(result).toEqual([225, 90, 23, 1.0]);
  });

  it("returns transparent when coordinates are out of bounds", async () => {
    vi.stubGlobal("OffscreenCanvas", undefined);
    vi.resetModules();
    const mod = await import("../pixelSample");
    const layer = makeLayer();
    expect(mod.performPixelSampling([layer], 100, 100, -1, 50)).toEqual([0, 0, 0, 0]);
  });
});

describe("scale-aware sampling for supersampled (2x) text bitmaps", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  function stubCtx() {
    const mockCtx = {
      clearRect: vi.fn(),
      drawImage: vi.fn(),
      getImageData: vi.fn(() => ({ data: new Uint8ClampedArray([10, 20, 30, 255]) })),
    };
    vi.stubGlobal(
      "OffscreenCanvas",
      class {
        width = 1;
        height = 1;
        getContext() {
          return mockCtx;
        }
      },
    );
    vi.resetModules();
    return mockCtx;
  }

  it("maps doc coords to bitmap space and down-samples the 2x source rect", async () => {
    const mockCtx = stubCtx();
    const mod = await import("../pixelSample");
    const bmp = { width: 200, height: 200 } as ImageBitmap; // 2x text bitmap
    const layer = makeLayer({ width: 100, height: 100, imageBitmap: bmp });

    const result = mod.performPixelSampling([layer], 100, 100, 10, 10);

    expect(result).toEqual([10, 20, 30, 1]);
    // doc (10,10) → bitmap (20,20); source rect = scale factor 2x2
    expect(mockCtx.drawImage).toHaveBeenCalledWith(bmp, 20, 20, 2, 2, 0, 0, 1, 1);
  });

  it("1x raster bitmaps sample a 1x1 source rect at doc coords", async () => {
    const mockCtx = stubCtx();
    const mod = await import("../pixelSample");
    const bmp = { width: 100, height: 100 } as ImageBitmap; // 1x raster
    const layer = makeLayer({ width: 100, height: 100, imageBitmap: bmp });

    mod.performPixelSampling([layer], 100, 100, 10, 10);

    expect(mockCtx.drawImage).toHaveBeenCalledWith(bmp, 10, 10, 1, 1, 0, 0, 1, 1);
  });

  it("divides by the layer scale when mapping doc→bitmap (2x bitmap, scale 2)", async () => {
    const mockCtx = stubCtx();
    const mod = await import("../pixelSample");
    const bmp = { width: 200, height: 200 } as ImageBitmap; // 2x text bitmap
    // scaleX=2: visible extent is 200 doc px, so doc (10,10) is bitmap (10,10).
    const layer = makeLayer({
      width: 100,
      height: 100,
      transform: { x: 0, y: 0, scaleX: 2, scaleY: 2 } as any,
      imageBitmap: bmp,
    });

    mod.performPixelSampling([layer], 100, 100, 10, 10);

    expect(mockCtx.drawImage).toHaveBeenCalledWith(bmp, 10, 10, 2, 2, 0, 0, 1, 1);
  });

  it("sampleSingleLayerAlpha maps to bitmap space for 2x text", async () => {
    const mockCtx = stubCtx();
    const mod = await import("../pixelSample");
    const bmp = { width: 200, height: 200 } as ImageBitmap;
    const layer = makeLayer({ width: 100, height: 100, imageBitmap: bmp });

    const alpha = mod.sampleSingleLayerAlpha([layer], 10, 10, "l1");

    expect(alpha).toBe(1);
    expect(mockCtx.drawImage).toHaveBeenCalledWith(bmp, 20, 20, 2, 2, 0, 0, 1, 1);
  });

  it("sampleSingleLayerAlpha 1x raster samples at doc coords unchanged", async () => {
    const mockCtx = stubCtx();
    const mod = await import("../pixelSample");
    const bmp = { width: 100, height: 100 } as ImageBitmap; // 1x raster
    const layer = makeLayer({ width: 100, height: 100, imageBitmap: bmp });

    const alpha = mod.sampleSingleLayerAlpha([layer], 10, 10, "l1");

    expect(alpha).toBe(1);
    expect(mockCtx.drawImage).toHaveBeenCalledWith(bmp, 10, 10, 1, 1, 0, 0, 1, 1);
  });
});
