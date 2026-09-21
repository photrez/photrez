import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { SelectionOperations } from "../SelectionOperations";
import { DocumentEngine } from "../../../engine/document";

/**
 * Diagnosis repro for: rect-marquee over the middle ~30% + Del destroys ~80%.
 *
 * Suspect: fillSelectionWithTransparent builds OffscreenCanvas(layer.width,
 * layer.height) from MODEL dims, draws the real bitmap 1:1 at the origin, and
 * replaces the bitmap — so any model-dims vs bitmap-dims divergence destroys
 * pixels far outside the marquee. These tests inject that divergence directly
 * (fault injection, not a production path) to prove the mechanism.
 */

function setupOffscreenCanvasMock() {
  const MockOffscreenCanvas = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    this._buffer = new Uint8ClampedArray(w * h * 4);
    const ctx = {
      width: w,
      height: h,
      _buffer: this._buffer,
      _fillStyle: "",
      get fillStyle() { return this._fillStyle; },
      set fillStyle(v: string) { this._fillStyle = v; },
      clearRect: vi.fn(function (this: any, x: number, y: number, cw: number, ch: number) {
        for (let row = Math.round(y); row < Math.round(y + ch); row++) {
          for (let col = Math.round(x); col < Math.round(x + cw); col++) {
            if (row < 0 || row >= this.height || col < 0 || col >= this.width) continue;
            const idx = (row * this.width + col) * 4;
            this._buffer[idx] = 0;
            this._buffer[idx + 1] = 0;
            this._buffer[idx + 2] = 0;
            this._buffer[idx + 3] = 0;
          }
        }
      }),
      drawImage: vi.fn(function (this: any, src: any, dx: number, dy: number) {
        if (src && src._buffer) {
          for (let row = 0; row < src.height; row++) {
            for (let col = 0; col < src.width; col++) {
              const destCol = Math.round(dx + col);
              const destRow = Math.round(dy + row);
              if (destRow < 0 || destRow >= this.height || destCol < 0 || destCol >= this.width) continue;
              const srcIdx = (row * src.width + col) * 4;
              const dstIdx = (destRow * this.width + destCol) * 4;
              this._buffer[dstIdx] = src._buffer[srcIdx];
              this._buffer[dstIdx + 1] = src._buffer[srcIdx + 1];
              this._buffer[dstIdx + 2] = src._buffer[srcIdx + 2];
              this._buffer[dstIdx + 3] = src._buffer[srcIdx + 3];
            }
          }
        }
      }),
      fillRect: vi.fn(function (this: any, x: number, y: number, fw: number, fh: number) {
        const m = /^#([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})$/.exec(this._fillStyle);
        let r = 0, g = 0, b = 0;
        if (m) { r = parseInt(m[1], 16); g = parseInt(m[2], 16); b = parseInt(m[3], 16); }
        for (let row = y; row < y + fh; row++) {
          for (let col = x; col < x + fw; col++) {
            if (row < 0 || row >= this.height || col < 0 || col >= this.width) continue;
            const idx = (row * this.width + col) * 4;
            this._buffer[idx] = r;
            this._buffer[idx + 1] = g;
            this._buffer[idx + 2] = b;
            this._buffer[idx + 3] = 255;
          }
        }
      }),
      getImageData: vi.fn(function (this: any) {
        return { data: this._buffer, width: this.width, height: this.height } as unknown as ImageData;
      }),
      putImageData: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      translate: vi.fn(),
      rotate: vi.fn(),
      scale: vi.fn(),
    };
    this.getContext = vi.fn(() => ctx);
    this.transferToImageBitmap = vi.fn(function (this: any) {
      return { width: this.width, height: this.height, _buffer: this._buffer } as unknown as ImageBitmap;
    });
  };

  vi.stubGlobal("OffscreenCanvas", MockOffscreenCanvas as unknown as typeof OffscreenCanvas);
}

function opaqueBitmap(size: number) {
  const offscreen = new OffscreenCanvas(size, size);
  const ctx = offscreen.getContext("2d")!;
  ctx.fillStyle = "#FF0000";
  ctx.fillRect(0, 0, size, size);
  return offscreen.transferToImageBitmap();
}

/** Fraction of ORIGINAL pixels destroyed (transparent or missing). */
function destroyedFraction(result: any, originalPixels: number): number {
  let opaque = 0;
  const buf = result._buffer as Uint8ClampedArray;
  const total = result.width * result.height;
  for (let i = 0; i < total; i++) {
    if (buf[i * 4 + 3] !== 0) opaque++;
  }
  return 1 - opaque / originalPixels;
}

