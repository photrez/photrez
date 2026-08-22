import { describe, expect, it, vi } from "vitest";
import {
  brushAlphaAtDistance,
  clearBrushTipCache,
  createBrushTip,
  falloff,
  getBrushDabSpacing,
  getBrushTipOuterRadius,
  getBrushTipCacheKey,
  getBrushTip,
  interpolateDabs,
  stampBrushTip,
  stampTerminalBrushTip,
  compositeMaskToImageData,
  paintMaskToContext,
  paintTransientBrushTipToContext,
  paintGradientDab,
  isGradientDabEnabled,
  buildGradientStops,
  paintMaskToContextDirty,
  compositeMaskToImageDataDirty,
  emptyDirtyRect,
  expandDirtyRect,
  clampDirtyRect,
  unionDirtyRect,
  getEffectiveFlowMultiplier,
  clamp01,
  brushPointsEqual,
  parsePaintColor,
  type BrushTip,
  type DirtyRect,
} from "../brushTipMask";

describe("brushTipMask falloff", () => {
  it("uses cosine falloff by default", () => {
    expect(falloff(0, "cosine")).toBe(0);
    expect(falloff(0.5, "cosine")).toBeCloseTo(0.5, 5);
    expect(falloff(1, "cosine")).toBe(1);
  });

  it("feathers the full radius at hardness 0", () => {
    const radius = 50;
    expect(brushAlphaAtDistance(0, radius, 0, "cosine")).toBe(1);
    expect(brushAlphaAtDistance(25, radius, 0, "cosine")).toBeCloseTo(0.5, 5);
    expect(brushAlphaAtDistance(49, radius, 0, "cosine")).toBeGreaterThan(0);
    expect(brushAlphaAtDistance(50, radius, 0, "cosine")).toBe(0);
  });

  it("keeps diameter fixed while hardness sharpens the falloff", () => {
    const radius = 50;
    expect(brushAlphaAtDistance(25, radius, 0, "cosine")).toBeCloseTo(0.5, 5);
    expect(brushAlphaAtDistance(25, radius, 0.5, "cosine")).toBe(1);
    expect(brushAlphaAtDistance(37.5, radius, 0.5, "cosine")).toBeCloseTo(0.5, 5);
    expect(brushAlphaAtDistance(40, radius, 0.8, "cosine")).toBe(1);
    expect(brushAlphaAtDistance(45, radius, 0.8, "cosine")).toBeCloseTo(0.5, 5);

    for (const hardness of [0, 0.5, 0.8, 1]) {
      expect(brushAlphaAtDistance(49, radius, hardness, "cosine")).toBeGreaterThan(0);
      expect(brushAlphaAtDistance(50, radius, hardness, "cosine")).toBe(0);
      expect(brushAlphaAtDistance(51, radius, hardness, "cosine")).toBe(0);
    }
  });

  it("creates a tip with center alpha and zero outer edge", () => {
    const tip = createBrushTip({ size: 21, hardness: 0, curve: "cosine" });
    const center = tip.diameter / 2;
    const centerAlpha = tip.data[center * tip.width + center];
    const corner = tip.data[tip.data.length - 1];
    expect(tip.width).toBe(24);
    expect(tip.height).toBe(24);
    expect(centerAlpha).toBeGreaterThan(0.98);
    expect(corner).toBe(0);
  });

  it("rounds cache keys to stable values", () => {
    expect(getBrushTipCacheKey({ size: 20.2, hardness: 0.333, curve: "cosine" })).toBe("20:33:cosine");
  });
});

describe("reference-calibrated soft round raster", () => {
  it("keeps hardness-0 alpha outside the nominal radius", () => {
    const radius = 50;
    expect(brushAlphaAtDistance(radius, radius, 0, "soft")).toBeGreaterThan(0.09);
    expect(brushAlphaAtDistance(radius * 1.4, radius, 0, "soft")).toBeGreaterThan(0.01);
  });

  it("allocates soft support beyond size without changing nominal radius", () => {
    const tip = createBrushTip({ size: 100, hardness: 0, curve: "soft" });
    expect(tip.radius).toBe(50);
    expect(tip.width).toBeGreaterThan(100);
    expect(tip.height).toBe(tip.width);
  });

  it("uses a one-pixel antialiased boundary below 22 render pixels", () => {
    const tip = createBrushTip({ size: 21, hardness: 0, curve: "soft" });
    const center = tip.diameter / 2;
    const centerAlpha = tip.data[center * tip.width + center];
    const boundaryAlpha = tip.data[center * tip.width + center + 10];

    expect(centerAlpha).toBe(1);
    expect(boundaryAlpha).toBeGreaterThan(0);
    expect(boundaryAlpha).toBeLessThan(1);
  });

  it("reuses cached data until size or hardness changes", () => {
    clearBrushTipCache();
    const first = getBrushTip({ size: 100, hardness: 0.5, curve: "soft" });
    expect(getBrushTip({ size: 100, hardness: 0.5, curve: "soft" })).toBe(first);
    expect(getBrushTip({ size: 101, hardness: 0.5, curve: "soft" })).not.toBe(first);
    expect(getBrushTip({ size: 100, hardness: 0.6, curve: "soft" })).not.toBe(first);
  });
});

