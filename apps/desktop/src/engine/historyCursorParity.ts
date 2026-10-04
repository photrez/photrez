// SPDX-License-Identifier: AGPL-3.0-or-later

import { historyBridgeEnabled } from "@/engine/history";
import { requireHistoryDepthNumbers } from "@/lib/protocol/pixelHistoryDepth";

/**
 * TS-vs-Rust history-cursor parity probe. DIAGNOSTIC ONLY.
 *
 * There are two undo stacks per document. TypeScript owns one (`CommandHistory`,
 * counting its own `undoStack`), Rust owns the other (`ProtocolEngine`, whose
 * cursor the `rust_pixels_*` history commands step). Nothing in production
 * compares them, so whether the two point at the same history position is
 * UNMEASURED - a drift between them would be invisible until it corrupted a
 * restore.
 *
 * The probe observes only while the TS->Rust history bridge is RECORDING. With
 * that bridge OFF - the shipping default, since nothing in production sets the
 * `localStorage["photrez.historyBridge"]` key the predicate requires
 * (apps/desktop/src/engine/history.ts `HISTORY_BRIDGE_GATE` / `historyBridgeEnabled`) - the two stacks are not two views of
 * ONE history: the host is deliberately the sole undo authority, and Rust's stream
 * is a partial record of it. It is not empty, though. `CommandHistory.commit`
 * appends `rust_pixels_record_external` / `apply_tile_patch` only inside
 * `bridgeRecordsFor()` (history.ts) - the bridge flag AND an attached doc-id
 * getter - so a metadata step is never recorded - while every PAINT step still is,
 * because the canonical writer `rust_pixels_write_region` is not gated at all
 * (useBrushOverlay.ts:254, and :193-196 states the queued commit reads no flag).
 * So bridge-off means the streams agree on pixel steps and separate by exactly one
 * per metadata step, and every such difference says only "Rust never heard of this
 * host step", which is the configured design rather than a drift. Comparing depths
 * across that boundary would report the design back as a fault.
 *
 * Under the bridge both stacks record every step and the comparison is real. It
 * WAS a drift source here: a metadata undo restored the host model without
 * stepping the Rust cursor at all (the cursor sync lived inside the tile branch
 * of the undo path, which a metadata entry never reaches), so the two cursors
 * separated by one per undone metadata step and the next `rust_pixels_undo`
 * consumed that un-stepped `External` entry - moving the cursor and returning no
 * tiles (proven in Rust, see apps/desktop/src-tauri/src/pixel_history_depth.rs
 * `undo_over_an_external_tip_consumes_it_and_returns_no_tiles`), which silently
 * swallowed the paint step the host had reverted.
 * `CommandHistory.undo()`/`redo()` now own that step, once per popped entry, so
 * both branches of the undo path step and neither steps twice; this probe is
 * what proves it, by reporting `in-sync` where it reported `diverged`.
 *
 * This module measures it and does nothing else. No CALLER branches on the
 * verdict: the undo/redo path fires the observation and forgets it. The only
 * output is one `console.warn` when the two cursors disagree, and one
 * `console.info` when the probe was asked to read but could not - never a
 * mix-up between "the cursors agree" and "the probe is blind". It never moves a
 * cursor, never throws into the caller, and never blocks the undo path.
 */

/** Rust's answer for one document's history cursor, as returned by `rust_pixels_history_tip`. */
export interface RustHistoryTip {
  total_depth: number;
  undo_depth: number;
  redo_depth: number;
  /** Payload kind the next undo would consume; null at cursor 0. */
  undo_tip_kind: string | null;
  /** Payload kind the next redo would consume; null at the stream end. */
  redo_tip_kind: string | null;
}

export type HistoryCursorParityVerdict =
  /** Both sides reported usable, self-consistent numbers and the depths match. */
  | "in-sync"
  /** Both cursors reported usable numbers and they disagree. */
  | "diverged"
  /**
   * No usable comparison was possible: the read failed, the reading was not a
   * well-formed tip, the host depth was unusable, or a newer step superseded this
   * reading before it landed. NEVER reported as `"in-sync"` - a blind probe and
   * an agreeing pair of cursors must stay distinguishable to whoever reads the log.
   */
  | "unknown";

