// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * A software OffscreenCanvas stand-in. Its pixel maths is the falsifiability
 * foundation for every raster claim in this repo, so every rule below is one a
 * lenient stub gets wrong in a way that HIDES A REGRESSION rather than raising one.
 *
 * When this shim drew nothing, `destination-out` became mathematically inert, four
 * rounds of investigation concluded "the eraser does not change the raster", and a
 * production fix was nearly written on that conclusion. A real-app run then found
 * brush and eraser sound. The failure was invisible because every suite was green.
 *
 * Rules enforced, each with a test that goes RED without it
 * (faithfulOffscreenCanvas.test.ts):
 *
 *  1. `transferToImageBitmap()` throws InvalidStateError when no 2d context was ever
 *     obtained from that canvas. A stub that returns a bitmap from a context-less
 *     canvas lets a first-use crash ship with every suite green. useBrushOverlay.ts
 *     honours the rule by calling getContext("2d") on its scratch canvas first.
 *  2. A draw source is read through its 2d CONTEXT, never by probing the source
 *     object: `OffscreenCanvas` has `getContext` and no `getImageData`, so probing
 *     the object alone finds nothing for every canvas source and the draw silently
 *     paints nothing. The full-canvas rect is passed EXPLICITLY and the result is
 *     accepted only if its byte count matches its own reported dimensions AND its size
 *     is the source's, because a
 *     canvas double with a `(x, y, w, h)` signature and no defaults answers a bare
 *     `getImageData()` with `NaN * NaN * 4` bytes - a zero-length read that, taken at
 *     face value, becomes a bitmap with real dimensions and no pixels, and every draw
 *     of it then spreads zeros. A rejected read still yields the SOURCE's geometry
 *     with a correctly sized transparent buffer: wrong geometry is the same lie as
 *     right geometry with no pixels.
 *  3. `drawImage` scales AND displaces in the 5-argument form (whole source into a
 *     destination rect), and honours the 9-argument source sub-rect form. Forms are
 *     counted AFTER the image, so they arrive as 2, 4 and 8 numbers; a 5-argument
 *     call tested as five numbers never matches and silently degrades to an unscaled
 *     draw at the origin.
 *  4. `createImageBitmap` snapshots the source's REAL pixels at call time. It is the
 *     model-bitmap re-seed path (paintTileSurface.toImageBitmap ->
 *     engine.setLayerImageBitmap -> overlayCtx.drawImage(layer.imageBitmap, 0, 0)),
 *     so a bitmap carrying no pixels re-seeds the next stroke with nothing.
 *  5. `clearRect` clears its rect and nothing else. Clearing the whole buffer hides a
 *     stale-pixel regression and manufactures a false "pixels were destroyed"
 *     reading; the per-tile clear (paintTileSurface.ts:251) and the dirty-rect clear
 *     (useBrushOverlay.ts:1065) both need the rest of the canvas to survive.
 *  6. `save`/`restore` scope the blend state AND the transform (fillStyle,
 *     globalAlpha, globalCompositeOperation, matrix). A no-op restore leaks
 *     `destination-out` and a fractional globalAlpha past the caller's save point, so
 *     an assertion about lockTransparency or destination-in then measures the leak,
 *     not the intent.
 *  7. `translate`, `scale`, `rotate`, `setTransform` and `resetTransform` maintain a
 *     real canvas matrix, and `drawImage` maps the destination rect through it. This
 *     is load-bearing: `layerComposite.drawLayerToContext` translates to the layer
 *     centre and then draws at `-lw/2, -lh/2, lw, lh`, and `performApplyCrop` composes
 *     the same sequence. While the transform was a no-op, that destination rect was
 *     drawn at the origin - a 64x64 layer in the top-left QUADRANT of a 64x64
 *     composite, 1024 painted pixels instead of 4096. Nothing noticed, because the
 *     unrecognised 5-argument drawImage (which also drew at the origin, unscaled) made
 *     the two defects cancel: a suite was green against a raster the product does not
 *     produce, and fixing either one alone turns the suite red.
 *
 * Deliberately NOT modelled. A test may not treat these as faithful:
 *  - PATHS paint nothing: `beginPath`, `moveTo`, `lineTo`, `closePath`, `rect`,
 *    `fill`, `stroke`, `clip`, `arc`. No paint or composite path builds a path, but
 *    `shapeRaster` and `textRasterizer` do and are not reached through this shim, so
 *    they are no-ops rather than throws - a throw here would break suites this shim
 *    never sees the inside of.
 *  - `fillRect` ignores the transform and always fills an axis-aligned rect. No
 *    caller sets a transform before a fillRect (`cropApply.ts:189` fills a fresh
 *    canvas with no transform), but a `translate` followed by a `fillRect` would be
 *    drawn unshifted.
 *  - `getContext("2d")` returns a FRESH handle per call, so state set through one
 *    handle is invisible to another. A real canvas shares one context per canvas.
 *  - the 2-number form `drawImage(img, dx, dy)` draws at the ORIGIN: dx/dy are read
 *    and discarded. Every live caller passes (0, 0) - useBrushOverlay.ts:865, :1034,
 *    :1075, :1710, :1716, :1737, :1856, :1864 and paintTileSurface.ts:191 - so no
 *    paint path is affected, but a non-zero offset is silently dropped.
 *  - a draw source's pixels are read from `getImageData` first, then its 2d context,
 *    then `data`, then `_buffer`. Neither `data` nor `_buffer` exists on a real
 *    canvas; they are this repo's per-suite canvas mocks, and `_buffer` is also what
 *    those mocks read back out of a draw source, so a bitmap minted here carries it
 *    too. The context read comes first so a real canvas is never shadowed by it, and
 *    neither branch assumes a reader tolerates a bare `getImageData()` - see rule 2.
 *  - `transferToImageBitmap` does not neuter the canvas (a real one does), and
 *    `close()` on a bitmap is a no-op.
 *  - `getImageData` on a returned bitmap is an affordance: a real ImageBitmap has no
 *    such method. It exists so `drawImage` can read the bitmap, so the pixels it
 *    reports must be real - a missing-pixel bitmap is exactly the defect it guards.
 *
 * Silence in the list above is not fidelity. A shim that quietly deviates is how four
 * rounds of investigation reached a wrong answer; add the rule here and its test
 * together, or do not add the method at all.
 */
