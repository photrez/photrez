import { describe, it, expect, vi, beforeEach } from "vitest";
import type { LayerNode } from "../../engine/types";
import type { LayerInfo } from "../layerHitTest";

function makeLayerInfo(over: Partial<LayerInfo> = {}): LayerInfo {
  return {
    id: "l1",
    // Required field — no cast so the compiler verifies every LayerInfo here
    // carries its hit-test class (box-hittable wiring can't be dropped).
    type: "raster",
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    width: 100,
    height: 100,
    visible: true,
    locked: false,
    ...over,
  };
}

function makeLayerNode(over: Partial<LayerNode> = {}): LayerNode {
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

describe("sampleSingleLayerAlpha", () => {
  beforeEach(() => vi.unstubAllGlobals());

  it("returns opaque alpha (1) for a visible layer with alpha=255 pixel", async () => {
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
    const { sampleSingleLayerAlpha } = await import("../../engine/pixelSample");
    const layer = makeLayerNode({ id: "top", imageBitmap: {} as ImageBitmap });
    expect(sampleSingleLayerAlpha([layer], 10, 10, "top")).toBe(1);
  });

  it("returns 0 for a transparent (alpha=0) pixel", async () => {
    vi.resetModules();
    const mockCtx = {
      clearRect: vi.fn(),
      drawImage: vi.fn(),
      getImageData: vi.fn(() => ({ data: new Uint8ClampedArray([0, 0, 0, 0]) })),
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
    const { sampleSingleLayerAlpha } = await import("../../engine/pixelSample");
    const layer = makeLayerNode({ id: "top", imageBitmap: {} as ImageBitmap });
    expect(sampleSingleLayerAlpha([layer], 10, 10, "top")).toBe(0);
  });

  it("returns 0 when the point is outside the layer bitmap bounds", async () => {
    vi.stubGlobal("OffscreenCanvas", undefined);
    vi.resetModules();
    const { sampleSingleLayerAlpha } = await import("../../engine/pixelSample");
    const layer = makeLayerNode({ id: "top", imageBitmap: {} as ImageBitmap });
    // (150,150) is outside the 100x100 layer at transform 0,0
    expect(sampleSingleLayerAlpha([layer], 150, 150, "top")).toBe(0);
  });

  it("returns 0 for an invisible or bitmap-less layer", async () => {
    vi.stubGlobal("OffscreenCanvas", undefined);
    vi.resetModules();
    const { sampleSingleLayerAlpha } = await import("../../engine/pixelSample");
    const hidden = makeLayerNode({ id: "h", visible: false, imageBitmap: {} as ImageBitmap });
    const empty = makeLayerNode({ id: "e", imageBitmap: null });
    expect(sampleSingleLayerAlpha([hidden], 10, 10, "h")).toBe(0);
    expect(sampleSingleLayerAlpha([empty], 10, 10, "e")).toBe(0);
  });
});

describe("hitTestLayers alpha-aware", () => {
  const top: LayerInfo = makeLayerInfo({ id: "top", transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false }, width: 100, height: 100 });
  const bottom: LayerInfo = makeLayerInfo({ id: "bottom", transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false }, width: 100, height: 100 });

  it("backward-compatible: bounding box only when alphaAt omitted", async () => {
    const { hitTestLayers } = await import("../layerHitTest");
    // Both overlap; without alpha sampling the topmost wins.
    const hit = hitTestLayers({ x: 50, y: 50 }, [top, bottom]);
    expect(hit?.id).toBe("top");
  });

  it("falls through transparent top layer to the layer underneath", async () => {
    const { hitTestLayers } = await import("../layerHitTest");
    // top is transparent at this point, bottom is opaque.
    const alphaAt = (id: string) => (id === "top" ? 0 : 1);
    const hit = hitTestLayers({ x: 50, y: 50 }, [top, bottom], (id, _x, _y) => alphaAt(id));
    expect(hit?.id).toBe("bottom");
  });

  it("selects the top layer when its pixel is opaque", async () => {
    const { hitTestLayers } = await import("../layerHitTest");
    const alphaAt = (id: string) => (id === "top" ? 1 : 1);
    const hit = hitTestLayers({ x: 50, y: 50 }, [top, bottom], (id, _x, _y) => alphaAt(id));
    expect(hit?.id).toBe("top");
  });

  it("treats sub-threshold alpha (e.g. 0.05) as a miss and falls through", async () => {
    const { hitTestLayers } = await import("../layerHitTest");
    const alphaAt = (id: string) => (id === "top" ? 0.05 : 1);
    const hit = hitTestLayers({ x: 50, y: 50 }, [top, bottom], (id, _x, _y) => alphaAt(id));
    expect(hit?.id).toBe("bottom");
  });

  it("returns null when no layer is hit at all", async () => {
    const { hitTestLayers } = await import("../layerHitTest");
    const hit = hitTestLayers({ x: 500, y: 500 }, [top, bottom], () => 1);
    expect(hit).toBeNull();
  });
});
