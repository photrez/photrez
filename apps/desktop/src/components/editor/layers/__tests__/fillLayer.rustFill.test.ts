// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { fillActiveLayerWithColor } from "../layerOperations";
import { CommandHistory, historyBridgeEnabled } from "@/engine/history";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";

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
  invoke: (cmd: string, args: any) => mockInvoke(cmd, args),
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

// OffscreenCanvas mock with real pixel buffer + drawImage that copies the source.
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
        drawImage(img: any) {
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

function makeFakes(opts: {
  w?: number; h?: number; sel?: any; locked?: boolean; visible?: boolean;
  basicAdjustment?: any; surfaceNull?: boolean;
} = {}) {
  const w = opts.w ?? 100, h = opts.h ?? 100;
  const surface = { context: { putImageData: vi.fn(), getImageData: vi.fn((_x: number, _y: number, ww: number, hh: number) => new FakeImageData(ww, hh)) }, pixelEpoch: 0, pixelVersion: 0 } as any;
  const commit = vi.fn();
  const history = { commit } as any;
  const uploadSurfaceTiles = vi.fn();
  const layer = {
    id: "L1", width: w, height: h, locked: opts.locked ?? false, visible: opts.visible ?? true, lockTransparency: false,
    transform: { scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false, x: 0, y: 0 },
    basicAdjustment: opts.basicAdjustment ?? null,
  };
  let basicAdj = layer.basicAdjustment;
  const clearBasicAdjustments = vi.fn(() => { basicAdj = null; layer.basicAdjustment = null; });
  const engine: any = {
    getActiveLayerId: () => "L1",
    getLayer: () => layer,
    getSelection: () => opts.sel ?? null,
    getPaintSurface: () => (opts.surfaceNull ? null : surface),
    getId: () => "doc1",
    snapshot: () => ({ basicAdjustment: basicAdj }),
    restore: (s: any) => { basicAdj = s.basicAdjustment; layer.basicAdjustment = basicAdj; },
    clearBasicAdjustments,
    getLayerImageBitmap: vi.fn(),
    setLayerImageBitmap: vi.fn(),
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
        return { before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }], after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }], epoch: 1, version: 1 };
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
      expect(imp.before.length).toBe(1); expect(imp.after.length).toBe(1);
      const cmds = mockInvoke.mock.calls.map((c) => c[0]);
      expect(cmds).toContain("rust_pixels_snapshot_layer");
      expect(cmds).toContain("rust_pixels_write_region");
      const wr = mockInvoke.mock.calls.find((c) => c[0] === "rust_pixels_write_region")![1];
      expect(wr.x).toBe(0); expect(wr.y).toBe(0); expect(wr.w).toBe(100); expect(wr.h).toBe(100);
      expect((wr.rgba as number[]).length).toBe(100 * 100 * 4);
      expect((wr.rgba as number[])[0]).toBe(255); expect((wr.rgba as number[])[3]).toBe(255);
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
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") throw new Error("layer not initialized");
      if (cmd === "rust_pixels_init") { initCalls.push(args); return undefined; }
      if (cmd === "rust_pixels_snapshot_layer") return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      if (cmd === "rust_pixels_write_region") return { before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }], after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }], epoch: 1, version: 1 };
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
        return { before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }], after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }], epoch, version: epoch };
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
      if (cmd === "rust_pixels_write_region") return { before: [{ x: 0, y: 0, w: 100, h: 100, data: gray }], after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }], epoch: 1, version: 1 };
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
      if (cmd === "rust_pixels_write_region") return { before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }], after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }], epoch: 1, version: 1 };
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
      if (cmd === "rust_pixels_write_region") return { before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }], after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }], epoch: 1, version: 1 };
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
      if (cmd === "rust_pixels_write_region") return { before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }], after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }], epoch: 1, version: 1 };
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
      if (cmd === "rust_pixels_write_region") return { before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }], after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }], epoch: 1, version: 1 };
      return undefined;
    });
    fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    await vi.waitFor(() => {
      const wr = mockInvoke.mock.calls.find((c) => c[0] === "rust_pixels_write_region")![1];
      const rgba = wr.rgba as number[];
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
      if (cmd === "rust_pixels_write_region") return { before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }], after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }], epoch: 1, version: 1 };
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
      if (cmd === "rust_pixels_write_region") return { before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }], after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }], epoch: 1, version: 1 };
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
          return { before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }], after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }], epoch: 1, version: 1 };
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

