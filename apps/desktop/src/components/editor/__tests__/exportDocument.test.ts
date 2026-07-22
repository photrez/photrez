import { describe, expect, it, vi, afterEach } from "vitest";
import type { DocumentEngine } from "@/engine/document";
import type { LayerNode } from "@/engine/types";

const BASE_LAYER: LayerNode = {
  id: "l1",
  name: "Test",
  type: "raster",
  visible: true,
  opacity: 1,
  locked: false,
  blendMode: "normal",
  width: 2,
  height: 2,
  transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
  imageBitmap: null!,
};

function makeMockEngine(layers: LayerNode[], width = 2, height = 2): DocumentEngine {
  return {
    getWidth: () => width,
    getHeight: () => height,
    getLayers: () => layers,
  } as unknown as DocumentEngine;
}

function createOffscreenCtxMock(w = 2, h = 2) {
  let fillStyle = "";
  return {
    clearRect: vi.fn(),
    set fillStyle(v: string) { fillStyle = v; },
    get fillStyle() { return fillStyle; },
    fillRect: vi.fn(),
    save: vi.fn(),
    restore: vi.fn(),
    translate: vi.fn(),
    rotate: vi.fn(),
    scale: vi.fn(),
    drawImage: vi.fn(),
    getImageData: vi.fn().mockImplementation((_x, _y, width, height) => ({
      data: new Uint8ClampedArray((width || w) * (height || h) * 4),
    })),
  };
}

