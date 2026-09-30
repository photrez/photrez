/**
 * Pixel-undo must leave `layer.imageBitmap` truthful.
 *
 * Defect under test: on the pixel-undo tiles fast path in
 * useEditorCommands.restoreHistorySnapshot, the authoritative Rust tiles are
 * uploaded (surface + GPU) but `layer.imageBitmap` is never re-derived from
 * them, and the live layer's `bitmapEpoch` is then stamped with the POST-undo
 * Rust epoch. `DocumentEngine.ensureBitmapCurrent` short-circuits on a matching
 * epoch, so the stamp is a lie that disables the repair forever: export/save
 * read `layer.imageBitmap` and ship the un-undone pixels, and a document tab
 * switch re-uploads the undone stroke.
 *
 * The snapshot cannot repair it: the brush commit writes the post-stroke
 * surface into the model bitmap BEFORE `history.commit` (useBrushOverlay), so
 * the popped snapshot holds the same post-stroke object and
 * `snapBitmap !== liveBitmap` is false.
 *
 * Chain driven here (real hooks, real DocumentEngine, real CommandHistory, real
 * useEditorCommands -> restoreHistorySnapshot("undo")):
 *   onPaintStroke -> commitBrushStroke -> commands.undo()
 * with the paint entry's tile memento round-tripping through the real history.
 *
 * Separate file (not appended to brushStrokeCommitOrdering.test.ts) because that
 * file is at the 1000-line ceiling.
 */
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { useBrushOverlay, flushC4Commits } from "../useBrushOverlay";
import { useEditorCommands } from "../useEditorCommands";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";

const DOC = "doc-undo";
const LAYER = "layer-1";
const SIZE = 64;

// -- byte-faithful jsdom canvas stand-ins --
// Every one of these must really carry bytes: the assertions hash the model
// bitmap, and a mock that drops pixels would make a broken repair look green.
type Bytes = { width: number; height: number; data: Uint8ClampedArray; close: () => void };

function carrier(width: number, height: number, data: Uint8ClampedArray): ImageBitmap {
  // close() is a no-op on purpose: DocumentEngine.replaceLayerBitmap may close a
  // superseded raster, and a carrier that dropped its bytes on close would make
  // the pre-stroke hash below move under the test.
  return { width, height, data, close: () => {} } as unknown as ImageBitmap;
}

