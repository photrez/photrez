// apps/desktop/src/components/editor/__tests__/saveWorkerPool.test.ts
//
// Contract tests for SaveWorkerPool.
//
// In jsdom (typeof Worker === "undefined") the pool falls back to main-thread
// serial encoding.  These tests verify the pool contract:
//   - encodeLayers returns correct results
//   - abort signal cancels in-flight encoding
//   - terminate() prevents reuse
//   - progress callbacks fire correctly
//   - edge cases (empty tasks, single task) work

import { describe, expect, it, vi, afterEach, beforeEach } from "vitest";

const PNG_BYTES = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d]);

/** Stub OffscreenCanvas so the pool's main-thread fallback can run in jsdom. */
function stubOffscreenCanvas(): void {
  vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    this.getContext = () => ({
      drawImage: vi.fn(),
      clearRect: vi.fn(),
    });
    this.convertToBlob = vi.fn().mockResolvedValue(new Blob([PNG_BYTES as BlobPart], { type: "image/png" }));
  }));
}

function makeBitmap(id: string): ImageBitmap {
  return { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
}

function makeTask(layerId: string): { layerId: string; imageBitmap: ImageBitmap; width: number; height: number } {
  return { layerId, imageBitmap: makeBitmap(layerId), width: 100, height: 100 };
}

describe("SaveWorkerPool", () => {
  beforeEach(() => {
    stubOffscreenCanvas();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  // Relies on the module singleton — reset for isolation.
  afterEach(async () => {
    const { resetSaveWorkerPool } = await import("../saveWorkerPool");
    resetSaveWorkerPool();
  });

  it("empty tasks returns empty map", async () => {
    const { SaveWorkerPool } = await import("../saveWorkerPool");
    const pool = new SaveWorkerPool();
    const signal = new AbortController().signal;
    const result = await pool.encodeLayers([], signal);
    expect(result).toEqual({});
  });

  it("encodes a single task and returns result", async () => {
    const { SaveWorkerPool } = await import("../saveWorkerPool");
    const pool = new SaveWorkerPool();
    const signal = new AbortController().signal;
    const task = makeTask("layer-1");
    const result = await pool.encodeLayers([task], signal);
    expect(result["layer-1"]).toBeDefined();
    expect(result["layer-1"]).toEqual(PNG_BYTES);
  });

  it("encodes multiple tasks and returns all results", async () => {
    const { SaveWorkerPool } = await import("../saveWorkerPool");
    const pool = new SaveWorkerPool(4);
    const signal = new AbortController().signal;

    const tasks = [
      makeTask("layer-a"),
      makeTask("layer-b"),
      makeTask("layer-c"),
    ];

    const result = await pool.encodeLayers(tasks, signal);
    expect(Object.keys(result)).toEqual(["layer-a", "layer-b", "layer-c"]);
    for (const key of ["layer-a", "layer-b", "layer-c"]) {
      expect(result[key]).toEqual(PNG_BYTES);
    }
  });

  it("aborted signal rejects with AbortError before encoding starts", async () => {
    const { SaveWorkerPool } = await import("../saveWorkerPool");
    const pool = new SaveWorkerPool();
    const ctrl = new AbortController();
    ctrl.abort();

    const task = makeTask("layer-1");
    await expect(pool.encodeLayers([task], ctrl.signal)).rejects.toThrow("aborted");
  });

  it("aborted signal during encoding rejects with AbortError", async () => {
    const { SaveWorkerPool } = await import("../saveWorkerPool");
    const pool = new SaveWorkerPool();
    const ctrl = new AbortController();

    const tasks = [makeTask("slow-1"), makeTask("slow-2"), makeTask("slow-3")];

    // Abort after a microtick so encoding begins first.
    const promise = pool.encodeLayers(tasks, ctrl.signal);
    ctrl.abort();

    await expect(promise).rejects.toThrow("aborted");
  });

  it("calls progress callback for each completed task", async () => {
    const { SaveWorkerPool } = await import("../saveWorkerPool");
    const pool = new SaveWorkerPool(4);
    const signal = new AbortController().signal;

    const tasks = [makeTask("a"), makeTask("b"), makeTask("c")];
    const progress = vi.fn();

    await pool.encodeLayers(tasks, signal, progress);

    // The fallback path calls onProgress after each serial encode.
    expect(progress).toHaveBeenCalledTimes(3);
    expect(progress).toHaveBeenNthCalledWith(1, 1, 3);
    expect(progress).toHaveBeenNthCalledWith(2, 2, 3);
    expect(progress).toHaveBeenNthCalledWith(3, 3, 3);
  });

  it("terminated pool throws on encodeLayers call", async () => {
    const { SaveWorkerPool } = await import("../saveWorkerPool");
    const pool = new SaveWorkerPool();
    pool.terminate();

    const signal = new AbortController().signal;
    const task = makeTask("dead");
    await expect(pool.encodeLayers([task], signal)).rejects.toThrow("terminated");
  });

  it("terminate is idempotent (can be called multiple times)", async () => {
    const { SaveWorkerPool } = await import("../saveWorkerPool");
    const pool = new SaveWorkerPool();
    pool.terminate();
    pool.terminate(); // second call should not throw
    const signal = new AbortController().signal;
    await expect(pool.encodeLayers([makeTask("x")], signal)).rejects.toThrow("terminated");
  });
});

describe("saveWorkerPool singleton", () => {
  afterEach(async () => {
    const { resetSaveWorkerPool } = await import("../saveWorkerPool");
    resetSaveWorkerPool();
  });

  it("getSaveWorkerPool returns the same instance on multiple calls", async () => {
    const { getSaveWorkerPool, destroySaveWorkerPool } = await import("../saveWorkerPool");
    const a = getSaveWorkerPool();
    const b = getSaveWorkerPool();
    expect(a).toBe(b);
    destroySaveWorkerPool(); // cleanup
  });

  it("destroySaveWorkerPool creates a fresh pool on next get", async () => {
    const { getSaveWorkerPool, destroySaveWorkerPool } = await import("../saveWorkerPool");
    const a = getSaveWorkerPool();
    destroySaveWorkerPool();
    const b = getSaveWorkerPool();
    expect(a).not.toBe(b);
    destroySaveWorkerPool();
  });
});
