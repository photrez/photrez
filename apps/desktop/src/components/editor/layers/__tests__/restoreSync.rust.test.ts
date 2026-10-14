// SPDX-License-Identifier: AGPL-3.0-or-later
// C5.4 restore-sync regression tests: orphan Rust pixel-store cleanup (Part 1)
// and bitmap sync for imperative undo/redo (Part 2).
import { describe, it, expect, vi, beforeEach } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";

// ── Tauri invoke mock ──
const invokeLog: { cmd: string; args: unknown }[] = [];
const mockInvoke = vi.fn(async (cmd: string, args: unknown, _options?: unknown) => {
  invokeLog.push({ cmd, args });
  if (cmd === "rust_pixels_get_epoch") return 0;
  if (cmd === "rust_pixels_init") return undefined;
  if (cmd === "rust_pixels_write_region") {
    // Post-image only: the real reply carries no pre-image.
    return {
      after: [{ x: 0, y: 0, w: 100, h: 100, data: (args as any).rgba ?? new Array(100 * 100 * 4).fill(128) }],
      epoch: 1,
      version: 1,
    };
  }
  if (cmd === "rust_pixels_remove_layer") return undefined;
  if (cmd === "rust_pixels_snapshot_layer") {
    return [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(128) }];
  }
  return undefined;
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args: unknown, options?: any) => mockInvoke(cmd, args, options),
}));

// ── jsdom polyfills ──
class FakeImageData {
  data: Uint8ClampedArray;
  width: number;
  height: number;
  constructor(data: Uint8ClampedArray | number, w?: number, h?: number) {
    if (typeof data === "number") {
      this.width = data; this.height = w!;
      this.data = new Uint8ClampedArray(data * w! * 4);
    } else {
      this.width = w!; this.height = h!; this.data = data;
    }
  }
}
(globalThis as any).ImageData = FakeImageData;

function installOffscreenCanvas() {
  (globalThis as any).OffscreenCanvas = class {
    width: number; height: number; _buffer: Uint8ClampedArray;
    constructor(w: number, h: number) {
      this.width = w; this.height = h;
      this._buffer = new Uint8ClampedArray(w * h * 4);
    }
    getContext() {
      const self = this;
      return {
        drawImage(img: any, _sx?: number, _sy?: number, _sw?: number, _sh?: number, _dx?: number, _dy?: number, _dw?: number, _dh?: number) {
          const d = img?.getImageData ? img.getImageData().data : (img?.data ?? []);
          if (d && d.length === self._buffer.length) self._buffer.set(d);
        },
        getImageData(_x: number, _y: number, _w: number, _h: number) {
          return { data: new Uint8ClampedArray(self._buffer), width: self.width, height: self.height };
        },
        putImageData(vi: any) { if (vi?.data) self._buffer.set(vi.data); },
        save() {}, restore() {}, translate() {}, rotate() {}, scale() {},
        fillRect() {}, clearRect() {},
        set fillStyle(_v: string) {},
        get fillStyle() { return ""; },
        globalAlpha: 1, globalCompositeOperation: "source-over",
      };
    }
    transferToImageBitmap() {
      return { width: this.width, height: this.height, close() {}, getImageData: () => ({ data: new Uint8ClampedArray(this._buffer), width: this.width, height: this.height }) };
    }
  };
}
installOffscreenCanvas();

(globalThis as any).createImageBitmap = async (src: any) => {
  const data = src?.data ?? new Uint8ClampedArray((src?.width ?? 1) * (src?.height ?? 1) * 4);
  return { width: src.width ?? 1, height: src.height ?? 1, close() {}, getImageData: () => ({ data, width: src.width ?? 1, height: src.height ?? 1 }) };
};

function makeBitmap(w = 100, h = 100, fill = 255): { width: number; height: number; data: Uint8ClampedArray; close: ReturnType<typeof vi.fn> } {
  return {
    width: w, height: h,
    data: new Uint8ClampedArray(w * h * 4).fill(fill),
    close: vi.fn(),
  };
}

beforeEach(() => {
  invokeLog.length = 0;
  mockInvoke.mockClear();
});

// ────────────────────────────────────────────────────────────────
// Part 1: Orphan Rust pixel-store cleanup
// ────────────────────────────────────────────────────────────────

describe("C5.4 Part 1: orphan Rust pixel-store cleanup on restore", () => {
  it("remove_layer is called for layers removed by snapshot restore", () => {
    const engine = new DocumentEngine("doc1", "Test", 100, 100);
    const bg = engine.addLayer("Background", 100, 100);
    const layer1 = engine.addLayer("Layer 1", 100, 100);
    const layer2 = engine.addLayer("Layer 2", 100, 100);

    // Snapshot with 3 layers
    const snap3 = engine.snapshot();
    expect(snap3.layers).toHaveLength(3);

    // Add a 4th layer (without committing to history)
    const layer3 = engine.addLayer("Layer 3", 100, 100);
    expect(engine.getLayers()).toHaveLength(4);

    // Restore to the 3-layer snapshot → layer3 should be orphan-cleaned
    engine.restore(snap3);
    expect(engine.getLayers()).toHaveLength(3);

    // Give fire-and-forget async a tick
    return new Promise<void>((resolve) => {
      setTimeout(() => {
        const removeCalls = invokeLog.filter(c => c.cmd === "rust_pixels_remove_layer");
        expect(removeCalls).toHaveLength(1);
        expect(removeCalls[0].args).toEqual({ docId: "doc1", layerId: layer3.id });
        resolve();
      }, 50);
    });
  });

  it("does NOT call remove_layer when no layers were removed", () => {
    const engine = new DocumentEngine("doc2", "Test", 100, 100);
    const bg = engine.addLayer("Background", 100, 100);
    const layer1 = engine.addLayer("Layer 1", 100, 100);

    const snap = engine.snapshot();
    engine.restore(snap);

    return new Promise<void>((resolve) => {
      setTimeout(() => {
        const removeCalls = invokeLog.filter(c => c.cmd === "rust_pixels_remove_layer");
        expect(removeCalls).toHaveLength(0);
        resolve();
      }, 50);
    });
  });

  it("cleans up multiple removed layers", () => {
    const engine = new DocumentEngine("doc3", "Test", 100, 100);
    const bg = engine.addLayer("Background", 100, 100);
    const layer1 = engine.addLayer("Layer 1", 100, 100);

    const snap2 = engine.snapshot(); // 2 layers

    // Add 2 more layers
    const layer2 = engine.addLayer("Layer 2", 100, 100);
    const layer3 = engine.addLayer("Layer 3", 100, 100);
    expect(engine.getLayers()).toHaveLength(4);

    // Restore to 2-layer snapshot → layer2 and layer3 removed
    engine.restore(snap2);
    expect(engine.getLayers()).toHaveLength(2);

    return new Promise<void>((resolve) => {
      setTimeout(() => {
        const removeCalls = invokeLog.filter(c => c.cmd === "rust_pixels_remove_layer");
        expect(removeCalls).toHaveLength(2);
        const removedIds = removeCalls.map(c => (c.args as any).layerId);
        expect(removedIds).toContain(layer2.id);
        expect(removedIds).toContain(layer3.id);
        resolve();
      }, 50);
    });
  });
});

// Part 2 (bitmap sync for imperative undo/redo) retired:
// `history.undo`/`redo` and `consumeLastUndo*Patches` are deleted. The bitmap
// sync they pinned now lives in `runFacadeExternalHandoff`'s pixel projection
// (reuploadAttachedImages + ensureBitmapCurrent), covered by rustPixelUndoHandoff.wiring.
