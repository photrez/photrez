// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { decodeRustBytes, readPixelSeedCall } from "@/lib/protocol/pixelSeedCall";
import { applyPaintBucketFill } from "../paintBucket";
import { computeChangedRegion } from "@/lib/paint/regionProducer";
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

const countInvoke = (cmd: string) => mockInvoke.mock.calls.filter((c) => c[0] === cmd).length;

// jsdom lacks ImageData; stub it (floodFill mutates .data in place).
class FakeImageData {
  data: Uint8ClampedArray;
  width: number;
  height: number;
  constructor(data: Uint8ClampedArray | number, w?: number, h?: number) {
    if (typeof data === "number") {
      this.width = data;
      this.height = w!;
      this.data = new Uint8ClampedArray(data * w! * 4);
    } else {
      this.width = w!;
      this.height = h!;
      this.data = data;
    }
  }
}
(globalThis as any).ImageData = FakeImageData;

const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args: any, options?: any) => mockInvoke(cmd, args, options),
}));

// Record applyRustTilesToSurface calls (the real impl just putImageData).
const applyCalls: { ctx: unknown; tiles: unknown }[] = [];
vi.mock("@/lib/rustShadow", async (importOriginal) => {
  const actual = (await importOriginal()) as any;
  return {
    ...actual,
    applyRustTilesToSurface: (ctx: unknown, tiles: unknown) => {
      applyCalls.push({ ctx, tiles });
      return actual.applyRustTilesToSurface(ctx, tiles);
    },
  };
});

// Toast spy: the failure toast must carry the raw rejection text, so the
// production toast module is kept and only `showToast` is intercepted.
const showToastMock = vi.fn();
vi.mock("@/components/editor/Toast", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/editor/Toast")>();
  return { ...actual, showToast: (...args: Parameters<typeof actual.showToast>) => showToastMock(...args) };
});

function makeFakes(opts: { sel?: any } = {}) {
  // toImageBitmap is part of the real PaintTileSurface contract and is what the
  // visible projection reads. Without it the projection is skipped, which is
  // correct but would let this suite pass without ever proving visibility.
  const surface = {
    context: { putImageData: vi.fn(), getImageData: vi.fn((_x: number, _y: number, w: number, h: number) => new FakeImageData(w, h)) },
    pixelEpoch: 0,
    pixelVersion: 0,
    toImageBitmap: vi.fn(async () => ({ width: 8, height: 8, close: vi.fn() })),
  } as any;
  const commit = vi.fn();
  const uploadSurfaceTiles = vi.fn();
  const uploadImage = vi.fn();
  const layer = {
    id: "L1", width: 8, height: 8, locked: false, visible: true, lockTransparency: false,
    transform: { scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false, x: 0, y: 0 },
  };
  const engine: any = {
    getActiveLayerId: () => "L1",
    getLayer: (id: string) => (id === "L1" ? layer : null),
    getSelection: () => opts.sel ?? null,
    getPaintSurface: (id: string) => (id === "L1" ? surface : null),
    snapshot: () => ({ __snap: true }),
    getLayerImageBitmap: vi.fn(),
    setLayerImageBitmap: vi.fn(),
    notifyVisualChange: vi.fn(),
  };
  const workspace: any = {
    getActiveEngine: () => engine,
    getActiveHistory: () => ({ commit }),
    getActiveDocumentId: () => "doc1",
  };
  const editor: any = {
    activeTool: () => "paintBucket",
    workspace,
    renderer: { uploadSurfaceTiles, uploadImage },
    scheduler: { requestRender: vi.fn() },
    fgColor: () => "#ff0000",
    fillTolerance: () => 0,
    fillContiguous: () => true,
  };
  const ctx: any = {
    editor,
    getDocCoords: () => ({ x: 1, y: 1 }),
    getCanvasRef: () => ({ current: null }),
  };
  return { surface, commit, uploadSurfaceTiles, uploadImage, engine, workspace, editor, ctx, surfaceRef: surface };
}

describe("computeChangedRegion (pure)", () => {
  it("finds the bbox of a changed 2x2 block and extracts rgba", () => {
    const before = new Uint8ClampedArray(4 * 4 * 4); // 4x4 transparent
    const after = before.slice();
    for (let y = 1; y <= 2; y++) {
      for (let x = 1; x <= 2; x++) {
        const i = (y * 4 + x) * 4;
        after[i] = 255; after[i + 3] = 255;
      }
    }
    const r = computeChangedRegion(before, after, 4, 4);
    expect(r).not.toBeNull();
    expect(r!.x).toBe(1); expect(r!.y).toBe(1); expect(r!.w).toBe(2); expect(r!.h).toBe(2);
    expect(r!.rgba.length).toBe(2 * 2 * 4);
    expect(r!.rgba[0]).toBe(255); expect(r!.rgba[3]).toBe(255);
  });
  it("returns null when identical", () => {
    const b = new Uint8ClampedArray(4 * 4 * 4);
    expect(computeChangedRegion(b, b.slice(), 4, 4)).toBeNull();
  });
});

