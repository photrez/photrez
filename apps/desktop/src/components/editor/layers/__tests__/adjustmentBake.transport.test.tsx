// SPDX-License-Identifier: AGPL-3.0-or-later
// Binary transport pins: adjustment-bake invoke args must cross as Uint8Array,
// not JSON number arrays. Counts/bytes only, no timing.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook } from "@solidjs/testing-library";
import { EditorProvider } from "../../shell/EditorContext";
import { DialogProvider } from "../../dialogs/DialogProvider";
import { WorkspaceManager } from "@/engine/workspace";
import { useLayerActions } from "../useLayerActions";

const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args: any) => mockInvoke(cmd, args),
}));

vi.mock("@/lib/rustShadow", async (importOriginal) => {
  const actual = (await importOriginal()) as any;
  return { ...actual, rehydratePaintSurfaceFromRust: vi.fn() };
});

const OriginalOffscreenCanvas = (globalThis as any).OffscreenCanvas;
beforeEach(() => {
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
        drawImage: vi.fn(),
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
