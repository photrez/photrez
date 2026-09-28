// SPDX-License-Identifier: AGPL-3.0-or-later
// Pre-event soak: >=50 consecutive production-entry raster operations for bucket,
// fill, and bake with photrez.rustPixels=1 set ONLY inside this harness. Counts
// are observed at the production boundary (invoke calls, history commits, error
// toasts), never through a spy on the routing guard.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderHook } from "@solidjs/testing-library";
import { EditorProvider } from "../shell/EditorContext";
import { DialogProvider } from "../dialogs/DialogProvider";
import { WorkspaceManager } from "@/engine/workspace";
import { applyPaintBucketFill } from "../canvas/pointerTools/paintBucket";
import { fillActiveLayerWithColor } from "../layers/layerOperations";
import { useLayerActions } from "../layers/useLayerActions";

const OPS = 50;

const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args: any) => mockInvoke(cmd, args),
}));

// The failure toast must carry the raw rejection text, so the production toast
// module is kept and only `showToast` is intercepted.
const showToastMock = vi.fn();
vi.mock("@/components/editor/Toast", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/editor/Toast")>();
  return { ...actual, showToast: (...args: Parameters<typeof actual.showToast>) => showToastMock(...args) };
});

vi.mock("@/lib/rustShadow", async (importOriginal) => {
  const actual = (await importOriginal()) as any;
  return { ...actual, rehydratePaintSurfaceFromRust: vi.fn() };
});

// jsdom has no ImageData / createImageBitmap / OffscreenCanvas.
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

const OriginalOffscreenCanvas = (globalThis as any).OffscreenCanvas;
const OriginalImageData = (globalThis as any).ImageData;
const OriginalCreateImageBitmap = (globalThis as any).createImageBitmap;

beforeEach(() => {
  (globalThis as any).ImageData = FakeImageData;
  (globalThis as any).createImageBitmap = async (src: any) => {
    const data = src?.data ?? src?._bytes ?? new Uint8ClampedArray((src?.width ?? 1) * (src?.height ?? 1) * 4);
    return { width: src.width, height: src.height, getImageData: () => ({ data, width: src.width, height: src.height }) };
  };
  (globalThis as any).OffscreenCanvas = class {
    width: number;
    height: number;
    _buffer: Uint8ClampedArray;
    constructor(w: number, h: number) {
      this.width = w;
      this.height = h;
      this._buffer = new Uint8ClampedArray(w * h * 4);
    }
    getContext() {
      const self = this;
      return {
        _fs: "" as string,
        get fillStyle() { return (this as any)._fs; },
        set fillStyle(v: string) { (this as any)._fs = v; },
        fillRect(x: number, y: number, w: number, h: number) {
          const hex = (this._fs as string).replace("#", "");
          const r = parseInt(hex.slice(0, 2), 16), g = parseInt(hex.slice(2, 4), 16), b = parseInt(hex.slice(4, 6), 16);
          for (let row = y; row < y + h; row++)
            for (let col = x; col < x + w; col++) {
              if (row < 0 || row >= self.height || col < 0 || col >= self.width) continue;
              const idx = (row * self.width + col) * 4;
              self._buffer[idx] = r; self._buffer[idx + 1] = g; self._buffer[idx + 2] = b; self._buffer[idx + 3] = 255;
            }
        },
        drawImage(img: any) {
          const d = img?.getImageData ? img.getImageData().data : (img?.data ?? img?._bytes);
          if (d && d.length === self._buffer.length) self._buffer.set(d);
        },
        getImageData() {
          return { data: self._buffer, width: self.width, height: self.height, colorSpace: "srgb" };
        },
        putImageData(v: any) { if (v && v.data) self._buffer.set(v.data); },
        save: () => {}, restore: () => {}, translate: () => {}, rotate: () => {}, scale: () => {},
        globalAlpha: 1, globalCompositeOperation: "source-over",
      };
    }
    transferToImageBitmap() {
      const buf = this._buffer;
      return { width: this.width, height: this.height, getImageData: () => ({ data: buf, width: this.width, height: this.height }), close: () => {} } as any;
    }
  };
  mockInvoke.mockReset();
  showToastMock.mockClear();
  localStorage.setItem("photrez.rustPixels", "1");
  localStorage.setItem("photrez.facade", "0");
  localStorage.setItem("photrez.facadeAuthority", "wasm");
});
afterEach(() => {
  (globalThis as any).OffscreenCanvas = OriginalOffscreenCanvas;
  (globalThis as any).ImageData = OriginalImageData;
  (globalThis as any).createImageBitmap = OriginalCreateImageBitmap;
  localStorage.removeItem("photrez.rustPixels");
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  vi.restoreAllMocks();
});

