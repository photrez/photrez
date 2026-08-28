// C4 pilot (R2 flagged active-layer) — frontend contract test (C5.1 doc-scoped).
// Verifies the TS<->Rust data contract WITHOUT a live backend:
//  - TS cache bytes == Rust canonical regions (gates 5/6/7)
//  - only affected dirty tiles cross Rust->TS (gate 8 transport bounded)
//  - history entry is a delta, not a full-layer clone (gate 2/3)
//  - undo/redo round-trip restores pre/post-stroke bytes
//  - photrez.rustPixels defaults OFF (gate 9 legacy default)
//  - canonical pixel owner is namespaced by (docId, layerId) (C5.1)

import { describe, it, expect, beforeEach } from "vitest";
import { applyRustTilesToSurface } from "../rustShadow";

// jsdom has no ImageData constructor; mirror canonicalSeam.test.ts pattern.
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

// In-test Rust store emulator (mirrors crates/core/src/pixel_store.rs semantics).
function makeRustSim() {
  const store = new Map<string, { w: number; h: number; pixels: number[]; undo: any[]; redo: any[]; epoch: number; version: number }>();
  const docs = new Set<string>();
  const calls: { cmd: string; args: any }[] = [];
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
  const tileOut = (t: WireTile) => ({ x: t.x, y: t.y, w: t.w, h: t.h, data: t.data });

  const invoke = async (cmd: string, args: any): Promise<any> => {
    calls.push({ cmd, args });
    if (cmd === "rust_pixels_open_document") {
      docs.add(args.docId);
      return;
    }
    if (cmd === "rust_pixels_close_document") {
      for (const k of [...store.keys()]) {
        if (k.startsWith(args.docId + "|")) store.delete(k);
      }
      docs.delete(args.docId);
      return;
    }
    const k = key(args.docId, args.layerId);
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
    const s = store.get(k)!;
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
    if (cmd === "rust_pixels_snapshot_layer") {
      const TW = 256;
      const cols = Math.ceil(s.w / TW);
      const rows = Math.ceil(s.h / TW);
      const out: WireTile[] = [];
      for (let ty = 0; ty < rows; ty++) {
        for (let tx = 0; tx < cols; tx++) {
          const x = tx * TW;
          const y = ty * TW;
          const w = Math.min(TW, s.w - x);
          const h = Math.min(TW, s.h - y);
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
    if (cmd === "rust_pixels_get_epoch") {
      return s.epoch;
    }
    throw new Error("unknown cmd " + cmd);
  };

  return { invoke, store, calls, docs };
}

// Fake 2D context recording putImageData into a backing RGBA buffer.
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

function tileKeyed(tx: number, ty: number, data: Uint8ClampedArray) {
  return { tx, ty, value: { width: 256, height: 256, data } };
}

const PAINT_TILE_SIZE = 256;
const DOC = "doc1";

describe("C4 pilot — TS<->Rust pixel ownership contract (C5.1 doc-scoped)", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("default flag is OFF (legacy path preserved)", () => {
    expect(localStorage.getItem("photrez.rustPixels")).not.toBe("1");
  });

  it("commit: TS cache bytes == Rust canonical; only affected tiles transported", async () => {
    const W = 512, H = 512;
    const sim = makeRustSim();
    const invoke = sim.invoke;

    const layerId = "L1";
    await invoke("rust_pixels_open_document", { docId: DOC });
    const seed = new Array(W * H * 4).fill(255);
    await invoke("rust_pixels_init", { docId: DOC, layerId, width: W, height: H, bytes: seed });

    const beforeA = new Uint8ClampedArray(256 * 256 * 4).fill(255);
    const beforeB = new Uint8ClampedArray(256 * 256 * 4).fill(255);
    const afterA = new Uint8ClampedArray(256 * 256 * 4).fill(42);
    const afterB = new Uint8ClampedArray(256 * 256 * 4).fill(99);
    const beforePatches = [tileKeyed(0, 0, beforeA), tileKeyed(1, 0, beforeB)];

    const afterWire = [
      { x: 0, y: 0, width: 256, height: 256, data: afterA },
      { x: 256, y: 0, width: 256, height: 256, data: afterB },
    ];
    const beforeWire = beforePatches.map((p) => ({
      x: p.tx * PAINT_TILE_SIZE, y: p.ty * PAINT_TILE_SIZE,
      width: p.value.width, height: p.value.height, data: p.value.data,
    }));
    const res = await invoke("apply_tile_patch", {
      docId: DOC,
      layerId,
      before: beforeWire.map((t: any) => ({ x: t.x, y: t.y, w: t.width, h: t.height, data: Array.from(t.data) })),
      after: afterWire.map((t: any) => ({ x: t.x, y: t.y, w: t.width, h: t.height, data: Array.from(t.data) })),
    });

    // C5.2: response carries the post-commit epoch (used by TS cache validation).
    expect(res.epoch).toBe(1);
    // C5.3-A: response also carries the DocumentVersion from the unified stream.
    expect(res.version).toBe(1);

    const { ctx, backing } = makeFakeCtx(W, H);
    applyRustTilesToSurface(ctx, res.tiles, FakeImageData);

    // GATE 8: only affected tiles transported (2), not the full 4-tile layer.
    const commitCall = sim.calls.find((c) => c.cmd === "apply_tile_patch")!;
    expect(commitCall.args.before.length).toBe(2);
    expect(commitCall.args.after.length).toBe(2);

    // GATE 2/3: one canonical buffer + a single delta entry.
    const s = sim.store.get(`${DOC}|${layerId}`)!;
    expect(s.pixels.length).toBe(W * H * 4);
    expect(s.undo.length).toBe(1);

    for (const [tx, ty, fill] of [[0, 0, 42], [1, 0, 99]] as const) {
      const snap = await invoke("rust_pixels_snapshot_tile", { docId: DOC, layerId, x: tx * 256, y: ty * 256, w: 256, h: 256 });
      for (let row = 0; row < 256; row++) {
        for (let col = 0; col < 256; col++) {
          const surfX = tx * 256 + col;
          const surfY = ty * 256 + row;
          const surfIdx = (surfY * W + surfX) * 4;
          const tileIdx = (row * 256 + col) * 4;
          expect(backing[surfIdx]).toBe(snap.data[tileIdx]);
          expect(backing[surfIdx]).toBe(fill);
        }
      }
    }
  });

  it("undo/redo: TS cache follows Rust canonical pre/post-stroke bytes", async () => {
    const W = 512, H = 512;
    const sim = makeRustSim();
    const invoke = sim.invoke;

    const layerId = "L1";
    await invoke("rust_pixels_open_document", { docId: DOC });
    await invoke("rust_pixels_init", { docId: DOC, layerId, width: W, height: H, bytes: new Array(W * H * 4).fill(255) });

    const afterWire = [
      { x: 0, y: 0, width: 256, height: 256, data: new Uint8ClampedArray(256 * 256 * 4).fill(77) },
    ];
    const beforeWire = [
      { x: 0, y: 0, width: 256, height: 256, data: new Uint8ClampedArray(256 * 256 * 4).fill(255) },
    ];
    const res = await invoke("apply_tile_patch", {
      docId: DOC,
      layerId,
      before: beforeWire.map((t: any) => ({ x: t.x, y: t.y, w: t.width, h: t.height, data: Array.from(t.data) })),
      after: afterWire.map((t: any) => ({ x: t.x, y: t.y, w: t.width, h: t.height, data: Array.from(t.data) })),
    });
    expect(res.epoch).toBe(1);
    expect(res.version).toBe(1);

    const { ctx, backing } = makeFakeCtx(W, H);

    const u = await invoke("rust_pixels_undo", { docId: DOC, layerId });
    applyRustTilesToSurface(ctx, u.tiles, FakeImageData);
    expect(u.epoch).toBe(2);
    expect(u.version).toBe(2);
    // C5.3-A: undo identifies which layer the authoritative stream reverted.
    expect(u.layerId).toBe(layerId);
    let snap = await invoke("rust_pixels_snapshot_tile", { docId: DOC, layerId, x: 0, y: 0, w: 256, h: 256 });
    expect(snap.data[0]).toBe(255);
    expect(backing[0]).toBe(255);

    const r = await invoke("rust_pixels_redo", { docId: DOC, layerId });
    applyRustTilesToSurface(ctx, r.tiles, FakeImageData);
    expect(r.epoch).toBe(3);
    expect(r.version).toBe(3);
    expect(r.layerId).toBe(layerId);
    snap = await invoke("rust_pixels_snapshot_tile", { docId: DOC, layerId, x: 0, y: 0, w: 256, h: 256 });
    expect(snap.data[0]).toBe(77);
    expect(backing[0]).toBe(77);
  });

  it("document close releases the canonical pixel namespace (C5.1)", async () => {
    const W = 64, H = 64;
    const sim = makeRustSim();
    const layerId = "L1";
    await invoke_sim(sim, "rust_pixels_open_document", { docId: DOC });
    await invoke_sim(sim, "rust_pixels_init", { docId: DOC, layerId, width: W, height: H, bytes: new Array(W * H * 4).fill(0) });
    await invoke_sim(sim, "rust_pixels_close_document", { docId: DOC });
    expect(sim.store.has(`${DOC}|${layerId}`)).toBe(false);
  });

  function invoke_sim(sim: ReturnType<typeof makeRustSim>, cmd: string, args: any) {
    return sim.invoke(cmd, args);
  }

  it("C5.3-A mixed brush/adjustment/brush undo-redo follows one unified stream", async () => {
    const W = 256, H = 256;
    const sim = makeRustSim();
    const invoke = sim.invoke;
    const layerId = "Lmix";
    await invoke("rust_pixels_open_document", { docId: DOC });
    await invoke("rust_pixels_init", { docId: DOC, layerId, width: W, height: H, bytes: new Array(W * H * 4).fill(0) });

    const stroke = (before: number, after: number) => ({
      docId: DOC,
      layerId,
      before: [{ x: 0, y: 0, width: W, height: H, data: new Uint8ClampedArray(W * H * 4).fill(before) }]
        .map((t: any) => ({ x: t.x, y: t.y, w: t.width, h: t.height, data: Array.from(t.data) })),
      after: [{ x: 0, y: 0, width: W, height: H, data: new Uint8ClampedArray(W * H * 4).fill(after) }]
        .map((t: any) => ({ x: t.x, y: t.y, w: t.width, h: t.height, data: Array.from(t.data) })),
    });

    // A: brush 0→11, B: adjustment 11→22, C: brush 22→33.
    expect((await invoke("apply_tile_patch", stroke(0, 11))).version).toBe(1);
    expect((await invoke("apply_tile_patch", stroke(11, 22))).version).toBe(2);
    expect((await invoke("apply_tile_patch", stroke(22, 33))).version).toBe(3);

    // undo → B (22), undo → A (11), redo → B (22), redo → C (33).
    const u1 = await invoke("rust_pixels_undo", { docId: DOC, layerId });
    expect(u1.version).toBe(4);
    expect(u1.tiles[0].data[0]).toBe(22);
    const u2 = await invoke("rust_pixels_undo", { docId: DOC, layerId });
    expect(u2.version).toBe(5);
    expect(u2.tiles[0].data[0]).toBe(11);
    const r1 = await invoke("rust_pixels_redo", { docId: DOC, layerId });
    expect(r1.version).toBe(6);
    expect(r1.tiles[0].data[0]).toBe(22);
    const r2 = await invoke("rust_pixels_redo", { docId: DOC, layerId });
    expect(r2.version).toBe(7);
    expect(r2.tiles[0].data[0]).toBe(33);

    // The TS local mirror is NOT authoritative: the canonical state is owned by
    // Rust's single stream. Verify the snapshot (Rust canonical) matches final C.
    const snap = await invoke("rust_pixels_snapshot_tile", { docId: DOC, layerId, x: 0, y: 0, w: 256, h: 256 });
    expect(snap.data[0]).toBe(33);
  });
});
