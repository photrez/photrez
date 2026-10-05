// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * A CPU 2D context that can actually CROP, for tests that compare the TypeScript
 * projection against the Rust pixel store.
 *
 * WHY A CROP-HONEST STUB IS REQUIRED. The obvious stubs copy a source raster from
 * its ORIGIN and require the byte lengths to match, so neither can perform a crop.
 * A convergence test over a crop that never crops compares two copies of the same
 * wrong image and passes for the wrong reason. This one implements the
 * 9-argument `drawImage(src, sx, sy, sw, sh, dx, dy, dw, dh)` form with
 * nearest-neighbour sampling, which is what makes the post-crop comparison carry
 * information.
 *
 * TWO TRAPS THIS FILE EXISTS TO AVOID, both of which make a green suite mean
 * nothing:
 *
 *   1. A BLANK RASTER LOOKS LIKE REAL PIXELS. An all-zero buffer compares equal to
 *   any other all-zero buffer, so an op that rasterised nothing would "converge".
 *   `fill()` therefore paints the whole surface rather than being a no-op - the
 *   parametric rasterizers (shape, text) build a path this stub does not track, and
 *   a no-op `fill()` would hand them a fully TRANSPARENT raster that then matches
 *   every other empty raster. `measureText` / `fillText` are likewise given
 *   deterministic, never-zero metrics so a measured text box cannot collapse.
 *
 *   2. A READ THAT RETURNS THE WRONG LENGTH COMPARES EQUAL TO ANOTHER WRONG
 *   ONE. Callers must still check the byte count themselves; this stub does not
 *   paper over a mismatch.
 *
 * SCOPE. Unlike `compositeCanvasStub` (the sibling stub for COMPOSITE ownership
 * tests, which refuses rotation and non-`source-over` blends outright), this one
 * accepts any transform and any `globalCompositeOperation` string, because the
 * ops measured with it - crop, delete pixels, composite - legitimately use them.
 * It is the permissive stub; that one is the strict one.
 */
export class CroppingCanvasStub {
  width: number;
  height: number;
  private buffer: Uint8ClampedArray;
  private contextObtained = false;

  constructor(w: number, h: number) {
    this.width = w;
    this.height = h;
    this.buffer = new Uint8ClampedArray(Math.max(0, w) * Math.max(0, h) * 4);
  }

