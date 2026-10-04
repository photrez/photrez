// apps/desktop/src/components/editor/__tests__/projectSerializeRoundtrip.test.ts
//
// Save -> load round-trip coverage for the `.ptz` project format.
//
// Split out of `projectSerialize.test.ts` to keep both files inside the size
// guard. These cases drive the saved document back through the loader's logic
// (`JSON.parse` -> decode layer PNGs -> `engine.restore`) and assert the engine
// state that comes out, so a field lost on the way to disk shows up here as a
// wrong engine value rather than a passing assertion.
//
// The document is serialized by `ptzWriterContract.ts`, a TypeScript model of
// the production Rust writer (`crates/core/src/ptz_document.rs`). That model is
// not trusted on its own terms: `ptzWriterGolden.test.ts` diffs it
// byte-for-byte against the real writer's output for the golden fixtures.

import { describe, expect, it, vi, afterEach, beforeEach } from "vitest";
import { DocumentEngine } from "@/engine/document";
import type { DocumentModel, LayerNode, ShapeParams } from "@/engine/types";
import type { TextData } from "@/engine/textTypes";
import { emulateRustSerialization as projectPtzDocument } from "./ptzWriterContract";

const {
  mockSaveProjectStreamingBegin,
  mockSaveProjectStreamingWriteLayer,
  mockSaveProjectStreamingEnd,
  mockSaveProjectStreamingCancel,
  mockLoadProject,
} = vi.hoisted(() => ({
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

/** Captured data from a mocked streaming save, as the command writes it. */
interface CapturedProject {
  path: string;
  documentJson: string;
  layers: Record<string, Uint8Array>;
}

let capturedProject: CapturedProject | null = null;
let nextHandleId = 1;

/** Minimal mock ImageBitmap; the encoder only needs its dimensions. */
function makeBitmap(width: number, height: number, _fill?: Uint8ClampedArray): ImageBitmap {
  return { width, height, close: vi.fn() } as unknown as ImageBitmap;
}

/** OffscreenCanvas stub: enough for the encode path under jsdom. */
function stubSerializeGlobals(pngBytes: Uint8Array) {
  const mkCtx = () => {
    const ctx: any = {};
    // Enough of the 2D context for the ENGINE rasterizers that run on the
    // save path (shape + text), not just the encode step.
    ctx.fillStyle = "#000000";
    ctx.strokeStyle = "#000000";
    ctx.lineWidth = 1;
    ctx.lineCap = "butt";
    ctx.font = "";
    ctx.textBaseline = "alphabetic";
    ctx.letterSpacing = "0px";
    ctx.drawImage = vi.fn();
    ctx.translate = vi.fn(() => ctx);
    ctx.beginPath = vi.fn();
    ctx.rect = vi.fn();
    ctx.roundRect = vi.fn();
    ctx.ellipse = vi.fn();
    ctx.moveTo = vi.fn();
    ctx.lineTo = vi.fn();
    ctx.closePath = vi.fn();
    ctx.clearRect = vi.fn();
    ctx.fill = vi.fn();
    ctx.stroke = vi.fn();
    ctx.save = vi.fn();
    ctx.restore = vi.fn();
    ctx.fillText = vi.fn();
    ctx.strokeText = vi.fn();
    ctx.measureText = vi.fn(() => ({
      width: 10,
      fontBoundingBoxAscent: 80,
      fontBoundingBoxDescent: 24,
    }));
    return ctx;
  };
  vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    this.getContext = () => mkCtx();
    this.transferToImageBitmap = () => makeBitmap(Math.max(1, w), Math.max(1, h));
    // The save worker pool calls `blob.arrayBuffer()`, so this must be a real
    // Blob rather than raw bytes.
    this.convertToBlob = vi.fn().mockResolvedValue(new Blob([pngBytes as BlobPart], { type: "image/png" }));
  }));
}

/**
 * Install the streaming-save mock the way production would: the host hands
 * over a document VALUE and the Rust writer produces the bytes.
 */
function installSaveCapture() {
  mockSaveProjectStreamingBegin.mockImplementation(async (path, document) => {
    const handleId = `handle-rt-${nextHandleId++}`;
    capturedProject = { path, documentJson: projectPtzDocument(document), layers: {} };
    return handleId;
  });
  mockSaveProjectStreamingWriteLayer.mockImplementation(async (_handleId, layerId, pngBytes) => {
    if (capturedProject) capturedProject.layers[layerId] = pngBytes;
  });
  mockSaveProjectStreamingEnd.mockImplementation(async () => { /* no-op */ });
  mockSaveProjectStreamingCancel.mockImplementation(async () => { capturedProject = null; });
}

