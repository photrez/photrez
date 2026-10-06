// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { decodeRustBytes, readPixelSeedCall } from "@/lib/protocol/pixelSeedCall";
import { fillActiveLayerWithColor } from "../layerOperations";
import { CommandHistory, historyBridgeEnabled } from "@/engine/history";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";
import { FaithfulOffscreenCanvas } from "@/__tests__/faithfulOffscreenCanvas";

// The history bridge fires through a dynamic import + fire-and-forget invoke, so
// draining the census once can still miss a command that has not started yet.
const settleBridge = async () => {
  for (let i = 0; i < 3; i++) {
    await new Promise((r) => setTimeout(r, 0));
    await flushPixelInvokeCensus();
  }
};

const enableHistoryBridge = () => {
  localStorage.setItem("photrez.historyBridge", "1");
  (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
};

const disableHistoryBridge = () => {
  localStorage.removeItem("photrez.historyBridge");
  delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
};

const countInvoke = (cmd: string) => mockInvoke.mock.calls.filter((c) => c[0] === cmd).length;

// jsdom lacks ImageData / createImageBitmap / OffscreenCanvas — stub them.
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

const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args: any, options?: any) => mockInvoke(cmd, args, options),
}));

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

// Toast spy: the failure toast must carry the raw rejection text, so the
// production toast module is kept and only `showToast` is intercepted.
const showToastMock = vi.fn();
vi.mock("@/components/editor/Toast", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/editor/Toast")>();
  return { ...actual, showToast: (...args: Parameters<typeof actual.showToast>) => showToastMock(...args) };
});

// createImageBitmap(ImageData) → object whose getImageData returns the same buffer,
// so the fill canvas can drawImage (copy) the canonical `before` pixels.
(globalThis as any).createImageBitmap = async (src: any) => {
  const data = src?.data ?? new Uint8ClampedArray((src?.width ?? 1) * (src?.height ?? 1) * 4);
  return { width: src.width, height: src.height, getImageData: () => ({ data, width: src.width, height: src.height }) };
};

function makeFakes(opts: {
  w?: number; h?: number; sel?: any; locked?: boolean; visible?: boolean;
  basicAdjustment?: any; surfaceNull?: boolean; noBitmap?: boolean;
  layerType?: "raster" | "shape" | "text";
} = {}) {
  const w = opts.w ?? 100, h = opts.h ?? 100;
  const surface = { context: { putImageData: vi.fn(), getImageData: vi.fn((_x: number, _y: number, ww: number, hh: number) => new FakeImageData(ww, hh)) }, pixelEpoch: 0, pixelVersion: 0 } as any;
  const commit = vi.fn();
  const history = { commit } as any;
  const uploadSurfaceTiles = vi.fn();
  const layer = {
    id: "L1", width: w, height: h, locked: opts.locked ?? false, visible: opts.visible ?? true, lockTransparency: false,
    // "raster" by default; a case that wants the parametric refusal passes
    // `layerType: "shape" | "text"`. See the `isShapeLayer` / `isTextLayer`
    // fakes below.
    type: opts.layerType ?? "raster",
    transform: { scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false, x: 0, y: 0 },
    basicAdjustment: opts.basicAdjustment ?? null,
  };
  let basicAdj = layer.basicAdjustment;
  const clearBasicAdjustments = vi.fn(() => { basicAdj = null; layer.basicAdjustment = null; });
  // `noBitmap` models a freshly added layer: getPaintSurface has nothing to seed
  // from until a raster is set, exactly like DocumentEngine.
  let hasBitmap = !opts.noBitmap;
  const setLayerImageBitmap = vi.fn(() => { hasBitmap = true; });
  const engine: any = {
    getActiveLayerId: () => "L1",
    getLayer: () => layer,
    getSelection: () => opts.sel ?? null,
    getPaintSurface: () => (opts.surfaceNull || !hasBitmap ? null : surface),
    getId: () => "doc1",
    snapshot: () => ({ basicAdjustment: basicAdj }),
    restore: (s: any) => { basicAdj = s.basicAdjustment; layer.basicAdjustment = basicAdj; },
    clearBasicAdjustments,
    getLayerImageBitmap: vi.fn(() => (hasBitmap ? ({ close: vi.fn() } as unknown as ImageBitmap) : null)),
    setLayerImageBitmap,
    // The parametric refusal. `fillActiveLayerWithColor` asks the ENGINE whether
    // the active layer is shape or text before it touches the store - a
    // parametric layer cannot own pixels, because its next param edit re-derives
    // the raster and would erase the fill (see the producer's header). These
    // answer from the fixture's own `layerType`, so a case can opt into the
    // refusal; the default is a plain raster layer, which is what Fill Layer is
    // for. The refusal itself is measured in
    // parametricLayerPaintRefusal.wiring.test.ts.
    isShapeLayer: (id: string) => layer.id === id && layer.type === "shape",
    isTextLayer: (id: string) => layer.id === id && layer.type === "text",
  };
  const renderer: any = { uploadImage: vi.fn(), uploadSurfaceTiles };
  return { w, h, surface, commit, history, uploadSurfaceTiles, engine, renderer, layer, surfaceNull: opts.surfaceNull };
}

