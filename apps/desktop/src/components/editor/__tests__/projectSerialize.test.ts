// apps/desktop/src/components/editor/__tests__/projectSerialize.test.ts
//
// Contract tests for .ptz project serialization.
//
// Catches the "pure functions pass but save/load silently corrupts data" pattern:
// `serializeAndSaveProject` is driven for real (only the Tauri boundary is
// mocked), and the document it produces is inspected at each layer.
//
// The save/load ROUND-TRIP cases live in `projectSerializeRoundtrip.test.ts`.
// The document bytes are produced by `ptzWriterContract.ts`, which is diffed
// against the real Rust writer in `ptzWriterGolden.test.ts`.

import { describe, expect, it, vi, afterEach, beforeEach } from "vitest";
import { DocumentEngine } from "@/engine/document";
import type { DocumentModel, LayerNode, ShapeParams } from "@/engine/types";
import type { TextData } from "@/engine/textTypes";
// Serializes exactly as the production Rust writer does; diffed against the real
// Rust bytes in `ptzWriterGolden.test.ts`.
import { emulateRustSerialization } from "./ptzWriterContract";

// ─── Hoisted mocks for @/tauri/native ───
const { mockSaveProjectStreamingBegin, mockSaveProjectStreamingWriteLayer, mockSaveProjectStreamingEnd, mockSaveProjectStreamingCancel, mockLoadProject } = vi.hoisted(() => ({
  mockSaveProjectStreamingBegin: vi.fn<(path: string, document: unknown) => Promise<string>>(),
  mockSaveProjectStreamingWriteLayer: vi.fn<(handleId: string, layerId: string, pngBytes: Uint8Array) => Promise<void>>(),
  mockSaveProjectStreamingEnd: vi.fn<(handleId: string) => Promise<void>>(),
  mockSaveProjectStreamingCancel: vi.fn<(handleId: string) => Promise<void>>(),
  mockLoadProject: vi.fn<(path: string) => Promise<{ document_json: string; layers: Record<string, string> }>>(),
}));

vi.mock("@/tauri/native", () => ({
  saveProjectStreamingBegin: mockSaveProjectStreamingBegin,
  saveProjectStreamingWriteLayer: mockSaveProjectStreamingWriteLayer,
  saveProjectStreamingEnd: mockSaveProjectStreamingEnd,
  saveProjectStreamingCancel: mockSaveProjectStreamingCancel,
  loadProject: mockLoadProject,
}));

// ─── Helpers ───

/** Creates a minimal mock ImageBitmap of given dimensions with RGBA pixel data. */
function makeBitmap(width: number, height: number, _fill: Uint8ClampedArray): ImageBitmap {
  // OffscreenCanvas is not available in jsdom; we mock it.  But for the test we
  // just need an object that looks like an ImageBitmap — the canvas mock
  // in serializeAndSaveProject will drawImage it, and convertToBlob returns PNG.
  return { width, height, close: vi.fn() } as unknown as ImageBitmap;
}

/** Captured data from a mocked serialize call — aggregated from streaming calls. */
interface CapturedProject {
  path: string;
  documentJson: string;
  layers: Record<string, Uint8Array>;
}

let capturedProject: CapturedProject | null = null;
let nextHandleId = 1;

/** OffscreenCanvas mock context with controllable convertToBlob.
 *  The 2D context is a full-enough stub that both the engine shape
 *  rasterizer (translate/fill/style/beginPath/path ops) and the serializer
 *  encode path (drawImage/convertToBlob) can run under jsdom. */
function createCanvasMock(pngBytes: Uint8Array) {
  const mkCtx = () => {
    const ctx: any = {};
    ctx.fillStyle = "#000000";
    ctx.strokeStyle = "#000000";
    ctx.lineWidth = 1;
    ctx.lineCap = "butt";
    ctx.translate = vi.fn(() => ctx);
    ctx.beginPath = vi.fn();
    ctx.rect = vi.fn();
    ctx.roundRect = vi.fn();
    ctx.ellipse = vi.fn();
    ctx.moveTo = vi.fn();
    ctx.lineTo = vi.fn();
    ctx.fill = vi.fn();
    ctx.stroke = vi.fn();
    ctx.drawImage = vi.fn();
    ctx.clearRect = vi.fn();
    ctx.save = vi.fn();
    ctx.restore = vi.fn();
    // Text-capable seam so the REAL engine text rasterizer runs under jsdom
    // (mirrors the shape-rasterizer extension added for shape v2).
    ctx.font = "";
    ctx.textBaseline = "alphabetic";
    ctx.letterSpacing = "0px";
    ctx.measureText = vi.fn(() => ({
      width: 10,
      fontBoundingBoxAscent: 80,
      fontBoundingBoxDescent: 24,
    }));
    ctx.fillText = vi.fn();
    ctx.strokeText = vi.fn();
    ctx.closePath = vi.fn();
    return ctx;
  };
  return {
    width: 0,
    height: 0,
    getContext: () => mkCtx(),
    transferToImageBitmap: function (this: any) {
      return makeBitmap(Math.max(1, this.width), Math.max(1, this.height), new Uint8ClampedArray(0));
    },
    convertToBlob: vi.fn().mockResolvedValue(new Blob([pngBytes as BlobPart], { type: "image/png" })),
  };
}

