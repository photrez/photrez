// apps/desktop/src/components/editor/saveWorkerPool.ts
//
// Web Worker pool for parallel PNG layer encoding.
// Falls back to main-thread serial encoding when Worker is unavailable
// (test environments, older runtimes).
//
// Singleton access: getSaveWorkerPool() / destroySaveWorkerPool()

export interface EncodeTask {
  layerId: string;
  imageBitmap: ImageBitmap;
  width: number;
  height: number;
}

export type EncodeProgressCallback = (completed: number, total: number) => void;

/** Called as soon as a single layer finishes encoding — enables streaming write-to-disk. */
export type LayerResultCallback = (layerId: string, pngBytes: Uint8Array) => void;

export class SaveWorkerPool {
  private workers: Worker[] = [];
  private disposed = false;
  private busy = false;

  constructor(
    private maxWorkers: number = navigator.hardwareConcurrency ?? 4,
  ) {}

  /**
   * Encode all given layer bitmaps to PNG bytes.
   * Returns a map of layerId → Uint8Array in the same order as tasks (but
   * results may arrive in any order). AbortSignal can cancel the operation.
   * onProgress is called after each completed task with (completed, total).
   *
   * Throws if another encodeLayers call is already in progress — prevents
   * the silent hang that occurs when two calls share the same Worker pool
   * and an abort in one terminates all workers, leaving the other orphaned.
   */
  async encodeLayers(
    tasks: EncodeTask[],
    signal: AbortSignal,
    onProgress?: EncodeProgressCallback,
    onLayerResult?: LayerResultCallback,
  ): Promise<Record<string, Uint8Array>> {
    if (this.disposed) {
      throw new Error("SaveWorkerPool has been terminated");
    }
    if (this.busy) {
      throw new Error("SaveWorkerPool is already encoding — concurrent saves are not supported. " +
        "Use the save queue in useEditorCommands to serialise save operations.");
    }
    if (tasks.length === 0) return {};
    if (signal.aborted) {
      throw new DOMException("The operation was aborted", "AbortError");
    }

    this.busy = true;
    try {
      // Fallback: if Workers are not available, encode serially on the main thread.
      const canUseWorkers = typeof Worker !== "undefined";
      if (!canUseWorkers) {
        return await this.encodeMainThread(tasks, signal, onProgress, onLayerResult);
      }

      return await this.encodeOnWorkers(tasks, signal, onProgress, onLayerResult);
    } finally {
      this.busy = false;
    }
  }

  /** Terminate all workers and prevent further use. */
  terminate(): void {
    this.disposed = true;
    for (const w of this.workers) {
      w.terminate();
    }
    this.workers = [];
  }

  // ── Private ──

  private async encodeMainThread(
    tasks: EncodeTask[],
    signal: AbortSignal,
    onProgress?: EncodeProgressCallback,
    onLayerResult?: LayerResultCallback,
  ): Promise<Record<string, Uint8Array>> {
    const results: Record<string, Uint8Array> = {};
    for (let i = 0; i < tasks.length; i++) {
      if (signal.aborted) {
        throw new DOMException("The operation was aborted", "AbortError");
      }
      const task = tasks[i];
      try {
        const canvas = new OffscreenCanvas(task.width, task.height);
        const ctx = canvas.getContext("2d");
        if (!ctx) {
          console.warn("[SaveWorkerPool] OffscreenCanvas context null, skipping layer", task.layerId);
          continue;
        }
        ctx.drawImage(task.imageBitmap, 0, 0);
        const blob = await canvas.convertToBlob({ type: "image/png" });
        results[task.layerId] = new Uint8Array(await blob.arrayBuffer());
        onLayerResult?.(task.layerId, results[task.layerId]);
      } catch (err) {
        throw new Error(`Layer ${task.layerId} encode error: ${err instanceof Error ? err.message : String(err)}`);
      }
      onProgress?.(i + 1, tasks.length);
    }
    return results;
  }

