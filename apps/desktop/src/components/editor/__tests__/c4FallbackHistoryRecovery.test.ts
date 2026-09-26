// Recovery proof for the approved caller-predicate fix on the deferred Rust
// brush commit: the SUCCESSFUL c4 commit passes alreadyRecordedInRust=true
// (rust_pixels_write_region already owns the apply), and the FAILED-write
// fallback leaves it false so history.ts records exactly one apply_tile_patch.
// Both paths are driven through the real useBrushOverlay hook and a real
// CommandHistory, with `@tauri-apps/api/core` invoke mocked by an in-test Rust
// store emulator. The rejection shape is the faithful one: a bare string
// ("E_RUST: boom"), never an Error instance.
//
// Covers:
//   1. c4 success  -> 1 state-changing apply (write_region), 0 apply_tile_patch.
//   2. bare-string write_region rejection -> exactly 1 apply_tile_patch, TS
//      history entry kept, undo/redo returns the recorded pre/post bytes, and
//      the emulator holds one recoverable apply.

import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { useBrushOverlay, flushC4Commits } from "../useBrushOverlay";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";
import { CommandHistory } from "@/engine/history";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import * as brushToolStateModule from "../brushToolState";
import * as docModule from "@/engine/document";

// historyBridgeEnabled() requires the Tauri runtime detector to report true.
vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(() => true),
  runTauriWindowAction: vi.fn(),
}));

// ── jsdom polyfills (mirror brushStrokeCommitOrdering.test.ts) ──
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

// jsdom's CanvasRenderingContext2D.drawImage throws on the polyfilled
// OffscreenCanvas brush scratch used by the live-preview composite path.
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

// ── in-test Rust store emulator (mirrors crates/core/src/pixel_store.rs) ──
type SimLayer = { w: number; h: number; pixels: number[]; undo: any[]; redo: any[]; epoch: number; version: number };
type SimCall = { cmd: string; args: any };

const wireW = (t: any) => (typeof t.w === "number" ? t.w : t.width);
const wireH = (t: any) => (typeof t.h === "number" ? t.h : t.height);

function makeSim(opts?: { rejectWriteRegion?: string }) {
  const store = new Map<string, SimLayer>();
  const calls: SimCall[] = [];
  const key = (docId: string, layerId: string) => `${docId}|${layerId}`;
  let applyCount = 0;
  const rejectWriteRegion = opts?.rejectWriteRegion;

  const write = (s: SimLayer, tiles: any[]) => {
    for (const t of tiles) {
      const tw = wireW(t);
      const th = wireH(t);
      for (let row = 0; row < th; row++) {
        const dst = ((t.y + row) * s.w + t.x) * 4;
        const src = row * tw * 4;
        for (let i = 0; i < tw * 4; i++) s.pixels[dst + i] = t.data[src + i];
      }
    }
  };

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
      if (!s) throw new Error("no layer"); // getRustEpoch() catches -> null
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
        pixels: Array.from(args.bytes),
        undo: [],
        redo: [],
        epoch: 0,
        version: 0,
      });
      return;
    }
    if (cmd === "rust_pixels_write_region") {
      // Post-IPC Rust failure shape: a bare string rejection (Tauri v2 Result<_, String>).
      if (rejectWriteRegion !== undefined) throw rejectWriteRegion;
      const s = store.get(k);
      if (!s) throw new Error("no layer");
      applyCount += 1; // state-changing apply, same counter as apply_tile_patch
      const { x, y, w, h, rgba } = args as { x: number; y: number; w: number; h: number; rgba: number[] };
      const beforePx = s.pixels.slice();
      const afterPx = s.pixels.slice();
      for (let row = 0; row < h; row++) {
        const dst = ((y + row) * s.w + x) * 4;
        const src = row * w * 4;
        for (let i = 0; i < w * 4; i++) afterPx[dst + i] = rgba[src + i];
      }
      s.pixels = afterPx;
      s.undo.push({ before: beforePx, after: afterPx });
      s.redo = [];
      s.epoch += 1;
      s.version += 1;
      const beforeData = beforePx.slice((y * s.w + x) * 4, (y * s.w + x) * 4 + w * h * 4);
      const afterData = afterPx.slice((y * s.w + x) * 4, (y * s.w + x) * 4 + w * h * 4);
      return { before: [{ x, y, w, h, data: beforeData }], after: [{ x, y, w, h, data: afterData }], epoch: s.epoch, version: s.version };
    }
    if (cmd === "apply_tile_patch") {
      const s = store.get(k);
      if (!s) throw new Error("no layer");
      applyCount += 1;
      write(s, args.after);
      s.undo.push({ before: args.before, after: args.after });
      s.redo = [];
      s.epoch += 1;
      s.version += 1;
      return { tiles: args.after.map((t: any) => ({ x: t.x, y: t.y, w: wireW(t), h: wireH(t), data: t.data })), epoch: s.epoch, version: s.version };
    }
    throw new Error("unknown cmd " + cmd);
  };

  return {
    invoke,
    store,
    calls,
    key,
    get applyCount() {
      return applyCount;
    },
    count: (cmd: string) => calls.filter((c) => c.cmd === cmd).length,
  };
}

