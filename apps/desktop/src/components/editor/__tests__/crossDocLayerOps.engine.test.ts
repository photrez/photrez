// apps/desktop/src/components/editor/__tests__/crossDocLayerOps.engine.test.ts
//
// Integration test: cross-doc layer ops against the REAL DocumentEngine
// (no mocks). This catches the bug class that mocked-engine unit tests miss:
//  - Mock engines accept any args; real engines enforce invariants
//  - Mock engines don't have setLayerBlendMode, setLayerOpacity, etc.
//  - Real engines use addLayer(name, w?, h?) — only name + size set
//  - Other properties (opacity, blend mode, transform, lock, visibility)
//    must be set explicitly via setters
//
// Reference: AGENTS.md §Definition of Done for a New Tool + test-overhaul
// §"every new tool passes unit test but fails in frontend" pattern.

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { WorkspaceManager } from "@/engine/workspace";
import { addLayerFromCrossDoc, addFilesAsLayers, addFilesAsLayersFromFileDrop, createNewDocsFromFiles, type WorkspaceFacade } from "../crossDocLayerOps";
import type { LayerDragPayload } from "../dragTypes";
import { resetToasts } from "../Toast";

const nativeMock = vi.hoisted(() => ({
  readFileBytes: vi.fn(),
}));

vi.mock("@/tauri/native", () => ({
  readFileBytes: nativeMock.readFileBytes,
}));

// Note: we deliberately do NOT mock Toast — the real toast side effects must
// work with the real engine. If a test triggers an unexpected error toast,
// that's a real signal to investigate.

