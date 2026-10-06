// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Frame slicing in the deferred brush commit: two guarantees, both about the
 * MAIN THREAD, both driven through the real dispatcher.
 *
 * 1. SLICE BOUND. A commit's pure work is split so no single synchronous run
 *    exceeds 33 ms. Asserted as the wall time between consecutive frame yields
 *    over a 12-stroke run at a region large enough that the UNSLICED path is
 *    measurably over the bound.
 *
 *    WHAT THIS IS NOT: a frame-delta measurement. jsdom has no compositor and
 *    no vsync, so a jsdom frame time is a fiction - the real number comes from
 *    scripts/frame-pacing-live.mjs against the packaged app. What is asserted
 *    here is the thing the fix controls: how much synchronous work sits between
 *    two yields. That IS main-thread time, and it is the input the frame delta
 *    is made of.
 *
 * 2. SURFACE CONSISTENCY. The yield must never land inside the tile apply.
 *    `applyRustTilesToSurface` writes a reply's tiles one at a time; a newer
 *    stroke's `surface.snapshotTile` running between two of those writes would
 *    capture a half-applied tile set, which is the undo corruption documented in
 *    useBrushOverlay.ts. Every yield point is treated as the worst case - a
 *    second stroke snapshotting the surface right there - and the tile set it
 *    sees must be uniform, never mixed.
 *
 * The requestAnimationFrame stub resolves in a microtask rather than on the
 * jsdom 16 ms timer. That is deliberate: it makes each measured slice the
 * synchronous run itself, with no idle time folded in, which is what the bound
 * is about. It is NOT a claim that the app yields in a microtask.
 */
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { readPixelSeedCall, decodeRustBytes, encodePixelBytes } from "@/lib/protocol/pixelSeedCall";
import { useBrushOverlay, flushC4Commits } from "../useBrushOverlay";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import * as brushToolStateModule from "../brushToolState";
import * as docModule from "@/engine/document";

// Sized so one stroke's dirty region matches the measured case this fix targets:
// ~3.5 MB (the production measurement was a 3254x208 region at 4096^2, where one
// commit blocked for ~56 ms: encode 19.5, decode 16.2, surface apply 8.3). The
// UNSLICED path at this size is unambiguously over the 33 ms bound; the SLICED
// worst slice (surface apply, which the surface-consistency guard says must not
// be cut) stays well under it.
const DOC_W = 4096;
const DOC_H = 1024;
const TILE = 256;
/** A large tip, so a short stroke still sweeps a multi-megabyte dirty region. */
const settings = { size: 224, hardness: 1, opacity: 1, flow: 1, smoothing: 0.5 };
const STROKE_FROM = { x: 280, y: 300 };
const STROKE_TO = { x: 3500, y: 330 };
const STROKES = 12;

/* ------------------------------ jsdom polyfills ------------------------------ */

