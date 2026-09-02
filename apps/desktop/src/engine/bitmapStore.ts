/**
 * Bitmap-token resource store for the snapshot-history bridge.
 *
 * A token → ImageBitmap registry, keyed per document. Its purpose is the
 * anti-detach invariant: an undo/redo step that restores a snapshot re-attaches
 * the SAME ImageBitmap object by token, so a live layer can never end up with a
 * detached ("image source is detached") or swapped bitmap.
 *
 * The store is NOT a second close authority by itself — the existing
 * `CommandHistory.disposeSnapshot` path remains the close owner. `release()`
 * closes exactly once and is idempotent (double-close is swallowed); a
 * token-registered bitmap that is still referenced by the live model or another
 * snapshot is never released.
 *
 * All of it is dormant by default: tokens are only registered/released when the
 * snapshot history bridge (`photrez.historyBridge === "1"` AND Tauri runtime) is
 * ON. With the bridge OFF nothing registers a token, so this module is inert and
 * default production is byte-identical.
 */

/** Which layer field a registered token's bitmap was captured from. */
export type BitmapField = "imageBitmap" | "baseImageBitmap";

export class BitmapStore {
  private tokens = new Map<string, ImageBitmap>();
  private fields = new Map<string, BitmapField>();

  /**
   * Register (or overwrite) the bitmap a token resolves to, remembering which
   * layer field it came from. Snapshot re-attach uses the field so a base-only
   * layer (imageBitmap=null, baseImageBitmap set) is re-attached to the SAME
   * field — never wrongly gaining an `imageBitmap` it did not have pre-action.
   */
  set(token: string, bitmap: ImageBitmap, field?: BitmapField): void {
    this.tokens.set(token, bitmap);
    if (field) this.fields.set(token, field);
  }

  /** Resolve a token to its bitmap, or null if it is not registered. */
  get(token: string): ImageBitmap | null {
    return this.tokens.get(token) ?? null;
  }

  /** The layer field a token's bitmap was registered from, or null if unknown. */
  getField(token: string): BitmapField | null {
    return this.fields.get(token) ?? null;
  }

  has(token: string): boolean {
    return this.tokens.has(token);
  }

  /**
   * Close the bitmap a token references exactly once and forget the mapping.
   * Idempotent: closing an already-closed (or already-released) bitmap is
   * swallowed, so a stray double-release can never crash.
   */
  release(token: string): void {
    const bitmap = this.tokens.get(token);
    if (!bitmap) return;
    this.tokens.delete(token);
    this.fields.delete(token);
    try {
      bitmap.close();
    } catch {
      // already closed — release is safe/idempotent by design
    }
  }

  /**
   * Drop the token mapping WITHOUT closing. Used when the bitmap was already
   * closed by the `disposeSnapshot` path (which then owns the close), so we keep
   * the token table tidy without a second (double) close.
   */
  drop(token: string): void {
    this.tokens.delete(token);
    this.fields.delete(token);
  }
}

// Per-document stores so concurrent documents never share a token namespace.
const stores = new Map<string, BitmapStore>();

/** Resolve (creating on first use) the BitmapStore for a document. */
export function bitmapStoreFor(docId: string): BitmapStore {
  let store = stores.get(docId);
  if (!store) {
    store = new BitmapStore();
    stores.set(docId, store);
  }
  return store;
}

/**
 * Drop a document's store when the document closes. Deliberately does NOT close
 * the bitmaps (closing is owned by the history disposeSnapshot path / GC), so a
 * bitmap still referenced elsewhere (e.g. clipboard) is never force-closed.
 */
export function releaseBitmapStore(docId: string): void {
  stores.delete(docId);
}

// Stable token per ImageBitmap OBJECT. The same object always maps to the same
// token, so a bitmap shared between a "before" and an "after" snapshot (or reused
// across commits) gets one stable identity — that is what lets re-attach always
// resolve to the exact same bitmap, never a different one.
const tokenByBitmap = new WeakMap<object, string>();
let tokenCounter = 0;

/** Stable token for a bitmap; creates and persists it on first encounter. */
export function tokenForBitmap(bitmap: ImageBitmap): string {
  let token = tokenByBitmap.get(bitmap);
  if (!token) {
    token = `bm-${++tokenCounter}`;
    tokenByBitmap.set(bitmap, token);
  }
  return token;
}

/** Read-only: the token a bitmap was registered under, or null if unregistered. */
export function existingTokenForBitmap(bitmap: ImageBitmap): string | null {
  return tokenByBitmap.get(bitmap) ?? null;
}