const writes = () => mockInvoke.mock.calls.filter((c) => c[0] === "rust_pixels_write_region").length;
const fallbackHits = () => showToastMock.mock.calls.filter((c) => c[1] === "error").length;

function okInvoke(opts: { failWrite?: string } = {}) {
  mockInvoke.mockImplementation(async (cmd: string, args: any) => {
    if (cmd === "rust_pixels_get_epoch") throw new Error("layer not initialized");
    if (cmd === "rust_pixels_write_region" && opts.failWrite) throw opts.failWrite;
    if (cmd === "rust_pixels_snapshot_layer") return [];
    if (cmd === "rust_pixels_init") return undefined;
    if (cmd === "rust_pixels_write_region") {
      return {
        before: [{ x: 0, y: 0, w: args.w, h: args.h, data: new Array(args.w * args.h * 4).fill(0) }],
        after: [{ x: 0, y: 0, w: args.w, h: args.h, data: Array.from(args.rgba) }],
        epoch: 1,
        version: 1,
      };
    }
    return undefined;
  });
}

// ---- bucket fixture (mirrors paintBucket.rustFill.test.ts) ----
function makeBucketFakes(size = 16) {
  const surface = {
    context: {
      putImageData: vi.fn(),
      getImageData: vi.fn((_x: number, _y: number, w: number, h: number) => new FakeImageData(w, h)),
    },
    pixelEpoch: 0,
    pixelVersion: 0,
  } as any;
  const commit = vi.fn();
  const layer = {
    id: "L1", width: size, height: size, locked: false, visible: true, lockTransparency: false,
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
    renderer: { uploadSurfaceTiles: vi.fn() },
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
  return { commit, ctx };
}

// ---- fill fixture (mirrors fillLayer.rustFill.test.ts) ----
function makeFillFakes(size = 32) {
  const surface = {
    context: {
      putImageData: vi.fn(),
      getImageData: vi.fn((_x: number, _y: number, w: number, h: number) => new FakeImageData(w, h)),
    },
    pixelEpoch: 0,
    pixelVersion: 0,
  } as any;
  const commit = vi.fn();
  const layer = {
    id: "L1", width: size, height: size, locked: false, visible: true, lockTransparency: false,
    transform: { scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false, x: 0, y: 0 },
    basicAdjustment: null as any,
  };
  let basicAdj = layer.basicAdjustment;
  const engine: any = {
    getActiveLayerId: () => "L1",
    getLayer: () => layer,
    getSelection: () => null,
    getPaintSurface: () => surface,
    getId: () => "doc1",
    snapshot: () => ({ basicAdjustment: basicAdj }),
    restore: (s: any) => { basicAdj = s.basicAdjustment; layer.basicAdjustment = basicAdj; },
    clearBasicAdjustments: () => { basicAdj = null; layer.basicAdjustment = null; },
    getLayerImageBitmap: vi.fn(),
    setLayerImageBitmap: vi.fn(),
  };
  const renderer: any = { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() };
  return { commit, engine, renderer };
}

// ---- bake fixture (real useLayerActions hook) ----
async function makeBakeHarness(size = 64) {
  const ws = new WorkspaceManager();
  ws.addDocument(WorkspaceManager.createBlankDocument("doc-bake", "Bake", size, size));
  ws.switchDocument("doc-bake");
  const engine = ws.getEngine("doc-bake")!;
  const layer = engine.addLayer("Bake", size, size);
  engine.setActiveLayer(layer.id);
  layer.basicAdjustment = { brightness: 20, contrast: 0, saturation: 0 };
  engine.setLayerImageBitmap(layer.id, {
    width: size, height: size, close: vi.fn(), _bytes: new Uint8ClampedArray(size * size * 4).fill(1),
  } as unknown as ImageBitmap);
  vi.spyOn(engine, "commitBasicAdjustment").mockImplementation(async (id: string) => {
    engine.setLayerImageBitmap(id, {
      width: size, height: size, close: vi.fn(), _bytes: new Uint8ClampedArray(size * size * 4).fill(200),
    } as unknown as ImageBitmap);
    return "cpu" as const;
  });
  vi.spyOn(engine, "getPaintSurface").mockReturnValue({
    context: { putImageData: vi.fn() }, pixelEpoch: 0, pixelVersion: 0,
  } as never);
  const renderer: any = { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn(), destroyTexture: vi.fn() };
  const scheduler: any = { requestRender: vi.fn() };
  const wrapper = (props: { children: any }) => (
    <DialogProvider>
      <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
        {props.children}
      </EditorProvider>
    </DialogProvider>
  );
  const { result } = renderHook(() => useLayerActions(), { wrapper });
  const commitSpy = vi.spyOn(ws.getActiveHistory()!, "commit");
  return { ws, engine, layer, result, commitSpy };
}

describe("pre-event pixel operation soak (photrez.rustPixels=1 harness only)", () => {
  it(`bucket: ${OPS} production-entry fills, one write and one undo step each`, async () => {
    const { commit, ctx } = makeBucketFakes();
    okInvoke();
    for (let i = 1; i <= OPS; i++) {
      const ev = new MouseEvent("pointerdown", { bubbles: true }) as PointerEvent;
      Object.defineProperty(ev, "pointerId", { value: 1 });
      applyPaintBucketFill(ctx, ev);
      await vi.waitFor(() => expect(commit.mock.calls.length).toBe(i), { timeout: 5000, interval: 5 });
    }
    await vi.waitFor(() => {
      expect(writes()).toBe(OPS);
      expect(commit.mock.calls.length).toBe(OPS);
    }, { timeout: 30000 });
    const historySteps = commit.mock.calls.length;
    const stateChangingApplies = writes();
    const fallback = fallbackHits();
    console.log(`soak bucket: operations=${OPS} stateChangingApplies=${stateChangingApplies} historySteps=${historySteps} fallbackHits=${fallback}`);
    expect(OPS).toBeGreaterThanOrEqual(50);
    expect(stateChangingApplies).toBe(OPS);
    expect(historySteps).toBe(OPS);
    expect(commit.mock.calls.every((c) => c[2])).toBe(true);
    expect(fallback).toBe(0);
  });

  it(`fill: ${OPS} production-entry fills, one write and one undo step each`, async () => {
    const { commit, engine, renderer } = makeFillFakes();
    okInvoke();
    for (let i = 1; i <= OPS; i++) {
      fillActiveLayerWithColor(engine, { commit } as any, renderer, "#00ff00");
      await vi.waitFor(() => expect(commit.mock.calls.length).toBe(i), { timeout: 5000, interval: 5 });
    }
    await vi.waitFor(() => {
      expect(writes()).toBe(OPS);
      expect(commit.mock.calls.length).toBe(OPS);
    }, { timeout: 30000 });
    const historySteps = commit.mock.calls.length;
    const stateChangingApplies = writes();
    const fallback = fallbackHits();
    console.log(`soak fill: operations=${OPS} stateChangingApplies=${stateChangingApplies} historySteps=${historySteps} fallbackHits=${fallback}`);
    expect(OPS).toBeGreaterThanOrEqual(50);
    expect(stateChangingApplies).toBe(OPS);
    expect(historySteps).toBe(OPS);
    expect(commit.mock.calls.every((c) => c[2])).toBe(true);
    expect(fallback).toBe(0);
  });

  it(`bake: ${OPS} production-entry bakes, one write and one undo step each`, async () => {
    const h = await makeBakeHarness();
    okInvoke();
    for (let i = 1; i <= OPS; i++) {
      await h.result.handleApplyAdjustment();
      await vi.waitFor(() => expect(h.commitSpy.mock.calls.length).toBe(i), { timeout: 5000, interval: 5 });
    }
    await vi.waitFor(() => {
      expect(writes()).toBe(OPS);
      expect(h.commitSpy.mock.calls.length).toBe(OPS);
    }, { timeout: 30000 });
    const historySteps = h.commitSpy.mock.calls.length;
    const stateChangingApplies = writes();
    const fallback = fallbackHits();
    console.log(`soak bake: operations=${OPS} stateChangingApplies=${stateChangingApplies} historySteps=${historySteps} fallbackHits=${fallback}`);
    expect(OPS).toBeGreaterThanOrEqual(50);
    expect(stateChangingApplies).toBe(OPS);
    expect(historySteps).toBe(OPS);
    expect(h.commitSpy.mock.calls.every((c) => c[2])).toBe(true);
    expect(fallback).toBe(0);
  });

  // Non-vacuous zero: a bare-string Tauri rejection must be counted once and
  // surfaced verbatim, proving fallbackHits=0 above is not an unobservable counter.
  it("a bare-string write_region rejection is observed as exactly one fallback hit", async () => {
    const { commit, ctx } = makeBucketFakes();
    okInvoke({ failWrite: "E_RUST: rust ipc unavailable" });
    const ev = new MouseEvent("pointerdown", { bubbles: true }) as PointerEvent;
    Object.defineProperty(ev, "pointerId", { value: 1 });
    applyPaintBucketFill(ctx, ev);

    await vi.waitFor(() => expect(fallbackHits()).toBe(1));
    expect(showToastMock.mock.calls[0][0]).toContain("E_RUST: rust ipc unavailable");
    expect(commit.mock.calls.length).toBe(0);
  });
});
