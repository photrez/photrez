// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * A software OffscreenCanvas stand-in that reproduces the platform rule which
 * production code depends on: `transferToImageBitmap()` throws InvalidStateError
 * when no 2d context has ever been obtained from that canvas.
 *
 * This is not a detail a lenient stub may skip. The real WebView2/Blink canvas
 * enforces it, and a stub that happily returns an ImageBitmap from a context-less
 * canvas lets production code ship a crash that throws on first use. That is
 * exactly how the Fill Layer empty-raster block reached a release with every
 * suite green.
 *
 * The same rule is honoured in production by useBrushOverlay.ts, which calls
 * getContext("2d") on its scratch canvas before transferring.
 */
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
    const ctx = {
      _fillStyle: "#000000",
      globalAlpha: 1,
      globalCompositeOperation: "source-over",
      imageSmoothingEnabled: true,
      get fillStyle() { return (this as unknown as { _fillStyle: string })._fillStyle; },
      set fillStyle(v: string) { (this as unknown as { _fillStyle: string })._fillStyle = v; },
      clearRect: () => { self.buffer.fill(0); },
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
        const sd: ArrayLike<number> | undefined =
          typeof src?.getImageData === "function" ? src.getImageData().data : src?.data;
        if (!sd) return;
        const sw = src?.width ?? 0;
        const sh = src?.height ?? 0;
        if (!sw || !sh) return;
        // Geometry: 3-arg draws the whole source at the origin; the 9-arg form
        // (sx, sy, sw, sh, dx, dy, dw, dh) draws a sub-rect, scaled nearest-neighbour.
        let sx = 0, sy = 0, cw = sw, ch = sh, dx = 0, dy = 0, dw = sw, dh = sh;
        if (rest.length >= 8) {
          [sx, sy, cw, ch, dx, dy, dw, dh] = rest;
        }
        const op = ctx.globalCompositeOperation;
        const ga = ctx.globalAlpha;
        for (let row = 0; row < dh; row++) {
          for (let col = 0; col < dw; col++) {
            const tx = dx + col;
            const ty = dy + row;
            if (tx < 0 || ty < 0 || tx >= self.width || ty >= self.height) continue;
            const ux = Math.min(cw - 1, Math.floor((col * cw) / dw));
            const uy = Math.min(ch - 1, Math.floor((row * ch) / dh));
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
        return { data: out, width: w, height: h, colorSpace: "srgb" } as unknown as ImageData;
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
      save: () => {},
      restore: () => {},
      translate: () => {},
      rotate: () => {},
      scale: () => {},
      beginPath: () => {},
      closePath: () => {},
      moveTo: () => {},
      lineTo: () => {},
      stroke: () => {},
      fill: () => {},
      rect: () => {},
      clip: () => {},
      setTransform: () => {},
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
  g.createImageBitmap = async (src: any) => ({
    width: src.width,
    height: src.height,
    getImageData: () => ({ data: src.data, width: src.width, height: src.height }),
  });
  return () => {
    g.OffscreenCanvas = prevCanvas;
    g.ImageData = prevImageData;
    g.createImageBitmap = prevCreateBitmap;
  };
}
