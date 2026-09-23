// GPU/worker boundary: WebGPU compute and worker output are an accelerator and
// preview seam only. Their pixels re-enter the document model as INPUT; a
// canonical history entry is produced only by the command layer committing an
// engine snapshot (see "Apply Adjustment" in components/editor/layers/
// useLayerActions.ts, which captures the pre-bake snapshot itself). Renderer
// textures stay display-only state owned by the WebGL2 backend; the print and
// save workers return files, never history payloads.
//
// Reference invariants (docs/architecture/CHECKPOINT-2026-08-24-hybrid-migration.md):
//   - "GPU resources = renderer-owned derived state ... RenderSnapshot/RenderDelta
//     carry resourceId + dirtyRect (logical), never GPU handles."
//   - "WebGPU compute (gpuCompute.ts WGSL invert/adjust) is an interactive
//     compute layer for filters, not the renderer. No WebGPU canvas ownership."
//   - History payloads are metadata/token based and "must not become pixel
//     history" - raw readback bytes never enter a history entry.
// docs/ARCHITECTURE.md "GPU Compute Layer": interactive compute runs on the
// GPU (CPU fallback retained) and feeds the document model, not history.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import * as gpuCompute from "@/lib/gpu/gpuCompute";

// Stub OffscreenCanvas so the engine bake runs in jsdom. getImageData returns
// a seeded buffer; putImageData captures the final ImageData so we can assert
// the worker's readback pixels were written back as document input.
let currentSourcePixels: Uint8ClampedArray | null = null;
let lastPutImageData: { data: Uint8ClampedArray } | null = null;

function setupOffscreenCanvasMock() {
  const MockConstructor = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    this.getContext = vi.fn(() => ({
      drawImage: vi.fn(),
      putImageData: vi.fn((img: any) => {
        lastPutImageData = img;
      }),
      getImageData: vi.fn(() => ({
        data: currentSourcePixels ?? new Uint8ClampedArray(w * h * 4),
      })),
    }));
    this.transferToImageBitmap = vi.fn(
      () =>
        ({ width: this.width, height: this.height, close: vi.fn() } as unknown as ImageBitmap),
    );
  };
  vi.stubGlobal("OffscreenCanvas", MockConstructor as unknown as typeof OffscreenCanvas);
}

// LIVE production path: useLayerActions captures engine.snapshot() BEFORE the
// bake and commits that snapshot itself after engine.commitBasicAdjustment.
function makeAdjustedEngine() {
  const engine = new DocumentEngine("doc-gpu-boundary", "GPU boundary", 2, 1);
  const layer = engine.addLayer("L1");
  const initial = { width: 2, height: 1, close: vi.fn() } as unknown as ImageBitmap;
  engine.setLayerImageBitmap(layer.id, initial);
  engine.applyBasicAdjustment(layer.id, { brightness: 20, contrast: 0, saturation: 0 });
  return { engine, layer, initial };
}

type HistoryInternals = { undoStack: Array<{ snapshot: unknown }> };

function undoEntriesOf(history: CommandHistory): Array<{ snapshot: unknown }> {
  return (history as unknown as HistoryInternals).undoStack;
}

// Identity walk over the committed snapshot graph. Typed arrays and buffers
// are not descended into: their identity is already checked by the === above,
// and element-walking a raw pixel buffer proves nothing extra.
function references(root: unknown, target: object): boolean {
  const seen = new WeakSet<object>();
  const walk = (value: unknown): boolean => {
    if (value === target) return true;
    if (value === null || typeof value !== "object") return false;
    if (seen.has(value)) return false;
    if (ArrayBuffer.isView(value) || value instanceof ArrayBuffer) return false;
    seen.add(value);
    return Object.values(value as Record<string, unknown>).some(walk);
  };
  return walk(root);
}

describe("GPU boundary - worker output never writes canonical history", () => {
  beforeEach(() => setupOffscreenCanvasMock());
  afterEach(() => {
    currentSourcePixels = null;
    lastPutImageData = null;
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("runs the adjustment GPU bake without any CommandHistory write", async () => {
    const commitSpy = vi.spyOn(CommandHistory.prototype, "commit");
    const snapshotSpy = vi.spyOn(CommandHistory.prototype, "recordSnapshotHistory");
    const { engine, layer, initial } = makeAdjustedEngine();

    currentSourcePixels = new Uint8ClampedArray([10, 20, 30, 255, 40, 50, 60, 128]);
    const workerOutput = new Uint8Array([180, 90, 210, 255, 1, 2, 3, 4]);
    const seam = vi.spyOn(gpuCompute, "adjustRgba").mockImplementation(async () => {
      return { data: workerOutput, usedGpu: true };
    });

    const result = await engine.commitBasicAdjustment(layer.id);

    expect(seam).toHaveBeenCalledTimes(1); // the seam really ran (anti-vacuity)
    expect(result).toBe("gpu");
    expect(layer.imageBitmap).not.toBe(initial); // readback landed as input
    expect(Array.from(lastPutImageData!.data)).toEqual(Array.from(workerOutput));
    expect(commitSpy).not.toHaveBeenCalled();
    expect(snapshotSpy).not.toHaveBeenCalled();
  });

  it("keeps the raw worker output buffer out of every committed history entry", async () => {
    const history = new CommandHistory();
    const { engine, layer } = makeAdjustedEngine();

    currentSourcePixels = new Uint8ClampedArray([10, 20, 30, 255, 40, 50, 60, 128]);
    const workerOutput = new Uint8Array([180, 90, 210, 255, 1, 2, 3, 4]);
    vi.spyOn(gpuCompute, "adjustRgba").mockImplementation(async () => {
      return { data: workerOutput, usedGpu: true };
    });

    // Command-layer ordering (mirrors useLayerActions "Apply Adjustment"):
    // capture BEFORE the bake, commit the engine snapshot AFTER it.
    const preBake = engine.snapshot();
    await engine.commitBasicAdjustment(layer.id);
    history.commit(preBake, "Apply Adjustment");

    const entries = undoEntriesOf(history);
    expect(entries.some((entry) => references(entry.snapshot, workerOutput))).toBe(false);
    expect(entries).toHaveLength(1);
    expect(Array.from(lastPutImageData!.data)).toEqual(Array.from(workerOutput)); // input half
  });
});

describe("GPU boundary - accelerator, renderer, and worker sources hold no history authority", () => {
  // Matches an import of the history module or a call into a history entry
  // point. Comments are stripped first so prose mentions do not count.
  const HISTORY_AUTHORITY =
    /\bCommandHistory\b|recordSnapshotHistory|history\s*\.\s*commit\b|from\s+["'][^"']*engine\/history["']/;

  const SRC_ROOT = resolve(__dirname, "../..");
  const SEAM_SOURCES: Array<[string, string]> = [
    ["WGSL accelerator", "lib/gpu/gpuCompute.ts"],
    ["renderer texture owner", "renderer/webgl2.ts"],
    ["print worker", "components/editor/print/printWorker.ts"],
    ["save worker", "components/editor/saveWorkerPool.ts"],
  ];

  function codeOnly(source: string): string {
    return source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");
  }

  it.each(SEAM_SOURCES)("%s source (%s) never references a history entry point", (_label, relPath) => {
    const code = codeOnly(readFileSync(resolve(SRC_ROOT, relPath), "utf8"));
    expect(code).not.toMatch(HISTORY_AUTHORITY);
  });
});
