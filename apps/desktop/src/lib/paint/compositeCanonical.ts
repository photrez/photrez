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
 * Prerequisite 3 - the history entry itself - is deliberately NOT done here, and
 * the reason is structural rather than an omission. These ops change the layer
 * VECTOR, not just a raster: merge down destroys two layers and mints one,
 * flatten destroys every layer and mints one. An undo of such a step is a
 * STRUCTURAL restore (the engine must resurrect N layers, in order, with their
 * rasters), and the tile-memento fast path in `useEditorCommands` deliberately
 * SKIPS `engine.restore` for entries that carry imperative patches. Marking
 * these steps `rustOwned` would therefore route undo down the paint fast path
 * and the resurrected layer vector would never happen. The history entry stays a
 * TypeScript snapshot; the Rust store is brought into agreement at op time and
 * re-agrees on both sides of the undo because `DocumentEngine.restore` already
 * reseeds every layer whose dimensions moved and drops the store of every layer
 * that vanished.
 *
 * NO GPU BYTES ARE INVOLVED. The composite is produced by a CPU canvas and read
 * back through `readbackBitmap`'s exact path, so the defect class that damaged
 * the gradient (WebGL output captured into history) cannot apply here.
 */
import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";
import type { WebGL2Backend } from "@/renderer/webgl2";
import { clampRegionToLayer } from "@/lib/paint/regionProducer";
import { syncFacadeVersionFromPixel } from "@/lib/protocol/facadeRegistry";

/** One composite destination, as the caller measured it. */
export interface CompositeDestination {
  /** The layer the composite was installed on. */
  layerId: string;
  width: number;
  height: number;
}

export type CompositeSeedResult =
  | { status: "CONVERGED"; epoch: number; version: number }
  | { status: "SKIPPED"; reason: string }
  | { status: "FAILED"; reason: string };

/**
 * Seed the Rust pixel store for a composite destination layer and record the
 * composite as one canonical `Pixel` entry.
 *
 * Order is load-bearing and mirrors the brush/bucket/fill arms exactly:
 *   1. probe `rust_pixels_get_epoch`; on rejection the layer is NEW (a merged
 *      layer always is), so seed it with the composite bytes;
 *   2. `rust_pixels_write_region` over the whole layer, which is the single
 *      canonical write and the single history entry;
 *   3. stamp the epoch the write actually produced.
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

    // One canonical write = one Pixel entry. The whole layer, because a
    // composite destination has no prior content to diff against.
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

    // Only stamp an epoch the store actually reported.
    layer.bitmapEpoch = res.epoch;
    syncFacadeVersionFromPixel(docId, res.version);
    renderer?.uploadImage?.(layerId, layer.imageBitmap);
    engine.notifyVisualChange();
    return { status: "CONVERGED", epoch: res.epoch, version: res.version };
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