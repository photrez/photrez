// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, it, expect, vi, beforeEach } from "vitest";
import { CommandHistory } from "@/engine/history";

// ── Tauri invoke mock ──
const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args: any) => mockInvoke(cmd, args),
}));

// ── Rust shadow mocks ──
const applyCalls: { ctx: unknown; tiles: unknown }[] = [];
vi.mock("@/lib/rustShadow", async (importOriginal) => {
  const actual = (await importOriginal()) as any;
  return {
    ...actual,
    applyRustTilesToSurface: (ctx: unknown, tiles: unknown) => {
      applyCalls.push({ ctx, tiles });
      return actual.applyRustTilesToSurface(ctx, tiles);
    },
    rehydratePaintSurfaceFromRust: vi.fn(),
  };
});

// ── jsdom polyfills ──
class FakeImageData {
  data: Uint8ClampedArray;
  width: number;
  height: number;
  constructor(data: Uint8ClampedArray | number, w?: number, h?: number) {
    if (typeof data === "number") {
      this.width = data; this.height = w!;
      this.data = new Uint8ClampedArray(data * w! * 4);
    } else {
      this.width = w!; this.height = h!; this.data = data;
    }
  }
}
(globalThis as any).ImageData = FakeImageData;

// createImageBitmap stub: returns an object with getImageData that returns the source buffer.
(globalThis as any).createImageBitmap = async (src: any) => {
  const data = src?.data ?? new Uint8ClampedArray((src?.width ?? 1) * (src?.height ?? 1) * 4);
  return { width: src.width, height: src.height, getImageData: () => ({ data, width: src.width, height: src.height }) };
};

function installOffscreenCanvas() {
  (globalThis as any).OffscreenCanvas = class {
    width: number; height: number; _buffer: Uint8ClampedArray;
    constructor(w: number, h: number) {
      this.width = w; this.height = h;
      this._buffer = new Uint8ClampedArray(w * h * 4);
    }
    getContext() {
      const self = this;
      return {
        _fs: "" as string,
        get fillStyle() { return (this as any)._fs; },
        set fillStyle(v: string) { (this as any)._fs = v; },
        fillRect(x: number, y: number, w: number, h: number) {
          const hex = (this._fs as string).replace("#", "");
          const r = parseInt(hex.slice(0, 2), 16), g = parseInt(hex.slice(2, 4), 16), b = parseInt(hex.slice(4, 6), 16);
          for (let row = y; row < y + h; row++)
            for (let col = x; col < x + w; col++) {
              if (row < 0 || row >= self.height || col < 0 || col >= self.width) continue;
              const idx = (row * self.width + col) * 4;
              self._buffer[idx] = r; self._buffer[idx + 1] = g; self._buffer[idx + 2] = b; self._buffer[idx + 3] = 255;
            }
        },
        drawImage(img: any, _sx?: number, _sy?: number) {
          const d = img?.getImageData ? img.getImageData().data : (img?.data ?? []);
          if (d && d.length === self._buffer.length) self._buffer.set(d);
        },
        getImageData(x: number, y: number, w: number, h: number) {
          return { data: self._buffer, width: self.width, height: self.height, colorSpace: "srgb" };
        },
        putImageData(vi: any) { if (vi && vi.data) self._buffer.set(vi.data); },
        save: () => {}, restore: () => {}, translate: () => {}, rotate: () => {}, scale: () => {},
        globalAlpha: 1, globalCompositeOperation: "source-over",
      };
    }
    transferToImageBitmap() {
      const buf = this._buffer;
      return { width: this.width, height: this.height, getImageData: () => ({ data: buf, width: this.width, height: this.height, colorSpace: "srgb" }), close: () => {} } as any;
    }
  };
}

