import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import type { DocumentEngine } from "@/engine/document";
import type { LayerNode } from "@/engine/types";

// Parallel-bake contract for encodeComposite:
// - per-layer ensure + bake work overlaps (bounded), instead of one by one
// - the composite still draws bottom-to-top in stack order with each baked
//   bitmap reaching its own layer (a reordered composite corrupts export)
// - a bake failure rejects and every temp bitmap made along the way is closed

const hoisted = vi.hoisted(() => ({
  drawIds: [] as string[],
  drawnBitmapById: new Map<string, unknown>(),
  ensureActive: 0,
  ensureMax: 0,
  bakeActive: 0,
  bakeMax: 0,
  madeBitmaps: [] as Array<{ close: () => void; closed: boolean }>,
  failBake: false,
}));

vi.mock("../wasmExport", () => ({
  encodeImageWithWasm: async () => new Uint8Array([9, 9, 9]),
}));

vi.mock("@/engine/layerComposite", () => ({
  drawLayerToContext: (_ctx: unknown, layer: { id: string; imageBitmap: unknown }) => {
    hoisted.drawIds.push(layer.id);
    hoisted.drawnBitmapById.set(layer.id, layer.imageBitmap);
  },
}));

vi.mock("@/engine/layerAdjustments", () => ({
  bakeAdjustmentToBitmapGpu: async () => {
    if (hoisted.failBake) throw new Error("gpu bake down");
    hoisted.bakeActive += 1;
    hoisted.bakeMax = Math.max(hoisted.bakeMax, hoisted.bakeActive);
    await new Promise((r) => setTimeout(r, 5));
    hoisted.bakeActive -= 1;
    const entry = { closed: false, close() { entry.closed = true; } };
    hoisted.madeBitmaps.push(entry);
    return entry as unknown as ImageBitmap;
  },
  bakeAdjustmentToBitmap: async () => {
    if (hoisted.failBake) throw new Error("cpu bake down");
    const entry = { closed: false, close() { entry.closed = true; } };
    hoisted.madeBitmaps.push(entry);
    return entry as unknown as ImageBitmap;
  },
}));

function makeLayer(id: string, opts?: { adjusted?: boolean; visible?: boolean; bitmap?: boolean }): LayerNode {
  return {
    id,
    name: id,
    type: "raster",
    visible: opts?.visible ?? true,
    opacity: 1,
    locked: false,
    blendMode: "normal",
    width: 2,
    height: 2,
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    imageBitmap: (opts?.bitmap ?? true) ? ({ marker: id } as unknown as ImageBitmap) : null!,
    ...(opts?.adjusted ? { basicAdjustment: { brightness: 10 } } : {}),
  } as LayerNode;
}

function makeEngine(layers: LayerNode[]): DocumentEngine {
  return {
    getId: () => "doc-1",
    getWidth: () => 2,
    getHeight: () => 2,
    getLayers: () => layers,
    ensureBitmapCurrent: async () => {
      hoisted.ensureActive += 1;
      hoisted.ensureMax = Math.max(hoisted.ensureMax, hoisted.ensureActive);
      await new Promise((r) => setTimeout(r, 5));
      hoisted.ensureActive -= 1;
    },
  } as unknown as DocumentEngine;
}

function stubCanvas() {
  const mockCtx = {
    clearRect: vi.fn(),
    fillRect: vi.fn(),
    getImageData: vi.fn().mockReturnValue({ data: new Uint8ClampedArray(16) }),
  };
  let fillStyle = "";
  Object.defineProperty(mockCtx, "fillStyle", {
    set(v: string) { fillStyle = v; },
    get() { return fillStyle; },
  });
  vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: unknown) {
    (this as { getContext: () => unknown }).getContext = () => mockCtx;
  }));
}

beforeEach(() => {
  hoisted.drawIds = [];
  hoisted.drawnBitmapById = new Map();
  hoisted.ensureActive = 0;
  hoisted.ensureMax = 0;
  hoisted.bakeActive = 0;
  hoisted.bakeMax = 0;
  hoisted.madeBitmaps = [];
  hoisted.failBake = false;
  stubCanvas();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("encodeComposite parallel bakes", () => {
  it("overlaps per-layer ensure and bake work", async () => {
    const layers = [
      makeLayer("top", { adjusted: true }),
      makeLayer("mid", { adjusted: true }),
      makeLayer("low", { adjusted: true }),
      makeLayer("base", { adjusted: true }),
      makeLayer("plain"),
    ];
    const { encodeComposite } = await import("../exportDocument");
    const bytes = await encodeComposite(makeEngine(layers), "png", 100);

    expect(bytes).toEqual(new Uint8Array([9, 9, 9]));
    expect(hoisted.ensureMax).toBeGreaterThan(1);
    expect(hoisted.bakeMax).toBeGreaterThan(1);
    expect(hoisted.bakeMax).toBeLessThanOrEqual(4);
  });

  it("composites bottom-to-top with each baked bitmap on its own layer", async () => {
    const layers = [
      makeLayer("top", { adjusted: true }),
      makeLayer("hidden", { adjusted: true, visible: false }),
      makeLayer("mid"),
      makeLayer("nopic", { bitmap: false }),
      makeLayer("bottom", { adjusted: true }),
    ];
    const { encodeComposite } = await import("../exportDocument");
    await encodeComposite(makeEngine(layers), "png", 100);

    expect(hoisted.drawIds).toEqual(["bottom", "mid", "top"]);
    for (const id of ["bottom", "top"]) {
      const drawn = hoisted.drawnBitmapById.get(id);
      expect(drawn).toBeDefined();
      expect((drawn as { marker?: string }).marker).toBeUndefined();
      expect(hoisted.madeBitmaps).toContain(drawn as { close: () => void; closed: boolean });
    }
    expect(hoisted.drawnBitmapById.get("mid")).toEqual({ marker: "mid" });
  });

  it("a bake failure rejects and closes every temp bitmap", async () => {
    hoisted.failBake = true;
    const layers = [makeLayer("a", { adjusted: true }), makeLayer("b", { adjusted: true })];
    const { encodeComposite } = await import("../exportDocument");

    await expect(encodeComposite(makeEngine(layers), "png", 100)).rejects.toThrow();
    expect(hoisted.drawIds).toEqual([]);
    for (const b of hoisted.madeBitmaps) expect(b.closed).toBe(true);
  });
});
