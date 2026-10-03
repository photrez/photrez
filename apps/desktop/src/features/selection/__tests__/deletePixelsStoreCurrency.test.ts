// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * NO RESURRECTION AFTER DELETE PIXELS / CUT.
 *
 * Delete and Cut used to clear the selection by assigning a new
 * `layer.imageBitmap` directly, so the Rust pixel store kept the PRE-delete
 * bytes at the same dimensions. The two owners then disagreed: the model showed
 * the hole, the store still held the pixels. The next stroke rehydrates its
 * PaintTileSurface from the store, so the deleted region came BACK.
 *
 * These tests drive PRODUCTION code (SelectionOperations.deleteSelection /
 * cutSelection) against a faithful store emulator that enforces the same
 * bounds/ownership rules as crates/core/src/pixel_store.rs, and assert on the
 * STORE - not only on the model bitmap - so a stale store fails the suite.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { DocumentEngine } from "../../../engine/document";
import { SelectionOperations } from "../SelectionOperations";
import { CommandHistory } from "../../../engine/history";
import {
  createRustStoreEmulator,
  installCreateImageBitmapMock,
  settlePixelOps,
  type RustStoreEmulator,
} from "@/lib/paint/__tests__/rustStoreEmulator";

const hoist = vi.hoisted(() => ({ invoke: null as null | ((c: string, a: any) => Promise<any>) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: (c: string, a: any) => hoist.invoke!(c, a) }));

let store: RustStoreEmulator;

const W = 100;
const H = 100;

/** Opaque red raster: a cleared pixel is unambiguous (alpha 0). */
function redRaster(): Uint8ClampedArray {
  const buf = new Uint8ClampedArray(W * H * 4);
  for (let i = 0; i < W * H; i++) {
    buf[i * 4] = 255;
    buf[i * 4 + 1] = 0;
    buf[i * 4 + 2] = 0;
    buf[i * 4 + 3] = 255;
  }
  return buf;
}

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
      return { width: this.width, height: this.height, _buffer: this._buffer, close: vi.fn() } as unknown as ImageBitmap;
    }
  }
  vi.stubGlobal("OffscreenCanvas", Stub as unknown as typeof OffscreenCanvas);
}

/**
 * One layer, a seeded Rust store, and a real CommandHistory - so the undo
 * assertions exercise the production dispatch rather than a stubbed history.
 */
function makeEngine(): { engine: DocumentEngine; history: CommandHistory; layerId: string; renderer: any } {
  const engine = new DocumentEngine("doc1", "Test", W, H);
  const layer = engine.addLayer("Photo", W, H);
  const canvas = new OffscreenCanvas(W, H);
  const ctx = canvas.getContext("2d") as any;
  ctx.fillStyle = "#ff0000";
  ctx.fillRect(0, 0, W, H);
  engine.setLayerImageBitmap(layer.id, canvas.transferToImageBitmap());
  engine.setActiveLayer(layer.id);
  store.seed(layer.id, W, H, redRaster());
  const history = new CommandHistory(50);
  const renderer = { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() } as any;
  return { engine, history, layerId: layer.id, renderer };
}

/** One brush dab, written the way the production brush commit writes. */
async function stroke(layerId: string, x: number, y: number, size = 2, value = 0): Promise<void> {
  await store.invoke("rust_pixels_write_region", {
    docId: "doc1",
    layerId,
    x,
    y,
    w: size,
    h: size,
    rgba: new Uint8Array(size * size * 4).fill(value),
  });
}

beforeEach(() => {
  installCanvasStub();
  installCreateImageBitmapMock();
  store = createRustStoreEmulator();
  hoist.invoke = store.invoke;
  SelectionOperations.__resetClipboard();
});

afterEach(() => {
  store.dispose();
  vi.unstubAllGlobals();
});

