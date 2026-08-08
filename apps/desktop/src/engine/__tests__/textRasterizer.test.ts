// Tests for the pure text rasterizer. Mirrors shapeRaster.test.ts: the
// OffscreenCanvas global is stubbed so the REAL rasterizeText body runs
// (same seam shapeRaster uses), and buildCSSFont/wrapText are pure and need
// no canvas at all. Lives in unit-node (node environment, no jsdom/document).

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { buildCSSFont, wrapText, rasterizeText } from "../textRasterizer";
import type { TextMeasurer } from "../textRasterizer";
import { DEFAULT_TEXT_DATA } from "../textTypes";

/** Deterministic measurer: each char is `size` px wide. */
function charMeasure(size: number): TextMeasurer {
  return { measureText: (s: string) => ({ width: s.length * size }) };
}

/** Reads the scaled font size out of a CSS font string (e.g. "96px \"Arial\""). */
function fontPxFrom(font: string): number {
  const m = /(\d+(?:\.\d+)?)px/.exec(font);
  return m ? Number(m[1]) : 0;
}

interface PaintCall {
  text: string;
  x: number;
  y: number;
}

interface RasterRecord {
  width: number;
  height: number;
  paints: PaintCall[];
  ctx: {
    font: string;
    fillStyle: string;
    textBaseline: string;
    letterSpacing: string | undefined;
  };
}

/**
 * Mirrors shapeRaster.test.ts: stubs the global OffscreenCanvas so
 * rasterizeText runs its real makeCanvas/toBitmap seam against a scripted
 * context. Metric contract mirrors REAL canvas behavior: measured width
 * scales with the font size assigned to the context (device px), so wrap
 * decisions track the scaled font. Ascent/descent are fixed values (80/24,
 * above the 0.8/0.2 fontSize fallback so provided metrics win).
 */
function setupOffscreenCanvasMock() {
  const instances: RasterRecord[] = [];
  const MockOffscreenCanvas = function (this: unknown, w: number, h: number) {
    const self = this as {
      width: number;
      height: number;
      getContext: () => unknown;
      transferToImageBitmap: () => unknown;
    };
    self.width = w;
    self.height = h;
    const ctx = {
      font: "",
      fillStyle: "",
      textBaseline: "alphabetic",
      letterSpacing: undefined as string | undefined,
      measureText: (s: string) => ({
        // real canvas: wider font -> wider text (fontPx is device px here)
        width: s.length * (fontPxFrom(ctx.font) * 0.5),
        fontBoundingBoxAscent: 80,
        fontBoundingBoxDescent: 24,
      }),
      fillText: (t: string, x: number, y: number) => {
        record.paints.push({ text: t, x, y });
      },
    };
    // record.ctx MUST be the same object the rasterizer writes to, otherwise
    // font/fillStyle/letterSpacing assertions read a stale copy.
    const record: RasterRecord = {
      width: w,
      height: h,
      paints: [],
      ctx,
    };
    instances.push(record);
    self.getContext = () => ctx;
    // Real OffscreenCanvas: transferToImageBitmap returns the CURRENT buffer
    // size (rasterizeText resizes the canvas after construction), so read
    // live dims instead of the constructor args.
    self.transferToImageBitmap = () => ({ width: self.width, height: self.height });
  } as unknown as typeof OffscreenCanvas;
  vi.stubGlobal("OffscreenCanvas", MockOffscreenCanvas);
  return instances;
}

describe("buildCSSFont", () => {
  it("plain 400 normal yields `48px \"Arial\"`", () => {
    expect(buildCSSFont(DEFAULT_TEXT_DATA, 48)).toBe('48px "Arial"');
  });

  it("italic + 700 yields `italic 700 96px \"Arial\"` (scale 2, size 48 -> 96)", () => {
    const data = { ...DEFAULT_TEXT_DATA, fontStyle: "italic" as const, fontWeight: 700 };
    expect(buildCSSFont(data, 96)).toBe('italic 700 96px "Arial"');
  });

  it("italic with weight 400 omits the weight prefix", () => {
    const data = { ...DEFAULT_TEXT_DATA, fontStyle: "italic" as const };
    expect(buildCSSFont(data, 48)).toBe('italic 48px "Arial"');
  });

  it("quotes the font family", () => {
    const data = { ...DEFAULT_TEXT_DATA, fontFamily: "Times New Roman" };
    expect(buildCSSFont(data, 48)).toBe('48px "Times New Roman"');
  });
});