if (typeof (globalThis as any).ImageData === "undefined") {
  (globalThis as any).ImageData = class {
    data: Uint8ClampedArray;
    width: number;
    height: number;
    constructor(data: Uint8ClampedArray, w: number, h: number) {
      this.data = data;
      this.width = w;
      this.height = h;
    }
  };
}
if (typeof (globalThis as any).createImageBitmap === "undefined") {
  (globalThis as any).createImageBitmap = async () => carrier(SIZE, SIZE, new Uint8ClampedArray(SIZE * SIZE * 4));
}
if (typeof (globalThis as any).OffscreenCanvas === "undefined") {
  class Ctx2D {
    globalAlpha = 1;
    globalCompositeOperation = "source-over";
    constructor(private readonly c: { width: number; height: number; buf: Uint8ClampedArray }) {}
    createImageData(w: number, h: number) {
      return { data: new Uint8ClampedArray(Math.max(0, w) * Math.max(0, h) * 4), width: w, height: h };
    }
    getImageData(sx: number, sy: number, sw: number, sh: number) {
      const out = new Uint8ClampedArray(Math.max(0, sw) * Math.max(0, sh) * 4);
      for (let y = 0; y < sh; y++) {
        for (let x = 0; x < sw; x++) {
          const px = sx + x, py = sy + y;
          if (px < 0 || py < 0 || px >= this.c.width || py >= this.c.height) continue;
          const s = (py * this.c.width + px) * 4, d = (y * sw + x) * 4;
          out[d] = this.c.buf[s];
          out[d + 1] = this.c.buf[s + 1];
          out[d + 2] = this.c.buf[s + 2];
          out[d + 3] = this.c.buf[s + 3];
        }
      }
      return new (globalThis as any).ImageData(out, sw, sh);
    }
    putImageData(img: { data: Uint8ClampedArray; width: number; height: number }, dx: number, dy: number): void {
      for (let y = 0; y < img.height; y++) {
        for (let x = 0; x < img.width; x++) {
          const px = dx + x, py = dy + y;
          if (px < 0 || py < 0 || px >= this.c.width || py >= this.c.height) continue;
          const s = (y * img.width + x) * 4, d = (py * this.c.width + px) * 4;
          this.c.buf[d] = img.data[s];
          this.c.buf[d + 1] = img.data[s + 1];
          this.c.buf[d + 2] = img.data[s + 2];
          this.c.buf[d + 3] = img.data[s + 3];
        }
      }
    }
    clearRect(x: number, y: number, w: number, h: number): void {
      this.putImageData(this.createImageData(Math.max(0, Math.ceil(w)), Math.max(0, Math.ceil(h))), x, y);
    }
    save(): void {}
    restore(): void {}
    drawImage(src: unknown, ...args: number[]): void {
      const s = src as { width?: number; height?: number; buf?: Uint8ClampedArray };
      if (!s?.buf) return;
      let sx = 0, sy = 0, sw = s.width!, sh = s.height!, dx = 0, dy = 0, dw = s.width!, dh = s.height!;
      if (args.length === 4) [dx, dy, dw, dh] = args;
      else if (args.length === 8) [sx, sy, sw, sh, dx, dy, dw, dh] = args;
      else if (args.length !== 2) return;
      if (dw <= 0 || dh <= 0 || sw <= 0 || sh <= 0) return;
      for (let py = Math.max(0, Math.round(dy)); py < Math.min(this.c.height, Math.round(dy + dh)); py++) {
        for (let px = Math.max(0, Math.round(dx)); px < Math.min(this.c.width, Math.round(dx + dw)); px++) {
          const fx = sx + Math.floor(((px - dx) * sw) / dw);
          const fy = sy + Math.floor(((py - dy) * sh) / dh);
          if (fx < 0 || fy < 0 || fx >= s.width! || fy >= s.height!) continue;
          const sa = s.buf[(fy * s.width! + fx) * 4 + 3] / 255;
          if (sa <= 0) continue;
          const d = (py * this.c.width + px) * 4;
          const da = this.c.buf[d + 3] / 255;
          const outA = sa + da * (1 - sa);
          if (outA <= 0) continue;
          this.c.buf[d] = Math.round((s.buf[(fy * s.width! + fx) * 4] * sa + this.c.buf[d] * da * (1 - sa)) / outA);
          this.c.buf[d + 1] = Math.round((s.buf[(fy * s.width! + fx) * 4 + 1] * sa + this.c.buf[d + 1] * da * (1 - sa)) / outA);
          this.c.buf[d + 2] = Math.round((s.buf[(fy * s.width! + fx) * 4 + 2] * sa + this.c.buf[d + 2] * da * (1 - sa)) / outA);
          this.c.buf[d + 3] = Math.round(outA * 255);
        }
      }
    }
  }
  (globalThis as any).OffscreenCanvas = class {
    width: number;
    height: number;
    buf: Uint8ClampedArray;
    private ctx: Ctx2D | null = null;
    constructor(w: number, h: number) {
      this.width = w;
      this.height = h;
      this.buf = new Uint8ClampedArray(Math.max(0, w) * Math.max(0, h) * 4);
    }
    getContext() {
      if (!this.ctx) this.ctx = new Ctx2D(this);
      return this.ctx;
    }
    transferToImageBitmap() {
      return carrier(this.width, this.height, this.buf);
    }
  };
}

// jsdom's 2d context cannot take the polyfilled OffscreenCanvas the brush
// scratch composites from.
const realGetContext = (globalThis as any).HTMLCanvasElement.prototype.getContext;
(globalThis as any).HTMLCanvasElement.prototype.getContext = function (type: string, ...rest: any[]) {
  if (type === "2d") {
    return {
      drawImage: () => {}, clearRect: () => {}, save: () => {}, restore: () => {},
      putImageData: () => {},
      getImageData: (_x: unknown, _y: unknown, w: unknown, h: unknown) => ({ data: new Uint8ClampedArray((w as number) * (h as number) * 4), width: w, height: h }),
      createImageData: (w: unknown, h: unknown) => ({ data: new Uint8ClampedArray((w as number) * (h as number) * 4), width: w, height: h }),
      globalCompositeOperation: "source-over", globalAlpha: 1,
    };
  }
  return realGetContext.apply(this, [type, ...rest]);
};

// -- in-test Rust canonical store --
type SimLayer = { w: number; h: number; pixels: Uint8ClampedArray; undo: any[]; redo: any[]; epoch: number; version: number };