describe("brushTipMask dabs", () => {
  it("uses fixed 10% size spacing regardless of hardness", () => {
    expect(getBrushDabSpacing(100, 0, 1)).toBe(10);
    expect(getBrushDabSpacing(100, 1, 1)).toBe(10);
    expect(getBrushDabSpacing(100, 0.5, 1)).toBe(10);
  });

  it("never returns less than 1 even for tiny brushes", () => {
    expect(getBrushDabSpacing(1, 0, 1)).toBe(1);
    expect(getBrushDabSpacing(2, 0, 1)).toBe(1);
  });

  it("scales linearly with size (no per-harness floor)", () => {
    expect(getBrushDabSpacing(20, 0, 1)).toBe(2);
    expect(getBrushDabSpacing(40, 0, 1)).toBe(4);
    expect(getBrushDabSpacing(80, 0, 1)).toBe(8);
    expect(getBrushDabSpacing(400, 0, 1)).toBe(40);
  });

  it("does not depend on flow", () => {
    expect(getBrushDabSpacing(100, 0, 0.1)).toBe(getBrushDabSpacing(100, 0, 1));
    expect(getBrushDabSpacing(100, 1, 0.5)).toBe(getBrushDabSpacing(100, 1, 1));
  });

  it("interpolates dabs between two points", () => {
    const result = interpolateDabs({ x: 0, y: 0 }, { x: 30, y: 0 }, 10, 0);
    expect(result.dabs).toEqual([{ x: 10, y: 0 }, { x: 20, y: 0 }, { x: 30, y: 0 }]);
    expect(result.carry).toBe(0);
  });

  it("carries leftover distance to the next segment", () => {
    const result = interpolateDabs({ x: 0, y: 0 }, { x: 15, y: 0 }, 10, 0);
    expect(result.dabs).toEqual([{ x: 10, y: 0 }]);
    expect(result.carry).toBe(5);
  });

  it("stamps a non-grid terminal endpoint exactly once", () => {
    const tip = createBrushTip({ size: 3, hardness: 1, curve: "soft" });
    const mask = new Uint8ClampedArray(30 * 5);
    const start = { x: 2, y: 2 };
    const endpoint = { x: 17, y: 2 };
    stampBrushTip(mask, 30, 5, tip, start.x, start.y, 0.5);

    const spaced = interpolateDabs(start, endpoint, 10, 0);
    let lastDab = start;
    for (const dab of spaced.dabs) {
      stampBrushTip(mask, 30, 5, tip, dab.x, dab.y, 0.5);
      lastDab = dab;
    }

    expect(mask[endpoint.y * 30 + endpoint.x]).toBe(0);
    expect(stampTerminalBrushTip(mask, 30, 5, tip, endpoint, lastDab, 0.5)).toBe(true);
    const terminalAlpha = mask[endpoint.y * 30 + endpoint.x];
    expect(terminalAlpha).toBeGreaterThan(0);

    expect(stampTerminalBrushTip(mask, 30, 5, tip, endpoint, endpoint, 0.5)).toBe(false);
    expect(mask[endpoint.y * 30 + endpoint.x]).toBe(terminalAlpha);
  });

  it("composites a transient endpoint only inside its clipped tip region", () => {
    const tip = createBrushTip({ size: 20, hardness: 1, curve: "soft" });
    const getImageData = vi.fn((_x: number, _y: number, width: number, height: number) => ({
      width,
      height,
      data: new Uint8ClampedArray(width * height * 4),
      colorSpace: "srgb",
    } as ImageData));
    const putImageData = vi.fn();
    const ctx = {
      canvas: { width: 100, height: 80 },
      getImageData,
      putImageData,
    } as unknown as CanvasRenderingContext2D;

    expect(paintTransientBrushTipToContext(
      ctx,
      tip,
      { x: 92, y: 40 },
      { x: 80, y: 40 },
      1,
      "#ff0000",
      false,
    )).toBe(true);

    expect(getImageData).toHaveBeenCalledWith(81, 29, 19, 22);
    const written = putImageData.mock.calls[0][0] as ImageData;
    expect(written.data[(10 * written.width + 10) * 4 + 3]).toBeGreaterThan(0);
  });

  it("skips a transient endpoint already occupied by the last regular dab", () => {
    const tip = createBrushTip({ size: 20, hardness: 1, curve: "soft" });
    const ctx = {
      canvas: { width: 100, height: 80 },
      getImageData: vi.fn(),
      putImageData: vi.fn(),
    } as unknown as CanvasRenderingContext2D;
    const endpoint = { x: 40, y: 30 };

    expect(paintTransientBrushTipToContext(
      ctx,
      tip,
      endpoint,
      endpoint,
      0.5,
      "#ff0000",
      false,
    )).toBe(false);
    expect(ctx.getImageData).not.toHaveBeenCalled();
  });

  it("accumulates dabs toward saturation (editor-standard source-over)", () => {
    const tip = createBrushTip({ size: 3, hardness: 1, curve: "cosine" });
    const mask = new Uint8ClampedArray(9);
    stampBrushTip(mask, 3, 3, tip, 1, 1, 0.5);
    stampBrushTip(mask, 3, 3, tip, 1, 1, 0.5);
    // First stamp: 128 (center alpha scaled by 0.5).
    // Second stamp pre-multiplied accumulation: 128 + round((255-128)*128/255) = 192.
    expect(mask[4]).toBe(192);
  });

  it("saturates after enough passes at opacity 50%", () => {
    const tip = createBrushTip({ size: 3, hardness: 1, curve: "cosine" });
    const mask = new Uint8ClampedArray(9);
    for (let i = 0; i < 20; i += 1) {
      stampBrushTip(mask, 3, 3, tip, 1, 1, 0.5);
    }
    // 20 passes at alpha 0.5 → 1 - (0.5)^20 ≈ 1.0
    expect(mask[4]).toBe(255);
  });

  it("saturates with 5 passes at full opacity", () => {
    const tip = createBrushTip({ size: 3, hardness: 1, curve: "cosine" });
    const mask = new Uint8ClampedArray(9);
    stampBrushTip(mask, 3, 3, tip, 1, 1, 1.0);
    expect(mask[4]).toBe(255);
  });

  it("supports subpixel stamping with bilinear interpolation", () => {
    const tip = createBrushTip({ size: 3, hardness: 0, curve: "cosine" });

    // Stamp at integer coordinates
    const maskInteger = new Uint8ClampedArray(9);
    stampBrushTip(maskInteger, 3, 3, tip, 1, 1, 1.0);

    // Stamp at fractional coordinates
    const maskFractional = new Uint8ClampedArray(9);
    stampBrushTip(maskFractional, 3, 3, tip, 1.5, 1, 1.0);

    // The mask stamped at 1.5 should be different (interpolated)
    expect(maskFractional).not.toEqual(maskInteger);
  });

  it("incremental dabs match continuous interpolation", () => {
    const spacing = 10;
    const seg1 = interpolateDabs({ x: 0, y: 0 }, { x: 15, y: 0 }, spacing, 0);
    const seg2 = interpolateDabs({ x: 15, y: 0 }, { x: 30, y: 0 }, spacing, seg1.carry);
    const full = interpolateDabs({ x: 0, y: 0 }, { x: 30, y: 0 }, spacing, 0);
    const combinedDabs = [...seg1.dabs, ...seg2.dabs];
    expect(combinedDabs).toEqual(full.dabs);
    expect(seg2.carry).toBe(full.carry);
  });

  it("composites mask to image data for brush", () => {
    const imgData = {
      width: 2,
      height: 2,
      data: new Uint8ClampedArray(16),
      colorSpace: "srgb",
    } as ImageData;
    const mask = new Uint8ClampedArray([255, 0, 0, 128]);
    compositeMaskToImageData(imgData, mask, "rgba(255,0,0,1)", false);
    expect(imgData.data[0]).toBe(255); // Red
    expect(imgData.data[3]).toBe(255); // Opaque alpha
    expect(imgData.data[12]).toBe(255); // Red
    expect(imgData.data[15]).toBe(128); // Semi-transparent
  });

  it("composites mask to image data for eraser", () => {
    const imgData = {
      width: 2,
      height: 2,
      data: new Uint8ClampedArray([0, 0, 0, 200, 0, 0, 0, 200, 0, 0, 0, 200, 0, 0, 0, 200]),
      colorSpace: "srgb",
    } as ImageData;
    const mask = new Uint8ClampedArray([255, 0, 0, 128]);
    compositeMaskToImageData(imgData, mask, "rgba(0,0,0,1)", true);
    expect(imgData.data[3]).toBe(0); // Erased (mask 255 -> alpha = 1)
    expect(imgData.data[7]).toBe(200); // Unchanged (mask 0)
    expect(imgData.data[15]).toBe(100); // Reduced alpha (mask 128 -> alpha = 0.5 -> 200 * (1 - 0.5) = 100)
  });
});

