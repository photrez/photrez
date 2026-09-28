// SPDX-License-Identifier: AGPL-3.0-or-later
// Binary transport pins: adjustment-bake invoke args must cross as Uint8Array,
// not JSON number arrays. Counts/bytes only, no timing.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook } from "@solidjs/testing-library";
import { EditorProvider } from "../../shell/EditorContext";
import { DialogProvider } from "../../dialogs/DialogProvider";
import { WorkspaceManager } from "@/engine/workspace";
import { useLayerActions } from "../useLayerActions";
import { historyBridgeEnabled } from "@/engine/history";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";
import { showToast } from "../../Toast";

const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args: any) => mockInvoke(cmd, args),
}));

vi.mock("../../Toast", async (importOriginal) => {
  const actual = (await importOriginal()) as typeof import("../../Toast");
  return { ...actual, showToast: vi.fn() };
});

vi.mock("@/lib/rustShadow", async (importOriginal) => {
  const actual = (await importOriginal()) as any;
  return { ...actual, rehydratePaintSurfaceFromRust: vi.fn() };
});

const OriginalOffscreenCanvas = (globalThis as any).OffscreenCanvas;
const OriginalImageData = (globalThis as any).ImageData;
beforeEach(() => {
  // jsdom has no ImageData constructor; applyRustTilesToSurface does `new Ctor(...)`.
  (globalThis as any).ImageData = class {
    data: Uint8ClampedArray;
    width: number;
    height: number;
    constructor(data: Uint8ClampedArray, w: number, h?: number) {
      this.data = data;
      this.width = w;
      this.height = h ?? data.length / 4 / w;
    }
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
        // Copy the source bitmap's bytes so the test can tell which bitmap
        // (pre-bake vs post-bake) actually reached the write payload.
        drawImage: vi.fn((img: any) => {
          if (img?._bytes) self._buffer.set(img._bytes);
        }),
        getImageData: vi.fn(() => ({ data: self._buffer, width: self.width, height: self.height })),
        putImageData: vi.fn(),
      };
    }
    transferToImageBitmap() {
      return { width: this.width, height: this.height, close: vi.fn() };
    }
  };
});
afterEach(() => {
  (globalThis as any).OffscreenCanvas = OriginalOffscreenCanvas;
  (globalThis as any).ImageData = OriginalImageData;
});

describe("adjustment bake binary transport", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    localStorage.setItem("photrez.rustPixels", "1");
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
  });
  afterEach(() => {
    localStorage.removeItem("photrez.rustPixels");
    localStorage.removeItem("photrez.facade");
    localStorage.removeItem("photrez.facadeAuthority");
    vi.restoreAllMocks();
  });

  // Defeat: send init bytes as a plain number array (useLayerActions.ts:347) instead of Uint8Array; the toBeInstanceOf(Uint8Array) check goes RED.
  it("sends init bytes + write rgba as Uint8Array through the real bake handler", async () => {
    const ws = new WorkspaceManager();
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-bake", "Bake", 100, 100));
    ws.switchDocument("doc-bake");
    const engine = ws.getEngine("doc-bake")!;
    const layer = engine.addLayer("Bake", 100, 100);
    engine.setActiveLayer(layer.id);
    layer.basicAdjustment = { brightness: 20, contrast: 0, saturation: 0 };
    engine.setLayerImageBitmap(layer.id, { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap);
    vi.spyOn(engine, "commitBasicAdjustment").mockResolvedValue("cpu");
    const surface = { context: { putImageData: vi.fn() }, pixelEpoch: 0, pixelVersion: 0 };
    vi.spyOn(engine, "getPaintSurface").mockReturnValue(surface as never);

    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") throw new Error("layer not initialized");
      if (cmd === "rust_pixels_init") return undefined;
      if (cmd === "rust_pixels_write_region") {
        return {
          before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }],
          after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }],
          epoch: 1,
          version: 1,
        };
      }
      return undefined;
    });

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

    await result.handleApplyAdjustment();

    await vi.waitFor(() => {
      expect(mockInvoke.mock.calls.some((c) => c[0] === "rust_pixels_write_region")).toBe(true);
    });
    const init = mockInvoke.mock.calls.find((c) => c[0] === "rust_pixels_init")![1];
    expect(init.bytes).toBeInstanceOf(Uint8Array);
    expect(init.bytes.length).toBe(100 * 100 * 4);
    const wr = mockInvoke.mock.calls.find((c) => c[0] === "rust_pixels_write_region")![1];
    expect(wr.rgba).toBeInstanceOf(Uint8Array);
    expect(wr.rgba.length).toBe(100 * 100 * 4);
  });
});

