import { describe, it, expect } from "vitest";
import { hitTestLayers, hitTestLayer, type LayerInfo } from "../viewport/layerHitTest";

function makeLayer(overrides: Partial<LayerInfo> = {}): LayerInfo {
  return {
    id: "layer-1",
    // Default to raster (alpha-aware fall-through) — LayerInfo.type is required
    // so every layer carries its hit-test class explicitly.
    type: "raster",
    visible: true,
    locked: false,
    transform: { x: 100, y: 100, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    width: 200,
    height: 100,
    ...overrides,
  };
}

describe("hitTestLayer", () => {
  it("returns true for point inside unrotated rect", () => {
    expect(hitTestLayer({ x: 150, y: 150 }, makeLayer())).toBe(true);
  });

  it("returns false for point outside rect", () => {
    expect(hitTestLayer({ x: 50, y: 50 }, makeLayer())).toBe(false);
  });

  it("returns true for point inside rotated layer", () => {
    const layer = makeLayer({ transform: { ...makeLayer().transform, rotation: 45 } });
    const inside = hitTestLayer({ x: 200, y: 150 }, layer);
    expect(inside).toBe(true);
  });

  it("returns false for hidden layer", () => {
    expect(hitTestLayer({ x: 150, y: 150 }, makeLayer({ visible: false }))).toBe(false);
  });

  it("returns false for point in the gap of a rotated rect", () => {
    const layer = makeLayer({ transform: { ...makeLayer().transform, rotation: 45 } });
    const inside = hitTestLayer({ x: 260, y: 50 }, layer);
    expect(inside).toBe(false);
  });

  // Parametric layers (text/shape) are selected by their WHOLE box — clicks in
  // the transparent padding of a text box or the hollow interior of an outline
  // shape must select that layer, not fall through to the Background beneath
  // it (@bug 2026-08-09: "clicking text/shape selects Background").
  it("text layer is hit even where its bitmap alpha is 0 (box-hittable)", () => {
    const text = makeLayer({ type: "text" });
    // Sampler reports fully transparent at the point, yet the text box owns it.
    expect(hitTestLayer({ x: 150, y: 150 }, text, () => 0)).toBe(true);
  });

  it("shape layer is hit even where its bitmap alpha is 0 (box-hittable)", () => {
    const shape = makeLayer({ type: "shape" });
    expect(hitTestLayer({ x: 150, y: 150 }, shape, () => 0)).toBe(true);
  });

  it("raster layer with alpha 0 still falls through (alpha-aware preserved)", () => {
    const raster = makeLayer({ type: "raster" });
    expect(hitTestLayer({ x: 150, y: 150 }, raster, () => 0)).toBe(false);
  });
});

describe("hitTestLayers", () => {
  it("returns topmost matching layer", () => {
    const top = makeLayer({ id: "top", transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } });
    const bottom = makeLayer({ id: "bottom", transform: { x: 100, y: 100, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } });
    const layers = [top, bottom];
    const result = hitTestLayers({ x: 50, y: 50 }, layers);
    expect(result?.id).toBe("top");
  });

  it("skips hidden layers", () => {
    const hidden = makeLayer({ id: "hidden", visible: false, transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } });
    const visible = makeLayer({ id: "visible", transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } });
    const result = hitTestLayers({ x: 50, y: 50 }, [hidden, visible]);
    expect(result?.id).toBe("visible");
  });

  it("returns null when no layer is hit", () => {
    expect(hitTestLayers({ x: 999, y: 999 }, [makeLayer()])).toBeNull();
  });

  it("regression: text layer transparent padding wins over an opaque Background", () => {
    // Mirrors the user bug: an opened image (opaque Background) + a text layer
    // whose box has transparent padding. A click inside the text box but off
    // the glyphs must select the TEXT, never the Background.
    const bg = makeLayer({
      id: "bg",
      type: "raster",
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      width: 1000,
      height: 1000,
    });
    const text = makeLayer({ id: "text", type: "text" });
    const layers = [text, bg];
    // At the click point the text alpha is 0 (transparent padding) and the
    // Background is fully opaque — before the fix this selected the Background.
    const alphaAt = (id: string) => (id === "text" ? 0 : 1);
    expect(hitTestLayers({ x: 150, y: 150 }, layers, alphaAt)?.id).toBe("text");
  });
});
