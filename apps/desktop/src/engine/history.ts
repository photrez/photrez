import type { DocumentModel, LayerNode } from "./types";
import { MAX_HISTORY_DEPTH } from "./types";
import type { TileUploadLike } from "../renderer/types";
import {
  bitmapStoreFor,
  existingTokenForBitmap,
  tokenForBitmap,
  type BitmapField,
} from "./bitmapStore";
import { syncFacadeVersionFromPixel, historyDegraded } from "@/lib/protocol/facadeRegistry";
import { isFacadeEnabled } from "@/lib/protocol/bridge";
import { RustCursorStepper } from "./historyCursorStep";
import { historyBridgeEnabled } from "./historyBridgeGate";
import type { SnapshotLayerMeta, SnapshotPayload } from "./snapshotTokenReattach";

/**
 * Route every TS commit into the SAME Rust `ProtocolEngine` cursor so TS and Rust
 * operations share ONE logical history position.
 *
 * The gate itself lives in `historyBridgeGate.ts` (one predicate, read by every
 * recording and stepping site); it is re-exported here so importers of this class
 * and importers of the predicate cannot drift onto two different gates.
 */
export { historyBridgeEnabled } from "./historyBridgeGate";
export type { SnapshotLayerMeta, SnapshotPayload, SnapshotLayerIdentity } from "./snapshotTokenReattach";
export { restoreSnapshotBitmapsByToken } from "./snapshotTokenReattach";

/**
 * TRANSITIONAL (`photrez.rustPixels`): the flag's last surviving production read.
 *
 * It used to arm the undo/redo tile-branch fetch in useEditorCommands, i.e. "fetch
 * this step's pixels from Rust instead of replaying the entry's own memento" for a
 * tile entry Rust does NOT own. That decision now lives with the cursor step it was
 * fused to (CommandHistory.stepRustCursor), so the flag cannot outlive the tile
 * routing: retire it when the flag retires, and delete this read with it.
 */