describe("wrapText", () => {
  it("wraps on spaces when a word does not fit the current line", () => {
    expect(wrapText(charMeasure(10), "one two three", 70)).toEqual(["one two", "three"]);
  });

  it("character-breaks a single word longer than maxWidth", () => {
    expect(wrapText(charMeasure(10), "supercalifragilistic", 50)).toEqual([
      "super",
      "calif",
      "ragil",
      "istic",
    ]);
  });

  it("preserves empty paragraphs (empty lines)", () => {
    expect(wrapText(charMeasure(10), "a\n\nb", 100)).toEqual(["a", "", "b"]);
  });

  it("returns [\"\"] for the empty string (never [])", () => {
    expect(wrapText(charMeasure(10), "", 100)).toEqual([""]);
  });

  it("wraps CJK paragraphs per character (R9)", () => {
    expect(wrapText(charMeasure(10), "测试文本", 15)).toEqual(["测", "试", "文", "本"]);
  });

  it("per-character wrapping applies to whole paragraph once CJK is present", () => {
    expect(wrapText(charMeasure(10), "ab测", 20)).toEqual(["ab", "测"]);
  });

  it("does not wrap when everything fits", () => {
    expect(wrapText(charMeasure(10), "hello world", 200)).toEqual(["hello world"]);
  });

  it("normalizes consecutive spaces during wrapping (a  b -> a b)", () => {
    expect(wrapText(charMeasure(10), "a  b", 100)).toEqual(["a b"]);
  });

  it("character-breaks surrogate pairs as whole code points (emoji)", () => {
    // each emoji is 2 UTF-16 units; width 4 per emoji at charMeasure(2)
    const lines = wrapText(charMeasure(2), "🏀🏀🏀🏀", 10);
    expect(lines).toEqual(["🏀🏀", "🏀🏀"]);
    expect(lines.join("")).toBe("🏀🏀🏀🏀"); // no content loss, no lone surrogates
  });

  it("does not throw or loop forever on maxWidth <= 0", () => {
    const lines = wrapText(charMeasure(10), "abc", 0);
    expect(lines.length).toBeGreaterThan(0);
    expect(lines.join("")).toBe("abc");
  });
});

