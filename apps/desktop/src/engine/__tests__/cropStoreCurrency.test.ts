// SPDX-License-Identifier: AGPL-3.0-or-later
import { pixelRegionDispatch } from "@/lib/protocol/pixelSeedCall";

/**
 * STORE CURRENCY AFTER A DIMENSION-CHANGING CROP.
 *
 * Crop with "delete cropped pixels" rewrote each layer's raster at new
 * dimensions. Before this suite, the Rust pixel store kept the PRE-crop bytes
 * at the PRE-crop dimensions, and `rust_pixels_write_region` validates every
 * region against the STORE's dimensions (crates/core/src/pixel_store.rs) - so
 * every stroke after a crop was rejected and painting was dead. Measured in the
 * real app: after a 128x128 -> 64x64 crop the store still reported 128x128 with
 * tiles [{0,0,128,128}], a real stroke emitted `write_region 0`, and the model
 * hash never moved.
 *
 * These tests drive PRODUCTION code (DocumentEngine.applyCrop -> cropApply, and
 * a real store emulator that enforces the same bounds check as Rust), so a
 * stale store makes them fail rather than silently pass.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { DocumentEngine } from "../../engine/document";
import { __setStoreCurrencyReporter } from "@/lib/paint/storeCurrency";
import {
  createRustStoreEmulator,
  installCreateImageBitmapMock,
  settlePixelOps,
  type RustStoreEmulator,
} from "@/lib/paint/__tests__/rustStoreEmulator";

const hoist = vi.hoisted(() => ({ invoke: null as null | ((c: string, a: any) => Promise<any>) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: (c: string, a: any) => hoist.invoke!(c, a) }));

let store: RustStoreEmulator;

/** Solid-red 100x100 raster, so a cleared pixel is unambiguous. */
function redRaster(w: number, h: number): Uint8ClampedArray {
  const buf = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    buf[i * 4] = 255;
    buf[i * 4 + 1] = 0;
    buf[i * 4 + 2] = 0;
    buf[i * 4 + 3] = 255;
  }
  return buf;
}