function rustPixelsFlagEnabled(): boolean {
  try {
    return typeof localStorage !== "undefined" && localStorage.getItem("photrez.rustPixels") === "1";
  } catch {
    return false;
  }
}

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
function disposeSnapshot(
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

interface SnapshotEntry {
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
   * Whether the most recent `commit()` appended an entry to the Rust stream
   * through the history bridge. Read by `installFacadeCommitShim` immediately
   * after `commit()` returns, so exactly one of the two recorders fires per host
   * commit. Public because the shim is a prototype wrapper outside this class.
   */
  private lastCommitRecordedInBridge = false;
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
   * Did the commit that just ran record its own Rust entry through the bridge?
   *
   * `installFacadeCommitShim` wraps `commit` and mirrors it into an `External`
   * entry; when the bridge was also active BOTH recorders appended an entry for
   * the same host commit, and a single cursor step per pop could not keep up with
   * a stream gaining two entries per commit. The shim reads this and stands down,
   * which is what makes "one host entry => one Rust entry => one cursor step" hold
   * with both recorders enabled.
   *
   * Only meaningful immediately after `commit()` on the same instance - it is a
   * per-commit signal, not history state.
   */
  get committedRecordedInBridge(): boolean {
    return this.lastCommitRecordedInBridge;
  }

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
   * THE bridge precondition, in one place, because recording and stepping must never
   * disagree: this history appends to a Rust stream only when the bridge is on AND a
   * doc-id getter is attached, so only then may a pop step the cursor.
   *
   * `commit`/`recordSnapshotHistory` and `stepRustCursor` all gate on THIS, not on
   * `historyBridgeEnabled()` alone. That asymmetry was a live defect:
   * `editorOpenImage.loadProjectFile` builds a `CommandHistory` WITHOUT a getter
   * (every other production history attaches one) and that is the File>Open path, so
   * its step consumed a brush stroke's Pixel entry - recorded by the ungated
   * `rust_pixels_write_region`.
   */
  private bridgeRecordsFor(): boolean {
    // `!= null`, not `!== null`: the field is typed `(() => string) | null` today, but
    // an `undefined` slipping in would make the strict form report "records" and then
    // throw inside the caller's `try {} catch {}`, which swallows it - a silent
    // no-record with a cursor step already fired, i.e. exactly the asymmetry this
    // predicate exists to remove.
    return historyBridgeEnabled() && this.docIdGetter != null;
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

  /**
   * Build a token snapshot payload for a model (bridge-ON only). Each layer's
   * primary bitmap (imageBitmap, or baseImageBitmap when there is no primary)
   * is registered in the per-doc BitmapStore under a STABLE token (same bitmap
   * object -> same token), so a later undo/redo re-attaches the exact same
   * bitmap by token — never a detached or different one.
   */
  private buildSnapshotPayload(model: DocumentModel): SnapshotPayload {
    const store = bitmapStoreFor(model.id);
    const layers: SnapshotLayerMeta[] = model.layers.map((l) => {
      const epoch = l.bitmapEpoch ?? 0;
      // Remember which field the primary bitmap came from so a base-only layer
      // (imageBitmap=null) re-attaches to baseImageBitmap, never imageBitmap.
      const primaryField: BitmapField | null = l.imageBitmap ? "imageBitmap" : l.baseImageBitmap ? "baseImageBitmap" : null;
      const primary = l.imageBitmap ?? l.baseImageBitmap ?? null;
      let bitmapToken: string | null = null;
      if (primary && primaryField) {
        bitmapToken = tokenForBitmap(primary);
        store.set(bitmapToken, primary, primaryField);
      }
      return {
        layerId: l.id,
        width: l.width,
        height: l.height,
        bitmapToken,
        epoch,
        pixelVersion: epoch,
      };
    });
    return { docId: model.id, version: 0, layers };
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
    // Gated by the runtime DEV flag localStorage["photrez.historyBridge"] === "1"
    // (plus isTauriRuntime, folded into historyBridgeEnabled()).
    //
    // Whether THIS commit appended an entry to the Rust stream by itself. Read by
    // the commit shim so it can stand down when the bridge already recorded, which
    // is what keeps "one host entry => exactly one Rust entry" true when both
    // recorders are active. Without it both fire on the same commit and a single
    // cursor step can no longer keep up with the stream.
    this.lastCommitRecordedInBridge = false;
    // Set by the record's own rejection, on THIS entry only. Declared before `fire`
    // because the closure below needs it, and assigned after the push below - same
    // object either way, since `stepRustCursor` reads the popped entry.
    const entry: SnapshotEntry = {
      snapshot,
      timestamp: Date.now(),
      lastPaintCoords: this.currentLastPaintCoords,
      label,
      imperative,
      pixelLayerIds: pixelLayerIds ?? null,
    };
    if (this.bridgeRecordsFor()) {
      const docId = this.docIdGetter!();
      // Fire-and-forget through the shared bridge entry, so the census registrar
      // is installed and the invoke carries one monotonic order. A failure must
      // never break TS history.
      const fire = (cmd: string, args: Record<string, unknown>) =>
        import("@/lib/protocol/bridge")
          .then(({ invokePixelCommand }) => invokePixelCommand(cmd, args))
          .catch(() => {
            // The record did NOT land, so the flag the shim reads is now a lie: it
            // stands down in good faith and nothing exists in the stream. Mark THIS
            // entry so `stepRustCursor` leaves its ungated path - see
            // `SnapshotEntry.rustRecordFailed` for why this is not a history latch.
            entry.rustRecordFailed = true;
          });
      try {
        if (imperative && !alreadyRecordedInRust) {
          this.lastCommitRecordedInBridge = true;
          fire("apply_tile_patch", {
            docId,
            layerId: imperative.layerId,
            before: imperative.before,
            after: imperative.after,
          });
        } else if (!alreadyRecordedInRust) {
          this.lastCommitRecordedInBridge = true;
          fire("rust_pixels_record_external", {
            docId,
            label: label ?? "ts-meta",
            affected: snapshot.activeLayerId ? [snapshot.activeLayerId] : [],
            adapterId: "ts",
            token: label ?? "ts-meta",
            // Host-owned size halves. Both absent for a size-neutral commit; both
            // present and differing for a commit that resizes the document.
            docSizeBefore: docSizeChange
              ? [docSizeChange.before.width, docSizeChange.before.height]
              : null,
            docSizeAfter: docSizeChange
              ? [docSizeChange.after.width, docSizeChange.after.height]
              : null,
          });
        }
      } catch {
        /* bridge is best-effort; a failure must never break TS history */
      }
    }

    this.undoStack.push(entry);

    // Clear redo stack on new operation
    this.redoStack = [];

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
   * post-action document states explicitly (bridge-ON only).
   *
   * Contract-correct before/after derivation: at a PRE-action commit the post-
   * action state does not yet exist, so the bridge can NEVER infer `after` from
   * the commit arg or from the stack. The caller passes both states, so:
   *   - `before` = the PRE-action state (what TS undo restores / what Rust
   *     `undo_snapshot` returns);
   *   - `after`  = the POST-action state (what Rust `redo_snapshot` returns).
   * There is NO stack inference — the payloads are built directly from the two
   * arguments, so undo can never return a state two-commits back (the off-by-one
   * the old `commit()` snapshot branch had).
   *
   * Gated by `historyBridgeEnabled()` + `docIdGetter` (same as commit). Fires
   * `rust_pixels_record_snapshot({ docId, before: beforePayload, after: afterPayload })`
   * via the existing dynamic-import `invoke` + `.catch(()=>{})`, wrapped in
   * try/catch (bridge is best-effort, must NEVER break TS history). This is the
   * SOLE `rust_pixels_record_snapshot` entry point.
   *
   * TS undo/redo authority stays unchanged: the PRE-action `before` state is
   * pushed as the undo-point, the redo stack is cleared, and `maxDepth` +
   * `disposeSnapshot(evicted.snapshot, live)` are enforced exactly as commit().
   */
  recordSnapshotHistory(before: DocumentModel, after: DocumentModel, label?: string, pixelLayerIds: string[] | null = null): void {
    // Same one-recorder-per-commit contract as `commit()`. Without this the shim's
    // snapshot wrapper cannot know the bridge already recorded, so with bridge ON
    // and facade ON one snapshot-typed commit appended BOTH a Snapshot entry (here)
    // and an External (the shim) - two entries, one host commit.
    this.lastCommitRecordedInBridge = false;
    // The entry this commit pushes, so a failed bridge record can be marked ON IT.
    // Built first because the push below must carry the very object the pop reads.
    const entry: SnapshotEntry = {
      snapshot: before,
      timestamp: Date.now(),
      lastPaintCoords: this.currentLastPaintCoords,
      label,
      snapshotType: "snapshot",
      pixelLayerIds: pixelLayerIds ?? null,
    };
    if (this.bridgeRecordsFor()) {
      const docId = this.docIdGetter!();
      // Registrations happen inside buildSnapshotPayload (idempotent per bitmap
      // object). before = the PRE-action state, after = the POST-action state.
      //
      // The payload build is INSIDE the try, and the recorder flag is set only after
      // it succeeds. It used to be set first, with the build outside: a throw from
      // `buildSnapshotPayload` then left the flag true, so the shim stood down in good
      // faith and NOTHING was recorded anywhere - one host entry, zero Rust entries.
      // Fire-and-forget through the shared bridge entry (census order + registrar).
      const fire = (cmd: string, args: Record<string, unknown>) =>
        import("@/lib/protocol/bridge")
          .then(({ invokePixelCommand }) => invokePixelCommand(cmd, args))
          .catch(() => {
            // The record did NOT land, and the shim stood down on a flag that says it
            // did. Mark THIS entry so nothing later steps onto a tip that is not its
            // own. Same treatment as `commit`'s fire - see
            // `SnapshotEntry.rustRecordFailed`.
            entry.rustRecordFailed = true;
          });
      try {
        const beforePayload = this.buildSnapshotPayload(before);
        const afterPayload = this.buildSnapshotPayload(after);
        // Set only once the payload is built AND the record is on its way, so the flag
        // the shim reads can never claim a record that was never issued.
        this.lastCommitRecordedInBridge = true;
        fire("rust_pixels_record_snapshot", { docId, before: beforePayload, after: afterPayload })
          .then((res) => {
            // Native-authority version sync: the snapshot command advances the
            // native engine document version; push it into the facade so a later
            // facade command is not rejected with E_VERSION_MISMATCH. Gated no-op
            // unless native authority is active.
            const v = (res as { version?: number } | undefined)?.version;
            if (typeof v === "number") syncFacadeVersionFromPixel(docId, v);
          })
          .catch(() => {});
      } catch {
        /* bridge is best-effort; a failure must never break TS history */
      }
    }

    // TS undo/redo authority: the undo-point is the PRE-action state (unchanged
    // convention) — undo restores `before`, which is what Rust undo_snapshot
    // returns for this step.
    this.undoStack.push(entry);

    // Clear redo stack on new operation
    this.redoStack = [];

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

  canUndo(): boolean {
    return this.undoStack.length > 0;
  }

  canRedo(): boolean {
    return this.redoStack.length > 0;
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
   *    (brush, eraser, bucket, seeded fill, gradient, delete-pixels), whatever the
   *    bridge flag says;
   *  - TRANSITIONAL `photrez.rustPixels`, tile entries only. The one arm that can
   *    step without a recorded counterpart, left as-is because it predates this
   *    method and retiring the flag is a separate cleanup;
   *  - `bridgeRecordsFor()`: the bridge, for BOTH entry kinds. A metadata commit
   *    appended an External entry through `rust_pixels_record_external` (all-scalar
   *    args) and a TS-owned tile commit appended a Pixel entry through
   *    `apply_tile_patch`, so each has the counterpart its pop consumes. The tile
   *    arm only reached the stream once `TilePatchWire` accepted the host's
   *    `TileUploadLike` spelling (`{x, y, width, height, data}`) as an alias of
   *    `w`/`h` - see `both_tile_shapes_are_accepted_at_the_wire_and_mint_exactly_one_entry`
   *    in paint_parity_cmds.rs, which drives both spellings through the real Tauri
   *    argument deserializer. Until that alias existed the bridge excluded tile
   *    entries here, which is why a bridge-ON tile pop used to leave the cursor
   *    behind.
   *
   * The bridge arm is gated on `bridgeRecordsFor()` - the SAME predicate this
   * class's own `commit` records on - so a history with no doc-id getter records
   * nothing and therefore steps nothing.
   *
   * "THE SAME PREDICATE" IS TRUE OF THE BRIDGE ONLY, and that limit is load-bearing.
   * It is NOT true of every writer to this stream. `installFacadeCommitShim`
   * (lib/protocol/facadeRegistry.ts) mirrors EVERY commit into an `External`
   * entry under `isFacadeEnabled()` - which reads `photrez.facade !== "0"`, so it
   * is ON unless explicitly opted out - while this method gates stepping on
   * `historyBridgeEnabled()`, which needs `photrez.historyBridge === "1"` AND the
   * Tauri runtime, so it is OFF unless explicitly opted in. At those two shipping
   * defaults the sides disagree: a metadata commit appends an `External` entry
   * that no pop will step, and the cursor is left pointing at it. The next
   * `rustOwned` pop then inherits it as its tip, and because
   * `ProtocolEngine::undo_pixel` moves the cursor over an `External` tip while
   * yielding no tiles (crates/core/src/history.rs:229-233, collapsed to `None`
   * AFTER the move by pixel_store.rs:745), that step would consume a
   * host-handoff entry instead of the paint entry and revert nothing.
   *
   * So the two entry-shaped arms below are additionally gated on the Rust tip
   * ACTUALLY being a `Pixel` entry, read through `rust_pixels_history_tip` before
   * the step is issued (`RustCursorStepper.fireGatedOnPixelTip`). The bridge arm is
   * deliberately NOT gated that way: the bridge recorded the counterpart its own
   * pop consumes, so an `External` tip there is the correct entry to move over and
   * refusing it would strand every metadata undo.
   *
   * It is one predicate for the bridge, so no two arms can co-fire: `undo()`/
   * `redo()` call `stepRustCursor` once per popped entry, and no restore path
   * issues a second invoke (`rustTileProjection` only AWAITS the step the pop
   * fired). A pop that leaves its step unconsumed - a bridge-ON TS-owned tile pop
   * with no Rust fetch armed - drops it on the next pop's leading `take()`, so it
   * is issued once and never re-issued.
   *
   * A snapshot-typed entry is NOT stepped: its cursor step is the token re-attach's
   * `rust_pixels_undo_snapshot` / `..._redo_snapshot`, and `undo_pixel` refuses to
   * move the cursor for a Snapshot tip (crates/core/src/history.rs:239).
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
    const bridgeArm = this.bridgeRecordsFor();
    // The shim arm: `installFacadeCommitShim` mirrors EVERY commit that is not
    // `alreadyRecordedInRust` into an `External` entry, under `isFacadeEnabled()`
    // and NOT under the history bridge. So with the bridge off - the shipping
    // default - a metadata or TS-owned tile commit records an entry whose only
    // possible consumer is this pop. Without this arm that entry is never stepped
    // and the two cursors separate by one per undone step, which is the drift the
    // tip-kind gate then refuses to cross (leaving a Ctrl+Z that reverts nothing).
    //
    // It cannot double-step against the bridge: `commit` marks the entries it
    // recorded itself, and the shim SKIPS those commits precisely so only one of
    // the two recorders ever fires for a given host entry. This arm is therefore
    // reached only when the bridge did NOT record, i.e. exactly when the shim did.
    //
    // `facadeMoved` and `entry.rustRecordFailed` both demote it to the GATED path.
    // The ungated path is only sound when this pop's own entry is known to be the tip,
    // and either fact destroys that knowledge: the handoff already consumed the
    // cursor, or this entry's own bridge record was rejected so nothing was recorded
    // for it at all. The gate refuses a tip that is not this entry's, which is the safe
    // outcome for both.
    //
    // Both are PER PRESS or PER ENTRY, never per history. A history-wide condition here
    // would demote every later pop too, and the demotion is a REFUSAL: an unrelated
    // entry whose `External` tip is perfectly correct would stop advancing the cursor.
    //
    // The last two clauses mirror the shim's OWN early-returns rather than
    // restating them, for the reason `rustPixelsFlagEnabled` above exists: this
    // predicate decides whether to take the UNGATED path, where a step with no entry
    // behind it consumes somebody else's. `isFacadeEnabled()` is the shim's feature
    // gate; `historyDegraded()` is the one `recordExternalTransitionFor` returns early
    // on, and that latch is only ever CLEARED by the test reset - so one failed
    // external transition turns recording off for the rest of the session, and
    // without this clause the arm would keep firing ungated with nothing behind it.
    // The shim's third early-return, no active engine, cannot be mirrored: the
    // history holds no engine handle, and a host pop implies a live document.
    //
    // TRANSITIONAL (`photrez.facade`): the whole arm, like the shim it consumes.
    const shimArm =
      !facadeMoved &&
      entry.rustRecordFailed !== true &&
      !bridgeArm &&
      entry.imperative?.rustOwned !== true &&
      isFacadeEnabled() &&
      !historyDegraded();
    const entryArm =
      entry.imperative?.rustOwned === true ||
      (entry.imperative !== undefined && rustPixelsFlagEnabled());
    if (!entryArm && !bridgeArm && !shimArm) return;
    const docId = entry.snapshot.id || this.docIdGetter?.();
    if (!docId) return;
    // "" is the honest "this step names no layer" answer for a metadata entry: the
    // command only reads layer_id to report an epoch when it produced no tiles.
    const layerId = entry.imperative?.layerId ?? entry.snapshot.activeLayerId ?? "";
    const req = { direction, docId, layerId };
    if (facadeMoved) return; // the handoff's cursor move IS this press's step
    if ((bridgeArm || shimArm) && entry.rustRecordFailed !== true) {
      // The recorder that owns this entry is not the pixel writer: the bridge when
      // it recorded, the shim when it did. Either way the tip for this pop is the
      // entry the recorder appended, so there is nothing to check and an `External`
      // tip is the CORRECT entry to move over. Refusing it here would strand every
      // metadata undo - which is the whole reason the shim records these at all.
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
   * business taking its tiles. The tile path is that caller for a bridge-ON
   * TS-owned tile pop: its fetch gate is `rustOwned || photrez.rustPixels`, so
   * `projectRustTiles` returns early and never claims the step - yet the parity
   * probe fires right afterwards over the same registry mutex. Awaiting this
   * orders the probe's read after the move without stealing the step from the
   * next pop's leading `take()`.
   */
  lastCursorStepSettled(): Promise<unknown> | null {
    return this.rustCursor.settled();
  }

  /**
   * Pop for a site that restores through the MODEL, never the Rust pixel path.
   *
   * The one such caller is the History panel's jump (`navigateHistory`), which loops
   * up to `steps` times and restores with `engine.restore` + `uploadImage`. Stepping
   * there would move Rust's canonical buffers and its cursor while the host restored
   * pixels from the TS stack - the mirror image of the drift this class prevents.
   *
   * NAMED rather than `undo(snap, false)`: an audit of "which pop sites do not step
   * the Rust cursor" must find them by grepping, and an anonymous boolean leaves
   * `grep stepCursor` pointing only at the declaration. `undo`/`redo` still default
   * `stepCursor` to true, so a pop that does not say otherwise owns its step.
   */
  undoThroughModel(currentSnapshot: DocumentModel): DocumentModel | null {
    return this.undo(currentSnapshot, false);
  }

  /** `redoThroughModel` - see `undoThroughModel`. */
  redoThroughModel(currentSnapshot: DocumentModel): DocumentModel | null {
    return this.redo(currentSnapshot, false);
  }

  /**
   * Pop the newest undo entry and return its snapshot. `stepCursor` is the
   * internal switch behind `undoThroughModel`; prefer that named form.
   */
  undo(currentSnapshot: DocumentModel, stepCursor = true): DocumentModel | null {
    if (!this.canUndo()) {
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

  getUndoCount(): number {
    return this.undoStack.length;
  }

  getRedoCount(): number {
    return this.redoStack.length;
  }
}