type PixelRead = { data: Uint8ClampedArray; width: number; height: number };

/** Canvas matrix form: x' = a*x + c*y + e, y' = b*x + d*y + f. */
type Matrix = [number, number, number, number, number, number];

/** Canvas post-multiplies: a new transform composes onto the RIGHT of the current one. */
function multiply(m: Matrix, t: Matrix): Matrix {
  return [
    m[0] * t[0] + m[2] * t[1],
    m[1] * t[0] + m[3] * t[1],
    m[0] * t[2] + m[2] * t[3],
    m[1] * t[2] + m[3] * t[3],
    m[0] * t[4] + m[2] * t[5] + m[4],
    m[1] * t[4] + m[3] * t[5] + m[5],
  ];
}

/**
 * A bounded copy of a sub-rect, out-of-range pixels left transparent black - the
 * same shape `getImageData` returns outside the canvas.
 */
function cropPixels(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  x: number,
  y: number,
  w: number,
  h: number,
): Uint8ClampedArray {
  const out = new Uint8ClampedArray(Math.max(0, w) * Math.max(0, h) * 4);
  for (let row = 0; row < h; row++) {
    for (let col = 0; col < w; col++) {
      const sx = x + col;
      const sy = y + row;
      if (sx < 0 || sy < 0 || sx >= width || sy >= height) continue;
      const si = (sy * width + sx) * 4;
      const di = (row * w + col) * 4;
      out[di] = data[si];
      out[di + 1] = data[si + 1];
      out[di + 2] = data[si + 2];
      out[di + 3] = data[si + 3];
    }
  }
  return out;
}

/**
 * Accept a `getImageData` result only if it is self-consistent AND describes the
 * source it was read from. A reader called with the wrong rect computes its length
 * from those arguments, so a stub whose signature is `(x, y, w, h)` with no defaults
 * returns a ZERO-LENGTH array for a bare call - and reporting that as a valid read
 * hands back a bitmap with real dimensions and no bytes, which then spreads zeros
 * across whatever draws it. That is the same defect as a bitmap built from `src.data`,
 * one call deeper. A read whose own size differs from the source is rejected for the
 * same reason: its pixels are not a picture of this canvas.
 */
