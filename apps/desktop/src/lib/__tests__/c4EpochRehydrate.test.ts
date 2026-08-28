// C5.2 — derived TS cache epoch validation + rehydration from Rust canonical.
// Mocks @tauri-apps/api/core so the production rehydrate path (which uses
// getInvoke()/dynamic import) exercises the in-test Rust sim.

import { describe, it, expect, vi, beforeEach } from "vitest";

let sim: ReturnType<typeof makeRustSim>;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args: any) => sim.invoke(cmd, args),
}));

import { rehydratePaintSurfaceFromRust, getRustEpoch } from "../rustShadow";

class FakeImageData {
  width: number;
  height: number;
  data: Uint8ClampedArray;
  constructor(data: Uint8ClampedArray, width: number, height: number) {
    this.data = data;
    this.width = width;
    this.height = height;
  }
}

type WireTile = { x: number; y: number; w: number; h: number; data: number[] };

function makeRustSim() {
  const store = new Map<string, { w: number; h: number; pixels: number[]; undo: any[]; redo: any[]; epoch: number }>();
  const key = (docId: string, layerId: string) => `${docId}|${layerId}`;
  const write = (s: any, tiles: WireTile[]) => {
    for (const t of tiles) {
      for (let row = 0; row < t.h; row++) {
        const dst = ((t.y + row) * s.w + t.x) * 4;
        const src = row * t.w * 4;
        for (let i = 0; i < t.w * 4; i++) s.pixels[dst + i] = t.data[src + i];
      }
    }
  };
  const invoke = async (cmd: string, args: any): Promise<any> => {
    if (cmd === "rust_pixels_open_document") return;
    if (cmd === "rust_pixels_init") {
      store.set(key(args.docId, args.layerId), {
        w: args.width, h: args.height, pixels: (args.bytes as number[]).slice(),
        undo: [], redo: [], epoch: 0,
      });
      return;
    }
    const s = store.get(key(args.docId, args.layerId))!;
    if (cmd === "apply_tile_patch") {
      write(s, args.after as WireTile[]);
      s.undo.push({ before: args.before, after: args.after });
      s.redo = [];
      s.epoch += 1;
      return { tiles: (args.after as WireTile[]).map((t) => ({ x: t.x, y: t.y, w: t.w, h: t.h, data: t.data })), epoch: s.epoch };
    }
    if (cmd === "rust_pixels_snapshot_layer") {
      const TW = 256;
      const cols = Math.ceil(s.w / TW);
      const rows = Math.ceil(s.h / TW);
      const out: WireTile[] = [];
      for (let ty = 0; ty < rows; ty++) {
        for (let tx = 0; tx < cols; tx++) {
          const x = tx * TW, y = ty * TW;
          const w = Math.min(TW, s.w - x), h = Math.min(TW, s.h - y);
          const data = new Array(w * h * 4);
          for (let row = 0; row < h; row++) {
            const dst = row * w * 4;
            const src = ((y + row) * s.w + x) * 4;
            for (let i = 0; i < w * 4; i++) data[dst + i] = s.pixels[src + i];
          }
          out.push({ x, y, w, h, data });
        }
      }
      return out;
    }
    if (cmd === "rust_pixels_get_epoch") return s.epoch;
    throw new Error("unknown cmd " + cmd);
  };
  return { invoke, store };
}

function makeFakeCtx(w: number, h: number) {
  const backing = new Uint8ClampedArray(w * h * 4);
  const ctx = {
    putImageData(img: { width: number; height: number; data: Uint8ClampedArray }, x: number, y: number) {
      for (let row = 0; row < img.height; row++) {
        const dst = ((y + row) * w + x) * 4;
        const src = row * img.width * 4;
        for (let i = 0; i < img.width * 4; i++) backing[dst + i] = img.data[src + i];
      }
    },
  };
  return { ctx, backing };
}

const DOC = "doc1";
const LAYER = "L1";
const W = 512, H = 512;

describe("C5.2 — derived TS cache epoch rehydration", () => {
  beforeEach(() => {
    localStorage.clear();
    (globalThis as any).ImageData = FakeImageData;
    sim = makeRustSim();
  });

  it("rehydrates a stale cache from the Rust canonical epoch", async () => {
    const { ctx, backing } = makeFakeCtx(W, H);
    const surface = { context: ctx as any, pixelEpoch: 0 };

    await sim.invoke("rust_pixels_init", { docId: DOC, layerId: LAYER, width: W, height: H, bytes: new Array(W * H * 4).fill(255) });
    // One commit on (0,0) → Rust epoch becomes 1; TS cache is still at 0 (stale).
    const after = new Uint8ClampedArray(256 * 256 * 4).fill(42);
    await sim.invoke("apply_tile_patch", {
      docId: DOC, layerId: LAYER,
      before: [{ x: 0, y: 0, w: 256, h: 256, data: new Array(256 * 256 * 4).fill(255) }],
      after: [{ x: 0, y: 0, w: 256, h: 256, data: Array.from(after) }],
    });
    expect(await getRustEpoch(DOC, LAYER)).toBe(1);

    const did = await rehydratePaintSurfaceFromRust(DOC, LAYER, surface as any);
    expect(did).toBe(true);
    expect(surface.pixelEpoch).toBe(1);
    // Derived cache now equals the canonical Rust buffer at (0,0).
    expect(backing[0]).toBe(42);
  });

  it("skips rehydration when the cache epoch already matches", async () => {
    const { ctx, backing } = makeFakeCtx(W, H);
    const surface = { context: ctx as any, pixelEpoch: 1 }; // already fresh

    await sim.invoke("rust_pixels_init", { docId: DOC, layerId: LAYER, width: W, height: H, bytes: new Array(W * H * 4).fill(255) });
    await sim.invoke("apply_tile_patch", {
      docId: DOC, layerId: LAYER,
      before: [{ x: 0, y: 0, w: 256, h: 256, data: new Array(256 * 256 * 4).fill(255) }],
      after: [{ x: 0, y: 0, w: 256, h: 256, data: new Array(256 * 256 * 4).fill(13) }],
    });
    // Pre-seed the cache with the matching pixel so a no-op is observable.
    backing.fill(13);

    const did = await rehydratePaintSurfaceFromRust(DOC, LAYER, surface as any);
    expect(did).toBe(false);
    expect(surface.pixelEpoch).toBe(1);
  });

  it("does nothing when Rust has no canonical storage for the layer", async () => {
    const { ctx } = makeFakeCtx(W, H);
    const surface = { context: ctx as any, pixelEpoch: 0 };
    // No rust_pixels_init for this layer → get_epoch throws → null → false.
    const did = await rehydratePaintSurfaceFromRust(DOC, "missing", surface as any);
    expect(did).toBe(false);
  });
});
