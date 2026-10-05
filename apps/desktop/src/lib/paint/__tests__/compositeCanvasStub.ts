// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * A CPU 2D context faithful enough to composite layers, for tests that measure
 * PIXEL OWNERSHIP rather than canvas behaviour.
 *
 * WHY IT IS SHARED. Every test that asks "do the TS projection and the Rust pixel
 * store agree about this composite?" has to stand up a canvas, because
 * `compositeAllLayers` / `compositeTwoLayers` (engine/layerComposite.ts) build
 * their result on `OffscreenCanvas` + `transferToImageBitmap`. Keeping a private
 * copy per file put a 130-line stub in each one and pushed the stamp ownership
 * file toward the 1000-line guard.
 *
 * WHAT IT MODELS: translation, alpha (`globalAlpha` with source-over), the
 * 5-argument `drawImage(bitmap, dx, dy, dw, dh)` that `drawLayerToContext` uses,
 * `getImageData`, and `transferToImageBitmap`.
 *
 * WHAT IT REFUSES TO MODEL, LOUDLY. A non-zero rotation, a non-unit scale, any
 * blend other than `source-over`, path `fill`/`fillRect`, and `putImageData` all
 * THROW instead of quietly producing a wrong image. That is the point: a stub
 * that silently mis-composites becomes the reason a convergence verdict reads a
 * value it should not, and the failure then looks like a product defect. Layers
 * in these tests carry rotation 0, unit scale and the `normal` blend, so the
 * throws never fire in normal use - they exist so the stub cannot become
 * accidentally load-bearing.
 */
export interface SourceRaster {
  width: number;
  height: number;
  data?: ArrayLike<number>;
}

export class CompositeCanvasStub {
  width: number;
  height: number;
  private buffer: Uint8ClampedArray;
  private tx = 0;
  private ty = 0;
  private stack: Array<{ tx: number; ty: number; alpha: number }> = [];
  private alpha = 1;
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
    let op = "source-over";
    const ctx = {
      globalCompositeOperation: "source-over" as string,
      get globalAlpha() { return self.alpha; },
      set globalAlpha(v: number) { self.alpha = v; },
      save() { self.stack.push({ tx: self.tx, ty: self.ty, alpha: self.alpha }); },
      restore() {
        const s = self.stack.pop();
        if (s) { self.tx = s.tx; self.ty = s.ty; self.alpha = s.alpha; }
      },
      translate(x: number, y: number) { self.tx += x; self.ty += y; },
      rotate(angle: number) {
        if (angle !== 0) throw new Error(`stub does not model rotation (${angle}); a rotated layer would mis-composite`);
      },
      scale(x: number, y: number) {
        if (x !== 1 || y !== 1) throw new Error(`stub does not model scaling (${x},${y}); a scaled layer would mis-composite`);
      },
      setTransform() {},
      beginPath() {}, closePath() {}, moveTo() {}, lineTo() {}, rect() {}, clip() {},
      fill() { throw new Error("stub does not model path fill"); },
      stroke() {},
      fillRect() { throw new Error("stub does not model fillRect"); },
      clearRect() { self.buffer.fill(0); },
      drawImage(src: SourceRaster, ...rest: number[]) {
        if (op !== "source-over") throw new Error(`stub only models source-over, got ${op}`);
        const sw = src.width;
        const sh = src.height;
        const srcData = src.data ?? [];
        // The 5-arg form `drawImage(bitmap, dx, dy, dw, dh)` is the one
        // `drawLayerToContext` uses; nearest-neighbour into the scaled box.
        const [dx, dy, dw, dh] = rest.length >= 4
          ? rest
          : [0, 0, sw, sh];
        for (let row = 0; row < dh; row++) {
          for (let col = 0; col < dw; col++) {
            const srcX = Math.min(sw - 1, Math.floor((col * sw) / dw));
            const srcY = Math.min(sh - 1, Math.floor((row * sh) / dh));
            if (srcX < 0 || srcY < 0) continue;
            const si = (srcY * sw + srcX) * 4;
            self.blend(
              Math.round(self.tx + dx + col), Math.round(self.ty + dy + row),
              srcData[si], srcData[si + 1], srcData[si + 2], srcData[si + 3],
            );
          }
        }
      },
      getImageData(x = 0, y = 0, w = self.width, h = self.height) {
        const out = new Uint8ClampedArray(w * h * 4);
        for (let row = 0; row < h; row++) {
          for (let col = 0; col < w; col++) {
            const si = ((y + row) * self.width + (x + col)) * 4;
            if (si < 0 || si + 3 >= self.buffer.length) continue;
            const di = (row * w + col) * 4;
            out[di] = self.buffer[si];
            out[di + 1] = self.buffer[si + 1];
            out[di + 2] = self.buffer[si + 2];
            out[di + 3] = self.buffer[si + 3];
          }
        }
        // `w`/`h` are the caller's numbers, so they are numbers here; the cast is
        // only to satisfy the DOM `ImageData` signature, which also carries
        // `colorSpace` as a union this stub does not narrow.
        return { data: out, width: w, height: h, colorSpace: "srgb" } as unknown as ImageData;
      },
      putImageData() { throw new Error("stub does not model putImageData"); },
      measureText: () => ({ width: 1 }),
      fillText() {}, strokeText() {},
      font: "", textBaseline: "alphabetic", letterSpacing: "0px", fillStyle: "#000000",
    };
    Object.defineProperty(ctx, "globalCompositeOperation", {
      get: () => op,
      set: (v: string) => { op = v; },
    });
    return ctx as unknown as CanvasRenderingContext2D;
  }

  /** Source-over with `globalAlpha`, the one blend the measured composite uses. */
  private blend(x: number, y: number, r: number, g: number, b: number, a: number): void {
    if (x < 0 || y < 0 || x >= this.width || y >= this.height) return;
    const i = (y * this.width + x) * 4;
    const sa = (a / 255) * this.alpha;
    if (sa <= 0) return;
    const da = this.buffer[i + 3] / 255;
    const outA = sa + da * (1 - sa);
    if (outA <= 0) { this.buffer[i] = this.buffer[i + 1] = this.buffer[i + 2] = this.buffer[i + 3] = 0; return; }
    this.buffer[i] = Math.round((r * sa + this.buffer[i] * da * (1 - sa)) / outA);
    this.buffer[i + 1] = Math.round((g * sa + this.buffer[i + 1] * da * (1 - sa)) / outA);
    this.buffer[i + 2] = Math.round((b * sa + this.buffer[i + 2] * da * (1 - sa)) / outA);
    this.buffer[i + 3] = Math.round(outA * 255);
  }

  transferToImageBitmap(): ImageBitmap {
    if (!this.contextObtained) {
      const err = new Error("Cannot transfer an ImageBitmap from an OffscreenCanvas with no context");
      err.name = "InvalidStateError";
      throw err;
    }
    const data = this.buffer;
    const width = this.width;
    const height = this.height;
    return {
      width,
      height,
      data,
      close: () => {},
      getImageData: () => ({ data: new Uint8ClampedArray(data), width, height, colorSpace: "srgb" }),
    } as unknown as ImageBitmap;
  }
}

