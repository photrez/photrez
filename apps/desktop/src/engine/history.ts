import type { DocumentModel, LayerNode } from "./types";
import { MAX_HISTORY_DEPTH } from "./types";
import type { TileUploadLike } from "../renderer/types";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import {
  bitmapStoreFor,
  existingTokenForBitmap,
  tokenForBitmap,
  type BitmapField,
} from "./bitmapStore";

/**
 * Route every TS commit into the SAME Rust `ProtocolEngine` cursor so TS and Rust
 * operations share ONE logical history position.
 *
 * Gated: the bridge is OFF by default in production. It is enabled only when the
 * runtime DEV gate `localStorage["photrez.historyBridge"] === "1"` is set AND the
 * app is running in the Tauri runtime. In default production the TS
 * `CommandHistory` remains the sole undo/redo authority (no Rust cursor append,
 * so the two histories never diverge).
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

// ── Snapshot-token payloads (mirror the rust core DocumentSnapshot/LayerSnapshot
//    DTOs so the Tauri commands round-trip 1:1, camelCase on the wire) ──

/** One layer's metadata + opaque bitmap token in a document snapshot. */
export interface SnapshotLayerMeta {
  layerId: string;
  width: number;
  height: number;
  bitmapToken: string | null;
  epoch: number;
  pixelVersion: number;
}

/** A document's atomic metadata + bitmap-token snapshot (no pixels). */
export interface SnapshotPayload {
  docId: string;
  version: number;
  layers: SnapshotLayerMeta[];
}

/**
 * The restored model's per-layer identity facts the re-attach verifies against
 * BEFORE applying any bitmap. Supplied (bridge-ON, snapshot-typed) by the
 * undo/redo caller from the snapshot being restored. `epoch`/`pixelVersion`
 * mirror what `buildSnapshotPayload` recorded at record time; the bitmaps are
 * the exact objects the restored model currently holds, so a payload whose
 * token resolves to a DIFFERENT object is provably not this step's snapshot.
 */
export interface SnapshotLayerIdentity {
  /** Expected epoch for this layer (the restored model's bitmapEpoch ?? 0). */
  epoch?: number;
  /** Expected pixelVersion for this layer (falls back to epoch when absent). */
  pixelVersion?: number;
  /** The restored model's current primary imageBitmap for this layer. */
  imageBitmap?: ImageBitmap | null;
  /** The restored model's current baseImageBitmap for this layer. */
  baseImageBitmap?: ImageBitmap | null;
}

/**
 * Re-attach the layer ImageBitmap(s) for a snapshot undo/redo step by token,
 * WITHOUT ever detaching a still-valid bitmap. This is the UX-critical
 * re-attach: it is called (bridge ON) after `engine.restore(snapshot)`, and it
 * asks the Rust snapshot cursor for the `before` (undo) or `after` (redo)
 * snapshot, then resolves each layer's `bitmapToken` from the BitmapStore and
 * re-assigns the SAME ImageBitmap object to the live layer.
 *
 * No-detach contract:
 *   - a token that resolves to a bitmap -> the EXACT same object is re-attached;
 *   - a token that resolves to null -> the existing bitmap is left untouched and
 *     the miss is surfaced (console.warn), never a silent detach;
 *   - the Rust command returning None (Ok) or rejecting (Err) -> the bridge did
 *     not own this entry (tip is not a Snapshot), so this returns false and the
 *     caller proceeds with the existing Model-A restore.
 *
 * Returns true when this helper re-attached at least one bitmap (caller may then
 * skip redundant work, though re-uploading is harmless).
 *
 * Identity gate: the payload is verified against this doc's restored model
 * before any bitmap is applied. If the returned snapshot's `docId` differs, or
 * (when `currentLayerIds` is supplied) its layer-id set no longer matches the
 * restored model, the re-attach is SKIPPED (returns false) and the Model-A
 * restore stays authoritative — never a wrong-step bitmap.
 *
 * Finding-B hardening (per-layer epoch/pixelVersion/token identity): a future
 * Snapshot producer that MUTATES a surviving layer's bitmap (e.g. an adjustment
 * bake) can emit step snapshots whose layer-id sets coincide with a different
 * step's payload. `currentLayerIds` alone cannot then tell them apart. When
 * `currentLayerMeta` is supplied, each payload layer's `epoch`/`pixelVersion`
 * MUST match the restored model's expected values, AND each token must resolve
 * to the EXACT bitmap object the restored model currently holds for that layer's
 * field. A payload that fails either check is SKIPPED (returns false + warn),
 * so a coincidentally-matching layer set can never silently attach the wrong
 * bitmap. The contiguous Delete Layer case is unaffected: its surviving layers
 * keep stable, consistent epochs and SAME bitmap objects across steps.
 */
