import type { DocumentId, DocumentTabSummary } from "./types";
import { MAX_OPEN_DOCUMENTS } from "./types";
import { DocumentEngine } from "./document";
import { CommandHistory } from "./history";

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

  // ─── Document Lifecycle ───
  addDocument(session: DocumentSession): void {
    if (this.sessions.size >= MAX_OPEN_DOCUMENTS) {
      throw new Error(`Maximum open document limit of ${MAX_OPEN_DOCUMENTS} reached`);
    }

    const id = session.engine.getId();
    this.sessions.set(id, session);
    this.activeDocumentId = id;

    // Connect document engine change triggers back to workspace context updates.
    // Only mark session as dirty if the engine itself reports dirty — this way
    // undo/redo to a clean snapshot (model.dirty === false) correctly clears the
    // dirty indicator. Save handlers must call engine.clearDirty() to keep the
    // engine dirty flag in sync (regression 2026-07-03: session.dirty was always
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
      // document stuck dirty after undo (regression 2026-07-06).
      session.dirty = session.engine.isDirty();
      this.notifyVisualChange();
    });

    this.notifyChange();
  }

  removeDocument(id: DocumentId): void {
    if (this.sessions.has(id)) {
      // Detach the engine's change callbacks entirely (instead of swapping
      // them for no-op closures) so a removed engine holds no reference back
      // into workspace state (review #40).
      const session = this.sessions.get(id)!;
      session.engine.clearCallbacks();

      const index = Array.from(this.sessions.keys()).indexOf(id);
      this.sessions.delete(id);

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
    bg.isBackground = true;
    bg.lockPosition = true;
    bg.lockRotation = true;

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

    return {
      engine,
      history: new CommandHistory(),
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
    bgLayer.isBackground = true;
    bgLayer.lockPosition = true;
    bgLayer.lockRotation = true;
    engine.clearDirty();

    return {
      engine,
      history: new CommandHistory(),
      displayName: name,
      sourcePath: null,
      dirty: false
    };
  }
}