describe("applyPaintBucketFill — Rust canonical path (C5.4 pilot)", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    applyCalls.length = 0;
    showToastMock.mockClear();
    localStorage.setItem("photrez.rustPixels", "1");
  });

  // Defeat: change paintBucket.ts to send a plain number array instead of encoding base64;
  // the toBeInstanceOf(Uint8Array) check on the decoded call goes RED.
  it("writes the changed region via rust_pixels_write_region and commits ONE history entry", async () => {
    const { surface, commit, uploadSurfaceTiles, editor, ctx } = makeFakes();

    // invoke sequence: snapshot (read source) + write_region (commit).
    let writeRes: any;
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") {
        return [{ x: 0, y: 0, w: 8, h: 8, data: new Array(8 * 8 * 4).fill(0) }];
      }
      if (cmd === "rust_pixels_write_region") {
        // The real reply carries no pre-image; this double matches it.
        writeRes = {
          after: [{ x: 0, y: 0, w: 8, h: 8, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }],
          epoch: 1, version: 1,
        };
        return writeRes;
      }
      return undefined;
    });

    const handled = applyPaintBucketFill(ctx, { pointerId: 1 } as any);
    expect(handled).toBe(true);

    // The canonical fill runs async (fire-and-forget); wait for it to commit.
    await vi.waitFor(() => {
      // Exactly ONE history commit → exactly ONE user-visible undo step.
      expect(commit).toHaveBeenCalledTimes(1);
      expect(commit.mock.calls[0][1]).toBe("Paint Bucket Fill");

      // Imperative memento present. For this Rust-owned entry the memento is a
      // cursor token only: the Rust entry owns the pixels, and one undo reads
      // them back from Rust instead of replaying the memento tiles. Its
      // pre-image is empty because the write reply carries none.
      const imp = commit.mock.calls[0][2];
      expect(imp.layerId).toBe("L1");
      expect(imp.surfaceWidth).toBe(8);
      expect(imp.surfaceHeight).toBe(8);
      expect(imp.before.length).toBe(0);
      expect(imp.after.length).toBe(1);

      // Read current pixels from Rust (overlapping fills use the Rust base).
      const cmds = mockInvoke.mock.calls.map((c) => c[0]);
      expect(cmds).toContain("rust_pixels_snapshot_layer");
      expect(cmds).toContain("rust_pixels_write_region");

      // write_region got the whole-layer changed region (transparent → filled red).
      const wr = decodeRustBytes<{ x: number; y: number; w: number; h: number; rgba: Uint8Array }>(
        mockInvoke.mock.calls.find((c) => c[0] === "rust_pixels_write_region")![1],
      );
      expect(wr.x).toBe(0); expect(wr.y).toBe(0); expect(wr.w).toBe(8); expect(wr.h).toBe(8);
      expect(wr.rgba.length).toBe(8 * 8 * 4);
      expect(wr.rgba[0]).toBe(255);
      expect(wr.rgba[3]).toBe(255);
      // The fill write crosses invoke base64-encoded, like the seed path above.
      expect(wr.rgba).toBeInstanceOf(Uint8Array);

      // TS derived cache updated from Rust result.
      expect(surface.pixelEpoch).toBe(1);
      expect(surface.pixelVersion).toBe(1);
      expect(applyCalls.length).toBeGreaterThan(0);
      // The tiles applied to the surface are the ones Rust returned, by value:
      // decodeRustBytes rebuilds them from the base64 the response carried.
      expect(applyCalls[applyCalls.length - 1].tiles).toEqual(writeRes.after);
      expect(uploadSurfaceTiles).toHaveBeenCalledWith(
        "L1",
        8,
        8,
        writeRes.after.map((t: { x: number; y: number; w: number; h: number; data: number[] }) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
      );
    }, { timeout: 2000 });
  });

  // RED-first: on the pre-fix tree this toast reads `Unknown error`, because a
  // Tauri v2 rejection is a bare string and `err instanceof Error` is false.
  it("surfaces a bare-string write_region rejection verbatim in the failure toast", async () => {
    const { commit, ctx } = makeFakes();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") {
        return [{ x: 0, y: 0, w: 8, h: 8, data: new Array(8 * 8 * 4).fill(0) }];
      }
      if (cmd === "rust_pixels_write_region") throw "E_RUST: boom";
      return undefined;
    });

    applyPaintBucketFill(ctx, { pointerId: 1 } as any);

    await vi.waitFor(() => {
      expect(showToastMock).toHaveBeenCalledWith("Fill failed: E_RUST: boom", "error");
    }, { timeout: 2000 });
    expect(commit).not.toHaveBeenCalled();
  });

  it("does NOT invoke write_region when the flood changes nothing", async () => {
    const { commit, editor, ctx } = makeFakes();
    // snapshot returns an already-filled buffer identical to the fill result.
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") {
        const data = new Array(8 * 8 * 4).fill(0);
        for (let i = 0; i < data.length; i += 4) { data[i] = 255; data[i + 3] = 255; }
        return [{ x: 0, y: 0, w: 8, h: 8, data }];
      }
      return undefined;
    });
    applyPaintBucketFill(ctx, { pointerId: 1 } as any);
    // The async IIFE runs; with no change it neither writes nor commits.
    await new Promise((r) => setTimeout(r, 30));
    expect(mockInvoke.mock.calls.some((c) => c[0] === "rust_pixels_write_region")).toBe(false);
    expect(commit).not.toHaveBeenCalled();
  });

