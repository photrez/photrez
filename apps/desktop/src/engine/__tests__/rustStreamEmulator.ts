// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * The Rust pixel-history stream, emulated for the parity tests.
 *
 * Extracted from `historyCursorParity.wiring.test.ts` so the emulator's rules sit
 * in one place and the test file stays readable; it is test-only and lives under
 * `__tests__`, so the static writer census does not scan it.
 *
 * EVERY rule here is a real behaviour of `PixelStoreRegistry` / `ProtocolEngine`,
 * not a convenience:
 *
 *  - `rust_pixels_record_external` appends an `External` entry, cursor = len. Its
 *    args are all scalars, so serde accepts it and it really does record (asserted
 *    in Rust by `host_tile_shape_is_rejected_at_the_wire_and_records_nothing`).
 *  - `rust_pixels_write_region` appends a `Pixel` entry, cursor = len. The
 *    canonical writer is ungated, so this is the only arm that records a Pixel
 *    entry for ordinary painting.
 *  - `apply_tile_patch` REJECTS the host's tile shape. `TilePatchWire` is
 *    `{x, y, w, h, data}` with no serde alias, and the host sends `TileUploadLike`
 *    = `{x, y, width, height, data}`, so serde answers `missing field \`w\`` and
 *    mints NOTHING. Accepting it here would be the mock-fidelity bug AGENTS.md
 *    names: a mock that takes what the real runtime refuses hides the rejection
 *    behind a green suite. An empty array deserializes fine, so it is accepted.
 *  - `rust_pixels_undo` / `rust_pixels_redo` MOVE THE CURSOR for both modelled
 *    kinds. `ProtocolEngine::undo_pixel` steps for an `External` tip
 *    (crates/core/src/history.rs:229-233) exactly as for a `Pixel` tip
 *    (history.rs:219-223); only the tile yield differs - an External step
 *    produces no tiles, and `PixelStoreRegistry::undo_pixel` collapses the empty
 *    `(None, None, None)` to `None` AFTER the cursor has already moved
 *    (pixel_store.rs:730-747). So "produced no tiles" must never be modelled as
 *    "did not move".
 *  - `rust_pixels_history_tip` answers `{total_depth, undo_depth, redo_depth,
 *    undo_tip_kind, redo_tip_kind}` - snake_case fields and camelCase kind strings,
 *    the serialization `HistoryTip` / `PayloadKind` actually produce.
 *
 * `Snapshot` / `Native` entries are NOT modelled: `undo_pixel` leaves the cursor
 * alone for those (history.rs:239) and no command this emulator handles records one.
 *
 * IN LOCKSTEP: if the `apply_tile_patch` tile shape is ever reconciled (a serde
 * alias on `TilePatchWire`, or the host sending `{w, h}` instead of
 * `TileUploadLike`'s `{width, height}`), THIS file must change in the same commit.
 * It mirrors the real rejection, so a one-sided fix turns that rejection into a
 * silent lie and every case here stops proving anything.
 */
import type { RustHistoryTip } from "../historyCursorParity";

export type TipKind = "pixel" | "external";

export interface RustStream {
  entries: TipKind[];
  cursor: number;
  /**
   * The tile each Pixel entry would yield, indexed with `entries`. Real
   * `undo_pixel` returns the entry's tiles for a Pixel tip and NONE for an
   * External tip, and that difference is load-bearing: the host's tile path treats
   * "no tiles" as the refusal branch for a Rust-owned entry
   * (`useEditorCommands`), so an emulator that returned `tiles: []` for a Pixel tip
   * would model the SUCCESS path as the refusal. External entries carry null.
   */
  payloads: ({ x: number; y: number; w: number; h: number; data: number[] } | null)[];
  /** Bumped by every cursor move, like `ProtocolEngine.version`. */
  version: number;
}

/** Per-document stream, mirroring PixelStoreRegistry's per-doc history. */
const streams = new Map<string, RustStream>();

export function resetStreams(): void {
  streams.clear();
}

export function streamFor(docId: string): RustStream {
  let s = streams.get(docId);
  if (!s) {
    s = { entries: [], cursor: 0, payloads: [], version: 0 };
    streams.set(docId, s);
  }
  return s;
}

/** Record an entry the way `record_external` / `write_region` do. */
export function record(docId: string, kind: TipKind): void {
  const s = streamFor(docId);
  s.entries.length = s.cursor; // a new entry drops the redo branch
  s.entries.push(kind);
  // A Pixel entry yields a distinguishable tile; an External entry yields none.
  s.payloads.push(
    kind === "pixel" ? { x: s.entries.length - 1, y: 0, w: 1, h: 1, data: [0, 0, 0, 255] } : null,
  );
  s.cursor = s.entries.length;
  s.version += 1;
}

/**
 * Move the cursor one step. `ProtocolEngine::undo_pixel` steps it for a `Pixel`
 * tip AND for an `External` tip; the difference between the two is only the tile
 * yield, which `undo_pixel` discards for an External step AFTER the move. So the
 * cursor moves for every kind this emulator can hold.
 *
 * Returns the tiles that step yields - the entry's own for a Pixel tip, none for
 * an External tip - or null when there was nothing to step.
 */
export function step(docId: string, direction: "undo" | "redo"): unknown[] | null {
  const s = streamFor(docId);
  const idx = direction === "undo" ? s.cursor - 1 : s.cursor;
  if (idx < 0 || idx >= s.entries.length) return null;
  s.cursor = direction === "undo" ? s.cursor - 1 : s.cursor + 1;
  s.version += 1;
  const payload = s.payloads[idx];
  return s.entries[idx] === "pixel" && payload ? [payload] : [];
}

/** The exact wire shape `rust_pixels_history_tip` serializes. */
export function tipFor(docId: string): RustHistoryTip {
  const s = streamFor(docId);
  return {
    total_depth: s.entries.length,
    undo_depth: s.cursor,
    redo_depth: s.entries.length - s.cursor,
    undo_tip_kind: s.cursor > 0 ? (s.entries[s.cursor - 1] ?? null) : null,
    redo_tip_kind: s.entries[s.cursor] ?? null,
  };
}

/**
 * Does this tile array survive the REAL JSON boundary of `apply_tile_patch`?
 * See the header: a tile without numeric `w`/`h` is rejected by serde.
 */
function tilesSurviveTheWire(tiles: unknown): boolean {
  if (!Array.isArray(tiles)) return true;
  return tiles.every(
    (t) => typeof (t as { w?: unknown }).w === "number" && typeof (t as { h?: unknown }).h === "number",
  );
}

/** One emulator answer, so a test can wrap it instead of re-implementing the stream. */
export async function answerInvoke(cmd: string, args: unknown): Promise<unknown> {
  const a = (args ?? {}) as { docId?: string; layerId?: string; before?: unknown; after?: unknown };
  const docId = (a.docId ?? "doc-1") as string;
  switch (cmd) {
    case "rust_pixels_open_document":
      streamFor(docId);
      return undefined;
    case "rust_pixels_record_external":
      record(docId, "external");
      return { tiles: [], epoch: 0, version: streamFor(docId).version };
    case "rust_pixels_write_region":
      record(docId, "pixel");
      return { before: [], after: [], epoch: 0, version: streamFor(docId).version };
    case "apply_tile_patch":
      if (!tilesSurviveTheWire(a.before) || !tilesSurviveTheWire(a.after)) {
        throw new Error("missing field `w`");
      }
      record(docId, "pixel");
      return { layer_id: a.layerId, tiles: [], epoch: 0, version: streamFor(docId).version };
    case "rust_pixels_undo":
    case "rust_pixels_redo": {
      const dir = cmd === "rust_pixels_undo" ? "undo" : "redo";
      const tiles = step(docId, dir);
      // A step with nothing to consume answers `Ok` with an empty result and the
      // CURRENT version (paint_parity_cmds.rs:186-198) - i.e. it did not move.
      return {
        layer_id: a.layerId,
        tiles: tiles ?? [],
        epoch: 0,
        version: streamFor(docId).version,
      };
    }
    case "rust_pixels_history_tip":
      return tipFor(docId);
    default:
      throw new Error(`emulator: unhandled command ${cmd}`);
  }
}