describe("fillActiveLayerWithColor — Rust canonical path (C5.4 Fill Layer)", () => {
  beforeEach(() => {
    installOffscreenCanvas();
    mockInvoke.mockReset();
    applyCalls.length = 0;
    showToastMock.mockClear();
    localStorage.setItem("photrez.rustPixels", "1");
  });

  // Defeat: send wr.rgba as a plain number array (layerOperations.ts:287) instead of Uint8Array; the toBeInstanceOf(Uint8Array) check goes RED.
  it("fills the whole layer through rust_pixels_write_region and commits ONE history step (no setLayerImageBitmap)", async () => {
    const { surface, commit, uploadSurfaceTiles, engine, renderer, history } = makeFakes();
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      if (cmd === "rust_pixels_write_region") {
        return { after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }], epoch: 1, version: 1 };
      }
      return undefined;
    });
    const ok = fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    expect(ok).toBe(true);
    await vi.waitFor(() => {
      expect(commit).toHaveBeenCalledTimes(1);
      expect(commit.mock.calls[0][1]).toBe("Fill Layer");
      const imp = commit.mock.calls[0][2];
      expect(imp.layerId).toBe("L1");
      // No pre-image: the write reply carries none, and this entry is a rustOwned
      // cursor token whose pixels undo reads back from Rust.
      expect(imp.before.length).toBe(0); expect(imp.after.length).toBe(1);
      const cmds = mockInvoke.mock.calls.map((c) => c[0]);
      expect(cmds).toContain("rust_pixels_snapshot_layer");
      expect(cmds).toContain("rust_pixels_write_region");
      const wr = decodeRustBytes<{ x: number; y: number; w: number; h: number; rgba: Uint8Array }>(
        mockInvoke.mock.calls.find((c) => c[0] === "rust_pixels_write_region")![1],
      );
      expect(wr.x).toBe(0); expect(wr.y).toBe(0); expect(wr.w).toBe(100); expect(wr.h).toBe(100);
      expect(wr.rgba.length).toBe(100 * 100 * 4);
      expect(wr.rgba[0]).toBe(255); expect(wr.rgba[3]).toBe(255);
      expect(wr.rgba).toBeInstanceOf(Uint8Array);
      expect(surface.pixelEpoch).toBe(1); expect(surface.pixelVersion).toBe(1);
      expect(uploadSurfaceTiles).toHaveBeenCalledWith("L1", 100, 100, expect.anything());
      expect(engine.setLayerImageBitmap).not.toHaveBeenCalled();
    }, { timeout: 2000 });
  });

  // RED-first: on the pre-fix tree this toast reads `Unknown error`, because a
  // Tauri v2 rejection is a bare string and `err instanceof Error` is false.
  it("surfaces a bare-string write_region rejection verbatim in the failure toast", async () => {
    const { commit, engine, renderer, history } = makeFakes();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") {
        return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      }
      if (cmd === "rust_pixels_write_region") throw "E_RUST: boom";
      return undefined;
    });

    fillActiveLayerWithColor(engine, history, renderer, "#ff0000");

    await vi.waitFor(() => {
      expect(showToastMock).toHaveBeenCalledWith("Fill Layer failed: E_RUST: boom", "error");
    }, { timeout: 2000 });
    expect(commit).not.toHaveBeenCalled();
  });

  it("seeds the canonical store via rust_pixels_init when the layer has no Rust entry yet (fill as FIRST raster op)", async () => {
    const { commit, engine, renderer, history } = makeFakes();
    const initCalls: any[] = [];
    mockInvoke.mockImplementation(async (cmd: string, args: any, options?: any) => {
      if (cmd === "rust_pixels_get_epoch") throw new Error("layer not initialized");
      if (cmd === "rust_pixels_init") { initCalls.push(readPixelSeedCall(cmd, args)!); return undefined; }
      if (cmd === "rust_pixels_snapshot_layer") return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      if (cmd === "rust_pixels_write_region") return { after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }], epoch: 1, version: 1 };
      return undefined;
    });
    fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    await vi.waitFor(() => {
      expect(initCalls.length).toBe(1);
      expect(initCalls[0].docId).toBe("doc1"); expect(initCalls[0].layerId).toBe("L1");
      expect(initCalls[0].width).toBe(100); expect(initCalls[0].height).toBe(100);
      expect(initCalls[0].bytes.length).toBe(100 * 100 * 4);
      expect(initCalls[0].bytes).toBeInstanceOf(Uint8Array);
      const cmds = mockInvoke.mock.calls.map((c) => c[0]);
      expect(cmds).toContain("rust_pixels_init");
      expect(cmds).toContain("rust_pixels_write_region");
      expect(commit).toHaveBeenCalledTimes(1);
    }, { timeout: 2000 });
  });

  it("increment epoch/version once on a SECOND fill (reads canonical)", async () => {
    const { commit, engine, renderer, surface, history } = makeFakes();
    let epoch = 0;
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return epoch;
      if (cmd === "rust_pixels_snapshot_layer") return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      if (cmd === "rust_pixels_write_region") {
        epoch += 1;
        return { after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }], epoch, version: epoch };
      }
      return undefined;
    });
    fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    await vi.waitFor(() => expect(commit).toHaveBeenCalledTimes(1), { timeout: 2000 });
    expect(surface.pixelEpoch).toBe(1);
    fillActiveLayerWithColor(engine, history, renderer, "#00ff00");
    await vi.waitFor(() => expect(commit).toHaveBeenCalledTimes(2), { timeout: 2000 });
    expect(surface.pixelEpoch).toBe(2);
    expect(surface.pixelVersion).toBe(2);
  });

  it("existing non-white layer: whole-layer fill overwrites and clears basicAdjustment", async () => {
    const { commit, engine, renderer, history } = makeFakes({ basicAdjustment: { brightness: 0.5 } });
    const gray = new Array(100 * 100 * 4).fill(128);
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") return [{ x: 0, y: 0, w: 100, h: 100, data: gray }];
      if (cmd === "rust_pixels_write_region") return { after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }], epoch: 1, version: 1 };
      return undefined;
    });
    fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    await vi.waitFor(() => {
      expect(commit).toHaveBeenCalledTimes(1);
      expect(engine.clearBasicAdjustments).toHaveBeenCalled();
    }, { timeout: 2000 });
  });

  it("rect selection → write_region scoped to the selection bbox", async () => {
    const { commit, engine, renderer, history } = makeFakes({ sel: { x: 10, y: 10, width: 20, height: 20, shape: "rect", inverted: false, angle: 0 } });
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      if (cmd === "rust_pixels_write_region") return { after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }], epoch: 1, version: 1 };
      return undefined;
    });
    fillActiveLayerWithColor(engine, history, renderer, "#00ff00");
    await vi.waitFor(() => {
      const wr = mockInvoke.mock.calls.find((c) => c[0] === "rust_pixels_write_region")![1];
      expect(wr.x).toBe(10); expect(wr.y).toBe(10); expect(wr.w).toBe(20); expect(wr.h).toBe(20);
    }, { timeout: 2000 });
  });

  it("ellipse selection → write_region scoped to the ellipse AABB", async () => {
    const { commit, engine, renderer, history } = makeFakes({ sel: { x: 20, y: 20, width: 60, height: 60, shape: "ellipse", inverted: false, angle: 0 } });
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      if (cmd === "rust_pixels_write_region") return { after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }], epoch: 1, version: 1 };
      return undefined;
    });
    fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    await vi.waitFor(() => {
      const wr = mockInvoke.mock.calls.find((c) => c[0] === "rust_pixels_write_region")![1];
      expect(wr.x).toBe(20); expect(wr.y).toBe(20); expect(wr.w).toBe(60); expect(wr.h).toBe(60);
    }, { timeout: 2000 });
  });

  it("inverted selection → write_region spans the whole layer (outside-region changed)", async () => {
    const { commit, engine, renderer, history } = makeFakes({ sel: { x: 10, y: 10, width: 20, height: 20, shape: "rect", inverted: true, angle: 0 } });
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      if (cmd === "rust_pixels_write_region") return { after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }], epoch: 1, version: 1 };
      return undefined;
    });
    fillActiveLayerWithColor(engine, history, renderer, "#0000ff");
    await vi.waitFor(() => {
      const wr = mockInvoke.mock.calls.find((c) => c[0] === "rust_pixels_write_region")![1];
      expect(wr.x).toBe(0); expect(wr.y).toBe(0); expect(wr.w).toBe(100); expect(wr.h).toBe(100);
    }, { timeout: 2000 });
  });

  it("transparent layer becomes opaque (alpha 255) under whole-layer fill", async () => {
    const { commit, engine, renderer, history } = makeFakes();
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      if (cmd === "rust_pixels_write_region") return { after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }], epoch: 1, version: 1 };
      return undefined;
    });
    fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    await vi.waitFor(() => {
      const wr = decodeRustBytes<{ rgba: Uint8Array }>(
        mockInvoke.mock.calls.find((c) => c[0] === "rust_pixels_write_region")![1],
      );
      const rgba = wr.rgba;
      expect(rgba[3]).toBe(255);
      let opaque = 0;
      for (let i = 3; i < rgba.length; i += 4) if (rgba[i] === 255) opaque++;
      expect(opaque).toBe(100 * 100);
    }, { timeout: 2000 });
  });

  it("locked layer → no-op (no invoke, no commit)", () => {
    const { commit, engine, renderer, history } = makeFakes({ locked: true });
    const ok = fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    expect(ok).toBe(false);
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
  });

  it("hidden layer → still fills (preserves existing Fill Layer behavior, no visible guard)", async () => {
    const { commit, engine, renderer, history } = makeFakes({ visible: false });
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      if (cmd === "rust_pixels_write_region") return { after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }], epoch: 1, version: 1 };
      return undefined;
    });
    const ok = fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    expect(ok).toBe(true);
    await vi.waitFor(() => expect(commit).toHaveBeenCalledTimes(1), { timeout: 2000 });
  });

  it("one undo step restores basicAdjustment (metadata) via history.undo + restore", async () => {
    const { engine, renderer } = makeFakes({ basicAdjustment: { brightness: 0.5 } });
    const history = new CommandHistory();
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      if (cmd === "rust_pixels_write_region") return { after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }], epoch: 1, version: 1 };
      return undefined;
    });
    fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    await vi.waitFor(() => expect(engine.getLayer().basicAdjustment).toBeNull(), { timeout: 2000 });
    const undone = history.undo(engine.snapshot());
    expect(undone).not.toBeNull();
    engine.restore(undone);
    expect(engine.getLayer().basicAdjustment).not.toBeNull();
  });

  // RED-first: before the fallback fix the catch only toasts, so the adjustment
  // cleared above write_region is unreachable by undo (undo count stays 0).
  it("records a fallback history entry when the adjustment clear lands but write_region rejects", async () => {
    const { engine, renderer } = makeFakes({ basicAdjustment: { brightness: 0.5 } });
    const history = new CommandHistory();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      if (cmd === "rust_pixels_write_region") throw "E_RUST: boom";
      return undefined;
    });

    fillActiveLayerWithColor(engine, history, renderer, "#ff0000");

    await vi.waitFor(() => {
      expect(showToastMock).toHaveBeenCalledWith("Fill Layer failed: E_RUST: boom", "error");
    }, { timeout: 2000 });
    // The clear DID land before the rejected write; it must still be undoable.
    expect(engine.getLayer().basicAdjustment).toBeNull();
    expect(history.getUndoCount()).toBe(1);
    const undone = history.undo(engine.snapshot());
    expect(undone).not.toBeNull();
    engine.restore(undone);
    expect(engine.getLayer().basicAdjustment).not.toBeNull();
  });

  // The bridge gate must be ON for this pin to mean anything: with it off,
  // commit() skips the Rust append entirely and a zero count proves nothing.
  describe("fill commit pin (history bridge ON)", () => {
    afterEach(() => {
      disableHistoryBridge();
    });

    it("records ZERO apply_tile_patch: rust_pixels_write_region already owns the entry", async () => {
      enableHistoryBridge();
      const { engine, renderer } = makeFakes();
      const history = new CommandHistory();
      history.attachDocIdGetter(() => "doc1");
      mockInvoke.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "rust_pixels_get_epoch") return 0;
        if (cmd === "rust_pixels_snapshot_layer") return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
        if (cmd === "rust_pixels_write_region") {
          return { after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }], epoch: 1, version: 1 };
        }
        return undefined;
      });

      expect(historyBridgeEnabled()).toBe(true);
      fillActiveLayerWithColor(engine, history, renderer, "#ff0000");

      await vi.waitFor(() => expect(history.getUndoCount()).toBe(1), { timeout: 2000 });
      await settleBridge();
      // Positive control first: the op ran and reached Rust exactly once.
      expect(countInvoke("rust_pixels_write_region")).toBe(1);
      expect(countInvoke("apply_tile_patch")).toBe(0);
    });
  });
});

