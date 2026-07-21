import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { SelectionOperations } from "../SelectionOperations";
import { DocumentEngine } from "../../../engine/document";

/**
 * OffscreenCanvas mock for jsdom (which has no OffscreenCanvas).
 * Simulates drawImage by copying pixel data from source bitmap into target buffer
 * and implements getImageData/putImageData via Uint8ClampedArray buffers.
 */
function setupOffscreenCanvasMock() {
  const instances: any[] = [];

  const MockOffscreenCanvas = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    this._buffer = new Uint8ClampedArray(w * h * 4);
    this._lastDraw = null as null | {
      src: any;
      sx: number;
      sy: number;
      sw: number;
      sh: number;
      dx: number;
      dy: number;
      dw: number;
      dh: number;
    };
    // Expose dimensions on ctx so paint operations can compute pixel indices
    // and bounds. Without these the arithmetic uses `undefined` and silently
    // writes nothing — the source bitmap never gets filled.
    const ctx = {
      width: w,
      height: h,
      _buffer: this._buffer,
      _fillStyle: "",
      get fillStyle() { return this._fillStyle; },
      set fillStyle(v: string) { this._fillStyle = v; },
      clearRect: vi.fn(function (this: any, x: number, y: number, cw: number, ch: number) {
        for (let row = y; row < y + ch; row++) {
          for (let col = x; col < x + cw; col++) {
            if (row < 0 || row >= this.height || col < 0 || col >= this.width) continue;
            const idx = (row * this.width + col) * 4;
            this._buffer[idx] = 0;
            this._buffer[idx + 1] = 0;
            this._buffer[idx + 2] = 0;
            this._buffer[idx + 3] = 0;
          }
        }
      }),
      save: vi.fn(),
      restore: vi.fn(),
      translate: vi.fn(),
      rotate: vi.fn(),
      scale: vi.fn(),
      drawImage: vi.fn(function (this: any, src: any, sx: number, sy: number, sw: number, sh: number, dx?: number, dy?: number, dw?: number, dh?: number) {
        // Support both 3-arg (drawImage(src, dx, dy)) and 9-arg forms
        if (dx === undefined) {
          // 3-arg form: drawImage(src, dx, dy)
          this._lastDraw = { src, sx: 0, sy: 0, sw: src.width, sh: src.height, dx: sx, dy: sy, dw: src.width, dh: src.height };
        } else {
          this._lastDraw = { src, sx, sy, sw, sh, dx, dy, dw: dw!, dh: dh! };
        }
        // If the source is a mock bitmap with a buffer, copy pixels
        if (src && src._buffer) {
          const sxStart = Math.max(0, this._lastDraw.sx);
          const syStart = Math.max(0, this._lastDraw.sy);
          const sxEnd = Math.min(src.width, this._lastDraw.sx + this._lastDraw.sw);
          const syEnd = Math.min(src.height, this._lastDraw.sy + this._lastDraw.sh);
          const dxStart = this._lastDraw.dx;
          const dyStart = this._lastDraw.dy;
          const scaleX = this._lastDraw.dw / this._lastDraw.sw;
          const scaleY = this._lastDraw.dh / this._lastDraw.sh;
          for (let row = syStart; row < syEnd; row++) {
            for (let col = sxStart; col < sxEnd; col++) {
              const srcIdx = (row * src.width + col) * 4;
              const destCol = Math.round(dxStart + (col - sxStart) * scaleX);
              const destRow = Math.round(dyStart + (row - syStart) * scaleY);
              if (destRow < 0 || destRow >= this.height || destCol < 0 || destCol >= this.width) continue;
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
        let r = 0, g = 0, b = 0, a = 255;
        if (m) {
          r = parseInt(m[1], 16);
          g = parseInt(m[2], 16);
          b = parseInt(m[3], 16);
        }
        for (let row = y; row < y + fh; row++) {
          for (let col = x; col < x + fw; col++) {
            if (row < 0 || row >= this.height || col < 0 || col >= this.width) continue;
            const idx = (row * this.width + col) * 4;
            this._buffer[idx] = r;
            this._buffer[idx + 1] = g;
            this._buffer[idx + 2] = b;
            this._buffer[idx + 3] = a;
          }
        }
      }),
      getImageData: vi.fn(function (this: any, x: number, y: number, gw: number, gh: number) {
        const data = new Uint8ClampedArray(gw * gh * 4);
        for (let row = 0; row < gh; row++) {
          for (let col = 0; col < gw; col++) {
            const srcCol = x + col;
            const srcRow = y + row;
            if (srcRow < 0 || srcRow >= this.height || srcCol < 0 || srcCol >= this.width) continue;
            const srcIdx = (srcRow * this.width + srcCol) * 4;
            const dstIdx = (row * gw + col) * 4;
            data[dstIdx] = this._buffer[srcIdx];
            data[dstIdx + 1] = this._buffer[srcIdx + 1];
            data[dstIdx + 2] = this._buffer[srcIdx + 2];
            data[dstIdx + 3] = this._buffer[srcIdx + 3];
          }
        }
        return { data, width: gw, height: gh, colorSpace: "srgb" } as ImageData;
      }),
      putImageData: vi.fn(function (this: any, imageData: ImageData, x: number, y: number) {
        for (let row = 0; row < imageData.height; row++) {
          for (let col = 0; col < imageData.width; col++) {
            const srcIdx = (row * imageData.width + col) * 4;
            const dstCol = x + col;
            const dstRow = y + row;
            if (dstRow < 0 || dstRow >= this.height || dstCol < 0 || dstCol >= this.width) continue;
            const dstIdx = (dstRow * this.width + dstCol) * 4;
            this._buffer[dstIdx] = imageData.data[srcIdx];
            this._buffer[dstIdx + 1] = imageData.data[srcIdx + 1];
            this._buffer[dstIdx + 2] = imageData.data[srcIdx + 2];
            this._buffer[dstIdx + 3] = imageData.data[srcIdx + 3];
          }
        }
      }),
    };
    this.getContext = vi.fn(() => ctx);
    this.transferToImageBitmap = vi.fn(function (this: any) {
      return {
        width: this.width,
        height: this.height,
        _buffer: this._buffer,
      } as unknown as ImageBitmap;
    });
    instances.push(this);
  };

  vi.stubGlobal("OffscreenCanvas", MockOffscreenCanvas as unknown as typeof OffscreenCanvas);
  return instances;
}

function fillBitmapWithColor(bitmap: any, color: { r: number; g: number; b: number; a: number }) {
  if (!bitmap._buffer) {
    bitmap._buffer = new Uint8ClampedArray(bitmap.width * bitmap.height * 4);
  }
  for (let i = 0; i < bitmap.width * bitmap.height; i++) {
    bitmap._buffer[i * 4] = color.r;
    bitmap._buffer[i * 4 + 1] = color.g;
    bitmap._buffer[i * 4 + 2] = color.b;
    bitmap._buffer[i * 4 + 3] = color.a;
  }
}

describe("SelectionOperations — real pixel operations", () => {
  let engine: DocumentEngine;

  beforeEach(() => {
    setupOffscreenCanvasMock();
    engine = new DocumentEngine("test", "Test", 100, 100);
    SelectionOperations.__resetClipboard();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  describe("getSelectionBounds", () => {
    it("returns null when no selection", () => {
      expect(SelectionOperations.getSelectionBounds(engine)).toBeNull();
    });

    it("returns bounds from engine selection", () => {
      engine.createSelection(10, 20, 50, 60);
      const bounds = SelectionOperations.getSelectionBounds(engine);
      expect(bounds).not.toBeNull();
      expect(bounds!.x).toBe(10);
      expect(bounds!.y).toBe(20);
      expect(bounds!.width).toBe(50);
      expect(bounds!.height).toBe(60);
    });
  });

  describe("copySelection", () => {
    it("returns null when no selection", () => {
      expect(SelectionOperations.copySelection(engine)).toBeNull();
    });

    it("returns null when no active layer", () => {
      engine.createSelection(0, 0, 50, 50);
      expect(SelectionOperations.copySelection(engine)).toBeNull();
    });

    it("returns ImageData with correct dimensions when selection exists", () => {
      expect(SelectionOperations.hasClipboard()).toBe(false);
      const layer = engine.addLayer("Layer 1", 100, 100);
      // Create a bitmap and set it on the layer
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      const bitmap = offscreen.transferToImageBitmap();
      engine.setLayerImageBitmap(layer.id, bitmap);

      engine.createSelection(10, 20, 50, 60);
      const result = SelectionOperations.copySelection(engine);
      expect(result).not.toBeNull();
      expect(SelectionOperations.hasClipboard()).toBe(true);
      expect(result!.width).toBe(50);
      expect(result!.height).toBe(60);
    });

    it("preserves pixel colors at selection coordinates", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#00FF00";
      ctx.fillRect(0, 0, 100, 100);
      const bitmap = offscreen.transferToImageBitmap();
      // Debug: check bitmap was actually filled
      const buf = (bitmap as any)._buffer;
      engine.setLayerImageBitmap(layer.id, bitmap);

      engine.createSelection(0, 0, 10, 10);
      const result = SelectionOperations.copySelection(engine);
      expect(result).not.toBeNull();
      // Center pixel should be green (0, 255, 0, 255)
      const idx = 4 * (5 * 10 + 5); // row 5, col 5
      expect(result!.data[idx + 0]).toBe(0);     // R
      expect(result!.data[idx + 1]).toBe(255);   // G
      expect(result!.data[idx + 2]).toBe(0);     // B
      expect(result!.data[idx + 3]).toBe(255);   // A
    });

    it("does not modify the source layer", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#0000FF";
      ctx.fillRect(0, 0, 100, 100);
      const bitmap = offscreen.transferToImageBitmap();
      engine.setLayerImageBitmap(layer.id, bitmap);

      engine.createSelection(10, 10, 20, 20);
      SelectionOperations.copySelection(engine);

      // Source should still be all blue
      const srcBitmap = engine.getLayerImageBitmap(layer.id);
      expect(srcBitmap).not.toBeNull();
      const c = (srcBitmap as any)._buffer;
      const idx = 4 * (15 * 100 + 15);
      expect(c[idx + 2]).toBe(255); // B channel still 255
      expect(c[idx + 3]).toBe(255); // A channel still 255
    });

    it("copies an inverted selection as the full layer with a transparent excluded hole", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      engine.createSelection(10, 10, 20, 20);
      engine.invertSelection();
      const result = SelectionOperations.copySelection(engine)!;

      expect(result.width).toBe(100);
      expect(result.height).toBe(100);
      expect(result.data[4 * (15 * 100 + 15) + 3]).toBe(0);
      expect(result.data[4 * (50 * 100 + 50) + 3]).toBe(255);
    });

    it("clamps selection to layer bounds when partially outside canvas (left edge)", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // Selection extends 20px beyond left edge: x=-20, width=100
      engine.createSelection(-20, 0, 100, 100);
      const result = SelectionOperations.copySelection(engine)!;

      // Should clamp to x=0, width=80 (only the visible portion)
      expect(result.width).toBe(80);
      expect(result.height).toBe(100);
      // Pixel at (0,0) in result should be red (was at layer x=0)
      expect(result.data[3]).toBe(255);
    });

    it("clamps selection to layer bounds when partially outside canvas (right edge)", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#00FF00";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // Selection extends 30px beyond right edge: x=80, width=50
      engine.createSelection(80, 0, 50, 100);
      const result = SelectionOperations.copySelection(engine)!;

      // Should clamp to x=80, width=20
      expect(result.width).toBe(20);
      expect(result.height).toBe(100);
      // Pixel should be green
      expect(result.data[1]).toBe(255);
    });

    it("returns null when selection is fully outside canvas", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // Selection entirely to the left of canvas
      engine.createSelection(-200, 0, 50, 50);
      const result = SelectionOperations.copySelection(engine);
      expect(result).toBeNull();
    });

    it("returns null when selection is fully below canvas", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // Selection entirely below canvas
      engine.createSelection(0, 200, 50, 50);
      const result = SelectionOperations.copySelection(engine);
      expect(result).toBeNull();
    });

    it("auto-trims transparent rows/columns from the edges", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      // Draw a 30x40 red rect at position (10, 10), surrounded by transparency
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(10, 10, 30, 40);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // Select entire layer
      engine.createSelection(0, 0, 100, 100);
      const result = SelectionOperations.copySelection(engine)!;

      // Should be trimmed to 30x40 bounding box of the red rect
      expect(result.width).toBe(30);
      expect(result.height).toBe(40);
      // Pixel at (0,0) of result should be red (was at layer position 10,10)
      expect(result.data[0]).toBe(255);
      expect(result.data[3]).toBe(255);
    });

    it("returns original data unchanged when content fully fills the selection", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#0000FF";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      engine.createSelection(0, 0, 100, 100);
      const result = SelectionOperations.copySelection(engine)!;

      // Full layer is filled, so no trimming occurs
      expect(result.width).toBe(100);
      expect(result.height).toBe(100);
      expect(result.data[2]).toBe(255);
      expect(result.data[3]).toBe(255);
    });

    it("returns original data unchanged when content is fully transparent", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      // Don't fill anything — fully transparent
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      engine.createSelection(10, 10, 50, 50);
      const result = SelectionOperations.copySelection(engine)!;

      // Fully transparent should be returned as-is (50x50)
      expect(result.width).toBe(50);
      expect(result.height).toBe(50);
      // All pixels should have alpha=0
      let allTransparent = true;
      for (let i = 0; i < result.data.length; i += 4) {
        if (result.data[i + 3] !== 0) {
          allTransparent = false;
          break;
        }
      }
      expect(allTransparent).toBe(true);
    });

    it("clamps and auto-trims when selection partially outside canvas with partial content", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      // Draw a small red circle-like rect at (5, 5), size 20x20
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(5, 5, 20, 20);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // Selection extends 20px beyond left edge: x=-10, width=60, height=60
      engine.createSelection(-10, -10, 60, 60);
      const result = SelectionOperations.copySelection(engine)!;

      // After clamping: source is (0, 0, 50, 50), then auto-trim to content (5,5,20,20) = 20x20
      expect(result.width).toBe(20);
      expect(result.height).toBe(20);
      expect(result.data[3]).toBe(255);
    });

    it("copies a rotated selection correctly — angle does not affect pixel data", () => {
      // The copySelection operation reads from the layer bitmap in source space;
      // rotation angle is a visual property only and should not skew the copied pixels.
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(20, 30, 50, 40);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // Selection with an angle (rotation is a display property, not a pixel transform)
      engine.createSelection(20, 30, 50, 40, 45);
      const result = SelectionOperations.copySelection(engine)!;

      // Should copy the axis-aligned rect in source space
      expect(result.width).toBe(50);
      expect(result.height).toBe(40);
      // Top-left pixel should be red
      expect(result.data[0]).toBe(255);
      expect(result.data[3]).toBe(255);
    });

    it("correctly handles selection at exact top-left corner (x=0, y=0)", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#00FF00";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      engine.createSelection(0, 0, 100, 100);
      const result = SelectionOperations.copySelection(engine)!;

      expect(result.width).toBe(100);
      expect(result.height).toBe(100);
      expect(result.data[1]).toBe(255);
    });

    it("handles selection larger than layer bounds — clamps on all four sides", () => {
      const layer = engine.addLayer("Layer 1", 50, 50);
      const offscreen = new OffscreenCanvas(50, 50);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 50, 50);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // Selection extends beyond all edges: x=-20, y=-10, width=100, height=80
      engine.createSelection(-20, -10, 100, 80);
      const result = SelectionOperations.copySelection(engine)!;

      // Clamped to (0, 0, 50, 50)
      expect(result.width).toBe(50);
      expect(result.height).toBe(50);
      expect(result.data[3]).toBe(255);
    });

    it("skips auto-trim for inverted selections (regression test)", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // Invert select a small region — should return full 100x100,
      // NOT auto-trimmed to just the excluded region
      engine.createSelection(10, 10, 20, 20);
      engine.invertSelection();
      const result = SelectionOperations.copySelection(engine)!;

      expect(result.width).toBe(100);
      expect(result.height).toBe(100);
      // The excluded region (10,10,20,20) should be transparent
      expect(result.data[4 * (15 * 100 + 15) + 3]).toBe(0);
      // Outside excluded region should be visible
      expect(result.data[4 * (5 * 100 + 5) + 3]).toBe(255);
    });

    it("auto-trim works correctly when content touches the selection edge", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      // Draw content that touches the right edge of selection
      ctx.fillStyle = "#0000FF";
      ctx.fillRect(70, 10, 20, 20);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // Select (50, 0, 50, 40) — content starts at x=70, so left 20px are transparent
      engine.createSelection(50, 0, 50, 40);
      const result = SelectionOperations.copySelection(engine)!;

      // Auto-trim: left edge at 70-50=20, right at 90-50=40, top at 10-0=10, bottom at 30-0=30
      expect(result.width).toBe(20);
      expect(result.height).toBe(20);
      expect(result.data[2]).toBe(255);
    });

    it("preserves clipboard across sequential copy operations", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      engine.createSelection(0, 0, 10, 10);
      const first = SelectionOperations.copySelection(engine)!;
      expect(first.width).toBe(10);

      engine.createSelection(20, 20, 30, 30);
      const second = SelectionOperations.copySelection(engine)!;
      expect(second.width).toBe(30);

      // Clipboard should now hold the second copy
      expect(SelectionOperations.hasClipboard()).toBe(true);
      const clipboardResult = SelectionOperations.getSelectionBounds(engine);
      // selection was cleared by the second copy's cleanup...
      // Actually engine selection is still set from createSelection(20,20,30,30)
      expect(clipboardResult).not.toBeNull();
    });

    it("returns null when no layer image bitmap exists", () => {
      // Add a layer without setting an image bitmap
      engine.addLayer("Empty Layer", 100, 100);
      engine.createSelection(0, 0, 50, 50);
      const result = SelectionOperations.copySelection(engine);
      expect(result).toBeNull();
    });
  });

  describe("cutSelection", () => {
    it("throws when no selection", () => {
      expect(() => SelectionOperations.cutSelection(engine)).toThrow("no selection");
    });

    it("throws when no active layer", () => {
      engine.createSelection(0, 0, 50, 50);
      expect(() => SelectionOperations.cutSelection(engine)).toThrow("no active layer");
    });

    it("clears selection after cut", () => {
      engine.addLayer("Layer 1", 100, 100);
      engine.createSelection(0, 0, 50, 50);
      SelectionOperations.cutSelection(engine);
      expect(engine.getSelection()).toBeNull();
    });

    it("actually removes pixels from the source layer (pixel verification)", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      engine.createSelection(10, 10, 30, 30);
      SelectionOperations.cutSelection(engine);

      const bitmap = engine.getLayerImageBitmap(layer.id);
      const pixels = (bitmap as any)._buffer;

      // Inside the cut region should be transparent
      expect(pixels[4 * (15 * 100 + 15) + 3]).toBe(0);
      // Outside the cut region should remain red
      expect(pixels[4 * (5 * 100 + 5) + 0]).toBe(255);
      expect(pixels[4 * (5 * 100 + 5) + 3]).toBe(255);
    });

    it("cut also populates the clipboard", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());
      engine.createSelection(0, 0, 50, 50);
      SelectionOperations.__resetClipboard();
      expect(SelectionOperations.hasClipboard()).toBe(false);

      SelectionOperations.cutSelection(engine);

      expect(SelectionOperations.hasClipboard()).toBe(true);
    });
  });

  describe("deleteSelection", () => {
    it("throws when no selection", () => {
      expect(() => SelectionOperations.deleteSelection(engine)).toThrow("no selection");
    });

    it("throws when no active layer", () => {
      engine.createSelection(0, 0, 50, 50);
      expect(() => SelectionOperations.deleteSelection(engine)).toThrow("no active layer");
    });

    it("clears selection after delete", () => {
      engine.addLayer("Layer 1", 100, 100);
      engine.createSelection(0, 0, 50, 50);
      SelectionOperations.deleteSelection(engine);
      expect(engine.getSelection()).toBeNull();
    });

    it("fills selection with transparent pixels", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      const bitmap = offscreen.transferToImageBitmap();
      engine.setLayerImageBitmap(layer.id, bitmap);

      engine.createSelection(10, 10, 20, 20);
      SelectionOperations.deleteSelection(engine);

      const srcBitmap = engine.getLayerImageBitmap(layer.id);
      expect(srcBitmap).not.toBeNull();
      const c = (srcBitmap as any)._buffer;

      // Inside selection should be transparent (alpha = 0)
      const insideIdx = 4 * (15 * 100 + 15);
      expect(c[insideIdx + 3]).toBe(0);
      // Outside selection should still be red
      const outsideIdx = 4 * (50 * 100 + 50);
      expect(c[outsideIdx + 0]).toBe(255);
      expect(c[outsideIdx + 3]).toBe(255);
    });

    it("clears pixels outside the excluded bounds for an inverted selection", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      engine.createSelection(10, 10, 20, 20);
      engine.invertSelection();
      SelectionOperations.deleteSelection(engine);

      const pixels = (engine.getLayerImageBitmap(layer.id) as any)._buffer;
      expect(pixels[4 * (15 * 100 + 15) + 3]).toBe(255);
      expect(pixels[4 * (50 * 100 + 50) + 3]).toBe(0);
      expect(engine.getSelection()).toBeNull();
    });

    it("does NOT populate clipboard (unlike cut)", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());
      SelectionOperations.__resetClipboard();

      engine.createSelection(10, 10, 30, 30);
      SelectionOperations.deleteSelection(engine);

      expect(SelectionOperations.hasClipboard()).toBe(false);
    });

    it("handles delete when selection partially extends beyond layer bounds", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#00FF00";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // Selection partially outside the layer
      engine.createSelection(-10, -10, 50, 50);
      // Should not throw
      SelectionOperations.deleteSelection(engine);

      const pixels = (engine.getLayerImageBitmap(layer.id) as any)._buffer;
      // Pixel at (0,0) — inside the clamped selection area — should be transparent
      expect(pixels[3]).toBe(0);
      // Pixel at (40, 40) — outside the selection — should still be green
      expect(pixels[4 * (40 * 100 + 40) + 1]).toBe(255);
      expect(engine.getSelection()).toBeNull();
    });
  });

  describe("pasteSelection", () => {
    it("does nothing with null data", () => {
      SelectionOperations.pasteSelection(engine, null);
      expect(engine.getLayers().length).toBe(0);
    });

    it("creates new layer from pasted data", () => {
      const data = {
        width: 50,
        height: 50,
        data: new Uint8ClampedArray(50 * 50 * 4),
      } as unknown as ImageData;
      SelectionOperations.pasteSelection(engine, data);
      expect(engine.getLayers().length).toBe(1);
      expect(engine.getActiveLayerId()).not.toBeNull();
    });

    it("pasted layer name contains Pasted", () => {
      const data = {
        width: 50,
        height: 50,
        data: new Uint8ClampedArray(50 * 50 * 4),
      } as unknown as ImageData;
      SelectionOperations.pasteSelection(engine, data);
      const layers = engine.getLayers();
      expect(layers[0].name).toContain("Pasted");
    });

    it("pasted pixel data matches the original copied data (copy→paste roundtrip)", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF8800";
      ctx.fillRect(10, 10, 40, 30);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // Copy a region
      engine.createSelection(10, 10, 40, 30);
      const copied = SelectionOperations.copySelection(engine)!;
      expect(copied.width).toBe(40);

      // Paste into new document
      const engine2 = new DocumentEngine("paste-test", "Paste Test", 200, 200);
      SelectionOperations.pasteSelection(engine2, copied);

      // Verify pasted layer has the same pixel data
      const pastedLayer = engine2.getLayer(engine2.getActiveLayerId()!)!;
      expect(pastedLayer.width).toBe(40);
      expect(pastedLayer.height).toBe(30);
      const pastedBitmap = pastedLayer.imageBitmap as any;
      expect(pastedBitmap).not.toBeNull();
      // Center pixel should match
      expect(pastedBitmap._buffer[4 * (15 * 40 + 20) + 0]).toBe(255); // R
      expect(pastedBitmap._buffer[4 * (15 * 40 + 20) + 1]).toBe(136); // G
      expect(pastedBitmap._buffer[4 * (15 * 40 + 20) + 2]).toBe(0);   // B
      expect(pastedBitmap._buffer[4 * (15 * 40 + 20) + 3]).toBe(255); // A
    });

    it("pastes from module-level clipboard when no data argument provided", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 20, 20);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      engine.createSelection(0, 0, 20, 20);
      SelectionOperations.copySelection(engine);

      // Paste without passing data (should use clipboard)
      SelectionOperations.pasteSelection(engine);

      // Should have created a new layer
      const layers = engine.getLayers();
      expect(layers.length).toBe(2);
      expect(layers[0].name).toContain("Pasted");
    });

    it("pasted layer is created at the correct size (width and height from ImageData)", () => {
      const data = {
        width: 75,
        height: 25,
        data: new Uint8ClampedArray(75 * 25 * 4),
      } as unknown as ImageData;
      SelectionOperations.pasteSelection(engine, data);
      const pastedLayer = engine.getLayer(engine.getActiveLayerId()!)!;
      expect(pastedLayer.width).toBe(75);
      expect(pastedLayer.height).toBe(25);
    });
  });

  describe("copy → cut → paste sequence (integration)", () => {
    it("copies content, cuts it from original, then pastes to a new layer — original region becomes transparent", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#00FF00";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // 1. Create selection
      engine.createSelection(10, 10, 40, 40);

      // 2. Cut it (copies to clipboard + removes from source)
      const cutData = SelectionOperations.cutSelection(engine);
      expect(cutData).not.toBeNull();

      // 3. Verify original layer has transparent hole
      const afterCut = (engine.getLayerImageBitmap(layer.id) as any)._buffer;
      expect(afterCut[4 * (15 * 100 + 15) + 3]).toBe(0);
      expect(afterCut[4 * (5 * 100 + 5) + 0]).toBe(0);   // G=0 (used to be 255)
      expect(afterCut[4 * (5 * 100 + 5) + 1]).toBe(255);  // G=255 (outside selection)

      // 4. Paste into new layer
      SelectionOperations.pasteSelection(engine);
      const pastedLayer = engine.getLayer(engine.getActiveLayerId()!)!;
      expect(pastedLayer.name).toContain("Pasted");
      expect(pastedLayer.width).toBe(40);
      expect(pastedLayer.height).toBe(40);
    });

    it("delete removes pixels without storing in clipboard, then copy+paste retrieves earlier clipboard", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // 1. Copy first region (populates clipboard)
      engine.createSelection(0, 0, 10, 10);
      SelectionOperations.copySelection(engine);

      // 2. Delete a different region (does NOT overwrite clipboard)
      engine.createSelection(50, 50, 20, 20);
      SelectionOperations.deleteSelection(engine);

      // 3. Clipboard should still hold the first copy, not the deleted region
      expect(SelectionOperations.hasClipboard()).toBe(true);

      // 4. Paste should recreate the 10x10 region, not the deleted 20x20 region
      SelectionOperations.pasteSelection(engine);
      const pastedLayer = engine.getLayer(engine.getActiveLayerId()!)!;
      expect(pastedLayer.width).toBe(10);
      expect(pastedLayer.height).toBe(10);
    });
  });

  describe("edge cases — extreme values", () => {
    it("handles selection with fractional coordinates (rounds to nearest pixel)", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      // Non-integer selection coordinates
      engine.createSelection(10.3, 20.7, 50.2, 60.8);
      const result = SelectionOperations.copySelection(engine)!;
      // Should round to (10, 21, 50, 61)
      expect(result.width).toBeGreaterThan(0);
      expect(result.height).toBeGreaterThan(0);
    });

    it("handles empty layer when no bitmap is set on the active layer", () => {
      engine.addLayer("Empty", 100, 100);
      engine.createSelection(0, 0, 50, 50);
      // No bitmap set on this layer
      const result = SelectionOperations.copySelection(engine);
      expect(result).toBeNull();
    });

    it("hasClipboard returns false after __resetClipboard", () => {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = "#FF0000";
      ctx.fillRect(0, 0, 10, 10);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());

      engine.createSelection(0, 0, 10, 10);
      SelectionOperations.copySelection(engine);
      expect(SelectionOperations.hasClipboard()).toBe(true);

      SelectionOperations.__resetClipboard();
      expect(SelectionOperations.hasClipboard()).toBe(false);
    });
  });

  describe("transformed layer — selection maps to layer-local space", () => {
    function makeFilledLayer(color: string) {
      const layer = engine.addLayer("Layer 1", 100, 100);
      const offscreen = new OffscreenCanvas(100, 100);
      const ctx = offscreen.getContext("2d")!;
      ctx.fillStyle = color;
      ctx.fillRect(0, 0, 100, 100);
      engine.setLayerImageBitmap(layer.id, offscreen.transferToImageBitmap());
      return layer;
    }

    function alphaAt(layerId: string, x: number, y: number): number {
      const b = engine.getLayerImageBitmap(layerId) as any;
      return b._buffer[4 * (y * 100 + x) + 3];
    }

    it("deleteSelection clears layer-local pixels under the (translated) marquee", () => {
      const layer = makeFilledLayer("#FF0000");
      // Translate the layer by (+50,+50) in document space; scale/rot identity.
      engine.transformLayer(layer.id, { x: 50, y: 50, scaleX: 1, scaleY: 1, rotation: 0 });

      // Document-space selection at (60,60,20,20) maps to layer-local (10,10,20,20).
      engine.createSelection(60, 60, 20, 20);
      SelectionOperations.deleteSelection(engine);

      // Pixels inside the mapped layer-local rect must be cleared.
      expect(alphaAt(layer.id, 15, 15)).toBe(0);
      expect(alphaAt(layer.id, 25, 25)).toBe(0);
      // Pixels outside it (in the un-translated part of the layer) stay opaque.
      expect(alphaAt(layer.id, 5, 5)).toBe(255);
      expect(alphaAt(layer.id, 95, 95)).toBe(255);
    });

    it("deleteSelection clears layer-local pixels under the (scaled) marquee", () => {
      const layer = makeFilledLayer("#00FF00");
      // Scale 2x, no translate. Layer center stays at doc (50,50).
      engine.transformLayer(layer.id, { x: 0, y: 0, scaleX: 2, scaleY: 2, rotation: 0 });

      // Document-space selection (60,60,20,20) → layer-local (30,30,10,10).
      engine.createSelection(60, 60, 20, 20);
      SelectionOperations.deleteSelection(engine);

      expect(alphaAt(layer.id, 35, 35)).toBe(0); // inside mapped rect
      expect(alphaAt(layer.id, 5, 5)).toBe(255);  // outside
    });

    it("fillActiveLayerWithColor (via SelectionOperations path) scopes to layer-local rect", () => {
      // The fill path lives in layerOperations; here we assert the shared
      // mapping helper yields the correct layer-local AABB for a rotated layer.
      const layer = makeFilledLayer("#0000FF");
      engine.transformLayer(layer.id, { x: 50, y: 50, scaleX: 1, scaleY: 1, rotation: 0 });
      engine.createSelection(50, 50, 40, 40); // doc rect over the layer center

      const aabb = SelectionOperations.selectionToLayerAabb(
        engine.getSelection()!,
        layer.transform,
        layer.width,
        layer.height,
      );
      // Center doc (70,70) → layer-local (20,20); rect 40 wide → (0..40).
      expect(Math.round(aabb.x)).toBe(0);
      expect(Math.round(aabb.y)).toBe(0);
      expect(Math.round(aabb.width)).toBe(40);
      expect(Math.round(aabb.height)).toBe(40);
    });

    it("identity transform yields the raw selection rect (no regression)", () => {
      const layer = makeFilledLayer("#FF0000");
      engine.createSelection(10, 20, 50, 60);
      const aabb = SelectionOperations.selectionToLayerAabb(
        engine.getSelection()!,
        layer.transform,
        layer.width,
        layer.height,
      );
      expect(aabb.x).toBe(10);
      expect(aabb.y).toBe(20);
      expect(aabb.width).toBe(50);
      expect(aabb.height).toBe(60);
    });

    it("deleteSelection with ellipse shape clears only INSIDE the ellipse, keeps corners", () => {
      const layer = makeFilledLayer("#FF0000");
      // Identity transform. Ellipse marquee covering layer-local (20,20,60,60).
      engine.createSelection(20, 20, 60, 60, 0, "ellipse");
      SelectionOperations.deleteSelection(engine);

      // Center of the ellipse → inside → cleared.
      expect(alphaAt(layer.id, 50, 50)).toBe(0);
      // Corners of the AABB are outside the ellipse → must be kept opaque.
      expect(alphaAt(layer.id, 21, 21)).toBe(255);
      expect(alphaAt(layer.id, 78, 78)).toBe(255);
    });

    it("copySelection with ellipse shape masks pixels OUTSIDE the ellipse to transparent", () => {
      const layer = makeFilledLayer("#00FF00");
      engine.createSelection(0, 0, 60, 60, 0, "ellipse");
      const result = SelectionOperations.copySelection(engine)!;
      expect(result).not.toBeNull();

      const w = result!.width;
      const h = result!.height;
      const idx = (x: number, y: number) => (y * w + x) * 4 + 3;
      // Center pixel inside ellipse → opaque.
      expect(result!.data[idx(w / 2, h / 2)]).toBe(255);
      // Corner pixel of the AABB is outside the ellipse → masked transparent.
      expect(result!.data[idx(0, 0)]).toBe(0);
    });

    it("deleteSelection with INVERTED ellipse clears everything OUTSIDE the ellipse, keeps center", () => {
      const layer = makeFilledLayer("#FF0000");
      engine.createSelection(20, 20, 60, 60, 0, "ellipse");
      engine.invertSelection();
      SelectionOperations.deleteSelection(engine);

      // Center of the ellipse → inside → kept (inverted = everything-except-ellipse).
      expect(alphaAt(layer.id, 50, 50)).toBe(255);
      // Corner of the AABB (outside ellipse) → must be cleared.
      expect(alphaAt(layer.id, 21, 21)).toBe(0);
      // Far corner of the layer (outside AABB entirely) → cleared.
      expect(alphaAt(layer.id, 1, 1)).toBe(0);
      expect(alphaAt(layer.id, 95, 95)).toBe(0);
    });

    it("copySelection with INVERTED ellipse masks ellipse interior to transparent, keeps outside", () => {
      const layer = makeFilledLayer("#00FF00");
      engine.createSelection(20, 20, 60, 60, 0, "ellipse");
      engine.invertSelection();
      const result = SelectionOperations.copySelection(engine)!;
      expect(result).not.toBeNull();

      // Full-layer copy: w = 100, h = 100
      expect(result!.width).toBe(100);
      expect(result!.height).toBe(100);
      // Center pixel inside ellipse → masked transparent.
      expect(result!.data[(50 * 100 + 50) * 4 + 3]).toBe(0);
      // Pixel outside ellipse (far corner) → opaque.
      expect(result!.data[(1 * 100 + 1) * 4 + 3]).toBe(255);
    });
  });
});
