import type { DocumentModel } from "./types";
import { MAX_HISTORY_DEPTH } from "./types";
import {
  disposeSnapshot,
  type HistoryTilePatches,
  type SnapshotEntry,
} from "./historyTypes";
export type { HistoryTilePatches, SnapshotEntry } from "./historyTypes";
import { historyDegraded } from "@/lib/protocol/facadeRegistry";
import {
  buildNativeHistoryItems,
  deriveOpenBaselineFloor,
  getNativeRedoCount,
  getNativeUndoCount,
  isNativeQueryAuthoritative,
  resyncDerivedView,
  setHistoryViewAuthority,
  settleCommitRecorders,
} from "./nativeHistoryView";
import { RustCursorStepper } from "./historyCursorStep";

/**
 * Route every TS commit into the SAME Rust `ProtocolEngine` cursor so TS and Rust
 * operations share ONE logical history position.
 */

export interface HistoryItem {
  label: string;
  isRedo: boolean;
}

export class CommandHistory {
  private undoStack: SnapshotEntry[] = [];
  private redoStack: SnapshotEntry[] = [];
  private maxDepth: number;
  private currentLastPaintCoords: { x: number; y: number } | null = null;
  private liveBitmapGetter: (() => Iterable<ImageBitmap | null>) | null = null;
  private docIdGetter: (() => string) | null = null;
  /** Whether the most recent undo()/redo() popped a snapshot-typed entry. */
  private lastPoppedIsSnapshot = false;
  /** Allowlist of the most recently popped entry (null = unknown). */
  private lastPoppedPixelLayerIds: string[] | null = null;
  /** Ordering, unhandled-rejection and did-not-move diagnostics for the step. */
  private readonly rustCursor = new RustCursorStepper();
  /**
   * The facade handoff consumed the Rust cursor for the press in flight, so the pop
   * that follows must not step again.
   *
   * `runFacadeExternalHandoff` can move the cursor through
   * `confirmExternalCursor` and STILL return false - it falls through when the
   * delta it got back was empty, which a legacy `External` entry with no captured
   * after-state produces. The host then pops, and an ungated arm would issue a
   * SECOND step for one press. One-shot: consumed by the next `stepRustCursor`.
   *
   * Consumed by exactly the pop that follows the note, which is why the dispatch arms
   * it immediately above `history.undo()` rather than at the handoff call site: two
   * guards in between return without popping, and an earlier arm would let this
   * outlive its press and suppress the NEXT one instead.
   */
  private facadeCursorConsumed = false;

  /**
   * The facade handoff moved the Rust cursor for the press in flight.
   *
   * Called by the undo dispatch when `runFacadeExternalHandoff` reports that it
   * committed the cursor (`confirmExternalCursor`) AND then fell through rather than
   * reporting the step handled. That combination is real: the handoff falls through
   * on an empty delta, which a legacy `External` entry with no captured after-state
   * produces, so the host pops afterwards. Without this note the pop's shim arm
   * would issue a SECOND cursor step for one press - one undo, two `rust_pixels_undo`.
   *
   * One-shot: the next `stepRustCursor` consumes it. A caller that reports handled
   * never needs it, because it never pops.
   */
  noteFacadeCursorMoved(): void {
    this.facadeCursorConsumed = true;
  }

  constructor(maxDepth: number = MAX_HISTORY_DEPTH) {
    this.maxDepth = maxDepth;
  }

  /**
   * Supply the active document id so commits can be appended to the correct Rust
   * history cursor. Called once by the editor shell.
   */
  attachDocIdGetter(getter: () => string): void {
    this.docIdGetter = getter;
  }

  /**
   * Register a getter for the bitmaps referenced by the LIVE document model.
   * Max-depth eviction consults this so a bitmap still in use by the model is
   * never closed, even when no remaining snapshot references it. Without it,
   * editing one layer >MAX_HISTORY_DEPTH times could close the bitmap of an
   * untouched layer (snapshot evicted → bitmap detached → "image source is
   * detached" on the next render). See workspace.ts attach call sites.
   */
  attachLiveBitmapGetter(getter: () => Iterable<ImageBitmap | null>): void {
    this.liveBitmapGetter = getter;
  }

  setLastPaintCoords(coords: { x: number; y: number } | null): void {
    this.currentLastPaintCoords = coords;
  }

