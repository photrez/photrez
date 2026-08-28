// C4 multi-stroke correctness regression (Bug 1 overlap composite + Bug 2
// cross-document seed scope). Drives the REAL production commit path via the
// hook, with `@tauri-apps/api/core` invoke mocked by an in-test Rust store
// emulator.
//
// Covers:
//   Bug 1 — every committed stroke composites onto the EXISTING canonical
//           pixels (overlapping strokes accumulate; no wipe). Verified at the
//           Rust unit level (commit_pixels_composites_onto_canonical_overlapping)
//           AND here via the emulator's faithful region-composite emulation.
//   Bug 2 — seeding state lives entirely in Rust, namespaced by (docId, layerId).
//           TS never emits rust_pixels_init; a fresh document/layer inits
//           independently and cannot inherit another doc's seeded state.
//   Plus the gating/contract scenarios from the original C4 fix:
//   1-3  fresh layer stroke A/B/C -> Rust version 1/2/3, init once
//   4    each Rust canonical state matches the applied stroke
//   5-8  unified stream undo/redo + redo truncation (sim contract)
//   9    no second init after the first seed (now: ensure-if-absent only)
//   10   a later commit failure does NOT permanently disable C4

import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { useBrushOverlay } from "../useBrushOverlay";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import * as brushToolStateModule from "../brushToolState";
import * as docModule from "@/engine/document";

// ── jsdom polyfills (mirror useBrushOverlay.tileGrad.test.ts) ──
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

// ── in-test Rust emulator (mirrors crates/core/src/pixel_store.rs semantics) ──
type WireTile = { x: number; y: number; w: number; h: number; data: number[] };
type SimLayer = { w: number; h: number; pixels: number[]; undo: any[]; redo: any[]; epoch: number; version: number };