describe("brushTipMask pixel profile", () => {
  function alphaAt(tip: BrushTip, x: number, y: number): number {
    return tip.data[y * tip.width + x];
  }

  it("rasterizes the calibrated hardness-0 curve through and beyond the nominal radius", () => {
    const size = 75;
    const radius = size / 2;
    const tip = createBrushTip({ size, hardness: 0, curve: "soft" });
    const center = tip.diameter / 2;
    const row = center;
    const atNominalRadius = alphaAt(tip, center + Math.floor(radius), row);
    const beyondRadius = alphaAt(tip, center + Math.floor(radius * 1.4), row);

    expect(tip.width).toBeGreaterThan(size);
    expect(atNominalRadius).toBeGreaterThan(0.08);
    expect(atNominalRadius).toBeLessThan(0.13);
    expect(beyondRadius).toBeGreaterThan(0);
    expect(beyondRadius).toBeLessThan(0.02);
  });

  it("uses dynamic support for calibrated tips and nominal support for hard tips", () => {
    const radius = 37.5;
    expect(getBrushTipOuterRadius(radius, 0, "soft")).toBeGreaterThan(radius * 1.6);
    expect(getBrushTipOuterRadius(radius, 0.5, "soft")).toBeGreaterThan(radius);
    expect(getBrushTipOuterRadius(radius, 1, "soft")).toBe(radius);
    expect(getBrushTipOuterRadius(radius, 0, "cosine")).toBe(radius);
  });

  it("keeps high hardness visually solid until its sharp calibrated shoulder", () => {
    const radius = 50;
    expect(brushAlphaAtDistance(radius * 0.8, radius, 0.9, "soft")).toBeGreaterThan(0.99);
    expect(brushAlphaAtDistance(radius, radius, 0.9, "soft")).toBeGreaterThan(0.4);
    expect(brushAlphaAtDistance(radius * 1.1, radius, 0.9, "soft")).toBeLessThan(0.001);
  });
});