function acceptRead(img: any, expectWidth: number, expectHeight: number): PixelRead | undefined {
  const data = img?.data as ArrayLike<number> | undefined;
  if (!data) return undefined;
  const { width, height } = img as { width?: unknown; height?: unknown };
  if (typeof width !== "number" || typeof height !== "number") return undefined;
  if (width * height * 4 !== data.length) return undefined;
  if (expectWidth > 0 && expectHeight > 0 && (width !== expectWidth || height !== expectHeight)) {
    return undefined;
  }
  return { data: data as Uint8ClampedArray, width, height };
}

/**
 * Read a draw source's pixels the way the platform does: through its 2d context.
 * Probing only the source object is the trap - a canvas exposes its pixels on its
 * CONTEXT and not on itself, so an object-probe finds nothing for every canvas
 * source and the draw becomes a silent no-op that a green suite cannot see.
 *
 * The rect is passed EXPLICITLY rather than relying on a bare `getImageData()`: a
 * real canvas defaults that to the whole canvas, so both spellings are equivalent
 * there, but a test double with a `(x, y, w, h)` signature and no defaults turns the
 * bare call into `NaN * NaN * 4` bytes. Several canvas mocks in this repo are exactly
 * that shape.
 */
function readSourcePixels(source: unknown): PixelRead | undefined {
  const s = source as any;
  if (!s) return undefined;
  const w = Number(s.width) || 0;
  const h = Number(s.height) || 0;
  const full = w > 0 && h > 0 ? [0, 0, w, h] : [];
  if (typeof s.getImageData === "function") {
    const read = acceptRead(s.getImageData(...full), w, h);
    if (read) return read;
  }
  // Only reach for a context on something that actually looks like a canvas with a
  // real size. Test doubles in other suites expose a `getContext` that is not a
  // pixel source, and calling it blind threw IndexSizeError there.
  const looksLikeCanvas =
    typeof s.getContext === "function" && w > 0 && h > 0;
  if (looksLikeCanvas) {
    try {
      const c = s.getContext("2d");
      if (c && typeof c.getImageData === "function") {
        const read = acceptRead(c.getImageData(...full), w, h);
        if (read) return read;
      }
    } catch {
      return undefined;
    }
  }
  const raw = s.data ?? s._buffer;
  if (!raw) return undefined;
  return { data: raw as Uint8ClampedArray, width: s.width, height: s.height };
}

/**
 * A bitmap over a COPY of the pixels, at the geometry the caller resolved. The copy
 * is what the platform hands back: later paints on the source canvas are not visible
 * through the bitmap. With nothing to show, the buffer is transparent black at exactly
 * that geometry - never a short buffer that a draw would smear across a destination.
 */
function bitmapOverPixels(px: PixelRead | undefined, width: number, height: number): ImageBitmap {
  const data = px
    ? new Uint8ClampedArray(px.data as ArrayLike<number>)
    : new Uint8ClampedArray(Math.max(0, width * height * 4));
  return {
    width,
    height,
    close: () => {},
    getImageData: (x = 0, y = 0, w = width, h = height) => ({
      data: cropPixels(data, width, height, x, y, w, h),
      width: w,
      height: h,
      colorSpace: "srgb",
    }),
  } as unknown as ImageBitmap;
}

/**
 * `createImageBitmap(source)` for jsdom, reading whatever surface the source
 * actually exposes: a context for a canvas, `getImageData` for an object that has
 * it, `data`/`_buffer` for this repo's own mocks.
 *
 * Exported so the store emulator's jsdom mock mints bitmaps through the SAME code.
 * Two helpers that both return "an ImageBitmap" must not be able to disagree about
 * whether the bitmap carries pixels - a helper whose bitmap reports a valid size over
 * nothing readable is exactly the defect this file exists to eliminate, and it is
 * what the emulator mock used to be.
 */
export function snapshotBitmap(
  source: unknown,
  fallbackWidth = 0,
  fallbackHeight = 0,
): ImageBitmap {
  const src = source as any;
  const px =
    src instanceof FaithfulOffscreenCanvas ? src.snapshotPixels() : readSourcePixels(src);
  // The SOURCE's size is the bitmap's geometry, whatever became of the read. A
  // rejected read must not leave a bitmap that claims 0x0 while the canvas it came
  // from is 2x2: wrong geometry is the same lie as right geometry with no pixels.
  const width = fallbackWidth || Number(src?.width) || px?.width || 0;
  const height = fallbackHeight || Number(src?.height) || px?.height || 0;
  return bitmapOverPixels(px, width, height);
}

export class FaithfulOffscreenCanvas {
  width: number;
  height: number;
  private buffer: Uint8ClampedArray;
  /** Mirrors the platform: a transfer requires an obtained context. */
  private contextObtained = false;

