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
 * WHAT THE VERSION DIAGNOSTIC CANNOT SEE. `version` answers only "did the cursor
 * move", and it CANNOT answer "did it move over MY entry". The `External` arm
 * (history.rs:229-233) moves the cursor AND bumps `version` while yielding no
 * tiles, so a step that consumed a host-handoff entry instead of its own `Pixel`
 * entry looks EXACTLY like a correct step to the check below: version advanced, no
 * rejection, no warning from here. The mismatch is caught by the tile path's
 * `[paint] Rust-owned step returned no tiles` warning
 * (apps/desktop/src/components/editor/useEditorCommands.ts) and, before it, by
 * `fireGatedOnPixelTip` refusing to issue the step at all. `version` is a
 * liveness check on the round trip, NOT a correctness check on the entry consumed.
 *
 * WIRE FIDELITY: the command names stay inline at the call so the static writer
 * census still reads this site as a cursor mover. The `apply_tile_patch` tile
 * shape is pinned on BOTH sides of the wire - by
 * `both_tile_shapes_are_accepted_at_the_wire_and_mint_exactly_one_entry` in
 * apps/desktop/src-tauri/src/paint_parity_cmds.rs (both spellings, through Tauri's
 * own argument deserializer) and by `tilesSurviveTheWire` in
 * `engine/__tests__/rustStreamEmulator.ts` - and both must change in the SAME
 * commit as any change to it, or one of them becomes a silent lie.
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

  /**
   * Issue one step ONLY if the Rust stream's tip for this direction is a `Pixel`
   * entry, then behave exactly as `fire` does.
   *
   * WHY THIS EXISTS. A step fired for an entry Rust does NOT own at the tip is
   * destructive, not merely useless: `ProtocolEngine::undo_pixel` moves the
   * unified cursor over an `External` tip and yields `(None, None, None)`
   * (crates/core/src/history.rs:229-233), which `PixelStoreRegistry::undo_pixel`
   * collapses to `None` AFTER the move has already happened
   * (crates/core/src/pixel_store.rs:745). The caller asked to revert ITS entry,
   * and instead consumed a different one and got no pixels. That is live at
   * shipping defaults, because the two sides of the stream are gated
   * differently - see `CommandHistory.stepRustCursor`.
   *
   * FAIL-SAFE DIRECTION: on an unreadable, slow or unrecognised tip answer, the
   * step is FIRED (fail-open), not skipped. The two possible mistakes are not
   * symmetric:
   *
   *   - Skipping a step that should have run silently loses a legitimate pixel
   *     undo. The host stack has ALREADY popped by this point, so the step is
   *     gone from the undo stack with nothing reverted - the user's Ctrl+Z
   *     evaporates with no error, and `redo` can put it back but `undo` cannot.
   *   - Firing a step onto an `External` tip consumes a host-handoff entry. The
   *     cost is bounded and already observable: the cursor moves one, the caller
   *     uploads no tiles, and the `[paint] Rust-owned step returned no tiles`
   *     warning fires. The step is recoverable by a further undo; the lost-undo
   *     case is not.
   *
   * So an unknown answer is treated as "assume the common case" - the `Pixel`
   * tip every ordinary paint step has - rather than as "assume the worst". The
   * gate narrows the defect to the case where the tip is KNOWN to be wrong, and
   * never converts a transport hiccup into a silently dropped undo.
   *
   * ORDER. The tip read is chained onto the SAME tail `fire` uses, so it is
   * issued after any earlier step has settled and before this step is - the read
   * cannot race the pop, and two rapid presses cannot interleave a read and a
   * step. Exactly one read per gated call; a second press issues its own, also
   * serialised behind the first.
   *
   * A `Snapshot` or `Metadata` tip is refused like `External`: `undo_pixel`
   * leaves the cursor alone for those (history.rs:239 / :269), so firing would
   * consume nothing and warn about nothing - while a `Snapshot` entry's own step
   * belongs to the token re-attach's `rust_pixels_*_snapshot` commands, which
   * this module does not observe (see SCOPE above).
   *
   * @returns the step's promise, or `null` when the tip was read and was not
   * `Pixel` (nothing was issued). A null is a real answer, not a failure: the
   * caller keeps its own memento fallback for a non-Rust-owned entry, and a
   * Rust-owned one keeps the existing "no tiles, warn" refusal.
   */
  fireGatedOnPixelTip(req: RustCursorStepRequest): Promise<unknown> {
    const { direction, docId } = req;
    // The READ is chained onto the same tail `fire` uses, so it lands after any
    // earlier step settled; the STEP is then chained behind this read's tail.
    // Read-then-step is therefore one serialised sequence per press, and two rapid
    // presses cannot interleave one press's read against the other's step.
    const gate = this.chain.then(() => this.readTipKind(docId, direction));
    // The tail must never reject, so one refused or failed step cannot stall the
    // queue - the same guarantee `fire` maintains for its own tail.
    this.chain = gate.then(
      () => {},
      () => {},
    );
    // Published SYNCHRONOUSLY, before any await: `take()` and `settled()` run at
    // the instant the pop does, so a handle published later would be invisible.
    // `issueStep` (not `fire`) because this sequence already IS the handle.
    const sequence = gate.then((tipKind) => {
      if (tipKind !== undefined && tipKind !== "pixel") {
        console.warn(
          "[history-cursor-step] skipping the cursor step: the Rust tip is not this entry's",
          {
            docId,
            direction,
            tipKind,
            note:
              "the stream tip is not a Pixel entry, so this step would consume a different entry",
          },
        );
        return null;
      }
      return this.issueStep(req);
    });
    // A rejecting step must NOT become an unhandled rejection, and this sequence is
    // the handle `take()`/`settled()` hand out - so a pop site that never claims it
    // (the metadata path does not) would leave the rejection with no handler at all.
    // `issueStep` observes the INNER step's rejection; this is the outer promise that
    // propagates it, and it needs its own handler. Observing here as well would
    // double-count the version baseline and risk a false "did not move" warning, so
    // this swallows rather than observes. A consumer that awaits the handle still
    // sees the rejection - this only stops it being unhandled.
    sequence.catch(() => {});
    return this.publish(sequence);
  }

  /**
   * The payload kind this direction would consume, or `undefined` when it could
   * not be established. `undefined` therefore covers all three "do not know"
   * cases - a rejected invoke, a malformed answer, and a kind this build does
   * not recognise - and every one of them means "fire" (see `fireGatedOnPixelTip`).
   *
   * Never rejects: a read that throws must not turn into an unhandled rejection
   * on the undo path, which is fail-soft by contract.
   */
  private async readTipKind(
    docId: string,
    direction: RustCursorStepDirection,
  ): Promise<string | undefined> {
    if (!docId) return undefined;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const raw = (await invoke("rust_pixels_history_tip", { docId })) as {
        undo_tip_kind?: unknown;
        redo_tip_kind?: unknown;
      } | null;
      const kind = direction === "undo" ? raw?.undo_tip_kind : raw?.redo_tip_kind;
      return typeof kind === "string" ? kind : undefined;
    } catch {
      return undefined;
    }
  }

  /**
   * Issue one step, chained behind any earlier one, and observe its outcome.
   * Publishes the step as this press's handle.
   */
  fire(req: RustCursorStepRequest): Promise<unknown> {
    this.last = this.issueStep(req);
    return this.last;
  }

  /**
   * Issue one step behind the chain and observe it, WITHOUT touching the published
   * handle.
   *
   * Split out of `fire` for the gate: `fireGatedOnPixelTip` publishes its own
   * read-then-step sequence synchronously, and the step inside that sequence
   * resolves LATER. Were the step to publish as well, a second press arriving in
   * between would have its handle overwritten by the first press's step - and
   * `take()` would hand the tile path somebody else's result. One handle per
   * press, published once, by whoever started the press.
   */
  private issueStep(req: RustCursorStepRequest): Promise<unknown> {
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
    return step;
  }

  /** The step fired by the most recent pop, or null when none fired. Consume-once. */
  take(): Promise<unknown> | null {
    const step = this.last;
    this.last = null;
    return step;
  }

  /**
   * Publish the in-flight read-then-step sequence as this press's handle.
   *
   * Internal to `fireGatedOnPixelTip`, which publishes synchronously so that
   * `take()` and `settled()` - both synchronous, both called by the pop the
   * instant it runs - can still see this press. A gated step that published
   * nothing would be invisible to both: the tile path would never learn whether
   * the step fired or was refused, and the tile branch would fail to sequence the
   * parity probe behind the step it is meant to follow.
   *
   * The published promise is the SAME one `fireGatedOnPixelTip` returns, so a
   * consumer awaiting it waits for the read AND the step, and observes `null` when
   * the gate refused - a real answer ("Rust had no entry for this one"), and
   * exactly what the pre-existing no-step path already returned.
   */
  private publish(sequence: Promise<unknown>): Promise<unknown> {
    this.last = sequence;
    return sequence;
  }

  /**
   * The most recent step's outcome, observed WITHOUT claiming it. NOT
   * consume-once: this leaves the handle in place, so calling it twice returns the
   * same promise and the step is still there for the next pop's `take()`.
   *
   * For a caller that must not own this step's tiles but must still know when its
   * cursor move has landed. The tile restore path needs exactly that: its fetch
   * gate is `rustOwned || photrez.rustPixels`, so a bridge-ON TS-owned tile pop
   * fires a step that branch has no reason to `take`, while the parity probe that
   * fires immediately afterwards reads `rust_pixels_history_tip` over the SAME
   * registry mutex. Without this, the probe's read races the step and can observe
   * the PRE-step cursor - a `diverged` for a step that moved exactly once.
   *
   * A caller using this instead of `take` leaves the handle in place, so the next
   * pop's leading `take()` still drops it: observing is not claiming.
   */
  settled(): Promise<unknown> | null {
    return this.last;
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