describe("brushTipMask effective flow multiplier", () => {
  it("keeps opacity and flow independent from hardness", () => {
    expect(getEffectiveFlowMultiplier(0)).toBe(1);
    expect(getEffectiveFlowMultiplier(0.8)).toBe(1);
    expect(getEffectiveFlowMultiplier(1.0)).toBe(1);
  });

  it("uses the same alpha scale for soft and hard brushes at equal opacity/flow", () => {
    const opacity = 1.0;
    const flow = 1.0;
    const softAlphaScale = opacity * flow * getEffectiveFlowMultiplier(0);
    const hardAlphaScale = opacity * flow * getEffectiveFlowMultiplier(1.0);
    expect(hardAlphaScale).toBe(1.0);
    expect(softAlphaScale).toBe(hardAlphaScale);
  });
});

describe("clamp01", () => {
  it("clamps within 0..1 range", () => {
    expect(clamp01(-1)).toBe(0);
    expect(clamp01(0)).toBe(0);
    expect(clamp01(0.5)).toBe(0.5);
    expect(clamp01(1)).toBe(1);
    expect(clamp01(2)).toBe(1);
  });

  it("handles NaN and Infinity (returns 0 for non-finite)", () => {
    expect(clamp01(NaN)).toBe(0);
    // Implementasi: !Number.isFinite → return 0
    expect(clamp01(Infinity)).toBe(0);
    expect(clamp01(-Infinity)).toBe(0);
  });
});

describe("brushPointsEqual", () => {
  it("returns true for identical points", () => {
    expect(brushPointsEqual({ x: 10, y: 20 }, { x: 10, y: 20 })).toBe(true);
  });

  it("returns true for points within tolerance (0.0001)", () => {
    expect(brushPointsEqual({ x: 10, y: 20 }, { x: 10.00005, y: 20.00005 })).toBe(true);
  });

  it("returns false for points beyond tolerance", () => {
    expect(brushPointsEqual({ x: 10, y: 20 }, { x: 11, y: 20 })).toBe(false);
  });

  it("returns false when either point is null", () => {
    expect(brushPointsEqual(null, { x: 10, y: 20 })).toBe(false);
    expect(brushPointsEqual({ x: 10, y: 20 }, null)).toBe(false);
    expect(brushPointsEqual(null, null)).toBe(false);
  });
});

