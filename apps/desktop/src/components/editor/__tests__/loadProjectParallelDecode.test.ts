import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import type { OpenImageParams } from "../editorOpenImage";

// Parallel-decode contract for loadProjectFile (.ptz open path):
// - layer PNGs decode with bounded overlap instead of one by one
// - each decoded bitmap lands on its own layer even when decodes finish
//   out of order (a crossed bitmap corrupts the opened document)
// - uploads stay one FULL upload per layer, in stack order
// - one corrupt layer fails the whole open the way the old loop did:
//   nothing reaches the workspace, nothing is uploaded, decoded bitmaps freed
//
// NOTE: the IPC transport itself is still base64 (load_project has no binary
// variant); this file pins the decode/attach/upload contract only.

const hoisted = vi.hoisted(() => ({
  mockLoadProject: vi.fn<() => Promise<{ document_json: string; layers: Record<string, string> }>>(),
  mockShowToast: vi.fn(),
  mockAddRecentFile: vi.fn(),
  decodeActive: 0,
  decodeMax: 0,
  madeBitmaps: [] as Array<{ size: number; closed: boolean; close: () => void }>,
}));

vi.mock("@/tauri/native", () => ({
  showOpenImageDialog: vi.fn(),
  loadProject: hoisted.mockLoadProject,
  readFileBytes: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: () => true,
}));

vi.mock("@/lib/recentFiles", () => ({
  addRecentFile: hoisted.mockAddRecentFile,
}));

vi.mock("../Toast", () => ({
  showToast: hoisted.mockShowToast,
}));

const mockAddDocument = vi.fn();
const mockUploadImage = vi.fn();

function makeParams(): OpenImageParams {
  return {
    workspace: { isFull: () => false, addDocument: mockAddDocument } as unknown as OpenImageParams["workspace"],
    renderer: { uploadImage: mockUploadImage } as unknown as OpenImageParams["renderer"],
    scheduler: { requestRender: vi.fn() } as unknown as OpenImageParams["scheduler"],
    onError: vi.fn(),
    onLoading: vi.fn(),
  } as unknown as OpenImageParams;
}

function layerJson(id: string) {
  return {
    id,
    name: id,
    type: "raster",
    visible: true,
    opacity: 1,
    locked: false,
    blendMode: "normal",
    width: 4,
    height: 4,
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    imageBitmap: null,
  };
}

function projectJson(ids: string[]) {
  return JSON.stringify({
    id: "doc-1",
    name: "proj",
    width: 4,
    height: 4,
    version: 3,
    activeLayerId: ids[0] ?? null,
    selection: null,
    viewport: { x: 0, y: 0, zoom: 1 },
    dirty: false,
    layers: ids.map(layerJson),
  });
}

let originalCreateImageBitmap: typeof globalThis.createImageBitmap;

beforeEach(() => {
  originalCreateImageBitmap = globalThis.createImageBitmap;
  hoisted.decodeActive = 0;
  hoisted.decodeMax = 0;
  hoisted.madeBitmaps = [];
  vi.clearAllMocks();
  globalThis.createImageBitmap = (async (blob: Blob) => {
    hoisted.decodeActive += 1;
    hoisted.decodeMax = Math.max(hoisted.decodeMax, hoisted.decodeActive);
    try {
      // Big blobs decode slowly so completions land out of order on purpose.
      await new Promise((r) => setTimeout(r, blob.size > 50 ? 20 : 0));
      const entry = { size: blob.size, closed: false, close() { entry.closed = true; } };
      hoisted.madeBitmaps.push(entry);
      return entry as unknown as ImageBitmap;
    } finally {
      hoisted.decodeActive -= 1;
    }
  }) as typeof globalThis.createImageBitmap;
});

afterEach(() => {
  globalThis.createImageBitmap = originalCreateImageBitmap;
  vi.restoreAllMocks();
});

