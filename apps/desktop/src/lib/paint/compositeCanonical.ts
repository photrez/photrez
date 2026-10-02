// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * CANONICAL SEEDING FOR A COMPOSITE-DESTINATION LAYER.
 *
 * Merge Down, Merge Selected and Flatten Image all produce a NEW layer whose
 * pixels are a CPU `OffscreenCanvas` composite (see `engine/layerComposite`).
 * That composite reaches Rust only through the model, so the Rust pixel store
 * has no entry for the destination layer at all - measured at `d69c387` as
 * `RUST-HAS-NO-ENTRY` for all three, with the projection holding 128x128 /
 * 65536 bytes while the store rejected the read with `layer not initialized`.
 *
 * This closes TWO of the three prerequisites, and the split matters:
 *
 *   1. The store is SEEDED for the destination layer, so it exists and holds
 *      the composite bytes. Convergence becomes measurable instead of impossible.
 *   2. `rust_pixels_write_region` is emitted, so the census records a WRITER
 *      call site for the op - one canonical `Pixel` history entry per merge.
 *
 * Prerequisite 3 - a PIXEL history entry for the destination - is deliberately NOT
 * left behind, and the reason is structural rather than an omission. These ops change
 * the layer VECTOR, not just a raster: merge down destroys two layers and mints one,
 * flatten destroys every layer and mints one. Their undo is therefore a STRUCTURAL
 * restore, and the layer vector lives on the SAME per-document cursor the pixel store
 * writes to. Leaving the canonical write's `Pixel` entry on top of the structural entry
 * makes an undo press step the pixel entry instead, and the structural restore never
 * runs - measured at `b124338` as `[facade-history] Rust took a pixel step with no TS
 * twin to drain undo`. So the write is closed by re-asserting the store from the
 * raster, which drops that entry: the composite's pixels are a projection under the
 * structural step, and the structural entry is the gesture's only cursor step. The
 * structural restore itself is Rust's - the routed command's own entry is undone
 * natively and projected back through `applyFacadeSnapshot`.
 *
 * NO GPU BYTES ARE INVOLVED. The composite is produced by a CPU canvas and read
 * back through `readbackBitmap`'s exact path, so the defect class that damaged
 * the gradient (WebGL output captured into history) cannot apply here.
 */
import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";
import type { WebGL2Backend } from "@/renderer/webgl2";
import { clampRegionToLayer } from "@/lib/paint/regionProducer";
import { syncLayerStoreToLayerRaster } from "@/lib/paint/storeCurrency";
import { syncFacadeVersionFromPixel } from "@/lib/protocol/facadeRegistry";

/** One composite destination, as the caller measured it. */
export interface CompositeDestination {
  /** The layer the composite was installed on. */
  layerId: string;
  width: number;
  height: number;
}

export type CompositeSeedResult =
  | { status: "CONVERGED"; epoch: number; version: number; projected: boolean }
  | { status: "SKIPPED"; reason: string }
  | { status: "FAILED"; reason: string };

/**
 * Seed the Rust pixel store for a composite destination layer and record the
 * composite as one canonical `Pixel` entry.
 *
 * Order is load-bearing and mirrors the brush/bucket/fill arms exactly, with one
 * extra step at the end whose reason is structural:
 *   1. probe `rust_pixels_get_epoch`; on rejection the layer is NEW (a merged
 *      layer always is), so seed it with the composite bytes;
 *   2. `rust_pixels_write_region` over the whole layer, which is the single
 *      canonical write the census records;
 *   3. re-assert the store from the raster through `syncLayerStoreToLayerRaster`,
 *      which is the projection primitive crop's store-currency repair already uses.
 *
 * Step 3 is not tidiness - it is what keeps ONE gesture to ONE cursor step. The
 * per-document cursor is shared between the pixel store and the graph commands
 * (`crates/core/src/pixel_store.rs`: `DocumentPixelStore::history` is the same
 * `ProtocolEngine` the registry hands `protocol_apply_command_native`), and
 * `write_region` opens a `Pixel` entry on it. A merge/flatten/merge-selected
 * records its STRUCTURAL change on that same cursor, so a `Pixel` entry left on top
 * of it is the entry an undo press steps: the handoff claims the undo, returns true,
 * `useEditorCommands` returns early, and the layer vector is never resurrected.
 * `resize_layer` calls `history.invalidate_layer`, which drops the entry - and it is
 * already the established way to say "this layer's raster is now exactly the model
 * raster, wholesale", which is precisely what a composite destination is. Without
 * step 3 the structural undo costs two presses: the first re-applies the composite's
 * own no-op pixel step, the second restores the layers.
 *
 * The seed uses `rust_pixels_init` rather than a zero-filled `write_region`
 * because `init_layer` REPLACES the buffer wholesale and drops the layer's pixel
 * history - which is exactly right for a layer id that was just minted, and
 * wrong for an existing one. A merge destination is always freshly minted, and
 * the probe above is what proves it.
 *
 * Never throws: a failed canonical seed must not lose the user's merge, which
 * already happened in the model by the time this runs.
 */