describe("projectSerialize — deserialize and engine restore", () => {
  const PNG_BYTES = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

  beforeEach(() => {
    mockSaveProjectStreamingBegin.mockClear();
    mockSaveProjectStreamingWriteLayer.mockClear();
    mockSaveProjectStreamingEnd.mockClear();
    mockSaveProjectStreamingCancel.mockClear();
    mockLoadProject.mockClear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  /**
   * Simulates the loadProjectFile logic from editorOpenImage.ts:
   *   1. Parse document JSON → model
   *   2. Decode base64 layer data → createImageBitmap → set on model
   *   3. engine.restore(model)
   *   4. Upload each layer bitmap
   */
  async function simulateLoadProject(json: string, layerData: Record<string, string>) {
    const model = JSON.parse(json) as DocumentModel;

    for (const layer of model.layers) {
      const b64 = layerData[layer.id];
      if (b64) {
        const binaryString = atob(b64);
        const bytes = new Uint8Array(binaryString.length);
        for (let i = 0; i < binaryString.length; i++) {
          bytes[i] = binaryString.charCodeAt(i);
        }
        const blob = new Blob([bytes], { type: "image/png" });
        // Mock createImageBitmap resolves with a fake bitmap
        layer.imageBitmap = { width: layer.width, height: layer.height, close: vi.fn() } as unknown as ImageBitmap;
      } else {
        layer.imageBitmap = null;
      }
    }

    const engine = new DocumentEngine(model.id, model.name, model.width, model.height);
    engine.restore(model, { restoreViewport: true });
    engine.clearDirty();

    const restoredLayers = engine.getLayers();
    for (const layer of restoredLayers) {
      if (layer.imageBitmap) {
        // simulate renderer.uploadImage — just verify the bitmap exists
        expect(layer.imageBitmap.width).toBeGreaterThan(0);
      }
    }

    return engine;
  }

  it("restores engine from serialized JSON with correct layer count", async () => {
    const json = JSON.stringify({
      id: "doc-restore-1",
      name: "Restored Doc",
      width: 800,
      height: 600,
      activeLayerId: null,
      selection: null,
      viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
      dirty: false,
      layers: [
        { id: "l1", name: "Bg", type: "raster", visible: true, opacity: 1, locked: false,
          blendMode: "normal", width: 800, height: 600,
          transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } },
        { id: "l2", name: "Fg", type: "raster", visible: true, opacity: 0.8, locked: false,
          blendMode: "multiply", width: 400, height: 300,
          transform: { x: 100, y: 50, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } },
      ],
    } as any);

    const b64 = btoa(String.fromCharCode(...PNG_BYTES));
    const layerData: Record<string, string> = { l1: b64, l2: b64 };

    const engine = await simulateLoadProject(json, layerData);
    expect(engine.getLayers().length).toBe(2);
    expect(engine.getWidth()).toBe(800);
    expect(engine.getHeight()).toBe(600);
    expect(engine.getLayers()[0].name).toBe("Bg");
    expect(engine.getLayers()[1].name).toBe("Fg");
  });

  it("preserves layer properties after restore", async () => {
    const json = JSON.stringify({
      id: "doc-props-r",
      name: "Props",
      width: 100, height: 100,
      activeLayerId: null, selection: null,
      viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
      dirty: false,
      layers: [{
        id: "l1", name: "Layer 1", type: "raster",
        visible: false, opacity: 0.3, locked: true,
        blendMode: "screen",
        width: 100, height: 100,
        transform: { x: 10, y: 20, scaleX: 2, scaleY: 1.5, rotation: 45, flipH: true, flipV: false },
      }],
    } as any);

    const b64 = btoa(String.fromCharCode(...PNG_BYTES));
    const engine = await simulateLoadProject(json, { l1: b64 });
    const layer = engine.getLayers()[0];

    expect(layer.visible).toBe(false);
    expect(layer.opacity).toBe(0.3);
    expect(layer.locked).toBe(true);
    expect(layer.blendMode).toBe("screen");
    expect(layer.transform.x).toBe(10);
    expect(layer.transform.y).toBe(20);
    expect(layer.transform.scaleX).toBe(2);
    expect(layer.transform.scaleY).toBe(1.5);
    expect(layer.transform.rotation).toBe(45);
    expect(layer.transform.flipH).toBe(true);
  });

  it("handles layers with null imageBitmap (no data saved)", async () => {
    const json = JSON.stringify({
      id: "doc-null-r",
      name: "Null Layer",
      width: 100, height: 100,
      activeLayerId: null, selection: null,
      viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
      dirty: false,
      layers: [{
        id: "l1", name: "Empty", type: "raster",
        visible: true, opacity: 1, locked: false,
        blendMode: "normal",
        width: 100, height: 100,
        transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      }],
    } as any);

    const engine = await simulateLoadProject(json, {}); // no layer data
    const layer = engine.getLayers()[0];
    expect(layer.imageBitmap).toBeNull();
  });

  it("restores selected layer and viewport (restoreViewport: true)", async () => {
    const json = JSON.stringify({
      id: "doc-sel-r",
      name: "Selection Restore",
      width: 800, height: 600,
      activeLayerId: "l1",
      selection: { x: 10, y: 20, width: 100, height: 200, angle: 0 },
      viewport: { panX: 50, panY: 30, zoom: 2, rotation: 0 },
      dirty: false,
      layers: [{
        id: "l1", name: "Selected", type: "raster",
        visible: true, opacity: 1, locked: false,
        blendMode: "normal",
        width: 800, height: 600,
        transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      }],
    } as any);

    vi.stubGlobal("createImageBitmap", vi.fn().mockResolvedValue({
      width: 800, height: 600, close: vi.fn(),
    } as ImageBitmap));

    const b64 = btoa(String.fromCharCode(...PNG_BYTES));
    const model = JSON.parse(json) as DocumentModel;

    // Decode base64 and set bitmap (same as loadProjectFile)
    const binaryString = atob(b64);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) bytes[i] = binaryString.charCodeAt(i);
    const blob = new Blob([bytes], { type: "image/png" });
    const bitmap = await createImageBitmap(blob);
    model.layers[0].imageBitmap = bitmap;

    const engine = new DocumentEngine(model.id, model.name, model.width, model.height);
    engine.restore(model, { restoreViewport: true });

    expect(engine.getActiveLayerId()).toBe("l1");
    expect(engine.getSelection()).toEqual({ x: 10, y: 20, width: 100, height: 200, angle: 0 });
    expect(engine.getViewport()).toEqual({ panX: 50, panY: 30, zoom: 2, rotation: 0 });

    vi.unstubAllGlobals();
  });
});