describe("loadProjectFile parallel decode", () => {
  it("decodes layers with overlap and lands each bitmap on its own layer", async () => {
    const ids = ["L1", "L2", "L3", "L4"];
    // L1 is the biggest blob so its decode finishes LAST on purpose.
    const sizes: Record<string, number> = { L1: 100, L2: 10, L3: 12, L4: 14 };
    const layers: Record<string, string> = {};
    for (const id of ids) layers[id] = btoa("x".repeat(sizes[id]));
    hoisted.mockLoadProject.mockResolvedValue({ document_json: projectJson(ids), layers });

    const { loadProjectFile } = await import("../editorOpenImage");
    await loadProjectFile("/path/p.ptz", makeParams(), "p.ptz");

    expect(hoisted.decodeMax).toBeGreaterThan(1);
    expect(hoisted.decodeMax).toBeLessThanOrEqual(4);
    expect(mockAddDocument).toHaveBeenCalledTimes(1);
    const engine = mockAddDocument.mock.calls[0][0].engine;
    const gotLayers = engine.getLayers();
    expect(gotLayers.map((l: { id: string }) => l.id)).toEqual(ids);
    for (const l of gotLayers) {
      expect((l.imageBitmap as unknown as { size: number }).size).toBe(sizes[l.id]);
    }
  });

  it("uploads one FULL texture per layer in stack order", async () => {
    const ids = ["L1", "L2", "L3"];
    const layers: Record<string, string> = {};
    for (const id of ids) layers[id] = btoa("x".repeat(100));
    hoisted.mockLoadProject.mockResolvedValue({ document_json: projectJson(ids), layers });

    const { loadProjectFile } = await import("../editorOpenImage");
    await loadProjectFile("/path/p.ptz", makeParams(), "p.ptz");

    expect(mockUploadImage).toHaveBeenCalledTimes(3);
    expect(mockUploadImage.mock.calls.map((c) => c[0])).toEqual(ids);
    for (const c of mockUploadImage.mock.calls) expect(c.length).toBe(2);
  });

  it("one corrupt layer fails the open with nothing committed and bitmaps freed", async () => {
    const ids = ["good1", "bad", "good2"];
    hoisted.mockLoadProject.mockResolvedValue({
      document_json: projectJson(ids),
      layers: { good1: btoa("x".repeat(10)), bad: "!!!not-base64!!!", good2: btoa("y".repeat(10)) },
    });

    const { loadProjectFile } = await import("../editorOpenImage");
    await expect(loadProjectFile("/path/p.ptz", makeParams(), "p.ptz")).rejects.toThrow();
    // Let in-flight decodes settle so the cleanup can be observed.
    await new Promise((r) => setTimeout(r, 40));
    expect(mockAddDocument).not.toHaveBeenCalled();
    expect(mockUploadImage).not.toHaveBeenCalled();
    expect(hoisted.madeBitmaps.length).toBeGreaterThan(0);
    for (const b of hoisted.madeBitmaps) expect(b.closed).toBe(true);
  });
});

describe("loadProjectFile layer size reconcile", () => {
  function sizedLayerJson(id: string, width: number, height: number) {
    return {
      ...layerJson(id),
      width,
      height,
    };
  }

  function sizedProjectJson(id: string, width: number, height: number) {
    return JSON.stringify({
      id: "doc-1",
      name: "proj",
      width,
      height,
      version: 3,
      activeLayerId: id,
      selection: null,
      viewport: { x: 0, y: 0, zoom: 1 },
      dirty: false,
      layers: [sizedLayerJson(id, width, height)],
    });
  }

  it("bitmap wins when stored dims diverge from decoded PNG bytes", async () => {
    globalThis.createImageBitmap = (async () => ({
      width: 60,
      height: 40,
      close() {},
    })) as unknown as typeof globalThis.createImageBitmap;
    hoisted.mockLoadProject.mockResolvedValue({
      document_json: sizedProjectJson("L1", 100, 100),
      layers: { L1: btoa("x".repeat(10)) },
    });

    const { loadProjectFile } = await import("../editorOpenImage");
    await loadProjectFile("/path/p.ptz", makeParams(), "p.ptz");

    const layer = mockAddDocument.mock.calls[0][0].engine.getLayers()[0];
    expect(layer.width).toBe(60);
    expect(layer.height).toBe(40);
  });

  it("matching sizes pass through untouched", async () => {
    globalThis.createImageBitmap = (async () => ({
      width: 60,
      height: 40,
      close() {},
    })) as unknown as typeof globalThis.createImageBitmap;
    hoisted.mockLoadProject.mockResolvedValue({
      document_json: sizedProjectJson("L1", 60, 40),
      layers: { L1: btoa("x".repeat(10)) },
    });

    const { loadProjectFile } = await import("../editorOpenImage");
    await loadProjectFile("/path/p.ptz", makeParams(), "p.ptz");

    const layer = mockAddDocument.mock.calls[0][0].engine.getLayers()[0];
    expect(layer.width).toBe(60);
    expect(layer.height).toBe(40);
    expect(layer.imageBitmap).toEqual({ width: 60, height: 40, close: expect.any(Function) });
  });

  it("missing bitmap leaves stored dims untouched", async () => {
    hoisted.mockLoadProject.mockResolvedValue({
      document_json: sizedProjectJson("L1", 100, 100),
      layers: {},
    });

    const { loadProjectFile } = await import("../editorOpenImage");
    await loadProjectFile("/path/p.ptz", makeParams(), "p.ptz");

    const layer = mockAddDocument.mock.calls[0][0].engine.getLayers()[0];
    expect(layer.width).toBe(100);
    expect(layer.height).toBe(100);
    expect(layer.imageBitmap).toBeNull();
  });
});
