// SPDX-License-Identifier: AGPL-3.0-or-later
import { encodePixelBytes } from "@/lib/protocol/pixelSeedCall";

/**
 * STORE-CURRENCY INVARIANT
 *
 * A layer has at most ONE canonical pixel owner, and the Rust pixel store
 * (`PixelStoreRegistry`) is it whenever a store exists for that layer. Every
 * other pixel surface is DERIVED and must agree with the store:
 *
 *   1. store dimensions/tiles  ==  the layer raster's dimensions
 *   2. the cached `PaintTileSurface` is dropped whenever (1) is broken
 *   3. `layer.bitmapEpoch` names the store epoch the installed bitmap holds,
 *      and is stamped only after the raster that matches it was installed
 *
 * Two production paths used to break this and leave the store stale:
 *
 * - Crop with "delete cropped pixels" rewrote `layer.imageBitmap/width/height`
 *   by direct field assignment, so the store kept pre-crop bytes at pre-crop
 *   dimensions. `rust_pixels_write_region` validates the region against the
 *   STORE's dimensions, so every subsequent stroke was rejected and painting
 *   was dead.
 * - Crop's undo restores the pre-crop layer through `restore()`, which dropped
 *   the paint surface but left the store at post-crop dimensions - the mirror
 *   image of the same defect.
 *
 * `syncLayerStoreToLayerRaster` is the single repair both call. It reseeds an
 * EXISTING store only: a layer with no store keeps the established contract
 * that its bitmap is the source of truth (`ensureBitmapCurrent` treats a
 * missing store as "nothing to sync" rather than creating one).
 */

import { invoke } from "@tauri-apps/api/core";

/** The slice of `DocumentEngine` this module needs. */
export interface StoreCurrencyLayer {
  id: string;
  width: number;
  height: number;
  imageBitmap?: ImageBitmap | null;
  bitmapEpoch?: number;
}

export interface StoreCurrencyEngine {
  getLayer(id: string): StoreCurrencyLayer | null | undefined;
  /** Drops the cached `PaintTileSurface` for a layer whose raster changed. */
  invalidatePaintSurface(id: string): void;
}

/**
 * Overridable sink for ownership failures, so tests can observe them without a
 * toast host. Defaults to a user-visible toast: a document whose Rust store and
 * model disagree about pixels is a broken document, and that must never be
 * reported only to a console the user never sees.
 */
type StoreCurrencyReporter = (message: string) => void;
let reportFailure: StoreCurrencyReporter = (message) => {
  void import("@/components/editor/Toast")
    .then(({ showToast }) => showToast(message, "error"))
    .catch(() => console.warn(`[store-currency] ${message}`));
};

/** Test-only: capture ownership failures instead of showing a toast. */
export function __setStoreCurrencyReporter(fn: StoreCurrencyReporter | null): void {
  reportFailure = fn ?? ((message) => console.warn(`[store-currency] ${message}`));
}

function reportStoreCurrencyFailure(message: string): void {
  reportFailure(message);
}

/**
 * Convert pixel bytes into the shape the Tauri IPC actually accepts for a Rust
 * `Vec<u8>` argument.
 *
 * The IPC replacer (tauri/scripts/process-ipc-message-fn.js) turns ONLY a
 * `Map`, a `Uint8Array` and an `ArrayBuffer` into a JSON sequence. Everything
 * else falls through to `JSON.stringify`, and a `Uint8ClampedArray` - which is
 * what `ImageData.data` and every canvas readback returns - is NOT
 * `instanceof Uint8Array`, so it serializes to `{"0":..,"1":..}` and serde
 * rejects it with `invalid type: map, expected a sequence`.
 *
 * Every byte payload crossing this boundary therefore goes through here.
 * Copying, rather than passing the clamped array, also detaches the payload
 * from any canvas backing store that could be recycled underneath us.
 */
export function toIpcBytes(bytes: Uint8ClampedArray): Uint8Array {
  return new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength);
}

/**
 * Read RGBA bytes back out of an `ImageBitmap` at its own dimensions.
 * Used when a caller hands back an installed raster rather than the bytes it
 * drew; prefer passing the exact buffer when the producer already has it.
 */
export function readbackBitmap(bitmap: ImageBitmap, width: number, height: number): Uint8ClampedArray {
  const canvas = new OffscreenCanvas(width, height);
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return new Uint8ClampedArray(width * height * 4);
  ctx.drawImage(bitmap as CanvasImageSource, 0, 0);
  return ctx.getImageData(0, 0, width, height).data;
}

