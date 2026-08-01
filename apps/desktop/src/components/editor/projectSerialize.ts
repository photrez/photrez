import { DocumentEngine } from "@/engine/document";
import { saveProjectStreamingBegin, saveProjectStreamingWriteLayer, saveProjectStreamingEnd, saveProjectStreamingCancel } from "@/tauri/native";
import { getSaveWorkerPool, resetSaveWorkerPool } from "./saveWorkerPool";
import type { EncodeTask, EncodeProgressCallback } from "./saveWorkerPool";

// ── Per-document dirty layer cache ──
// Map<DocId, Map<LayerId, Uint8Array>> with a global byte budget. Layers stay
// cached so clean layers skip re-encode on consecutive saves. The budget caps
// memory (a large 4K layer PNG can be tens of MB; many dirty docs would
// otherwise balloon RAM) by evicting the least-recently-used entries.
const dirtyLayerCache = new Map<string, Map<string, Uint8Array>>();
let cacheByteBudget = 256 * 1024 * 1024; // 256 MiB
let cacheBytes = 0;

/** Insert or refresh a cached layer. Moves the entry to most-recent position. */
function cacheSet(docId: string, layerId: string, bytes: Uint8Array): void {
  let cache = dirtyLayerCache.get(docId);
  if (!cache) {
    cache = new Map();
    dirtyLayerCache.set(docId, cache);
  }
  const prev = cache.get(layerId);
  cacheBytes += bytes.byteLength - (prev?.byteLength ?? 0);
  cache.delete(layerId); // re-insert so the entry counts as most-recent
  cache.set(layerId, bytes);
  evictIfOverBudget();
}

/** Read a cached layer, moving it to most-recent position on a hit. */
function cacheRead(docId: string, layerId: string): Uint8Array | undefined {
  const cache = dirtyLayerCache.get(docId);
  if (!cache) return undefined;
  const bytes = cache.get(layerId);
  if (bytes === undefined) return undefined;
  cache.delete(layerId);
  cache.set(layerId, bytes);
  return bytes;
}

function evictIfOverBudget(): void {
  while (cacheBytes > cacheByteBudget) {
    // Map iteration order = insertion order, so the first doc/layer is the
    // least-recently-used across the whole cache. Re-reading after cacheSet
    // keeps the scan O(docs) per eviction, not O(entries).
    let evictDoc: string | null = null;
    let evictLayer: string | null = null;
    for (const [docId, cache] of dirtyLayerCache) {
      const first = cache.keys().next();
      if (!first.done) {
        evictDoc = docId;
        evictLayer = first.value;
        break;
      }
    }
    if (evictDoc === null || evictLayer === null) break;
    const cache = dirtyLayerCache.get(evictDoc)!;
    const bytes = cache.get(evictLayer);
    if (bytes) cacheBytes -= bytes.byteLength;
    cache.delete(evictLayer);
    if (cache.size === 0) dirtyLayerCache.delete(evictDoc);
  }
}

/** Clear the layer-encode cache for a specific document, or all documents. */
export function clearLayerCache(docId?: string): void {
  if (docId) {
    const cache = dirtyLayerCache.get(docId);
    if (cache) {
      for (const bytes of cache.values()) cacheBytes -= bytes.byteLength;
      dirtyLayerCache.delete(docId);
    }
  } else {
    dirtyLayerCache.clear();
    cacheBytes = 0;
  }
}

export interface SerializeOptions {
  /** AbortSignal for cancelling an in-flight save. */
  signal?: AbortSignal;
  /** Called after each layer completes encoding. */
  onEncodeProgress?: EncodeProgressCallback;
}

/**
 * Serialize a document model and all layer bitmaps to the .ptz format,
 * then write the result to disk via Tauri IPC.
 *
 * Encoding uses the shared SaveWorkerPool for parallel PNG encode across
 * multiple Web Workers.  Clean layers (not dirty since last save) are served
 * from an in-memory cache and skip re-encode entirely.
 */