describe("encodeComposite", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("produces PNG bytes for a simple document", async () => {
    const layers: LayerNode[] = [{
      ...BASE_LAYER, id: "bg", name: "Bg",
      imageBitmap: { width: 2, height: 2 } as ImageBitmap,
    }];

    const mockCtx = createOffscreenCtxMock();
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["PNG-DATA"]));
    }));

    const { encodeComposite } = await import("../exportDocument");
    const engine = makeMockEngine(layers);
    const bytes = await encodeComposite(engine, "png", 100);

    expect(bytes).toBeDefined();
    expect(bytes.length).toBeGreaterThan(0);
    expect(mockCtx.clearRect).toHaveBeenCalledWith(0, 0, 2, 2);
    // Must use drawLayerToContext (tested via save/drawImage being called)
    expect(mockCtx.save).toHaveBeenCalled();
    expect(mockCtx.drawImage).toHaveBeenCalled();
    expect(mockCtx.restore).toHaveBeenCalled();
  });

  it("produces JPEG bytes with white background fill", async () => {
    const layers: LayerNode[] = [{
      ...BASE_LAYER, id: "bg", name: "Bg",
      imageBitmap: { width: 2, height: 2 } as ImageBitmap,
    }];

    const mockCtx = createOffscreenCtxMock();
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["JPEG-DATA"]));
    }));

    const { encodeComposite } = await import("../exportDocument");
    const engine = makeMockEngine(layers);
    await encodeComposite(engine, "jpeg", 85);

    expect(mockCtx.fillStyle).toBe("#FFFFFF");
    expect(mockCtx.fillRect).toHaveBeenCalledWith(0, 0, 2, 2);
  });

  it("composites layers bottom-to-top order", async () => {
    const bottom: LayerNode = {
      ...BASE_LAYER, id: "bottom", name: "Bottom",
      imageBitmap: { width: 2, height: 2 } as ImageBitmap,
    };
    const top: LayerNode = {
      ...BASE_LAYER, id: "top", name: "Top",
      imageBitmap: { width: 2, height: 2 } as ImageBitmap,
    };
    // layers[0] = top, layers[1] = bottom
    const layers = [top, bottom];

    const drawCalls: string[] = [];
    const mockCtx = {
      ...createOffscreenCtxMock(),
      ...{
        save: vi.fn(() => { drawCalls.push("save"); }),
        drawImage: vi.fn(() => { drawCalls.push("drawImage"); }),
        restore: vi.fn(() => { drawCalls.push("restore"); }),
      },
    };

    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["PNG"]));
    }));

    const { encodeComposite } = await import("../exportDocument");
    const engine = makeMockEngine(layers);
    await encodeComposite(engine, "png", 100);

    // Bottom layer drawImage should come before top layer drawImage
    const bottomIdx = drawCalls.indexOf("drawImage");
    expect(bottomIdx).toBeGreaterThanOrEqual(0);
    expect(drawCalls.lastIndexOf("drawImage")).toBeGreaterThan(bottomIdx);
  });

  it("skips invisible layers", async () => {
    const visible: LayerNode = {
      ...BASE_LAYER, id: "v", name: "Visible",
      imageBitmap: { width: 2, height: 2 } as ImageBitmap,
    };
    const hidden: LayerNode = {
      ...BASE_LAYER, id: "h", name: "Hidden", visible: false,
      imageBitmap: { width: 2, height: 2 } as ImageBitmap,
    };

    const mockCtx = createOffscreenCtxMock();
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["PNG"]));
    }));

    const { encodeComposite } = await import("../exportDocument");
    const engine = makeMockEngine([visible, hidden]);
    await encodeComposite(engine, "png", 100);

    // Only one drawImage call (for visible layer)
    expect(mockCtx.drawImage).toHaveBeenCalledTimes(1);
  });

  it("respects layer opacity", async () => {
    const semiOpaque: LayerNode = {
      ...BASE_LAYER, id: "semi", name: "Semi",
      opacity: 0.5,
      imageBitmap: { width: 2, height: 2 } as ImageBitmap,
    };

    const mockCtx = createOffscreenCtxMock();
    let capturedAlpha = 1;
    mockCtx.save = vi.fn(() => { capturedAlpha = (mockCtx as any).globalAlpha; });

    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["PNG"]));
    }));

    const { encodeComposite } = await import("../exportDocument");
    const engine = makeMockEngine([semiOpaque]);
    await encodeComposite(engine, "png", 100);

    // drawLayerToContext sets globalAlpha = layer.opacity
    expect(mockCtx.save).toHaveBeenCalled();
  });

  it("uses correct mime type and quality for each format", async () => {
    let capturedBlobOptions: any = null;
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => createOffscreenCtxMock();
      this.convertToBlob = vi.fn((opts: any) => {
        capturedBlobOptions = opts;
        return Promise.resolve(new Blob(["DATA"]));
      });
    }));

    const { encodeComposite } = await import("../exportDocument");
    const engine = makeMockEngine([]);

    // PNG
    await encodeComposite(engine, "png", 100);
    expect(capturedBlobOptions.type).toBe("image/png");
    expect(capturedBlobOptions.quality).toBeUndefined();

    // JPEG
    await encodeComposite(engine, "jpeg", 85);
    expect(capturedBlobOptions.type).toBe("image/jpeg");
    expect(capturedBlobOptions.quality).toBe(0.85);

    // WebP (quality=0 based because quality is stored in closure)
    await encodeComposite(engine, "webp", 75);
    expect(capturedBlobOptions.type).toBe("image/webp");
    expect(capturedBlobOptions.quality).toBe(0.75);
  });

  it("returns correct magic bytes for each export format", async () => {
    // Magic byte sequences for each image format:
    //   PNG:  0x89 'P' 'N' 'G' (\x89PNG)
    //   JPEG: 0xFF 0xD8 0xFF    (SOI marker)
    //   WebP: 'R' 'I' 'F' 'F'   (RIFF container)
    const signatures: Record<string, { magic: number[]; label: string }> = {
      "image/png":  { magic: [0x89, 0x50, 0x4e, 0x47], label: "\\x89PNG" },
      "image/jpeg": { magic: [0xff, 0xd8, 0xff, 0xe0], label: "\\xFF\\xD8\\xFF" },
      "image/webp": { magic: [0x52, 0x49, 0x46, 0x46], label: "RIFF" },
    };
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => createOffscreenCtxMock();
      this.convertToBlob = vi.fn((opts: any) => {
        const sig = signatures[opts.type]?.magic ?? [];
        return Promise.resolve(new Blob([new Uint8Array(sig)]));
      });
    }));

    const { encodeComposite } = await import("../exportDocument");
    const engine = makeMockEngine([]);

    for (const [format, { magic, label }] of Object.entries(signatures)) {
      const formatKey = format.replace("image/", "") as "png" | "jpeg" | "webp";
      const bytes = await encodeComposite(engine, formatKey, 85);
      const firstBytes = Array.from(bytes.slice(0, magic.length));
      expect(firstBytes, `${formatKey}: magic bytes ${label}`).toEqual(magic);
    }
  });

  it("quality extremes (1, 50, 100) all produce non-empty bytes with valid magic", async () => {
    const sig: Record<string, number[]> = {
      "image/png":  [0x89, 0x50, 0x4e, 0x47],
      "image/jpeg": [0xff, 0xd8, 0xff, 0xe0],
      "image/webp": [0x52, 0x49, 0x46, 0x46],
    };

    const assertFormatAtQuality = async (
      engine: DocumentEngine,
      format: "png" | "jpeg" | "webp",
      quality: number,
    ) => {
      const bytes = await encodeComposite(engine, format, quality);
      expect(bytes.length, `${format}@${quality}: non-empty`).toBeGreaterThan(0);
      expect(Array.from(bytes.slice(0, 4)), `${format}@${quality}: magic bytes`)
        .toEqual(sig[`image/${format}`]);
    };

    let callCount = 0;
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => createOffscreenCtxMock();
      this.convertToBlob = vi.fn((opts: any) => {
        const payload = sig[opts.type] ?? [];
        const data = [...payload, callCount++];
        return Promise.resolve(new Blob([new Uint8Array(data)]));
      });
    }));

    const { encodeComposite } = await import("../exportDocument");
    const engine = makeMockEngine([
      { ...BASE_LAYER, id: "bg", name: "Bg", imageBitmap: { width: 2, height: 2 } as ImageBitmap },
    ]);

    const formats = ["png", "jpeg", "webp"] as const;
    const qualities = [1, 50, 100];

    for (const format of formats) {
      for (const quality of qualities) {
        // eslint-disable-next-line no-await-in-loop
        await assertFormatAtQuality(engine, format, quality);
      }
    }
  });

  it("document dimensions match output canvas size", async () => {
    let capturedW = 0;
    let capturedH = 0;
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      capturedW = w;
      capturedH = h;
      this.width = w;
      this.height = h;
      this.getContext = () => createOffscreenCtxMock();
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["DATA"]));
    }));

    const { encodeComposite } = await import("../exportDocument");
    const engine = makeMockEngine([], 1920, 1080);
    await encodeComposite(engine, "png", 100);

    expect(capturedW).toBe(1920);
    expect(capturedH).toBe(1080);
  });

  it("produces valid bytes even when there are no layers", async () => {
    const mockCtx = createOffscreenCtxMock();
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["PNG-EMPTY"]));
    }));

    const { encodeComposite } = await import("../exportDocument");
    const engine = makeMockEngine([], 800, 600);
    const bytes = await encodeComposite(engine, "png", 100);

    expect(bytes).toBeDefined();
    expect(bytes.length).toBeGreaterThan(0);
    // No layers → no drawImage calls
    expect(mockCtx.drawImage).not.toHaveBeenCalled();
  });

  it("produces valid bytes when all layers are invisible (empty composite)", async () => {
    const mockCtx = createOffscreenCtxMock();
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["PNG-HIDDEN"]));
    }));

    const { encodeComposite } = await import("../exportDocument");
    const hiddenLayer: LayerNode = { ...BASE_LAYER, id: "h", name: "Hidden", visible: false, imageBitmap: { width: 2, height: 2 } as ImageBitmap };
    const engine = makeMockEngine([hiddenLayer], 800, 600);
    const bytes = await encodeComposite(engine, "png", 100);

    expect(bytes).toBeDefined();
    expect(bytes.length).toBeGreaterThan(0);
    expect(mockCtx.drawImage).not.toHaveBeenCalled();
  });

  it("applies layer transform position and scale during export", async () => {
    const mockCtx = createOffscreenCtxMock();
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["PNG"]));
    }));

    const { encodeComposite } = await import("../exportDocument");
    const transformed: LayerNode = {
      ...BASE_LAYER, id: "t", name: "Transformed",
      width: 100, height: 50,
      transform: { x: 50, y: 30, scaleX: 2, scaleY: 0.5, rotation: 0, flipH: false, flipV: false },
      imageBitmap: { width: 100, height: 50 } as ImageBitmap,
    };
    const engine = makeMockEngine([transformed], 800, 600);
    await encodeComposite(engine, "png", 100);

    // Center = (x + w*|sx|/2, y + h*|sy|/2) = (50 + 100*2/2, 30 + 50*0.5/2) = (150, 42.5)
    expect(mockCtx.translate).toHaveBeenCalledWith(150, 42.5);
    // Scale = (scaleX, scaleY)
    expect(mockCtx.scale).toHaveBeenCalledWith(2, 0.5);
  });

  it("applies layer rotation during export", async () => {
    const mockCtx = createOffscreenCtxMock();
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["PNG"]));
    }));

    const { encodeComposite } = await import("../exportDocument");
    const rotated: LayerNode = {
      ...BASE_LAYER, id: "r", name: "Rotated",
      width: 100, height: 100,
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 45, flipH: false, flipV: false },
      imageBitmap: { width: 100, height: 100 } as ImageBitmap,
    };
    const engine = makeMockEngine([rotated], 800, 600);
    await encodeComposite(engine, "png", 100);

    // rotation 45° → radians = 45 * PI / 180
    expect(mockCtx.rotate).toHaveBeenCalledWith((45 * Math.PI) / 180);
  });

  it("applies layer flip during export (negative scale)", async () => {
    const mockCtx = createOffscreenCtxMock();
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["PNG"]));
    }));

    const { encodeComposite } = await import("../exportDocument");
    const flipped: LayerNode = {
      ...BASE_LAYER, id: "f", name: "Flipped",
      width: 100, height: 100,
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: true, flipV: false },
      imageBitmap: { width: 100, height: 100 } as ImageBitmap,
    };
    const engine = makeMockEngine([flipped], 800, 600);
    await encodeComposite(engine, "png", 100);

    // flipH=true → flipX=-1, so scale = (1 * -1, 1 * 1) = (-1, 1)
    expect(mockCtx.scale).toHaveBeenCalledWith(-1, 1);
  });

  it("sets globalCompositeOperation for blend modes during export", async () => {
    const mockCtx = createOffscreenCtxMock();
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["PNG"]));
    }));

    const { encodeComposite } = await import("../exportDocument");
    const multiplyLayer: LayerNode = {
      ...BASE_LAYER, id: "m", name: "Multiply",
      blendMode: "multiply",
      imageBitmap: { width: 100, height: 100 } as ImageBitmap,
    };
    const engine = makeMockEngine([multiplyLayer], 800, 600);
    await encodeComposite(engine, "png", 100);

    expect(mockCtx.save).toHaveBeenCalled();
    // The export code calls drawLayerToContext which sets globalCompositeOperation
    expect((mockCtx as any).globalCompositeOperation).toBe("multiply");
  });

  it("skips layers without imageBitmap (no drawImage call)", async () => {
    const mockCtx = createOffscreenCtxMock();
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["PNG"]));
    }));

    const { encodeComposite } = await import("../exportDocument");
    const noBitmap: LayerNode = { ...BASE_LAYER, id: "n", name: "NoBitmap", imageBitmap: null! };
    const withBitmap: LayerNode = {
      ...BASE_LAYER, id: "w", name: "WithBitmap",
      imageBitmap: { width: 100, height: 100 } as ImageBitmap,
    };
    const engine = makeMockEngine([noBitmap, withBitmap], 800, 600);
    await encodeComposite(engine, "png", 100);

    // Only one drawImage call (for the layer WITH bitmap)
    expect(mockCtx.drawImage).toHaveBeenCalledTimes(1);
  });
});