// The flag is still OFF in production, so these routing cases are pinned here and
// the whole block is deleted when the flag is retired.
describe("keeps photrez.rustPixels-OFF behavior (transitional; delete when the flag is retired)", () => {
  beforeEach(() => {
    installOffscreenCanvas();
    mockInvoke.mockReset();
    applyCalls.length = 0;
    showToastMock.mockClear();
    localStorage.setItem("photrez.rustPixels", "1");
  });
  afterEach(() => {
    localStorage.removeItem("photrez.rustPixels");
  });

  it("flag ON routes the fill through Rust; flag OFF keeps the legacy bitmap arm", async () => {
    const on = makeFakes();
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") {
        return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }];
      }
      if (cmd === "rust_pixels_write_region") {
        return {
          before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }],
          after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }],
          epoch: 1,
          version: 1,
        };
      }
      return undefined;
    });

    expect(fillActiveLayerWithColor(on.engine, on.history, on.renderer, "#ff0000")).toBe(true);
    await vi.waitFor(
      () => expect(mockInvoke.mock.calls.filter(([c]) => c === "rust_pixels_write_region")).toHaveLength(1),
      { timeout: 2000 },
    );
    // The two arms are mutually exclusive on observed state: the Rust arm never
    // replaces the bitmap and its single history step carries the tile memento.
    expect(on.engine.setLayerImageBitmap).not.toHaveBeenCalled();
    expect(on.commit).toHaveBeenCalledTimes(1);
    expect(on.commit.mock.calls[0][1]).toBe("Fill Layer");
    expect(on.commit.mock.calls[0][2]).toBeTruthy();

    localStorage.removeItem("photrez.rustPixels");
    const off = makeFakes();
    mockInvoke.mockClear();
    mockInvoke.mockImplementation(async () => undefined);

    expect(fillActiveLayerWithColor(off.engine, off.history, off.renderer, "#ff0000")).toBe(true);
    await vi.waitFor(() => expect(off.commit).toHaveBeenCalledTimes(1), { timeout: 2000 });
    expect(mockInvoke.mock.calls.some(([c]) => c === "rust_pixels_write_region")).toBe(false);
    expect(off.engine.setLayerImageBitmap).toHaveBeenCalledTimes(1);
    expect(off.commit.mock.calls[0][2]).toBeUndefined();
  });

  // Flag ON with no surface must fail visibly instead of quietly writing through
  // the legacy arm while the flag says Rust is authoritative.
  it("flag ON with no paint surface: visible error, zero write_region, no silent legacy write", () => {
    const { commit, engine, renderer, history } = makeFakes({ surfaceNull: true });
    const ok = fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    expect(ok).toBe(true);
    expect(showToastMock).toHaveBeenCalledWith("Rust pixel surface not ready", "warn");
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(engine.setLayerImageBitmap).not.toHaveBeenCalled();
    expect(commit).not.toHaveBeenCalled();
  });

  // Moved from the main describe (former lines 335-343): it pins the flag-OFF
  // legacy arm for a layer with no paint surface.
  it("shape/text layer (no PaintTileSurface) with the flag OFF → legacy path, NOT rust write_region, setLayerImageBitmap called", () => {
    localStorage.setItem("photrez.rustPixels", "0");
    const { commit, engine, renderer, history } = makeFakes({ surfaceNull: true });
    const ok = fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    expect(ok).toBe(true);
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(engine.setLayerImageBitmap).toHaveBeenCalled();
    expect(commit).toHaveBeenCalledTimes(1);
  });

  // Flag-OFF arm characterization: only valid while the photrez.rustPixels gate
  // exists. Retiring the gate removes the flag-OFF arm from production, so this
  // case fails there and is deleted together with this block.
  it("legacy behavior unchanged when flag OFF (setLayerImageBitmap, no rust write_region)", () => {
    localStorage.setItem("photrez.rustPixels", "0");
    const { commit, engine, renderer, history } = makeFakes();
    const ok = fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    expect(ok).toBe(true);
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(engine.setLayerImageBitmap).toHaveBeenCalled();
    expect(commit).toHaveBeenCalledTimes(1);
  });
});
