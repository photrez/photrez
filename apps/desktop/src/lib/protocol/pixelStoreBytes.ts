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
 */
export async function getPixelStoreBytes(docId: string): Promise<PixelStoreBytes> {
  const raw = (await invoke("rust_pixels_store_bytes", { docId })) as
    | Partial<PixelStoreBytes>
    | null;
  const layer_count = requireNumber(raw?.layer_count, CONTEXT);
  const row_major_bytes = requireNumber(raw?.row_major_bytes, CONTEXT);
  const total_bytes = requireNumber(raw?.total_bytes, CONTEXT);
  const tile_graph = requireTileGraph(raw?.tile_graph, CONTEXT);
  if (total_bytes !== row_major_bytes + tile_graph.total_bytes) {
    throw new Error(`${CONTEXT}: total_bytes is not row_major + tile_graph.total_bytes`);
  }
  if (tile_graph.shared_bytes + tile_graph.private_bytes !== tile_graph.total_bytes) {
    throw new Error(`${CONTEXT}: shared + private does not equal tile_graph.total_bytes`);
  }
  return { layer_count, row_major_bytes, tile_graph, total_bytes };
}