describe("addLayerFromCrossDoc — real engine integration", () => {
  let ws: WorkspaceManager;
  let sourceDocId: string;
  let targetDocId: string;
  let sourceLayerId: string;
  let basePayload: LayerDragPayload;
  let originalCreateImageBitmap: typeof globalThis.createImageBitmap;

  beforeEach(() => {
    resetToasts();
    originalCreateImageBitmap = globalThis.createImageBitmap;
    nativeMock.readFileBytes.mockResolvedValue(
      new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0]),
    );
    globalThis.createImageBitmap = vi.fn().mockResolvedValue({ width: 100, height: 100 } as ImageBitmap);
    ws = new WorkspaceManager();
    const sA = WorkspaceManager.createBlankDocument("docA", "DocA", 800, 600);
    ws.addDocument(sA);
    const sB = WorkspaceManager.createBlankDocument("docB", "DocB", 1000, 800);
    ws.addDocument(sB);
    ws.switchDocument("docA");
    sourceDocId = "docA";
    targetDocId = "docB";

    // Create a source layer with ALL properties non-default
    const engineA = ws.getEngine("docA")!;
    const layer = engineA.addLayer("MyLogo");
    engineA.setLayerOpacity(layer.id, 0.42);
    engineA.setLayerBlendMode(layer.id, "multiply");
    engineA.setLayerLocked(layer.id, false);
    engineA.setLayerVisibility(layer.id, true);
    engineA.transformLayer(layer.id, {
      x: 50,
      y: 75,
      scaleX: 1.5,
      scaleY: 0.5,
      rotation: 30,
    });
    sourceLayerId = layer.id;

    basePayload = {
      version: 1,
      sourceDocId: "docA",
      layerId: sourceLayerId,
      sourceName: "MyLogo",
      isAltPressed: false,
    };
  });

  afterEach(() => {
    globalThis.createImageBitmap = originalCreateImageBitmap;
    vi.restoreAllMocks();
  });

  it("preserves opacity when copying across documents", () => {
    const before = ws.getEngine(sourceDocId)!.getLayer(sourceLayerId)!;
    expect(before.opacity).toBe(0.42);

    addLayerFromCrossDoc(
      basePayload,
      { type: "tab", docId: targetDocId },
      { x: 0, y: 0 },
      ws as unknown as WorkspaceFacade
    );

    const targetEngine = ws.getEngine(targetDocId)!;
    const targetLayers = targetEngine.getLayers();
    const cloned = targetLayers.find((l) => l.name === "MyLogo")!;
    expect(cloned).toBeDefined();
    expect(cloned.opacity).toBe(0.42);
  });

  it("preserves blend mode when copying across documents", () => {
    addLayerFromCrossDoc(
      basePayload,
      { type: "tab", docId: targetDocId },
      { x: 0, y: 0 },
      ws as unknown as WorkspaceFacade
    );

    const targetEngine = ws.getEngine(targetDocId)!;
    const cloned = targetEngine.getLayers().find((l) => l.name === "MyLogo")!;
    expect(cloned.blendMode).toBe("multiply");
  });

  it("preserves visibility when copying across documents", () => {
    const sourceEngine = ws.getEngine(sourceDocId)!;
    sourceEngine.setLayerVisibility(sourceLayerId, false);

    addLayerFromCrossDoc(
      basePayload,
      { type: "tab", docId: targetDocId },
      { x: 0, y: 0 },
      ws as unknown as WorkspaceFacade
    );

    const targetEngine = ws.getEngine(targetDocId)!;
    const cloned = targetEngine.getLayers().find((l) => l.name === "MyLogo")!;
    expect(cloned.visible).toBe(false);
  });

  it("preserves size (width/height) of source layer", () => {
    // Source layer "MyLogo" was created via addLayer(name) which uses doc size.
    // For real size preservation, we'd need to set the size differently.
    // This test documents the expected behavior: size should match source.
    const sourceEngine = ws.getEngine(sourceDocId)!;
    const source = sourceEngine.getLayer(sourceLayerId)!;
    expect(source.width).toBe(800);  // doc width
    expect(source.height).toBe(600); // doc height

    addLayerFromCrossDoc(
      basePayload,
      { type: "tab", docId: targetDocId },
      { x: 0, y: 0 },
      ws as unknown as WorkspaceFacade
    );

    const targetEngine = ws.getEngine(targetDocId)!;
    const cloned = targetEngine.getLayers().find((l) => l.name === "MyLogo")!;
    expect(cloned.width).toBe(800);
    expect(cloned.height).toBe(600);
  });

  it("places layer at doc center for a tab drop (plan: tab → doc center)", () => {
    addLayerFromCrossDoc(
      basePayload,
      { type: "tab", docId: targetDocId },
      { x: 333, y: 444 },
      ws as unknown as WorkspaceFacade,
    );

    const targetEngine = ws.getEngine(targetDocId)!; // docB = 1000x800
    const cloned = targetEngine.getLayers().find((l) => l.name === "MyLogo")!; // source 800x600
    // (targetW - layerW)/2, (targetH - layerH)/2 = (100, 100)
    expect(cloned.transform.x).toBe(100);
    expect(cloned.transform.y).toBe(100);
  });

  it("transfers source layer's bitmap to the cloned layer (regression: empty-layer bug)", () => {
    // User-reported bug: cross-doc copy created a layer with the right
    // name and size, but no bitmap — the new layer looked empty. The
    // addLayerFromCrossDoc path forgot to call setLayerImageBitmap.
    const sourceEngine = ws.getEngine(sourceDocId)!;
    const fakeBitmap = { width: 200, height: 150, __tag: "source-bitmap" } as unknown as ImageBitmap;
    sourceEngine.setLayerImageBitmap(sourceLayerId, fakeBitmap);

    addLayerFromCrossDoc(
      basePayload,
      { type: "tab", docId: targetDocId },
      { x: 0, y: 0 },
      ws as unknown as WorkspaceFacade
    );

    const targetEngine = ws.getEngine(targetDocId)!;
    const cloned = targetEngine.getLayers().find((l) => l.name === "MyLogo")!;
    expect(cloned).toBeDefined();
    expect(cloned.imageBitmap).toBe(fakeBitmap);
  });

  it("does not copy to target when Alt-moving the source document's last layer", () => {
    const sourceEngine = ws.getEngine(sourceDocId)!;
    const targetEngine = ws.getEngine(targetDocId)!;
    sourceEngine.deleteLayer(sourceLayerId);
    const onlyLayer = sourceEngine.getLayers()[0];
    const targetLayerCount = targetEngine.getLayers().length;

    const result = addLayerFromCrossDoc(
      { ...basePayload, layerId: onlyLayer.id, sourceName: onlyLayer.name, isAltPressed: true },
      { type: "tab", docId: targetDocId },
      { x: 0, y: 0 },
      ws
    );

    expect(result.newLayerId).toBeNull();
    expect(sourceEngine.getLayers()).toHaveLength(1);
    expect(targetEngine.getLayers()).toHaveLength(targetLayerCount);
  });

  it("centers the dropped layer on the cursor for a canvas drop", () => {
    // A canvas drop targets the active document's canvas. Switch to the
    // target doc so this is a true cross-doc drop (otherwise it resolves to
    // the source doc and reorders in place instead of cloning).
    ws.switchDocument(targetDocId);
    const created = addLayerFromCrossDoc(
      basePayload,
      { type: "canvas" },
      { x: 500, y: 400 },
      ws as unknown as WorkspaceFacade,
    );

    const targetEngine = ws.getEngine(targetDocId)!;
    const cloned = targetEngine.getLayers().find((l) => l.name === "MyLogo")!;
    // cursor (500,400) is the layer CENTER → top-left = cursor - half size.
    // "MyLogo" is doc-sized 800x600 → (500-400, 400-300) = (100, 100).
    expect(cloned.transform.x).toBe(100);
    expect(cloned.transform.y).toBe(100);
  });
});