export async function restoreSnapshotBitmapsByToken(
  direction: "undo" | "redo",
  docId: string,
  resolve: (token: string) => ImageBitmap | null,
  set: (layerId: string, bitmap: ImageBitmap, field?: BitmapField) => boolean,
  fieldFor?: (token: string) => BitmapField | null,
  currentLayerIds?: () => Iterable<string>,
  currentLayerMeta?: (layerId: string) => SnapshotLayerIdentity | null,
): Promise<boolean> {
  if (!historyBridgeEnabled()) return false;
  let snapshot: SnapshotPayload | null = null;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    snapshot = (await invoke<SnapshotPayload | null>(
      direction === "undo" ? "rust_pixels_undo_snapshot" : "rust_pixels_redo_snapshot",
      { docId },
    )) as SnapshotPayload | null;
  } catch {
    // Tauri v2 invoke REJECTS with an error-envelope object on a Rust Err; the
    // snapshot cursor is best-effort — a failure must never break undo/redo.
    return false;
  }
  if (!snapshot || !snapshot.layers || snapshot.layers.length === 0) return false;
  // Finding 2 (identity assertion): the Rust snapshot cursor can, under cursor
  // drift (or a mixed stream), return a DIFFERENT Snapshot's payload for this
  // doc. Re-attaching that payload would overwrite the Model-A restore with the
  // wrong-step bitmap. Before applying, verify the payload is actually for THIS
  // doc's restored model: the docId must match, and (when the live layer-id set
  // is supplied) the payload's layer-id set must match the restored model's.
  // On any mismatch we SKIP the re-attach (return false) so the Model-A restore
  // stays authoritative — never fail-open into a wrong-step bitmap.
  if (snapshot.docId !== docId) {
    console.warn("[history] snapshot payload docId mismatch; skipping re-attach", {
      expected: docId,
      actual: snapshot.docId,
    });
    return false;
  }
  if (currentLayerIds) {
    const modelLayerIds = new Set(currentLayerIds());
    const snapLayerIds = new Set(snapshot.layers.map((l) => l.layerId));
    const sameLayerSet =
      modelLayerIds.size === snapLayerIds.size &&
      [...snapLayerIds].every((id) => modelLayerIds.has(id));
    if (!sameLayerSet) {
      console.warn("[history] snapshot layer-set mismatch; skipping re-attach", {
        docId,
        payload: [...snapLayerIds],
        model: [...modelLayerIds],
      });
      return false;
    }
  }
  // Finding-B per-layer identity gate. When `currentLayerMeta` is supplied, the
  // restored model's per-layer epoch/pixelVersion (and current bitmap objects)
  // are the ground truth for THIS step. A payload built from a DIFFERENT step of
  // a future bitmap-mutating producer can share the docId AND the layer-id set,
  // but its per-layer epoch/pixelVersion (and the bitmap its token resolves to)
  // will differ. Verify every payload layer before any set — fail-closed.
  let metaById: Map<string, SnapshotLayerIdentity> | null = null;
  if (currentLayerMeta) {
    metaById = new Map();
    for (const layer of snapshot.layers) {
      const meta = currentLayerMeta(layer.layerId);
      if (!meta) {
        console.warn("[history] snapshot layer absent from restored model; skipping re-attach", {
          docId,
          layerId: layer.layerId,
        });
        return false;
      }
      const expEpoch = meta.epoch ?? 0;
      const expPixel = meta.pixelVersion ?? expEpoch;
      if (layer.epoch !== expEpoch || layer.pixelVersion !== expPixel) {
        console.warn("[history] snapshot layer epoch/pixelVersion mismatch; skipping re-attach", {
          docId,
          layerId: layer.layerId,
          payloadEpoch: layer.epoch,
          payloadPixelVersion: layer.pixelVersion,
          expectedEpoch: expEpoch,
          expectedPixelVersion: expPixel,
        });
        return false;
      }
      metaById.set(layer.layerId, meta);
    }
  }
  let applied = false;
  for (const layer of snapshot.layers) {
    if (!layer.bitmapToken) continue;
    const bitmap = resolve(layer.bitmapToken);
    if (bitmap) {
      // Hazard #3: re-attach to the SAME layer field the token was registered
      // from (imageBitmap vs baseImageBitmap). When no fieldFor resolver is
      // supplied (the isolated-helper unit call) the set callback receives the
      // legacy 2-arg form, so existing consumers/tests are unaffected.
      const field = layer.bitmapToken && fieldFor ? fieldFor(layer.bitmapToken) : null;
      // Finding-B token→bitmap identity: the token must resolve to the EXACT
      // bitmap the restored model currently holds for this layer's field. A
      // coincidental layer-set + epoch match cannot make a wrong-step payload
      // resolve to the model's own bitmap object unless it truly is the model's
      // bitmap — so a mismatch (including the model holding null while the
      // payload claims a bitmap) is provably the wrong step. Skip fail-closed.
      if (metaById) {
        const meta = metaById.get(layer.layerId);
        const modelBitmap = meta
          ? field === "baseImageBitmap"
            ? meta.baseImageBitmap ?? null
            : meta.imageBitmap ?? null
          : null;
        if (bitmap !== modelBitmap) {
          console.warn("[history] snapshot token resolves to a different bitmap than restored model; skipping re-attach", {
            docId,
            layerId: layer.layerId,
            field: field ?? "imageBitmap",
          });
          return false;
        }
      }
      applied = field ? set(layer.layerId, bitmap, field) || applied : set(layer.layerId, bitmap) || applied;
    } else {
      // Token present but unresolvable: never silently detach the existing one.
      console.warn(
        "[history] snapshot bitmap token unresolved; keeping existing bitmap",
        { docId, layerId: layer.layerId, token: layer.bitmapToken },
      );
    }
  }
  return applied;
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
  ): void {
    // Append this commit to the unified Rust history cursor.
    //  - imperative TS pixel op NOT yet in Rust (text/gradient/shape/transform)
    //    -> `apply_tile_patch` (Pixel entry, same command Rust strokes use).
    //  - non-pixel TS (metadata) op -> `rust_pixels_record_external` (External entry).
    //  - `alreadyRecordedInRust` (brush/fill/adjustment bake) -> Rust already owns
    //    the Pixel entry via `rust_pixels_write_region`; skip to avoid double-count.
    //  - snapshot-type ops (before+after states) use `recordSnapshotHistory`, NOT
    //    this method — see its comment for the contract-correct before/after.
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
  recordSnapshotHistory(before: DocumentModel, after: DocumentModel, label?: string): void {
    if (historyBridgeEnabled() && this.docIdGetter) {
      const docId = this.docIdGetter();
      // Registrations happen inside buildSnapshotPayload (idempotent per bitmap
      // object). before = the PRE-action state, after = the POST-action state.
      const beforePayload = this.buildSnapshotPayload(before);
      const afterPayload = this.buildSnapshotPayload(after);
      // Dynamic import (matching document.ts) keeps the Tauri API out of the
      // module graph at import time so browser/test imports stay side-effect free.
      const fire = (cmd: string, args: Record<string, unknown>) =>
        import("@tauri-apps/api/core")
          .then(({ invoke }) => invoke(cmd, args))
          .catch(() => {});
      try {
        fire("rust_pixels_record_snapshot", { docId, before: beforePayload, after: afterPayload });
      } catch {
        /* bridge is best-effort; a failure must never break TS history */
      }
    }

    // TS undo/redo authority: the undo-point is the PRE-action state (unchanged
    // convention) — undo restores `before`, which is what Rust undo_snapshot
    // returns for this step.
    this.undoStack.push({
      snapshot: before,
      timestamp: Date.now(),
      lastPaintCoords: this.currentLastPaintCoords,
      label,
      snapshotType: "snapshot",
    });

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

  undo(currentSnapshot: DocumentModel): DocumentModel | null {
    if (!this.canUndo()) {
      return null;
    }

    const previousEntry = this.undoStack.pop()!;
    this.lastPoppedIsSnapshot = previousEntry.snapshotType === "snapshot";

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
    });
    this.currentLastPaintCoords = previousEntry.lastPaintCoords;
    // Tile patches to execute for THIS undo (pre-stroke tiles of the entry).
    this.lastUndoPatches = previousEntry.imperative;

    // NOTE: Rust cursor sync is NOT done here. It is done by useEditorCommands.ts
    // after determining whether the operation is a pixel undo (rust_pixels_undo)
    // or a metadata undo (no patches). Doing it here would cause DOUBLE UNDO
    // when photrez.rustPixels=1.

    return previousEntry.snapshot;
  }

  /** Tile patches to execute for the just-performed undo (consume-once). */
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
    this.lastPoppedIsSnapshot = nextEntry.snapshotType === "snapshot";

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
    });

    this.currentLastPaintCoords = nextEntry.lastPaintCoords;
    // Tile patches to execute for THIS redo (post-stroke tiles of the entry).
    this.lastRedoPatches = nextEntry.imperative;

    // NOTE: Rust cursor sync is NOT done here. See undo() comment.

    return nextEntry.snapshot;
  }

  /** Tile patches to execute for the just-performed redo (consume-once). */
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

  getUndoCount(): number {
    return this.undoStack.length;
  }

  getRedoCount(): number {
    return this.redoStack.length;
  }
}