describe("applyPaintBucketFill — Rust canonical seed (FIRST raster op)", () => {
  it("seeds the canonical store via rust_pixels_init when the layer has no Rust entry yet", async () => {
    localStorage.setItem("photrez.rustPixels", "1");
    const { surface, commit, editor, ctx } = makeFakes();
    const initCalls: any[] = [];
    let writeRes: any;
    // get_epoch rejects → fill must seed the layer from the derived pixels.
    mockInvoke.mockImplementation(async (cmd: string, args: any, options?: any) => {
      if (cmd === "rust_pixels_get_epoch") throw new Error("layer not initialized");
      if (cmd === "rust_pixels_init") { initCalls.push(readPixelSeedCall(cmd, args)!); return undefined; }
      if (cmd === "rust_pixels_snapshot_layer") {
        return [{ x: 0, y: 0, w: 8, h: 8, data: new Array(8 * 8 * 4).fill(0) }];
      }
      if (cmd === "rust_pixels_write_region") {
        // The real reply carries no pre-image; this double matches it.
        writeRes = {
          after: [{ x: 0, y: 0, w: 8, h: 8, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }],
          epoch: 1, version: 1,
        };
        return writeRes;
      }
      return undefined;
    });

    const handled = applyPaintBucketFill(ctx, { pointerId: 1 } as any);
    expect(handled).toBe(true);

    await vi.waitFor(() => {
      // Ensure-if-absent seeded the canonical store with the current layer bytes.
      expect(initCalls.length).toBe(1);
      expect(initCalls[0].docId).toBe("doc1");
      expect(initCalls[0].layerId).toBe("L1");
      expect(initCalls[0].width).toBe(8);
      expect(initCalls[0].height).toBe(8);
      expect(initCalls[0].bytes.length).toBe(8 * 8 * 4);
      expect(initCalls[0].bytes).toBeInstanceOf(Uint8Array);

      // Then the normal read → fill → write path still ran.
      const cmds = mockInvoke.mock.calls.map((c) => c[0]);
      expect(cmds).toContain("rust_pixels_init");
      expect(cmds).toContain("rust_pixels_snapshot_layer");
      expect(cmds).toContain("rust_pixels_write_region");

      expect(commit).toHaveBeenCalledTimes(1);
      expect(surface.pixelEpoch).toBe(1);
      expect(surface.pixelVersion).toBe(1);
    }, { timeout: 2000 });
  });
});
});

// ── Restored coverage ───────────────────────────────────────────────────────
// 2143dcd deleted this file's "legacy path upload granularity" block and its
// flag ON/OFF routing test. The granularity assertions pinned REAL product
// behaviour (how much of the layer a fill touches), so they are re-pinned here
// against the canonical Rust arm, which is where the region is computed now.
// The routing half of the deleted flag test asserted the flag-OFF arm, which no
// longer exists; that half is retired on purpose and is NOT restored.

// A full-layer write is what an unconstrained fill produces, so a zeroed source
// and an 8x8 layer give a known region to assert against.
function okFillInvoke() {
  mockInvoke.mockImplementation(async (cmd: string, args: any) => {
    if (cmd === "rust_pixels_get_epoch") return 0;
    if (cmd === "rust_pixels_snapshot_layer") {
      return [{ x: 0, y: 0, w: 8, h: 8, data: new Array(8 * 8 * 4).fill(0) }];
    }
    if (cmd === "rust_pixels_write_region") {
      // Post-image only: the real reply carries no pre-image.
      return {
        after: [{ x: 0, y: 0, w: 8, h: 8, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }],
        epoch: 1, version: 1,
      };
    }
    return undefined;
  });
}

const writeRegionArgs = () =>
  mockInvoke.mock.calls.find((c) => c[0] === "rust_pixels_write_region")![1] as
    { x: number; y: number; w: number; h: number };

