// SPDX-License-Identifier: AGPL-3.0-or-later
//
// TypeScript caller for the read-only history-depth probe
// (`rust_pixels_history_depth`). Contract: map every returned field explicitly
// and turn any failure into a REJECTED promise - an error must never arrive as
// a zero-filled depth struct, because a caller that sees `total_depth: 0` would
// conclude "no history exists" from a failed IPC call.

import { invoke } from "@tauri-apps/api/core";

export interface PixelHistoryDepth {
  total_depth: number;
  undo_depth: number;
  redo_depth: number;
  affected_layer_ids: string[];
}

/** Read a document's history depth. Rejects (never resolves zero depths) when
 *  the command is unregistered, the store is uninitialized, or the doc_id is
 *  empty/whitespace/unknown/oversize - all of which are Rust-side Err(String)
 *  rejections. */
export async function getPixelHistoryDepth(docId: string): Promise<PixelHistoryDepth> {
  const raw = (await invoke("rust_pixels_history_depth", { docId })) as Partial<PixelHistoryDepth> | null;
  if (
    raw == null ||
    typeof raw.total_depth !== "number" ||
    typeof raw.undo_depth !== "number" ||
    typeof raw.redo_depth !== "number" ||
    !Array.isArray(raw.affected_layer_ids)
  ) {
    throw new Error("rust_pixels_history_depth: malformed depth response");
  }
  return {
    total_depth: raw.total_depth,
    undo_depth: raw.undo_depth,
    redo_depth: raw.redo_depth,
    affected_layer_ids: raw.affected_layer_ids,
  };
}
