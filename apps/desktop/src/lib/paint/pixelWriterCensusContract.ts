// SPDX-License-Identifier: AGPL-3.0-or-later

import { CENSUS_COMMANDS } from "@/lib/protocol/pixelInvokeCensus";

/**
 * THE SINGLE-WRITER CENSUS CONTRACT.
 *
 * "One canonical pixel owner" is only enforceable if the set of writers is
 * declared. This module is that declaration, and it is deliberately SEPARATE from
 * the census test that enforces it: a test file that both declares and verifies
 * its own list can be edited to agree with itself, whereas here each declaration
 * sits beside the reasoning that justifies it, and adding a role is a visible
 * production-source change a reviewer has to read.
 *
 * FOUR roles, and why each command has exactly one. The two that matter most for
 * "one writer" are WRITER and PROJECTION: together they are every way bytes can
 * enter the store, and the difference between them is WHO OWNS THE HISTORY STEP.
 *
 *   WRITER       - writes bytes AND records the canonical history entry, so one
 *                  gesture is one undoable step. Every raster-producing operation
 *                  must land here exactly once.
 *   PROJECTION   - replaces the store's buffer wholesale and records NOTHING,
 *                  because the host step that caused the rewrite already owns the
 *                  history entry (a crop's raster rewrite, a layer's removal, the
 *                  ensure-if-absent seed). These are the only sanctioned ways to
 *                  write the store outside a history entry, and each is pinned by
 *                  the store-currency suite.
 *   CURSOR_MOVER - restores bytes from an entry that ALREADY exists, so it can
 *                  never originate a step. "At most once per direction per step"
 *                  is its contract, not "exactly once per gesture".
 *   READ         - returns bytes or metadata and mutates nothing.
 *
 * A fifth set, ENTRY_ONLY, covers commands that record a history entry carrying NO
 * pixel delta (`rust_pixels_record_external` mirrors a host metadata mutation onto
 * the shared cursor). They are history steps, so they ARE census-routed - they
 * just are not pixel writers.
 *
 * Adding a command to WRITER or PROJECTION is a decision that must be argued in a
 * review. The census test fails on anything in production source that is not
 * declared here, so an undeclared writer cannot ship.
 */

/**
 * Read-only probes. They mutate nothing, so they are NOT writers, and they are
 * deliberately OUTSIDE the invoke census for the same reason: a drain that counted
 * probe traffic would report a state change where only a read happened. Declared
 * BY NAME so a genuine new reader cannot be silently treated as a writer.
 *
 * `rust_pixels_history_tip` is the cursor-parity probe: it reports one document's
 * stream depth plus the payload kind each direction would consume, so the host's
 * own undo depth can be compared against this cursor. It reads the same
 * `&self`-only accessor every depth read uses, and a second call on an unchanged
 * document is byte-identical (pinned in `pixel_history_depth.rs`), so it belongs
 * here by the same rule as the other reads and NOT on the census.
 */
const READS = new Set<string>([
  "rust_pixels_get_epoch",
  "rust_pixels_snapshot_layer",
  "rust_pixels_snapshot_tile",
  "rust_pixels_history_depth",
  "rust_pixels_history_tip",
  "rust_pixels_open_document",
  "rust_pixels_close_document",
]);

/**
 * Canonical writers: bytes plus the history entry, one per gesture.
 *
 * `rust_pixels_write_region` is THE canonical command. `apply_tile_patch` is its
 * recognised sibling - the bridge-shaped form of the same single step, used by the
 * history bridge's pixel arm for a TS pixel op Rust has not recorded yet, and
 * guarded by `alreadyRecordedInRust` so it never double-counts one already written
 * through `rust_pixels_write_region`.
 * `rust_pixels_record_snapshot` records a `Snapshot` entry carrying whole-layer
 * payloads: bytes, but not a `Pixel` entry.
 */
const WRITERS = new Set<string>([
  "rust_pixels_write_region",
  "apply_tile_patch",
  "rust_pixels_record_snapshot",
]);

/**
 * Projections: a whole-buffer replace under a host-owned history entry.
 *
 * `rust_pixels_init` is the ensure-if-absent seed (the first raster op on a layer,
 * seeded from the pixels the surface already holds).
 * `rust_pixels_resize_layer` is the store-currency repair after a dimension
 * change; it deliberately DROPS that layer's pixel history, because a
 * stale-dimension tile patch must never replay onto a new grid.
 * `rust_pixels_remove_layer` is the ownership repair when a resize cannot land: it
 * drops the store so the documented "no store means the bitmap is the source of
 * truth" contract is restored.
 */
const PROJECTIONS = new Set<string>([
  "rust_pixels_init",
  "rust_pixels_resize_layer",
  "rust_pixels_remove_layer",
]);

/** Cursor movers: they restore bytes from an existing entry, never creating one. */
const CURSOR_MOVERS = new Set<string>(["rust_pixels_undo", "rust_pixels_redo"]);

/**
 * History steps carrying no pixel delta. `rust_pixels_record_external` mirrors a
 * host metadata mutation onto the shared cursor so mixed history keeps one
 * position; the pixels themselves are restored host-side. Census-routed because it
 * IS a step - omitting it would let an ordering assertion read a gap where a step
 * happened.
 */
const ENTRY_ONLY = new Set<string>(["rust_pixels_record_external"]);

export const CENSUS_COMMANDS_FOR_TEST = {
  isDeclared: (command: string): boolean =>
    WRITERS.has(command) ||
    PROJECTIONS.has(command) ||
    CURSOR_MOVERS.has(command) ||
    ENTRY_ONLY.has(command) ||
    READS.has(command),
  isProjection: (command: string): boolean => PROJECTIONS.has(command),
  isRead: (command: string): boolean => READS.has(command),
  isCursorOnly: (command: string): boolean => CURSOR_MOVERS.has(command),
  isEntryOnly: (command: string): boolean => ENTRY_ONLY.has(command),
  declaredWriters: (): string[] => [...WRITERS].sort(),
  /**
   * Commands the invoke census records, read from the census module rather than
   * restated: the census test asserts every one of them is classified, so this
   * contract and the census's own set cannot drift apart unnoticed.
   */
  censusCommands: (): string[] => [...CENSUS_COMMANDS].sort(),
  /**
   * Whether a command is routed through the invoke census. The census wrapper is
   * what installs the monotonic ordering oracle; a history step that bypassed it
   * would be invisible to every ordering assertion the closed paths rely on.
   *
   * PROJECTIONS are the deliberate exception: seeding, a store-currency reseed and
   * the ownership repair record no step, and the census exists to order steps.
   */
  isCensusRouted: (command: string): boolean => CENSUS_COMMANDS.has(command),
} as const;