/** jsdom has no OffscreenCanvas; the crop path needs a 2d context + transfer. */
function installCanvasStub(): void {
  class Stub {
    width: number;
    height: number;
    _buffer: Uint8ClampedArray;
    constructor(w: number, h: number) {
      this.width = w;
      this.height = h;
      this._buffer = new Uint8ClampedArray(w * h * 4);
    }
    getContext(): any {
      const self = this;
      return {
        get width() { return self.width; },
        get height() { return self.height; },
        fillStyle: "",
        globalAlpha: 1,
        globalCompositeOperation: "source-over",
        save() {}, restore() {}, translate() {}, rotate() {}, scale() {},
        clearRect(x: number, y: number, w: number, h: number) {
          for (let r = y; r < y + h; r++) {
            for (let c = x; c < x + w; c++) {
              if (r < 0 || r >= self.height || c < 0 || c >= self.width) continue;
              const i = (r * self.width + c) * 4;
              self._buffer[i] = self._buffer[i + 1] = self._buffer[i + 2] = self._buffer[i + 3] = 0;
            }
          }
        },
        fillRect(x: number, y: number, w: number, h: number) {
          for (let r = y; r < y + h; r++) {
            for (let c = x; c < x + w; c++) {
              if (r < 0 || r >= self.height || c < 0 || c >= self.width) continue;
              const i = (r * self.width + c) * 4;
              self._buffer[i] = self._buffer[i + 1] = self._buffer[i + 2] = 0;
              self._buffer[i + 3] = 255;
            }
          }
        },
        drawImage(src: any) {
          const s = src as any;
          const sw = s.width ?? self.width;
          const sh = s.height ?? self.height;
          const sbuf: Uint8ClampedArray = s._buffer ?? new Uint8ClampedArray(sw * sh * 4);
          for (let r = 0; r < sh; r++) {
            for (let c = 0; c < sw; c++) {
              if (r >= self.height || c >= self.width) continue;
              const si = (r * sw + c) * 4;
              const di = (r * self.width + c) * 4;
              self._buffer[di] = sbuf[si];
              self._buffer[di + 1] = sbuf[si + 1];
              self._buffer[di + 2] = sbuf[si + 2];
              self._buffer[di + 3] = sbuf[si + 3];
            }
          }
        },
        getImageData(x: number, y: number, w: number, h: number) {
          const data = new Uint8ClampedArray(w * h * 4);
          for (let r = 0; r < h; r++) {
            for (let c = 0; c < w; c++) {
              const sx = x + c, sy = y + r;
              if (sy < 0 || sy >= self.height || sx < 0 || sx >= self.width) continue;
              const si = (sy * self.width + sx) * 4;
              const di = (r * w + c) * 4;
              data[di] = self._buffer[si];
              data[di + 1] = self._buffer[si + 1];
              data[di + 2] = self._buffer[si + 2];
              data[di + 3] = self._buffer[si + 3];
            }
          }
          return { data, width: w, height: h, colorSpace: "srgb" } as ImageData;
        },
        putImageData(img: any, x: number, y: number) {
          for (let r = 0; r < img.height; r++) {
            for (let c = 0; c < img.width; c++) {
              const dx = x + c, dy = y + r;
              if (dy < 0 || dy >= self.height || dx < 0 || dx >= self.width) continue;
              const si = (r * img.width + c) * 4;
              const di = (dy * self.width + dx) * 4;
              self._buffer[di] = img.data[si];
              self._buffer[di + 1] = img.data[si + 1];
              self._buffer[di + 2] = img.data[si + 2];
              self._buffer[di + 3] = img.data[si + 3];
            }
          }
        },
      };
    }
    transferToImageBitmap() {
      const buffer = this._buffer;
      return { width: this.width, height: this.height, _buffer: buffer, close: vi.fn() } as unknown as ImageBitmap;
    }
  }
  vi.stubGlobal("OffscreenCanvas", Stub as unknown as typeof OffscreenCanvas);
}

/** An engine whose one layer has a seeded 128x128 Rust store, like the real app. */
function makeEngine(): { engine: DocumentEngine; layerId: string } {
  const engine = new DocumentEngine("doc1", "Test", 128, 128);
  const layer = engine.addLayer("Photo", 128, 128);
  const canvas = new OffscreenCanvas(128, 128);
  const ctx = canvas.getContext("2d") as any;
  ctx.fillStyle = "#ff0000";
  ctx.fillRect(0, 0, 128, 128);
  engine.setLayerImageBitmap(layer.id, canvas.transferToImageBitmap());
  store.seed(layer.id, 128, 128, redRaster(128, 128));
  return { engine, layerId: layer.id };
}

beforeEach(() => {
  installCanvasStub();
  installCreateImageBitmapMock();
  store = createRustStoreEmulator();
  hoist.invoke = store.invoke;
});

afterEach(() => {
  store.dispose();
  vi.unstubAllGlobals();
});