  getLastPaintCoords(): { x: number; y: number } | null {
    return this.currentLastPaintCoords;
  }

  commit(
    snapshot: DocumentModel,
    label?: string,
    imperative?: HistoryTilePatches,
    alreadyRecordedInRust = false,
    pixelLayerIds: string[] | null = null,
    /**
     * The document-size halves of THIS commit, for a commit whose host mutation
     * resizes the document (a crop). Supplied by the caller because only the
     * caller knows both: it commits BEFORE it mutates, so the pre-mutation size
     * is the `snapshot` argument, and it computes the post-mutation size from its
     * own parameters.
     *
     * Left undefined for every size-neutral commit (delete, move, reorder, a
     * metadata tweak). That is a complete answer, not a pending state: Rust then
     * records an External entry with no size and its undo emits no size delta, so
     * such a step stays an empty layer delta for the host. The engine cannot
     * derive these itself - its own document size is baseline-only and goes stale
     * after the first crop, so a second crop would otherwise capture the size
     * from before the first and undo to a size the user was never in.
     */
    docSizeChange?: { before: { width: number; height: number }; after: { width: number; height: number } },
    /**
     * Layer ids THIS commit mints - a merge down / merge selected / flatten
     * destination. The host mints the id and commits BEFORE the mutation that
     * creates it, so the declaration travels with this call rather than in ambient
     * state: the facade commit shim mirrors on EVERY commit, so a module-level
     * declaration would be consumed by an unrelated commit.
     *
     * Undefined for every commit that mints nothing, which records an empty set and
     * leaves the engine's survivor rule untouched for a host-pushed layer.
     */
    mintedLayerIds?: string[],
  ): void {
    // Append this commit to the unified Rust history cursor.
    //  - imperative TS pixel op NOT yet in Rust (text/shape/transform)
    //    -> `apply_tile_patch` (Pixel entry, same command Rust strokes use).
    //  - non-pixel TS (metadata) op -> `rust_pixels_record_external` (External entry).
    //  - `alreadyRecordedInRust` (brush/fill/adjustment bake) -> Rust already owns
    //    the Pixel entry via `rust_pixels_write_region`; skip to avoid double-count.
    //  - snapshot-type ops (before+after states) use `recordSnapshotHistory`, NOT
    //    this method — see its comment for the contract-correct before/after.
    // Set by the facade commit shim's external-transition mirror; consumed by
    // `settleRecordsThenResync` so the derived view refreshes after the shim's
    // record lands.
    this.lastCommitRecord = null;
    const entry: SnapshotEntry = {
      snapshot,
      timestamp: Date.now(),
      lastPaintCoords: this.currentLastPaintCoords,
      label,
      imperative,
      pixelLayerIds: pixelLayerIds ?? null,
    };

    this.undoStack.push(entry);

    // Clear redo stack on new operation
    this.redoStack = [];

    // The native stream just gained an entry; refresh the cached query view
    // so the UI does not serve a dead undo button over live work. The read must
    // wait for the facade commit shim's external-transition mirror (registered
    // via flushExternalTransitions) to settle, or it observes the pre-commit
    // stream and publishes a count one short.
    void this.settleRecordsThenResync(entry.snapshot.id || this.docIdGetter?.() || "");

    // Enforce max depth
    if (this.undoStack.length > this.maxDepth) {
      const evicted = this.undoStack.shift()!; // Evict oldest
      // Bitmaps still referenced by the live document model (not just other
      // snapshots) must survive eviction — see attachLiveBitmapGetter.
      const liveBitmaps = this.liveBitmapGetter ? Array.from(this.liveBitmapGetter()) : [];
      const live = (b: unknown) =>
        this.undoStack.some((e) => e.snapshot.layers.some((l) => l.imageBitmap === b || l.baseImageBitmap === b)) ||
        this.redoStack.some((e) => e.snapshot.layers.some((l) => l.imageBitmap === b || l.baseImageBitmap === b)) ||
        snapshot.layers.some((l) => l.imageBitmap === b || l.baseImageBitmap === b) ||
        liveBitmaps.includes(b as ImageBitmap);
      disposeSnapshot(evicted.snapshot, live);
    }
  }