const isUsableDepth = (n: unknown): n is number =>
  typeof n === "number" && Number.isInteger(n) && n >= 0;

/**
 * Compare Rust's reported cursor against the host's undo depth.
 *
 * A non-usable Rust reading (null, or a depth that is not a non-negative integer)
 * is `"unknown"`, never `"in-sync"`: an absent reading is not evidence of
 * agreement, and reporting it as agreement would make the probe report health
 * exactly when it has no data.
 *
 * The remaining depth and tip-kind fields are compared too, against the only
 * expectations the stream geometry implies (`PixelStoreRegistry::get_history_tip`
 * builds all five from one cursor, so a wire reading that violates any of them is
 * not a tip this probe can reason about): the two branches must partition the
 * stream, and a direction with depth 0 has no entry to name.
 *
 * HONEST LIMIT: this compares the two CURSORS, not the two stacks, and it does not
 * claim a kind match it cannot make. The host side CAN classify its own entries -
 * `SnapshotEntry` (history.ts `interface SnapshotEntry`) carries `imperative?` (a Pixel step),
 * `snapshotType?: "snapshot"` (a Snapshot step), and neither (an External step), so
 * three of Rust's four `PayloadKind`s have a host counterpart. What does not exist
 * is 1:1 entry IDENTITY between the stacks: no shared entry id, no way to prove
 * that the entries at a given cursor offset are the same user action, and Rust's
 * fourth kind (`PayloadKind::Metadata`, from a native `RenderLayer` payload) has no
 * host commit at all. So an equal depth plus a matching kind would still be
 * coincidence rather than proof, and the kinds are reported in the divergence log
 * for an operator to read instead of being compared as if they settled it.
 *
 * SECOND HONEST LIMIT - a divergence past the host's eviction depth is NOT a
 * cursor fault. `MAX_HISTORY_DEPTH` is 50 (engine/types.ts) and `CommandHistory`
 * evicts its OLDEST entry past it (history.ts, in both `commit` and
 * `recordSnapshotHistory`); the Rust stream has no such cap. So from the 51st
 * commit onward the host depth is permanently lower than the Rust depth while both
 * cursors sit at the same user action, and this probe reports `diverged` for a
 * difference that is the configured eviction policy. Read a `diverged` verdict as
 * "these depths differ, and here is how much" - not as "a step was missed". The
 * eviction policy is not changed here; reconciling the two caps is a separate
 * change, and until then the log line is the only output.
 */
export function classifyHistoryCursorParity(
  rust: RustHistoryTip | null | undefined,
  tsUndoDepth: number,
): HistoryCursorParityVerdict {
  if (!rust || !isUsableDepth(rust.undo_depth) || !isUsableDepth(tsUndoDepth)) return "unknown";
  if (!isUsableDepth(rust.redo_depth) || !isUsableDepth(rust.total_depth)) return "unknown";
  if (rust.undo_depth + rust.redo_depth !== rust.total_depth) return "unknown";
  if ((rust.undo_tip_kind === null) !== (rust.undo_depth === 0)) return "unknown";
  if ((rust.redo_tip_kind === null) !== (rust.redo_depth === 0)) return "unknown";
  return rust.undo_depth === tsUndoDepth ? "in-sync" : "diverged";
}

/**
 * Read one document's Rust cursor. Rejects on any failure rather than resolving a
 * zero cursor - the same rule `getPixelHistoryDepth` follows, because a caller
 * that saw `undo_depth: 0` from a failed call would conclude "the host stack is
 * empty" from an IPC error. The three depth fields are validated by the SHARED
 * guard both depth readers use (same Rust struct, same accessor), so a malformed
 * tip is rejected here for the same reason a malformed depth is.
 */
export async function readRustHistoryTip(docId: string): Promise<RustHistoryTip> {
  const { invoke } = await import("@tauri-apps/api/core");
  const raw: unknown = await invoke("rust_pixels_history_tip", { docId });
  const depths = requireHistoryDepthNumbers(raw, "rust_pixels_history_tip");
  const kinds = raw as Partial<RustHistoryTip> | null;
  return {
    ...depths,
    undo_tip_kind: typeof kinds?.undo_tip_kind === "string" ? kinds.undo_tip_kind : null,
    redo_tip_kind: typeof kinds?.redo_tip_kind === "string" ? kinds.redo_tip_kind : null,
  };
}

