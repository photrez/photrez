import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { invertRgba, invertRgbaCpu, isGpuComputeAvailable, adjustRgba, adjustRgbaCpu, __resetInvertPoaCacheForTests } from "./gpuCompute";
import { applyBasicAdjustmentToPixelsTs, type BasicAdjustment } from "@/engine/layerAdjustments";

// Controllable fake for the wasm module namespace (Technique A tier).
const poaState = vi.hoisted(() => ({ mod: null as any }));
vi.mock("@/components/editor/wasmExport", () => ({
  getWasmExportModule: () => poaState.mod,
}));

function stubNavigatorGpu(present: boolean) {
  const nav = navigator as unknown as { gpu?: unknown };
  if (present) {
    Object.defineProperty(nav, "gpu", { value: {}, configurable: true });
  } else {
    delete nav.gpu;
  }
}

describe("gpuCompute invert (fallback path)", () => {
  it("isGpuComputeAvailable is false under jsdom (no navigator.gpu)", () => {
    expect(isGpuComputeAvailable()).toBe(false);
  });

  it("invertRgbaCpu inverts RGB and preserves alpha", () => {
    const px = new Uint8Array([10, 20, 30, 255, 0, 0, 0, 0]);
    expect(Array.from(invertRgbaCpu(px))).toEqual([245, 235, 225, 255, 255, 255, 255, 0]);
  });

  it("invertRgba falls back to CPU when GPU is absent and matches CPU result", async () => {
    const px = new Uint8Array([10, 20, 30, 255]);
    const res = await invertRgba(px);
    expect(res.usedGpu).toBe(false);
    expect(Array.from(res.data)).toEqual([245, 235, 225, 255]);
  });

  it("does not mutate the caller's input buffer", async () => {
    const px = new Uint8Array([10, 20, 30, 255]);
    const before = Array.from(px);
    await invertRgba(px);
    expect(Array.from(px)).toEqual(before);
  });
});

describe("gpuCompute invert — Technique A (Rust PoA) wiring", () => {
  const W = 2;
  const H = 1;
  const px = new Uint8Array([10, 20, 30, 255, 0, 0, 0, 128]);
  const inverted = new Uint8Array([245, 235, 225, 255, 255, 255, 255, 128]);

  beforeEach(() => {
    poaState.mod = null;
    __resetInvertPoaCacheForTests();
    stubNavigatorGpu(false);
  });
  afterEach(() => {
    stubNavigatorGpu(false);
    vi.restoreAllMocks();
  });

  it("routes through the Rust PoA renderer when present (producer called once per size)", async () => {
    stubNavigatorGpu(true);
    const createSpy = vi.fn(async (_w: number, _h: number) => ({
      invert: vi.fn(async (p: Uint8Array) => Uint8Array.from(inverted.map((b, i) => b ^ (p[i] ^ p[i])))),
    }));
    poaState.mod = { WebGpuAdjustRenderer: { create: createSpy } };

    const res1 = await invertRgba(px, W, H);
    expect(res1.usedGpu).toBe(true);
    expect(res1.data).toBeInstanceOf(Uint8Array);
    expect(createSpy).toHaveBeenCalledTimes(1);
    expect(createSpy).toHaveBeenCalledWith(W, H);

    // second call same size -> cached renderer, no re-create
    await invertRgba(px, W, H);
    expect(createSpy).toHaveBeenCalledTimes(1);
  });

  it("falls back to CPU when the wasm module lacks WebGpuAdjustRenderer", async () => {
    stubNavigatorGpu(true);
    poaState.mod = {};
    const res = await invertRgba(px, W, H);
    expect(res.usedGpu).toBe(false);
    expect(Array.from(res.data)).toEqual(Array.from(invertRgbaCpu(px)));
  });

  it("falls back to CPU when the PoA invert rejects (and resets cache for retry)", async () => {
    stubNavigatorGpu(true);
    const createSpy = vi.fn(async () => ({
      invert: vi.fn(async () => {
        throw new Error("device lost");
      }),
    }));
    poaState.mod = { WebGpuAdjustRenderer: { create: createSpy } };
    const res = await invertRgba(px, W, H);
    expect(res.usedGpu).toBe(false);
    expect(Array.from(res.data)).toEqual(Array.from(invertRgbaCpu(px)));
    // cache was reset -> next call re-creates
    await invertRgba(px, W, H);
    expect(createSpy).toHaveBeenCalledTimes(2);
  });

  it("skips the PoA tier entirely when dimensions are omitted", async () => {
    stubNavigatorGpu(true);
    const createSpy = vi.fn();
    poaState.mod = { WebGpuAdjustRenderer: { create: createSpy } };
    const res = await invertRgba(px);
    expect(createSpy).not.toHaveBeenCalled();
    expect(res.usedGpu).toBe(false);
    expect(Array.from(res.data)).toEqual(Array.from(invertRgbaCpu(px)));
  });
});

describe("gpuCompute adjust (B/C/S fallback path)", () => {
  const adj: BasicAdjustment = { brightness: 20, contrast: -40, saturation: 60 };

  it("adjustRgbaCpu matches the pure-TS reference bit-exact", () => {
    const px = new Uint8ClampedArray([100, 150, 200, 255, 0, 0, 0, 0, 255, 128, 64, 128]);
    const ref = applyBasicAdjustmentToPixelsTs(px, adj);
    expect(Array.from(adjustRgbaCpu(px, adj))).toEqual(Array.from(ref));
  });

  it("adjustRgba falls back to CPU when GPU is absent and matches the reference", async () => {
    const px = new Uint8ClampedArray([100, 150, 200, 255, 0, 0, 0, 0]);
    const res = await adjustRgba(px, adj);
    expect(res.usedGpu).toBe(false);
    const ref = applyBasicAdjustmentToPixelsTs(px, adj);
    expect(Array.from(res.data)).toEqual(Array.from(ref));
  });

  it("does not mutate the caller's input buffer", async () => {
    const px = new Uint8ClampedArray([100, 150, 200, 255]);
    const before = Array.from(px);
    await adjustRgba(px, adj);
    expect(Array.from(px)).toEqual(before);
  });
});