describe("parsePaintColor", () => {
  it("parses 6-digit hex", () => {
    const c = parsePaintColor("#ff8800");
    expect(c.r).toBe(255);
    expect(c.g).toBe(136);
    expect(c.b).toBe(0);
    expect(c.a).toBe(1);
  });

  it("parses 3-digit hex", () => {
    const c = parsePaintColor("#f80");
    expect(c.r).toBe(255);
    expect(c.g).toBe(136);
    expect(c.b).toBe(0);
    expect(c.a).toBe(1);
  });

  it("parses rgba() with alpha", () => {
    const c = parsePaintColor("rgba(100, 200, 50, 0.5)");
    expect(c.r).toBe(100);
    expect(c.g).toBe(200);
    expect(c.b).toBe(50);
    expect(c.a).toBeCloseTo(0.5, 6);
  });

  it("parses rgb() without alpha (defaults to 1)", () => {
    const c = parsePaintColor("rgb(10, 20, 30)");
    expect(c.r).toBe(10);
    expect(c.g).toBe(20);
    expect(c.b).toBe(30);
    expect(c.a).toBe(1);
  });

  it("returns black+opaque for unparseable strings", () => {
    const c = parsePaintColor("not-a-color");
    expect(c.r).toBe(0);
    expect(c.g).toBe(0);
    expect(c.b).toBe(0);
    expect(c.a).toBe(1);
  });

  it("does not match negative rgb values (falls through to default)", () => {
    // Regex \\d+ doesn't match '-5' → falls through to hex → fails → black
    const c = parsePaintColor("rgb(300, -5, 128)");
    expect(c.r).toBe(0);
    expect(c.g).toBe(0);
    expect(c.b).toBe(0);
  });
});

describe("interpolateDabs edge cases", () => {
  it("returns empty dabs for zero distance", () => {
    const result = interpolateDabs({ x: 10, y: 10 }, { x: 10, y: 10 }, 5, 0);
    expect(result.dabs).toEqual([]);
    expect(result.carry).toBe(0);
  });

  it("returns empty dabs when distance is smaller than spacing", () => {
    const result = interpolateDabs({ x: 0, y: 0 }, { x: 3, y: 0 }, 10, 0);
    expect(result.dabs).toEqual([]);
    expect(result.carry).toBe(3);
  });

  it("carry reduces first dab distance", () => {
    const result = interpolateDabs({ x: 0, y: 0 }, { x: 10, y: 0 }, 10, 5);
    // first dab at spacing-carry = 5, then at 15 > 10 → only one dab
    expect(result.dabs).toEqual([{ x: 5, y: 0 }]);
    expect(result.carry).toBe(5);
  });

  it("handles carry larger than spacing (wraps)", () => {
    const result = interpolateDabs({ x: 0, y: 0 }, { x: 30, y: 0 }, 10, 12);
    // next = 10-12 = -2, while -2 <= 30: dab at -2/30 = -0.067 → x = -2
    // then next = 8, 18, 28 → dabs at 8, 18, 28
    expect(result.dabs).toHaveLength(4);
    expect(result.carry).toBe(2); // 30 - (38-10) = 2
  });

  it("handles large carry (distant from previous segment)", () => {
    const result = interpolateDabs({ x: 0, y: 0 }, { x: 5, y: 5 }, 10, 8);
    // distance = 7.07, next = 2 → dab at t=2/7.07, then next=12 > 7.07
    // carry = 7.07 - (12-10) = 5.07
    expect(result.dabs).toHaveLength(1);
    expect(result.carry).toBeCloseTo(5.07, 2);
  });
});

