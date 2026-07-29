import { DocumentEngine } from "@/engine/document";
import { saveProjectStreamingBegin, saveProjectStreamingWriteLayer, saveProjectStreamingEnd, saveProjectStreamingCancel } from "@/tauri/native";
import { getSaveWorkerPool, resetSaveWorkerPool } from "./saveWorkerPool";
import type { EncodeTask, EncodeProgressCallback } from "./saveWorkerPool";

// ── Per-document dirty layer cache ──
// Map<DocId, Map<LayerId, Uint8Array>>
// Caches encoded PNG bytes so clean layers skip re-encode on consecutive saves.
const dirtyLayerCache = new Map<string, Map<string, Uint8Array>>();

/** Clear the layer-encode cache for a specific document, or all documents. */
export function clearLayerCache(docId?: string): void {
  if (docId) {
    dirtyLayerCache.delete(docId);
  } else {
    dirtyLayerCache.clear();
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

  // Ensure per-document cache entry exists.
  let cache = dirtyLayerCache.get(docId);
  if (!cache) {
    cache = new Map();
    dirtyLayerCache.set(docId, cache);
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

      if (!isDirty && cache.has(layer.id) && layer.imageBitmap) {
        // Clean layer: write from cache immediately (parallel).
        cleanLayerWrites.push(writeLayer(layer.id, cache.get(layer.id)!));
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
          cache!.set(layerId, pngBytes);
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
    for (const [id] of cache) {
      if (!currentIds.has(id)) {
        cache.delete(id);
      }
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