  constructor(width: number, height: number) {
    this.width = width;
    this.height = height;
    this.buffer = new Uint8ClampedArray(Math.max(0, width) * Math.max(0, height) * 4);
  }

  getContext(type: string): CanvasRenderingContext2D {
    if (type !== "2d") return null as unknown as CanvasRenderingContext2D;
    this.contextObtained = true;
    const self = this;
    /** Blend state scoped by save()/restore(); see the state rules in the header. */
    const savedState: Array<{ fill: string; alpha: number; op: string; matrix: Matrix }> = [];
    let matrix: Matrix = [1, 0, 0, 1, 0, 0];
    const ctx = {
      _fillStyle: "#000000",
      globalAlpha: 1,
      globalCompositeOperation: "source-over",
      imageSmoothingEnabled: true,
      get fillStyle() { return (this as unknown as { _fillStyle: string })._fillStyle; },
      set fillStyle(v: string) { (this as unknown as { _fillStyle: string })._fillStyle = v; },
      clearRect(x = 0, y = 0, w = self.width, h = self.height) {
        // The given rect only. Over-clearing the whole buffer would hide a
        // stale-pixel regression and manufacture a false "pixels were destroyed"
        // reading for any caller that clears one tile or one dirty rect.
        const x0 = Math.max(0, Math.min(x, x + w));
        const x1 = Math.min(self.width, Math.max(x, x + w));
        const y0 = Math.max(0, Math.min(y, y + h));
        const y1 = Math.min(self.height, Math.max(y, y + h));
        for (let row = y0; row < y1; row++) {
          self.buffer.fill(0, (row * self.width + x0) * 4, (row * self.width + x1) * 4);
        }
      },
      fillRect(x: number, y: number, w: number, h: number) {
        const hex = ctx.fillStyle.replace("#", "");
        const r = parseInt(hex.slice(0, 2), 16) || 0;
        const g = parseInt(hex.slice(2, 4), 16) || 0;
        const b = parseInt(hex.slice(4, 6), 16) || 0;
        for (let row = y; row < y + h; row++) {
          for (let col = x; col < x + w; col++) {
            if (row < 0 || row >= self.height || col < 0 || col >= self.width) continue;
            const i = (row * self.width + col) * 4;
            self.buffer[i] = r;
            self.buffer[i + 1] = g;
            self.buffer[i + 2] = b;
            self.buffer[i + 3] = 255;
          }
        }
      },
      drawImage(src: any, ...rest: number[]) {
        const px = readSourcePixels(src);
        if (!px) return;
        const sd = px.data;
        const sw = src?.width ?? 0;
        const sh = src?.height ?? 0;
        if (!sw || !sh) return;
        // Geometry. `rest` holds the arguments AFTER the image, so the platform's
        // three forms arrive here as 2, 4 and 8 numbers:
        //   drawImage(img, dx, dy)                          -> 2: whole source, 1:1
        //   drawImage(img, dx, dy, dw, dh)                  -> 4: whole source scaled into that rect
        //   drawImage(img, sx, sy, sw, sh, dx, dy, dw, dh)  -> 8: source sub-rect scaled into that rect
        // Counting the image itself is the trap: a 5-ARGUMENT call is 4 numbers, so
        // a `rest.length === 5` test never matches and the call falls through to the
        // 2-number default, drawing the whole source UNSCALED at the ORIGIN whatever
        // destination rect it was given. The nearest-neighbour sampler below already
        // spans the destination rect correctly once the form is recognised - the
        // miscount was the whole defect. Live 4-number callers: the >256px
        // preview-tip downscale (useBrushOverlay.ts:676) and the shadow harness dabs
        // (rustShadow.ts:631,665).
        let sx = 0, sy = 0, cw = sw, ch = sh, dx = 0, dy = 0, dw = sw, dh = sh;
        if (rest.length === 4) {
          [dx, dy, dw, dh] = rest;
        } else if (rest.length >= 8) {
          [sx, sy, cw, ch, dx, dy, dw, dh] = rest;
        }
        const op = ctx.globalCompositeOperation;
        const ga = ctx.globalAlpha;
        // Map the destination rect through the current transform, then inverse-map
        // each DEVICE pixel centre back into destination-rect space: that is the
        // shape a real canvas rasterises (a parallelogram under rotation), and it
        // degenerates to the plain axis-aligned rect when the matrix is a translate.
        // A dropped translate is not a cosmetic gap: layerComposite.drawLayerToContext
        // translates to the layer centre and draws at -lw/2, -lh/2, so with the
        // translate ignored a 64x64 layer lands in the top-left QUADRANT of a 64x64
        // composite - 1024 painted pixels instead of 4096.
        const [ma, mb, mc, md, me, mf] = matrix;
        const det = ma * md - mb * mc;
        if (det === 0) return; // a degenerate transform collapses the draw to nothing
        const corner = (ux: number, uy: number): [number, number] => [
          ma * ux + mc * uy + me,
          mb * ux + md * uy + mf,
        ];
        const corners = [
          corner(dx, dy),
          corner(dx + dw, dy),
          corner(dx, dy + dh),
          corner(dx + dw, dy + dh),
        ];
        const xs = corners.map((p) => p[0]);
        const ys = corners.map((p) => p[1]);
        const minX = Math.max(0, Math.floor(Math.min(...xs)));
        const maxX = Math.min(self.width, Math.ceil(Math.max(...xs)));
        const minY = Math.max(0, Math.floor(Math.min(...ys)));
        const maxY = Math.min(self.height, Math.ceil(Math.max(...ys)));
        for (let ty = minY; ty < maxY; ty++) {
          for (let tx = minX; tx < maxX; tx++) {
            const px = tx + 0.5 - me;
            const py = ty + 0.5 - mf;
            const u = (md * px - mc * py) / det - dx;
            const v = (ma * py - mb * px) / det - dy;
            if (u < 0 || u >= dw || v < 0 || v >= dh) continue;
            const ux = Math.min(cw - 1, Math.floor((u * cw) / dw));
            const uy = Math.min(ch - 1, Math.floor((v * ch) / dh));
            const si = ((sy + uy) * sw + (sx + ux)) * 4;
            const ti = (ty * self.width + tx) * 4;
            const sa = (sd[si + 3] / 255) * ga;
            if (op === "destination-out") {
              // result.alpha = dst.alpha * (1 - src.alpha). Straight (non-premultiplied)
              // RGBA keeps dst colour and drops alpha; a browser's result is the same
              // shape, with colour collapsing once alpha reaches zero.
              self.buffer[ti + 3] = Math.round(self.buffer[ti + 3] * (1 - sa));
              if (self.buffer[ti + 3] === 0) {
                self.buffer[ti] = 0;
                self.buffer[ti + 1] = 0;
                self.buffer[ti + 2] = 0;
              }
            } else if (op === "destination-in") {
              self.buffer[ti + 3] = Math.round(self.buffer[ti + 3] * sa);
            } else {
              // source-over (and copy): blend src over dst by its alpha.
              const da = self.buffer[ti + 3] / 255;
              const oa = sa + da * (1 - sa);
              for (let c = 0; c < 3; c++) {
                self.buffer[ti + c] = Math.round(
                  (sd[si + c] * sa + self.buffer[ti + c] * da * (1 - sa)) / (oa || 1),
                );
              }
              self.buffer[ti + 3] = Math.round(oa * 255);
            }
          }
        }
      },
      getImageData(x = 0, y = 0, w = self.width, h = self.height) {
        return {
          data: cropPixels(self.buffer, self.width, self.height, x, y, w, h),
          width: w,
          height: h,
          colorSpace: "srgb",
        } as unknown as ImageData;
      },
      putImageData(img: any, dx = 0, dy = 0) {
        if (!img?.data) return;
        for (let row = 0; row < img.height; row++) {
          for (let col = 0; col < img.width; col++) {
            const tx = dx + col;
            const ty = dy + row;
            if (tx < 0 || ty < 0 || tx >= self.width || ty >= self.height) continue;
            const si = (row * img.width + col) * 4;
            const ti = (ty * self.width + tx) * 4;
            self.buffer[ti] = img.data[si];
            self.buffer[ti + 1] = img.data[si + 1];
            self.buffer[ti + 2] = img.data[si + 2];
            self.buffer[ti + 3] = img.data[si + 3];
          }
        }
      },
      save() {
        savedState.push({
          fill: ctx._fillStyle,
          alpha: ctx.globalAlpha,
          op: ctx.globalCompositeOperation,
          matrix: matrix.slice() as Matrix,
        });
      },
      restore() {
        const s = savedState.pop();
        // A real canvas ignores restore() on an empty stack rather than resetting.
        if (!s) return;
        ctx._fillStyle = s.fill;
        ctx.globalAlpha = s.alpha;
        ctx.globalCompositeOperation = s.op;
        matrix = s.matrix;
      },
      // A real transform stack. `drawLayerToContext` and `performApplyCrop` both
      // compose translate -> rotate -> scale -> drawImage(-lw/2, -lh/2, lw, lh), so
      // these are load-bearing, not decoration: with them dropped, that sequence
      // puts the whole raster in one corner of the destination.
      translate(x = 0, y = 0) {
        matrix = multiply(matrix, [1, 0, 0, 1, x, y]);
      },
      scale(x = 1, y = 1) {
        matrix = multiply(matrix, [x, 0, 0, y, 0, 0]);
      },
      rotate(radians = 0) {
        const cos = Math.cos(radians);
        const sin = Math.sin(radians);
        matrix = multiply(matrix, [cos, sin, -sin, cos, 0, 0]);
      },
      setTransform(a = 1, b = 0, c = 0, d = 1, e = 0, f = 0) {
        matrix = [a, b, c, d, e, f];
      },
      resetTransform() {
        matrix = [1, 0, 0, 1, 0, 0];
      },
      beginPath: () => {},
      closePath: () => {},
      moveTo: () => {},
      lineTo: () => {},
      // PATHS still paint nothing: see the not-modelled list in the header. Kept as
      // no-ops rather than throws because engine rasterisers call them.
      stroke: () => {},
      fill: () => {},
      rect: () => {},
      clip: () => {},
      // The real 2d context has this; the brush/eraser commit path calls it, and its
      // absence made every such path throw "ctx.createImageData is not a function".
      createImageData: (w: number, h: number) => ({
        data: new Uint8ClampedArray(Math.max(0, w) * Math.max(0, h) * 4),
        width: w,
        height: h,
        colorSpace: "srgb",
      }),
    };
    return ctx as unknown as CanvasRenderingContext2D;
  }