const hoist = vi.hoisted(() => {
  let sim: ReturnType<typeof makeSim> | null = null;
  const invoke = (cmd: string, args: any) => sim!.invoke(cmd, args);
  return { invoke, setSim: (s: ReturnType<typeof makeSim>) => { sim = s; }, getSim: () => sim! };
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: hoist.invoke }));

// ── harness (mirrors brushStrokeCommitOrdering.test.ts) ──
function makeSurface() {
  const snapshotted: unknown[] = [];
  const surface: any = {
    snapshotted,
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
    snapshotTile: vi.fn((t: { x: number; y: number; w: number; h: number }) => {
      const patch = { tx: t.x / 256, ty: t.y / 256, value: { width: t.w, height: t.h, data: new Uint8ClampedArray(t.w * t.h * 4).fill(7) } };
      snapshotted.push(patch);
      return patch;
    }),
    restoreTile: vi.fn(),
    readRect: vi.fn((_x: number, _y: number, w: number, h: number) => ({
      width: w,
      height: h,
      data: new Uint8ClampedArray(w * h * 4),
    })),
  };
  return surface;
}

const DOC = "doc-test";
const LAYER = "layer-1";

function makeHarness(surface: any, history: CommandHistory) {
  const layer = {
    id: LAYER,
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
  const doc = { id: DOC };
  const engine: any = {
    getId: () => doc.id,
    getActiveLayerId: () => layer.id,
    getLayer: () => layer,
    snapshot: vi.fn(() => ({ id: doc.id, name: "doc", width: 512, height: 512, layers: [], activeLayerId: LAYER, selection: null, viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 }, dirty: false })),
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
  return { overlay, engine, layer };
}

const settings = { size: 20, hardness: 1, opacity: 1, flow: 1, smoothing: 0.5 };

// Let the bridge's dynamic-import + invoke microtask chain settle before
// asserting apply counts (this is NOT a census read; census reads must use
// flushPixelInvokeCensus).
const flushBridge = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

describe("c4 recovery (approved caller predicate)", () => {
  beforeAll(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({ confirm: vi.fn() } as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
  });
  afterAll(() => {
    vi.restoreAllMocks();
    (globalThis as any).HTMLCanvasElement.prototype.getContext = realGetContext;
  });
  beforeEach(() => {
    vi.spyOn(brushToolStateModule, "getPaintToolBlockReason").mockImplementation(() => null);
    vi.spyOn(docModule, "isFacadeOwnedLayer").mockImplementation(() => false);
    localStorage.setItem("photrez.rustPixels", "1");
    localStorage.setItem("photrez.historyBridge", "1");
    localStorage.removeItem("photrez.canonicalCommit");
  });
  afterEach(() => {
    localStorage.clear();
    void flushC4Commits();
  });

  const stroke = async (overlay: ReturnType<typeof useBrushOverlay>, engine: any, history: CommandHistory) => {
    overlay.onPaintStroke([{ x: 30, y: 30 }], false, settings, false);
    await overlay.commitBrushStroke(engine, history, LAYER, false);
    await flushC4Commits();
    await flushBridge();
  };

  it("c4 success: rust_pixels_write_region lands and the commit records exactly ONE state-changing apply", async () => {
    hoist.setSim(makeSim());
    const surface = makeSurface();
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC);
    const { overlay, engine } = makeHarness(surface, history);
    const censusBefore = (await flushPixelInvokeCensus()).entries.length;

    await stroke(overlay, engine, history);

    const sim = hoist.getSim();
    expect(sim.count("rust_pixels_write_region")).toBe(1);
    expect(sim.count("apply_tile_patch")).toBe(0); // no double-apply: Rust already owns the state change
    expect(sim.applyCount).toBe(1);
    expect(history.getUndoCount()).toBe(1);
    const census = await flushPixelInvokeCensus();
    expect(census.entries.slice(censusBefore)).toEqual([
      { order: expect.any(Number), command: "rust_pixels_write_region", phase: "resolved" },
    ]);
  });

  it("bare-string rejection: exactly one apply_tile_patch + real CommandHistory undo/redo recovers pre/post bytes", async () => {
    hoist.setSim(makeSim({ rejectWriteRegion: "E_RUST: boom" }));
    const surface = makeSurface();
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC);
    const { overlay, engine } = makeHarness(surface, history);
    const censusBeforeRejection = (await flushPixelInvokeCensus()).entries.length;

    await stroke(overlay, engine, history);

    const sim = hoist.getSim();
    expect(sim.count("rust_pixels_write_region")).toBe(1); // attempted, rejected with a bare string
    expect(sim.count("apply_tile_patch")).toBe(1); // the fallback records the ONLY apply
    expect(sim.applyCount).toBe(1);
    expect(history.getUndoCount()).toBe(1); // TS history entry kept despite the rejection
    // The census keeps both: the failed write first, then the single recovery
    // apply, in that order.
    const census = await flushPixelInvokeCensus();
    const recorded = census.entries.slice(censusBeforeRejection);
    expect(recorded.map((e) => `${e.command}:${e.phase}`)).toEqual([
      "rust_pixels_write_region:rejected",
      "apply_tile_patch:resolved",
    ]);
    expect(recorded[1].order).toBeGreaterThan(recorded[0].order);

    // Undo returns the entry's pre-stroke bytes, redo its post-stroke bytes.
    const undone = history.undo(engine.snapshot());
    expect(undone).not.toBeNull();
    const undoPatches = history.consumeLastUndoPatches();
    expect(undoPatches, "undo patches for the recovered entry").toBeDefined();
    expect(undoPatches!.before.length).toBeGreaterThan(0);

    const redone = history.redo(engine.snapshot());
    expect(redone).not.toBeNull();
    const redoPatches = history.consumeLastRedoPatches();
    expect(redoPatches, "redo patches for the recovered entry").toBeDefined();
    expect(redoPatches!.after.length).toBeGreaterThan(0);

    // Byte-for-byte: the recorded patches are exactly what the emulator applied.
    const applyCall = sim.calls.find((c) => c.cmd === "apply_tile_patch")!;
    const flat = (ps: { data: Uint8ClampedArray }[]) => Array.from(ps.flatMap((p) => Array.from(p.data)));
    expect(flat(undoPatches!.before)).toEqual(flat(applyCall.args.before));
    expect(flat(redoPatches!.after)).toEqual(flat(applyCall.args.after));
    expect(flat(undoPatches!.before)).not.toEqual(flat(redoPatches!.after));

    // The Rust emulator holds exactly one recoverable apply (undo/redo round-trip).
    const layer = sim.store.get(sim.key(DOC, LAYER))!;
    expect(layer.undo.length).toBe(1);
    expect(layer.redo.length).toBe(0);
  });
});