/** Stubs global OffscreenCanvas so the engine shape rasterizer AND serialize
 *  encode path can run under jsdom. */
function stubSerializeGlobals(pngBytes: Uint8Array) {
  const mockCanvas = createCanvasMock(pngBytes);
  vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    this.getContext = () => mockCanvas.getContext();
    this.transferToImageBitmap = mockCanvas.transferToImageBitmap;
    this.convertToBlob = mockCanvas.convertToBlob;
  }));
}

// ─── Tests ───

describe("projectSerialize — serializeAndSaveProject", () => {
  const PNG_BYTES = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d]);

  beforeEach(() => {
    capturedProject = null;
    nextHandleId = 1;
    mockSaveProjectStreamingBegin.mockClear();
    mockSaveProjectStreamingWriteLayer.mockClear();
    mockSaveProjectStreamingEnd.mockClear();
    mockSaveProjectStreamingCancel.mockClear();
    mockLoadProject.mockClear();

    // Streaming calls aggregate into capturedProject.layers.
    // The host sends the document as a VALUE; Rust serializes it to the
    // `document.json` bytes. These tests emulate that Rust step so they still
    // assert on a written document (the real writer is proven in Rust:
    // photrez-core `ptz_document` and photrez-desktop `save_stream`).
    mockSaveProjectStreamingBegin.mockImplementation(async (path, document) => {
      const handleId = `handle-${nextHandleId++}`;
      capturedProject = { path, documentJson: emulateRustSerialization(document), layers: {} };
      return handleId;
    });
    mockSaveProjectStreamingWriteLayer.mockImplementation(async (_handleId, layerId, pngBytes) => {
      if (capturedProject) {
        capturedProject.layers[layerId] = pngBytes;
      }
    });
    mockSaveProjectStreamingEnd.mockImplementation(async () => { /* no-op */ });
    mockSaveProjectStreamingCancel.mockImplementation(async () => { capturedProject = null; });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("calls saveProject with correct arguments for a single-layer document", async () => {
    stubSerializeGlobals(PNG_BYTES);

    const engine = new DocumentEngine("doc-save-1", "Test Doc", 100, 80);
    const l1 = engine.addLayer("Layer 1", 100, 80);
    engine.setLayerImageBitmap(l1.id, makeBitmap(100, 80, new Uint8ClampedArray(100 * 80 * 4)));

    const { serializeAndSaveProject } = await import("../projectSerialize");

    await serializeAndSaveProject(engine, "/path/to/project.ptz");

    expect(mockSaveProjectStreamingBegin).toHaveBeenCalledTimes(1);
    expect(mockSaveProjectStreamingWriteLayer).toHaveBeenCalled(); // at least 1 layer
    expect(mockSaveProjectStreamingEnd).toHaveBeenCalledTimes(1);
    expect(capturedProject).not.toBeNull();
    expect(capturedProject!.path).toBe("/path/to/project.ptz");
    expect(capturedProject!.layers[l1.id]).toBeDefined();
    // Binary layer bytes should match the encoded PNG (no base64 round-trip).
    expect(capturedProject!.layers[l1.id]).toEqual(PNG_BYTES);
  });

  it("serialized document JSON has imageBitmap set to null for each layer", async () => {
    stubSerializeGlobals(PNG_BYTES);

    const engine = new DocumentEngine("doc-null-bmp", "Null Bitmap", 50, 50);
    const l1 = engine.addLayer("A", 50, 50);
    engine.setLayerImageBitmap(l1.id, makeBitmap(50, 50, new Uint8ClampedArray(50 * 50 * 4)));
    engine.addLayer("B", 50, 50); // no imageBitmap

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/path/test.ptz");

    expect(capturedProject).not.toBeNull();
    const parsed = JSON.parse(capturedProject!.documentJson) as DocumentModel;

    expect(parsed.layers.length).toBe(2);
    for (const layer of parsed.layers) {
      expect(layer.imageBitmap).toBeNull();
    }
    // Layer "A" should have base64 data; Layer "B" should not
    expect(capturedProject!.layers[l1.id]).toBeDefined();
    expect(Object.keys(capturedProject!.layers).length).toBe(1);
  });

  it("includes document metadata in serialized JSON", async () => {
    stubSerializeGlobals(PNG_BYTES);

    const engine = new DocumentEngine("doc-meta", "Meta Doc", 1920, 1080);
    engine.addLayer("L1", 100, 100);

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/path/meta.ptz");

    const parsed = JSON.parse(capturedProject!.documentJson) as DocumentModel;
    expect(parsed.id).toBe("doc-meta");
    expect(parsed.name).toBe("Meta Doc");
    expect(parsed.width).toBe(1920);
    expect(parsed.height).toBe(1080);
  });

  it("writes photrez-ptz format + version:3 marker (.ptz v3 additive)", async () => {
    stubSerializeGlobals(PNG_BYTES);

    const engine = new DocumentEngine("doc-ver", "Versioned", 64, 64);
    engine.addLayer("L1", 64, 64);

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/path/ver.ptz");

    const parsed = JSON.parse(capturedProject!.documentJson) as DocumentModel & { format?: string; version?: number };
    expect(parsed.format).toBe("photrez-ptz");
    expect(parsed.version).toBe(3);
  });

  it("loader tolerates alpha.1 projects without a version field (backward-compatible)", async () => {
    stubSerializeGlobals(PNG_BYTES);

    const engine = new DocumentEngine("doc-legacy", "Legacy", 64, 64);
    engine.addLayer("L1", 64, 64);

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/path/legacy.ptz");

    // Simulate loadProjectFile parse path (editorOpenImage.ts): strip version, then parse.
    const parsed = JSON.parse(capturedProject!.documentJson) as DocumentModel & { format?: string; version?: number };
    delete parsed.version;
    delete parsed.format;
    const reloaded = JSON.parse(JSON.stringify(parsed)) as DocumentModel;
    // No crash, model intact — loadProjectFile handles missing version as compatible.
    expect(reloaded.id).toBe("doc-legacy");
    expect(reloaded.layers.length).toBe(1);
  });

  it("serializes layer properties (name, opacity, visible, blendMode, transform)", async () => {
    stubSerializeGlobals(PNG_BYTES);

    const engine = new DocumentEngine("doc-props", "Props", 100, 100);
    const l1 = engine.addLayer("Custom Name", 100, 100);
    engine.setLayerImageBitmap(l1.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));
    engine.setLayerOpacity(l1.id, 0.5);
    engine.setLayerVisibility(l1.id, false);
    engine.setLayerBlendMode(l1.id, "multiply");

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/path/props.ptz");

    const parsed = JSON.parse(capturedProject!.documentJson) as DocumentModel;
    const layer = parsed.layers.find(l => l.id === l1.id);
    expect(layer).toBeDefined();
    expect(layer!.name).toBe("Custom Name");
    expect(layer!.opacity).toBe(0.5);
    expect(layer!.visible).toBe(false);
    expect(layer!.blendMode).toBe("multiply");
    expect(layer!.transform).toEqual({ x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false });
  });

  it("does not fail on layers with null imageBitmap (no data saved)", async () => {
    stubSerializeGlobals(PNG_BYTES);

    const engine = new DocumentEngine("doc-null", "Null Layer", 100, 100);
    engine.addLayer("No Bitmap", 100, 100); // no imageBitmap set

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/path/null.ptz");

    expect(mockSaveProjectStreamingBegin).toHaveBeenCalledTimes(1);
    expect(mockSaveProjectStreamingEnd).toHaveBeenCalledTimes(1);
    const parsed = JSON.parse(capturedProject!.documentJson) as DocumentModel;
    expect(parsed.layers.length).toBe(1);
    expect(capturedProject!.layers).toEqual({});
  });

  it("handles multiple layers with and without bitmaps", async () => {
    stubSerializeGlobals(PNG_BYTES);

    const engine = new DocumentEngine("doc-multi", "Multi Layer", 200, 200);
    const l1 = engine.addLayer("BG", 200, 200);
    engine.setLayerImageBitmap(l1.id, makeBitmap(200, 200, new Uint8ClampedArray(200 * 200 * 4)));
    engine.addLayer("Empty", 100, 100); // no bitmap
    const l3 = engine.addLayer("Top", 100, 100);
    engine.setLayerImageBitmap(l3.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/path/multi.ptz");

    const parsed = JSON.parse(capturedProject!.documentJson) as DocumentModel;
    expect(parsed.layers.length).toBe(3);
    // Order follows iteration (top → bottom) then insertion order in the layers object.
    // Empty (no bitmap) is skipped, so only l3 (Top) and l1 (BG) are saved.
    expect(Object.keys(capturedProject!.layers)).toEqual([l3.id, l1.id]);
  });

  // ── Dirty layer cache tests ──
  it("only encodes dirty layers when cache is populated", async () => {
    stubSerializeGlobals(PNG_BYTES);

    const engine = new DocumentEngine("doc-cache-1", "Cache Doc", 100, 100);
    const l1 = engine.addLayer("A", 100, 100);
    engine.setLayerImageBitmap(l1.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));
    const l2 = engine.addLayer("B", 100, 100);
    engine.setLayerImageBitmap(l2.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));

    // Clean import so cache is module-level
    const { serializeAndSaveProject, clearLayerCache } = await import("../projectSerialize");
    clearLayerCache(engine.getId());

    let offscreenCount = 0;
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      offscreenCount++;
      this.width = w;
      this.height = h;
      this.getContext = () => ({ drawImage: vi.fn() });
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob([PNG_BYTES], { type: "image/png" }));
    }));

    // First save: both layers dirty → both encoded
    await serializeAndSaveProject(engine, "/path/cache1.ptz");
    expect(offscreenCount).toBe(2); // both layers encoded
    expect(Object.keys(capturedProject!.layers)).toContain(l1.id);
    expect(Object.keys(capturedProject!.layers)).toContain(l2.id);

    // Simulate clearDirty (as useEditorCommands does after a successful save)
    engine.clearDirty();

    // Second save: no dirty layers → both from cache
    offscreenCount = 0;
    await serializeAndSaveProject(engine, "/path/cache2.ptz");
    expect(offscreenCount).toBe(0); // no OffscreenCanvas created — all from cache
    expect(Object.keys(capturedProject!.layers)).toContain(l1.id);
    expect(Object.keys(capturedProject!.layers)).toContain(l2.id);
  });

  it("clean layers served from cache issue zero ensureBitmapCurrent calls", async () => {
    stubSerializeGlobals(PNG_BYTES);

    const engine = new DocumentEngine("doc-ensure-skip", "Ensure Skip", 100, 100);
    const l1 = engine.addLayer("A", 100, 100);
    engine.setLayerImageBitmap(l1.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));
    const l2 = engine.addLayer("B", 100, 100);
    engine.setLayerImageBitmap(l2.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));

    const { serializeAndSaveProject, clearLayerCache } = await import("../projectSerialize");
    clearLayerCache(engine.getId());

    // First save populates the cache; the second save serves both from cache.
    await serializeAndSaveProject(engine, "/path/ensure1.ptz");
    engine.clearDirty();

    const spy = vi.spyOn(engine, "ensureBitmapCurrent");
    await serializeAndSaveProject(engine, "/path/ensure2.ptz");
    // Clean layers never touch their bitmap, so no sync readback is needed.
    expect(spy).not.toHaveBeenCalled();
    expect(Object.keys(capturedProject!.layers)).toContain(l1.id);
    expect(Object.keys(capturedProject!.layers)).toContain(l2.id);
    spy.mockRestore();
  });

  it("caches carry-forward: edited layer re-encoded, clean layer from cache", async () => {
    const { serializeAndSaveProject, clearLayerCache } = await import("../projectSerialize");
    clearLayerCache("doc-cache-2");

    const engine = new DocumentEngine("doc-cache-2", "Incremental", 100, 100);
    const l1 = engine.addLayer("BG", 100, 100);
    engine.setLayerImageBitmap(l1.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));
    const l2 = engine.addLayer("Edit", 100, 100);
    engine.setLayerImageBitmap(l2.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));

    let offscreenCount = 0;
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      offscreenCount++;
      this.width = w;
      this.height = h;
      this.getContext = () => ({ drawImage: vi.fn() });
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob([PNG_BYTES], { type: "image/png" }));
    }));

    // First save: both encoded
    await serializeAndSaveProject(engine, "/path/inc1.ptz");
    expect(offscreenCount).toBe(2);

    // Clear dirty, simulate saved state
    engine.clearDirty();

    // Edit only layer 2
    engine.markLayerDirty(l2.id);

    // Second save: only l2 encoded, l1 from cache
    offscreenCount = 0;
    await serializeAndSaveProject(engine, "/path/inc2.ptz");
    expect(offscreenCount).toBe(1); // only l2
    expect(Object.keys(capturedProject!.layers)).toContain(l1.id);
    expect(Object.keys(capturedProject!.layers)).toContain(l2.id);
  });

  it("clears cache for deleted layers", async () => {
    const { serializeAndSaveProject, clearLayerCache } = await import("../projectSerialize");
    clearLayerCache("doc-del");

    const engine = new DocumentEngine("doc-del", "Delete Layer", 100, 100);
    const l1 = engine.addLayer("Keep", 100, 100);
    engine.setLayerImageBitmap(l1.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));
    const l2 = engine.addLayer("Remove", 100, 100);
    engine.setLayerImageBitmap(l2.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));

    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => ({ drawImage: vi.fn() });
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob([PNG_BYTES], { type: "image/png" }));
    }));

    // First save populates cache
    await serializeAndSaveProject(engine, "/path/del1.ptz");

    // Remove l2 and clear dirty
    engine.deleteLayer(l2.id);
    engine.clearDirty();

    // Second save: l1 from cache, l2 should not appear
    await serializeAndSaveProject(engine, "/path/del2.ptz");
    expect(Object.keys(capturedProject!.layers)).toEqual([l1.id]);
  });

  it("LRU evicts least-recently-used layers when byte budget is exceeded", async () => {
    const { serializeAndSaveProject, clearLayerCache, setLayerCacheBudget } = await import("../projectSerialize");
    clearLayerCache(); // clear all (also resets byte accounting)
    setLayerCacheBudget(30); // tiny budget: two 12-byte PNG entries fit, a third forces eviction
    try {
      const engine = new DocumentEngine("doc-lru", "LRU", 100, 100);
      const l1 = engine.addLayer("A", 100, 100);
      engine.setLayerImageBitmap(l1.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));
      const l2 = engine.addLayer("B", 100, 100);
      engine.setLayerImageBitmap(l2.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));
      const l3 = engine.addLayer("C", 100, 100);
      engine.setLayerImageBitmap(l3.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));

      let offscreenCount = 0;
      vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
        offscreenCount++;
        this.width = w;
        this.height = h;
        this.getContext = () => ({ drawImage: vi.fn() });
        this.convertToBlob = vi.fn().mockResolvedValue(new Blob([PNG_BYTES], { type: "image/png" }));
      }));

      // First save: all three encoded; A is evicted (oldest) to stay in budget
      await serializeAndSaveProject(engine, "/path/lru1.ptz");
      expect(offscreenCount).toBe(3);

      engine.clearDirty();

      // Second save: B and C served from cache; A (evicted) must re-encode
      offscreenCount = 0;
      await serializeAndSaveProject(engine, "/path/lru2.ptz");
      expect(offscreenCount).toBe(1); // only A re-encoded
    } finally {
      setLayerCacheBudget(256 * 1024 * 1024); // restore production budget
      clearLayerCache();
    }
  });

  it("per-document cache isolation: two engines don't share cache", async () => {
    const { serializeAndSaveProject, clearLayerCache } = await import("../projectSerialize");
    clearLayerCache(); // clear all

    const engineA = new DocumentEngine("doc-A", "Doc A", 50, 50);
    const la = engineA.addLayer("A", 50, 50);
    engineA.setLayerImageBitmap(la.id, makeBitmap(50, 50, new Uint8ClampedArray(50 * 50 * 4)));

    const engineB = new DocumentEngine("doc-B", "Doc B", 100, 100);
    const lb = engineB.addLayer("B", 100, 100);
    engineB.setLayerImageBitmap(lb.id, makeBitmap(100, 100, new Uint8ClampedArray(100 * 100 * 4)));

    let callCount = 0;
    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      callCount++;
      this.width = w;
      this.height = h;
      this.getContext = () => ({ drawImage: vi.fn() });
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob([PNG_BYTES], { type: "image/png" }));
    }));

    // First save for both
    await serializeAndSaveProject(engineA, "/path/a1.ptz");
    engineA.clearDirty();

    await serializeAndSaveProject(engineB, "/path/b1.ptz");
    engineB.clearDirty();

    expect(callCount).toBe(2); // each engine encoded its layer

    // Second save for both — should use own cache
    callCount = 0;
    await serializeAndSaveProject(engineA, "/path/a2.ptz");
    // Only engineA should use cache, but engineB is separate
    await serializeAndSaveProject(engineB, "/path/b2.ptz");
    expect(callCount).toBe(0); // both from cache
  });
});
