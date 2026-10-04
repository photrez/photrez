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
 *    in Rust by `both_tile_shapes_are_accepted_at_the_wire_and_mint_exactly_one_entry`).
 *  - `rust_pixels_write_region` appends a `Pixel` entry, cursor = len. The
 *    canonical writer is ungated, so this is the only arm that records a Pixel
 *    entry for ordinary painting.
 *  - `apply_tile_patch` ACCEPTS BOTH tile spellings and appends a `Pixel` entry,
 *    cursor = len. `TilePatchWire` is `{x, y, w, h, data}` with
 *    `#[serde(alias = "width")]` / `#[serde(alias = "height")]` on the
 *    dimensions, so the host's `TileUploadLike` = `{x, y, width, height, data}`
 *    deserializes into the same struct and mints the entry the cursor steps. A
 *    tile carrying NEITHER spelling is still rejected (see `tilesSurviveTheWire`),
 *    which is the boundary this emulator has to keep modelling in both directions.
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
 * ORDERING - what this models and what it does not. `rust_pixels_undo`,
 * `rust_pixels_redo` and `rust_pixels_history_tip` all queue on the SAME
 * `PixelStoreRegistry` mutex in the real backend, so the order two of them land in
 * is a genuine race. Both cursor arms here therefore yield ONE macrotask before
 * touching the stream, so a move becomes observable only after the round trip has
 * been issued, and a read issued too early observes the PRE-move cursor. That is
 * the race that made the tile branch's unsequenced parity probe report a spurious
 * `diverged`; moving at invoke time hid it behind a green suite.
 *
 * NOT MODELLED: which of two concurrently queued calls wins the mutex. This
 * emulator answers in invocation order after an equal delay, so a test can assert
 * the production code SEQUENCES its read behind its own step (which is the fix and
 * is deterministic), but it cannot prove the fix survives an adversarial mutex.
 * That residual is stated where the probe's other limits live
 * (`historyCursorParity.ts`).
 *
 * ACCEPTANCE ASYMMETRY THIS DOES NOT MODEL. Two shapes reach this file's guard
 * differently than the real command treats them:
 *  - a tile carrying BOTH `w` and `width` - rejected here too, as a duplicate field;
 *  - a tile whose `w`/`h` are present but NOT numeric - rejected here as a missing
 *    dimension, where serde would reject it as a type error. Both are payloads no
 *    production producer emits (`history.commit` types its mementos
 *    `TileUploadLike[]`, all numeric), so neither divergence is reachable from the
 *    app; they are listed so a reader does not mistake "accepted" for "anything goes".
 *
 * IN LOCKSTEP: `apply_tile_patch`'s tile shape is pinned on BOTH sides of the wire
 * - by `both_tile_shapes_are_accepted_at_the_wire_and_mint_exactly_one_entry` in
 * apps/desktop/src-tauri/src/paint_parity_cmds.rs (which drives both spellings
 * through Tauri v2's real argument deserialization) and by
 * `tilesSurviveTheWire` here. If that contract ever changes, THIS file must change
 * with it: an emulator that accepts what the runtime rejects - or rejects what it
 * accepts - is a silent lie and every case here stops proving anything.
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
function step(docId: string, direction: "undo" | "redo"): unknown[] | null {
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
 *
 * `TilePatchWire` declares `w`/`h` and ALIASES the host's `width`/`height` onto
 * them, so serde accepts either spelling and rejects a tile that carries NEITHER.
 * A tile carrying BOTH spellings is also rejected, as serde's derive treats a field
 * and its alias as one field name and reports a duplicate (pinned in Rust by the
 * both-spellings half of
 * `both_tile_shapes_are_accepted_at_the_wire_and_mint_exactly_one_entry`). The
 * guard is kept rather than dropped: an emulator that accepted anything would
 * silently diverge from a command that rejects two of those three shapes, which is
 * the mock-fidelity bug AGENTS.md names.
 */
function tilesSurviveTheWire(tiles: unknown): boolean {
  if (!Array.isArray(tiles)) return true;
  return tiles.every((t) => {
    const tile = t as { w?: unknown; h?: unknown; width?: unknown; height?: unknown };
    const hasShort = typeof tile.w === "number" && typeof tile.h === "number";
    const hasLong = typeof tile.width === "number" && typeof tile.height === "number";
    // Both spellings is the duplicate-field case, not a "pick one".
    if (hasShort && hasLong) return false;
    return hasShort || hasLong;
  });
}

/** One macrotask. Used by the cursor arms to model the IPC round trip. */
const tick = () => new Promise<void>((r) => setTimeout(r, 0));

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
      // ONE MACROTASK BEFORE THE MOVE (`setTimeout(0)`, see `tick` above), and the
      // move itself lands synchronously as the first statement of this
      // continuation - i.e. the cursor becomes observable only once the step's
      // promise is ABOUT TO resolve, never before the round trip has been issued.
      //
      // This is what makes a real race VISIBLE. `rust_pixels_undo` and
      // `rust_pixels_history_tip` queue on the same `PixelStoreRegistry` mutex in
      // the app, so a probe that reads without sequencing behind its own pop's step
      // can be served by whichever call wins - and the pre-fix tile branch did
      // exactly that for a bridge-ON TS-owned tile pop. Moving at invoke time, as
      // this emulator used to, erased that race by construction and hid the defect
      // behind a green suite.
      await tick();
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
      // Same shape on the READ side: a read issued before the step's move has
      // landed observes the pre-step cursor, exactly as the real registry would.
      await tick();
      return tipFor(docId);
    default:
      throw new Error(`emulator: unhandled command ${cmd}`);
  }
}
