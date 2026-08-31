import type { DocumentModel, LayerNode } from "./types";
import { MAX_HISTORY_DEPTH } from "./types";
import type { TileUploadLike } from "../renderer/types";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";

/**
 * Phase 1 (History Unification): route every TS commit into the SAME Rust
 * `ProtocolEngine` cursor so TS and Rust operations share ONE logical history
 * position.
 *
 * Gated: the bridge is OFF by default in production. It is enabled only when
 * the runtime DEV gate `localStorage["photrez.historyBridge"] === "1"` is set
 * AND the app is running in the Tauri runtime. In default production the TS
 * `CommandHistory` remains the sole undo/redo authority (no Rust cursor append,
 * so no TS/Rust split-brain).
 */
const HISTORY_BRIDGE_GATE = "photrez.historyBridge";

/**
 * Reusable predicate (shared by the commit funnel AND the undo/redo cursor-sync
 * in useEditorCommands so the two can never drift): the TS→Rust history bridge
 * is enabled only when the runtime DEV gate `localStorage["photrez.historyBridge"]
 * === "1"` is set AND the app runs in the Tauri runtime. Default production OFF.
 */
export function historyBridgeEnabled(): boolean {
  return (
    typeof localStorage !== "undefined" &&
    localStorage.getItem(HISTORY_BRIDGE_GATE) === "1" &&
    isTauriRuntime()
  );
}

/**
 * Fase 1 tile store: imperative before/after tile patches for a paint commit.
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
}

/**
 * Release GPU/heap-backed ImageBitmaps held by a discarded snapshot. An
 * ImageBitmap is closed only when no other snapshot in the undo/redo stacks
 * and no live document model still references it — shared bitmaps across
 * snapshots must survive eviction of one entry.
 */
function disposeSnapshot(
  snap: DocumentModel,
  stillReferenced: (bitmap: unknown) => boolean,
): void {
  const closeIfUnused = (layer: LayerNode) => {
    if (layer.imageBitmap && !stillReferenced(layer.imageBitmap)) {
      try { layer.imageBitmap.close(); } catch { /* already closed */ }
    }
    if (layer.baseImageBitmap && !stillReferenced(layer.baseImageBitmap)) {
      try { layer.baseImageBitmap.close(); } catch { /* already closed */ }
    }
  };
  for (const layer of snap.layers) closeIfUnused(layer);
}

interface SnapshotEntry {
  snapshot: DocumentModel;
  timestamp: number;
  lastPaintCoords: { x: number; y: number } | null;
  label?: string;
  /** Fase 1 tile patches (paint commits). Presence marks an imperative entry. */
  imperative?: HistoryTilePatches;
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

  constructor(maxDepth: number = MAX_HISTORY_DEPTH) {
    this.maxDepth = maxDepth;
  }

  /**
   * Phase 1: supply the active document id so commits can be appended to the
   * correct Rust history cursor. Called once by the editor shell.
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
  ): void {
    // Phase 1: append this commit to the unified Rust history cursor.
    //  - imperative TS pixel op NOT yet in Rust (text/gradient/shape/transform)
    //    -> `apply_tile_patch` (Pixel entry, same command Rust strokes use).
    //  - non-pixel TS (metadata) op -> `rust_pixels_record_external` (External entry).
    //  - `alreadyRecordedInRust` (brush/fill/adjustment bake) -> Rust already owns
    //    the Pixel entry via `rust_pixels_write_region`; skip to avoid double-count.
    // Gated by the runtime DEV flag localStorage["photrez.historyBridge"] === "1"
    // (plus isTauriRuntime, folded into historyBridgeEnabled()).
    if (historyBridgeEnabled() && this.docIdGetter) {
      const docId = this.docIdGetter();
      // Dynamic import (matching document.ts) keeps the Tauri API out of the
      // module graph at import time so browser/test imports stay side-effect free.
      const fire = (cmd: string, args: Record<string, unknown>) =>
        import("@tauri-apps/api/core")
          .then(({ invoke }) => invoke(cmd, args))
          .catch(() => {});
      try {
        if (imperative && !alreadyRecordedInRust) {
          fire("apply_tile_patch", {
            docId,
            layerId: imperative.layerId,
            before: imperative.before,
            after: imperative.after,
          });
        } else if (!alreadyRecordedInRust) {
          fire("rust_pixels_record_external", {
            docId,
            label: label ?? "ts-meta",
            affected: snapshot.activeLayerId ? [snapshot.activeLayerId] : [],
            adapterId: "ts",
            token: label ?? "ts-meta",
          });
        }
      } catch {
        /* bridge is best-effort; a failure must never break TS history */
      }
    }

    this.undoStack.push({
      snapshot,
      timestamp: Date.now(),
      lastPaintCoords: this.currentLastPaintCoords,
      label,
      imperative,
    });

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

  canUndo(): boolean {
    return this.undoStack.length > 0;
  }

  canRedo(): boolean {
    return this.redoStack.length > 0;
  }

  private lastUndoPatches?: HistoryTilePatches;
  private lastRedoPatches?: HistoryTilePatches;

  undo(currentSnapshot: DocumentModel): DocumentModel | null {
    if (!this.canUndo()) {
      return null;
    }

    const previousEntry = this.undoStack.pop()!;

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
    });

    this.currentLastPaintCoords = previousEntry.lastPaintCoords;
    // Fase 1: patches to execute for THIS undo (pre-stroke tiles of the entry).
    this.lastUndoPatches = previousEntry.imperative;

    // NOTE: Rust cursor sync is NOT done here. It is done by useEditorCommands.ts
    // after determining whether the operation is a pixel undo (rust_pixels_undo)
    // or a metadata undo (no patches). Doing it here would cause DOUBLE UNDO
    // when photrez.rustPixels=1.

    return previousEntry.snapshot;
  }

  /** Fase 1: tile patches to execute for the just-performed undo (consume-once). */
  consumeLastUndoPatches(): HistoryTilePatches | undefined {
    const p = this.lastUndoPatches;
    this.lastUndoPatches = undefined;
    return p;
  }

  redo(currentSnapshot: DocumentModel): DocumentModel | null {
    if (!this.canRedo()) {
      return null;
    }

    const nextEntry = this.redoStack.pop()!;

    // Save current to undo stack — the entry's own imperative travels with it
    // (see undo(): entry-owned patches, no live-state reads).
    this.undoStack.push({
      snapshot: currentSnapshot,
      timestamp: Date.now(),
      lastPaintCoords: this.currentLastPaintCoords,
      label: nextEntry.label,
      imperative: nextEntry.imperative,
    });

    this.currentLastPaintCoords = nextEntry.lastPaintCoords;
    // Fase 1: patches to execute for THIS redo (post-stroke tiles of the entry).
    this.lastRedoPatches = nextEntry.imperative;

    // NOTE: Rust cursor sync is NOT done here. See undo() comment.

    return nextEntry.snapshot;
  }

  /** Fase 1: tile patches to execute for the just-performed redo (consume-once). */
  consumeLastRedoPatches(): HistoryTilePatches | undefined {
    const p = this.lastRedoPatches;
    this.lastRedoPatches = undefined;
    return p;
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
  }

  getUndoCount(): number {
    return this.undoStack.length;
  }

  getRedoCount(): number {
    return this.redoStack.length;
  }
}
