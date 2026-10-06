// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * A faithful in-process emulator of the Rust pixel store, for tests that must
 * observe the CANONICAL store rather than a mock that cannot see it.
 *
 * MOCK FIDELITY (see AGENTS.md "Mock fidelity rule"). A mock that reads an IPC
 * argument IN-PROCESS cannot reproduce a transport rejection, so every pixel
 * payload here is decoded through the same base64 contract the Rust boundary
 * uses, and anything that is not a base64 string is rejected with the message
 * serde produces.
 *
 * That rejection is not hypothetical: it is a shipped bug this emulator
 * reproduces. `ImageData.data` and a readback buffer are `Uint8ClampedArray`,
 * which is NOT `instanceof Uint8Array`, so passing one straight to
 * `rust_pixels_resize_layer` was rejected at the boundary and the store stayed
 * at pre-crop dimensions. A mock that read `args.bytes` as an in-process
 * ArrayLike could never have seen it. Pixel payloads are base64 for the same
 * family of reasons - see `pixelSeedCall.ts`.
 *
 * The pixel behaviours emulated are the ones production depends on, copied
 * from crates/core/src/pixel_store.rs:
 *  - `rust_pixels_get_epoch` REJECTS for an unseeded layer ("layer not
 *    initialized"), so a caller can detect "no store yet" by catch, not by a
 *    sentinel value;
 *  - `rust_pixels_write_region` REJECTS an out-of-bounds region
 *    (`x+w > layer.width`), which is exactly the failure a stale store produced:
 *    a stroke after a dimension-changing crop was validated against pre-crop
 *    dimensions and dropped;
 *  - `rust_pixels_resize_layer` REBUILDS the buffer at the new dimensions with
 *    epoch reset to 0, and drops the layer's pixel history (a stale-dimension
 *    tile patch must never replay onto the new grid);
 *  - `rust_pixels_write_region` returns the `after` tiles plus the bumped epoch
 *    and version, and tiles are 256-grid. It carries NO pre-image: the write
 *    appends the pre-image to the store's own history, and undo returns it from
 *    there. A double that answered `before` would let a caller read a field the
 *    real command stopped sending.
 */
import { vi } from "vitest";
import { snapshotBitmap } from "@/__tests__/faithfulOffscreenCanvas";
import { decodePixelBytes, readPixelSeedCall } from "@/lib/protocol/pixelSeedCall";

/**
 * Decode a pixel-payload argument the way the Rust boundary does: base64 in,
 * bytes out. Anything that is not a base64 string is rejected with the message
 * the Rust boundary produces, so a caller that ships the wrong JS type fails in
 * tests exactly as it does in the app.
 */
function transportDecode(bytesBase64: unknown, param: string): number[] {
  if (typeof bytesBase64 !== "string") {
    throw `invalid args \`${param}\` for command: invalid type: map, expected a base64 string`;
  }
  try {
    return Array.from(decodePixelBytes(bytesBase64));
  } catch {
    throw `invalid args \`${param}\` for command: invalid base64`;
  }
}

export interface EmulatedTile {
  x: number;
  y: number;
  w: number;
  h: number;
  data: number[];
}

export interface EmulatedLayer {
  width: number;
  height: number;
  pixels: Uint8ClampedArray;
  epoch: number;
  version: number;
  /** Undo stack of full-layer snapshots; `resize_layer` clears it. */
  history: { before: Uint8ClampedArray; after: Uint8ClampedArray }[];
}

export interface RustStoreEmulator {
  invoke: (cmd: string, args: any, options?: any) => Promise<any>;
  calls: { cmd: string; args: any }[];
  layers: Map<string, EmulatedLayer>;
  count: (cmd: string) => number;
  /** Seed a layer exactly as `rust_pixels_init` would. */
  seed: (layerId: string, width: number, height: number, pixels?: Uint8ClampedArray) => void;
  hash: (layerId: string) => string;
  pixelAt: (layerId: string, x: number, y: number) => number[];
  dispose: () => void;
}

