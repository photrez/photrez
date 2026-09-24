// Brush commit hot path: per-stroke work pins (counts and branches, no wall time).
// Drives the REAL production commit path via the hook with invoke mocked by an
// in-test Rust store emulator (mirrors crates/core/src/pixel_store.rs).
//
// Part 1: a fresh stroke on a current surface issues zero full-layer reads and
//   exactly one epoch probe. A stale surface still takes the full read.
// Part 2: response bytes reach history and the renderer as one shared copy
//   with bytes equal to the store response.
// Part 3: two overlapping stroke commits serialize behind each other: both
//   land in order, versions rise one step each, and the queue drains.
// Part 4: two strokes with OVERLAPPING dirty rects accumulate byte-exact:
//   both writes land, history and renderer pins fire once per stroke, and the
//   store holds the union of both sent regions (no wipe of stroke 1).
// Part 5: fallback/frequency baseline - per-condition counts (never wall time)
//   of the synchronous-fallback warn (useBrushOverlay.ts:231) and the two
//   full-layer read branches (seed readRect(0,0,w,h); stale rehydrate via
//   rust_pixels_snapshot_layer) over 10-commit runs: healthy after seed,
//   stale surface, absent store layer, and IPC write failure.

import { afterAll, beforeAll, beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { PaintTileSurface } from "@/lib/paint/paintTileSurface";
import { useBrushOverlay, flushC4Commits, c4PendingCommits } from "../useBrushOverlay";
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

const realGetContext = (globalThis as any).HTMLCanvasElement.prototype.getContext;
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

type SimLayer = { w: number; h: number; pixels: number[]; undo: any[]; redo: any[]; epoch: number; version: number };

function makeSim(opts?: { delayFirstWriteMs?: number; failWriteRegion?: boolean }) {
  const store = new Map<string, SimLayer>();
  const calls: { cmd: string; args: any }[] = [];
  const key = (docId: string, layerId: string) => `${docId}|${layerId}`;
  let commitCount = 0;
  let initCount = 0;
  const delayFirstWriteMs = opts?.delayFirstWriteMs ?? 0;
  const failWriteRegion = opts?.failWriteRegion ?? false;

  const invoke = async (cmd: string, args: any): Promise<any> => {
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
    if (cmd === "rust_pixels_write_region") {
      commitCount += 1;
      if (delayFirstWriteMs > 0 && commitCount === 1) {
        await new Promise<void>((resolve) => setTimeout(resolve, delayFirstWriteMs));
      }
      if (failWriteRegion) {
        // Mirrors Tauri v2 rejecting with the Rust `Err(String)` payload; the
        // production catch does not inspect the shape, only that it rejected.
        throw "rust ipc unavailable";
      }
      const layer = store.get(k);
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
      return {
        before: [{ x, y, w: rw, h: rh, data: beforePx.slice((y * layer.w + x) * 4, (y * layer.w + x) * 4 + rw * rh * 4) }],
        after: [{ x, y, w: rw, h: rh, data: afterPx.slice((y * layer.w + x) * 4, (y * layer.w + x) * 4 + rw * rh * 4) }],
        epoch: layer.epoch,
        version: layer.version,
      };
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

function makeSurface() {
  const surface: any = {
    context: {
      drawImage: vi.fn(),
      clearRect: vi.fn(),
      save: vi.fn(),
      restore: vi.fn(),
      putImageData: vi.fn(),
      globalCompositeOperation: "source-over",
      globalAlpha: 1,
    },
    pixelEpoch: undefined as number | undefined,
    pixelVersion: undefined as number | undefined,
    snapshotTile: vi.fn((t: { x: number; y: number; w: number; h: number }) => ({
      tx: t.x / 256,
      ty: t.y / 256,
      value: { width: t.w, height: t.h, data: new Uint8ClampedArray(t.w * t.h * 4).fill(7) },
    })),
    restoreTile: vi.fn(),
    readRect: vi.fn((_x: number, _y: number, w: number, h: number) => ({
      width: w,
      height: h,
      data: new Uint8ClampedArray(w * h * 4),
    })),
  };
  return surface;
}

// Buffer-backed 2d canvas for the overlap test: reads reflect prior writes,
// so an accumulation break shows up as wrong bytes. Only the ops the commit
// seam uses are supported: create/get/putImageData, clearRect, drawImage in
// 3/5/9-arg form (source-over with globalAlpha, nearest sampling), and
// save/restore. Anything else throws instead of silently faking it.
interface FaithfulImageData {
  data: Uint8ClampedArray;
  width: number;
  height: number;
}

class FaithfulCanvas {
  width: number;
  height: number;
  buf: Uint8ClampedArray;
  private ctx: FaithfulCtx2D;
  constructor(w: number, h: number) {
    this.width = w;
    this.height = h;
    this.buf = new Uint8ClampedArray(Math.max(0, Math.floor(w)) * Math.max(0, Math.floor(h)) * 4);
    this.ctx = new FaithfulCtx2D(this);
  }
  getContext(_kind: string, _opts?: unknown): FaithfulCtx2D {
    return this.ctx;
  }
}

class FaithfulCtx2D {
  globalAlpha = 1;
  globalCompositeOperation = "source-over";
  private readonly canvas: FaithfulCanvas;
  private readonly stack: { alpha: number; op: string }[] = [];
  constructor(canvas: FaithfulCanvas) {
    this.canvas = canvas;
  }
  createImageData(w: number, h: number): FaithfulImageData {
    return { data: new Uint8ClampedArray(Math.max(0, w) * Math.max(0, h) * 4), width: w, height: h };
  }
  getImageData(sx: number, sy: number, sw: number, sh: number): FaithfulImageData {
    const out = new Uint8ClampedArray(sw * sh * 4);
    const W = this.canvas.width;
    const H = this.canvas.height;
    const buf = this.canvas.buf;
    for (let y = 0; y < sh; y++) {
      for (let x = 0; x < sw; x++) {
        const px = sx + x;
        const py = sy + y;
        if (px < 0 || py < 0 || px >= W || py >= H) continue;
        const s = (py * W + px) * 4;
        const d = (y * sw + x) * 4;
        out[d] = buf[s];
        out[d + 1] = buf[s + 1];
        out[d + 2] = buf[s + 2];
        out[d + 3] = buf[s + 3];
      }
    }
    return { data: out, width: sw, height: sh };
  }
  putImageData(img: FaithfulImageData, dx: number, dy: number): void {
    const W = this.canvas.width;
    const H = this.canvas.height;
    const buf = this.canvas.buf;
    for (let y = 0; y < img.height; y++) {
      for (let x = 0; x < img.width; x++) {
        const px = dx + x;
        const py = dy + y;
        if (px < 0 || py < 0 || px >= W || py >= H) continue;
        const s = (y * img.width + x) * 4;
        const d = (py * W + px) * 4;
        buf[d] = img.data[s];
        buf[d + 1] = img.data[s + 1];
        buf[d + 2] = img.data[s + 2];
        buf[d + 3] = img.data[s + 3];
      }
    }
  }
  clearRect(x: number, y: number, w: number, h: number): void {
    const x0 = Math.max(0, Math.floor(x));
    const y0 = Math.max(0, Math.floor(y));
    const x1 = Math.min(this.canvas.width, Math.ceil(x + w));
    const y1 = Math.min(this.canvas.height, Math.ceil(y + h));
    const buf = this.canvas.buf;
    for (let py = y0; py < y1; py++) {
      for (let px = x0; px < x1; px++) {
        const d = (py * this.canvas.width + px) * 4;
        buf[d] = 0;
        buf[d + 1] = 0;
        buf[d + 2] = 0;
        buf[d + 3] = 0;
      }
    }
  }
  save(): void {
    this.stack.push({ alpha: this.globalAlpha, op: this.globalCompositeOperation });
  }
  restore(): void {
    const s = this.stack.pop();
    if (!s) return;
    this.globalAlpha = s.alpha;
    this.globalCompositeOperation = s.op;
  }
  drawImage(src: unknown, ...args: number[]): void {
    if (!(src instanceof FaithfulCanvas)) throw new Error("faithful ctx cannot read this source");
    let sx = 0;
    let sy = 0;
    let sw = src.width;
    let sh = src.height;
    let dx = 0;
    let dy = 0;
    let dw = src.width;
    let dh = src.height;
    if (args.length === 2) {
      [dx, dy] = args;
    } else if (args.length === 4) {
      [dx, dy, dw, dh] = args;
    } else if (args.length === 8) {
      [sx, sy, sw, sh, dx, dy, dw, dh] = args;
    } else {
      throw new Error("faithful ctx drawImage needs 3, 5, or 9 args");
    }
    if (this.globalCompositeOperation !== "source-over") throw new Error("faithful ctx only blends source-over");
    if (dw <= 0 || dh <= 0 || sw <= 0 || sh <= 0) return;
    const W = this.canvas.width;
    const H = this.canvas.height;
    const buf = this.canvas.buf;
    const sbuf = src.buf;
    const x0 = Math.max(0, Math.round(dx));
    const y0 = Math.max(0, Math.round(dy));
    const x1 = Math.min(W, Math.round(dx + dw));
    const y1 = Math.min(H, Math.round(dy + dh));
    for (let py = y0; py < y1; py++) {
      for (let px = x0; px < x1; px++) {
        const fx = sx + Math.floor(((px - dx) * sw) / dw);
        const fy = sy + Math.floor(((py - dy) * sh) / dh);
        if (fx < 0 || fy < 0 || fx >= src.width || fy >= src.height) continue;
        const s = (fy * src.width + fx) * 4;
        const sa = (sbuf[s + 3] / 255) * this.globalAlpha;
        if (sa <= 0) continue;
        const d = (py * W + px) * 4;
        const da = buf[d + 3] / 255;
        const outA = sa + da * (1 - sa);
        if (outA <= 0) {
          buf[d] = 0;
          buf[d + 1] = 0;
          buf[d + 2] = 0;
          buf[d + 3] = 0;
          continue;
        }
        buf[d] = Math.round((sbuf[s] * sa + buf[d] * da * (1 - sa)) / outA);
        buf[d + 1] = Math.round((sbuf[s + 1] * sa + buf[d + 1] * da * (1 - sa)) / outA);
        buf[d + 2] = Math.round((sbuf[s + 2] * sa + buf[d + 2] * da * (1 - sa)) / outA);
        buf[d + 3] = Math.round(outA * 255);
      }
    }
  }
}

function makeHarness(surface: any, uploadSurfaceTiles = vi.fn()) {
  const layer = {
    id: "layer-1",
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
  const history: any = {
    entries: [] as any[],
    commit: vi.fn((p: any) => { history.entries.push(p); }),
    setLastPaintCoords: vi.fn(),
    getLastPaintCoords: vi.fn(() => null),
  };
  const doc = { id: "doc-test" };
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
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles },
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
  return { overlay, layer, engine, history, uploadSurfaceTiles, surface };
}

const settings = { size: 20, hardness: 1, opacity: 1, flow: 1, smoothing: 0.5 };
const count = (sim: ReturnType<typeof makeSim>, cmd: string) => sim.calls.filter((c) => c.cmd === cmd).length;

describe("brush commit hot path", () => {
  beforeAll(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({ confirm: vi.fn() } as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
  });
  afterAll(() => {
    vi.restoreAllMocks();
  });
  beforeEach(() => {
    vi.spyOn(brushToolStateModule, "getPaintToolBlockReason").mockImplementation((l: any, e: any) => null);
    vi.spyOn(docModule, "isFacadeOwnedLayer").mockImplementation((id: string) => false);
    hoist.setSim(makeSim());
    localStorage.setItem("photrez.rustPixels", "1");
    localStorage.removeItem("photrez.canonicalCommit");
  });
  afterEach(() => {
    localStorage.clear();
  });

  it("part 1: a fresh stroke on a current surface takes no full read and one epoch probe", async () => {
    const surface = makeSurface();
    const { overlay, engine, history } = makeHarness(surface);
    const sim = hoist.getSim();

    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history as any, "layer-1", false);
    await flushC4Commits();

    const snapsAfterFirst = count(sim, "rust_pixels_snapshot_layer");
    const epochsAfterFirst = count(sim, "rust_pixels_get_epoch");

    overlay.onPaintStroke([{ x: 60, y: 60 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history as any, "layer-1", false);
    await flushC4Commits();

    expect(count(sim, "rust_pixels_write_region")).toBe(2);
    expect(count(sim, "rust_pixels_snapshot_layer") - snapsAfterFirst).toBe(0);
    expect(count(sim, "rust_pixels_get_epoch") - epochsAfterFirst).toBe(1);
  });

  it("part 1: a stale surface still takes the full read", async () => {
    const surface = makeSurface();
    const { overlay, engine, history } = makeHarness(surface);
    const sim = hoist.getSim();

    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history as any, "layer-1", false);
    await flushC4Commits();

    (surface as any).pixelEpoch = -1;
    const snapsBefore = count(sim, "rust_pixels_snapshot_layer");
    overlay.onPaintStroke([{ x: 60, y: 60 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history as any, "layer-1", false);
    await flushC4Commits();

    expect(count(sim, "rust_pixels_write_region")).toBe(2);
    expect(count(sim, "rust_pixels_snapshot_layer") - snapsBefore).toBe(1);
  });

  it("part 2: history and renderer share one response copy with identical bytes", async () => {
    const surface = makeSurface();
    const uploadSurfaceTiles = vi.fn();
    const { overlay, engine, history } = makeHarness(surface, uploadSurfaceTiles);
    const sim = hoist.getSim();

    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history as any, "layer-1", false);
    await flushC4Commits();

    const imperative = history.commit.mock.calls[0][2];
    const uploaded = uploadSurfaceTiles.mock.calls[0][3];
    expect(imperative.after.length).toBeGreaterThan(0);
    expect(uploaded.length).toBe(imperative.after.length);
    const wr = sim.calls.find((c) => c.cmd === "rust_pixels_write_region")!;
    for (let i = 0; i < imperative.after.length; i++) {
      expect(uploaded[i].data).toBe(imperative.after[i].data);
      expect(Array.from(imperative.after[i].data as Uint8ClampedArray)).toEqual(Array.from(wr.args.rgba as number[]));
    }
  });

  it("part 3: two overlapping commits serialize behind each other with no drop", async () => {
    hoist.setSim(makeSim({ delayFirstWriteMs: 10 }));
    const surface = makeSurface();
    const uploadSurfaceTiles = vi.fn();
    const { overlay, engine, history } = makeHarness(surface, uploadSurfaceTiles);
    const sim = hoist.getSim();

    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    const first = overlay.commitBrushStroke(engine, history as any, "layer-1", false);
    overlay.onPaintStroke([{ x: 200, y: 200 }], false, settings, false);
    const second = overlay.commitBrushStroke(engine, history as any, "layer-1", false);
    expect(c4PendingCommits("doc-test", "layer-1")).toBeGreaterThan(0);
    await Promise.all([first, second]);
    await flushC4Commits();

    expect(count(sim, "rust_pixels_write_region")).toBe(2);
    expect(sim.store.get("doc-test|layer-1")!.version).toBe(2);
    expect(history.entries.length).toBe(2);
    const firstAfter = history.commit.mock.calls[0][2].after[0];
    const secondAfter = history.commit.mock.calls[1][2].after[0];
    expect(firstAfter.x).toBeLessThan(secondAfter.x);
    expect(c4PendingCommits("doc-test", "layer-1")).toBe(0);
    expect(c4PendingCommits()).toBe(0);
  });

  it("part 4: two strokes with overlapping dirty rects accumulate byte-exact through the commit seam", async () => {
    // A real PaintTileSurface needs a stateful 2d context (the file stub
    // returns zeros for every read, which would hide a wipe). Swap in the
    // buffer-backed canvas for this test only.
    const prevOffscreen = (globalThis as unknown as { OffscreenCanvas: unknown }).OffscreenCanvas;
    (globalThis as unknown as { OffscreenCanvas: unknown }).OffscreenCanvas = FaithfulCanvas as unknown as typeof OffscreenCanvas;
    try {
      // Slow the first store write so stroke 2 is captured and enqueued while
      // stroke 1 is still in flight: the queue must order them anyway.
      hoist.setSim(makeSim({ delayFirstWriteMs: 10 }));
      const surface = new PaintTileSurface(512, 512);
      const uploadSurfaceTiles = vi.fn();
      const { overlay, engine, history } = makeHarness(surface, uploadSurfaceTiles);
      const sim = hoist.getSim();

      overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
      const first = overlay.commitBrushStroke(engine, history as any, "layer-1", false);
      overlay.onPaintStroke([{ x: 35, y: 30 }], false, settings, false);
      const second = overlay.commitBrushStroke(engine, history as any, "layer-1", false);
      await Promise.all([first, second]);
      await flushC4Commits();

      // One store write per stroke, in stroke order, with truly overlapping rects.
      const writes = sim.calls.filter((c) => c.cmd === "rust_pixels_write_region");
      expect(writes.length).toBe(2);
      const r1 = writes[0].args as { x: number; y: number; w: number; h: number; rgba: Uint8Array };
      const r2 = writes[1].args as { x: number; y: number; w: number; h: number; rgba: Uint8Array };
      expect(r1.x).toBeLessThan(r2.x);
      expect(r2.x).toBeLessThan(r1.x + r1.w);
      expect([r1.y, r1.w, r1.h]).toEqual([r2.y, r2.w, r2.h]);

      // One history entry and one renderer upload per stroke, rects matching.
      expect(history.commit).toHaveBeenCalledTimes(2);
      const a1 = history.commit.mock.calls[0][2].after[0] as { x: number; y: number; width: number; height: number };
      const a2 = history.commit.mock.calls[1][2].after[0] as { x: number; y: number; width: number; height: number };
      expect([a1.x, a1.y, a1.width, a1.height]).toEqual([r1.x, r1.y, r1.w, r1.h]);
      expect([a2.x, a2.y, a2.width, a2.height]).toEqual([r2.x, r2.y, r2.w, r2.h]);

      expect(uploadSurfaceTiles).toHaveBeenCalledTimes(2);
      const u1 = uploadSurfaceTiles.mock.calls[0][3][0] as { x: number; y: number; width: number; height: number };
      const u2 = uploadSurfaceTiles.mock.calls[1][3][0] as { x: number; y: number; width: number; height: number };
      expect([u1.x, u1.y, u1.width, u1.height]).toEqual([r1.x, r1.y, r1.w, r1.h]);
      expect([u2.x, u2.y, u2.width, u2.height]).toEqual([r2.x, r2.y, r2.w, r2.h]);

      const stored = sim.store.get("doc-test|layer-1");
      expect(stored!.version).toBe(2);

      // Dab content reaches the store: each dab center is red in the final
      // pixels. This is independent of the sent payloads below, so a lost
      // composite (stroke never lands) reddens here even though the replay
      // stays self-consistent. (No-wipe of stroke 1 by stroke 2 is pinned
      // by the replay: a stale overlapping write breaks that equality.)
      // Probes sit at the dab centers, well inside the ink; the soft tip
      // profile falls to zero near the disc edge, so edge probes would be
      // transparent even when accumulation is correct.
      const px = (x: number, y: number): number[] => {
        const o = (y * 512 + x) * 4;
        return Array.from((stored!.pixels as unknown as Uint8Array).slice(o, o + 4));
      };
      // Red is exact for any dab alpha over the transparent base; alpha
      // itself belongs to the stroke pipeline, so only require it positive.
      for (const [x, y] of [[30, 30], [35, 30]] as const) {
        const p = px(x, y);
        expect([p[0], p[1], p[2]]).toEqual([255, 0, 0]);
        expect(p[3]).toBeGreaterThan(0);
      }

      // Byte-exact union: replay the seed bytes plus both sent regions with
      // the same region copy the store performs. A stroke-2 wipe of stroke 1
      // (stale read, wrong offset, out-of-order land) breaks this equality.
      const init = sim.calls.find((c) => c.cmd === "rust_pixels_init")!;
      const expected = Array.from(init.args.bytes as Uint8Array);
      for (const wcall of writes) {
        const a = wcall.args as { x: number; y: number; w: number; h: number; rgba: Uint8Array };
        const src = Array.from(a.rgba);
        for (let row = 0; row < a.h; row++) {
          const dst = ((a.y + row) * 512 + a.x) * 4;
          const s = row * a.w * 4;
          for (let i = 0; i < a.w * 4; i++) expected[dst + i] = src[s + i];
        }
      }
      expect(Array.from(stored!.pixels)).toEqual(expected);
      expect(c4PendingCommits("doc-test", "layer-1")).toBe(0);
    } finally {
      (globalThis as unknown as { OffscreenCanvas: unknown }).OffscreenCanvas = prevOffscreen;
    }
  });
});

// Part 5: fallback/frequency baseline. Counts only - no wall-time asserts here
// (timings live in the dedicated bench harnesses). Each condition runs N
// commits and counts the synchronous-fallback warn plus both full-layer read
// branches, so later tasks gate on "fallback hits == 0" against real numbers.
const FALLBACK_WARN = "[paint] async deferred commit failed";

function seedFullReadCount(surface: ReturnType<typeof makeSurface>): number {
  return surface.readRect.mock.calls.filter(
    (c: unknown[]) => c[0] === 0 && c[1] === 0 && c[2] === 512 && c[3] === 512,
  ).length;
}

function fallbackWarnCount(spy: { mock: { calls: unknown[][] } }): number {
  return spy.mock.calls.filter((c) => String(c[0]).includes(FALLBACK_WARN)).length;
}

async function runCommits(
  overlay: ReturnType<typeof makeHarness>["overlay"],
  engine: unknown,
  history: unknown,
  surface: unknown,
  n: number,
  opts?: { stale?: boolean },
): Promise<void> {
  for (let i = 0; i < n; i++) {
    if (opts?.stale) (surface as { pixelEpoch: number }).pixelEpoch = -1;
    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine as never, history as never, "layer-1", false);
    await flushC4Commits();
  }
}

describe("part 5: commit fallback frequency baseline (counts only)", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({ confirm: vi.fn() } as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
    vi.spyOn(brushToolStateModule, "getPaintToolBlockReason").mockImplementation(() => null);
    vi.spyOn(docModule, "isFacadeOwnedLayer").mockImplementation((id: string) => false);
    hoist.setSim(makeSim());
    localStorage.setItem("photrez.rustPixels", "1");
    localStorage.removeItem("photrez.canonicalCommit");
  });
  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("healthy after seed: 0 fallback warns, 0 rehydrates, no new full reads over 10 commits", async () => {
    const surface = makeSurface();
    const { overlay, engine, history } = makeHarness(surface);
    const warns = vi.spyOn(console, "warn").mockImplementation(() => {});

    // Warmup commit seeds the store: exactly one full seed read is allowed here.
    await runCommits(overlay, engine, history, surface, 1);
    expect(seedFullReadCount(surface)).toBe(1);

    const sim = hoist.getSim();
    const snapsBefore = count(sim, "rust_pixels_snapshot_layer");
    await runCommits(overlay, engine, history, surface, 10);

    expect(fallbackWarnCount(warns)).toBe(0);
    expect(seedFullReadCount(surface)).toBe(1); // no further full readRect
    expect(count(sim, "rust_pixels_snapshot_layer") - snapsBefore).toBe(0); // no rehydrate
    expect(count(sim, "rust_pixels_write_region")).toBe(11);
  });

  it("stale surface: one full rehydrate read per commit, 0 fallback warns over 10 commits", async () => {
    const surface = makeSurface();
    const { overlay, engine, history } = makeHarness(surface);
    const warns = vi.spyOn(console, "warn").mockImplementation(() => {});

    await runCommits(overlay, engine, history, surface, 1); // seed
    const sim = hoist.getSim();
    const snapsBefore = count(sim, "rust_pixels_snapshot_layer");
    await runCommits(overlay, engine, history, surface, 10, { stale: true });

    expect(count(sim, "rust_pixels_snapshot_layer") - snapsBefore).toBe(10);
    expect(fallbackWarnCount(warns)).toBe(0);
    // The stale rehydrate reads from the store, not via surface.readRect.
    expect(seedFullReadCount(surface)).toBe(1);
    expect(count(sim, "rust_pixels_write_region")).toBe(11);
  });

  it("absent store layer: exactly one full seed read on the first commit, none after", async () => {
    const surface = makeSurface();
    const { overlay, engine, history } = makeHarness(surface);
    const warns = vi.spyOn(console, "warn").mockImplementation(() => {});

    await runCommits(overlay, engine, history, surface, 1);
    expect(seedFullReadCount(surface)).toBe(1);
    expect(count(hoist.getSim(), "rust_pixels_init")).toBe(1);

    await runCommits(overlay, engine, history, surface, 10);
    expect(seedFullReadCount(surface)).toBe(1);
    expect(count(hoist.getSim(), "rust_pixels_init")).toBe(1);
    expect(fallbackWarnCount(warns)).toBe(0);
  });

  it("IPC write failure: every commit warns the fallback once and still lands in history", async () => {
    hoist.setSim(makeSim({ failWriteRegion: true }));
    const surface = makeSurface();
    const { overlay, engine, history } = makeHarness(surface);
    const warns = vi.spyOn(console, "warn").mockImplementation(() => {});

    await runCommits(overlay, engine, history, surface, 10);

    expect(fallbackWarnCount(warns)).toBe(10);
    expect(history.commit).toHaveBeenCalledTimes(10);
    expect(count(hoist.getSim(), "rust_pixels_write_region")).toBe(10); // attempted, none succeeded
    expect(c4PendingCommits("doc-test", "layer-1")).toBe(0);
  });
});