// A layer that cannot host a raster at all must fail visibly rather than quietly
// writing through the legacy arm, which would be a second pixel owner with no
// Rust Pixel entry to undo against. This fake engine never yields a surface (its
// getPaintSurface ignores the bitmap it was handed), so it stands in for that.
describe("fillActiveLayerWithColor with no paint surface", () => {
  beforeEach(() => {
    installOffscreenCanvas();
    mockInvoke.mockReset();
    applyCalls.length = 0;
    showToastMock.mockClear();
  });

  it("surfaces a visible error, zero write_region, and no silent legacy write", () => {
    const { commit, engine, renderer, history } = makeFakes({ surfaceNull: true });
    const ok = fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    expect(ok).toBe(true);
    expect(showToastMock).toHaveBeenCalledWith("Rust pixel surface not ready", "warn");
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
  });
});

// A freshly added layer has no raster, and Alt+Del on it must still fill. The
// fill materialises the empty raster first so the canonical store has something
// to seed from, then records through Rust like any other layer.
// RED-first: before the blank raster was materialised, the surface stayed null,
// the op took the "surface not ready" exit and committed nothing.
describe("fillActiveLayerWithColor on a layer with no raster yet", () => {
  beforeEach(() => {
    installOffscreenCanvas();
    mockInvoke.mockReset();
    applyCalls.length = 0;
    showToastMock.mockClear();
  });

  it("materialises the empty raster and records one rustOwned canonical write", async () => {
    const { commit, engine, renderer, history } = makeFakes({ noBitmap: true });
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") {
        return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      }
      if (cmd === "rust_pixels_write_region") {
        return {
          after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }],
          epoch: 1, version: 1,
        };
      }
      return undefined;
    });

    expect(fillActiveLayerWithColor(engine, history, renderer, "#ff0000")).toBe(true);

    await vi.waitFor(() => {
      expect(engine.setLayerImageBitmap, "the empty raster was materialised").toHaveBeenCalledTimes(1);
      expect(mockInvoke.mock.calls.map((c) => c[0]), "recorded through Rust").toContain("rust_pixels_write_region");
      expect(commit).toHaveBeenCalledTimes(1);
      expect(commit.mock.calls[0][2]?.rustOwned, "the twin is a cursor token").toBe(true);
    }, { timeout: 2000 });
    expect(showToastMock, "no refusal toast").not.toHaveBeenCalled();
  });
});
// Faithful canvas: transferToImageBitmap() throws InvalidStateError unless a 2d
// context was obtained first, exactly like the real WebView2 canvas. The lenient
// stub this replaces is what let a context-less transfer ship as a crash that no
// suite could see.
function installOffscreenCanvas() {
  (globalThis as any).OffscreenCanvas = FaithfulOffscreenCanvas;
}