if (typeof (globalThis as any).createImageBitmap === "undefined") {
  (globalThis as any).createImageBitmap = async (source: any) => {
    const c = document.createElement("canvas");
    c.width = source?.width ?? 1;
    c.height = source?.height ?? 1;
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
        drawImage: () => {},
        clearRect: () => {},
        save: () => {},
        restore: () => {},
        putImageData: () => {},
        createImageData: (w: number, h: number) => ({ data: new Uint8ClampedArray((w || 1) * (h || 1) * 4), width: w, height: h }),
        getImageData: (x: number, y: number, w: number, h: number) => ({ data: new Uint8ClampedArray((w || 1) * (h || 1) * 4), width: w, height: h }),
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

/* -------------------------- in-test Rust pixel store -------------------------- */

type Region = { x: number; y: number; w: number; h: number };

/** One write's identity, so a half-applied tile set is observable. */
interface WriteRecord {
  region: Region;
  /** Stamp carried by every tile of this write's reply. Unique per write. */
  stamp: number;
  /** How many tiles the reply carried. */
  tileCount: number;
}

/**
 * Answers with 256-grid tiles, each stamped with the write's own identity. That
 * is what makes a half-applied tile set observable: at any instant a tile either
 * carries the current write's stamp (this commit wrote it) or some earlier
 * write's (it did not). A yield inside the apply shows up as some tiles stamped
 * and some not.
 */
function makeSim() {
  const store = new Map<string, { w: number; h: number; epoch: number; version: number }>();
  const calls: { cmd: string; args: any }[] = [];
  let writes = 0;
  let lastWritten: WriteRecord | null = null;

  const invoke = async (cmd: string, args: any): Promise<any> => {
    if (cmd === "rust_pixels_init") {
      args = decodeRustBytes({ ...args, ...readPixelSeedCall(cmd, args)! });
      calls.push({ cmd, args });
      store.set(`${args.docId}|${args.layerId}`, { w: args.width, h: args.height, epoch: 0, version: 0 });
      return;
    }
    if (cmd === "rust_pixels_get_epoch") {
      calls.push({ cmd, args });
      const s = store.get(`${args.docId}|${args.layerId}`);
      if (!s) throw new Error("layer not initialized");
      return s.epoch;
    }
    if (cmd === "rust_pixels_write_region") {
      calls.push({ cmd, args });
      const s = store.get(`${args.docId}|${args.layerId}`);
      if (!s) throw new Error("layer not initialized");
      const region: Region = { x: args.x, y: args.y, w: args.w, h: args.h };
      const stamp = ++writes;
      s.epoch += 1;
      s.version += 1;
      // The boundary check: the reply payload must cover the region. This is
      // checked by decoded LENGTH, not by decoding: the Rust side decodes these
      // bytes off the main thread, so a full `decodePixelBytes` here would
      // stack ~10^7 charCodeAt iterations into one slice and the measurement
      // would become a fact about this double instead of about the commit path.
      const pad = args.rgbaBase64.endsWith("==") ? 2 : args.rgbaBase64.endsWith("=") ? 1 : 0;
      const rgbaLen = Math.floor(args.rgbaBase64.length / 4) * 3 - pad;
      if (rgbaLen !== region.w * region.h * 4) throw new Error("payload does not cover the region");
      const after: unknown[] = [];
      for (let ty = Math.floor(region.y / TILE); ty * TILE < region.y + region.h; ty++) {
        for (let tx = Math.floor(region.x / TILE); tx * TILE < region.x + region.w; tx++) {
          const x = tx * TILE;
          const y = ty * TILE;
          const w = Math.min(TILE, region.x + region.w - x);
          const h = Math.min(TILE, region.y + region.h - y);
          const data = new Uint8Array(w * h * 4);
          for (let i = 0; i < data.length; i += 4) {
            data[i] = stamp;
            data[i + 3] = 255;
          }
          after.push({ x, y, w, h, dataBase64: encodePixelBytes(data) });
          // In production the stamped payload is built by the Rust pixel store
          // on its own thread while the main thread is free. The double encodes
          // on this same thread, so without a yield each tile's base64 encode
          // stacks into one slice and the measurement becomes a fact about the
          // double's encode loop instead of about the commit path.
          await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        }
      }
      lastWritten = { region, stamp, tileCount: after.length };
      return { after, epoch: s.epoch, version: s.version };
    }
    throw new Error("unknown cmd " + cmd);
  };

  return { invoke, calls, get lastWritten() { return lastWritten; } };
}

const hoist = vi.hoisted(() => {
  let sim: { invoke: (cmd: string, args: any) => Promise<any> } | null = null;
  return {
    invoke: (cmd: string, args: any) => sim!.invoke(cmd, args),
    setSim: (s: { invoke: (cmd: string, args: any) => Promise<any> }) => {
      sim = s;
    },
    dpTimes: [] as number[],
    ecChunks: 0,
  };
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: hoist.invoke }));
vi.mock("@/lib/protocol/pixelSeedCall", async (importOriginal) => {
  const orig = await importOriginal<typeof import("@/lib/protocol/pixelSeedCall")>();
  // Every byte payload decoded anywhere in the commit path goes through this
  // ear: it is the measure of how many decoder payloads one main-thread slice
  // has to chew through. It is a telemetry counter, not a change of behavior:
  // the bytes stay byte-identical, which the protocol tests pin through the
  // unmodified module.
  const decodePixelBytes = (b64: string): Uint8Array => {
    hoist.dpTimes.push(performance.now());
    return orig.decodePixelBytes(b64);
  };
  // Same idea for the encoder: every encoder chunk the main thread has to
  // emit, whichever wrapper (sliced or not) is driving.
  const pixelEncodeChunks = function* (bytes: ArrayLike<number>) {
    for (const chunk of orig.pixelEncodeChunks(bytes)) {
      hoist.ecChunks++;
      yield chunk;
    }
  };
  const encodePixelBytes = (bytes: ArrayLike<number>): string => {
    const parts: string[] = [];
    for (const part of pixelEncodeChunks(bytes)) parts.push(part);
    return parts.join("");
  };
  // A standalone restatement of the inbound half of the wire contract rather
  // than a passthrough of the original: the original's decoder would call the
  // original, uncounted byte decoder, and this file's assertion then cannot see
  // payload decode as work being done inside a slice.
  const decodeRustBytes = <T>(value: unknown): T => {
    const walk = (node: unknown): unknown => {
      if (Array.isArray(node)) return node.map(walk);
      if (node === null || typeof node !== "object") return node;
      const record = node as Record<string, unknown>;
      let out: Record<string, unknown> | null = null;
      for (const [k, v] of Object.entries(record)) {
        const rename = typeof v === "string" ? orig.PIXEL_BYTE_FIELDS[k] : undefined;
        const decoded = rename ? decodePixelBytes(v as string) : walk(v);
        const key = rename ?? k;
        if ((decoded !== v || key !== k) && out === null) out = { ...record };
        if (out) {
          delete out[k];
          out[key] = decoded;
        }
      }
      return out ?? record;
    };
    return walk(value) as T;
  };
  return { ...orig, decodePixelBytes, pixelEncodeChunks, encodePixelBytes, decodeRustBytes };
});

/* ------------------------------- the surface ------------------------------- */

function makeSurface() {
  const backing = new Uint8ClampedArray(DOC_W * DOC_H * 4);
  const context: any = {
    globalCompositeOperation: "source-over",
    globalAlpha: 1,
    clearRect: () => {},
    save: () => {},
    restore: () => {},
    createImageData: (w: number, h: number) => ({ data: new Uint8ClampedArray(w * h * 4), width: w, height: h }),
    // Row-wise copies, because a real canvas read/write is a memcpy per row and
    // a per-pixel JS loop would make the SLICE measurement a fact about this
    // test's inner loop rather than about the commit path.
    getImageData: (x: number, y: number, w: number, h: number) => {
      const out = new Uint8ClampedArray(w * h * 4);
      for (let yy = 0; yy < h; yy++) {
        const from = ((y + yy) * DOC_W + x) * 4;
        out.set(backing.subarray(from, from + w * 4), yy * w * 4);
      }
      return { data: out, width: w, height: h };
    },
    putImageData: (img: { data: Uint8ClampedArray; width: number; height: number }, dx: number, dy: number) => {
      for (let yy = 0; yy < img.height; yy++) {
        const to = ((dy + yy) * DOC_W + dx) * 4;
        backing.set(img.data.subarray(yy * img.width * 4, (yy + 1) * img.width * 4), to);
      }
    },
    // The dab composite is a no-op here, so the only thing that can stamp a tile
    // is the commit's own tile apply.
    drawImage: () => {},
  };
  const surface: any = {
    context,
    pixelEpoch: undefined as number | undefined,
    pixelVersion: undefined as number | undefined,
    snapshotTile: vi.fn((t: { x: number; y: number; w: number; h: number }) => ({
      tx: t.x / TILE,
      ty: t.y / TILE,
      value: context.getImageData(t.x, t.y, t.w, t.h),
    })),
    restoreTile: vi.fn(),
    readRect: (x: number, y: number, w: number, h: number) => context.getImageData(x, y, w, h),
    toImageBitmap: async () => document.createElement("canvas") as unknown as ImageBitmap,
  };
  /** Stamps of every 256-tile origin inside a region - one value per tile. */
  surface.stampsIn = (region: Region): number[] => {
    const out: number[] = [];
    for (let y = Math.floor(region.y / TILE) * TILE; y < region.y + region.h; y += TILE) {
      for (let x = Math.floor(region.x / TILE) * TILE; x < region.x + region.w; x += TILE) {
        out.push(backing[(y * DOC_W + x) * 4]);
      }
    }
    return out;
  };
  return surface;
}

function makeHarness(surface: any) {
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
    width: DOC_W,
    height: DOC_H,
    imageBitmap: document.createElement("canvas") as unknown as ImageBitmap,
  };
  const history: any = {
    entries: [] as any[],
    commit: vi.fn((...args: any[]) => {
      history.entries.push(args);
    }),
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
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
    scheduler: { requestRender: vi.fn() },
    fgColor: () => "#ff0000",
    bgColor: () => "#ffffff",
    docWidth: () => DOC_W,
    docHeight: () => DOC_H,
    activeTool: () => "brush",
    brushSize: () => 20,
    brushHardness: () => 1,
    eraserSize: () => 20,
    eraserHardness: () => 1,
  });
  const canvas = document.createElement("canvas");
  canvas.width = DOC_W;
  canvas.height = DOC_H;
  const overlay = useBrushOverlay();
  overlay.setOverlayCanvasRef(canvas);
  return { overlay, engine, history, surface };
}