describe("paint bucket write-region granularity (canonical Rust arm)", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    applyCalls.length = 0;
    showToastMock.mockClear();
  });

  it("scopes the region to the fill AABB for a non-inverted selection", async () => {
    // The marquee must CONTAIN the click point (1,1): a fill started outside a
    // non-inverted mask changes nothing and correctly writes no region at all.
    const { ctx } = makeFakes({ sel: { x: 0, y: 0, width: 4, height: 4, angle: 0, shape: "rect", inverted: false } });
    okFillInvoke();
    applyPaintBucketFill(ctx, { pointerId: 1 } as any);
    await vi.waitFor(() => {
      const wr = writeRegionArgs();
      // Bounded by the marquee: the fill may only touch pixels inside it.
      expect(wr.x).toBeGreaterThanOrEqual(0);
      expect(wr.y).toBeGreaterThanOrEqual(0);
      expect(wr.x + wr.w, "the region stops at the marquee edge").toBeLessThanOrEqual(4);
      expect(wr.y + wr.h, "the region stops at the marquee edge").toBeLessThanOrEqual(4);
    }, { timeout: 2000 });
  });

  it("spans the whole layer for an inverted selection", async () => {
    const { ctx } = makeFakes({ sel: { x: 2, y: 2, width: 4, height: 4, angle: 0, shape: "rect", inverted: true } });
    okFillInvoke();
    applyPaintBucketFill(ctx, { pointerId: 1 } as any);
    await vi.waitFor(() => {
      const wr = writeRegionArgs();
      expect([wr.x, wr.y, wr.w, wr.h], "an inverted fill can touch the whole layer").toEqual([0, 0, 8, 8]);
    }, { timeout: 2000 });
  });

  it("spans the whole layer with no selection", async () => {
    const { ctx } = makeFakes();
    okFillInvoke();
    applyPaintBucketFill(ctx, { pointerId: 1 } as any);
    await vi.waitFor(() => {
      const wr = writeRegionArgs();
      expect([wr.x, wr.y, wr.w, wr.h], "an unconstrained fill can touch the whole layer").toEqual([0, 0, 8, 8]);
    }, { timeout: 2000 });
  });

  // Re-pins the deleted flag-ON half of the routing test, with one assertion
  // deliberately inverted: the Rust arm NOW installs a layer raster, because the
  // drawn layer comes from layer.imageBitmap and the op must be visible when it
  // runs. The deleted test asserted setLayerImageBitmap was never called, which
  // is exactly what made a recorded fill invisible until undo+redo.
  it("commits exactly one rustOwned entry and makes the fill visible", async () => {
    const { ctx, commit, uploadImage } = makeFakes();
    okFillInvoke();
    applyPaintBucketFill(ctx, { pointerId: 1 } as any);
    await vi.waitFor(() => {
      expect(commit).toHaveBeenCalledTimes(1);
      expect(commit.mock.calls[0][1]).toBe("Paint Bucket Fill");
      expect(commit.mock.calls[0][2]?.rustOwned).toBe(true);
      // The visible texture is refreshed from the canonical pixels.
      expect(uploadImage).toHaveBeenCalled();
    }, { timeout: 2000 });
  });
});

// The bridge gate must be ON for this pin to mean anything: with it off,
// commit() skips the Rust append entirely and a zero count proves nothing.
describe("paint bucket commit pin (history bridge ON)", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    applyCalls.length = 0;
    localStorage.setItem("photrez.rustPixels", "1");
    localStorage.setItem("photrez.historyBridge", "1");
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
  });
  afterEach(() => {
    localStorage.removeItem("photrez.rustPixels");
    localStorage.removeItem("photrez.historyBridge");
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  it("records ZERO apply_tile_patch: rust_pixels_write_region already owns the entry", async () => {
    const { ctx, workspace } = makeFakes();
    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc1");
    workspace.getActiveHistory = () => history;

    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") {
        return [{ x: 0, y: 0, w: 8, h: 8, data: new Array(8 * 8 * 4).fill(0) }];
      }
      if (cmd === "rust_pixels_write_region") {
        return {
              after: [{ x: 0, y: 0, w: 8, h: 8, data: Array.from(decodeRustBytes<{ rgba: Uint8Array }>(args).rgba) }],
          epoch: 1,
          version: 1,
        };
      }
      return undefined;
    });

    expect(historyBridgeEnabled()).toBe(true);
    applyPaintBucketFill(ctx, { pointerId: 1 } as any);

    await vi.waitFor(() => expect(history.getUndoCount()).toBe(1), { timeout: 2000 });
    await settleBridge();
    // Positive control first: the fill reached Rust exactly once.
    expect(countInvoke("rust_pixels_write_region")).toBe(1);
    expect(countInvoke("apply_tile_patch")).toBe(0);
  });
});
