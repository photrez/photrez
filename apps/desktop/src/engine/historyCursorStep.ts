// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * The Rust undo-cursor step: one invoke per popped history entry, serialised, and
 * observed.
 *
 * Extracted from `history.ts` so the ordering, the unhandled-rejection guarantee
 * and the did-not-move diagnostic live in one place. The DECISION to step - which
 * entry kinds own a Rust entry - stays in `CommandHistory.stepRustCursor`, because
 * it needs the popped entry and the doc-id getter.
 *
 * Three contracts this module owns:
 *
 *  - ORDER. Every step is issued off the previous one's settled tail, so steps
 *    reach the Rust registry in POP order. Two pops fired back-to-back (a held
 *    Ctrl+Z) would otherwise race their round-trips against the registry mutex, and
 *    the tiles a step returns are attributed to whichever invoke answers first.
 *    The tail never rejects, so one failed step cannot stall the queue.
 *
 *  - NO UNHANDLED REJECTION. Pop sites that restore through the model never
 *    consume the step, and `rust_pixels_undo` genuinely rejects:
 *    `ProtocolEngine::undo_pixel` answers `Err(E_EXTERNAL_PENDING)` while a
 *    pending-external barrier is set (crates/core/src/history.rs:209-214). The only
 *    SETTER of that barrier is the native walker's external-handoff arm
 *    (crates/core/src/document_core_apply.rs:975); `record_external` only CHECKS it
 *    (history.rs:290).
 *
 *  - A SILENT NO-OP MUST NOT STAY SILENT. `rust_pixels_undo` answers `Ok` with an
 *    UNMOVED cursor for a Snapshot/Native tip (history.rs:239) or at cursor 0
 *    (apps/desktop/src-tauri/src/paint_parity_cmds.rs:186-198), and rejects when the
 *    barrier is set. Either way the host stack has already popped, so the next step
 *    proceeds from a stale cursor. Both outcomes warn, from here, so BOTH restore
 *    paths get the diagnostic whether or not they consume the step. Non-fatal by
 *    contract: the undo path is fail-soft.
 *
 * "Did not move" is read off `version`: `undo_pixel` bumps it exactly when it moves
 * the cursor (history.rs:221 / :231) and returns the current one otherwise. A step
 * this process never saw (the facade walker's own steps) can leave the baseline
 * stale-low, which HIDES a no-op - a missed warning, never a false alarm - and the
 * parity probe is the independent check.
 *
 * WIRE FIDELITY: the command names stay inline at the call so the static writer
 * census still reads this site as a cursor mover. If the `apply_tile_patch` tile
 * shape is ever reconciled (serde alias, or sending `{w, h}` instead of
 * `TileUploadLike`'s `{width, height}`), the emulator in
 * `engine/__tests__/rustStreamEmulator.ts` must be reconciled in the SAME change -
 * it mirrors the real rejection, so a one-sided fix turns its rejection into a
 * silent lie.
 *
 * SCOPE - which cursor commands this observes: `rust_pixels_undo` and
 * `rust_pixels_redo`, and nothing else. The SNAPSHOT cursor commands
 * (`rust_pixels_undo_snapshot` / `rust_pixels_redo_snapshot`) are driven by
 * `restoreSnapshotBitmapsByToken` in history.ts, which reports its own rejections
 * under the same `[history-cursor-step]` prefix but does NOT do a did-not-move
 * check: `undo_snapshot` returns the payload rather than a version, so there is
 * nothing here to compare. Wiring that would mean a second observation channel for
 * one command pair, and it is not needed today - a Snapshot tip leaves an
 * unmoved cursor precisely because `undo_pixel` refuses it (history.rs:239), and
 * the pixel step's own diagnostic covers the sibling case.
 */

export type RustCursorStepDirection = "undo" | "redo";

export interface RustCursorStepRequest {
  direction: RustCursorStepDirection;
  docId: string;
  /** May be "" - the command only uses it to report an epoch for an empty result. */
  layerId: string;
}

export class RustCursorStepper {
  private chain: Promise<unknown> = Promise.resolve();
  private last: Promise<unknown> | null = null;
  private lastVersion: { docId: string; version: number } | null = null;

  /** Issue one step, chained behind any earlier one, and observe its outcome. */
  fire(req: RustCursorStepRequest): Promise<unknown> {
    const { direction, docId, layerId } = req;
    const step = this.chain.then(() =>
      import("@/lib/protocol/bridge").then(({ invokePixelCommand }) =>
        invokePixelCommand(direction === "undo" ? "rust_pixels_undo" : "rust_pixels_redo", {
          docId,
          layerId,
        }),
      ),
    );
    // Attached to `step` ITSELF: a handler on the derived tail alone would leave
    // `step` unhandled for every consumer that never takes it.
    step.then(
      (res) => this.observeResolved(req, res),
      (err: unknown) => this.observeRejected(req, err),
    );
    this.chain = step.then(
      () => {},
      () => {},
    );
    this.last = step;
    return step;
  }

  /** The step fired by the most recent pop, or null when none fired. Consume-once. */
  take(): Promise<unknown> | null {
    const step = this.last;
    this.last = null;
    return step;
  }

  private observeResolved(req: RustCursorStepRequest, res: unknown): void {
    const version = (res as { version?: unknown } | null)?.version;
    if (typeof version !== "number") return;
    const prev = this.lastVersion;
    if (prev && prev.docId === req.docId && version <= prev.version) {
      console.warn("[history-cursor-step] the Rust cursor step did not move the cursor", {
        docId: req.docId,
        direction: req.direction,
        version,
        previousVersion: prev.version,
      });
    }
    this.lastVersion = { docId: req.docId, version };
  }

  private observeRejected(req: RustCursorStepRequest, err: unknown): void {
    console.warn("[history-cursor-step] the Rust cursor step was rejected", {
      docId: req.docId,
      direction: req.direction,
      error: String(err),
    });
  }
}