/** An ImageBitmap carrying real, non-uniform RGBA bytes. */
export function stubRasterBitmap(w: number, h: number, pixels: Uint8ClampedArray): ImageBitmap {
  const canvas = new CompositeCanvasStub(w, h);
  // The DOM `drawImage` signature takes `CanvasImageSource`, which a plain
  // `{width, height, data}` object is not; the stub reads the structural
  // `SourceRaster` shape, so the context is re-typed to what it actually accepts.
  const ctx = canvas.getContext("2d") as unknown as {
    drawImage: (src: SourceRaster, dx: number, dy: number, dw: number, dh: number) => void;
  };
  ctx.drawImage({ width: w, height: h, data: pixels }, 0, 0, w, h);
  return canvas.transferToImageBitmap();
}

/**
 * A non-uniform raster, so a composite that composited nothing (or composited
 * the wrong layer) cannot equal a real one. The `null - null = 0` trap this
 * guards is called out in `ownerConvergence.test.ts`: an all-zero buffer looks
 * exactly like real pixels, and a verdict drawn from one is a false negative
 * rather than a measurement. `seed` varies the pattern so two layers cannot
 * produce the same bytes.
 */
export function stubGradientRaster(w: number, h: number, seed = 0): Uint8ClampedArray {
  const buf = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      buf[i] = (x * 31 + seed * 7) & 0xff;
      buf[i + 1] = (y * 17 + seed * 11) & 0xff;
      buf[i + 2] = ((x + y) * 7 + seed * 23) & 0xff;
      buf[i + 3] = 255;
    }
  }
  return buf;
}

/**
 * Read a layer's raster back through this stub, as the production
 * `readbackBitmap` path does. Returns the exact bytes plus a non-uniformity
 * count, so a caller can refuse a blank read BEFORE comparing anything - the
 * false-negative guard, not a convenience.
 */
export function readStubProjection(
  bitmap: ImageBitmap,
  width: number,
  height: number,
): { bytes: Uint8ClampedArray; distinct: number } {
  const canvas = new CompositeCanvasStub(width, height);
  // `drawImage` on the DOM context is typed for `CanvasImageSource`, which an
  // `ImageBitmap` satisfies but this stub's own objects do not; the structural
  // `SourceRaster` is what the stub actually reads.
  const ctx = canvas.getContext("2d") as unknown as {
    drawImage: (src: SourceRaster, dx: number, dy: number, dw: number, dh: number) => void;
    getImageData: (x: number, y: number, w: number, h: number) => ImageData;
  };
  ctx.drawImage(bitmap as unknown as SourceRaster, 0, 0, width, height);
  const bytes = ctx.getImageData(0, 0, width, height).data;
  const expected = width * height * 4;
  if (bytes.length !== expected) {
    throw new Error(`projection read is ${bytes.length} bytes, expected ${expected}`);
  }
  if (expected === 0) throw new Error("projection read is empty; refusing to compare");
  return { bytes, distinct: new Set(Array.from(bytes.subarray(0, 64))).size };
}

/**
 * Install this stub as the global `OffscreenCanvas`, returning a restore
 * function. Production composites through the global, so a test that measures a
 * composite's bytes has to replace it.
 */
export function installCompositeCanvas(): () => void {
  const g = globalThis as Record<string, unknown>;
  const prior = g.OffscreenCanvas;
  g.OffscreenCanvas = CompositeCanvasStub;
  return () => {
    if (prior === undefined) delete g.OffscreenCanvas;
    else g.OffscreenCanvas = prior;
  };
}