describe("stampBrushTip edge cases", () => {
  it("clamps alphaScale to 0..1", () => {
    const tip = createBrushTip({ size: 3, hardness: 1, curve: "cosine" });
    const mask = new Uint8ClampedArray(9);
    stampBrushTip(mask, 3, 3, tip, 1, 1, 2.0);  // > 1
    expect(mask[4]).toBe(255);  // saturates (same as alphaScale=1)
  });

  it("does nothing when alphaScale is 0", () => {
    const tip = createBrushTip({ size: 3, hardness: 0, curve: "cosine" });
    const mask = new Uint8ClampedArray(9);
    stampBrushTip(mask, 3, 3, tip, 1, 1, 0);
    expect(mask[4]).toBe(0);
  });

  it("does nothing when alphaScale is negative", () => {
    const tip = createBrushTip({ size: 3, hardness: 0, curve: "cosine" });
    const mask = new Uint8ClampedArray(9);
    stampBrushTip(mask, 3, 3, tip, 1, 1, -1);
    expect(mask[4]).toBe(0);
  });

  it("handles stamp at mask edge (partial overlap)", () => {
    const tip = createBrushTip({ size: 5, hardness: 1, curve: "cosine" });
    const mask = new Uint8ClampedArray(3 * 3);
    // Center at (0, 0) — only bottom-right quadrant of tip overlaps
    stampBrushTip(mask, 3, 3, tip, 0, 0, 1.0);
    // Tip extends from center - 3 to center + 3, but mask clips to 0..2
    // At least some pixels in mask should be painted
    const painted = Array.from(mask).filter(a => a > 0);
    expect(painted.length).toBeGreaterThan(0);
  });

  it("handles stamp completely outside mask (no overlap)", () => {
    const tip = createBrushTip({ size: 3, hardness: 1, curve: "cosine" });
    const mask = new Uint8ClampedArray(3 * 3);
    stampBrushTip(mask, 3, 3, tip, -10, -10, 1.0);
    expect(Array.from(mask).every(v => v === 0)).toBe(true);
  });

  it("handles stamp with fractional center coordinates", () => {
    const tip = createBrushTip({ size: 3, hardness: 1, curve: "cosine" });
    // Integer stamp
    const maskInt = new Uint8ClampedArray(9);
    stampBrushTip(maskInt, 3, 3, tip, 1, 1, 1.0);
    // Fractional stamp
    const maskFrac = new Uint8ClampedArray(9);
    stampBrushTip(maskFrac, 3, 3, tip, 1.3, 0.7, 1.0);
    expect(maskFrac).not.toEqual(maskInt);
  });
});

describe("getBrushTipOuterRadius edge cases", () => {
  it("returns radius for cosine curve regardless of hardness", () => {
    expect(getBrushTipOuterRadius(50, 0, "cosine")).toBe(50);
    expect(getBrushTipOuterRadius(50, 1, "cosine")).toBe(50);
  });

  it("returns radius+0.5 for soft curve below reliable threshold", () => {
    // 21px diameter → 10.5 radius < 11 (MIN_RELIABLE = 22/2 = 11)
    expect(getBrushTipOuterRadius(10.5, 0, "soft")).toBe(11);
  });

  it("expands support for soft curve above threshold", () => {
    // 100px diameter → 50 radius
    const result = getBrushTipOuterRadius(50, 0, "soft");
    expect(result).toBeGreaterThan(50);
  });

  it("returns radius for hard edge soft curve", () => {
    expect(getBrushTipOuterRadius(50, 1, "soft")).toBe(50);
  });
});

describe("DirtyRect utilities", () => {
  it("emptyDirtyRect returns sentinel values", () => {
    const r = emptyDirtyRect();
    expect(r.x0).toBe(Number.MAX_SAFE_INTEGER);
    expect(r.y0).toBe(Number.MAX_SAFE_INTEGER);
    expect(r.x1).toBe(-1);
    expect(r.y1).toBe(-1);
  });

  it("expandDirtyRect expands from empty to include a point with radius", () => {
    let r = emptyDirtyRect();
    r = expandDirtyRect(r, 50, 60, 10);
    expect(r.x0).toBe(40);
    expect(r.y0).toBe(50);
    expect(r.x1).toBe(61);
    expect(r.y1).toBe(71);
  });

  it("expandDirtyRect expands existing bounds outward", () => {
    const r: DirtyRect = { x0: 10, y0: 10, x1: 30, y1: 30 };
    const expanded = expandDirtyRect(r, 100, 200, 5);
    expect(expanded.x0).toBe(10);  // Math.min(10, 95) = 10
    expect(expanded.y0).toBe(10);  // Math.min(10, 195) = 10
    expect(expanded.x1).toBe(106); // Math.max(30, 106) = 106
    expect(expanded.y1).toBe(206); // Math.max(30, 206) = 206
  });

  it("clampDirtyRect clamps negative coords to 0", () => {
    const r: DirtyRect = { x0: -10, y0: -5, x1: 110, y1: 105 };
    const clamped = clampDirtyRect(r, 100, 100);
    expect(clamped.x0).toBe(0);
    expect(clamped.y0).toBe(0);
    expect(clamped.x1).toBe(100);
    expect(clamped.y1).toBe(100);
  });

  it("clampDirtyRect keeps valid rect unchanged", () => {
    const r: DirtyRect = { x0: 10, y0: 10, x1: 50, y1: 50 };
    const clamped = clampDirtyRect(r, 100, 100);
    expect(clamped).toEqual(r);
  });

  it("clampDirtyRect clamps oversized coords to boundary", () => {
    const r: DirtyRect = { x0: 0, y0: 0, x1: 200, y1: 300 };
    const clamped = clampDirtyRect(r, 100, 100);
    expect(clamped.x1).toBe(100);
    expect(clamped.y1).toBe(100);
  });

  it("unionDirtyRect returns the union of two rects", () => {
    const a: DirtyRect = { x0: 10, y0: 10, x1: 30, y1: 30 };
    const b: DirtyRect = { x0: 20, y0: 40, x1: 50, y1: 60 };
    const u = unionDirtyRect(a, b);
    expect(u.x0).toBe(10);
    expect(u.y0).toBe(10);
    expect(u.x1).toBe(50);
    expect(u.y1).toBe(60);
  });

  it("unionDirtyRect with empty rect returns the non-empty rect", () => {
    const empty = emptyDirtyRect();
    const r: DirtyRect = { x0: 10, y0: 10, x1: 20, y1: 20 };
    const u = unionDirtyRect(empty, r);
    expect(u).toEqual(r);
  });
});

