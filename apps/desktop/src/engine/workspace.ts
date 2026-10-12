import type { DocumentId, DocumentTabSummary } from "./types";
import { MAX_OPEN_DOCUMENTS } from "./types";
import { DocumentEngine, releaseFacadeOwned } from "./document";
import { CommandHistory } from "./history";
import { releaseBitmapStore } from "./bitmapStore";
import { clearNativeSeed, createNativeSeed, seedNativeCanonical } from "@/lib/protocol/bridge";
import { buildCanonicalDocumentPayload } from "@/lib/protocol/canonicalSeed";
import { removeFacade } from "@/lib/protocol/facadeRegistry";
import {
  awaitBackgroundFlagCommit,
  clearBackgroundFlagCommit,
  commitFacadeBackgroundFlag,
  isBackgroundFlagRouteArmed,
  markBackgroundFlagOnModel,
  trackBackgroundFlagCommit,
} from "@/lib/protocol/backgroundFlagRouting";
import type { RenderLayer } from "@/lib/protocol/types";

export interface DocumentSession {
  engine: DocumentEngine;
  history: CommandHistory;
  displayName: string;
  sourcePath: string | null;
  dirty: boolean;
}

export class WorkspaceManager {
  private sessions: Map<DocumentId, DocumentSession> = new Map();
  private activeDocumentId: DocumentId | null = null;
  private onChangeListeners: Set<() => void> = new Set();
  private onVisualChangeListeners: Set<() => void> = new Set();