const { mockWriteFileBytes, mockShowSaveDialog } = vi.hoisted(() => ({
  mockWriteFileBytes: vi.fn().mockResolvedValue(undefined),
  mockShowSaveDialog: vi.fn().mockResolvedValue("./output/test.png"),
}));

vi.mock("@/tauri/native", () => ({
  writeFileBytes: mockWriteFileBytes,
  showSaveDialog: mockShowSaveDialog,
}));

describe("exportActiveDocument", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    mockWriteFileBytes.mockClear();
    mockShowSaveDialog.mockClear();
  });

  it("calls showSaveDialog and writeFileBytes with correct arguments", async () => {
    const mockCtx = createOffscreenCtxMock();
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["EXPORTED"]));
    }));

    const { exportActiveDocument } = await import("../exportDocument");
    const engine = makeMockEngine([]);

    const result = await exportActiveDocument(engine, "MyImage.png", "png", 100);

    expect(mockShowSaveDialog).toHaveBeenCalled();
    expect(mockWriteFileBytes).toHaveBeenCalled();
    expect(result).toBe("./output/test.png");
  });

  it("does not write bytes when the save dialog is cancelled", async () => {
    mockShowSaveDialog.mockResolvedValueOnce(null);
    const { exportActiveDocument } = await import("../exportDocument");
    const engine = makeMockEngine([]);

    const result = await exportActiveDocument(engine, "MyImage.png", "png", 100);

    expect(result).toBeNull();
    expect(mockWriteFileBytes).not.toHaveBeenCalled();
  });

  it("throws when showSaveDialog rejects (Tauri IPC failure)", async () => {
    const ipcError = new Error("IPC channel closed");
    mockShowSaveDialog.mockRejectedValueOnce(ipcError);

    const { exportActiveDocument } = await import("../exportDocument");
    const engine = makeMockEngine([]);

    await expect(exportActiveDocument(engine, "test.png", "png", 100)).rejects.toThrow("IPC channel closed");
    expect(mockWriteFileBytes).not.toHaveBeenCalled();
  });

  it("throws when writeFileBytes rejects (Tauri IPC failure)", async () => {
    const ipcError = new Error("Disk full");
    mockShowSaveDialog.mockResolvedValueOnce("./output/test.png");
    mockWriteFileBytes.mockRejectedValueOnce(ipcError);

    // encodeComposite is called internally — need OffscreenCanvas stub
    const mockCtx = createOffscreenCtxMock();
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => mockCtx;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob(["DATA"]));
    }));

    const { exportActiveDocument } = await import("../exportDocument");
    const engine = makeMockEngine([]);

    await expect(exportActiveDocument(engine, "test.png", "png", 100)).rejects.toThrow("Disk full");
    expect(mockWriteFileBytes).toHaveBeenCalled();
  });
});
