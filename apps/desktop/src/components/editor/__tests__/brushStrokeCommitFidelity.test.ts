// Scratch-composite faithful regression (deferred Rust commit: the stroke must not be dropped)
// stroke). Drives the REAL production commit path via the hook, with
// @tauri-apps/api/core invoke mocked by an in-test Rust store emulator.
//
// Unlike c4MultiStroke.test.ts (which zero-mocks the surface and locks the
// defect in as "passing"), this test models the REAL coupling at
// paintTileSurface.ts: the surface context's drawImage/putImageData write into
// a backing RGBA buffer that readRect reads from. The scratch (cachedTileScratch)
// is seeded with a known non-background dab pattern so we can assert the exact
// bytes that reach rust_pixels_write_region.
//
// RED/GREEN proof:
//   - On the UNFIXED code, c4CoreCommit rehydrates (absolute putImageData of the
//     PRE-stroke canonical) and readRect returns all-zeros -> the rgba sent to
//     rust_pixels_write_region lacks the dab -> assertion FAILS.
//   - After the fix, c4CoreCommit composites the per-job scratch snapshot onto
//     the surface (after rehydrate, before readRect) -> rgba carries the dab ->
//     assertion PASSES.
//
// No new dependencies: the backing buffer is implemented IN THE MOCK.

import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { useBrushOverlay, flushC4Commits } from "../useBrushOverlay";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import * as brushToolStateModule from "../brushToolState";
import * as docModule from "@/engine/document";

// -- seed control (the scratch singleton is simulated by a mutable seed) --
const SEED_A: [number, number, number, number] = [123, 45, 67, 255];
const SEED_B: [number, number, number, number] = [200, 100, 30, 255];
let currentSeed: number[] = SEED_A;

// -- jsdom polyfills --
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

// OffscreenCanvas polyfill with a per-instance pixel buffer so putImageData and
// getImageData round-trip faithfully. A canvas reports the LIVE seed until it has
// been written via putImageData (the `_dirty` flag), after which it reports its
// own stored buffer. This models the two real canvases used by the fix:
//   - cachedTileScratch (the shared brush scratch) is seeded with the live dab
//     pattern at enqueue time (production draws the dabs with drawImage, which the
//     mock no-ops, so we report the live seed as the "rasterized dabs").
//   - the temporary snapshot canvas in c4CoreCommit receives the per-job snapshot
//     via putImageData and is then read back by the deferred composite.
let savedOffscreen: any;
if (typeof (globalThis as any).OffscreenCanvas === "undefined") {
  savedOffscreen = undefined;
  (globalThis as any).OffscreenCanvas = class {
    width: number;
    height: number;
    private _data: Uint8ClampedArray;
    private _dirty = false;
    constructor(w: number, h: number) {
      this.width = w;
      this.height = h;
      this._data = new Uint8ClampedArray((w || 1) * (h || 1) * 4);
    }
    getContext() {
      const self = this;
      return {
        drawImage: vi.fn(),
        clearRect: vi.fn(),
        save: vi.fn(),
        restore: vi.fn(),
        putImageData: (img: { data: ArrayLike<number>; width: number; height: number }, dx: number, dy: number) => {
          self._dirty = true;
          const w = img.width, h = img.height;
          for (let y = 0; y < h; y++) {
            for (let x = 0; x < w; x++) {
              const si = (y * w + x) * 4;
              const di = ((dy + y) * self.width + (dx + x)) * 4;
              self._data[di] = img.data[si];
              self._data[di + 1] = img.data[si + 1];
              self._data[di + 2] = img.data[si + 2];
              self._data[di + 3] = img.data[si + 3];
            }
          }
        },
        createImageData: vi.fn((w: number, h: number) => ({ data: new Uint8ClampedArray((w || 1) * (h || 1) * 4), width: w, height: h })),
        getImageData: (_x: number, _y: number, w: number, h: number) => {
          const out = new Uint8ClampedArray((w || 1) * (h || 1) * 4);
          if (self._dirty) {
            out.set(self._data.subarray(0, out.length));
          } else {
            for (let i = 0; i < out.length; i += 4) {
              out[i] = currentSeed[0];
              out[i + 1] = currentSeed[1];
              out[i + 2] = currentSeed[2];
              out[i + 3] = currentSeed[3];
            }
          }
          return { data: out, width: w, height: h };
        },
        globalCompositeOperation: "source-over",
        globalAlpha: 1,
      };
    }
    transferToImageBitmap() {
      return document.createElement("canvas");
    }
  };
}