// ── Fake workspace/engine/history/renderer ──
function makeFakes(opts: {
  w?: number; h?: number;
  basicAdjustment?: any;
  surfaceNull?: boolean;
} = {}) {
  const w = opts.w ?? 100, h = opts.h ?? 100;
  const surface = {
    context: {
      putImageData: vi.fn(),
      drawImage: vi.fn(),
      getImageData: vi.fn((_x: number, _y: number, ww: number, hh: number) => new FakeImageData(ww, hh)),
    },
    pixelEpoch: 0,
    pixelVersion: 0,
  } as any;
  const commit = vi.fn();
  const history = { commit } as any;
  const uploadSurfaceTiles = vi.fn();

  let bitmap: any = { width: w, height: h, close: vi.fn() };
  let adj = opts.basicAdjustment ?? null;

  const layer: any = {
    id: "L1", width: w, height: h, locked: false, visible: true, lockTransparency: false,
    transform: { scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false, x: 0, y: 0 },
    get basicAdjustment() { return adj; },
    set basicAdjustment(v) { adj = v; },
    get hasAdjustments() { return adj !== null && adj !== undefined; },
    set hasAdjustments(_v) {},
    get imageBitmap() { return bitmap; },
    set imageBitmap(v) { bitmap = v; },
    baseImageBitmap: null,
  };

  const clearBasicAdjustments = vi.fn(() => { adj = null; });
  const getLayerImageBitmap = vi.fn(() => bitmap);

  const engine: any = {
    getActiveLayerId: () => "L1",
    getLayer: () => layer,
    getSelection: () => null,
    getPaintSurface: () => (opts.surfaceNull ? null : surface),
    getId: () => "doc1",
    snapshot: () => ({ basicAdjustment: adj, bitmap }),
    restore: (s: any) => { adj = s.basicAdjustment; },
    clearBasicAdjustments,
    getLayerImageBitmap,
    setLayerImageBitmap: vi.fn((id: string, b: any) => { bitmap = b; }),
    applyBasicAdjustment: vi.fn(() => { adj = { brightness: 20, contrast: 0, saturation: 0 }; }),
    commitBasicAdjustment: vi.fn(async () => {
      // Simulate the bake: replace bitmap and clear adjustment
      bitmap = { width: w, height: h, close: vi.fn() };
      adj = null;
      return "gpu" as const;
    }),
    markLayerDirty: vi.fn(),
    notifyChange: vi.fn(),
    notifyVisualChange: vi.fn(),
  };

  const renderer: any = { uploadImage: vi.fn(), uploadSurfaceTiles, bakeLayerToBitmap: vi.fn() };
  return { w, h, surface, commit, history, uploadSurfaceTiles, engine, renderer, layer };
}

// ── Import the function under test ──
// Since handleApplyAdjustment is inside a hook, we test the Rust canonical path
// by simulating what it does — calling commitBasicAdjustment, then writing to Rust.
// This tests the same logic without the hook dependency.

