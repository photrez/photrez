import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { DocumentEngine } from "../document";
import type { RenderBackend } from "../../renderer/types";

// Stub OffscreenCanvas so the CPU bake fallback (bakeAdjustmentToBitmap) can
// run in jsdom when no GPU renderer is supplied.
function setupOffscreenCanvasMock() {
  const MockConstructor = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    this.getContext = vi.fn(() => ({
      drawImage: vi.fn(),
      putImageData: vi.fn(),
      getImageData: vi.fn(() => ({ data: new Uint8ClampedArray(w * h * 4) })),
    }));
    this.transferToImageBitmap = vi.fn(() => ({
      width: this.width,
      height: this.height,
      close: vi.fn(),
    } as unknown as ImageBitmap));
  };
  vi.stubGlobal("OffscreenCanvas", MockConstructor as unknown as typeof OffscreenCanvas);
}

describe("commitBasicAdjustment GPU bake", () => {
  beforeEach(() => setupOffscreenCanvasMock());
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("prefers the GPU bake when the renderer provides bakeLayerToBitmap", async () => {
    const engine = new DocumentEngine("doc-1", "Test", 100, 100);
    const layer = engine.addLayer("L1");
    const initial = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initial);
    engine.applyBasicAdjustment(layer.id, { brightness: 20, contrast: 0, saturation: 0 });

    const gpuBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    const renderer = {
      uploadImage: vi.fn(),
      requestRender: vi.fn(),
      bakeLayerToBitmap: vi.fn(() => gpuBitmap),
    } as unknown as RenderBackend;

    const result = await engine.commitBasicAdjustment(layer.id, renderer);

    expect(result).toBe("gpu");
    expect((renderer as any).bakeLayerToBitmap).toHaveBeenCalledTimes(1);
    expect(layer.basicAdjustment).toBeUndefined();
    expect(layer.hasAdjustments).toBe(false);
    expect(layer.imageBitmap).toBe(gpuBitmap); // GPU result used directly
    expect(layer.imageBitmap).not.toBe(initial);
  });

  it("prefers the async PBO bake when the renderer provides bakeLayerToBitmapAsync", async () => {
    const engine = new DocumentEngine("doc-1", "Test", 100, 100);
    const layer = engine.addLayer("L1");
    const initial = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initial);
    engine.applyBasicAdjustment(layer.id, { brightness: 20, contrast: 0, saturation: 0 });

    const gpuBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    const renderer = {
      uploadImage: vi.fn(),
      requestRender: vi.fn(),
      bakeLayerToBitmapAsync: vi.fn(async () => gpuBitmap),
      bakeLayerToBitmap: vi.fn(),
    } as unknown as RenderBackend;

    const result = await engine.commitBasicAdjustment(layer.id, renderer);

    expect(result).toBe("gpu");
    expect((renderer as any).bakeLayerToBitmapAsync).toHaveBeenCalledTimes(1);
    expect((renderer as any).bakeLayerToBitmap).not.toHaveBeenCalled();
    expect(layer.imageBitmap).toBe(gpuBitmap);
  });

  it("falls back to the sync GPU bake when the async bake returns null", async () => {
    const engine = new DocumentEngine("doc-1", "Test", 100, 100);
    const layer = engine.addLayer("L1");
    const initial = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initial);
    engine.applyBasicAdjustment(layer.id, { brightness: 20, contrast: 0, saturation: 0 });

    const gpuBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    const renderer = {
      uploadImage: vi.fn(),
      requestRender: vi.fn(),
      bakeLayerToBitmapAsync: vi.fn(async () => null),
      bakeLayerToBitmap: vi.fn(() => gpuBitmap),
    } as unknown as RenderBackend;

    const result = await engine.commitBasicAdjustment(layer.id, renderer);

    expect(result).toBe("gpu");
    expect((renderer as any).bakeLayerToBitmapAsync).toHaveBeenCalledTimes(1);
    expect((renderer as any).bakeLayerToBitmap).toHaveBeenCalledTimes(1);
    expect(layer.imageBitmap).toBe(gpuBitmap);
  });

  it("falls back to the CPU bake when no renderer is supplied", async () => {
    const engine = new DocumentEngine("doc-1", "Test", 100, 100);
    const layer = engine.addLayer("L1");
    const initial = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initial);
    engine.applyBasicAdjustment(layer.id, { brightness: 20, contrast: 0, saturation: 0 });

    const result = await engine.commitBasicAdjustment(layer.id); // no renderer

    expect(result).toBe("cpu");
    expect(layer.basicAdjustment).toBeUndefined();
    expect(layer.imageBitmap).not.toBe(initial); // CPU-baked into a fresh bitmap
    expect(initial.close).not.toHaveBeenCalled();
  });

  it("does not allocate when the adjustment is already zero (no-op)", async () => {
    const engine = new DocumentEngine("doc-1", "Test", 100, 100);
    const layer = engine.addLayer("L1");
    const initial = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initial);
    engine.applyBasicAdjustment(layer.id, { brightness: 0, contrast: 0, saturation: 0 });

    const renderer = {
      uploadImage: vi.fn(),
      requestRender: vi.fn(),
      bakeLayerToBitmap: vi.fn(() => ({ width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap)),
    } as unknown as RenderBackend;

    const result = await engine.commitBasicAdjustment(layer.id, renderer);

    expect(result).toBe("noop");
    expect((renderer as any).bakeLayerToBitmap).not.toHaveBeenCalled();
    expect(layer.imageBitmap).toBe(initial); // kept, no wasted bake
  });
});
