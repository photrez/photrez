import { describe, it, expect, vi, afterEach } from "vitest";
import type { LayerNode } from "../types";
import { drawLayerToContext, compositeAllLayers } from "../layerComposite";

function makeLayer(over: Partial<LayerNode> = {}): LayerNode {
  return {
    id: "l1",
    name: "Layer",
    type: "raster",
    visible: true,
    opacity: 1,
    locked: false,
    blendMode: "normal",
    width: 100,
    height: 50,
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    imageBitmap: {} as ImageBitmap,
    ...over,
  } as LayerNode;
}

function makeCtx(drawImage = vi.fn()) {
  return {
    save: vi.fn(),
    restore: vi.fn(),
    translate: vi.fn(),
    rotate: vi.fn(),
    scale: vi.fn(),
    drawImage,
  } as unknown as OffscreenCanvasRenderingContext2D;
}

describe("drawLayerToContext — doc-space destination size (WYSIWYG text parity)", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("draws a 2x text bitmap at the layer's DOC width/height, not native size", () => {
    const drawImage = vi.fn();
    const ctx = makeCtx(drawImage);
    // Text bitmaps rasterize at RASTER_SCALE (2x) — native 200x100, doc box 100x50.
    const bmp = { width: 200, height: 100 } as ImageBitmap;
    const layer = makeLayer({ width: 100, height: 50, imageBitmap: bmp });

    drawLayerToContext(ctx, layer);

    expect(drawImage).toHaveBeenCalledWith(bmp, -50, -25, 100, 50);
  });

  it("1x raster bitmaps (native == doc) keep the same drawImage geometry", () => {
    const drawImage = vi.fn();
    const ctx = makeCtx(drawImage);
    const bmp = { width: 100, height: 50 } as ImageBitmap;
    const layer = makeLayer({ width: 100, height: 50, imageBitmap: bmp });

    drawLayerToContext(ctx, layer);

    expect(drawImage).toHaveBeenCalledWith(bmp, -50, -25, 100, 50);
  });
});

describe("compositeAllLayers — doc-size canvas via drawLayerToContext", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("composites a 2x text layer onto the doc-size canvas at DOC dimensions", () => {
    const drawImage = vi.fn();
    const ctx = makeCtx(drawImage);
    vi.stubGlobal(
      "OffscreenCanvas",
      class {
        width = 0;
        height = 0;
        constructor(w: number, h: number) {
          this.width = w;
          this.height = h;
        }
        getContext() {
          return ctx;
        }
        transferToImageBitmap() {
          return {} as ImageBitmap;
        }
      },
    );

    const bmp = { width: 200, height: 100 } as ImageBitmap;
    const layer = makeLayer({ width: 100, height: 50, imageBitmap: bmp });
    const result = compositeAllLayers([layer], 100, 50);

    expect(result).not.toBeNull();
    expect(drawImage).toHaveBeenCalledWith(bmp, -50, -25, 100, 50);
  });
});