  /**
   * Record a snapshot-type history step that carries BOTH the pre-action and
   * post-action document states explicitly.
   *
   * Contract-correct before/after derivation: at a PRE-action commit the post-
   * action state does not yet exist, so it can NEVER be inferred from the
   * commit arg or from the stack. The caller passes both states, so:
   *   - `before` = the PRE-action state (what TS undo restores / what Rust
   *     `undo_snapshot` returns);
   *   - `after`  = the POST-action state (what Rust `redo_snapshot` returns).
   * There is NO stack inference — the payloads are built directly from the two
   * arguments, so undo can never return a state two-commits back (the off-by-one
   * the old `commit()` snapshot branch had).
   *
   * No Rust command fires from here: the recorder for this entry is the facade
   * commit shim (`installFacadeCommitShim` wraps this method too). It mirrors the
   * push into an `External` entry, and the facade walker consumes that mirror on
   * walker consumes that mirror on undo/redo.
   *
   * TS undo/redo authority stays unchanged: the PRE-action `before` state is
   * pushed as the undo-point, the redo stack is cleared, and `maxDepth` +
   * `disposeSnapshot(evicted.snapshot, live)` are enforced exactly as commit().
   */
  recordSnapshotHistory(before: DocumentModel, after: DocumentModel, label?: string, pixelLayerIds: string[] | null = null): void {
    // Set by the facade commit shim's external-transition mirror; consumed by
    // `settleRecordsThenResync` so the derived view refreshes after the shim's
    // record lands.
    this.lastCommitRecord = null;
    // The entry this commit pushes, so a failed record can be marked ON IT.
    // Built first because the push below must carry the very object the pop reads.
    const entry: SnapshotEntry = {
      snapshot: before,
      timestamp: Date.now(),
      lastPaintCoords: this.currentLastPaintCoords,
      label,
      snapshotType: "snapshot",
      pixelLayerIds: pixelLayerIds ?? null,
    };

    // TS undo/redo authority: the undo-point is the PRE-action state (unchanged
    // convention) — undo restores `before`, which is what Rust undo_snapshot
    // returns for this step.
    this.undoStack.push(entry);

    // Clear redo stack on new operation
    this.redoStack = [];

    // See commit(): keep the cached native-query view current (deferred until
    // the shim's mirror has settled).
    void this.settleRecordsThenResync(before.id || this.docIdGetter?.() || "");

    // Enforce max depth (mirror commit()): bitmaps still referenced by the live
    // model (including the POST-action `after` state, which is not pushed) must
    // survive eviction — see attachLiveBitmapGetter.
    if (this.undoStack.length > this.maxDepth) {
      const evicted = this.undoStack.shift()!; // Evict oldest
      const liveBitmaps = this.liveBitmapGetter ? Array.from(this.liveBitmapGetter()) : [];
      const live = (b: unknown) =>
        this.undoStack.some((e) => e.snapshot.layers.some((l) => l.imageBitmap === b || l.baseImageBitmap === b)) ||
        this.redoStack.some((e) => e.snapshot.layers.some((l) => l.imageBitmap === b || l.baseImageBitmap === b)) ||
        after.layers.some((l) => l.imageBitmap === b || l.baseImageBitmap === b) ||
        liveBitmaps.includes(b as ImageBitmap);
      disposeSnapshot(evicted.snapshot, live);
    }
  }

  /** Last native query this history re-synced from, or null before the first one. */
  private nativeQuery: import("@/lib/protocol/types").HistoryQueryResult | null = null;

  /**
   * Set by the workspace to republish UI state (history rows, undo affordance)
   * whenever the derived view above was re-read. Without this, a deferred
   * resync refreshes the cache but no surface ever repaints from it.
   */
  onDerivedViewChanged: (() => void) | null = null;

  /** The facade shim record fired by the most recent commit/recordSnapshotHistory. */
  private lastCommitRecord: Promise<unknown> | null = null;

  /** Settle this commit's recorders first; the timing contract lives on `settleCommitRecorders`. */
  private async settleRecordsThenResync(docId: string): Promise<void> {
    const record = this.lastCommitRecord;
    this.lastCommitRecord = null;
    await settleCommitRecorders(record, docId);
    await this.resyncFromNativeQuery();
  }

