// Pins the epoch-keep invariant of the deferred brush commit: the engine's
// cached PaintTileSurface must stay FRESH across the commit's model bitmap
// sync, otherwise every stroke's next commit reads surface.pixelEpoch (a
// fresh 0) against the store epoch and pays a full rust_pixels_snapshot_layer
// rehydrate (~3 s).
//
// WHAT IT PINS
// - The deferred c4 commit passes preservePaintSurface to setLayerImageBitmap,
//   so DocumentEngine keeps the same paint-surface instance instead of
//   dropping it (document.ts paintSurfaces.delete).
// - Observable effect: zero rust_pixels_snapshot_layer calls across two
//   strokes on a current surface.
//
// Mock fidelity: the engine mock mirrors the REAL DocumentEngine surface
// lifecycle (cache keyed by layer id, drop on bitmap replace unless the
// caller preserves). A surface started with pixelEpoch 0 stands in for a
// fresh PaintTileSurface(...) the way PaintTileSurface initialises it.

import { afterAll, beforeAll, beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { decodeRustBytes, readPixelSeedCall } from "@/lib/protocol/pixelSeedCall";

vi.mock("@tauri-apps/api/core", () => ({ invoke: hoist.invoke }));

import { useBrushOverlay, flushC4Commits } from "../useBrushOverlay";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import * as brushToolStateModule from "../brushToolState";
import * as docModule from "@/engine/document";

if (typeof (globalThis as any).createImageBitmap === "undefined") {
  (globalThis as any).createImageBitmap = async (source: any) => {
    const w = source?.width ?? 100;
    const h = source?.height ?? 80;
    const c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    (c as any).close = () => {};
    return c as unknown as ImageBitmap;
  };
}
if (typeof (globalThis as any).ImageData === "undefined") {
  (globalThis as any).ImageData = class {
    data: any;
    width: number;
    height: number;
    constructor(data: any, w: number, h: number) {
      this.data = data;
      this.width = w;
      this.height = h;
    }
  };
}
if (typeof (globalThis as any).OffscreenCanvas === "undefined") {
  (globalThis as any).OffscreenCanvas = class {
    width: number;
    height: number;
    constructor(w: number, h: number) {
      this.width = w;
      this.height = h;
    }
    getContext() {
      return {
        drawImage: vi.fn(),
        clearRect: vi.fn(),
        save: vi.fn(),
        restore: vi.fn(),
        putImageData: vi.fn(),
        createImageData: vi.fn((w: number, h: number) => ({ data: new Uint8ClampedArray((w || 1) * (h || 1) * 4), width: w, height: h })),
        getImageData: vi.fn((x: number, y: number, w: number, h: number) => ({ data: new Uint8ClampedArray((w || 1) * (h || 1) * 4), width: w, height: h })),
        globalCompositeOperation: "source-over",
        globalAlpha: 1,
      };
    }
    transferToImageBitmap() {
      return document.createElement("canvas");
    }
  };
}

function makeFreshSurface() {
  return {
    context: {
      drawImage: vi.fn(),
      clearRect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      putImageData: vi.fn(),
      globalCompositeOperation: "source-over",
      globalAlpha: 1,
    },
    // Mirrors `pixelEpoch: number = 0` on a freshly constructed surface.
    pixelEpoch: 0 as number,
    pixelVersion: 0 as number,
    snapshotTile: vi.fn((t: { x: number; y: number; w: number; h: number }) => ({
      tx: Math.floor(t.x / 256),
      ty: Math.floor(t.y / 256),
      value: { width: t.w, height: t.h, data: new Uint8ClampedArray(t.w * t.h * 4).fill(7) },
    })),
    restoreTile: vi.fn(),
    readRect: vi.fn((_x: number, _y: number, w: number, h: number) => ({
      width: w,
      height: h,
      data: new Uint8ClampedArray(w * h * 4),
    })),
    toImageBitmap: vi.fn(async () => {
      const c = document.createElement("canvas");
      c.width = 512;
      c.height = 512;
      return c as unknown as ImageBitmap;
    }),
  };
}

type SimLayer = { w: number; h: number; pixels: number[]; epoch: number; version: number };

function makeSim() {
  const store = new Map<string, SimLayer>();
  const calls: { cmd: string; args: any }[] = [];
  const key = (docId: string, layerId: string) => `${docId}|${layerId}`;
  const invoke = async (cmd: string, args: any): Promise<any> => {
    args = decodeRustBytes({ ...args, ...(cmd === "rust_pixels_init" ? readPixelSeedCall(cmd, args)! : {}) });
    calls.push({ cmd, args });
    const k = key(args.docId, args.layerId);
    if (cmd === "rust_pixels_get_epoch") {
      const s = store.get(k);
      if (!s) throw new Error("no layer");
      return s.epoch;
    }
    if (cmd === "rust_pixels_snapshot_layer") {
      const s = store.get(k);
      if (!s) return [];
      return [{ x: 0, y: 0, w: s.w, h: s.h, data: s.pixels.slice() }];
    }
    if (cmd === "rust_pixels_init") {
      store.set(k, { w: args.width, h: args.height, pixels: (args.bytes as number[]).slice(), epoch: 0, version: 0 });
      return;
    }
    if (cmd === "rust_pixels_write_region") {
      args = decodeRustBytes(args);
      const layer = store.get(k);
      if (!layer) throw new Error("no layer");
      const x = args.x as number, y = args.y as number, rw = args.w as number, rh = args.h as number;
      const rgba = args.rgba as number[];
      const afterPx = layer.pixels.slice();
      for (let row = 0; row < rh; row++) {
        const dst = ((y + row) * layer.w + x) * 4;
        const src = row * rw * 4;
        for (let i = 0; i < rw * 4; i++) afterPx[dst + i] = rgba[src + i];
      }
      layer.pixels = afterPx;
      layer.epoch += 1;
      layer.version += 1;
      return {
        after: [{ x, y, w: rw, h: rh, data: afterPx.slice((y * layer.w + x) * 4, (y * layer.w + x) * 4 + rw * rh * 4) }],
        epoch: layer.epoch,
        version: layer.version,
      };
    }
    throw new Error("unknown cmd " + cmd);
  };
  return { invoke, calls };
}

const hoist = vi.hoisted(() => {
  let sim: ReturnType<typeof makeSim> | null = null;
  const invoke = (cmd: string, args: any, options?: any) => sim!.invoke(cmd, args);
  return { invoke, setSim: (s: ReturnType<typeof makeSim>) => { sim = s; }, getSim: () => sim! };
});

describe("brush commit keeps the paint surface fresh across the bitmap sync", () => {
  beforeAll(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({ confirm: vi.fn() } as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
  });
  afterAll(() => {
    vi.restoreAllMocks();
  });
  beforeEach(() => {
    vi.spyOn(brushToolStateModule, "getPaintToolBlockReason").mockImplementation(() => null);
    vi.spyOn(docModule, "isFacadeOwnedLayer").mockImplementation(() => false);
    hoist.setSim(makeSim());
  });
  afterEach(() => {
    localStorage.clear();
  });

  it("two strokes on a current surface take zero full-layer reads and one surface instance", async () => {
    const layer: any = {
      id: "layer-1",
      name: "L",
      visible: true,
      locked: false,
      lockTransparency: false,
      hasAdjustments: false,
      width: 512,
      height: 512,
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      imageBitmap: document.createElement("canvas") as unknown as ImageBitmap,
    };
    const surfaces = new Map<string, any>();
    // Faithful mirror of DocumentEngine's surface lifecycle: cached per layer,
    // dropped on bitmap replace unless the caller passes preservePaintSurface.
    const engine: any = {
      getActiveLayerId: () => layer.id,
      getLayer: () => layer,
      snapshot: vi.fn(() => ({ model: true })),
      rustHoldsLayer: () => true,
      getPaintSurface: (id: string) => {
        const cached = surfaces.get(id);
        if (cached) return cached;
        const fresh = makeFreshSurface();
        surfaces.set(id, fresh);
        return fresh;
      },
      setLayerImageBitmap: vi.fn((id: string, b: unknown, opts?: { preservePaintSurface?: boolean }) => {
        layer.imageBitmap = b as ImageBitmap;
        if (!opts?.preservePaintSurface) surfaces.delete(id);
      }),
    };
    const history: any = {
      commit: vi.fn(),
      setLastPaintCoords: vi.fn(),
      getLastPaintCoords: vi.fn(() => null),
    };
    const doc = { id: "doc-test" };
    mockUseEditor({
      workspace: {
        getActiveEngine: () => engine,
        getActiveHistory: () => history,
        getActiveDocumentId: () => doc.id,
      },
      renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
      scheduler: { requestRender: vi.fn() },
      fgColor: () => "#ff0000",
      bgColor: () => "#ffffff",
      docWidth: () => 512,
      docHeight: () => 512,
      activeTool: () => "brush",
      brushSize: () => 20,
      brushHardness: () => 1,
      eraserSize: () => 20,
      eraserHardness: () => 1,
    });
    const canvas = document.createElement("canvas");
    canvas.width = 512;
    canvas.height = 512;
    const overlay = useBrushOverlay();
    overlay.setOverlayCanvasRef(canvas);
    const settings = { size: 20, hardness: 1, opacity: 1, flow: 1, smoothing: 0.5 };

    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history as any, "layer-1", false);
    await flushC4Commits();
    const surfaceAfterFirst = engine.getPaintSurface("layer-1");

    overlay.onPaintStroke([{ x: 60, y: 60 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history as any, "layer-1", false);
    await flushC4Commits();

    const sim = hoist.getSim();
    const snapshotCount = sim.calls.filter((c) => c.cmd === "rust_pixels_snapshot_layer").length;
    expect(sim.calls.filter((c) => c.cmd === "rust_pixels_write_region").length).toBe(2);
    // No commit may pay the full-layer rehydrate round trip.
    expect(snapshotCount).toBe(0);
    // Same surface instance across both commits: the store epoch it tracks
    // (res.epoch) is exactly what the next commit's epoch probe compares
    // against, so the guard skips the rehydrate.
    expect(engine.getPaintSurface("layer-1")).toBe(surfaceAfterFirst);
    expect(engine.setLayerImageBitmap).toHaveBeenCalledWith(
      "layer-1",
      expect.anything(),
      expect.objectContaining({ preservePaintSurface: true }),
    );
  });

  it("DocumentEngine keeps a preserved surface and drops a plain one", () => {
    const { DocumentEngine } = docModule;
    const engine = new DocumentEngine("doc-unit", "un", 512, 512);
    const layer = engine.addLayer("L", 512, 512);
    engine.setLayerImageBitmap(layer.id, { width: 512, height: 512, close: () => {} } as unknown as ImageBitmap);

    const dropped = engine.getPaintSurface(layer.id);
    engine.setLayerImageBitmap(layer.id, { width: 512, height: 512, close: () => {} } as unknown as ImageBitmap);
    expect(engine.getPaintSurface(layer.id)).not.toBe(dropped);

    const kept = engine.getPaintSurface(layer.id);
    engine.setLayerImageBitmap(layer.id, { width: 512, height: 512, close: () => {} } as unknown as ImageBitmap, { preservePaintSurface: true });
    expect(engine.getPaintSurface(layer.id)).toBe(kept);
  });
});