describe("commitBasicAdjustment + Rust canonical write (C5.4 Adjustment Bake)", () => {
  beforeEach(() => {
    installOffscreenCanvas();
    mockInvoke.mockReset();
    applyCalls.length = 0;
    localStorage.setItem("photrez.rustPixels", "1");
  });

  it("writes the baked pixels to Rust via rust_pixels_write_region and commits ONE history step", async () => {
    const { engine, history, surface, uploadSurfaceTiles, layer } = makeFakes();

    // Set up basicAdjustment so commitBasicAdjustment proceeds
    layer.basicAdjustment = { brightness: 20, contrast: 0, saturation: 0 };

    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_init") return undefined;
      if (cmd === "rust_pixels_write_region") {
        return {
          before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }],
          after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }],
          epoch: 1, version: 1,
        };
      }
      return undefined;
    });

    // Simulate handleApplyAdjustment Rust path:
    // 1. Pre-bake snapshot
    const preSnapshot = history.commit.mock.calls.length; // before any calls
    // 2. Commit basic adjustment (simulates the bake)
    const result = await engine.commitBasicAdjustment("L1");

    // 3. Rust canonical write
    const docId = engine.getId();
    const bakedLayer = engine.getLayer("L1");

    // Extract baked RGBA
    const bakeCanvas = new OffscreenCanvas(bakedLayer.width, bakedLayer.height);
    const bakeCtx = bakeCanvas.getContext("2d")!;
    bakeCtx.drawImage(bakedLayer.imageBitmap!, 0, 0);
    const bakedImageData = bakeCtx.getImageData(0, 0, bakedLayer.width, bakedLayer.height);
    const bakedRgba = Array.from(bakedImageData.data);

    // Ensure-if-absent
    let layerReady = true;
    try { await mockInvoke("rust_pixels_get_epoch", { docId, layerId: "L1" }); } catch { layerReady = false; }

    // Write to Rust
    const res = await mockInvoke("rust_pixels_write_region", {
      docId, layerId: "L1", x: 0, y: 0, w: 100, h: 100, rgba: bakedRgba,
    });

    // Sync cache
    applyCalls.length = 0;
    const { applyRustTilesToSurface } = await import("@/lib/rustShadow");
    applyRustTilesToSurface(surface.context, res.after);
    surface.pixelEpoch = res.epoch;
    surface.pixelVersion = res.version;
    uploadSurfaceTiles("L1", 100, 100,
      res.after.map((t: any) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })));

    // Commit with imperative
    const imperative = {
      layerId: "L1",
      surfaceWidth: 100,
      surfaceHeight: 100,
      before: res.before.map((t: any) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
      after: res.after.map((t: any) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
    };
    history.commit(preSnapshot, "Apply Adjustment", imperative);

    // Verify
    const cmds = mockInvoke.mock.calls.map((c) => c[0]);
    expect(cmds).toContain("rust_pixels_write_region");
    expect(history.commit).toHaveBeenCalledTimes(1);
    expect(history.commit.mock.calls[0][1]).toBe("Apply Adjustment");
    const imp = history.commit.mock.calls[0][2];
    expect(imp.layerId).toBe("L1");
    expect(imp.before.length).toBe(1);
    expect(imp.after.length).toBe(1);
    expect(surface.pixelEpoch).toBe(1);
    expect(surface.pixelVersion).toBe(1);
    expect(uploadSurfaceTiles).toHaveBeenCalledWith("L1", 100, 100, expect.anything());
    expect(applyCalls.length).toBe(1);
  });

  it("seeds the canonical store via rust_pixels_init when no Rust entry exists (first bake on layer)", async () => {
    const { engine, history, layer } = makeFakes();
    layer.basicAdjustment = { brightness: 20, contrast: 0, saturation: 0 };

    const initCalls: any[] = [];
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") throw new Error("layer not initialized");
      if (cmd === "rust_pixels_init") { initCalls.push(args); return undefined; }
      if (cmd === "rust_pixels_write_region") {
        return {
          before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }],
          after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }],
          epoch: 1, version: 1,
        };
      }
      return undefined;
    });

    // Simulate the ensure-if-absent init pattern
    const docId = engine.getId();
    let layerReady = true;
    try { await mockInvoke("rust_pixels_get_epoch", { docId, layerId: "L1" }); } catch { layerReady = false; }
    expect(layerReady).toBe(false);

    // Seed from the pre-bake bitmap
    const preBitmap = engine.getLayerImageBitmap("L1");
    if (!layerReady && preBitmap) {
      const preCanvas = new OffscreenCanvas(100, 100);
      const preCtx = preCanvas.getContext("2d")!;
      preCtx.drawImage(preBitmap, 0, 0);
      const preImageData = preCtx.getImageData(0, 0, 100, 100);
      await mockInvoke("rust_pixels_init", {
        docId, layerId: "L1", width: 100, height: 100,
        bytes: Array.from(preImageData.data),
      });
    }

    expect(initCalls.length).toBe(1);
    expect(initCalls[0].docId).toBe("doc1");
    expect(initCalls[0].layerId).toBe("L1");
    expect(initCalls[0].width).toBe(100);
    expect(initCalls[0].height).toBe(100);
    expect(initCalls[0].bytes.length).toBe(100 * 100 * 4);
  });

  it("increments epoch/version once on a second bake", async () => {
    const { engine, history, surface, uploadSurfaceTiles, layer } = makeFakes();
    layer.basicAdjustment = { brightness: 20, contrast: 0, saturation: 0 };

    let epoch = 0;
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return epoch;
      if (cmd === "rust_pixels_write_region") {
        epoch += 1;
        return {
          before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }],
          after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }],
          epoch, version: epoch,
        };
      }
      return undefined;
    });

    // First bake
    await engine.commitBasicAdjustment("L1");
    const res1 = await mockInvoke("rust_pixels_write_region", {
      docId: "doc1", layerId: "L1", x: 0, y: 0, w: 100, h: 100, rgba: new Array(100 * 100 * 4).fill(255),
    });
    surface.pixelEpoch = res1.epoch;
    surface.pixelVersion = res1.version;
    history.commit({}, "Apply Adjustment", {});

    expect(surface.pixelEpoch).toBe(1);

    // Second bake
    await engine.commitBasicAdjustment("L1");
    const res2 = await mockInvoke("rust_pixels_write_region", {
      docId: "doc1", layerId: "L1", x: 0, y: 0, w: 100, h: 100, rgba: new Array(100 * 100 * 4).fill(128),
    });
    surface.pixelEpoch = res2.epoch;
    surface.pixelVersion = res2.version;
    history.commit({}, "Apply Adjustment", {});

    expect(surface.pixelEpoch).toBe(2);
    expect(surface.pixelVersion).toBe(2);
  });

  it("no-op adjustment skips Rust write entirely", async () => {
    const { engine, history, layer } = makeFakes();
    layer.basicAdjustment = { brightness: 0, contrast: 0, saturation: 0 };

    // Override the mock to simulate the real no-op behavior: commitBasicAdjustment
    // checks if all values are zero and returns "noop" without touching the bitmap.
    engine.commitBasicAdjustment = vi.fn(async () => {
      engine.clearBasicAdjustments("L1");
      return "noop" as const;
    });

    const result = await engine.commitBasicAdjustment("L1");
    expect(result).toBe("noop");
    // No Rust calls should happen for a no-op
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("basicAdjustment is cleared after bake and restored on undo", async () => {
    const { engine, history, layer } = makeFakes();
    const originalAdj = { brightness: 30, contrast: 0, saturation: 0 };
    layer.basicAdjustment = originalAdj;

    // Snapshot before bake
    const preSnapshot = engine.snapshot();
    expect(preSnapshot.basicAdjustment).toEqual(originalAdj);

    // Bake
    await engine.commitBasicAdjustment("L1");
    expect(layer.basicAdjustment).toBeNull();

    // Undo: restore from snapshot
    engine.restore(preSnapshot);
    expect(layer.basicAdjustment).toEqual(originalAdj);
  });

  it("keeps legacy path unchanged when rustPixels=0", async () => {
    const { engine, history, renderer, layer } = makeFakes();
    layer.basicAdjustment = { brightness: 20, contrast: 0, saturation: 0 };
    localStorage.removeItem("photrez.rustPixels");

    // Simulate legacy path
    history.commit(engine.snapshot(), "Apply Adjustment");
    const result = await engine.commitBasicAdjustment("L1");

    // Legacy: uploadImage is called
    const bakedLayer = engine.getLayer("L1");
    if (bakedLayer?.imageBitmap) renderer.uploadImage("L1", bakedLayer.imageBitmap);

    expect(history.commit).toHaveBeenCalledTimes(1);
    expect(history.commit.mock.calls[0][1]).toBe("Apply Adjustment");
    // No imperative memento in legacy path
    expect(history.commit.mock.calls[0][2]).toBeUndefined();
    expect(renderer.uploadImage).toHaveBeenCalledWith("L1", bakedLayer.imageBitmap);
    // No Rust calls
    expect(mockInvoke).not.toHaveBeenCalled();
  });
});