  /** A copy of the current pixels; the source a transferred/created bitmap sees. */
  snapshotPixels(): PixelRead {
    return { data: new Uint8ClampedArray(this.buffer), width: this.width, height: this.height };
  }

  transferToImageBitmap(): ImageBitmap {
    if (!this.contextObtained) {
      // Same message and error type the real canvas produces.
      const err = new Error(
        "Failed to execute 'transferToImageBitmap' on 'OffscreenCanvas': " +
        "Cannot transfer an ImageBitmap from an OffscreenCanvas with no context",
      );
      err.name = "InvalidStateError";
      throw err;
    }
    return bitmapOverPixels(this.snapshotPixels(), this.width, this.height);
  }
}

/** jsdom has neither global; the pixel paths construct both. */
export class ShimImageData {
  data: Uint8ClampedArray;
  width: number;
  height: number;
  constructor(data: Uint8ClampedArray | number, w?: number, h?: number) {
    if (typeof data === "number") {
      this.width = data;
      this.height = w!;
      this.data = new Uint8ClampedArray(data * w! * 4);
    } else {
      this.width = w!;
      this.height = h!;
      this.data = data;
    }
  }
}

/**
 * Install the faithful canvas plus the two globals jsdom lacks. Returns a
 * restore function.
 */
export function installFaithfulCanvas(): () => void {
  const g = globalThis as Record<string, unknown>;
  const prevCanvas = g.OffscreenCanvas;
  const prevImageData = g.ImageData;
  const prevCreateBitmap = g.createImageBitmap;
  g.OffscreenCanvas = FaithfulOffscreenCanvas;
  g.ImageData = ShimImageData;
  // A canvas has no `.data`, so reporting `src.data` handed back a bitmap whose
  // pixels were `undefined`; `drawImage` took its getImageData branch, got
  // nothing and returned early. This is the model-bitmap re-seed path
  // (paintTileSurface.toImageBitmap -> engine.setLayerImageBitmap ->
  // overlayCtx.drawImage(layer.imageBitmap, 0, 0)), so every stroke after the
  // first re-seeded from nothing and the next commit started from a blank layer.
  g.createImageBitmap = async (src: any) =>
    snapshotBitmap(src, Number(src?.width) || 0, Number(src?.height) || 0);
  return () => {
    g.OffscreenCanvas = prevCanvas;
    g.ImageData = prevImageData;
    g.createImageBitmap = prevCreateBitmap;
  };
}