function makeSim() {
  const store = new Map<string, SimLayer>();
  const invoke = async (cmd: string, args: any): Promise<any> => {
    if (cmd === "rust_pixels_open_document" || cmd === "rust_pixels_close_document") return;
    const k = `${args.docId}|${args.layerId}`;
    const s = store.get(k);
    if (cmd === "rust_pixels_get_epoch") {
      if (!s) throw new Error("no layer");
      return s.epoch;
    }
    if (cmd === "rust_pixels_snapshot_layer") {
      if (!s) return [];
      return [{ x: 0, y: 0, w: s.w, h: s.h, data: s.pixels.slice() }];
    }
    if (cmd === "rust_pixels_init") {
      store.set(k, { w: args.width, h: args.height, pixels: new Uint8ClampedArray(args.bytes as ArrayLike<number>), undo: [], redo: [], epoch: 0, version: 0 });
      return;
    }
    if (cmd === "rust_pixels_write_region") {
      if (!s) throw new Error("no layer");
      const beforePx = s.pixels.slice();
      const afterPx = s.pixels.slice();
      for (let row = 0; row < args.h; row++) {
        const dst = ((args.y + row) * s.w + args.x) * 4;
        for (let i = 0; i < args.w * 4; i++) afterPx[dst + i] = args.rgba[row * args.w * 4 + i];
      }
      s.pixels = afterPx;
      s.undo.push({ before: beforePx, after: afterPx });
      s.redo = [];
      s.epoch += 1;
      s.version += 1;
      const cut = (p: Uint8ClampedArray) => p.slice((args.y * s.w + args.x) * 4, (args.y * s.w + args.x) * 4 + args.w * args.h * 4);
      return { before: [{ x: args.x, y: args.y, w: args.w, h: args.h, data: cut(beforePx) }], after: [{ x: args.x, y: args.y, w: args.w, h: args.h, data: cut(afterPx) }], epoch: s.epoch, version: s.version };
    }
    if (cmd === "rust_pixels_undo" || cmd === "rust_pixels_redo") {
      if (!s) throw new Error("no layer");
      const undo = cmd === "rust_pixels_undo";
      const e = (undo ? s.undo : s.redo).pop();
      if (!e) return { tiles: [], epoch: s.epoch, version: s.version, layerId: args.layerId };
      s.pixels = (undo ? e.before : e.after).slice();
      (undo ? s.redo : s.undo).push(e);
      s.epoch += 1;
      s.version += 1;
      const src = undo ? e.before : e.after;
      return { tiles: [{ x: 0, y: 0, w: s.w, h: s.h, data: src.slice() }], epoch: s.epoch, version: s.version, layerId: args.layerId };
    }
    throw new Error("unknown cmd " + cmd);
  };
  return { invoke, store };
}

const hoist = vi.hoisted(() => {
  let sim: ReturnType<typeof makeSim> | null = null;
  return { invoke: (c: string, a: any) => sim!.invoke(c, a), setSim: (s: ReturnType<typeof makeSim>) => { sim = s; } };
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: hoist.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));