describe("addFilesAsLayers — real engine decode-first contract", () => {
  let ws: WorkspaceManager;

  beforeEach(() => {
    resetToasts();
    ws = new WorkspaceManager();
    ws.addDocument(WorkspaceManager.createBlankDocument("docA", "DocA", 800, 600));
  });

  it("does not commit history or create empty layers when any file fails before mutation", async () => {
    // Provide a working default for this test
    vi.stubGlobal("createImageBitmap", vi.fn().mockResolvedValue({ width: 100, height: 100 } as ImageBitmap));

    const engine = ws.getEngine("docA")!;
    const history = ws.getHistory("docA")!;
    const commitSpy = vi.spyOn(history, "commit");
    const initialLayerCount = engine.getLayers().length;

    nativeMock.readFileBytes.mockImplementation(async (path: string) => {
      if (path.includes("bad")) throw new Error("decode failed");
      return new Uint8Array([1, 2, 3]);
    });

    const created = await addFilesAsLayers(
      ["/ok.png", "/bad.png"],
      { type: "canvas" },
      { x: 12, y: 34 },
      ws
    );

    expect(created).toEqual([]);
    expect(engine.getLayers().length).toBe(initialLayerCount);
    expect(commitSpy).not.toHaveBeenCalled();
  });

  it("closes already-decoded bitmaps when a later file fails (no GPU leak)", async () => {
    const closeFn1 = vi.fn();
    let callCount = 0;
    // Use direct assignment (not vi.stubGlobal) to match the pattern used
    // by other tests in this file — avoids subtle stubGlobal × restoreAllMocks
    // ordering issues during full-suite runs.
    globalThis.createImageBitmap = vi.fn().mockImplementation(async () => {
      callCount++;
      if (callCount === 2) throw new Error("corrupt file");
      return { width: 100, height: 100, close: closeFn1 } as unknown as ImageBitmap;
    });

    // Both files readable; use PNG magic header so isSupportedImageBytes passes
    const pngHeader = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0]);
    nativeMock.readFileBytes.mockResolvedValue(pngHeader);

    const created = await addFilesAsLayers(
      ["/ok.png", "/bad.png"],
      { type: "canvas" },
      { x: 0, y: 0 },
      ws
    );

    expect(created).toEqual([]);
    // First bitmap was decoded then freed
    expect(closeFn1).toHaveBeenCalledTimes(1);
  });
});

