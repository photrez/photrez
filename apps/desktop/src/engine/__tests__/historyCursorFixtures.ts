// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * The tile mementos a host commit really produces, and the two ways one reaches
 * the Rust stream.
 *
 * Extracted from `historyCursorParity.wiring.test.ts` so the wire shape - the part
 * the real `apply_tile_patch` deserializes - has one home instead of being
 * restated per case. Test-only and under `__tests__`, so the static writer census
 * does not scan it.
 */
import { vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { CommandHistory, HistoryTilePatches } from "../history";
import type { DocumentModel } from "../types";

/**
 * A tile memento in the SHAPE THE HOST REALLY SENDS: `TileUploadLike` =
 * `{x, y, width, height, data}`. It is deliberately not `{w, h}`, because that is
 * the only shape any producer builds (useBrushOverlay re-maps Rust's `{w, h}`
 * answer to `width`/`height` before committing), and a test that used `w`/`h`
 * would be asserting against a payload the app never produces.
 *
 * The real command accepts this spelling: `TilePatchWire` aliases `width`/`height`
 * onto `w`/`h`, pinned in Rust by
 * `both_tile_shapes_are_accepted_at_the_wire_and_mint_exactly_one_entry`, which
 * drives both spellings through Tauri v2's own argument deserializer.
 */
export const hostTile = (x: number, y: number, rgba: number[]) => ({
  x,
  y,
  width: 2,
  height: 2,
  data: new Uint8ClampedArray(rgba),
});

/** A TS-owned tile entry: the shape a text/shape/transform commit produces. */
export const makePatches = (): HistoryTilePatches => ({
  layerId: "l1",
  surfaceWidth: 10,
  surfaceHeight: 10,
  before: [hostTile(0, 0, [1, 1, 1, 255, 1, 1, 1, 255, 1, 1, 1, 255, 1, 1, 1, 255])],
  after: [hostTile(0, 0, [2, 2, 2, 255, 2, 2, 2, 255, 2, 2, 2, 255, 2, 2, 2, 255])],
});

/** The same memento marked Rust-owned: a brush-shaped twin, not a pixel source. */
export const makeRustOwnedPatches = (): HistoryTilePatches => ({
  ...makePatches(),
  rustOwned: true,
});

/**
 * Commit the way a BRUSH stroke does: the canonical, ungated writer
 * `rust_pixels_write_region` records the Pixel entry, and the host then commits the
 * twin with `alreadyRecordedInRust`, so the bridge records nothing itself.
 *
 * `history.commit(model, "Text", makePatches(), false)` is the OTHER tile shape -
 * a TS-owned entry whose only bridge arm is `apply_tile_patch`, which accepts the
 * host's `{width, height}` and records its own Pixel entry, so that pop steps too.
 */
export async function commitRustOwnedPaint(history: CommandHistory, model: DocumentModel) {
  await vi.mocked(invoke)("rust_pixels_write_region", { docId: model.id, layerId: "l1" });
  history.commit(model, "Brush", makeRustOwnedPatches(), true);
}