function makeSim(opts?: { failCommitOnCall?: number }) {
  const store = new Map<string, SimLayer>();
  const calls: { cmd: string; args: any }[] = [];
  const key = (docId: string, layerId: string) => `${docId}|${layerId}`;
  let commitCount = 0;
  let initCount = 0;
  const failCommitOnCall = opts?.failCommitOnCall ?? -1;

  const write = (s: SimLayer, tiles: WireTile[]) => {
    for (const t of tiles) {
      for (let row = 0; row < t.h; row++) {
        const dst = ((t.y + row) * s.w + t.x) * 4;
        const src = row * t.w * 4;
        for (let i = 0; i < t.w * 4; i++) s.pixels[dst + i] = t.data[src + i];
      }
    }
  };
  const tileOut = (t: WireTile) => ({ x: t.x, y: t.y, w: t.w, h: t.h, data: t.data });

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
    const s = store.get(k);
    if (cmd === "paint_parity_commit") {
      commitCount += 1;
      if (commitCount === failCommitOnCall) throw new Error("simulated commit failure");
      // ensure-if-absent: init from TS bytes ONLY when Rust has no layer.
      let layer = s;
      if (!layer) {
        initCount += 1;
        layer = {
          w: args.req.w as number,
          h: args.req.h as number,
          pixels: (args.bytes as number[]).slice(),
          undo: [],
          redo: [],
          epoch: 0,
          version: 0,
        };
        store.set(k, layer);
      }
      // Emulate composite onto EXISTING canonical pixels (Bug 1 semantics).
      const w = layer.w;
      const h = layer.h;
      const b = args.req.brush as number;
      const color = (args.req.tip_color as number[]) ?? [0, 0, 0];
      const beforePx = layer.pixels.slice();
      const afterPx = layer.pixels.slice();
      let minX = w, minY = h, maxX = 0, maxY = 0;
      for (const d of args.req.dabs as { x: number; y: number }[]) {
        const x0 = Math.max(0, Math.floor(d.x - b / 2));
        const x1 = Math.min(w, Math.ceil(d.x + b / 2));
        const y0 = Math.max(0, Math.floor(d.y - b / 2));
        const y1 = Math.min(h, Math.ceil(d.y + b / 2));
        for (let y = y0; y < y1; y++) {
          for (let x = x0; x < x1; x++) {
            const i = (y * w + x) * 4;
            afterPx[i] = color[0];
            afterPx[i + 1] = color[1];
            afterPx[i + 2] = color[2];
            afterPx[i + 3] = 255;
          }
        }
        minX = Math.min(minX, x0);
        minY = Math.min(minY, y0);
        maxX = Math.max(maxX, x1);
        maxY = Math.max(maxY, y1);
      }
      const dirtyW = Math.max(0, maxX - minX);
      const dirtyH = Math.max(0, maxY - minY);
      layer.pixels = afterPx;
      layer.undo.push({ before: beforePx, after: afterPx });
      layer.redo = []; // truncate redo on new commit (unified-stream behavior)
      layer.epoch += 1;
      layer.version += 1;
      const beforeData = beforePx.slice((minY * w + minX) * 4, (minY * w + minX) * 4 + dirtyW * dirtyH * 4);
      const afterData = afterPx.slice((minY * w + minX) * 4, (minY * w + minX) * 4 + dirtyW * dirtyH * 4);
      return {
        before: [{ x: minX, y: minY, w: dirtyW, h: dirtyH, data: beforeData }],
        after: [{ x: minX, y: minY, w: dirtyW, h: dirtyH, data: afterData }],
        epoch: layer.epoch,
        version: layer.version,
      };
    }
    // Legacy unified-stream history contract (used by the standalone undo/redo test).
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
      return;
    }
    if (!s) throw new Error("no layer");
    if (cmd === "apply_tile_patch") {
      write(s, args.after as WireTile[]);
      s.undo.push({ before: args.before, after: args.after });
      s.redo = [];
      s.epoch += 1;
      s.version += 1;
      return { tiles: (args.after as WireTile[]).map(tileOut), epoch: s.epoch, version: s.version };
    }
    if (cmd === "rust_pixels_undo") {
      const e = s.undo.pop();
      if (!e) return { tiles: [], epoch: s.epoch, version: s.version, layerId: args.layerId };
      write(s, e.before as WireTile[]);
      s.redo.push(e);
      s.epoch += 1;
      s.version += 1;
      return { tiles: (e.before as WireTile[]).map(tileOut), epoch: s.epoch, version: s.version, layerId: args.layerId };
    }
    if (cmd === "rust_pixels_redo") {
      const e = s.redo.pop();
      if (!e) return { tiles: [], epoch: s.epoch, version: s.version, layerId: args.layerId };
      write(s, e.after as WireTile[]);
      s.undo.push(e);
      s.epoch += 1;
      s.version += 1;
      return { tiles: (e.after as WireTile[]).map(tileOut), epoch: s.epoch, version: s.version, layerId: args.layerId };
    }
    if (cmd === "rust_pixels_snapshot_tile") {
      const out = new Array(args.w * args.h * 4);
      for (let row = 0; row < args.h; row++) {
        const dst = row * args.w * 4;
        const src = ((args.y + row) * s.w + args.x) * 4;
        for (let i = 0; i < args.w * 4; i++) out[dst + i] = s.pixels[src + i];
      }
      return { x: args.x, y: args.y, w: args.w, h: args.h, data: out };
    }
    throw new Error("unknown cmd " + cmd);
  };

  return {
    invoke,
    store,
    calls,
    get initCount() { return initCount; },
    get commitCount() { return commitCount; },
  };
}

const hoist = vi.hoisted(() => {
  let sim: ReturnType<typeof makeSim> | null = null;
  const invoke = (cmd: string, args: any) => sim!.invoke(cmd, args);
  return { invoke, setSim: (s: ReturnType<typeof makeSim>) => { sim = s; }, getSim: () => sim! };
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: hoist.invoke }));