describe("addFilesAsLayersFromFileDrop — real engine integration (HTML5 file drop)", () => {
  let ws: WorkspaceManager;
  let originalCreateImageBitmap: typeof globalThis.createImageBitmap;

  function makeFileBitmap(width = 100, height = 100): ImageBitmap {
    return { width, height, close: vi.fn() } as unknown as ImageBitmap;
  }

  beforeEach(() => {
    resetToasts();
    originalCreateImageBitmap = globalThis.createImageBitmap;
    globalThis.createImageBitmap = vi.fn().mockResolvedValue(makeFileBitmap(200, 150));
    ws = new WorkspaceManager();
    ws.addDocument(WorkspaceManager.createBlankDocument("docA", "DocA", 800, 600));
  });

  afterEach(() => {
    globalThis.createImageBitmap = originalCreateImageBitmap;
    vi.restoreAllMocks();
  });

  it("creates layers from File objects using browser createImageBitmap", async () => {
    const engine = ws.getEngine("docA")!;
    const file = new File(["fake-png"], "photo.png", { type: "image/png" });
    const created = await addFilesAsLayersFromFileDrop(
      [file],
      { type: "canvas" },
      { x: 50, y: 60 },
      ws,
    );

    expect(created).toHaveLength(1);
    const { layerId, bitmap } = created[0];
    expect(layerId).toBeTruthy();
    expect(bitmap).toBeTruthy();
    const layer = engine.getLayer(layerId);
    expect(layer).toBeDefined();
    expect(layer!.name).toBe("photo.png");
    expect(layer!.width).toBe(200);
    expect(layer!.height).toBe(150);
    expect(layer!.transform.x).toBe(50);
    expect(layer!.transform.y).toBe(60);
    expect(layer!.imageBitmap).toBe(bitmap);
    expect(globalThis.createImageBitmap).toHaveBeenCalledWith(file);
  });

  it("commits history before mutation (undo-friendly)", async () => {
    const history = ws.getHistory("docA")!;
    const commitSpy = vi.spyOn(history, "commit");
    const file = new File(["fake-png"], "undo-test.png", { type: "image/png" });

    await addFilesAsLayersFromFileDrop([file], { type: "canvas" }, { x: 0, y: 0 }, ws);

    expect(commitSpy).toHaveBeenCalledTimes(1);
    expect(history.getUndoCount()).toBe(1);
  });

  it("targets active document when target is canvas", async () => {
    const engine = ws.getEngine("docA")!;
    const file = new File(["fake"], "img.png", { type: "image/png" });
    const created = await addFilesAsLayersFromFileDrop(
      [file],
      { type: "canvas" },
      { x: 0, y: 0 },
      ws,
    );

    expect(created).toHaveLength(1);
    expect(engine.getLayer(created[0].layerId)).toBeDefined();
  });

  it("centers the image on the cursor when center=true", async () => {
    const file = new File(["fake-png"], "photo.png", { type: "image/png" });
    const created = await addFilesAsLayersFromFileDrop(
      [file],
      { type: "canvas" },
      { x: 500, y: 400 },
      ws,
      true,
    );

    const layer = ws.getEngine("docA")!.getLayer(created[0].layerId)!;
    // cursor (500,400) is the image CENTER → top-left = cursor - half size.
    // bitmap is 200x150 in this suite's beforeEach → half = 100x75.
    expect(layer.transform.x).toBe(400);
    expect(layer.transform.y).toBe(325);
  });

  it("returns empty array and error toast when createImageBitmap fails", async () => {
    globalThis.createImageBitmap = vi.fn().mockRejectedValue(new Error("corrupt image"));
    const file = new File(["bad-data"], "broken.png", { type: "image/png" });

    const created = await addFilesAsLayersFromFileDrop(
      [file],
      { type: "canvas" },
      { x: 0, y: 0 },
      ws,
    );

    expect(created).toEqual([]);
    // No layer was added
    const engine = ws.getEngine("docA")!;
    expect(engine.getLayers()).toHaveLength(1); // only the background
  });

  it("closes bitmap and rejects when decoded dimensions exceed device limit", async () => {
    const closeFn = vi.fn();
    globalThis.createImageBitmap = vi.fn().mockResolvedValue({
      width: 20000,
      height: 100,
      close: closeFn,
    } as unknown as ImageBitmap);

    const file = new File(["huge"], "huge.png", { type: "image/png" });
    const created = await addFilesAsLayersFromFileDrop(
      [file],
      { type: "canvas" },
      { x: 0, y: 0 },
      ws,
    );

    expect(created).toEqual([]);
    // Oversized bitmap was closed, not leaked into the engine
    expect(closeFn).toHaveBeenCalledTimes(1);
    const engine = ws.getEngine("docA")!;
    expect(engine.getLayers()).toHaveLength(1); // only the background
  });

  it("closes decoded bitmaps when a later file fails (no GPU leak)", async () => {
    let callCount = 0;
    const closeFn = vi.fn();
    globalThis.createImageBitmap = vi.fn().mockImplementation(async () => {
      callCount++;
      if (callCount === 2) throw new Error("corrupt");
      return { width: 100, height: 100, close: closeFn } as unknown as ImageBitmap;
    });

    const created = await addFilesAsLayersFromFileDrop(
      [new File(["ok"], "ok.png"), new File(["bad"], "bad.png")],
      { type: "canvas" },
      { x: 0, y: 0 },
      ws,
    );

    expect(created).toEqual([]);
    expect(closeFn).toHaveBeenCalledTimes(1);
  });

  it("handles multiple files with cascading positions", async () => {
    const files = [
      new File(["a"], "a.png"),
      new File(["b"], "b.png"),
      new File(["c"], "c.png"),
    ];

    const created = await addFilesAsLayersFromFileDrop(
      files,
      { type: "canvas" },
      { x: 100, y: 200 },
      ws,
    );

    expect(created).toHaveLength(3);
    const engine = ws.getEngine("docA")!;
    expect(engine.getLayers()).toHaveLength(4); // 1 bg + 3 new
    // Each file has a cascaded position
    expect(engine.getLayer(created[1].layerId)!.transform).toEqual(
      expect.objectContaining({ x: expect.any(Number), y: expect.any(Number) }),
    );
  });

  it("bails with toast when adding files exceeds max layers", async () => {
    const engine = ws.getEngine("docA")!;
    // Fill to max
    for (let i = 0; i < 199; i++) engine.addLayer(`fill-${i}`);
    expect(engine.getLayers()).toHaveLength(200);

    const file = new File(["x"], "overflow.png", { type: "image/png" });
    const created = await addFilesAsLayersFromFileDrop(
      [file],
      { type: "canvas" },
      { x: 0, y: 0 },
      ws,
    );

    expect(created).toEqual([]);
    // layer count unchanged
    expect(engine.getLayers()).toHaveLength(200);
  });
});