describe("keeps photrez.rustPixels-OFF behavior (transitional; delete when the flag is retired)", () => {
  const W = 100;
  const H = 100;
  const preBakeBytes = new Uint8ClampedArray(W * H * 4).fill(1);
  const postBakeBytes = new Uint8ClampedArray(W * H * 4).fill(200);

  const makeBitmap = (bytes: Uint8ClampedArray) =>
    ({ width: W, height: H, close: vi.fn(), _bytes: bytes }) as unknown as ImageBitmap;

  async function renderBakeHandler(surfaceReady: boolean) {
    const ws = new WorkspaceManager();
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-bake", "Bake", W, H));
    ws.switchDocument("doc-bake");
    const engine = ws.getEngine("doc-bake")!;
    const layer = engine.addLayer("Bake", W, H);
    engine.setActiveLayer(layer.id);
    layer.basicAdjustment = { brightness: 20, contrast: 0, saturation: 0 };
    engine.setLayerImageBitmap(layer.id, makeBitmap(preBakeBytes));
    vi.spyOn(engine, "commitBasicAdjustment").mockImplementation(async (id: string) => {
      engine.setLayerImageBitmap(id, makeBitmap(postBakeBytes));
      return "cpu" as const;
    });
    const surface = { context: { putImageData: vi.fn() }, pixelEpoch: 0, pixelVersion: 0 };
    vi.spyOn(engine, "getPaintSurface").mockReturnValue(
      surfaceReady ? (surface as never) : null,
    );
    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") throw new Error("layer not initialized");
      if (cmd === "rust_pixels_init") return undefined;
      if (cmd === "rust_pixels_write_region") {
        return {
          before: [{ x: 0, y: 0, w: W, h: H, data: new Array(W * H * 4).fill(0) }],
          after: [{ x: 0, y: 0, w: W, h: H, data: Array.from(args.rgba) }],
          epoch: 1,
          version: 1,
        };
      }
      return undefined;
    });
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
    return { ws, engine, layer, renderer, result, commitSpy, surface };
  }

  beforeEach(() => {
    mockInvoke.mockReset();
    vi.mocked(showToast).mockClear();
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
  });
  afterEach(() => {
    localStorage.removeItem("photrez.rustPixels");
    localStorage.removeItem("photrez.facade");
    localStorage.removeItem("photrez.facadeAuthority");
    vi.restoreAllMocks();
  });

  it("flag ON writes the post-bake bytes once and commits one history step", async () => {
    expect(preBakeBytes).not.toEqual(postBakeBytes);
    localStorage.setItem("photrez.rustPixels", "1");
    const h = await renderBakeHandler(true);

    await h.result.handleApplyAdjustment();

    await vi.waitFor(() => {
      expect(mockInvoke.mock.calls.filter((c) => c[0] === "rust_pixels_write_region")).toHaveLength(1);
    });
    await vi.waitFor(() => expect(h.commitSpy).toHaveBeenCalledTimes(1));

    const wr = mockInvoke.mock.calls.find((c) => c[0] === "rust_pixels_write_region")![1];
    expect(Array.from(wr.rgba)).toEqual(Array.from(postBakeBytes));
    expect(Array.from(wr.rgba)).not.toEqual(Array.from(preBakeBytes));
    // Mutually exclusive with the legacy arm: no TS bitmap upload on the Rust arm.
    expect(h.renderer.uploadImage).not.toHaveBeenCalled();
    expect(
      vi.mocked(showToast).mock.calls.filter((c) => String(c[0]).startsWith("Adjustment Bake failed")),
    ).toEqual([]);
    await vi.waitFor(() => {
      const bakes = h.commitSpy.mock.calls.filter((c) => c[1] === "Apply Adjustment");
      expect(bakes).toHaveLength(1);
      expect(bakes[0][2]?.before?.length).toBeGreaterThan(0);
      expect(bakes[0][2]?.after?.length).toBeGreaterThan(0);
    });
  });

  it("flag OFF keeps the legacy bitmap arm with zero write_region calls", async () => {
    localStorage.removeItem("photrez.rustPixels");
    const h = await renderBakeHandler(true);

    await h.result.handleApplyAdjustment();

    await vi.waitFor(() => expect(h.renderer.uploadImage).toHaveBeenCalledTimes(1));
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "rust_pixels_write_region")).toHaveLength(0);
    expect(h.commitSpy).toHaveBeenCalledTimes(1);
    expect(h.renderer.uploadSurfaceTiles).not.toHaveBeenCalled();
  });

  it("flag ON without a ready surface surfaces an error instead of a silent legacy write", async () => {
    localStorage.setItem("photrez.rustPixels", "1");
    const h = await renderBakeHandler(false);

    await h.result.handleApplyAdjustment();

    await vi.waitFor(() => {
      expect(vi.mocked(showToast)).toHaveBeenCalledWith("Rust pixel surface not ready", "warn");
    });
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "rust_pixels_write_region")).toHaveLength(0);
  });
});