const TILE = 256;

function tileKey(x: number, y: number): string {
  return `${Math.floor(x / TILE)},${Math.floor(y / TILE)}`;
}

/** FNV-1a over the canonical buffer; a stable identity for "did pixels move". */
export function hashPixels(pixels: Uint8ClampedArray): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < pixels.length; i++) {
    h ^= pixels[i];
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

function tilesOf(layer: EmulatedLayer): EmulatedTile[] {
  const cols = Math.ceil(layer.width / TILE);
  const rows = Math.ceil(layer.height / TILE);
  const out: EmulatedTile[] = [];
  for (let ty = 0; ty < rows; ty++) {
    for (let tx = 0; tx < cols; tx++) {
      const x = tx * TILE;
      const y = ty * TILE;
      const w = Math.min(TILE, layer.width - x);
      const h = Math.min(TILE, layer.height - y);
      if (w === 0 || h === 0) continue;
      const data: number[] = [];
      for (let row = 0; row < h; row++) {
        const src = ((y + row) * layer.width + x) * 4;
        for (let i = 0; i < w * 4; i++) data.push(layer.pixels[src + i]);
      }
      out.push({ x, y, w, h, data });
    }
  }
  return out;
}

function regionTiles(layer: EmulatedLayer, x: number, y: number, w: number, h: number): EmulatedTile[] {
  const cols = Math.ceil(layer.width / TILE);
  const rows = Math.ceil(layer.height / TILE);
  const tx0 = Math.floor(x / TILE);
  const ty0 = Math.floor(y / TILE);
  const tx1 = Math.min(Math.floor((x + w - 1) / TILE), cols - 1);
  const ty1 = Math.min(Math.floor((y + h - 1) / TILE), rows - 1);
  const out: EmulatedTile[] = [];
  for (let ty = ty0; ty <= ty1; ty++) {
    for (let tx = tx0; tx <= tx1; tx++) {
      const tx0px = tx * TILE;
      const ty0px = ty * TILE;
      const tw = Math.min(TILE, layer.width - tx0px);
      const th = Math.min(TILE, layer.height - ty0px);
      const data: number[] = [];
      for (let row = 0; row < th; row++) {
        const src = ((ty0px + row) * layer.width + tx0px) * 4;
        for (let i = 0; i < tw * 4; i++) data.push(layer.pixels[src + i]);
      }
      out.push({ x: tx0px, y: ty0px, w: tw, h: th, data });
    }
  }
  return out;
}

/**
 * Install the emulator as `@tauri-apps/api/core`'s `invoke`. Call from a test
 * file AFTER `vi.mock("@tauri-apps/api/core", () => ({ invoke: <this> }))`
 * has been hoisted, by assigning the returned `invoke` to the hoisted mock.
 */
export function createRustStoreEmulator(): RustStoreEmulator {
  const calls: { cmd: string; args: any }[] = [];
  const layers = new Map<string, EmulatedLayer>();

  const invoke = async (cmd: string, args: any, options?: any): Promise<any> => {
    calls.push({ cmd, args });
    const layerId: string = args?.layerId ?? readPixelSeedCall(cmd, args)?.layerId;
    switch (cmd) {
      case "rust_pixels_open_document":
        return undefined;
      case "rust_pixels_get_epoch": {
        const l = layers.get(layerId);
        // Mirrors PixelStoreRegistry::get_epoch -> Err("layer not initialized").
        if (!l) throw new Error(`layer not initialized: ${layerId}`);
        return l.epoch;
      }
      case "rust_pixels_init": {
        // Raw body + header metadata: the shape the command actually receives.
        const seed = readPixelSeedCall(cmd, args)!;
        const pixels = new Uint8ClampedArray(seed.bytes);
        layers.set(seed.layerId, {
          width: seed.width,
          height: seed.height,
          pixels,
          epoch: 0,
          version: 0,
          history: [],
        });
        return undefined;
      }
      case "rust_pixels_remove_layer": {
        layers.delete(layerId);
        return undefined;
      }
      case "rust_pixels_resize_layer": {
        const w = args.width as number;
        const h = args.height as number;
        const pixels = new Uint8ClampedArray(transportDecode(args.bytesBase64, "bytesBase64"));
        if (pixels.length !== w * h * 4) {
          // PixelLayer::new asserts the buffer length; a mismatched reseed is a
          // bug, not something to paper over.
          throw new Error("pixel buffer size mismatch");
        }
        // Rebuilds the buffer AND drops the layer's pixel history: a stale-dim
        // tile patch must never be replayed onto the new grid.
        layers.set(layerId, { width: w, height: h, pixels, epoch: 0, version: 0, history: [] });
        return undefined;
      }
      case "rust_pixels_snapshot_layer": {
        const l = layers.get(layerId);
        if (!l) throw new Error(`layer not initialized: ${layerId}`);
        return tilesOf(l);
      }
      case "rust_pixels_snapshot_tile": {
        const l = layers.get(layerId);
        if (!l) throw new Error(`layer not initialized: ${layerId}`);
        const w = args.w as number;
        const h = args.h as number;
        const data: number[] = [];
        for (let row = 0; row < h; row++) {
          const src = ((args.y + row) * l.width + args.x) * 4;
          for (let i = 0; i < w * 4; i++) data.push(l.pixels[src + i]);
        }
        return { key: tileKey(args.x, args.y), x: args.x, y: args.y, w, h, data };
      }
      case "rust_pixels_write_region": {
        const l = layers.get(layerId);
        if (!l) throw new Error(`layer not initialized or region out of bounds: ${layerId}`);
        const x = args.x as number;
        const y = args.y as number;
        const w = args.w as number;
        const h = args.h as number;
        const rgba = transportDecode(args.rgbaBase64, "rgbaBase64");
        // DocumentPixelStore::write_region bounds/length validation. This is the
        // exact check a stale store failed after a dimension-changing crop.
        if (x < 0 || y < 0 || w === 0 || h === 0) {
          throw new Error(`layer not initialized or region out of bounds: ${layerId}`);
        }
        if (x + w > l.width || y + h > l.height) {
          throw new Error(`layer not initialized or region out of bounds: ${layerId}`);
        }
        if (rgba.length !== w * h * 4) {
          throw new Error(`layer not initialized or region out of bounds: ${layerId}`);
        }
        const before = l.pixels.slice();
        for (let row = 0; row < h; row++) {
          const dst = ((y + row) * l.width + x) * 4;
          for (let i = 0; i < w * 4; i++) l.pixels[dst + i] = rgba[row * w * 4 + i];
        }
        l.history.push({ before, after: l.pixels.slice() });
        l.epoch += 1;
        l.version += 1;
        // The real reply carries the POST-image only - no pre-image. It used to
        // answer `before` too, which let a caller read a field the runtime stopped
        // sending: the write records the pre-image in this store's own history
        // (the `l.history.push` above), and undo returns it from there.
        return {
          after: regionTiles(l, x, y, w, h),
          epoch: l.epoch,
          version: l.version,
        };
      }
      case "rust_pixels_undo":
      case "rust_pixels_redo": {
        const l = layers.get(layerId);
        if (!l) throw new Error(`layer not initialized: ${layerId}`);
        const idx = cmd === "rust_pixels_undo" ? l.history.length - 1 : l.history.length;
        if (idx < 0 || idx >= l.history.length) return { layer_id: layerId, tiles: [], epoch: l.epoch, version: l.version };
        const step = l.history[idx];
        l.pixels = (cmd === "rust_pixels_undo" ? step.before : step.after).slice();
        if (cmd === "rust_pixels_undo") l.history.pop();
        else l.history.push(step);
        l.epoch += 1;
        l.version += 1;
        return { layer_id: layerId, tiles: tilesOf(l), epoch: l.epoch, version: l.version };
      }
      case "rust_pixels_record_external":
        return { tiles: [], epoch: layers.get(layerId)?.epoch ?? 0, version: 0 };
      case "protocol_apply_command_native":
        // Not a pixel command: the native graph shadow mirrors selection state
        // through this and heals on the next absolute selection op. The pixel
        // store is unaffected, so accept and ignore it.
        return { status: "applied" };
      default:
        throw new Error(`emulator: unhandled command ${cmd}`);
    }
  };

  return {
    invoke,
    calls,
    layers,
    count: (cmd) => calls.filter((c) => c.cmd === cmd).length,
    seed(layerId, width, height, pixels) {
      const buf = pixels ?? new Uint8ClampedArray(width * height * 4);
      layers.set(layerId, {
        width,
        height,
        pixels: new Uint8ClampedArray(buf),
        epoch: 0,
        version: 0,
        history: [],
      });
    },
    hash: (layerId) => {
      const l = layers.get(layerId);
      return l ? hashPixels(l.pixels) : "missing";
    },
    pixelAt(layerId, x, y) {
      const l = layers.get(layerId);
      if (!l) throw new Error(`layer not initialized: ${layerId}`);
      const i = (y * l.width + x) * 4;
      return [l.pixels[i], l.pixels[i + 1], l.pixels[i + 2], l.pixels[i + 3]];
    },
    dispose: () => {
      layers.clear();
      calls.length = 0;
    },
  };
}

/**
 * `createImageBitmap` shim for jsdom (which has none). `PaintTileSurface
 * .toImageBitmap()` routes through it, and the Rust-recorded ops project the
 * store's tiles back into `layer.imageBitmap` through exactly that call.
 *
 * MOCK FIDELITY: the bitmap carries the source's REAL pixels, read through the same
 * reader the faithful canvas shim uses (`snapshotBitmap`) and COPIED at call time.
 * The previous body read `src.data`, which a canvas does not have, so every bitmap
 * this mock handed back reported a valid size over zeroed pixels: indistinguishable
 * from a real bitmap by shape, and it painted nothing at all when a draw source was
 * read through it. A helper whose canvas stand-in looks real and carries nothing is
 * the same defect the faithful shim exists to eliminate, one layer over - and it is
 * the layer a suite reaches when it installs this mock instead of that shim.
 *
 * `_buffer` and the `close` spy are kept because the per-suite canvas mocks in this
 * repo read a draw source through `_buffer`, and callers assert on `close`.
 */
export function installCreateImageBitmapMock(): void {
  (globalThis as any).createImageBitmap = async (src: any) => {
    const bitmap = snapshotBitmap(src, Number(src?.width) || 0, Number(src?.height) || 0) as unknown as Record<string, unknown>;
    const read = bitmap.getImageData as () => { data: Uint8ClampedArray };
    bitmap._buffer = read().data;
    bitmap.close = vi.fn();
    return bitmap;
  };
  // `applyRustTilesToSurface` constructs `ImageData` directly, and jsdom has
  // none; without it the Rust-recorded ops throw while applying the store's
  // returned tiles and never reach their history commit.
  if (typeof (globalThis as any).ImageData === "undefined") {
    (globalThis as any).ImageData = class {
      data: Uint8ClampedArray;
      width: number;
      height: number;
      colorSpace = "srgb";
      constructor(a: Uint8ClampedArray | number, b?: number, c?: number) {
        if (typeof a === "number") {
          this.width = a;
          this.height = b!;
          this.data = new Uint8ClampedArray(a * b! * 4);
        } else {
          this.data = a;
          this.width = b!;
          this.height = c!;
        }
      }
    };
  }
}

/**
 * Drain every microtask/timer hop a fire-and-forget pixel op can take. The
 * production path is `void (async () => {...})()` behind two dynamic imports,
 * so a single tick is not enough to observe the write.
 */
export async function settlePixelOps(rounds = 8): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise((r) => setTimeout(r, 0));
  }
}