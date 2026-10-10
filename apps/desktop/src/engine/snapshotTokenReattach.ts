// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * Snapshot-token payloads for the history bridge's snapshot records.
 *
 * Extracted from `history.ts` so the payload DTOs sit in one file instead of
 * two hundred lines of the history class. `CommandHistory` still OWNS the
 * payloads it builds (`buildSnapshotPayload`); it imports the DTOs from here
 * so the two ends cannot disagree about a field.
 */

// ── Snapshot-token payloads (mirror the rust core DocumentSnapshot/LayerSnapshot
//    DTOs so the Tauri commands round-trip 1:1, camelCase on the wire) ──

/** One layer's metadata + opaque bitmap token in a document snapshot. */
export interface SnapshotLayerMeta {
  layerId: string;
  width: number;
  height: number;
  bitmapToken: string | null;
  epoch: number;
  pixelVersion: number;
}

/** A document's atomic metadata + bitmap-token snapshot (no pixels). */
export interface SnapshotPayload {
  docId: string;
  version: number;
  layers: SnapshotLayerMeta[];
}