  /**
   * The query is the authority only when the native stream actually knows this
   * document: entries recorded outside the TS stack (routed ops, structural
   * commits) live there, but a session whose commits never reached the stream
   * (the legacy bridge/wasm arms) leaves it empty - and then it must not
   * mask the TS stack. In the native path every document carries at least the
   * document-open baseline entry (cursor 1 at open), so this test cannot
   * misfire on the canonical engine.
   */
  private lastAuthoritativeView = false;

  /** The query is the authority only when the native stream actually knows this
   *  document: entries recorded outside the TS stack (routed ops, structural
   *  commits) live there, but a session whose commits never reached the stream
   *  (the legacy bridge/wasm arms) leaves it empty - and then it must not
   *  mask the TS stack. In the native path every document carries at least the
   *  document-open baseline entry (cursor 1 at open), so this test cannot
   *  misfire on the canonical engine. A change of authority is always
   *  announced: a view silently switching sources is a defect, so the
   *  transition is logged (console lines for operators) AND published through
   *  `historyViewAuthority` for observability - only tests branch on that
   *  signal; no production consumer reads it. */
  private nativeQueryAuthoritative(): boolean {
    const now = isNativeQueryAuthoritative(this.nativeQuery);
    if (now !== this.lastAuthoritativeView) {
      if (now) {
        console.info("[history] native stream is now the authoritative view");
        setHistoryViewAuthority("native");
      } else {
        console.warn(
          "[history] native stream is no longer authoritative; the view falls back to the TS stacks",
        );
        setHistoryViewAuthority("ts-fallback");
      }
      this.lastAuthoritativeView = now;
    }
    return now;
  }

  canUndo(): boolean {
    if (this.nativeQueryAuthoritative()) {
      return getNativeUndoCount(this.nativeQuery!) > 0;
    }
    return this.undoStack.length > 0;
  }

  canRedo(): boolean {
    if (this.nativeQueryAuthoritative()) {
      return getNativeRedoCount(this.nativeQuery!) > 0;
    }
    return this.redoStack.length > 0;
  }

  /** Rebuild this stack's view from `protocol_history_query_native`; the never-rejects contract lives on `resyncDerivedView`. */
  async resyncFromNativeQuery(): Promise<void> {
    await resyncDerivedView(this.docIdGetter?.(), (query) => {
      this.nativeQuery = query;
      this.onDerivedViewChanged?.();
    });
  }

  private lastUndoPatches?: HistoryTilePatches;
  private lastRedoPatches?: HistoryTilePatches;