/** FNV-1a over the bitmap's bytes - order-sensitive, so a shifted raster fails. */
function hash(bmp: unknown): string {
  const data = (bmp as Bytes).data;
  let h = 0x811c9dc5;
  for (let i = 0; i < data.length; i++) {
    h ^= data[i];
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

/** Non-zero, non-uniform seed so a blank/zeroed re-derive can never match it. */
function seedPixels(): Uint8ClampedArray {
  const out = new Uint8ClampedArray(SIZE * SIZE * 4);
  for (let i = 0; i < SIZE * SIZE; i++) {
    out[i * 4] = (i * 7) & 0xff;
    out[i * 4 + 1] = (i * 13) & 0xff;
    out[i * 4 + 2] = (i * 29) & 0xff;
    out[i * 4 + 3] = 255;
  }
  return out;
}

const settings = { size: 20, hardness: 1, opacity: 1, flow: 1, smoothing: 0.5 };

describe("pixel undo leaves layer.imageBitmap truthful (rustPixels=1)", () => {
  beforeAll(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({ confirm: vi.fn() } as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
  });
  afterAll(() => vi.restoreAllMocks());
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem("photrez.rustPixels", "1");
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    localStorage.removeItem("photrez.canonicalCommit"); // C3 off -> C4 deferred commit path
  });
  afterEach(() => localStorage.clear());

  it("model bitmap hash returns to the pre-stroke hash and the epoch is never stamped over a stale bitmap", async () => {
    const sim = makeSim();
    hoist.setSim(sim);

    const engine = new DocumentEngine(DOC, "Undo", SIZE, SIZE);
    const history = new CommandHistory();
    const layer = engine.addLayer("L");
    engine.setActiveLayer(layer.id);

    // Seed Rust canonical + the model bitmap from the same non-zero pattern.
    const seed = seedPixels();
    sim.store.set(`${DOC}|${layer.id}`, { w: SIZE, h: SIZE, pixels: seed.slice(), undo: [], redo: [], epoch: 0, version: 0 });
    engine.setLayerImageBitmap(layer.id, carrier(SIZE, SIZE, seed.slice()));

    // Paint surface: a real byte-backed store. The C4 commit path rehydrates it
    // from Rust, composites dabs, and reads the dirty rect back out.
    const buf = new Uint8ClampedArray(SIZE * SIZE * 4);
    const surface = {
      context: {
        putImageData: (img: { data: Uint8ClampedArray; width: number; height: number }, x: number, y: number) => {
          for (let yy = 0; yy < img.height; yy++) {
            for (let xx = 0; xx < img.width; xx++) {
              const px = x + xx, py = y + yy;
              if (px < 0 || py < 0 || px >= SIZE || py >= SIZE) continue;
              const s = (yy * img.width + xx) * 4, d = (py * SIZE + px) * 4;
              buf[d] = img.data[s]; buf[d + 1] = img.data[s + 1]; buf[d + 2] = img.data[s + 2]; buf[d + 3] = img.data[s + 3];
            }
          }
        },
        // Source-over composite of a buffer-backed OffscreenCanvas source (the
        // brush dab scratch) into the surface bytes.
        drawImage: (src: any, ...a: number[]) => {
          let sx = 0, sy = 0, sw = src.width, sh = src.height, dx = 0, dy = 0, dw = src.width, dh = src.height;
          if (a.length === 4) [dx, dy, dw, dh] = a;
          else if (a.length === 8) [sx, sy, sw, sh, dx, dy, dw, dh] = a;
          else if (a.length !== 2) return;
          for (let py = Math.max(0, Math.round(dy)); py < Math.min(SIZE, Math.round(dy + dh)); py++) {
            for (let px = Math.max(0, Math.round(dx)); px < Math.min(SIZE, Math.round(dx + dw)); px++) {
              const fx = sx + Math.floor(((px - dx) * sw) / dw);
              const fy = sy + Math.floor(((py - dy) * sh) / dh);
              if (fx < 0 || fy < 0 || fx >= src.width || fy >= src.height) continue;
              const s = (fy * src.width + fx) * 4;
              const sa = src.buf[s + 3] / 255;
              if (sa <= 0) continue;
              const d = (py * SIZE + px) * 4;
              const da = buf[d + 3] / 255;
              const outA = sa + da * (1 - sa);
              if (outA <= 0) continue;
              buf[d] = Math.round((src.buf[s] * sa + buf[d] * da * (1 - sa)) / outA);
              buf[d + 1] = Math.round((src.buf[s + 1] * sa + buf[d + 1] * da * (1 - sa)) / outA);
              buf[d + 2] = Math.round((src.buf[s + 2] * sa + buf[d + 2] * da * (1 - sa)) / outA);
              buf[d + 3] = Math.round(outA * 255);
            }
          }
        },
        getImageData: (x: number, y: number, w: number, h: number) => {
          const out = new Uint8ClampedArray(Math.max(0, w) * Math.max(0, h) * 4);
          for (let yy = 0; yy < h; yy++) {
            for (let xx = 0; xx < w; xx++) {
              const px = x + xx, py = y + yy;
              if (px < 0 || py < 0 || px >= SIZE || py >= SIZE) continue;
              const s = (py * SIZE + px) * 4, d = (yy * w + xx) * 4;
              out[d] = buf[s]; out[d + 1] = buf[s + 1]; out[d + 2] = buf[s + 2]; out[d + 3] = buf[s + 3];
            }
          }
          return new (globalThis as any).ImageData(out, w, h);
        },
        clearRect: () => {}, save: () => {}, restore: () => {},
        globalCompositeOperation: "source-over", globalAlpha: 1,
      },
      pixelEpoch: undefined as number | undefined,
      pixelVersion: undefined as number | undefined,
      readRect: (x: number, y: number, w: number, h: number) => surface.context.getImageData(x, y, w, h),
      snapshotTile: (t: { x: number; y: number; w: number; h: number }) => ({
        tx: t.x / 256, ty: t.y / 256, value: surface.context.getImageData(t.x, t.y, t.w, t.h),
      }),
      restoreTile: () => {},
      toImageBitmap: async () => carrier(SIZE, SIZE, buf.slice()),
    };
    vi.spyOn(engine, "getPaintSurface").mockReturnValue(surface as never);

    mockUseEditor({
      workspace: {
        getActiveEngine: () => engine,
        getActiveHistory: () => history,
        getActiveDocumentId: () => DOC,
        notifyVisualChange: vi.fn(),
      },
      renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
      scheduler: { requestRender: vi.fn() },
      activeDocumentId: () => DOC,
      layerTransformSession: () => null,
      setLayerTransformSession: vi.fn(),
      activeTool: () => "brush",
      cropInteractionMode: () => "modern",
      canCropUndo: () => false,
      canCropRedo: () => false,
      canModernCropUndo: () => false,
      canModernCropRedo: () => false,
      layers: () => engine.getLayers(),
      activeLayerId: () => engine.getActiveLayerId(),
      selectedLayerIds: () => [],
      setSelectedLayerIds: vi.fn(),
      toggleLayerSelection: vi.fn(),
      rangeSelectLayers: vi.fn(),
      selectedLayerId: () => null,
      setSelectedLayerId: vi.fn(),
      textEditSession: () => null,
      setTextEditSession: vi.fn(),
      setStatusLoadingMessage: vi.fn(),
      setShowExportDialog: vi.fn(),
      setShowPrintDialog: vi.fn(),
      setShowResizeDialog: vi.fn(),
      fgColor: () => "#ff0000",
      bgColor: () => "#ffffff",
      docWidth: () => SIZE,
      docHeight: () => SIZE,
      brushSize: () => 20,
      brushHardness: () => 1,
      eraserSize: () => 20,
      eraserHardness: () => 1,
    });

    const overlay = useBrushOverlay();
    const canvas = document.createElement("canvas");
    canvas.width = SIZE;
    canvas.height = SIZE;
    overlay.setOverlayCanvasRef(canvas);
    const commands = useEditorCommands(() => {});

    const preHash = hash(engine.getLayer(layer.id)!.imageBitmap);

    overlay.onPaintStroke([{ x: 20, y: 20 }], false, settings, false);
    await overlay.commitBrushStroke(engine as never, history as never, layer.id, false);
    await flushC4Commits();

    const rust = sim.store.get(`${DOC}|${layer.id}`)!;
    expect(rust.undo.length, "PROBE the stroke recorded one Rust Pixel entry").toBe(1);
    const postHash = hash(engine.getLayer(layer.id)!.imageBitmap);
    expect(postHash, "PROBE the stroke is visible in the model bitmap").not.toBe(preHash);

    commands.undo();
    for (let i = 0; i < 40; i++) await Promise.resolve();
    await new Promise((r) => setTimeout(r, 0));

    const postUndoRustEpoch = sim.store.get(`${DOC}|${layer.id}`)!.epoch;
    const live = engine.getLayer(layer.id)!;

    // 1. The model bitmap must show the un-stroked pixels, not the stroke.
    expect(hash(live.imageBitmap)).toBe(preHash);

    // 2. The epoch must never claim currency over a stale bitmap. (Design-neutral
    //    form of the "bitmap is absent or epoch unmoved" check: a re-derivation
    //    legitimately carries the real post-undo epoch, but only once the bytes
    //    behind it are the reverted ones.)
    expect(
      live.bitmapEpoch === postUndoRustEpoch && hash(live.imageBitmap) !== preHash,
      `epoch ${String(live.bitmapEpoch)} was stamped on stale pixels while Rust is at ${postUndoRustEpoch}`,
    ).toBe(false);

    // 3. The repair must not blank the layer: Navigator/Adjustments guard on
    //    imageBitmap, so dropping it would hide the layer until some consumer
    //    happened to call ensureBitmapCurrent.
    expect(live.imageBitmap).not.toBeNull();
  });
});