  private async encodeOnWorkers(
    tasks: EncodeTask[],
    signal: AbortSignal,
    onProgress?: EncodeProgressCallback,
    onLayerResult?: LayerResultCallback,
  ): Promise<Record<string, Uint8Array>> {
    const count = Math.min(tasks.length, this.maxWorkers);

    // Lazily create workers.
    while (this.workers.length < count) {
      try {
        const worker = new Worker(
          new URL("./saveWorker.ts", import.meta.url),
          { type: "module" },
        );
        this.workers.push(worker);
      } catch (err) {
        console.warn("[SaveWorkerPool] Failed to create Worker, falling back to main-thread encode:", err);
        return this.encodeMainThread(tasks, signal, onProgress, onLayerResult);
      }
    }

    return new Promise<Record<string, Uint8Array>>((resolve, reject) => {
      const results: Record<string, Uint8Array> = {};
      let nextIdx = 0;
      let completed = 0;
      let settled = false;

      const onAbort = () => {
        settled = true;
        cleanup();
        // Reset workers instead of terminating them: each worker drops its
        // stale result via a reset token (see saveWorker.ts), so the pool
        // stays warm for the next encodeLayers call. This matters for the
        // autosave-aborted-by-manual-save path — the manual save reuses the
        // same workers immediately instead of paying terminate + re-create.
        for (const w of this.workers) {
          w.postMessage({ type: "reset" });
        }
        reject(new DOMException("The operation was aborted", "AbortError"));
      };
      signal.addEventListener("abort", onAbort, { once: true });

      const cleanup = () => {
        signal.removeEventListener("abort", onAbort);
      };

      /** Dispatch the next available task to a specific worker. */
      function dispatchNext(worker: Worker): void {
        const idx = nextIdx++;
        if (idx >= tasks.length) return; // no more tasks

        const task = tasks[idx];
        // Create a GPU-side copy so the engine's original ImageBitmap (and any
        // undo-history snapshot that references it) stays valid on the main
        // thread — transfer neuters the copied bitmap only.  Structured clone
        // (the old approach) caused GPU→CPU readback that blocked main thread
        // 50-200ms per 4K layer.  GPU copy + transfer = ~1ms, zero main-thread
        // block, and the engine/history originals are untouched.
        let sendBitmap: ImageBitmap;
        let canTransfer = false;
        try {
          const copyCanvas = new OffscreenCanvas(task.width, task.height);
          const copyCtx = copyCanvas.getContext("2d")!;
          copyCtx.drawImage(task.imageBitmap, 0, 0);
          sendBitmap = copyCanvas.transferToImageBitmap();
          canTransfer = typeof ImageBitmap !== "undefined" && sendBitmap instanceof ImageBitmap;
        } catch {
          // OffscreenCanvas unavailable — fall back to structured clone.
          sendBitmap = task.imageBitmap;
          canTransfer = false;
        }
        const transfer: Transferable[] = canTransfer ? [sendBitmap] : [];
        worker.postMessage({
          type: "encode",
          layerId: task.layerId,
          width: task.width,
          height: task.height,
          imageBitmap: sendBitmap,
        }, transfer);
      }

      /** Handler for Worker message events. */
      function onWorkerMessage(this: Worker, e: MessageEvent): void {
        if (settled) return;
        const msg = e.data;

        if (msg.type === "error") {
          cleanup();
          settled = true;
          reject(new Error(`Layer ${msg.layerId} encode error: ${msg.error}`));
          return;
        }

        if (msg.type === "result") {
          results[msg.layerId] = msg.pngBytes;
          onLayerResult?.(msg.layerId, msg.pngBytes);
        }

        completed++;
        onProgress?.(completed, tasks.length);

        if (completed >= tasks.length) {
          cleanup();
          settled = true;
          resolve(results);
          return;
        }

        // This worker is free — dispatch the next task if any remain.
        dispatchNext(this);
      }

      // Initial dispatch: send one task per worker.
      const used = Math.min(count, tasks.length);
      for (let i = 0; i < used; i++) {
        const worker = this.workers[i];
        worker.onmessage = onWorkerMessage;
        worker.onerror = (err) => {
          if (settled) return;
          cleanup();
          settled = true;
          reject(new Error(`Worker error: ${err.message}`));
        };
        dispatchNext(worker);
      }
    });
  }
}

// ── Singleton ──

let globalPool: SaveWorkerPool | null = null;

/** Get or create the global SaveWorkerPool singleton. */
export function getSaveWorkerPool(): SaveWorkerPool {
  if (!globalPool) {
    globalPool = new SaveWorkerPool();
  }
  return globalPool;
}

/** Terminate the global singleton and release workers. */
export function destroySaveWorkerPool(): void {
  if (globalPool) {
    globalPool.terminate();
    globalPool = null;
  }
}

/** Reset the singleton (for test teardown). */
export function resetSaveWorkerPool(): void {
  destroySaveWorkerPool();
}
