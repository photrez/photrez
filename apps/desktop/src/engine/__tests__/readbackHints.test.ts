// SPDX-License-Identifier: AGPL-3.0-or-later
// Readback-hint pins: 2D contexts on pixel readback paths must ask for
// frequent reads. Counts/args only, no timing.
import { describe, it, expect, vi, afterEach } from "vitest";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { bakeAdjustmentToBitmap, bakeAdjustmentToBitmapGpu } from "@/engine/layerAdjustments";
import { DocumentEngine } from "@/engine/document";

const seen: unknown[][] = [];

function stubCanvas() {
  seen.length = 0;
  vi.stubGlobal(
    "OffscreenCanvas",
    class {
      width: number;
      height: number;
      constructor(w: number, h: number) {
        this.width = w;
        this.height = h;
      }
      getContext(...args: unknown[]) {
        seen.push(args);
        const w = this.width;
        const h = this.height;
        return {
          drawImage: vi.fn(),
          getImageData: vi.fn(() => ({ data: new Uint8ClampedArray(w * h * 4), width: w, height: h })),
          putImageData: vi.fn(),
          clearRect: vi.fn(),
        };
      }
      transferToImageBitmap() {
        return { width: this.width, height: this.height, close: vi.fn() };
      }
    },
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

const IDENTITY = { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false };

// Defeat: drop the second getContext("2d", ...) argument in any covered path (for example SelectionOperations.ts:123); args[1] becomes undefined and toEqual goes RED.
describe("readback 2d contexts carry willReadFrequently", () => {
  it("copySelection scratch reads with the hint", () => {
    stubCanvas();
    const sel = { x: 0, y: 0, width: 8, height: 8, angle: 0, shape: "rect", inverted: false };
    const engine = {
      getSelection: () => sel,
      getActiveLayerId: () => "L",
      getLayerImageBitmap: () => ({ width: 8, height: 8 }),
      getLayer: () => ({ id: "L", width: 8, height: 8, transform: IDENTITY, imageBitmap: {} }),
    } as never;
    SelectionOperations.copySelection(engine);
    expect(seen.length).toBeGreaterThan(0);
    for (const args of seen) expect(args[1]).toEqual({ willReadFrequently: true });
  });

  it("adjustment bake scratch reads with the hint", () => {
    stubCanvas();
    bakeAdjustmentToBitmap({} as ImageBitmap, 8, 8, { brightness: 10, contrast: 0, saturation: 0 });
    expect(seen.length).toBeGreaterThan(0);
    for (const args of seen) expect(args[1]).toEqual({ willReadFrequently: true });
  });

  it("GPU adjustment bake scratch reads with the hint", async () => {
    stubCanvas();
    await bakeAdjustmentToBitmapGpu({} as ImageBitmap, 2, 1, { brightness: 0, contrast: 0, saturation: 0 });
    expect(seen.length).toBeGreaterThan(0);
    for (const args of seen) expect(args[1]).toEqual({ willReadFrequently: true });
  });

  it("invert scratch reads with the hint", async () => {
    stubCanvas();
    const engine = new DocumentEngine("hint-doc", "Hint", 8, 8);
    const layer = engine.addLayer("L", 8, 8);
    engine.setLayerImageBitmap(layer.id, { width: 8, height: 8 } as unknown as ImageBitmap);
    await engine.invertLayerPixels(layer.id);
    expect(seen.length).toBeGreaterThan(0);
    for (const args of seen) expect(args[1]).toEqual({ willReadFrequently: true });
  });
});