describe("rasterizeText", () => {
  let instances: RasterRecord[];
  beforeEach(() => {
    instances = setupOffscreenCanvasMock();
  });
  afterEach(() => vi.unstubAllGlobals());

  const pointMultiLine = {
    ...DEFAULT_TEXT_DATA,
    content: "AB\nCD",
  };

  it("point multi-line: unscaled doc dims from 2x canvas (width >= text width)", () => {
    const result = rasterizeText(pointMultiLine);
    // canvas: width max line 2*96*0.5 + 4 padding = 100; height 2*1.4*96 + 80+24 + 4 = 377
    const bitmap = result.imageBitmap as unknown as { width: number; height: number };
    expect(bitmap.width).toBe(100);
    expect(bitmap.height).toBe(377);
    expect(result.width).toBe(50);
    expect(result.height).toBe(188.5);
    // doc width covers the longest line (48 doc px) plus padding
    expect(result.width).toBeGreaterThanOrEqual(48);
    expect(result.height).toBeGreaterThanOrEqual(2 * 1.4 * 48);
  });

  it("scale=2 bitmap is exactly 2x the doc dimensions", () => {
    const result = rasterizeText(pointMultiLine);
    const bitmap = result.imageBitmap as unknown as { width: number; height: number };
    expect(bitmap.width).toBeCloseTo(result.width * 2, 5);
    expect(bitmap.height).toBeCloseTo(result.height * 2, 5);
  });

  it("area mode: canvas width is boxWidth * scale, not line width", () => {
    const data = { ...DEFAULT_TEXT_DATA, boxMode: "area" as const, boxWidth: 100, content: "AB" };
    const result = rasterizeText(data);
    const bitmap = result.imageBitmap as unknown as { width: number; height: number };
    expect(bitmap.width).toBe(204); // 100*2 + 4 padding
    expect(result.width).toBe(102);
  });

  it("draws with top baseline, the composed font, fill color and text-align x offsets", () => {
    const data = {
      ...DEFAULT_TEXT_DATA,
      content: "AB\nCDE",
      align: "right" as const,
      color: "#123456",
      fontStyle: "italic" as const,
      fontWeight: 700,
    };
    rasterizeText(data);
    const record = instances[0];
    expect(record.ctx.font).toBe('italic 700 96px "Arial"');
    expect(record.ctx.fillStyle).toBe("#123456");
    expect(record.ctx.textBaseline).toBe("top");
    // total width = "CDE" (3*96*0.5 = 144); "AB" (2*96*0.5 = 96) right x = 2 + 144 - 96 = 50
    expect(record.paints).toEqual([
      { text: "AB", x: 50, y: 2 },
      { text: "CDE", x: 2, y: 2 + 1.4 * 96 },
    ]);
  });

  it("applies letterSpacing when the context supports it", () => {
    const data = { ...DEFAULT_TEXT_DATA, content: "AB", letterSpacing: 5 };
    rasterizeText(data);
    expect(instances[0].ctx.letterSpacing).toBe("5px");
  });

  it("ignores letterSpacing silently when unsupported (no throw)", () => {
    const noLetterSpacing = function (this: unknown, w: number, h: number) {
      const self = this as { width: number; height: number; getContext: () => unknown; transferToImageBitmap: () => unknown };
      self.width = w;
      self.height = h;
      const ctx = {
        font: "",
        fillStyle: "",
        textBaseline: "alphabetic",
        measureText: (s: string) => ({
          width: s.length * (fontPxFrom(ctx.font) * 0.5),
          fontBoundingBoxAscent: 80,
          fontBoundingBoxDescent: 24,
        }),
        fillText: () => {},
      };
      self.getContext = () => ctx;
      self.transferToImageBitmap = () => ({ width: self.width, height: self.height });
    } as unknown as typeof OffscreenCanvas;
    vi.stubGlobal("OffscreenCanvas", noLetterSpacing);
    const result = rasterizeText({ ...DEFAULT_TEXT_DATA, content: "AB", letterSpacing: 12 });
    const bitmap = result.imageBitmap as unknown as { width: number };
    expect(bitmap.width).toBeGreaterThan(0);
  });

  it("falls back to fontSize*0.8/0.2 when fontBoundingBox metrics are missing", () => {
    const noMetrics = function (this: unknown, w: number, h: number) {
      const self = this as { width: number; height: number; getContext: () => unknown; transferToImageBitmap: () => unknown };
      self.width = w;
      self.height = h;
      const ctx = {
        font: "",
        fillStyle: "",
        textBaseline: "alphabetic",
        measureText: (s: string) => ({ width: s.length * (fontPxFrom(ctx.font) * 0.5) }),
        fillText: () => {},
      };
      self.getContext = () => ctx;
      self.transferToImageBitmap = () => ({ width: self.width, height: self.height });
    } as unknown as typeof OffscreenCanvas;
    vi.stubGlobal("OffscreenCanvas", noMetrics);
    // 2 lines: 2*1.4*96 + fallback (0.8+0.2)*96 + 4 = 368.8 -> ceil 369
    const result = rasterizeText(pointMultiLine);
    const bitmap = result.imageBitmap as unknown as { width: number; height: number };
    expect(bitmap.height).toBe(369);
  });

  it("invalid data (null, garbage, fontSize 0, NaN) does not throw and yields a bitmap", () => {
    const invalidInputs: unknown[] = [
      null,
      "junk",
      42,
      { fontSize: 0 },
      { fontSize: NaN },
      { fontSize: "48", content: 42 },
    ];
    for (const input of invalidInputs) {
      const result = rasterizeText(input as Parameters<typeof rasterizeText>[0]);
      const bitmap = result.imageBitmap as unknown as { width: number; height: number };
      expect(bitmap.width).toBeGreaterThan(0);
      expect(bitmap.height).toBeGreaterThan(0);
      expect(result.width).toBeGreaterThan(0);
      expect(result.height).toBeGreaterThan(0);
    }
  });

  it("sanity-clamps the scale (0/NaN -> 2, out-of-range -> 1/64..64)", () => {
    // area mode exposes the scale in the canvas width (text width always wins
    // in point mode, so use area to prove the clamp)
    const base = { ...DEFAULT_TEXT_DATA, content: "AB", boxMode: "area" as const, boxWidth: 100 };
    const s0 = rasterizeText(base, 0);
    const sNaN = rasterizeText(base, NaN);
    const sHuge = rasterizeText(base, 1000);
    expect(s0.imageBitmap as unknown as { width: number }).toMatchObject({
      width: 204, // 100*2 default scale + 4 padding
    });
    expect(sNaN.imageBitmap as unknown as { width: number }).toMatchObject({
      width: 204,
    });
    // scale clamped to 64: width = ceil(100*64 + 4) = 6404
    expect(sHuge.imageBitmap as unknown as { width: number }).toMatchObject({
      width: 6404,
    });
  });

  it("area text wraps against the SCALED box width (device px, matches measureText)", () => {
    // "AA" at fontSize 48: measured width = 2*fontPx*0.5 = 24*scale*2 (48 at
    // scale 1, 96 at scale 2). boxWidth 60 doc: scaled box is 60 (scale 1)
    // and 120 (scale 2) -> one line at BOTH scales. Passing the unscaled 60
    // doc width would wrap "AA" into 2 lines at scale 2 (96 > 60).
    const data = { ...DEFAULT_TEXT_DATA, content: "AA", boxMode: "area" as const, boxWidth: 60 };
    const r1 = rasterizeText(data, 1);
    const r2 = rasterizeText(data, 2);
    expect(instances[0].paints.length).toBe(1);
    expect(instances[1].paints.length).toBe(1);
    // single line at scale 2: height = 1.4*96 + 80 + 24 + 4 = 242.4 -> 243
    expect((r2.imageBitmap as unknown as { height: number }).height).toBe(243);
    expect((r2.imageBitmap as unknown as { width: number }).width).toBe(124); // 60*2 + 4
  });

  it("clamps the scale floor (1/128 -> 1/64) to a small but valid canvas", () => {
    const base = { ...DEFAULT_TEXT_DATA, content: "AB", boxMode: "area" as const, boxWidth: 100 };
    const result = rasterizeText(base, 1 / 128);
    const bitmap = result.imageBitmap as unknown as { width: number; height: number };
    expect(bitmap.width).toBe(6); // ceil(100*(1/64) + 4)
    expect(bitmap.width).toBeGreaterThan(0);
    expect(bitmap.height).toBeGreaterThan(0);
    expect(result.width).toBeGreaterThan(0);
    expect(result.height).toBeGreaterThan(0);
  });

  it("clamps the canvas to 8192 in both dimensions (huge font + many lines)", () => {
    const data = {
      ...DEFAULT_TEXT_DATA,
      fontSize: 2000,
      content: "X".repeat(400) + "\n".repeat(59) + "Y",
    };
    const result = rasterizeText(data);
    const bitmap = result.imageBitmap as unknown as { width: number; height: number };
    expect(bitmap.width).toBe(8192);
    expect(bitmap.height).toBe(8192);
  });

  it("empty content still returns a valid bitmap (does not crash)", () => {
    const result = rasterizeText(DEFAULT_TEXT_DATA);
    const bitmap = result.imageBitmap as unknown as { width: number; height: number };
    expect(bitmap.width).toBeGreaterThan(0);
    expect(bitmap.height).toBeGreaterThan(0);
  });

  it("fallback without OffscreenCanvas: returns canvas with no-op close() (ancient WebKit path)", () => {
    // Mirrors the shapeRaster.test.ts fallback seam test. This file runs in
    // unit-node (no jsdom), so document is stubbed instead of spied.
    vi.stubGlobal("OffscreenCanvas", undefined);
    const ctxStub: Record<string, unknown> & {
      font: string;
      fillStyle: string;
      textBaseline: string;
      letterSpacing: string;
    } = {
      font: "",
      fillStyle: "",
      textBaseline: "alphabetic",
      letterSpacing: "0px",
      measureText: (s: string) => ({
        width: s.length * 10,
        fontBoundingBoxAscent: 80,
        fontBoundingBoxDescent: 24,
      }),
      fillText: () => undefined,
    };
    const canvasMock = {
      width: 0,
      height: 0,
      getContext: () => ctxStub,
    } as unknown as HTMLCanvasElement;
    vi.stubGlobal("document", { createElement: () => canvasMock });

    const result = rasterizeText(DEFAULT_TEXT_DATA);
    const bitmap = result.imageBitmap as unknown as { close?: () => void; width: number };
    expect(typeof bitmap.close).toBe("function");
    // Consumers call bitmap.close() on replace/dispose; the fallback must not throw.
    expect(() => bitmap.close?.()).not.toThrow();
    expect(bitmap.width).toBeGreaterThan(0);
  });
});
