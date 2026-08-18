import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { DocumentEngine } from "../document";
import * as gpuCompute from "@/lib/gpu/gpuCompute";

// Mirror document-bake.test.ts: stub OffscreenCanvas so the engine pixel pass
// runs in jsdom. getImageData returns a seeded buffer; putImageData captures
// the final ImageData so we can assert the inverted pixels were written back.
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

describe("invertLayerPixels GPU compute wiring", () => {
  beforeEach(() => setupOffscreenCanvasMock());
  afterEach(() => {
    currentSourcePixels = null;
    lastPutImageData = null;
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("routes layer pixels through invertRgba and bakes the inverted result into a fresh bitmap", async () => {
    const engine = new DocumentEngine("doc-1", "Test", 2, 1);
    const layer = engine.addLayer("L1");
    const initial = { width: 2, height: 1, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initial);

    currentSourcePixels = new Uint8ClampedArray([10, 20, 30, 255, 40, 50, 60, 128]);
    const inverted = new Uint8Array([245, 235, 225, 255, 215, 205, 195, 128]);

    // Spy on the GPU module to prove the engine op actually calls it.
    const invertSpy = vi
      .spyOn(gpuCompute, "invertRgba")
      .mockImplementation(async (data) => {
        expect(Array.from(data.slice(0, 4))).toEqual([10, 20, 30, 255]);
        return { data: inverted, usedGpu: false };
      });

    const result = await engine.invertLayerPixels(layer.id);

    expect(result).toBe("cpu"); // navigator.gpu absent in jsdom -> CPU fallback
    expect(invertSpy).toHaveBeenCalledTimes(1); // PROVES the wiring to the GPU module
    expect(layer.imageBitmap).not.toBe(initial); // baked into a fresh bitmap
    expect(Array.from(lastPutImageData!.data)).toEqual(Array.from(inverted)); // written back
  });

  it("does not detach the source bitmap when a snapshot was committed (undo-safe)", async () => {
    const engine = new DocumentEngine("doc-1", "Test", 2, 1);
    const layer = engine.addLayer("L1");
    const initial = { width: 2, height: 1, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initial);
    engine.snapshot(); // caller snapshots BEFORE mutation (registers the live bitmap for undo)

    currentSourcePixels = new Uint8ClampedArray([10, 20, 30, 255, 40, 50, 60, 128]);
    vi.spyOn(gpuCompute, "invertRgba").mockImplementation(async () => ({
      data: new Uint8Array([245, 235, 225, 255, 215, 205, 195, 128]),
      usedGpu: false,
    }));

    await engine.invertLayerPixels(layer.id);

    expect(initial.close).not.toHaveBeenCalled(); // snapshot retained the old raster
  });

  it("returns noop when the layer has no bitmap", async () => {
    const engine = new DocumentEngine("doc-1", "Test", 2, 1);
    const layer = engine.addLayer("L1"); // no setLayerImageBitmap -> imageBitmap null
    const result = await engine.invertLayerPixels(layer.id);
    expect(result).toBe("noop");
  });
});