  // C5.1: notify the Rust canonical pixel registry of document open/close so the
  // document-namespaced pixel storage is created/released deterministically.
  // Guarded: no-op outside the Tauri runtime (tests / web build).
  private notifyRustPixelDoc(
    cmd: "rust_pixels_open_document" | "rust_pixels_close_document",
    id: DocumentId,
  ): void {
    if (typeof (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ === "undefined") return;
    import("@tauri-apps/api/core")
      .then((m) => (m as { invoke: (c: string, a: Record<string, unknown>) => Promise<unknown> }).invoke(cmd, { docId: id }))
      .catch(() => {});
  }

  // ─── Document Lifecycle ───
  /**
   * Open a factory-built session: install it only once the factory's own
   * background-flag commit - which is what seeds this document natively and
   * marks its layers facade-owned - has landed.
   *
   * WHY THE OPEN WAITS FOR IT. That commit is not a cosmetic flag step. It is
   * the only thing that both seeds the native engine with this document's
   * layers and hands them to the facade ledger, so a commit that never lands
   * leaves a document the native engine does not hold and the ledger does not
   * claim. `facadeOwnedIdsOf(docId)` - the ACTIVE document's claims in the
   * ledger - is what the undo dispatch reads, so such a document's undo runs
   * on the host stack instead of the native walker - the executor the
   * native-walker's retirement removes. Opening it anyway is what
   * this gate exists to prevent; the project-open route refuses a refused seed
   * for the same reason.
   *
   * A refused open leaves nothing behind: the per-document state the aborted
   * attempt created is dropped - the tracked commit, and the facade entry,
   * because a surviving facade seeded with the discarded engine's layers makes
   * a retry under the same id skip its own seed - and the rasters the discarded
   * engine held are closed, because no session will own them. The rejected SEED
   * entry needs no drop here: `createNativeSeed` evicts its own recorded promise
   * on rejection, guarded by identity so an earlier attempt's late rejection
   * cannot evict a later seed's entry.
   *
   * Routes that opt the guard out (`photrez.bgFlagRoute=0`) track no commit, so
   * this resolves immediately and the legacy direct setter stands.
   */
  async addSeededDocument(session: DocumentSession): Promise<void> {
    const id = session.engine.getId();
    try {
      await awaitBackgroundFlagCommit(id);
    } catch (err) {
      clearBackgroundFlagCommit(id);
      // The per-document facade entry is per-document state this attempt
      // created too: the refused seed seeded it with the discarded engine's
      // layers and bound it to that engine. It survives the abort, and because
      // `commitFacadeBackgroundFlag` only seeds a facade that holds no layers, a
      // retry under the same id then skips the seed and applies to a native
      // engine that was never seeded (`document not open: <id>`). Drop it the
      // way the seed entry is dropped - but only when nothing is open under this
      // id, because the entry is keyed by DOCUMENT id and a live same-id session
      // is driving that very facade.
      if (!this.sessions.has(id)) removeFacade(id);
      for (const layer of session.engine.getLayers()) {
        layer.imageBitmap?.close();
        layer.baseImageBitmap?.close();
      }
      throw new Error(
        `Document open failed: the canonical document seed was refused (${
          err instanceof Error ? err.message : String(err)
        })`,
      );
    }
    this.addDocument(session);
  }

  addDocument(session: DocumentSession): void {
    if (this.sessions.size >= MAX_OPEN_DOCUMENTS) {
      throw new Error(`Maximum open document limit of ${MAX_OPEN_DOCUMENTS} reached`);
    }

    const id = session.engine.getId();
    this.sessions.set(id, session);
    this.notifyRustPixelDoc("rust_pixels_open_document", id);
    this.activeDocumentId = id;

    // Native-authority cutover seed: this is the earliest point that holds the
    // live engine model with its real layers, and it always precedes every
    // protocol command for the doc. Seed the native engine here with the REAL
    // layers + starting version so a later bridge command can never seed it
    // empty (which would clobber the TS model).
    {
      const seededLayers: RenderLayer[] = session.engine.getLayers().map((l) => ({
        id: l.id,
        name: l.name,
        visible: l.visible,
        opacity: l.opacity,
        resourceId: 0,
        x: l.transform.x,
        y: l.transform.y,
        scaleX: l.transform.scaleX,
        scaleY: l.transform.scaleY,
        rotation: l.transform.rotation,
      }));
      createNativeSeed(id, 0, seededLayers).catch(() => {});
      // Full canonical-document shadow (all layer content + selection) so the native
      // engine holds a complete typed copy. Ordered BEHIND the factory's in-flight
      // background-flag commit: seed_canonical replaces the native layer set, so a
      // payload built before the flag apply would carry no flag and clobber it. The
      // seed promise registers only after the commit settles (the commit's own
      // apply awaits the seed barrier), which is what keeps this ordering from
      // deadlocking. Fire-and-forget, same as the layer seed; log a failed push
      // instead of swallowing it so a missing shadow is visible.
      void awaitBackgroundFlagCommit(id)
        .then(() => seedNativeCanonical(id, buildCanonicalDocumentPayload(session.engine)))
        .catch((e) => console.warn("[canonical-seed] shadow seed failed", e));
    }

    // Connect document engine change triggers back to workspace context updates.
    // Only mark session as dirty if the engine itself reports dirty — this way
    // undo/redo to a clean snapshot (model.dirty === false) correctly clears the
    // dirty indicator. Save handlers must call engine.clearDirty() to keep the
    // engine dirty flag in sync (@regression 2026-07-03: session.dirty was always
    // set to true unconditionally, making every document appear dirty on open).
    session.engine.onChange(() => {
      if (session.engine.isDirty()) {
        session.dirty = true;
      }
      this.notifyChange();
    });
    session.engine.onVisualChange(() => {
      // Always sync session.dirty with engine.isDirty() — this ensures that
      // undo to a clean snapshot (engine.restore → notifyVisualChange with
      // model.dirty === false) correctly clears the session dirty flag.
      // Previously this was set unconditionally to true, which left the
      // document stuck dirty after undo (@regression 2026-07-06).
      session.dirty = session.engine.isDirty();
      this.notifyVisualChange();
    });
    // The derived history view re-reads the native stream asynchronously; the
    // workspace must republish when that read lands or the History panel keeps
    // serving the stale row set.
    session.history.onDerivedViewChanged = () => this.notifyVisualChange();

    this.notifyChange();
  }

  removeDocument(id: DocumentId): void {
    if (this.sessions.has(id)) {
      // Detach the engine's change callbacks entirely (instead of swapping
      // them for no-op closures) so a removed engine holds no reference back
      // into workspace state (review #40), AND retire it so its facade-ownership
      // writes are inert. Retirement is not the same as the callback detach: the
      // closed engine stays reachable by anything that captured it (an in-flight
      // funnel holds the engine it was driving), and its projection reconciles
      // ownership under `getId()` - which a reopen of the same file re-mints
      // under the SAME id. Without this, a projection through the closed engine
      // after a close-then-reopen releases the REOPENED session's claims and
      // disarms its undo.
      const session = this.sessions.get(id)!;
      session.engine.retire();
      // Drop the tracked factory flag commit so its settled closure cannot
      // retain the removed engine; a reopen re-tracks via the factory.
      clearBackgroundFlagCommit(id);

      const index = Array.from(this.sessions.keys()).indexOf(id);
      this.sessions.delete(id);
      this.notifyRustPixelDoc("rust_pixels_close_document", id);
      // Release this document's facade-ownership claims. Ownership is recorded
      // per document, so this drops only the closing document's claims - a
      // sibling document opened from the same project carries the same layer ids
      // and keeps its ownership (and its native undo walker) across this close.
      releaseFacadeOwned(id, session.engine.getLayers().map((l) => l.id));
      // Drop the doc's token registry (does NOT close bitmaps — close ownership
      // stays with the history disposeSnapshot path / GC; see bitmapStore).
      releaseBitmapStore(id);

      // Native-authority cutover seed: drop this doc's native seed/adapter state
      // so reopening the same id re-seeds the engine with the restored model
      // (a stale resolved promise would otherwise skip the re-seed).
      clearNativeSeed(id);
      // Evict the per-doc facade alongside the native seed so a reopened doc id
      // gets a FRESH facade (not a stale one at an old renderedVersion).
      removeFacade(id);

      // consecutive assignments to `this.activeDocumentId` where the
      // first (line 53) computed `keys.indexOf(id) + 1` against the
      // already-mutated `keys` array — `indexOf(id)` returned `-1`
      // after the delete, so `+ 1` resolved to `0`, picking the
      // first document instead of the neighbor. Line 56 then
      // immediately overwrote that value with a correct computation
      // against `remainingKeys`, so behavior was accidentally right
      // but the dead-code path was a foot-gun if anyone refactored
      // the trailing line.

      // Adjust active document focus: activate the neighbor at the
      // same index, or the new last document if the removed one was
      // last.
      if (this.activeDocumentId === id) {
        if (this.sessions.size > 0) {
          const remainingKeys = Array.from(this.sessions.keys());
          this.activeDocumentId = remainingKeys[Math.min(index, remainingKeys.length - 1)];
        } else {
          this.activeDocumentId = null;
        }
      }

      this.notifyChange();
    }
  }

  switchDocument(id: DocumentId): void {
    if (this.sessions.has(id)) {
      this.activeDocumentId = id;
      this.notifyChange();
    }
  }

  reorderDocument(fromIndex: number, toIndex: number): void {
    const entries = Array.from(this.sessions.entries());
    if (
      fromIndex < 0 ||
      fromIndex >= entries.length ||
      toIndex < 0 ||
      toIndex >= entries.length ||
      fromIndex === toIndex
    ) {
      return;
    }
    const [moved] = entries.splice(fromIndex, 1);
    entries.splice(toIndex, 0, moved);
    this.sessions = new Map(entries);
    this.notifyChange();
  }

  // ─── Accessors ───
  getActiveSession(): DocumentSession | null {
    if (!this.activeDocumentId) return null;
    return this.sessions.get(this.activeDocumentId) || null;
  }

  getActiveEngine(): DocumentEngine | null {
    const session = this.getActiveSession();
    return session ? session.engine : null;
  }

  getActiveHistory(): CommandHistory | null {
    const session = this.getActiveSession();
    return session ? session.history : null;
  }

  getSession(id: DocumentId): DocumentSession | null {
    return this.sessions.get(id) || null;
  }

  getSessions(): DocumentSession[] {
    return [...this.sessions.values()];
  }

  getEngine(id: DocumentId): DocumentEngine | null {
    return this.getSession(id)?.engine ?? null;
  }

  getHistory(id: DocumentId): CommandHistory | null {
    return this.getSession(id)?.history ?? null;
  }

  getDocumentCount(): number {
    return this.sessions.size;
  }

  isFull(): boolean {
    return this.sessions.size >= MAX_OPEN_DOCUMENTS;
  }

  getActiveDocumentId(): DocumentId | null {
    return this.activeDocumentId;
  }

  getTabSummaries(): DocumentTabSummary[] {
    return Array.from(this.sessions.entries()).map(([id, session]) => ({
      id,
      displayName: session.displayName,
      isDirty: session.dirty
    }));
  }

  // ─── Change Notification ───
  // Multiple listeners supported (Set instead of single-slot): a removed
  // listener must unsubscribe itself — there is no automatic cleanup.
  onChange(callback: () => void): () => void {
    this.onChangeListeners.add(callback);
    return () => {
      this.onChangeListeners.delete(callback);
    };
  }

  private notifyChange(): void {
    for (const cb of this.onChangeListeners) {
      cb();
    }
  }

  onVisualChange(callback: () => void): () => void {
    this.onVisualChangeListeners.add(callback);
    return () => {
      this.onVisualChangeListeners.delete(callback);
    };
  }

  notifyVisualChange(): void {
    for (const cb of this.onVisualChangeListeners) {
      cb();
    }
  }

  // ─── Factory Methods ───
  static createBlankDocument(
    id: DocumentId,
    name: string,
    width: number,
    height: number,
    options?: { backgroundColor?: "white" | "transparent" }
  ): DocumentSession {
    const engine = new DocumentEngine(id, name, width, height);
    const bg = engine.addLayer("Background"); // Default empty background layer
    // Route through Rust so graph guards (delete bg / reorder bg-pin) apply.
    // The factory cannot await - it is a synchronous constructor - so it fires
    // the commit and hands the caller its promise; `addSeededDocument` is the
    // open gate that awaits it and refuses the open when the native seed behind
    // it was refused. The model still gets the flag synchronously
    // (markBackgroundFlagOnModel), so the canonical payload and synchronous
    // readers see it while the native apply is still in flight.
    if (isBackgroundFlagRouteArmed()) {
      markBackgroundFlagOnModel(engine, bg.id);
      const commit = commitFacadeBackgroundFlag(engine as never, [bg.id]);
      // Mark the rejection observed for a caller that never opens the session
      // (a factory-built session that is discarded), so a refused seed is not an
      // unhandled rejection. The tracked promise still rejects for the gate.
      void commit.catch(() => {});
      trackBackgroundFlagCommit(engine.getId(), commit);
    } else {
      engine.markLayerAsBackground(bg.id);
    }

    if (options?.backgroundColor === "white") {
      const canvas = new OffscreenCanvas(width, height);
      const ctx = canvas.getContext("2d");
      if (ctx) {
        ctx.fillStyle = "#ffffff";
        ctx.fillRect(0, 0, width, height);
        bg.imageBitmap = canvas.transferToImageBitmap();
      }
    }

    engine.clearDirty();

    const history = new CommandHistory();
    history.attachLiveBitmapGetter(() =>
      engine.getModel().layers.flatMap((l) =>
        [l.imageBitmap, l.baseImageBitmap].filter((b): b is ImageBitmap | null => b !== undefined),
      ),
    );
    // Phase 1: lets the history bridge append commits to the correct Rust cursor.
    history.attachDocIdGetter(() => engine.getModel().id);

    return {
      engine,
      history,
      displayName: name,
      sourcePath: null,
      dirty: false
    };
  }

  static createDocumentFromImage(
    id: DocumentId,
    name: string,
    bitmap: ImageBitmap
  ): DocumentSession {
    const engine = new DocumentEngine(id, name, bitmap.width, bitmap.height);
    const bgLayer = engine.addLayer("Background", bitmap.width, bitmap.height);
    engine.setLayerImageBitmap(bgLayer.id, bitmap);
    // Route through Rust so graph guards (delete bg / reorder bg-pin) apply.
    // Same shape as createBlankDocument's commit, including why there is no
    // fallback: the open gate is `addSeededDocument`.
    if (isBackgroundFlagRouteArmed()) {
      markBackgroundFlagOnModel(engine, bgLayer.id);
      const commit = commitFacadeBackgroundFlag(engine as never, [bgLayer.id]);
      void commit.catch(() => {});
      trackBackgroundFlagCommit(engine.getId(), commit);
    } else {
      engine.markLayerAsBackground(bgLayer.id);
    }
    engine.clearDirty();

    const history = new CommandHistory();
    history.attachLiveBitmapGetter(() =>
      engine.getModel().layers.flatMap((l) =>
        [l.imageBitmap, l.baseImageBitmap].filter((b): b is ImageBitmap | null => b !== undefined),
      ),
    );
    // Phase 1: lets the history bridge append commits to the correct Rust cursor.
    history.attachDocIdGetter(() => engine.getModel().id);

    return {
      engine,
      history,
      displayName: name,
      sourcePath: null,
      dirty: false
    };
  }
}