describe("compositeMaskToImageDataDirty", () => {
  it("composites brush mask only within the dirty rect", () => {
    const imgData: ImageData = {
      width: 4,
      height: 4,
      data: new Uint8ClampedArray(4 * 4 * 4),
      colorSpace: "srgb",
    };
    // 4x4 mask: top-left 2x2 has alpha=255, rest 0
    const mask = new Uint8ClampedArray([
      255, 255, 0, 0,
      255, 255, 0, 0,
      0, 0, 0, 0,
      0, 0, 0, 0,
    ]);
    const rect: DirtyRect = { x0: 0, y0: 0, x1: 2, y1: 2 };
    compositeMaskToImageDataDirty(imgData, 0, 0, mask, 4, rect, "#ff0000", false);

    // Pixels inside rect: painted red opaque
    expect(imgData.data[0]).toBe(255);  // R at (0,0)
    expect(imgData.data[3]).toBe(255);  // A at (0,0)
    // Pixels outside rect: untouched (alpha 0)
    expect(imgData.data[2 * 4 + 0]).toBe(0);  // R at (2,0)
    expect(imgData.data[3 * 4 + 3]).toBe(0);  // A at (3,0)
  });

  it("composites eraser mask only within the dirty rect", () => {
    const imgData: ImageData = {
      width: 2,
      height: 2,
      data: new Uint8ClampedArray([
        0, 0, 0, 200,
        0, 0, 0, 200,
        0, 0, 0, 200,
        0, 0, 0, 200,
      ]),
      colorSpace: "srgb",
    };
    // 2x2 mask: top-left pixel alpha=255, rest 0
    const mask = new Uint8ClampedArray([
      255, 0,
      0, 0,
    ]);
    const rect: DirtyRect = { x0: 0, y0: 0, x1: 1, y1: 1 };
    compositeMaskToImageDataDirty(imgData, 0, 0, mask, 2, rect, "#000000", true);

    expect(imgData.data[3]).toBe(0);    // Erased (mask 255)
    expect(imgData.data[7]).toBe(200);  // Unchanged (outside rect)
  });

  it("handles imageData origin offset correctly", () => {
    // ImageData is a 2x2 sub-region of a larger composition, starting at (1,1)
    const imgData: ImageData = {
      width: 2,
      height: 2,
      data: new Uint8ClampedArray(2 * 2 * 4),
      colorSpace: "srgb",
    };
    // Full 4x4 mask: only pixel (2,2) has alpha=255
    const mask = new Uint8ClampedArray([
      0, 0, 0, 0,
      0, 0, 0, 0,
      0, 0, 255, 0,
      0, 0, 0, 0,
    ]);
    // Dirty rect in full-image coords: rows 1..2, cols 1..2
    const rect: DirtyRect = { x0: 1, y0: 1, x1: 3, y1: 3 };
    compositeMaskToImageDataDirty(imgData, 1, 1, mask, 4, rect, "#ff0000", false);

    // Pixel (2,2) in full image → (1,1) in ImageData → data[12..15]
    expect(imgData.data[12]).toBe(255); // painted red
    expect(imgData.data[15]).toBe(255); // opaque
  });
});