// ── harness (mirrors useBrushOverlay.tileGrad.test.ts) ──
function makeSurface() {
  const snapshotted: unknown[] = [];
  const restored: unknown[] = [];
  const surface: any = {
    snapshotted,
    restored,
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
    restoreTile: vi.fn((p: unknown) => restored.push(p)),
    readRect: vi.fn((_x: number, _y: number, w: number, h: number) => ({
      width: w,
      height: h,
      data: new Uint8ClampedArray(w * h * 4),
    })),
  };
  return surface;
}

function makeHarness(surface: any, layerId = "layer-1", docId = "doc-test", uploadSurfaceTiles = vi.fn()) {
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
  const doc = { id: docId };
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
  return { overlay, layer, engine, history, commit, uploadSurfaceTiles, surface, setDocId: (d: string) => { doc.id = d; } };
}

const settings = { size: 20, hardness: 1, opacity: 1, flow: 1, smoothing: 0.5 };
const DOC = "doc-test";
const LAYER = "layer-1";

describe("C4 Bug 1 — overlapping strokes composite onto canonical (real hook)", () => {
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
    localStorage.setItem("photrez.rustPixels", "1"); // C4 ON
    localStorage.removeItem("photrez.canonicalCommit"); // C3 OFF (separate mode)
  });
  afterEach(() => {
    localStorage.clear();
  });

  const pixel = (s: SimLayer, x: number, y: number) => {
    const i = (y * s.w + x) * 4;
    return [s.pixels[i], s.pixels[i + 1], s.pixels[i + 2], s.pixels[i + 3]];
  };

  it("stroke A→B→C: version 1→2→3, ensure-if-absent once, no rust_pixels_init, no C3, no duplicate render", async () => {
    const surface = makeSurface();
    const { overlay, engine, history, uploadSurfaceTiles, surface: surf } = makeHarness(surface);
    const sim = hoist.getSim();

    const stroke = async (x: number, y: number) => {
      overlay.onPaintStroke([{ x, y }], false, settings, false);
      await overlay.commitBrushStroke(engine, history as any, LAYER, false);
    };

    await stroke(30, 30);
    const entry = sim.store.get(`${DOC}|${LAYER}`);
    expect(entry, "PROBE canonical store entry").toBeTruthy();
    expect(pixel(entry!, 30, 30)).toEqual([255, 0, 0, 255]); // scenario 4: stroke reached canonical
    expect(entry!.version).toBe(1);
    expect(surf.pixelVersion).toBe(1);

    await stroke(60, 60);
    expect(pixel(sim.store.get(`${DOC}|${LAYER}`)!, 60, 60)).toEqual([255, 0, 0, 255]);
    expect(sim.store.get(`${DOC}|${LAYER}`)!.version).toBe(2);
    expect(surf.pixelVersion).toBe(2);

    await stroke(90, 90);
    expect(pixel(sim.store.get(`${DOC}|${LAYER}`)!, 90, 90)).toEqual([255, 0, 0, 255]);
    expect(sim.store.get(`${DOC}|${LAYER}`)!.version).toBe(3);
    expect(surf.pixelVersion).toBe(3);

    // scenario 1-3 + 9: ensure-if-absent ran exactly once; three commits; TS no longer emits rust_pixels_init.
    expect(sim.initCount).toBe(1);
    expect(sim.calls.filter((c) => c.cmd === "paint_parity_commit").length).toBe(3);
    expect(sim.calls.filter((c) => c.cmd === "rust_pixels_init").length).toBe(0);
    expect(sim.calls.filter((c) => c.cmd === "paint_parity_shadow").length).toBe(0);
    expect(sim.calls.filter((c) => c.cmd === "apply_tile_patch").length).toBe(0);
    // scenario "no duplicate rendering": legacy Phase-B drawImage is skipped when C4 applied.
    expect(surface.context.drawImage).not.toHaveBeenCalled();
    expect(uploadSurfaceTiles).toHaveBeenCalled();
  });

  it("Bug 1 — overlapping stroke B does NOT wipe A's pixels (both accumulate)", async () => {
    const surface = makeSurface();
    const { overlay, engine, history } = makeHarness(surface);
    const sim = hoist.getSim();

    const stroke = async (x: number, y: number) => {
      overlay.onPaintStroke([{ x, y }], false, settings, false);
      await overlay.commitBrushStroke(engine, history as any, LAYER, false);
    };

    await stroke(30, 30); // A: red region around (30,30)
    await stroke(45, 30); // B: red region around (45,30), overlaps A partially
    const entry = sim.store.get(`${DOC}|${LAYER}`)!;
    // Both stroke centers survive -> multi-stroke accumulation through paint_parity_commit.
    expect(pixel(entry, 30, 30)).toEqual([255, 0, 0, 255]);
    expect(pixel(entry, 45, 30)).toEqual([255, 0, 0, 255]);
    // A-only region (20..35) is NOT wiped by B's overlapping commit.
    expect(pixel(entry, 22, 30)).toEqual([255, 0, 0, 255]);
    expect(entry.version).toBe(2);
  });

  it("scenario 10: a later commit failure does NOT permanently disable C4 (idempotent retry, no re-seed)", async () => {
    const LAYER10 = "layer-10";
    hoist.setSim(makeSim({ failCommitOnCall: 2 })); // 2nd commit throws
    const surface = makeSurface();
    const { overlay, engine, history, surface: surf } = makeHarness(surface, LAYER10);
    const sim = hoist.getSim();

    const stroke = async (x: number, y: number) => {
      overlay.onPaintStroke([{ x, y }], false, settings, false);
      await overlay.commitBrushStroke(engine, history as any, LAYER10, false);
    };

    await stroke(30, 30); // success -> ensure-if-absent + commit #1
    expect(pixel(sim.store.get(`${DOC}|${LAYER10}`)!, 30, 30)).toEqual([255, 0, 0, 255]);
    expect(sim.store.get(`${DOC}|${LAYER10}`)!.version).toBe(1);

    await stroke(60, 60); // commit #2 FAILS -> caught, no flag to clear, legacy fallback
    expect(sim.calls.filter((c) => c.cmd === "paint_parity_commit").length).toBe(2);

    await stroke(90, 90); // C4 retries: commit #3 SUCCEEDS (idempotent, no re-seed)
    expect(sim.calls.filter((c) => c.cmd === "paint_parity_commit").length).toBe(3);
    expect(sim.initCount).toBe(1); // no re-seed after failure (Bug 2 design)
    expect(pixel(sim.store.get(`${DOC}|${LAYER10}`)!, 90, 90)).toEqual([255, 0, 0, 255]);
    expect(sim.store.get(`${DOC}|${LAYER10}`)!.version).toBe(2); // no reset -> 2
    expect(surf.pixelVersion).toBe(2);
  });
});