/**
 * Monotonic probe op counter plus a single-slot in-flight guard.
 *
 * `tsUndoDepth` is captured at call time, but the Rust cursor is read after a
 * dynamic `import` plus an IPC round trip that takes the same registry mutex the
 * real history commands take, and `HistoryTip` carries no version or sequence
 * number. A reading that lands after the NEXT step's cursor move is therefore
 * indistinguishable from a current one by its own payload. Two rules close that
 * hole: every probe claims an op number, and a probe whose op is no longer the
 * latest drops itself as `"unknown"` instead of pairing a stale depth against a
 * fresh cursor; and at most one read is outstanding, so a step arriving during a
 * read supersedes it and skips its own - a held undo key cannot pile up round
 * trips against the mutex the restore path needs.
 */
let latestProbeOp = 0;
let probeInFlight = false;

export interface HistoryCursorParityReading {
  verdict: HistoryCursorParityVerdict;
  /** The reading the verdict came from; null when there was none to compare. */
  tip: RustHistoryTip | null;
  /**
   * False when the probe declined to read because the TS->Rust history bridge is
   * not recording (see the module header). Distinct from a read that was
   * attempted and produced nothing: the former is a deliberate skip, the latter
   * is a fault, and neither is evidence of agreement.
   */
  observing: boolean;
}

const NO_READING = (): HistoryCursorParityReading => ({
  verdict: "unknown",
  tip: null,
  observing: false,
});

/**
 * Take one cursor reading and classify it. Never rejects: every failure mode
 * resolves as `"unknown"` so a caller can await the verdict without a catch.
 */
export function probeHistoryCursorParity(
  docId: string,
  tsUndoDepth: number,
): Promise<HistoryCursorParityReading> {
  // The one gate, in the one entry point both production call sites use, so
  // neither call site has to know about it: with the bridge not recording there
  // is no Rust stream to compare against and a read would be pure noise.
  if (!docId || !historyBridgeEnabled()) return Promise.resolve(NO_READING());
  const op = ++latestProbeOp;
  if (probeInFlight) return Promise.resolve({ ...NO_READING(), observing: true });
  probeInFlight = true;
  return readRustHistoryTip(docId)
    .then((tip) => ({
      // Superseded: a newer step already claimed an op, so this cursor read may
      // describe that step rather than the one whose depth we captured.
      verdict: op === latestProbeOp ? classifyHistoryCursorParity(tip, tsUndoDepth) : "unknown",
      tip,
      observing: true,
    }))
    .catch(() => ({ verdict: "unknown" as const, tip: null, observing: true }))
    .finally(() => {
      probeInFlight = false;
    });
}

/**
 * Fire-and-forget observation, called from the undo/redo path. Never returns a
 * rejected promise and never awaits inside the caller: the verdict is a log line,
 * so the undo must not wait on (or fail because of) the measurement.
 */
export function observeHistoryCursorParity(
  docId: string,
  tsUndoDepth: number,
  direction: "undo" | "redo",
): void {
  void probeHistoryCursorParity(docId, tsUndoDepth).then(({ verdict, tip, observing }) => {
    if (!observing) return; // deliberately not reading: not a fault, nothing to report
    if (verdict === "diverged") {
      console.warn("[history-cursor-parity] TS and Rust undo depths disagree", {
        docId,
        direction,
        tsUndoDepth,
        rustUndoDepth: tip?.undo_depth ?? null,
        rustTotalDepth: tip?.total_depth ?? null,
        rustRedoDepth: tip?.redo_depth ?? null,
        rustUndoTipKind: tip?.undo_tip_kind ?? null,
        rustRedoTipKind: tip?.redo_tip_kind ?? null,
      });
    } else if (verdict === "unknown") {
      // Its own line, so an operator can tell "no drift detected" from "nothing
      // was read": a rejected read, a malformed tip, or a step that overtook this
      // one. Never a warn - the undo/redo path did not fail.
      console.info("[history-cursor-parity] no usable reading this step", {
        docId,
        direction,
        tsUndoDepth,
      });
    }
  });
}
