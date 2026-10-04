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

/**
 * The three depth numbers every history-depth read carries, validated against a
 * raw wire response. THROWS when any of them is missing or is not a number, so a
 * failed or malformed call can never arrive as a zero-filled depth - a caller
 * that saw depth 0 from a broken call would conclude "no history exists".
 *
 * Shared by both read-only depth callers (`getPixelHistoryDepth` and the
 * cursor-parity probe in `@/engine/historyCursorParity`) because they read the
 * SAME Rust history struct through the same `&self`-only accessor, so the
 * malformed-response rule must not be re-implemented per caller. `context` is the
 * Tauri command name and lands in the thrown message.
 */
export function requireHistoryDepthNumbers(
  raw: unknown,
  context: string,
): { total_depth: number; undo_depth: number; redo_depth: number } {
  const r = raw as Partial<PixelHistoryDepth> | null;
  if (
    r == null ||
    typeof r.total_depth !== "number" ||
    typeof r.undo_depth !== "number" ||
    typeof r.redo_depth !== "number"
  ) {
    throw new Error(`${context}: malformed depth response`);
  }
  return { total_depth: r.total_depth, undo_depth: r.undo_depth, redo_depth: r.redo_depth };
}

/** Read a document's history depth. Rejects (never resolves zero depths) when
 *  the command is unregistered, the store is uninitialized, or the doc_id is
 *  empty/whitespace/unknown/oversize - all of which are Rust-side Err(String)
 *  rejections. */
export async function getPixelHistoryDepth(docId: string): Promise<PixelHistoryDepth> {
  const raw = (await invoke("rust_pixels_history_depth", { docId })) as Partial<PixelHistoryDepth> | null;
  const depths = requireHistoryDepthNumbers(raw, "rust_pixels_history_depth");
  const affected = raw?.affected_layer_ids;
  if (!Array.isArray(affected)) {
    throw new Error("rust_pixels_history_depth: malformed depth response");
  }
  return { ...depths, affected_layer_ids: affected };
}