/**
 * Bring the Rust pixel store for `layerId` to exactly the layer raster's
 * current state, then advance `bitmapEpoch` so the next `ensureBitmapCurrent`
 * sees a current raster instead of rebuilding one it does not need.
 *
 * Contract:
 * - No store for the layer -> no-op, returns false. The bitmap stays the
 *   source of truth (same rule as `ensureBitmapCurrent`).
 * - Store present -> `rust_pixels_resize_layer` REPLACES the buffer and its
 *   tile grid at the new dimensions, which also drops that layer's pixel
 *   history entries: a stale-dimension tile patch must never be replayed onto
 *   the new grid (`DocumentPixelStore::resize_layer`).
 * - The cached paint surface is dropped before the write, so no later paint op
 *   can composite onto pre-rewrite tiles.
 * - `bitmapEpoch` is stamped from a fresh probe AFTER the resize landed, so it
 *   can never claim a currency the raster does not have.
 *
 * Returns whether the store is now current with the raster: true when it was
 * reseeded, false when the layer has no store (the documented "bitmap is the
 * source of truth" fallback).
 */
export async function syncLayerStoreToLayerRaster(
  docId: string,
  engine: StoreCurrencyEngine,
  layerId: string,
  /** Exact RGBA bytes of the post-rewrite raster, when the producer still has it. */
  rgba?: Uint8ClampedArray,
): Promise<boolean> {
  const layer = engine.getLayer(layerId);
  if (!layer || !layer.imageBitmap || layer.width <= 0 || layer.height <= 0) return false;

  // Read-only probe: stays outside the six-command census by design, and its
  // rejection is the "this layer has no canonical store" signal.
  try {
    await invoke("rust_pixels_get_epoch", { docId, layerId });
  } catch {
    return false;
  }

  const bytes = toIpcBytes(rgba ?? readbackBitmap(layer.imageBitmap, layer.width, layer.height));
  // The cached surface is derived from the raster this write replaces; drop it
  // first so a concurrent paint op rebuilds from the new tile grid.
  engine.invalidatePaintSurface(layerId);
  try {
    await invoke("rust_pixels_resize_layer", {
      docId,
      layerId,
      width: layer.width,
      height: layer.height,
      bytesBase64: encodePixelBytes(bytes),
    });
  } catch (err) {
    // THE OWNERSHIP REPAIR. Leaving the store at pre-rewrite dimensions IS the
    // cross-owner disagreement this module exists to prevent: the model shows
    // cropped pixels, the store holds pre-crop ones at the old size, and every
    // later stroke is silently dropped by write_region's bounds check. A log
    // line cannot repair that, so the store is DROPPED instead. That restores
    // the single-owner contract the rest of the codebase already relies on
    // (ensureBitmapCurrent treats a missing store as "the bitmap IS the source
    // of truth"), and the next paint op re-seeds it from the current raster at
    // the current dimensions, so painting works again. Nothing usable is lost:
    // a store that could not be resized was already wrong-dimensioned, so its
    // pixel history could not have been replayed either.
    try {
      await invoke("rust_pixels_remove_layer", { docId, layerId });
    } catch {
      // Even the repair failed; the invariant cannot be restored from here.
    }
    engine.invalidatePaintSurface(layerId);
    // The epoch now names no store at all; clearing it stops ensureBitmapCurrent
    // from treating the pre-rewrite epoch as meaningful.
    layer.bitmapEpoch = undefined;
    reportStoreCurrencyFailure(
      `Pixel history for one layer was dropped: its store could not be re-recorded (${String(err)}). Painting will re-seed it.`,
    );
    return false;
  }
  // Stamp the epoch the resize actually produced. `resize_layer` rebuilds the
  // layer from scratch, so its epoch is NOT the pre-rewrite one; probing is the
  // only honest source.
  const epoch = await invoke("rust_pixels_get_epoch", { docId, layerId });
  if (typeof epoch === "number") layer.bitmapEpoch = epoch;
  return true;
}

/** One rewritten raster, as reported by a producer that just replaced it. */
export interface RewrittenRaster {
  id: string;
  width: number;
  height: number;
  /** Exact RGBA bytes of the rewritten raster, when the producer still has them. */
  rgba?: Uint8ClampedArray;
}

/**
 * Bring every rewritten raster's store into agreement with the model, in one
 * pass.
 *
 * A reseed that fails is NOT swallowed. `syncLayerStoreToLayerRaster` already
 * repairs ownership by dropping the store and telling the user; this only guards
 * against a throw from that repair itself (an unreachable bridge, say), which
 * would otherwise leave the document with two disagreeing owners AND no signal
 * at all - the exact failure mode that shipped the stale-store bug in the first
 * place. Returns the ids that could not be brought current.
 */
export async function syncRewrittenRastersToStores(
  docId: string,
  engine: StoreCurrencyEngine,
  rewritten: RewrittenRaster[],
): Promise<string[]> {
  const unrepaired: string[] = [];
  for (const r of rewritten) {
    try {
      await syncLayerStoreToLayerRaster(docId, engine, r.id, r.rgba);
    } catch (err) {
      unrepaired.push(r.id);
      reportStoreCurrencyFailure(
        `A layer's pixel store could not be repaired after an edit (${String(err)}). That layer may reject painting until it is re-seeded.`,
      );
    }
  }
  return unrepaired;
}