describe("C4 Bug 2 — cross-document seed scope (no stale seeded state)", () => {
  beforeAll(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({ confirm: vi.fn() } as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
  });
  afterAll(() => vi.restoreAllMocks());
  beforeEach(() => {
    vi.spyOn(brushToolStateModule, "getPaintToolBlockReason").mockImplementation((l: any, e: any) => null);
    vi.spyOn(docModule, "isFacadeOwnedLayer").mockImplementation((id: string) => false);
    hoist.setSim(makeSim());
    localStorage.setItem("photrez.rustPixels", "1");
    localStorage.removeItem("photrez.canonicalCommit");
  });
  afterEach(() => localStorage.clear());

  const pixel = (s: SimLayer, x: number, y: number) => {
    const i = (y * s.w + x) * 4;
    return [s.pixels[i], s.pixels[i + 1], s.pixels[i + 2], s.pixels[i + 3]];
  };

  it("doc B does not inherit doc A's seeded state; each inits independently", async () => {
    const surface = makeSurface();
    const h = makeHarness(surface, "L", "doc-A");
    const sim = hoist.getSim();

    const strokeOn = async (docId: string, x: number, y: number) => {
      h.setDocId(docId);
      h.overlay.onPaintStroke([{ x, y }], false, settings, false);
      await h.overlay.commitBrushStroke(h.engine, h.history as any, "L", false);
    };

    await strokeOn("doc-A", 30, 30);
    expect(pixel(sim.store.get("doc-A|L")!, 30, 30)).toEqual([255, 0, 0, 255]);
    expect(sim.store.get("doc-A|L")!.version).toBe(1);

    // close doc A (releases Rust store entry)
    await sim.invoke("rust_pixels_close_document", { docId: "doc-A" });
    expect(sim.store.get("doc-A|L")).toBeUndefined();

    // doc B: equivalent layer identity "L" must init FRESH, not inherit A's seeded state.
    await strokeOn("doc-B", 30, 30);
    const b = sim.store.get("doc-B|L")!;
    expect(b, "doc-B layer initialized independently").toBeTruthy();
    expect(b.version).toBe(1); // fresh, not carried over from doc A
    expect(pixel(b, 30, 30)).toEqual([255, 0, 0, 255]);

    // TS never emits rust_pixels_init (Bug 2: no TS seeded flag at all).
    expect(sim.calls.filter((c) => c.cmd === "rust_pixels_init").length).toBe(0);
    // exactly one ensure-if-absent per document.
    expect(sim.initCount).toBe(2);
    // doc B first stroke committed via paint_parity_commit (not the old shadow/delta path).
    expect(sim.calls.filter((c) => c.cmd === "paint_parity_commit").length).toBe(2);
  });
});