  getContext(type: string): CanvasRenderingContext2D {
    if (type !== "2d") return null as unknown as CanvasRenderingContext2D;
    this.contextObtained = true;
    const self = this;
    let fill = "#000000";
    let alpha = 1;
    const ctx = {
      globalAlpha: 1,
      globalCompositeOperation: "source-over",
      imageSmoothingEnabled: true,
      get fillStyle() { return fill; },
      set fillStyle(v: string) { fill = v; },
      save() {},
      restore() {},
      translate() {},
      rotate() {},
      scale() {},
      beginPath() {},
      closePath() {},
      moveTo() {},
      lineTo() {},
      // TRAP 1: painting the whole surface, not the path. See the file header.
      fill() { ctx.fillRect(0, 0, self.width, self.height); },
      stroke() {},
      rect() {},
      clip() {},
      setTransform() {},
      font: "",
      textBaseline: "alphabetic",
      letterSpacing: "0px",
      // Deterministic metrics: width from the string length and the font size, so
      // a measured text box is stable across runs and the rasterized layer has
      // real dimensions. Never zero, so the box cannot collapse to nothing.
      measureText(text: string) {
        const px = parseFloat(String(ctx.font).match(/(\d+(?:\.\d+)?)px/)?.[1] ?? "48");
        const width = Math.max(1, text.length * px * 0.5);
        return {
          width,
          actualBoundingBoxAscent: px * 0.8,
          actualBoundingBoxDescent: px * 0.25,
          fontBoundingBoxAscent: px * 0.8,
          fontBoundingBoxDescent: px * 0.25,
        };
      },
      fillText(text: string, x: number, y: number) {
        const m = ctx.measureText(text);
        ctx.fillRect(
          x,
          y,
          m.width,
          parseFloat(String(ctx.font).match(/(\d+(?:\.\d+)?)px/)?.[1] ?? "48"),
        );
      },
      strokeText() {},
      clearRect(x: number, y: number, w: number, h: number) {
        for (let r = y; r < y + h; r++) {
          for (let c = x; c < x + w; c++) self.setPx(c, r, 0, 0, 0, 0);
        }
      },
      fillRect(x: number, y: number, w: number, h: number) {
        const hex = fill.replace("#", "");
        const r = parseInt(hex.slice(0, 2), 16) || 0;
        const g = parseInt(hex.slice(2, 4), 16) || 0;
        const b = parseInt(hex.slice(4, 6), 16) || 0;
        for (let row = y; row < y + h; row++) {
          for (let col = x; col < x + w; col++) {
            self.setPx(col, row, r, g, b, Math.round(255 * alpha));
          }
        }
      },
      drawImage(src: any, ...rest: number[]) {
        const sw = src.width as number;
        const sh = src.height as number;
        const srcData: ArrayLike<number> = typeof src.getImageData === "function"
          ? src.getImageData().data
          : (src.data as ArrayLike<number>);
        // 3-arg form = whole image at native size; 9-arg form = a crop/scale.
        const [sx, sy, cw, ch, dx, dy, dw, dh] = rest.length >= 6
          ? rest
          : [0, 0, sw, sh, 0, 0, sw, sh];
        for (let row = 0; row < dh; row++) {
          for (let col = 0; col < dw; col++) {
            const srcX = sx + Math.floor((col * cw) / dw);
            const srcY = sy + Math.floor((row * ch) / dh);
            if (srcX < 0 || srcY < 0 || srcX >= sw || srcY >= sh) continue;
            const si = (srcY * sw + srcX) * 4;
            self.setPx(
              dx + col,
              dy + row,
              srcData[si], srcData[si + 1], srcData[si + 2], srcData[si + 3],
            );
          }
        }
      },
      getImageData(x = 0, y = 0, w = self.width, h = self.height) {
        const out = new Uint8ClampedArray(w * h * 4);
        for (let row = 0; row < h; row++) {
          for (let col = 0; col < w; col++) {
            const sx = x + col;
            const sy = y + row;
            if (sx < 0 || sy < 0 || sx >= self.width || sy >= self.height) continue;
            const si = (sy * self.width + sx) * 4;
            const di = (row * w + col) * 4;
            out[di] = self.buffer[si];
            out[di + 1] = self.buffer[si + 1];
            out[di + 2] = self.buffer[si + 2];
            out[di + 3] = self.buffer[si + 3];
          }
        }
        return { data: out, width: w, height: h, colorSpace: "srgb" } as ImageData;
      },
      putImageData(img: any, dx = 0, dy = 0) {
        for (let row = 0; row < img.height; row++) {
          for (let col = 0; col < img.width; col++) {
            const si = (row * img.width + col) * 4;
            const di = ((dy + row) * self.width + (dx + col)) * 4;
            if (di < 0 || di + 3 >= self.buffer.length) continue;
            self.buffer[di] = img.data[si];
            self.buffer[di + 1] = img.data[si + 1];
            self.buffer[di + 2] = img.data[si + 2];
            self.buffer[di + 3] = img.data[si + 3];
          }
        }
      },
    };
    return ctx as unknown as CanvasRenderingContext2D;
  }

  private setPx(x: number, y: number, r: number, g: number, b: number, a: number): void {
    if (x < 0 || y < 0 || x >= this.width || y >= this.height) return;
    const i = (y * this.width + x) * 4;
    this.buffer[i] = r;
    this.buffer[i + 1] = g;
    this.buffer[i + 2] = b;
    this.buffer[i + 3] = a;
  }

  transferToImageBitmap(): ImageBitmap {
    if (!this.contextObtained) {
      const err = new Error("Cannot transfer an ImageBitmap from an OffscreenCanvas with no context");
      err.name = "InvalidStateError";
      throw err;
    }
    const buf = this.buffer;
    const width = this.width;
    const height = this.height;
    return {
      width,
      height,
      close: () => {},
      getImageData: (x = 0, y = 0, w = width, h = height) => {
        const out = new Uint8ClampedArray(w * h * 4);
        for (let row = 0; row < h; row++) {
          for (let col = 0; col < w; col++) {
            const si = ((y + row) * width + (x + col)) * 4;
            const di = (row * w + col) * 4;
            out[di] = buf[si];
            out[di + 1] = buf[si + 1];
            out[di + 2] = buf[si + 2];
            out[di + 3] = buf[si + 3];
          }
        }
        return { data: out, width: w, height: h, colorSpace: "srgb" };
      },
    } as unknown as ImageBitmap;
  }
}

/**
 * Install this stub as the global `OffscreenCanvas`, returning a restore function.
 * The production rasterizers all composite through the global, so a test that
 * measures their output has to replace it.
 */
export function installCroppingCanvas(): () => void {
  const g = globalThis as Record<string, unknown>;
  const prior = g.OffscreenCanvas;
  g.OffscreenCanvas = CroppingCanvasStub;
  return () => {
    if (prior === undefined) delete g.OffscreenCanvas;
    else g.OffscreenCanvas = prior;
  };
}