describe("delete pixels / cut do not leave the Rust store stale", () => {
  it("NON-VACUITY: the jsdom createImageBitmap mock carries this suite's canvas pixels", async () => {
    // The rest of this file asserts store currency - write counts, hashes, alpha at
    // chosen coordinates - and never looks at the pixels a minted bitmap carries. That
    // left the mock unasserted, and it is what `PaintTileSurface.toImageBitmap` routes
    // through: a bitmap reporting a valid size with no bytes turns every draw of it
    // into a silent no-op that a write-count assertion cannot see.
    // Filled through `putImageData`, NOT `fillRect`. Written with `fillStyle =
    // "#ff0000"` + `fillRect` first, this assertion failed with `expected +0 to be
    // 64`, and the reason is the useful part rather than an obstacle: this stub's
    // `fillRect` (`:76-84`) never reads `fillStyle` and always writes opaque BLACK
    // (`_buffer[i..i+2] = 0`, `_buffer[i+3] = 255`). The fill was painting all along -
    // just not in the colour the assertion was asking for. So this suite cannot
    // express a chosen colour through its fill path at all, and `makeEngine()`'s
    // `ctx.fillStyle = "#ff0000"` does not do what it reads like it does: the layer's
    // canvas is black while `redRaster()` seeds the STORE red. A colour assertion here
    // has to go through `putImageData` (`:119-132`), which writes `_buffer` byte for
    // byte. Recorded so a colour assertion in this file is not mistaken for coverage
    // of the fill path.
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
      _buffer: Uint8ClampedArray;
    };
    expect(bitmap.width, "the bitmap keeps the canvas geometry").toBe(8);
    expect(bitmap._buffer.length, "and a full-size buffer, not a short one").toBe(8 * 8 * 4);
    let painted = 0;
    for (let i = 0; i + 3 < bitmap._buffer.length; i += 4) {
      if (
        bitmap._buffer[i] === 255 &&
        bitmap._buffer[i + 1] === 0 &&
        bitmap._buffer[i + 2] === 0 &&
        bitmap._buffer[i + 3] === 255
      ) {
        painted += 1;
      }
    }
    expect(
      painted,
      "every filled pixel must survive the mock, or a delete re-seeding from it would " +
        "clear the whole layer and no write count would notice",
    ).toBe(8 * 8);
  });
  it("deleteSelection records one rust_pixels_write_region over the deleted region", async () => {
    const { engine, history, layerId, renderer } = makeEngine();
    engine.createSelection(20, 20, 30, 30);

    SelectionOperations.deleteSelection(engine, history, renderer);
    await settlePixelOps();

    // One canonical write, and it landed in the store (not only in the model).
    expect(store.count("rust_pixels_write_region")).toBe(1);
    expect(store.pixelAt(layerId, 25, 25)[3]).toBe(0);
    // Outside the marquee is untouched.
    expect(store.pixelAt(layerId, 80, 80)[3]).toBe(255);
  });

  it("commits exactly one history entry, marked rustOwned", async () => {
    const { engine, history, renderer } = makeEngine();
    const commit = vi.spyOn(history, "commit");
    engine.createSelection(20, 20, 30, 30);

    SelectionOperations.deleteSelection(engine, history, renderer);
    await settlePixelOps();

    expect(commit).toHaveBeenCalledTimes(1);
    expect(commit.mock.calls[0][1]).toBe("Delete Pixels");
    const imperative = commit.mock.calls[0][2];
    expect(imperative?.rustOwned).toBe(true);
    expect(imperative?.layerId).toBe(engine.getActiveLayerId());
    expect(commit.mock.calls[0][3]).toBe(true);
  });

  it("paint, delete, paint again: the deleted region STAYS deleted", async () => {
    const { engine, history, layerId, renderer } = makeEngine();

    // 1. paint a dab inside the region we are about to delete
    await stroke(layerId, 22, 22, 4, 0);
    expect(store.pixelAt(layerId, 23, 23)[3]).toBe(0);

    // 2. delete the marquee
    engine.createSelection(20, 20, 30, 30);
    SelectionOperations.deleteSelection(engine, history, renderer);
    await settlePixelOps();
    const afterDelete = store.hash(layerId);
    expect(store.pixelAt(layerId, 25, 25)[3]).toBe(0);

    // 3. paint again, elsewhere. The brush path rehydrates its surface from the
    //    store first; with a stale store that rehydration would resurrect the
    //    deleted pixels and undo the delete.
    await stroke(layerId, 80, 80, 2, 0);

    expect(store.hash(layerId)).not.toBe(afterDelete);
    expect(store.pixelAt(layerId, 25, 25)[3]).toBe(0);
    expect(store.pixelAt(layerId, 21, 21)[3]).toBe(0);
    // The store and the model must agree on the hole.
    const modelBuffer = (engine.getLayerImageBitmap(layerId) as any)._buffer as Uint8ClampedArray;
    expect(modelBuffer[(25 * W + 25) * 4 + 3]).toBe(0);
  });

  it("NON-VACUITY: undo restores the deleted pixels EXACTLY once", async () => {
    const { engine, history, layerId, renderer } = makeEngine();
    const beforeHash = store.hash(layerId);

    engine.createSelection(20, 20, 30, 30);
    SelectionOperations.deleteSelection(engine, history, renderer);
    await settlePixelOps();
    expect(store.pixelAt(layerId, 25, 25)[3]).toBe(0);

    // The undo DISPATCH contract: the TS store pops the entry and hands back
    // its cursor token; for a rustOwned token the dispatcher then takes the
    // pixels from Rust and steps the Rust cursor once. `CommandHistory.undo`
    // alone only moves the TS stack, so both halves are driven here exactly as
    // restoreHistorySnapshot does.
    const restored = history.undo(engine.snapshot());
    expect(restored).not.toBeNull();
    const patches = history.consumeLastUndoPatches();
    expect(patches?.rustOwned, "the delete entry is a cursor token Rust owns").toBe(true);

    const rustRes = await store.invoke("rust_pixels_undo", { docId: "doc1", layerId: patches!.layerId });
    expect(rustRes.tiles.length).toBeGreaterThan(0);

    // Once, not twice: the pixels are back and the whole buffer matches the
    // pre-delete hash (a double restore would overshoot into another state).
    expect(store.pixelAt(layerId, 25, 25)[3]).toBe(255);
    expect(store.hash(layerId)).toBe(beforeHash);

    // A second undo has nothing left to pop, so the store must not move again.
    const settled = store.hash(layerId);
    expect(history.canUndo()).toBe(false);
    expect(history.undo(engine.snapshot())).toBeNull();
    await settlePixelOps();
    expect(store.hash(layerId)).toBe(settled);
  });

  it("cutSelection records the clear and populates the clipboard", async () => {
    const { engine, history, layerId, renderer } = makeEngine();
    engine.createSelection(20, 20, 30, 30);

    SelectionOperations.cutSelection(engine, history, renderer);
    await settlePixelOps();

    expect(SelectionOperations.hasClipboard()).toBe(true);
    expect(store.count("rust_pixels_write_region")).toBe(1);
    expect(store.pixelAt(layerId, 25, 25)[3]).toBe(0);
  });

  it("an inverted selection clears OUTSIDE the marquee in the store too", async () => {
    const { engine, history, layerId, renderer } = makeEngine();
    engine.createSelection(20, 20, 30, 30);
    engine.invertSelection();

    SelectionOperations.deleteSelection(engine, history, renderer);
    await settlePixelOps();

    expect(store.pixelAt(layerId, 25, 25)[3]).toBe(255);
    expect(store.pixelAt(layerId, 80, 80)[3]).toBe(0);
  });

  it("seeds a layer that had no store yet (delete can be the FIRST raster op)", async () => {
    const engine = new DocumentEngine("doc3", "Test", W, H);
    const layer = engine.addLayer("Fresh", W, H);
    const canvas = new OffscreenCanvas(W, H);
    (canvas.getContext("2d") as any).fillRect(0, 0, W, H);
    engine.setLayerImageBitmap(layer.id, canvas.transferToImageBitmap());
    engine.setActiveLayer(layer.id);
    const history = new CommandHistory(50);
    const renderer = { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() } as any;

    engine.createSelection(20, 20, 30, 30);
    SelectionOperations.deleteSelection(engine, history, renderer);
    await settlePixelOps();

    // A store now exists for this layer and the delete is recorded in it.
    expect(store.layers.has(layer.id)).toBe(true);
    expect(store.count("rust_pixels_init")).toBe(1);
    expect(store.count("rust_pixels_write_region")).toBe(1);
    expect(store.pixelAt(layer.id, 25, 25)[3]).toBe(0);
  });
});