describe("store currency after a dimension-changing crop", () => {
  it("NON-VACUITY: the jsdom createImageBitmap mock carries this suite's canvas pixels", async () => {
    // The other assertions in this file are about store CURRENCY - dimensions, tile
    // coverage, write geometry - and none of them looks at the pixels a bitmap
    // carries. That left a hole: this suite's canvas stub exposes its raster as
    // `_buffer`, and `createImageBitmap` is what `PaintTileSurface.toImageBitmap`
    // routes through, so a bitmap reporting a valid size and no bytes made every
    // draw in this file a silent no-op no assertion here could see.
    // This pins the contract the rest of the file rests on: a filled canvas comes
    // back out of the mock as a bitmap with a full-size buffer and those pixels in it.
    // Filled through `putImageData`, NOT `fillRect`. Written with `fillStyle =
    // "#ff0000"` + `fillRect` first, this assertion failed with `expected +0 to be
    // 64` - and the reason is worth recording rather than working around silently:
    // this stub's `fillRect` (`:75-84`) never reads `fillStyle` and always writes
    // opaque BLACK (`_buffer[i..i+2] = 0`, `_buffer[i+3] = 255`). So a fill through
    // it cannot produce a chosen colour at all, and `makeEngine()`'s
    // `ctx.fillStyle = "#ff0000"` above does not do what it reads like it does - the
    // layer's canvas is black while `redRaster()` seeds the STORE red. Any assertion
    // about a specific colour in this suite must therefore go through `putImageData`
    // (`:118-131`), which writes `_buffer` byte for byte. Recorded here so nobody
    // reads a colour assertion in this file as covering the fill path.
    const canvas = new OffscreenCanvas(8, 8);
    const ctx = canvas.getContext("2d") as any;
    const filled = new Uint8ClampedArray(8 * 8 * 4);
    for (let i = 0; i < filled.length; i += 4) {
      filled[i] = 255;
      filled[i + 3] = 255;
    }
    ctx.putImageData({ data: filled, width: 8, height: 8 }, 0, 0);

    const bitmap = (await (globalThis as any).createImageBitmap(canvas)) as {
      width: number;
      height: number;
      _buffer: Uint8ClampedArray;
    };
    expect(bitmap.width, "the bitmap keeps the canvas geometry").toBe(8);
    expect(bitmap.height).toBe(8);
    expect(
      bitmap._buffer.length,
      "a full-size buffer: a short one reports a raster it does not have",
    ).toBe(8 * 8 * 4);
    let painted = 0;
    for (let i = 0; i + 3 < bitmap._buffer.length; i += 4) {
      const r = bitmap._buffer[i];
      const g = bitmap._buffer[i + 1];
      const b = bitmap._buffer[i + 2];
      const a = bitmap._buffer[i + 3];
      if (r === 255 && g === 0 && b === 0 && a === 255) painted += 1;
    }
    expect(
      painted,
      "every filled pixel must survive the mock, or a draw of this bitmap paints nothing",
    ).toBe(8 * 8);
  });
  it("reseeds the Rust store to the post-crop dimensions and tile coverage", async () => {
    const { engine, layerId } = makeEngine();

    engine.applyCrop(32, 32, 64, 64, { deleteCroppedPixels: true });
    await settlePixelOps();

    const layer = engine.getLayer(layerId)!;
    const seeded = store.layers.get(layerId)!;

    // The layer raster moved 128x128 -> 64x64.
    expect(layer.width).toBe(64);
    expect(layer.height).toBe(64);
    // The STORE must agree. Before the fix it still read 128x128 here.
    expect(seeded.width).toBe(64);
    expect(seeded.height).toBe(64);
    expect(seeded.pixels.length).toBe(64 * 64 * 4);
    // Tile coverage follows the new grid, so no 128x128 tile remains.
    const tiles = await store.invoke("rust_pixels_snapshot_layer", { docId: "doc1", layerId });
    expect(tiles).toHaveLength(1);
    expect({ x: tiles[0].x, y: tiles[0].y, w: tiles[0].w, h: tiles[0].h })
      .toEqual({ x: 0, y: 0, w: 64, h: 64 });
  });

  it("a stroke after the crop emits exactly ONE rust_pixels_write_region that lands", async () => {
    const { engine, layerId } = makeEngine();
    engine.applyCrop(32, 32, 64, 64, { deleteCroppedPixels: true });
    await settlePixelOps();

    const hashBefore = store.hash(layerId);
    store.calls.length = 0;

    // The real stroke step: paint one dab into the store the way the brush
    // commit does. This is the step that emitted `write_region 0` in the app.
    await store.invoke(
      "rust_pixels_write_region",
      pixelRegionDispatch("doc1", layerId, 10, 10, 4, 4, new Uint8Array(4 * 4 * 4).fill(255)),
    );

    expect(store.count("rust_pixels_write_region")).toBe(1);
    // The write landed: the store moved, and the dab is readable back.
    expect(store.hash(layerId)).not.toBe(hashBefore);
    expect(store.pixelAt(layerId, 10, 10)).toEqual([255, 255, 255, 255]);
    expect(store.layers.get(layerId)!.epoch).toBe(1);
  });

  it("NON-VACUITY: an out-of-bounds write is rejected, so a stale store cannot pass by accident", async () => {
    const { engine, layerId } = makeEngine();
    engine.applyCrop(32, 32, 64, 64, { deleteCroppedPixels: true });
    await settlePixelOps();

    // The pre-crop stroke that used to be dropped: a region that only fits the
    // STALE 128x128 store. With a current 64x64 store it must REJECT.
    await expect(
      store.invoke(
        "rust_pixels_write_region",
        pixelRegionDispatch("doc1", layerId, 100, 100, 8, 8, new Uint8Array(8 * 8 * 4).fill(255)),
      ),
    ).rejects.toThrow(/out of bounds/);
  });

  it("advances bitmapEpoch to the epoch the store actually holds", async () => {
    const { engine, layerId } = makeEngine();
    engine.applyCrop(32, 32, 64, 64, { deleteCroppedPixels: true });
    await settlePixelOps();

    const layer = engine.getLayer(layerId)!;
    // A matching epoch is what tells `ensureBitmapCurrent` the installed raster
    // really is the canonical one. Before the fix bitmapEpoch stayed at the
    // pre-crop value while the bitmap was a different size entirely.
    expect(layer.bitmapEpoch).toBe(store.layers.get(layerId)!.epoch);
  });

  it("leaves a layer with NO Rust store alone (no store is created)", async () => {
    const engine = new DocumentEngine("doc2", "Test", 128, 128);
    const layer = engine.addLayer("Blank", 128, 128);
    const canvas = new OffscreenCanvas(128, 128);
    (canvas.getContext("2d") as any).fillRect(0, 0, 128, 128);
    engine.setLayerImageBitmap(layer.id, canvas.transferToImageBitmap());

    engine.applyCrop(32, 32, 64, 64, { deleteCroppedPixels: true });
    await settlePixelOps();

    // The documented contract: with no store the bitmap IS the source of truth,
    // so no store is minted and nothing is seeded.
    expect(store.layers.has(layer.id)).toBe(false);
    expect(store.count("rust_pixels_init")).toBe(0);
    expect(store.count("rust_pixels_resize_layer")).toBe(0);
  });

  it("crop UNDO (restore) reseeds the store back to the pre-crop dimensions", async () => {
    const { engine, layerId } = makeEngine();
    const beforeCrop = engine.snapshot();

    engine.applyCrop(32, 32, 64, 64, { deleteCroppedPixels: true });
    await settlePixelOps();
    expect(store.layers.get(layerId)!.width).toBe(64);

    // Undo restores the 128x128 layer; the store must follow, or the next
    // stroke is validated against the post-crop grid (the mirror defect).
    engine.restore(beforeCrop);
    await settlePixelOps();

    const layer = engine.getLayer(layerId)!;
    expect(layer.width).toBe(128);
    expect(store.layers.get(layerId)!.width).toBe(128);
    expect(store.layers.get(layerId)!.height).toBe(128);
  });

  it("a stroke lands at the right GEOMETRY, not just inside the store", async () => {
    // The measured failure was "write_region landed but the geometry was still
    // wrong because the store was stale". This proves the store's tile grid is
    // the post-crop grid, so a stroke at post-crop (40,40) is stored at exactly
    // that offset rather than at a pre-crop offset.
    const { engine, layerId } = makeEngine();
    engine.applyCrop(32, 32, 64, 64, { deleteCroppedPixels: true });
    await settlePixelOps();

    // Whatever the crop produced is the baseline this test compares against:
    // the content of a crop is the crop's business, not this test's.
    const neighbourBefore = store.pixelAt(layerId, 39, 39);
    const px = store.layers.get(layerId)!.pixels;

    await store.invoke(
      "rust_pixels_write_region",
      pixelRegionDispatch("doc1", layerId, 40, 40, 2, 2, new Uint8Array(2 * 2 * 4).fill(255)),
    );

    // The dab sits where it was painted.
    expect(store.pixelAt(layerId, 40, 40)).toEqual([255, 255, 255, 255]);
    expect(store.pixelAt(layerId, 41, 41)).toEqual([255, 255, 255, 255]);
    // A neighbouring pixel is untouched.
    expect(store.pixelAt(layerId, 39, 39)).toEqual(neighbourBefore);
    // The store's own pixels prove the OFFSET: row-major index (40*64 + 40) of
    // a 64-wide buffer. On the stale pre-crop grid the same dab would have been
    // written into a 128-wide row at a different absolute index, which is the
    // "landed but geometry wrong" symptom the real app showed.
    expect(px[(40 * 64 + 40) * 4]).toBe(255);
    expect(px[(41 * 64 + 41) * 4 + 3]).toBe(255);
    expect(px[(40 * 64 + 39) * 4 + 3]).toBe(neighbourBefore[3]);
  });

  it("the active layer survives the crop (a stroke target still exists)", async () => {
    // SEPARATE DEFECT, deliberately pinned here so the two are never confused.
    // `applyCropPreview`'s UI teardown calls `engine.setActiveLayer(null)`
    // (cropToolActions.ts:140, pinned by cropToolActions.test.ts:142), which
    // leaves NO paint target until the user clicks a layer row. That is a
    // crop-tool selection-policy decision in the UI layer, NOT pixel ownership,
    // and the engine must not compound it: applyCrop leaves the active layer
    // alone so the store fix and the selection policy stay separable.
    const { engine, layerId } = makeEngine();
    engine.setActiveLayer(layerId);
    expect(engine.getActiveLayerId()).toBe(layerId);

    engine.applyCrop(32, 32, 64, 64, { deleteCroppedPixels: true });
    await settlePixelOps();

    expect(engine.getActiveLayerId(), "applyCrop must not clear the active layer").toBe(layerId);
    // And the store is current for that layer, so a stroke on it lands.
    await store.invoke(
      "rust_pixels_write_region",
      pixelRegionDispatch("doc1", engine.getActiveLayerId()!, 1, 1, 2, 2, new Uint8Array(2 * 2 * 4).fill(255)),
    );
    expect(store.count("rust_pixels_write_region")).toBe(1);
  });

  it("a reseed that FAILS at the boundary drops the store instead of leaving two owners", async () => {
    // The swallowed-warning defect: a failed reseed used to be downgraded to a
    // console.warn, so the document kept a stale store AND the user saw a
    // flawless crop. The correct behaviour is to restore the single-owner
    // contract by dropping the store (nothing usable is lost - a store that
    // could not be resized was already wrong-dimensioned) and to say so.
    const { engine, layerId } = makeEngine();
    const failures: string[] = [];
    __setStoreCurrencyReporter((m) => failures.push(m));

    // Make the resize fail the way a real transport rejection does. The mock's
    // `invoke` is bound into the hoisted Tauri mock in beforeEach, so that is
    // what has to be swapped.
    const realInvoke = hoist.invoke!;
    hoist.invoke = async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_resize_layer") throw "E_RUST: invalid args `bytesBase64`: invalid type: map";
      return realInvoke(cmd, args);
    };

    try {
      engine.applyCrop(32, 32, 64, 64, { deleteCroppedPixels: true });
      await settlePixelOps();
    } finally {
      hoist.invoke = realInvoke;
      __setStoreCurrencyReporter(null);
    }

    // The store is GONE, not stale: the model is unambiguously the sole owner.
    expect(store.layers.has(layerId)).toBe(false);
    // And the user was told, because dropping the store loses that layer's
    // pixel history - a real consequence, not a silent internal detail.
    expect(failures).toHaveLength(1);
    expect(failures[0]).toMatch(/could not be re-recorded/i);
    // No epoch is claimed against a store that no longer exists.
    expect(engine.getLayer(layerId)!.bitmapEpoch).toBeUndefined();
  });
});