// The bridge gate must be ON for this pin to mean anything: with it off,
// commit() skips the Rust append entirely and a zero count proves nothing.
describe("adjustment bake commit pin (history bridge ON)", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    localStorage.setItem("photrez.rustPixels", "1");
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    localStorage.setItem("photrez.historyBridge", "1");
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
  });
  afterEach(() => {
    localStorage.removeItem("photrez.rustPixels");
    localStorage.removeItem("photrez.facade");
    localStorage.removeItem("photrez.facadeAuthority");
    localStorage.removeItem("photrez.historyBridge");
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    vi.restoreAllMocks();
  });

  it("records ZERO apply_tile_patch: rust_pixels_write_region already owns the entry", async () => {
    const ws = new WorkspaceManager();
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-bake", "Bake", 100, 100));
    ws.switchDocument("doc-bake");
    const engine = ws.getEngine("doc-bake")!;
    const layer = engine.addLayer("Bake", 100, 100);
    engine.setActiveLayer(layer.id);
    layer.basicAdjustment = { brightness: 20, contrast: 0, saturation: 0 };
    engine.setLayerImageBitmap(layer.id, { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap);
    vi.spyOn(engine, "commitBasicAdjustment").mockResolvedValue("cpu");
    const surface = { context: { putImageData: vi.fn() }, pixelEpoch: 0, pixelVersion: 0 };
    vi.spyOn(engine, "getPaintSurface").mockReturnValue(surface as never);

    mockInvoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "rust_pixels_get_epoch") throw new Error("layer not initialized");
      if (cmd === "rust_pixels_init") return undefined;
      if (cmd === "rust_pixels_write_region") {
        return {
          before: [{ x: 0, y: 0, w: 100, h: 100, data: new Array(100 * 100 * 4).fill(0) }],
          after: [{ x: 0, y: 0, w: 100, h: 100, data: Array.from(args.rgba) }],
          epoch: 1,
          version: 1,
        };
      }
      return undefined;
    });

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

    expect(historyBridgeEnabled()).toBe(true);
    await result.handleApplyAdjustment();

    const history = ws.getActiveHistory()!;
    await vi.waitFor(() => expect(history.getUndoCount()).toBe(1), { timeout: 2000 });
    for (let i = 0; i < 3; i++) {
      await new Promise((r) => setTimeout(r, 0));
      await flushPixelInvokeCensus();
    }
    // Positive control first: the bake reached Rust exactly once.
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "rust_pixels_write_region")).toHaveLength(1);
    expect(mockInvoke.mock.calls.filter((c) => c[0] === "apply_tile_patch")).toHaveLength(0);
  });
});