  /**
   * Step the Rust undo cursor EXACTLY ONCE for the entry this pop consumed.
   *
   * The pop owns the step, not the UI dispatcher that called it. It used to be a
   * side effect of the tile branch in useEditorCommands, and a metadata entry
   * carries no tile patches, so it never reached that branch: every undone
   * metadata step left the Rust cursor one behind, and the next pixel step's
   * `rust_pixels_undo` then consumed THAT un-stepped entry instead of its own -
   * `ProtocolEngine::undo_pixel` still moves the cursor over an External tip
   * (crates/core/src/history.rs:229-233) while yielding no tiles, so the paint
   * step was silently never reverted while the host reported success.
   *
   * A step with no counterpart would consume somebody else's entry, so it fires
   * only for these arms:
   *  - `imperative.rustOwned`: `rust_pixels_write_region` recorded the Pixel entry
   *    (brush, eraser, bucket, seeded fill, gradient, delete-pixels);
   *  - the facade commit shim's mirror: `installFacadeCommitShim`
   *    (lib/protocol/facadeRegistry.ts) appends an `External` entry for every
   *    commit that is not `alreadyRecordedInRust`, so a metadata or TS-owned tile
   *    commit has the counterpart its pop consumes.
   *
   * The shim arm is unguarded only because the shim is this entry's OWN recorder,
   * so the Rust tip is known to be its entry. No other arm may assume that, and
   * the mismatch it causes is exactly the drift this method closed: when a commit
   * was recorded by one gate and stepped under another, a metadata commit appended
   * an `External` entry that no pop stepped and the cursor stayed on it. The next
   * `rustOwned` pop then inherited that entry as its tip, and because
   * `ProtocolEngine::undo_pixel` moves the cursor over an `External` tip while
   * yielding no tiles (crates/core/src/history.rs:229-233, collapsed to `None`
   * AFTER the move by pixel_store.rs:745), that step consumed a host-handoff entry
   * instead of the paint entry and reverted nothing.
   *
   * So the two entry-shaped arms below are additionally gated on the Rust tip
   * ACTUALLY being a `Pixel` entry, read through `rust_pixels_history_tip` before
   * the step is issued (`RustCursorStepper.fireGatedOnPixelTip`). The shim arm is
   * deliberately NOT gated that way: its `External` tip is the correct entry to
   * move over, and refusing it would strand every metadata undo.
   *
   * No two arms can co-fire: `undo()`/`redo()` call `stepRustCursor` once per
   * popped entry, and no restore path issues a second invoke (`rustTileProjection`
   * only AWAITS the step the pop fired). A pop that leaves its step unconsumed
   * drops it on the next pop's leading `take()`, so it is issued once and never
   * re-issued.
   *
   * A snapshot-typed entry is NOT stepped: the pop returns before every arm, and
   * `undo_pixel` refuses to move the cursor for a Snapshot tip
   * (crates/core/src/history.rs:239). The facade walker owns its shim mirror.
   */
  private stepRustCursor(entry: SnapshotEntry, direction: "undo" | "redo"): void {
    this.rustCursor.take(); // never leave a previous pop's handle claimable
    // The facade handoff already moved the Rust cursor for THIS press and still fell
    // through (it reports handled=false after `confirmExternalCursor` when the delta
    // it got back was empty, which a legacy `External` entry with no captured
    // after-state produces). Stepping again would be a second undo for one press.
    const facadeMoved = this.facadeCursorConsumed;
    this.facadeCursorConsumed = false;
    if (entry.snapshotType === "snapshot") return;
    // The shim arm: `installFacadeCommitShim` mirrors EVERY commit that is not
    // `alreadyRecordedInRust` into an `External` entry. A metadata or TS-owned tile commit records an entry whose only possible
    // consumer is this pop. Without this arm that entry is never stepped and the
    // two cursors separate by one per undone step, which is the drift the
    // tip-kind gate then refuses to cross (leaving a Ctrl+Z that reverts nothing).
    //
    // `facadeMoved` and `entry.rustRecordFailed` both demote it to the GATED path.
    // The ungated path is only sound when this pop's own entry is known to be the tip,
    // and either fact destroys that knowledge: the handoff already consumed the
    // cursor, or this entry's own record was rejected so nothing was recorded
    // for it at all. The gate refuses a tip that is not this entry's, which is the safe
    // outcome for both.
    //
    // Both are PER PRESS or PER ENTRY, never per history. A history-wide condition here
    // would demote every later pop too, and the demotion is a REFUSAL: an unrelated
    // entry whose `External` tip is perfectly correct would stop advancing the cursor.
    //
    // The last two clauses mirror the shim's OWN early-returns rather than
    // restating them, because this predicate decides whether to take the UNGATED
    // path, where a step with no entry behind it consumes somebody else's.
    // `historyDegraded()` is the one `recordExternalTransitionFor` returns early
    // on, and that latch is only ever CLEARED by the test reset - so one failed
    // external transition turns recording off for the rest of the session, and
    // without this clause the arm would keep firing ungated with nothing behind it.
    // The shim's third early-return, no active engine, cannot be mirrored: the
    // history holds no engine handle, and a host pop implies a live document.
    //
    // The shim's feature gate (`isFacadeEnabled()`) is gone with the retired
    // `photrez.facade` opt-out: the shim now always mirrors, so this arm must too.
    const shimArm =
      !facadeMoved &&
      entry.rustRecordFailed !== true &&
      entry.imperative?.rustOwned !== true &&
      !historyDegraded();
    const entryArm = entry.imperative?.rustOwned === true;
    if (!entryArm && !shimArm) return;
    const docId = entry.snapshot.id || this.docIdGetter?.();
    if (!docId) return;
    // "" is the honest "this step names no layer" answer for a metadata entry: the
    // command only reads layer_id to report an epoch when it produced no tiles.
    const layerId = entry.imperative?.layerId ?? entry.snapshot.activeLayerId ?? "";
    const req = { direction, docId, layerId };
    if (facadeMoved) return; // the handoff's cursor move IS this press's step
    if (shimArm && entry.rustRecordFailed !== true) {
      // The recorder that owns this entry is not the pixel writer: the shim.
      // The tip for this pop is the entry the recorder appended, so there is
      // nothing to check and an `External` tip is the CORRECT entry to move over.
      // Refusing it here would strand every metadata undo - which is the whole
      // reason the shim records these at all.
      this.rustCursor.fire(req);
      return;
    }
    // A pixel-writer arm: the stream's tip must actually BE this entry before a
    // step may move the cursor over it, because this host entry is not the one the
    // shim recorded. The gate publishes its own handle synchronously, so
    // `take()`/`settled()` still observe this press. See the note on the gate's
    // fail-safe direction in `fireGatedOnPixelTip`.
    this.rustCursor.fireGatedOnPixelTip(req);
  }