describe("C4 unified-stream undo/redo contract (sim)", () => {
  beforeEach(() => {
    hoist.setSim(makeSim());
    localStorage.clear();
  });

  const strokeArgs = (before: number, after: number) => ({
    docId: "d",
    layerId: "L",
    before: [{ x: 0, y: 0, w: 10, h: 10, data: new Array(400).fill(before) }],
    after: [{ x: 0, y: 0, w: 10, h: 10, data: new Array(400).fill(after) }],
  });

  it("A→B→C→undo→undo→redo→new truncates redo (scenarios 5-8)", async () => {
    const sim = hoist.getSim();
    await sim.invoke("rust_pixels_open_document", { docId: "d" });
    await sim.invoke("rust_pixels_init", { docId: "d", layerId: "L", width: 10, height: 10, bytes: new Array(400).fill(0) });

    // A 0→11, B 11→22, C 22→33
    expect((await sim.invoke("apply_tile_patch", strokeArgs(0, 11))).version).toBe(1);
    expect((await sim.invoke("apply_tile_patch", strokeArgs(11, 22))).version).toBe(2);
    expect((await sim.invoke("apply_tile_patch", strokeArgs(22, 33))).version).toBe(3);

    let snap = await sim.invoke("rust_pixels_snapshot_tile", { docId: "d", layerId: "L", x: 0, y: 0, w: 10, h: 10 });
    expect(snap.data[0]).toBe(33);

    // scenario 5: undo -> B (22)
    const u1 = await sim.invoke("rust_pixels_undo", { docId: "d", layerId: "L" });
    expect(u1.version).toBe(4);
    expect(u1.tiles[0].data[0]).toBe(22);
    // scenario 6: undo -> A (11)
    const u2 = await sim.invoke("rust_pixels_undo", { docId: "d", layerId: "L" });
    expect(u2.version).toBe(5);
    expect(u2.tiles[0].data[0]).toBe(11);
    // scenario 7: redo -> B (22)
    const r1 = await sim.invoke("rust_pixels_redo", { docId: "d", layerId: "L" });
    expect(r1.version).toBe(6);
    expect(r1.tiles[0].data[0]).toBe(22);

    // scenario 8: new stroke after undo truncates redo
    const nw = await sim.invoke("apply_tile_patch", strokeArgs(22, 44));
    expect(nw.version).toBe(7);
    const r2 = await sim.invoke("rust_pixels_redo", { docId: "d", layerId: "L" });
    expect(r2.tiles.length).toBe(0); // redo stack truncated
    snap = await sim.invoke("rust_pixels_snapshot_tile", { docId: "d", layerId: "L", x: 0, y: 0, w: 10, h: 10 });
    expect(snap.data[0]).toBe(44);
  });
});