function alphaAt(result: any, x: number, y: number): number | null {
  if (x < 0 || y < 0 || x >= result.width || y >= result.height) return null;
  return (result._buffer as Uint8ClampedArray)[(y * result.width + x) * 4 + 3];
}

describe("selection delete under model-dims vs bitmap-dims divergence (diagnosis repro)", () => {
  beforeEach(() => {
    setupOffscreenCanvasMock();
    SelectionOperations.__resetClipboard();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("control: dims match, middle marquee clears only the marquee", () => {
    const engine = new DocumentEngine("test", "Test", 100, 100);
    const layer = engine.addLayer("Photo", 100, 100);
    engine.setLayerImageBitmap(layer.id, opaqueBitmap(100));

    engine.createSelection(35, 35, 30, 30);
    SelectionOperations.deleteSelection(engine);

    const result = engine.getLayerImageBitmap(layer.id) as any;
    expect(result.width).toBe(100);
    expect(result.height).toBe(100);
    // 30x30 of 100x100 selected -> ~9% destroyed.
    expect(destroyedFraction(result, 10000)).toBeLessThan(0.15);
    // Edges intact, marquee cleared.
    expect(alphaAt(result, 5, 5)).toBe(255);
    expect(alphaAt(result, 50, 50)).toBe(0);
  });

  // Deferred: dims-divergence producer needs production measurement; pinned until fixed.
  it.fails("mismatch model LARGER than bitmap: delete destroys far beyond the marquee", () => {
    const engine = new DocumentEngine("test", "Test", 100, 100);
    const layer = engine.addLayer("Photo", 60, 60);
    engine.setLayerImageBitmap(layer.id, opaqueBitmap(60));
    // Fault injection: model dims diverge to the document size (the fallback
    // fillSelectionWithTransparent would then read 100x100 for a 60x60 bitmap).
    engine.getLayer(layer.id)!.width = 100;
    engine.getLayer(layer.id)!.height = 100;

    engine.createSelection(35, 35, 30, 30);
    SelectionOperations.deleteSelection(engine);

    const result = engine.getLayerImageBitmap(layer.id) as any;
    // A 9%-area marquee must not destroy a quarter of the photo. Measured
    // against the model area the user sees: the never-copied right/bottom
    // bands render blank, so they count as destroyed.
    expect(destroyedFraction(result, 10000)).toBeLessThan(0.25);
  });

  // Deferred: dims-divergence producer needs production measurement; pinned until fixed.
  it.fails("mismatch model SMALLER than bitmap: delete destroys far beyond the marquee", () => {
    const engine = new DocumentEngine("test", "Test", 100, 100);
    const layer = engine.addLayer("Photo", 100, 100);
    engine.setLayerImageBitmap(layer.id, opaqueBitmap(100));
    // Fault injection: model dims shrink below the real bitmap.
    engine.getLayer(layer.id)!.width = 60;
    engine.getLayer(layer.id)!.height = 60;

    engine.createSelection(35, 35, 30, 30);
    SelectionOperations.deleteSelection(engine);

    const result = engine.getLayerImageBitmap(layer.id) as any;
    // A 9%-area marquee must not destroy a quarter of the photo.
    expect(destroyedFraction(result, 10000)).toBeLessThan(0.25);
  });

  it("scaled layer (production convention: base dims + scale in transform): delete clears only the marquee", () => {
    const engine = new DocumentEngine("test", "Test", 100, 100);
    const layer = engine.addLayer("Photo", 100, 100);
    engine.setLayerImageBitmap(layer.id, opaqueBitmap(100));
    // Placed-photo state per the production convention: bitmap and model dims
    // stay at base size; display size lives in the transform only.
    engine.transformLayer(layer.id, { x: 0, y: 0, scaleX: 0.5, scaleY: 0.5 });

    // Displayed box is (0,0,50,50); marquee over its middle.
    engine.createSelection(17, 17, 16, 16);
    SelectionOperations.deleteSelection(engine);

    const result = engine.getLayerImageBitmap(layer.id) as any;
    expect(result.width).toBe(100);
    expect(result.height).toBe(100);
    // 16x16 of the 50x50 displayed box selected -> ~10% destroyed.
    expect(destroyedFraction(result, 10000)).toBeLessThan(0.25);
    // Doc (25,25) maps to local (50,50), inside the marquee -> cleared.
    expect(alphaAt(result, 50, 50)).toBe(0);
    // Doc (5,5) maps to local (10,10), outside the marquee -> intact.
    expect(alphaAt(result, 10, 10)).toBe(255);
  });
});