  /**
   * The Rust cursor step fired by the most recent undo()/redo(), or null when that
   * pop owned no Rust entry. Consume-once.
   *
   * Awaiting it NEVER moves the cursor - the pop already did. The tile path awaits
   * it for that step's authoritative tiles; the metadata path awaits it so the
   * cursor-parity probe reads a settled cursor rather than one still in flight.
   */
  takeLastCursorStep(): Promise<unknown> | null {
    return this.rustCursor.take();
  }

  /**
   * The same step, observed WITHOUT claiming it. NOT consume-once: repeated calls
   * return the same promise, and the step stays claimable by the next pop.
   *
   * For a caller that must sequence behind the step's cursor move but has no
   * business taking its tiles. The tile path is that caller for a TS-owned
   * tile pop: its fetch gate is `rustOwned`, so `projectRustTiles` returns
   * early and never claims the step - yet the pop still fired one (the shim
   * arm recorded the counterpart), and the caller's next native read shares
   * the registry mutex that step holds. Awaiting this
   * orders the probe's read after the move without stealing the step from the
   * next pop's leading `take()`.
   */
  lastCursorStepSettled(): Promise<unknown> | null {
    return this.rustCursor.settled();
  }

  /**
   * Pop the newest undo entry and return its snapshot. `stepCursor=false` is
   * only for a scratch history that records nothing in Rust (the perf audit's
   * legacy-undo probe): stepping here would move Rust's canonical cursor for
   * an entry the scratch stack does not hold. Production pops must leave the
   * default `true` - the pop owns the step for the entry it consumed.
   */
  undo(currentSnapshot: DocumentModel, stepCursor = true): DocumentModel | null {
    if (!this.canUndo()) {
      return null;
    }

    // Gate and pop must read the same source: when the native stream is the
    // authority it may report work this derived stack cannot restore (a
    // natively-recorded-only document leaves the TS stack empty). Returning
    // null routes the caller to the native dispatcher; popping anyway is a
    // TypeError on an empty stack.
    if (this.nativeQueryAuthoritative() && this.undoStack.length === 0) {
      return null;
    }
    const previousEntry = this.undoStack.pop()!;
    this.lastPoppedIsSnapshot = previousEntry.snapshotType === "snapshot";
    this.lastPoppedPixelLayerIds = previousEntry.pixelLayerIds ?? null;

    // Save current to redo stack. The imperative is OWNED BY THE ENTRY
    // (tile-memento model): it travels unchanged so a later redo replays
    // THIS entry's after-tiles — never live surface state (2026-08-22 bug:
    // a getter-parked object made every redo replay the last stroke only).
    this.redoStack.push({
      snapshot: currentSnapshot,
      timestamp: Date.now(),
      lastPaintCoords: this.currentLastPaintCoords,
      label: previousEntry.label,
      imperative: previousEntry.imperative,
      // Finding 1: propagate snapshotType so a later redo of a Snapshot entry
      // still reports `lastPoppedIsSnapshot=true` AFTER a stack hop. Without this
      // the redo twin is untyped, so redo() never consults the Rust snapshot
      // cursor and rust_pixels_redo_snapshot is dead from production.
      snapshotType: previousEntry.snapshotType,
      pixelLayerIds: previousEntry.pixelLayerIds ?? null,
      // Same reason as snapshotType above, and the same class of bug if dropped: this
      // entry's own Rust record was rejected, so NOTHING exists in the stream behind
      // it. The twin must carry that, or a later redo of it reads `undefined` here,
      // takes the bridge arm's UNGATED path, and steps onto whatever sits at the tip.
      rustRecordFailed: previousEntry.rustRecordFailed,
    });
    this.currentLastPaintCoords = previousEntry.lastPaintCoords;
    // Tile patches to execute for THIS undo (pre-stroke tiles of the entry).
    this.lastUndoPatches = previousEntry.imperative;

    // This pop owns the Rust cursor step for the entry it consumed - see
    // stepRustCursor for why the step belongs here and not in the caller's
    // tile/metadata branch split.
    if (stepCursor) this.stepRustCursor(previousEntry, "undo");
    else this.rustCursor.take();

    return previousEntry.snapshot;
  }

