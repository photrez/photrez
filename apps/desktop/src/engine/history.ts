import type { DocumentModel } from "./types";
import { MAX_HISTORY_DEPTH } from "./types";
import {
  disposeSnapshot,
  type HistoryTilePatches,
  type SnapshotEntry,
} from "./historyTypes";
export type { HistoryTilePatches, SnapshotEntry } from "./historyTypes";
import {
  buildNativeHistoryItems,
  getNativeRedoCount,
  getNativeUndoCount,
  isNativeQueryAuthoritative,
  resyncDerivedView,
  setHistoryViewAuthority,
  settleCommitRecorders,
} from "./nativeHistoryView";

/**
 * Route every TS commit into the SAME Rust `DocumentCore` cursor so TS and Rust
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
    docSizeChange?: { before: { width: number; height: number }; after: { width: number; height: number } },
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
   * No TS undo/redo pops the stacks: the native walker is the
   * undo/redo executor. The TS stacks remain an inert payload store (commit/
   * recordSnapshotHistory still push, eviction still runs, clear() still
   * disposes) — the pre-op state recorded here is what Rust external-entry
   * undo restores through the native stream.
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
    return false;
  }

  canRedo(): boolean {
    if (this.nativeQueryAuthoritative()) {
      return getNativeRedoCount(this.nativeQuery!) > 0;
    }
    return false;
  }

  /** Rebuild this stack's view from `protocol_history_query_native`; the never-rejects contract lives on `resyncDerivedView`. */
  async resyncFromNativeQuery(): Promise<void> {
    await resyncDerivedView(this.docIdGetter?.(), (query) => {
      this.nativeQuery = query;
      this.onDerivedViewChanged?.();
    });
  }

  getHistoryStack(): HistoryItem[] {
    if (this.nativeQueryAuthoritative()) {
      return buildNativeHistoryItems(this.nativeQuery!);
    }
    return [];
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
  }

  getUndoCount(): number {
    if (this.nativeQueryAuthoritative()) {
      return getNativeUndoCount(this.nativeQuery!);
    }
    return 0;
  }

  getRedoCount(): number {
    if (this.nativeQueryAuthoritative()) {
      return getNativeRedoCount(this.nativeQuery!);
    }
    return 0;
  }
}
