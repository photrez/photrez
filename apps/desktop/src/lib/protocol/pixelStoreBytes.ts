// SPDX-License-Identifier: AGPL-3.0-or-later
//
// TypeScript caller for the read-only pixel-store byte probe
// (`rust_pixels_store_bytes`). Contract: map every returned field explicitly and
// turn any failure into a REJECTED promise - an error must never arrive as a
// zero-byte report, because a caller that sees `total_bytes: 0` would conclude
// "this document costs nothing" from a failed IPC call.
//
// The wire shape is snake_case because `PixelStoreBytes` carries no serde
// rename attribute; every field is read by its Rust name deliberately, so a
// rename on either side reddens this wrapper rather than silently reading
// `undefined`.

import { invoke } from "@tauri-apps/api/core";

/** One document's byte footprint, as the Rust probe reports it. */
export interface PixelStoreBytes {
  layer_count: number;
  row_major_bytes: number;
  tile_graph: {
    total_bytes: number;
    shared_bytes: number;
    private_bytes: number;
    tile_count: number;
    state_count: number;
    tile_reference_count: number;
  };
  total_bytes: number;
  /**
   * DERIVED, NOT MEASURED: `width*height*4` for each layer that has never been
   * committed, so its packed canon does not exist yet. It is the anchor the
   * first commit WILL pack - not memory in use, and never to be added into a
   * footprint figure as though it were.
   *
   * A seeded, never-committed document reports a tile graph of zero here, so
   * without this figure `total_bytes` reads as a complete footprint when the
   * store would hold twice that once the first stroke lands. Non-zero means the
   * report is INCOMPLETE, not that the document is cheap.
   */
  owed_anchor_bytes: number;
  /** Layers counted in `owed_anchor_bytes`. */
  owed_anchor_layer_count: number;
}

const CONTEXT = "rust_pixels_store_bytes: malformed byte report";

function requireNumber(value: unknown, context: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new Error(context);
  }
  return value;
}

function requireTileGraph(raw: unknown, context: string): PixelStoreBytes["tile_graph"] {
  const g = raw as Partial<PixelStoreBytes["tile_graph"]> | null;
  if (g == null) throw new Error(context);
  return {
    total_bytes: requireNumber(g.total_bytes, context),
    shared_bytes: requireNumber(g.shared_bytes, context),
    private_bytes: requireNumber(g.private_bytes, context),
    tile_count: requireNumber(g.tile_count, context),
    state_count: requireNumber(g.state_count, context),
    tile_reference_count: requireNumber(g.tile_reference_count, context),
  };
}

/**
 * Read a document's pixel-store byte footprint. Rejects (never resolves a
 * zero-byte report) when the command is unregistered, the store is
 * uninitialized, or the doc_id is empty/whitespace/unknown/oversize - all of
 * which are Rust-side `Err(String)` rejections.
 *
 * The split is `shared_bytes` = tiles two or more committed states reference and
 * `private_bytes` = tiles one state owns.
 *
 * The two arithmetic identities Rust guarantees are checked too, because a
 * response can be well-typed and still wrong: a field renamed on the Rust side
 * arrives as `undefined`, which `requireNumber` catches, but a field whose value
 * was transposed between two numbers does not. `total === row_major +
 * tile_graph.total` and `shared + private === tile_graph.total` both hold by
 * construction in Rust (`private_bytes` is computed as `total - shared`), so
 * violating either means the numbers were crossed in transit, not that the store
 * is in a strange state.
 *
 * The pending fields are read and required for the same reason, and with more
 * force: a response that omits them is a response from a build that cannot say
 * whether a layer's tile graph exists yet, and treating a missing figure as zero
 * is how a half-built store read as a cheap one. `requireNumber` rejects it
 * rather than defaulting.
 */
export async function getPixelStoreBytes(docId: string): Promise<PixelStoreBytes> {
  const raw = (await invoke("rust_pixels_store_bytes", { docId })) as
    | Partial<PixelStoreBytes>
    | null;
  const layer_count = requireNumber(raw?.layer_count, CONTEXT);
  const row_major_bytes = requireNumber(raw?.row_major_bytes, CONTEXT);
  const total_bytes = requireNumber(raw?.total_bytes, CONTEXT);
  const owed_anchor_bytes = requireNumber(raw?.owed_anchor_bytes, CONTEXT);
  const owed_anchor_layer_count = requireNumber(raw?.owed_anchor_layer_count, CONTEXT);
  const tile_graph = requireTileGraph(raw?.tile_graph, CONTEXT);
  if (total_bytes !== row_major_bytes + tile_graph.total_bytes) {
    throw new Error(`${CONTEXT}: total_bytes is not row_major + tile_graph.total_bytes`);
  }
  if (tile_graph.shared_bytes + tile_graph.private_bytes !== tile_graph.total_bytes) {
    throw new Error(`${CONTEXT}: shared + private does not equal tile_graph.total_bytes`);
  }
  // Two one-way guards, each satisfied by EVERY real Rust response and each
  // catching a pair Rust cannot produce.
  //
  // Both are deliberately one-way. `owed_anchor_bytes` is `width*height*4` per
  // unbuilt layer, so a document whose only unbuilt layer is 0x0 reports one
  // owed layer and ZERO owed bytes - legitimately. Guarding that direction would
  // reject a real 0x0 document, which is reachable because `PixelLayer::new`
  // accepts 0x0 with an empty buffer.
  if (owed_anchor_bytes > 0 && owed_anchor_layer_count === 0) {
    // Bytes with nothing to owe them: the two fields arrived swapped, or one of
    // them was fabricated in transit.
    throw new Error(
      `${CONTEXT}: owed_anchor_bytes is ${owed_anchor_bytes} but owed_anchor_layer_count is 0`,
    );
  }
  if (owed_anchor_layer_count > layer_count) {
    // Unbuilt layers are a SUBSET of the document's layers, so this can only be
    // a transposed or invented count.
    throw new Error(
      `${CONTEXT}: owed_anchor_layer_count ${owed_anchor_layer_count} exceeds layer_count ${layer_count}`,
    );
  }
  return {
    layer_count,
    row_major_bytes,
    tile_graph,
    total_bytes,
    owed_anchor_bytes,
    owed_anchor_layer_count,
  };
}