export async function seedCompositeCanonicalPixels(
  engine: DocumentEngine,
  renderer: WebGL2Backend | undefined,
  destination: CompositeDestination,
): Promise<CompositeSeedResult> {
  const { layerId, width, height } = destination;
  const docId = engine.getId();
  if (!docId || !layerId) return { status: "SKIPPED", reason: "no canonical pixel namespace" };
  if (width <= 0 || height <= 0) {
    return { status: "SKIPPED", reason: `destination is ${width}x${height}` };
  }
  const layer = engine.getLayer(layerId);
  if (!layer?.imageBitmap) {
    return { status: "SKIPPED", reason: "destination carries no raster" };
  }

  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const { pixelInvoke } = await import("@/lib/protocol/pixelInvokeCensus");

    // The composite bytes, read back from the CPU canvas the layer already
    // holds. Never from a GPU readback.
    const canvas = new OffscreenCanvas(width, height);
    const ctx = canvas.getContext("2d");
    if (!ctx) return { status: "SKIPPED", reason: "no 2d context to read the composite" };
    ctx.drawImage(layer.imageBitmap, 0, 0);
    const rgba = new Uint8Array(ctx.getImageData(0, 0, width, height).data.buffer as ArrayBuffer);

    let seeded = true;
    try {
      await invoke("rust_pixels_get_epoch", { docId, layerId });
    } catch {
      // REJECTION IS THE SIGNAL: the real transport rejects for Rust `Err`, so a
      // catch here means "no canonical store for this layer yet" - not a
      // transport failure. A mock that resolved instead would make every merge
      // skip its seed and report CONVERGED with nothing behind it.
      seeded = false;
    }
    if (!seeded) {
      await invoke("rust_pixels_init", { docId, layerId, width, height, bytes: rgba });
    }

    // One canonical write = the census WRITER site for the op. The whole layer,
    // because a composite destination has no prior content to diff against.
    const region = clampRegionToLayer({ x: 0, y: 0, w: width, h: height }, width, height);
    const res = (await pixelInvoke("rust_pixels_write_region", {
      docId,
      layerId,
      x: region.x,
      y: region.y,
      w: region.w,
      h: region.h,
      rgba,
    })) as { epoch: number; version: number };

    // Re-assert the projection: the destination's raster IS the model raster, so its
    // store entry carries no independent cursor step (see the header). This also
    // stamps `bitmapEpoch` from a fresh probe, so it cannot claim a currency the
    // raster does not have after the buffer was replaced.
    const projected = await syncLayerStoreToLayerRaster(
      docId,
      engine,
      layerId,
      new Uint8ClampedArray(rgba),
    );

    // Only stamp a version the write actually reported.
    syncFacadeVersionFromPixel(docId, res.version);
    // The destination's store row is a projection, not a user edit, so the engine
    // retires it when the projection that created the layer goes away (the undo of
    // this structural step). Unmarked here it would outlive its layer and keep
    // serving a retired id.
    engine.markCompositeStoreProjection(layerId);
    renderer?.uploadImage?.(layerId, layer.imageBitmap);
    engine.notifyVisualChange();
    return { status: "CONVERGED", epoch: res.epoch, version: res.version, projected };
  } catch (err) {
    return { status: "FAILED", reason: String(err) };
  }
}

/**
 * Seed every composite destination an op produced, in one pass.
 *
 * Each destination is seeded independently so one failure cannot stop the others
 * - a merge with three destinations and a dead bridge on the second must still
 * converge the third.
 */
export async function seedCompositeDestinations(
  engine: DocumentEngine,
  renderer: WebGL2Backend | undefined,
  destinations: CompositeDestination[],
): Promise<CompositeSeedResult[]> {
  const results: CompositeSeedResult[] = [];
  for (const destination of destinations) {
    results.push(await seedCompositeCanonicalPixels(engine, renderer, destination));
  }
  return results;
}

/** Re-exported so callers can type a history handle without a second import. */
export type { CommandHistory };