  /** Tile patches to execute for the just-performed undo (consume-once). */
  consumeLastUndoPatches(): HistoryTilePatches | undefined {
    const p = this.lastUndoPatches;
    this.lastUndoPatches = undefined;
    return p;
  }

  /** `redo(currentSnapshot, stepCursor)` - see `undo` for the flag's contract. */
  redo(currentSnapshot: DocumentModel, stepCursor = true): DocumentModel | null {
    if (!this.canRedo()) {
      return null;
    }

    // Same gate/pop alignment rule as undo(): a natively-recorded-only
    // document leaves the TS stacks empty, so popping would throw.
    if (this.nativeQueryAuthoritative() && this.redoStack.length === 0) {
      return null;
    }
    const nextEntry = this.redoStack.pop()!;
    this.lastPoppedIsSnapshot = nextEntry.snapshotType === "snapshot";
    this.lastPoppedPixelLayerIds = nextEntry.pixelLayerIds ?? null;

    // Save current to undo stack — the entry's own imperative travels with it
    // (see undo(): entry-owned patches, no live-state reads).
    this.undoStack.push({
      snapshot: currentSnapshot,
      timestamp: Date.now(),
      lastPaintCoords: this.currentLastPaintCoords,
      label: nextEntry.label,
      imperative: nextEntry.imperative,
      // Finding 1: propagate snapshotType so the undo twin of a Snapshot entry
      // keeps its type after a stack hop — a later undo of that twin still
      // reports `lastPoppedIsSnapshot=true` (Snapshots survive undo/redo hops).
      snapshotType: nextEntry.snapshotType,
      pixelLayerIds: nextEntry.pixelLayerIds ?? null,
      // As in undo(): a rejected record must survive the hop, or the twin takes the
      // ungated path for an entry that has no counterpart in the stream.
      rustRecordFailed: nextEntry.rustRecordFailed,
    });

    this.currentLastPaintCoords = nextEntry.lastPaintCoords;
    // Tile patches to execute for THIS redo (post-stroke tiles of the entry).
    this.lastRedoPatches = nextEntry.imperative;

    // This pop owns the Rust cursor step for the entry it consumed (symmetric
    // with undo(); see stepRustCursor).
    if (stepCursor) this.stepRustCursor(nextEntry, "redo");
    else this.rustCursor.take();

    return nextEntry.snapshot;
  }

  /** Tile patches to execute for the just-performed redo (consume-once). */
  consumeLastRedoPatches(): HistoryTilePatches | undefined {
    const p = this.lastRedoPatches;
    this.lastRedoPatches = undefined;
    return p;
  }