// Overlay HTMLCanvasElement 2d context is a no-op (jsdom cannot rasterize).
let savedGetContext: any;
const realGetContext = (globalThis as any).HTMLCanvasElement?.prototype?.getContext;
savedGetContext = realGetContext;
(globalThis as any).HTMLCanvasElement.prototype.getContext = function (type: string, ...rest: any[]) {
  if (type === "2d") {
    return {
      drawImage: () => {},
      clearRect: () => {},
      save: () => {},
      restore: () => {},
      putImageData: () => {},
      getImageData: (_x: unknown, _y: unknown, w: unknown, h: unknown) => ({ data: new Uint8ClampedArray((w as number) * (h as number) * 4), width: w, height: h }),
      createImageData: (w: unknown, h: unknown) => ({ data: new Uint8ClampedArray((w as number) * (h as number) * 4), width: w, height: h }),
      globalCompositeOperation: "source-over",
      globalAlpha: 1,
    };
  }
  return realGetContext.apply(this, [type, ...rest]);
};

// -- in-test Rust emulator (mirrors crates/core/src/pixel_store.rs semantics) --
type WireTile = { x: number; y: number; w: number; h: number; data: number[] };
type SimLayer = { w: number; h: number; pixels: number[]; undo: any[]; redo: any[]; epoch: number; version: number };

function makeSim(opts?: { failCommitOnCall?: number }) {
  const store = new Map<string, SimLayer>();
  const calls: { cmd: string; args: any }[] = [];
  const key = (docId: string, layerId: string) => `${docId}|${layerId}`;
  let commitCount = 0;
  let initCount = 0;
  const failCommitOnCall = opts?.failCommitOnCall ?? -1;

  const invoke = async (cmd: string, args: any): Promise<any> => {
    calls.push({ cmd, args });
    if (cmd === "rust_pixels_open_document") return;
    if (cmd === "rust_pixels_close_document") {
      for (const k of [...store.keys()]) if (k.startsWith(args.docId + "|")) store.delete(k);
      return;
    }
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
    const s = store.get(k);
    if (cmd === "rust_pixels_init") {
      store.set(k, {
        w: args.width,
        h: args.height,
        pixels: (args.bytes as number[]).slice(),
        undo: [],
        redo: [],
        epoch: 0,
        version: 0,
      });
      initCount += 1;
      return;
    }
    // Deferred dirty-region write: region replace, one history step.
    if (cmd === "rust_pixels_write_region") {
      commitCount += 1;
      if (commitCount === failCommitOnCall) throw new Error("simulated commit failure");
      let layer = s;
      if (!layer) throw new Error("no layer");
      const x = args.x as number, y = args.y as number, rw = args.w as number, rh = args.h as number;
      const rgba = args.rgba as number[];
      const beforePx = layer.pixels.slice();
      const afterPx = layer.pixels.slice();
      for (let row = 0; row < rh; row++) {
        const dst = ((y + row) * layer.w + x) * 4;
        const src = row * rw * 4;
        for (let i = 0; i < rw * 4; i++) afterPx[dst + i] = rgba[src + i];
      }
      layer.pixels = afterPx;
      layer.undo.push({ before: beforePx, after: afterPx });
      layer.redo = [];
      layer.epoch += 1;
      layer.version += 1;
      return { before: [{ x, y, w: rw, h: rh, data: beforePx.slice((y * layer.w + x) * 4, (y * layer.w + x) * 4 + rw * rh * 4) }], after: [{ x, y, w: rw, h: rh, data: afterPx.slice((y * layer.w + x) * 4, (y * layer.w + x) * 4 + rw * rh * 4) }], epoch: layer.epoch, version: layer.version };
    }
    throw new Error("unknown cmd " + cmd);
  };

  return { invoke, store, calls, get initCount() { return initCount; }, get commitCount() { return commitCount; } };
}

