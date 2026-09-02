// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, it, expect, vi, beforeEach } from "vitest";
import { applyPaintBucketFill, computeChangedRegion } from "../paintBucket";

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
  invoke: (cmd: string, args: any) => mockInvoke(cmd, args),
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

function makeFakes() {
  const surface = { context: { putImageData: vi.fn(), getImageData: vi.fn((_x: number, _y: number, w: number, h: number) => new FakeImageData(w, h)) }, pixelEpoch: 0, pixelVersion: 0 } as any;
  const commit = vi.fn();
  const uploadSurfaceTiles = vi.fn();
  const layer = {
    id: "L1", width: 8, height: 8, locked: false, visible: true, lockTransparency: false,
    transform: { scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false, x: 0, y: 0 },
  };
  const engine: any = {
    getActiveLayerId: () => "L1",
    getLayer: (id: string) => (id === "L1" ? layer : null),
    getSelection: () => null,
    getPaintSurface: (id: string) => (id === "L1" ? surface : null),
    snapshot: () => ({ __snap: true }),
    getLayerImageBitmap: vi.fn(),
    setLayerImageBitmap: vi.fn(),
  };
  const workspace: any = {
    getActiveEngine: () => engine,
    getActiveHistory: () => ({ commit }),
    getActiveDocumentId: () => "doc1",
  };
  const editor: any = {
    activeTool: () => "paintBucket",
    workspace,
    renderer: { uploadSurfaceTiles },
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
  return { surface, commit, uploadSurfaceTiles, engine, workspace, editor, ctx, surfaceRef: surface };
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
    localStorage.setItem("photrez.rustPixels", "1");
  });

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
        writeRes = {
          before: [{ x: 0, y: 0, w: 8, h: 8, data: new Array(8 * 8 * 4).fill(0) }],
          after: [{ x: 0, y: 0, w: 8, h: 8, data: Array.from(args.rgba) }],
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

      // Imperative memento present (drives undo/redo; Rust entry is subordinate).
      const imp = commit.mock.calls[0][2];
      expect(imp.layerId).toBe("L1");
      expect(imp.surfaceWidth).toBe(8);
      expect(imp.surfaceHeight).toBe(8);
      expect(imp.before.length).toBe(1);
      expect(imp.after.length).toBe(1);

      // Read current pixels from Rust (overlapping fills use the Rust base).
      const cmds = mockInvoke.mock.calls.map((c) => c[0]);
      expect(cmds).toContain("rust_pixels_snapshot_layer");
      expect(cmds).toContain("rust_pixels_write_region");

      // write_region got the whole-layer changed region (transparent → filled red).
      const wr = mockInvoke.mock.calls.find((c) => c[0] === "rust_pixels_write_region")![1];
      expect(wr.x).toBe(0); expect(wr.y).toBe(0); expect(wr.w).toBe(8); expect(wr.h).toBe(8);
      expect((wr.rgba as number[]).length).toBe(8 * 8 * 4);
      expect((wr.rgba as number[])[0]).toBe(255);
      expect((wr.rgba as number[])[3]).toBe(255);

      // TS derived cache updated from Rust result.
      expect(surface.pixelEpoch).toBe(1);
      expect(surface.pixelVersion).toBe(1);
      expect(applyCalls.length).toBeGreaterThan(0);
      expect(applyCalls[applyCalls.length - 1].tiles).toBe(writeRes.after);
      expect(uploadSurfaceTiles).toHaveBeenCalledWith(
        "L1",
        8,
        8,
        writeRes.after.map((t: { x: number; y: number; w: number; h: number; data: number[] }) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
      );
    }, { timeout: 2000 });
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

  it("seeds the canonical store via rust_pixels_init when the layer has no Rust entry yet (fill as FIRST raster op)", async () => {
    const { surface, commit, editor, ctx } = makeFakes();
    const initCalls: any[] = [];
    let writeRes: any;
    // get_epoch rejects → fill must seed the layer from the derived pixels.
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") throw new Error("layer not initialized");
      if (cmd === "rust_pixels_init") { initCalls.push(args); return undefined; }
      if (cmd === "rust_pixels_snapshot_layer") {
        return [{ x: 0, y: 0, w: 8, h: 8, data: new Array(8 * 8 * 4).fill(0) }];
      }
      if (cmd === "rust_pixels_write_region") {
        writeRes = {
          before: [{ x: 0, y: 0, w: 8, h: 8, data: new Array(8 * 8 * 4).fill(0) }],
          after: [{ x: 0, y: 0, w: 8, h: 8, data: Array.from(args.rgba) }],
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