export async function serializeAndSaveProject(
  engine: DocumentEngine,
  path: string,
  options?: SerializeOptions,
): Promise<void> {
  const model = engine.snapshot();
  const docId = engine.getId();
  const dirtyIds = new Set(engine.getDirtyLayerIds());
  const externalSignal = options?.signal;

  // Safety: if the engine is dirty but no layer IDs are tracked
  // (e.g. edits happened during an async save window and clearDirty already
  // cleared the dirty-Id set), fall back to re-encoding all layers with bitmaps
  // rather than serving stale cache data.
  if (dirtyIds.size === 0 && engine.isDirty()) {
    for (const layer of model.layers) {
      if (layer.imageBitmap) dirtyIds.add(layer.id);
    }
  }

  // ── Serialize model JSON (before any IPC, fast local work) ──
  const serializedModel = {
    ...model,
    format: "photrez-ptz",
    version: 1,
    layers: model.layers.map((l) => ({
      ...l,
      imageBitmap: null,
    })),
  };
  const documentJson = JSON.stringify(serializedModel);

  // ── Begin streaming save (Rust: create temp file, write document.json) ──
  // Must succeed before any layer writes — a failure here means the file path
  // is invalid, disk full, or permissions error. AbortSignal checked inside
  // saveProjectStreamingBegin via Tauri's built-in cancellation.
  const handleId = await saveProjectStreamingBegin(path, documentJson);

  // ── Internal abort controller: stop encoding on write failure ──
  // `effectiveSignal` is always `abortController.signal` so BOTH an external
  // cancel AND an internal write-failure set the SAME signal.  Before this fix
  // `effectiveSignal = externalSignal ?? abortController.signal` meant an
  // internal abort was invisible to the pool when externalSignal was set —
  // encoding continued for layers that would never be persisted.
  const abortController = new AbortController();
  if (externalSignal) {
    // Guard: if externalSignal is already aborted the 'abort' event never fires,
    // so we must abort the controller immediately rather than waiting for a
    // listener that will never be called. (WHATWG / Node.js recommended pattern.)
    if (externalSignal.aborted) {
      abortController.abort();
    } else {
      externalSignal.addEventListener("abort", () => abortController.abort(), { once: true });
    }
  }
  const effectiveSignal = abortController.signal;

  // Track in-flight IPC writes so we can confirm all finished before `end`.
  const pendingWrites: Promise<void>[] = [];

  // ── Helper: write one layer to Rust, handle error ──
  const writeLayer = async (id: string, bytes: Uint8Array): Promise<void> => {
    if (effectiveSignal.aborted) {
      throw new DOMException("Aborted", "AbortError");
    }
    await saveProjectStreamingWriteLayer(handleId, id, bytes);
  };

  try {
    // ── Separate clean (cache) from dirty (needs encode) ──
    const encodeTasks: EncodeTask[] = [];
    const cleanLayerWrites: Promise<void>[] = [];

    for (const layer of model.layers) {
      const isDirty = dirtyIds.has(layer.id);
      const cached = cacheRead(docId, layer.id);

      if (!isDirty && cached && layer.imageBitmap) {
        // Clean layer: write from cache immediately (parallel).
        cleanLayerWrites.push(writeLayer(layer.id, cached));
        continue;
      }

      if (layer.imageBitmap) {
        encodeTasks.push({
          layerId: layer.id,
          imageBitmap: layer.imageBitmap,
          width: layer.width,
          height: layer.height,
        });
      }
    }

    // ── Parallel: write clean layers + encode dirty layers ──
    // Clean writes fire immediately. Dirty encode streams each result to
    // onLayerResult which writes it to Rust and caches it.
    if (encodeTasks.length > 0) {
      const pool = getSaveWorkerPool();
      // Start clean writes AND dirty encoding simultaneously.
      await Promise.all([
        pool.encodeLayers(encodeTasks, effectiveSignal, options?.onEncodeProgress, (layerId, pngBytes) => {
          cacheSet(docId, layerId, pngBytes);
          const p = writeLayer(layerId, pngBytes).catch((err) => {
            // Stop encoding on write failure — abort triggers pool termination.
            abortController.abort();
            throw err;
          });
          pendingWrites.push(p);
        }),
        // Clean writes alongside encoding — no sequential blocking.
        ...cleanLayerWrites,
      ]);
    } else {
      // No dirty layers — still wait for clean writes.
      await Promise.all(cleanLayerWrites);
    }

    // ── Wait for all in-flight stream writes ──
    await Promise.all(pendingWrites);

    // ── Evict deleted layers from cache ──
    const currentIds = new Set(model.layers.map((l) => l.id));
    const docCache = dirtyLayerCache.get(docId);
    if (docCache) {
      for (const [id, bytes] of docCache) {
        if (!currentIds.has(id)) {
          cacheBytes -= bytes.byteLength;
          docCache.delete(id);
        }
      }
      if (docCache.size === 0) dirtyLayerCache.delete(docId);
    }

    // ── End: Rust finalizes ZIP, fsync, atomic rename ──
    await saveProjectStreamingEnd(handleId);
  } catch (err) {
    // Stop encoding if still in progress.
    abortController.abort();
    // Clean up Rust side: drop ZipWriter, delete temp file.
    try { await saveProjectStreamingCancel(handleId); } catch { /* ignore cleanup errors */ }
    throw err;
  }
}

// ═══════════════════════════════════════════════════════
// Test support
// ═══════════════════════════════════════════════════════

/**
 * Reset the shared SaveWorkerPool singleton.
 * Call from test `beforeEach` / `afterEach` to isolate test runs.
 */
export function resetTestWorkerPool(): void {
  resetSaveWorkerPool();
}

/** Override the LRU byte budget (test support). */
export function setLayerCacheBudget(bytes: number): void {
  cacheByteBudget = bytes;
}