const hoist = vi.hoisted(() => {
  let sim: ReturnType<typeof makeSim> | null = null;
  const invoke = (cmd: string, args: any) => sim!.invoke(cmd, args);
  return { invoke, setSim: (s: ReturnType<typeof makeSim>) => { sim = s; }, getSim: () => sim! };
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: hoist.invoke }));

// -- faithful surface (backing buffer + drawImage/putImageData/readRect coupling) --
const W = 512, H = 512;
function makeSurface() {
  const backing = new Uint8ClampedArray(W * H * 4); // all zeros = pre-stroke background
  const snapshotted: unknown[] = [];
  const restored: unknown[] = [];
  const context: any = {
    globalCompositeOperation: "source-over",
    globalAlpha: 1,
    clearRect: () => {},
    save: () => {},
    restore: () => {},
    // getImageData is unused by the surface (the brush uses cachedTileScratch);
    // kept for completeness of the 2d context shape.
    getImageData: (_x: number, _y: number, w: number, h: number) => ({ data: new Uint8ClampedArray(w * h * 4), width: w, height: h }),
    createImageData: (w: number, h: number) => ({ data: new Uint8ClampedArray(w * h * 4), width: w, height: h }),
    // Absolute replace (used by rehydrate + applyRustTilesToSurface).
    putImageData: (img: { data: ArrayLike<number>; width: number; height: number }, dx: number, dy: number) => {
      const w = img.width, h = img.height;
      for (let y = 0; y < h; y++) {
        for (let x = 0; x < w; x++) {
          const si = (y * w + x) * 4;
          const di = ((dy + y) * W + (dx + x)) * 4;
          backing[di] = img.data[si];
          backing[di + 1] = img.data[si + 1];
          backing[di + 2] = img.data[si + 2];
          backing[di + 3] = img.data[si + 3];
        }
      }
    },
    // Source-over composite (mirrors the synchronous composite plus the deferred Rust composite).
    drawImage: (src: any, _sx: number, _sy: number, sw: number, sh: number, dx: number, dy: number, dw: number, dh: number) => {
      const sctx = src.getContext("2d");
      const img = sctx.getImageData(0, 0, sw, sh);
      for (let y = 0; y < dh; y++) {
        for (let x = 0; x < dw; x++) {
          const si = (y * dw + x) * 4;
          const di = ((dy + y) * W + (dx + x)) * 4;
          const sa = img.data[si + 3] / 255;
          for (let c = 0; c < 3; c++) backing[di + c] = img.data[si + c] * sa + backing[di + c] * (1 - sa);
          backing[di + 3] = img.data[si + 3] + backing[di + 3] * (1 - sa);
        }
      }
    },
  };
  const surface: any = {
    context,
    pixelEpoch: undefined as number | undefined,
    pixelVersion: undefined as number | undefined,
    snapshotted,
    restored,
    snapshotTile: vi.fn((t: { x: number; y: number; w: number; h: number }) => {
      const patch = { tx: t.x / 256, ty: t.y / 256, value: { width: t.w, height: t.h, data: new Uint8ClampedArray(t.w * t.h * 4).fill(7) } };
      snapshotted.push(patch);
      return patch;
    }),
    restoreTile: vi.fn((p: unknown) => restored.push(p)),
    readRect: (x: number, y: number, w: number, h: number) => {
      const data = new Uint8ClampedArray(w * h * 4);
      for (let yy = 0; yy < h; yy++) {
        for (let xx = 0; xx < w; xx++) {
          const si = ((y + yy) * W + (x + xx)) * 4;
          const di = (yy * w + xx) * 4;
          data[di] = backing[si];
          data[di + 1] = backing[si + 1];
          data[di + 2] = backing[si + 2];
          data[di + 3] = backing[si + 3];
        }
      }
      return { width: w, height: h, data };
    },
  };
  return surface;
}

// -- harness (mirrors c4MultiStroke.test.ts) --
const settings = { size: 20, hardness: 1, opacity: 1, flow: 1, smoothing: 0.5 };
const DOC = "doc-test";
const LAYER = "layer-1";