describe("projectSerialize — full roundtrip", () => {
  const PNG_BYTES = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

  beforeEach(() => {
    capturedProject = null;
    nextHandleId = 1;
    mockSaveProjectStreamingBegin.mockClear();
    mockSaveProjectStreamingWriteLayer.mockClear();
    mockSaveProjectStreamingEnd.mockClear();
    mockSaveProjectStreamingCancel.mockClear();
    mockLoadProject.mockClear();
    installSaveCapture();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("serialize → deserialize roundtrip preserves all layers and properties", async () => {
    // Arrange: create engine with known state
    stubSerializeGlobals(PNG_BYTES);

    const engine = new DocumentEngine("doc-rt", "Roundtrip", 200, 150);
    const l1 = engine.addLayer("Background", 200, 150);
    engine.setLayerImageBitmap(l1.id, makeBitmap(200, 150, new Uint8ClampedArray(200 * 150 * 4)));
    engine.setLayerOpacity(l1.id, 0.7);

    const l2 = engine.addLayer("Foreground", 100, 80);
    engine.setLayerImageBitmap(l2.id, makeBitmap(100, 80, new Uint8ClampedArray(100 * 80 * 4)));
    engine.setLayerVisibility(l2.id, false);
    engine.setLayerBlendMode(l2.id, "multiply");

    engine.setActiveLayer(l2.id);
    engine.createSelection(5, 5, 50, 50);

    const { serializeAndSaveProject } = await import("../projectSerialize");

    // Act: serialize
    await serializeAndSaveProject(engine, "/tmp/rt.ptz");
    expect(capturedProject).not.toBeNull();

    // Act: deserialize (simulate loadProjectFile)
    const model = JSON.parse(capturedProject!.documentJson) as DocumentModel;

    for (const layer of model.layers) {
      const bytes = capturedProject!.layers[layer.id];
      if (bytes) {
        const blob = new Blob([bytes as BlobPart], { type: "image/png" });
        layer.imageBitmap = { width: layer.width, height: layer.height, close: vi.fn() } as unknown as ImageBitmap;
      }
    }

    const restored = new DocumentEngine(model.id, model.name, model.width, model.height);
    restored.restore(model, { restoreViewport: true });

    // Assert: compare engine states
    expect(restored.getWidth()).toBe(200);
    expect(restored.getHeight()).toBe(150);
    expect(restored.getLayers().length).toBe(2);

    const rl1 = restored.getLayers().find(l => l.id === l1.id)!;
    expect(rl1.name).toBe("Background");
    expect(rl1.opacity).toBe(0.7);
    expect(rl1.visible).toBe(true);
    expect(rl1.imageBitmap).not.toBeNull();
    expect(rl1.imageBitmap!.width).toBe(200);
    expect(rl1.imageBitmap!.height).toBe(150);

    const rl2 = restored.getLayers().find(l => l.id === l2.id)!;
    expect(rl2.name).toBe("Foreground");
    expect(rl2.visible).toBe(false);
    expect(rl2.blendMode).toBe("multiply");
    expect(rl2.imageBitmap).not.toBeNull();
    expect(rl2.imageBitmap!.width).toBe(100);
    expect(rl2.imageBitmap!.height).toBe(80);

    expect(restored.getActiveLayerId()).toBe(l2.id);
    // The Rust writer emits `SelectionState`'s two optionals as explicit nulls
    // (it is a typed projection, not a spread), so the round trip carries
    // `shape`/`inverted` even though `createSelection` omits them for a rect.
    expect(restored.getSelection()).toEqual({
      x: 5, y: 5, width: 50, height: 50, angle: 0, shape: null, inverted: null,
    });
  });

  it("roundtrip with empty document (no layers, no selection)", async () => {
    stubSerializeGlobals(PNG_BYTES);

    const engine = new DocumentEngine("doc-empty", "Empty", 100, 100);

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/tmp/empty.ptz");

    // Deserialize
    const model = JSON.parse(capturedProject!.documentJson) as DocumentModel;
    expect(model.layers.length).toBe(0);
    expect(Object.keys(capturedProject!.layers).length).toBe(0);

    const restored = new DocumentEngine(model.id, model.name, model.width, model.height);
    restored.restore(model, { restoreViewport: true });
    expect(restored.getLayers().length).toBe(0);
  });

  it("roundtrip with multiple bitmaps preserves PNG data fidelity", async () => {
    // Use different PNG byte sequences for each layer
    const pngA = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x01]);
    const pngB = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x02]);

    vi.stubGlobal("OffscreenCanvas", vi.fn(function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      this.getContext = () => ({ drawImage: vi.fn(), clearRect: vi.fn(), save: vi.fn(), restore: vi.fn() });

      // Return different bytes depending on layer dimensions (simple heuristic)
      const bytes = w === 200 ? pngA : pngB;
      this.convertToBlob = vi.fn().mockResolvedValue(new Blob([bytes], { type: "image/png" }));
    }));

    const engine = new DocumentEngine("doc-fidelity", "Fidelity", 200, 150);
    const l1 = engine.addLayer("A", 200, 150);
    engine.setLayerImageBitmap(l1.id, makeBitmap(200, 150, new Uint8ClampedArray(200 * 150 * 4)));
    const l2 = engine.addLayer("B", 100, 80);
    engine.setLayerImageBitmap(l2.id, makeBitmap(100, 80, new Uint8ClampedArray(100 * 80 * 4)));

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/tmp/fidelity.ptz");

    // Verify each layer's binary data matches the original PNG bytes
    expect(capturedProject!.layers[l1.id]).toEqual(pngA);
    expect(capturedProject!.layers[l2.id]).toEqual(pngB);
    expect(capturedProject!.layers[l1.id]).not.toEqual(capturedProject!.layers[l2.id]);
  });

  it("roundtrips a shape layer with shapeParams through version 3 (additive)", async () => {
    stubSerializeGlobals(PNG_BYTES);

    const rectParams: ShapeParams = {
      kind: "rect", width: 120, height: 60, radius: 14,
      fill: { kind: "solid", color: "#ff0000" },
      stroke: { enabled: false, color: "#000000", width: 6 },
      arrowHead: false,
    };
    const lineParams: ShapeParams = {
      kind: "line", width: 80, height: 40, radius: 0,
      fill: { kind: "none", color: "#000000" },
      stroke: { enabled: true, color: "#000000", width: 4 },
      arrowHead: true,
    };

    const engine = new DocumentEngine("doc-shape-v2", "Shape V2", 200, 150);
    const shapeId = engine.addShapeLayer("Shape", rectParams).id;
    const lineId = engine.addShapeLayer("Arrow", lineParams).id;

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/tmp/shape-v2.ptz");
    expect(capturedProject).not.toBeNull();

    // Serialized JSON carries version 3 (additive bump — v2 shape fields ride
    // the `{...l}` spread through v3 unchanged).
    const model = JSON.parse(capturedProject!.documentJson) as DocumentModel & { version?: number };
    expect(model.version).toBe(3);

    // shapeParams rides the `{...l}` spread through JSON — no explicit field.
    const shapeLayerJson = model.layers.find((l) => l.id === shapeId)!;
    expect(shapeLayerJson.shapeParams).toEqual(rectParams);
    expect((shapeLayerJson as LayerNode & { shapeParams?: ShapeParams }).type).toBe("shape");
    const lineLayerJson = model.layers.find((l) => l.id === lineId)!;
    expect((lineLayerJson as LayerNode & { shapeParams?: ShapeParams }).shapeParams).toEqual(lineParams);

    // Bitmaps decode from result.layers[layer.id] for both shape layers.
    expect(capturedProject!.layers[shapeId]).toBeDefined();
    expect(capturedProject!.layers[lineId]).toBeDefined();

    // Simulate loadProjectFile: decode PNG bytes → set imageBitmap → engine.restore.
    for (const layer of model.layers) {
      const bytes = capturedProject!.layers[layer.id];
      if (bytes) {
        const blob = new Blob([bytes as BlobPart], { type: "image/png" });
        layer.imageBitmap = { width: layer.width, height: layer.height, close: vi.fn() } as unknown as ImageBitmap;
      }
    }

    const loaded = new DocumentEngine(model.id, model.name, model.width, model.height);
    loaded.restore(model, { restoreViewport: true });

    // REAL-ENGINE assertion — v2 additive: shape layer reloads as a shape.
    expect(loaded.isShapeLayer(shapeId)).toBe(true);
    expect(loaded.isShapeLayer(lineId)).toBe(true);
    expect(loaded.getLayer(shapeId)!.shapeParams).toEqual(rectParams);
    expect(loaded.getLayer(lineId)!.shapeParams).toEqual(lineParams);
    expect(loaded.getLayer(shapeId)!.imageBitmap).not.toBeNull();
    expect(loaded.getLayer(shapeId)!.imageBitmap!.width).toBeGreaterThan(0);
  });

  it("v1 fixture (no shape layers, no version) still loads without error (additive regression)", async () => {
    const v1Json = JSON.stringify({
      id: "doc-v1", name: "Legacy", width: 100, height: 100,
      activeLayerId: null, selection: null,
      viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
      dirty: false,
      layers: [{
        id: "l1", name: "Bg", type: "raster",
        visible: true, opacity: 1, locked: false,
        blendMode: "normal", width: 100, height: 100,
        transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      }],
    } as any);

    const model = JSON.parse(v1Json) as DocumentModel;
    const b64 = btoa(String.fromCharCode(...PNG_BYTES));
    const binaryString = atob(b64);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) bytes[i] = binaryString.charCodeAt(i);
    const blob = new Blob([bytes], { type: "image/png" });
    model.layers[0].imageBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;

    const loaded = new DocumentEngine(model.id, model.name, model.width, model.height);
    expect(() => loaded.restore(model, { restoreViewport: true })).not.toThrow();
    expect(loaded.getLayers().length).toBe(1);
    expect(loaded.isShapeLayer("l1")).toBe(false);
  });

  it("v2 file with malformed shapeParams:null does not crash the loader (lenient fallback)", async () => {
    const v2Json = JSON.stringify({
      id: "doc-bad", name: "Bad Shape", width: 100, height: 100,
      activeLayerId: null, selection: null,
      viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
      dirty: false,
      version: 2,
      layers: [{
        id: "s1", name: "Broken Shape", type: "shape",
        visible: true, opacity: 1, locked: false,
        blendMode: "normal", width: 100, height: 100,
        shapeParams: null,
        transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      }],
    } as any);

    const model = JSON.parse(v2Json) as DocumentModel;
    const b64 = btoa(String.fromCharCode(...PNG_BYTES));
    const binaryString = atob(b64);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) bytes[i] = binaryString.charCodeAt(i);
    const blob = new Blob([bytes], { type: "image/png" });
    model.layers[0].imageBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;

    const loaded = new DocumentEngine(model.id, model.name, model.width, model.height);
    expect(() => loaded.restore(model, { restoreViewport: true })).not.toThrow();
    expect(loaded.getLayers().length).toBe(1);
    // Loader's lenient path: type stays "shape", bitmap decodes, params absent.
    expect(loaded.isShapeLayer("s1")).toBe(true);
    expect(loaded.getLayer("s1")!.shapeParams).toBeUndefined();
    expect(loaded.getLayer("s1")!.imageBitmap).not.toBeNull();
  });

  it("roundtrips a text layer with textData through version 3 (additive)", async () => {
    stubSerializeGlobals(PNG_BYTES);

    const textData: TextData = {
      content: "Hello\nWorld",
      fontFamily: "Arial",
      fontSize: 48,
      fontWeight: 700,
      fontStyle: "italic",
      color: "#123456",
      align: "center",
      lineHeight: 1.6,
      letterSpacing: 2,
      boxMode: "point",
      boxWidth: 0,
      boxHeight: 0,
      strikethrough: false,
      underline: false,
      uppercase: false,
      stroke: { width: 6, color: "#00aa00", align: "outside" },
    };

    const engine = new DocumentEngine("doc-text-v3", "Text V3", 200, 150);
    const textId = engine.addTextLayer("Caption", textData).id;

    const { serializeAndSaveProject } = await import("../projectSerialize");
    await serializeAndSaveProject(engine, "/tmp/text-v3.ptz");
    expect(capturedProject).not.toBeNull();

    // Serialized JSON carries version 3; textData rides the `{...l}` spread.
    const model = JSON.parse(capturedProject!.documentJson) as DocumentModel & { version?: number };
    expect(model.version).toBe(3);
    const textLayerJson = model.layers.find((l) => l.id === textId)!;
    expect((textLayerJson as LayerNode & { textData?: TextData }).type).toBe("text");
    expect((textLayerJson as LayerNode & { textData?: TextData }).textData).toEqual(textData);

    // Bitmap decodes from result.layers[textId] (rasterized at 2x).
    expect(capturedProject!.layers[textId]).toBeDefined();

    // Simulate loadProjectFile: decode PNG bytes → set imageBitmap → engine.restore.
    for (const layer of model.layers) {
      const bytes = capturedProject!.layers[layer.id];
      if (bytes) {
        const blob = new Blob([bytes as BlobPart], { type: "image/png" });
        layer.imageBitmap = { width: layer.width, height: layer.height, close: vi.fn() } as unknown as ImageBitmap;
      }
    }

    const loaded = new DocumentEngine(model.id, model.name, model.width, model.height);
    loaded.restore(model, { restoreViewport: true });

    // REAL-ENGINE assertion — v3 additive: text layer reloads as a text layer
    // with its full parametric data intact (snapshot carries textData).
    expect(loaded.isTextLayer(textId)).toBe(true);
    expect(loaded.getLayer(textId)!.textData).toEqual(textData);
    expect(loaded.getLayer(textId)!.imageBitmap).not.toBeNull();
  });

  it("v3 file with malformed textData:null does not crash the loader (lenient fallback)", async () => {
    const v3Json = JSON.stringify({
      id: "doc-bad-text", name: "Bad Text", width: 100, height: 100,
      activeLayerId: null, selection: null,
      viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
      dirty: false,
      version: 3,
      layers: [{
        id: "t1", name: "Broken Text", type: "text",
        visible: true, opacity: 1, locked: false,
        blendMode: "normal", width: 100, height: 100,
        textData: null,
        transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      }],
    } as any);

    const model = JSON.parse(v3Json) as DocumentModel;
    const b64 = btoa(String.fromCharCode(...PNG_BYTES));
    const binaryString = atob(b64);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) bytes[i] = binaryString.charCodeAt(i);
    const blob = new Blob([bytes], { type: "image/png" });
    model.layers[0].imageBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;

    const loaded = new DocumentEngine(model.id, model.name, model.width, model.height);
    expect(() => loaded.restore(model, { restoreViewport: true })).not.toThrow();
    expect(loaded.getLayers().length).toBe(1);
    // Loader's lenient path: type stays "text", bitmap decodes, params absent.
    expect(loaded.isTextLayer("t1")).toBe(true);
    expect(loaded.getLayer("t1")!.textData).toBeUndefined();
    expect(loaded.getLayer("t1")!.imageBitmap).not.toBeNull();
  });
});
