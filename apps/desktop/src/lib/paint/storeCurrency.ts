// SPDX-License-Identifier: AGPL-3.0-or-later

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
 * Returns true when a store was reseeded.
 */
export async function syncLayerStoreToLayerRaster(
  docId: string,
  engine: StoreCurrencyEngine,
  layerId: string,
  /** Exact RGBA bytes of the post-rewrite raster, when the producer has them. */
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

  const bytes = rgba ?? readbackBitmap(layer.imageBitmap, layer.width, layer.height);
  // The cached surface is derived from the raster this write replaces; drop it
  // first so a concurrent paint op rebuilds from the new tile grid.
  engine.invalidatePaintSurface(layerId);
  await invoke("rust_pixels_resize_layer", {
    docId,
    layerId,
    width: layer.width,
    height: layer.height,
    bytes,
  });
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
 * pass. Failures are per-layer and non-fatal: a store that cannot be reseeded
 * leaves that layer's bitmap authoritative, which is the documented fallback.
 */
export async function syncRewrittenRastersToStores(
  docId: string,
  engine: StoreCurrencyEngine,
  rewritten: RewrittenRaster[],
): Promise<void> {
  for (const r of rewritten) {
    try {
      await syncLayerStoreToLayerRaster(docId, engine, r.id, r.rgba);
    } catch (err) {
      console.warn(`[store-currency] could not reseed the pixel store for ${r.id}:`, err);
    }
  }
}