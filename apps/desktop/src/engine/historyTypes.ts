import type { DocumentModel, LayerNode } from "./types";
import type { TileUploadLike } from "../renderer/types";
import { bitmapStoreFor, existingTokenForBitmap } from "./bitmapStore";

/**
 * Imperative before/after tile patches for a paint commit.
 * Data-shaped (not closures) so multi-step history UIs can replay them later.
 */
export interface HistoryTilePatches {
  layerId: string;
  surfaceWidth: number;
  surfaceHeight: number;
  /** Pre-stroke pixels of every touched tile (undo direction). */
  before: TileUploadLike[];
  /** Post-stroke pixels of every touched tile (redo direction). */
  after: TileUploadLike[];
  /**
   * Set when Rust ALREADY recorded this pixel step (rust_pixels_write_region
   * succeeded). The entry is then a cursor token for a step Rust holds, not a
   * second copy of it: the undo/redo dispatch takes the pixels from Rust and
   * refuses to replay `before`/`after`, which no store has. Entries Rust does
   * not own keep replaying their own tiles.
   */
  rustOwned?: boolean;
}

/**
 * Release GPU/heap-backed ImageBitmaps held by a discarded snapshot. An
 * ImageBitmap is closed only when no other snapshot in the undo/redo stacks
 * and no live document model still references it — shared bitmaps across
 * snapshots must survive eviction of one entry.
 *
 * Coordination with the BitmapStore: a token-registered bitmap is released
 * through `store.release(token)` (closes exactly once); a bitmap that was never
 * token-registered is closed directly. A still-referenced or live bitmap is not
 * closed by either path. With the history bridge OFF no token is ever registered,
 * so every bitmap takes the direct-close path — identical to default behavior.
 */
export function disposeSnapshot(
  snap: DocumentModel,
  stillReferenced: (bitmap: unknown) => boolean,
): void {
  const store = bitmapStoreFor(snap.id);
  const closeIfUnused = (layer: LayerNode) => {
    if (layer.imageBitmap && !stillReferenced(layer.imageBitmap)) {
      const token = existingTokenForBitmap(layer.imageBitmap);
      if (token && store.get(token) === layer.imageBitmap) {
        store.release(token);
      } else {
        try { layer.imageBitmap.close(); } catch { /* already closed */ }
      }
    }
    if (layer.baseImageBitmap && !stillReferenced(layer.baseImageBitmap)) {
      const token = existingTokenForBitmap(layer.baseImageBitmap);
      if (token && store.get(token) === layer.baseImageBitmap) {
        store.release(token);
      } else {
        try { layer.baseImageBitmap.close(); } catch { /* already closed */ }
      }
    }
  };
  for (const layer of snap.layers) closeIfUnused(layer);
}

export interface SnapshotEntry {
  snapshot: DocumentModel;
  timestamp: number;
  lastPaintCoords: { x: number; y: number } | null;
  label?: string;
  /** Tile patches (paint commits). Presence marks an imperative entry. */
  imperative?: HistoryTilePatches;
  /**
   * "snapshot" marks an entry recorded via `recordSnapshotHistory` (before+after
   * states). Undefined = a plain commit (External/metadata/pixel). The undo/redo
   * caller gates the Rust snapshot re-attach on this so a MIXED entry stream
   * ([External, Snapshot]) never re-attaches a wrong-step bitmap on an External
   * step — Rust snapshot cursor only steps for Snapshot entries.
   */
  snapshotType?: "snapshot";
  /**
   * Pixel-layer allowlist for the upload narrow in useEditorCommands.
   * null/undefined = unknown, keep the identity-map fallback. [] =
   * provably metadata-only, skip every upload even when the restored
   * bitmap object differs. [ids] = only those layers are candidates
   * (the identity check still applies to each).
   */
  pixelLayerIds?: string[] | null;
  /**
   * This entry's own Rust record was REJECTED, so nothing exists in the stream behind
   * it while the shim stood down in good faith on the recorder flag.
   *
   * Per entry, deliberately, and not a per-history latch. A latch poisons every LATER
   * pop too: the failure is routine (a bridge record is fire-and-forget and can be
   * rejected for reasons that have nothing to do with the next entry), and demoting an
   * unrelated entry onto the gated path makes it REFUSE a tip that was perfectly
   * correct - which stops the cursor advancing at all. One failed record must affect
   * exactly one entry.
   *
   * Set from the record's own `.catch`, so it is always after the push - the pop reads
   * the same object. The race is the one the fire-and-forget record already has: a pop
   * that lands before the rejection arrives has already issued its step, and the marker
   * is then simply never consulted for that entry.
   */
  rustRecordFailed?: true;
}