  /**
   * Drain the TS twin of a pixel step RUST ALREADY TOOK.
   *
   * `useEditorCommands`'s tile path pops the twin with `undo()`/`redo()` and
   * then fetches pixels from Rust, so that path drains itself. The facade
   * handoff is the other executor of the same pixel step and has no such pop:
   * leaving its twin in place froze the TS depth while the Rust cursor
   * advanced (measured at 70ea270 - 5 strokes, 5 undos, ts_undo stuck at 5
   * with `canUndo()` still true, so a 6th press was dispatched and walked the
   * cursor past the start). The handoff calls this to advance the TS cursor
   * with the Rust one.
   *
   * ONLY ever touches an entry marked `imperative.rustOwned`: that mark is the
   * single source saying "Rust recorded this step", so a metadata or TS-owned
   * entry - which the TS store still has to execute - is never drained here.
   * Returns whether a twin was drained, so a caller can tell "step taken" from
   * "no twin for this step" (a stream whose Pixel entries have no TS twin).
   *
   * The entry MOVES to the opposite stack, it is not discarded: the work is
   * still undoable/redoable in the other direction, and throwing it away would
   * make `canRedo()` lie instead of `canUndo()`. Its `imperative` memento
   * travels with it, which is safe because a `rustOwned` entry's tiles are
   * stale pixels by construction and every consumer of a popped `rustOwned`
   * entry refuses them in favour of Rust's bytes.
   *
   * NOT a cursor step: this drain runs on the facade handoff, where the walker
   * (`Command::Undo`/`Command::Redo`) already moved the Rust cursor for the step
   * before the twin was touched. Stepping here would be the double step that
   * `undo()`/`redo()` exist to make impossible.
   */
  discardRustOwnedPixelStep(direction: "undo" | "redo"): boolean {
    const from = direction === "undo" ? this.undoStack : this.redoStack;
    const to = direction === "undo" ? this.redoStack : this.undoStack;
    const top = from[from.length - 1];
    if (!top || top.imperative?.rustOwned !== true) return false;
    from.pop();
    to.push(top);
    return true;
  }

  getHistoryStack(): HistoryItem[] {
    if (this.nativeQueryAuthoritative()) {
      return buildNativeHistoryItems(this.nativeQuery!);
    }
    const items: HistoryItem[] = [];

    // Base/original state (active when undoStack is empty)
    items.push({ label: "Open", isRedo: false });

    // Undo stack items
    for (const entry of this.undoStack) {
      items.push({
        label: entry.label || "Unknown Operation",
        isRedo: false,
      });
    }

    // Redo stack items (reverse order of redoStack array)
    for (let i = this.redoStack.length - 1; i >= 0; i--) {
      items.push({
        label: this.redoStack[i].label || "Unknown Operation",
        isRedo: true,
      });
    }

    return items;
  }

  clear(): void {
    // The stacks are about to be dropped, so every bitmap they hold becomes
    // unreferenced — close all of them (shared references across the two stacks
    // are still closed only once because disposeSnapshot closes per-layer).
    // NOTE: the native-query cache is deliberately NOT cleared. The query is
    // the authority on what exists; the TS stacks are the derived view, and
    // clearing them must not hide history the engine still owns.
    for (const e of this.undoStack) disposeSnapshot(e.snapshot, () => false);
    for (const e of this.redoStack) disposeSnapshot(e.snapshot, () => false);
    this.undoStack = [];
    this.redoStack = [];
    this.currentLastPaintCoords = null;
    this.lastPoppedIsSnapshot = false;
    this.lastPoppedPixelLayerIds = null;
  }

  /**
   * Whether the most recent undo()/redo() popped a snapshot-typed entry
   * (recorded via `recordSnapshotHistory`). The snapshot re-attach in
   * useEditorCommands is gated on this: an External/metadata entry must NOT
   * consult the Rust snapshot cursor (which may point at a Snapshot entry —
   * redo would re-attach a wrong-step bitmap).
   */
  isLastPoppedSnapshotEntry(): boolean {
    return this.lastPoppedIsSnapshot;
  }

  /**
   * Allowlist of the most recently popped entry for the upload narrow in
   * useEditorCommands. null = unknown, keep the identity-map fallback.
   */
  getLastPoppedPixelLayerIds(): string[] | null {
    return this.lastPoppedPixelLayerIds;
  }

  /**
   * This host's TS-side undo depth, read from the TS stack itself - NOT from
   * the cached native query. The cursor-parity probe compares it against Rust's
   * cursor; feeding it `getUndoCount()` would read the same `protocol_history_query_native`
   * cache the Rust side already mirrors, making the comparison Rust-vs-Rust.
   */
  getTsUndoDepth(): number {
    return this.undoStack.length;
  }

  getUndoCount(): number {
    if (this.nativeQueryAuthoritative()) {
      return getNativeUndoCount(this.nativeQuery!);
    }
    return this.undoStack.length;
  }

  getRedoCount(): number {
    if (this.nativeQueryAuthoritative()) {
      return getNativeRedoCount(this.nativeQuery!);
    }
    return this.redoStack.length;
  }
}
