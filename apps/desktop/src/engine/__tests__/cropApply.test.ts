import { describe, it, expect, vi, afterEach } from "vitest";
import type { LayerNode } from "../types";
import { performApplyCrop } from "../cropApply";

function makeLayer(over: Partial<LayerNode> = {}): LayerNode {
  return {
    id: "l1",
    name: "Layer",
    type: "text",
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

describe("performApplyCrop deleteCropped — text bitmap stays doc-space", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("draws a 2x text bitmap onto the cropped canvas at DOC width/height", () => {
    const ctx = {
      clearRect: vi.fn(),
      save: vi.fn(),
      translate: vi.fn(),
      rotate: vi.fn(),
      scale: vi.fn(),
      drawImage: vi.fn(),
      restore: vi.fn(),
    };
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

    const bmp = { width: 200, height: 100 } as ImageBitmap; // RASTER_SCALE 2x text
    const layer = makeLayer({ width: 100, height: 50, imageBitmap: bmp });
    const layers = [layer];

    performApplyCrop(layers, 0, 0, 100, 50, { deleteCroppedPixels: true });

    // Native-size drawImage(bmp, -50, -25) would render text double-size.
    expect(ctx.drawImage).toHaveBeenCalledWith(bmp, -50, -25, 100, 50);
  });
});