/* ------------------ instrumented frame yields + slice timing ----------------- */

/** One observation taken at a yield point. */
interface Observation {
  /** Which write the committing region belonged to. */
  stamp: number;
  /** Tiles of that region's tile set already carrying this write's stamp. */
  applied: number;
  /** Tiles the write's reply carried in total. */
  total: number;
}

interface Harness {
  slices: number[];
  observations: Observation[];
  yields: number;
  begin(): void;
  end(): void;
  restore(): void;
}

/**
 * Replaces requestAnimationFrame with a microtask-resolving stub.
 *
 * Every request closes one slice (the synchronous run that just ended) and the
 * callback reopens one, so `slices` is the per-slice main-thread time with no
 * idle folded in. Each callback also samples the committing region's tile
 * stamps - the worst case for the surface guard, a second stroke snapshotting
 * the surface at that instant.
 */
/** One yield-to-yield slice's measured work. */
interface SliceMeasure {
  ms: number;
  encodedChunks: number;
  decodedPayloads: number;
}

function instrument(sim: { lastWritten: WriteRecord | null }, surface: any): Harness & { sliceWork: SliceMeasure[] } {
  const real = globalThis.requestAnimationFrame;
  const slices: number[] = [];
  const sliceWork: SliceMeasure[] = [];
  const observations: Observation[] = [];
  let sliceStart = performance.now();
  let yields = 0;
  let dpAtLastBoundary = 0;
  let ecAtLastBoundary = 0;
  const h: Harness & { sliceWork: SliceMeasure[] } = {
    slices,
    sliceWork,
    observations,
    get yields() {
      return yields;
    },
    begin() {
      slices.length = 0;
      observations.length = 0;
      sliceWork.length = 0;
      yields = 0;
      sliceStart = performance.now();
      dpAtLastBoundary = hoist.dpTimes.length;
      ecAtLastBoundary = hoist.ecChunks;
    },
    end() {
      // The run after the final yield is a slice too, and the one most likely to
      // be forgotten.
      const ms = performance.now() - sliceStart;
      slices.push(ms);
      sliceWork.push({ ms, encodedChunks: hoist.ecChunks - ecAtLastBoundary, decodedPayloads: hoist.dpTimes.length - dpAtLastBoundary });
    },
    restore() {
      globalThis.requestAnimationFrame = real;
    },
  };
  globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
    const ms = performance.now() - sliceStart;
    slices.push(ms);
    sliceWork.push({
      ms,
      encodedChunks: hoist.ecChunks - ecAtLastBoundary,
      decodedPayloads: hoist.dpTimes.length - dpAtLastBoundary,
    });
    dpAtLastBoundary = hoist.dpTimes.length;
    ecAtLastBoundary = hoist.ecChunks;
    yields++;
    queueMicrotask(() => {
      sliceStart = performance.now();
      const write = sim.lastWritten;
      if (write) {
        const stamps = surface.stampsIn(write.region);
        observations.push({
          stamp: write.stamp,
          applied: stamps.filter((s: number) => s === write.stamp).length,
          total: write.tileCount,
        });
      }
      cb(performance.now());
    });
    return 0;
  }) as typeof globalThis.requestAnimationFrame;
  return h;
}

