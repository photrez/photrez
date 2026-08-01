import type { DocumentModel, LayerNode } from "./types";
import { MAX_HISTORY_DEPTH } from "./types";

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

  constructor(maxDepth: number = MAX_HISTORY_DEPTH) {
    this.maxDepth = maxDepth;
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

  commit(snapshot: DocumentModel, label?: string): void {
    this.undoStack.push({
      snapshot,
      timestamp: Date.now(),
      lastPaintCoords: this.currentLastPaintCoords,
      label,
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

  undo(currentSnapshot: DocumentModel): DocumentModel | null {
    if (!this.canUndo()) {
      return null;
    }

    const previousEntry = this.undoStack.pop()!;

    // Save current to redo stack
    this.redoStack.push({
      snapshot: currentSnapshot,
      timestamp: Date.now(),
      lastPaintCoords: this.currentLastPaintCoords,
      label: previousEntry.label,
    });

    this.currentLastPaintCoords = previousEntry.lastPaintCoords;

    return previousEntry.snapshot;
  }

  redo(currentSnapshot: DocumentModel): DocumentModel | null {
    if (!this.canRedo()) {
      return null;
    }

    const nextEntry = this.redoStack.pop()!;

    // Save current to undo stack
    this.undoStack.push({
      snapshot: currentSnapshot,
      timestamp: Date.now(),
      lastPaintCoords: this.currentLastPaintCoords,
      label: nextEntry.label,
    });

    this.currentLastPaintCoords = nextEntry.lastPaintCoords;

    return nextEntry.snapshot;
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