describe("paintMaskToContextDirty", () => {
  it("composites brush into the dirty rect region", () => {
    // Full canvas is 4x4
    const fullData = new Uint8ClampedArray(4 * 4 * 4);
    const ctx = {
      canvas: { width: 4, height: 4 } as HTMLCanvasElement,
      getImageData: vi.fn((x: number, y: number, w: number, h: number) => ({
        width: w,
        height: h,
        data: fullData.subarray((y * 4 + x) * 4, ((y + h) * 4 + x + w) * 4),
        colorSpace: "srgb",
      } as unknown as ImageData)),
      putImageData: vi.fn(),
    } as unknown as CanvasRenderingContext2D;

    // 4x4 mask: only (0,0) and (1,0) have paint
    const mask = new Uint8ClampedArray([
      128, 128, 0, 0,
      0, 0, 0, 0,
      0, 0, 0, 0,
      0, 0, 0, 0,
    ]);
    const rect: DirtyRect = { x0: 0, y0: 0, x1: 2, y1: 1 };

    paintMaskToContextDirty(ctx, mask, 4, 4, rect, "#ff0000", false);

    // getImageData called with the dirty rect bounds
    expect(ctx.getImageData).toHaveBeenCalledWith(0, 0, 2, 1);
    // putImageData called with the same rect origin
    expect(ctx.putImageData).toHaveBeenCalled();
  });

  it("skips composite when dirty rect is empty", () => {
    const ctx = {
      canvas: { width: 4, height: 4 } as HTMLCanvasElement,
      getImageData: vi.fn(),
      putImageData: vi.fn(),
    } as unknown as CanvasRenderingContext2D;

    // Rect with zero area (x1 <= x0)
    const rect: DirtyRect = { x0: 2, y0: 0, x1: 2, y1: 4 };
    paintMaskToContextDirty(ctx, new Uint8ClampedArray(16), 4, 4, rect, "#ff0000", false);

    expect(ctx.getImageData).not.toHaveBeenCalled();
    expect(ctx.putImageData).not.toHaveBeenCalled();
  });
});

describe("gradient dab experiment (R3, flag-gated)", () => {
  const tip = {
    width: 32,
    height: 32,
    radius: 16,
    diameter: 32,
    R_nominal: 16,
    dataSize: 64,
    data: (() => {
      const d = new Float32Array(64 * 64);
      for (let y = 0; y < 64; y++)
        for (let x = 0; x < 64; x++) {
          const nx = (x - 32) / 31;
          const ny = (y - 32) / 31;
          const dist = Math.sqrt(nx * nx + ny * ny);
          d[y * 64 + x] = dist <= 1 ? Math.pow(1 - dist, 0.8) : 0;
        }
      return d;
    })(),
  };
  const endpoint = { x: 100, y: 100, pressure: 1 };
  function makeCtx() {
    return {
      canvas: { width: 1920, height: 1080 },
      createRadialGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
      fillRect: vi.fn(),
      getImageData: vi.fn(() => ({ data: new Uint8ClampedArray(32 * 32 * 4), width: 32, height: 32 })),
      putImageData: vi.fn(),
      globalCompositeOperation: null,
      fillStyle: null,
    } as unknown as CanvasRenderingContext2D;
  }
  const setFlag = (on: boolean) => {
    // unit-node has no localStorage; stub what isGradientDabEnabled reads
    (globalThis as any).localStorage = { getItem: () => (on ? "1" : null) };
  };

  it("isGradientDabEnabled defaults to false", () => {
    setFlag(false);
    expect(isGradientDabEnabled()).toBe(false);
  });

  it("flag on -> dab routes through radialGradient fillRect (no CPU pixel pass)", () => {
    setFlag(true);
    const ctx = makeCtx();
    const ok = paintTransientBrushTipToContext(ctx, tip as any, endpoint, null, 0.5, "#c85028", false);
    expect(ok).toBe(true);
    expect(ctx.createRadialGradient).toHaveBeenCalledTimes(1);
    expect(ctx.fillRect).toHaveBeenCalledTimes(1);
    expect(ctx.putImageData).not.toHaveBeenCalled();
  });

  it("flag off -> legacy per-pixel path (getImageData/putImageData used)", () => {
    setFlag(false);
    const ctx = makeCtx();
    const ok = paintTransientBrushTipToContext(ctx, tip as any, endpoint, null, 0.5, "#c85028", false);
    expect(ok).toBe(true);
    expect(ctx.createRadialGradient).not.toHaveBeenCalled();
    expect(ctx.getImageData).toHaveBeenCalled();
    expect(ctx.putImageData).toHaveBeenCalled();
  });

  it("eraser never uses the gradient experiment", () => {
    setFlag(true);
    const ctx = makeCtx();
    paintTransientBrushTipToContext(ctx, tip as any, endpoint, null, 0.5, "#c85028", true);
    expect(ctx.createRadialGradient).not.toHaveBeenCalled();
  });

  it("buildGradientStops samples center->edge monotonically from tip data", () => {
    const stops = buildGradientStops(tip as any, 12);
    expect(stops.length).toBe(13);
    expect(stops[0][0]).toBe(0);
    expect(stops[0][1]).toBeGreaterThanOrEqual(stops[12][1]);
  });
});