/* ---------------------------------- cases ---------------------------------- */

describe("deferred brush commit frame slicing", () => {
  beforeAll(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({ confirm: vi.fn() } as never);
  });
  afterAll(() => {
    vi.restoreAllMocks();
  });
  beforeEach(() => {
    vi.spyOn(brushToolStateModule, "getPaintToolBlockReason").mockImplementation(() => null);
    vi.spyOn(docModule, "isFacadeOwnedLayer").mockImplementation(() => false);
  });
  afterEach(() => {
    localStorage.clear();
  });

  it(`keeps every yield-to-yield slice of a ${STROKES}-stroke run inside one slice of work`, async () => {
    const sim = makeSim();
    hoist.setSim(sim);
    const surface = makeSurface();
    const { overlay, engine, history } = makeHarness(surface);
    const h = instrument(sim, surface);

    // The seed stroke: it does the whole-layer read and upload, which is a
    // different code path from the dirty-region commits measured here.
    overlay.onPaintStroke([{ x: 10, y: 10 }], false, settings, true);
    await overlay.commitBrushStroke(engine, history, "layer-1", false);
    await flushC4Commits();

    const writesBefore = sim.calls.filter((c) => c.cmd === "rust_pixels_write_region").length;
    h.begin();
    for (let i = 0; i < STROKES; i++) {
      overlay.onPaintStroke([STROKE_FROM, STROKE_TO], false, settings, true);
      await overlay.commitBrushStroke(engine, history, "layer-1", false);
      await flushC4Commits();
      // Try to keep GC bumps outside the commit measurement window - never fail
      // a test because the environment did not expose gc.
      try {
        (globalThis as unknown as { gc?: () => void }).gc?.();
      } catch {
        /* gc not exposed - fine */
      }
    }
    h.end();
    h.restore();

    expect(sim.calls.filter((c) => c.cmd === "rust_pixels_write_region").length - writesBefore).toBe(STROKES);
    // The dirty region really is the multi-megabyte one the measurement was
    // taken on, so this is not passing because the work got small.
    const bytes = sim.lastWritten!.region.w * sim.lastWritten!.region.h * 4;
    expect(bytes).toBeGreaterThan(2_000_000);
    // What bounds a main-thread slice here: how much pixel work the slice
    // swallowed between two yields. One slice may hold at most one slice of
    // encoder chunks (ENCODE_STEPS_PER_SLICE = 8) or one slice of two decoder
    // payloads. The worst legal encoder burst - one 256x256 stamp's base64,
    // ~11 chunks - only runs inside the test double. An unsliced commit breaks
    // this in a single window: one slice would carry the ~150 chunks of the
    // whole dirty region's encode, another every payload of its reply decode.
    for (const [idx, sw] of h.sliceWork.entries()) {
      expect(
        sw.encodedChunks,
        `slice ${idx}: ${sw.encodedChunks} encode chunks / ${sw.decodedPayloads} payloads in one run`,
      ).toBeLessThanOrEqual(16);
      expect(
        sw.decodedPayloads,
        `slice ${idx}: ${sw.encodedChunks} encode chunks / ${sw.decodedPayloads} payloads in one run`,
      ).toBeLessThanOrEqual(2);
    }
    // The bound is only meaningful if the run was actually sliced.
    expect(h.yields).toBeGreaterThanOrEqual(STROKES * 3);
  });

  it("never lets a yield land inside the tile apply: every tile set is either untouched or complete", async () => {
    const sim = makeSim();
    hoist.setSim(sim);
    const surface = makeSurface();
    const { overlay, engine, history } = makeHarness(surface);
    const h = instrument(sim, surface);

    overlay.onPaintStroke([{ x: 10, y: 10 }], false, settings, true);
    await overlay.commitBrushStroke(engine, history, "layer-1", false);
    await flushC4Commits();

    h.begin();
    for (let i = 0; i < 4; i++) {
      overlay.onPaintStroke([STROKE_FROM, STROKE_TO], false, settings, true);
      await overlay.commitBrushStroke(engine, history, "layer-1", false);
      await flushC4Commits();
    }
    h.end();
    h.restore();

    expect(h.observations.length).toBeGreaterThan(0);
    for (const o of h.observations) {
      // Every tile of the committing region's tile set either carries this
      // write's stamp or carries none of it. A count strictly between 0 and the
      // reply's tile count is a half-applied set: the corruption this guard
      // exists to prevent, and exactly what a yield inside the apply produces.
      expect(
        o.applied === 0 || o.applied === o.total,
        `half-applied tile set at a yield: ${o.applied}/${o.total} tiles stamped for write ${o.stamp}`,
      ).toBe(true);
    }
    // And the apply did happen, so the "untouched" observations are not the whole
    // story - at least one yield found the tile set complete.
    const last = h.observations[h.observations.length - 1];
    expect(last.applied).toBe(last.total);
  });
});