function makeHarness(surface: any, layerId = LAYER) {
  const layer = {
    id: layerId,
    name: "L",
    visible: true,
    locked: false,
    lockTransparency: false,
    isBackground: false,
    hasAdjustments: false,
    basicAdjustment: undefined,
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    width: 512,
    height: 512,
    imageBitmap: document.createElement("canvas") as unknown as ImageBitmap,
  };
  const history = {
    entries: [] as any[],
    commit: vi.fn((p: any) => { history.entries.push(p); }),
    setLastPaintCoords: vi.fn(),
    getLastPaintCoords: vi.fn(() => null),
  };
  const commit = history.commit;
  const doc = { id: DOC };
  const engine: any = {
    getActiveLayerId: () => layer.id,
    getLayer: () => layer,
    snapshot: vi.fn(() => ({ model: true })),
    setLayerImageBitmap: vi.fn(),
    getPaintSurface: () => surface,
  };
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
  return { overlay, layer, engine, history, commit, surface };
}

function regionIsSolid(rgba: Uint8ClampedArray, seed: number[]): boolean {
  for (let i = 0; i < rgba.length; i += 4) {
    if (rgba[i] !== seed[0] || rgba[i + 1] !== seed[1] || rgba[i + 2] !== seed[2] || rgba[i + 3] !== seed[3]) return false;
  }
  return true;
}

describe("Scratch composite faithful dab-pixel proof", () => {
  beforeAll(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({ confirm: vi.fn() } as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
  });
  afterAll(() => {
    vi.restoreAllMocks();
    if (savedOffscreen === undefined) delete (globalThis as any).OffscreenCanvas;
    else (globalThis as any).OffscreenCanvas = savedOffscreen;
    (globalThis as any).HTMLCanvasElement.prototype.getContext = savedGetContext;
  });
  beforeEach(() => {
    vi.spyOn(brushToolStateModule, "getPaintToolBlockReason").mockImplementation((l: any, e: any) => null);
    vi.spyOn(docModule, "isFacadeOwnedLayer").mockImplementation((id: string) => false);
    hoist.setSim(makeSim());
    currentSeed = SEED_A;
    localStorage.setItem("photrez.rustPixels", "1");
    localStorage.removeItem("photrez.canonicalCommit");
  });
  afterEach(() => {
    localStorage.clear();
  });

  it("RED(unfixed)/GREEN(fixed): rgba to rust_pixels_write_region contains the dab", async () => {
    const surface = makeSurface();
    const { overlay, engine, history } = makeHarness(surface);
    const sim = hoist.getSim();

    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history as any, LAYER, false);
    await flushC4Commits();

    const wr = sim.calls.find((c) => c.cmd === "rust_pixels_write_region");
    expect(wr, "a write_region was issued").toBeTruthy();
    const rgba = new Uint8ClampedArray(wr!.args.rgba as number[]);
    // Whole dirty region must be the seeded dab. On unfixed code this is all
    // zeros (rehydrate clobbered the surface, no composite happened).
    expect(regionIsSolid(rgba, SEED_A)).toBe(true);
  });

  it("aliasing: each deferred commit uses its OWN synchronous scratch snapshot", async () => {
    const surface = makeSurface();
    const { overlay, engine, history } = makeHarness(surface);
    const sim = hoist.getSim();

    // Stroke 1 commits with the shared scratch holding SEED_A.
    currentSeed = SEED_A;
    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history as any, LAYER, false);

    // Simulate stroke 2 rasterizing into the SHARED module-level scratch
    // (cachedTileScratch is reused/resized across strokes). If the deferred
    // commit read the shared scratch at flush time it would now see SEED_B.
    currentSeed = SEED_B;
    overlay.onPaintStroke([{ x: 60, y: 60 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history as any, LAYER, false);

    await flushC4Commits();

    const wrs = sim.calls.filter((c) => c.cmd === "rust_pixels_write_region");
    expect(wrs.length).toBe(2);
    const rgba1 = new Uint8ClampedArray(wrs[0].args.rgba as number[]);
    const rgba2 = new Uint8ClampedArray(wrs[1].args.rgba as number[]);
    // job 1 must carry SEED_A, job 2 must carry SEED_B (each its own snapshot).
    expect(regionIsSolid(rgba1, SEED_A)).toBe(true);
    expect(regionIsSolid(rgba2, SEED_B)).toBe(true);
  });
});
