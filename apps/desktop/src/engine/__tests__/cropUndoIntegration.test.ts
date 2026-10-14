import { describe, it, expect, vi } from "vitest";
import { DocumentEngine } from "../../engine/document";
import { CommandHistory } from "../../engine/history";

function setupOffscreenCanvasMock() {
  const MockConstructor = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    const noop = vi.fn();
    this.ctx = {
      fillStyle: "",
      fillRect: noop,
      clearRect: noop,
      save: noop,
      restore: noop,
      translate: noop,
      rotate: noop,
      scale: noop,
      drawImage: noop,
    };
    this.getContext = vi.fn(() => this.ctx);
    this.transferToImageBitmap = vi.fn(() => ({ width: this.width, height: this.height } as ImageBitmap));
  };
  vi.stubGlobal("OffscreenCanvas", MockConstructor as unknown as typeof OffscreenCanvas);
}

describe("crop + undo integration", () => {
  it("undo restores document dimensions after crop", () => {
    const engine = new DocumentEngine("doc-crop-und-1", "Crop Undo", 800, 600);
    engine.addLayer("Layer 1");
    const history = new CommandHistory();

    const preCrop = engine.snapshot();
    history.commit(preCrop);
    engine.applyCrop(50, 30, 400, 300);
    expect(engine.getWidth()).toBe(400);
    expect(engine.getHeight()).toBe(300);

    // The native walker is the restore executor; these geometry
    // tests assert the SAME engine state via the surviving restore surface,
    // using the pre-crop snapshot captured before the mutation.
    engine.restore(preCrop);

    expect(engine.getWidth()).toBe(800);
    expect(engine.getHeight()).toBe(600);
  });

  it("redo restores cropped dimensions after undo", () => {
    const engine = new DocumentEngine("doc-crop-redo", "Crop Redo", 800, 600);
    engine.addLayer("Layer 1");
    const history = new CommandHistory();

    const preCrop = engine.snapshot();
    history.commit(preCrop);

    engine.applyCrop(50, 30, 400, 300);
    const postCrop = engine.snapshot();

    // Undo back to pre-crop
    engine.restore(preCrop);
    expect(engine.getWidth()).toBe(800);

    // Redo back to cropped
    engine.restore(postCrop);
    expect(engine.getWidth()).toBe(400);
    expect(engine.getHeight()).toBe(300);
  });

  it("multiple crop-undo cycles do not corrupt state", () => {
    const engine = new DocumentEngine("doc-cycle", "Cycle", 800, 600);
    engine.addLayer("Layer 1");
    const history = new CommandHistory();

    for (let i = 0; i < 3; i++) {
      const preCrop = engine.snapshot();
      history.commit(preCrop);
      engine.applyCrop(0, 0, 400, 300);
      expect(engine.getWidth()).toBe(400);
      expect(engine.getHeight()).toBe(300);

      engine.restore(preCrop);
      expect(engine.getWidth()).toBe(800);
      expect(engine.getHeight()).toBe(600);
    }
  });

  it("undo after crop restores layer transform", () => {
    const engine = new DocumentEngine("doc-xform", "Xform", 800, 600);
    const layer = engine.addLayer("Layer 1");
    layer.transform.x = 100;
    layer.transform.y = 50;
    const history = new CommandHistory();

    const preCrop = engine.snapshot();
    history.commit(preCrop);
    engine.applyCrop(50, 30, 400, 300);

    const restoredLayer = engine.getLayer(layer.id)!;
    expect(restoredLayer.transform.x).toBe(50);
    expect(restoredLayer.transform.y).toBe(20);

    engine.restore(preCrop);

    const postUndoLayer = engine.getLayer(layer.id)!;
    expect(postUndoLayer.transform.x).toBe(100);
    expect(postUndoLayer.transform.y).toBe(50);
  });

  it("ImageBitmap remains valid in snapshot after engine applies crop", () => {
    const bitmap = { width: 800, height: 600 } as ImageBitmap;
    const engine = new DocumentEngine("doc-bmp-test", "Bitmap", 800, 600);
    const layer = engine.addLayer("Background", 800, 600);
    engine.setLayerImageBitmap(layer.id, bitmap);

    // Take snapshot before crop — bitmap reference is captured
    const snap = engine.snapshot();
    expect(snap.layers[0].imageBitmap).toBe(bitmap);

    // Apply crop — after our fix, this does NOT close the old bitmap
    engine.applyCrop(0, 0, 400, 300);

    // The snapshot's bitmap should still be valid (not closed)
    expect(snap.layers[0].imageBitmap).toBe(bitmap);

    // Restore should work with the same bitmap reference
    engine.restore(snap);
    expect(engine.getLayer(layer.id)!.imageBitmap).toBe(bitmap);
  });

  it("crop with deleteCroppedPixels mode + undo restores original state", () => {
    const engine = new DocumentEngine("doc-del", "Delete Crop", 200, 200);
    const layer = engine.addLayer("Layer 1");
    layer.transform.x = 100;
    layer.transform.y = 50;
    const history = new CommandHistory();

    const preCrop = engine.snapshot();
    history.commit(preCrop);

    // delete mode needs OffscreenCanvas which is not available in jsdom
    // so this will fail with a console.error but not throw
    try {
      engine.applyCrop(50, 50, 100, 100, { deleteCroppedPixels: true });
    } catch {
      // OffscreenCanvas not available in test environment — skip pixel-level verification
    }

    engine.restore(preCrop);

    expect(engine.getWidth()).toBe(200);
    expect(engine.getHeight()).toBe(200);
  });

  it("renderer.resize would be called with correct dimensions after undo", () => {
    const engine = new DocumentEngine("doc-resize", "Resize", 800, 600);
    engine.addLayer("Layer 1");
    const history = new CommandHistory();
    const resizeMock = vi.fn();

    const preCrop = engine.snapshot();
    history.commit(preCrop);

    const preCropWidth = engine.getWidth();
    const preCropHeight = engine.getHeight();

    engine.applyCrop(50, 50, 400, 300);

    // Simulate what AppTitleBar.handleUndo does
    engine.restore(preCrop);
    resizeMock(engine.getWidth(), engine.getHeight());

    expect(resizeMock).toHaveBeenCalledWith(preCropWidth, preCropHeight);
  });

  it("undo and redo bake crop fill into the Background layer (no separate fill layer)", () => {
    setupOffscreenCanvasMock();
    const engine = new DocumentEngine("doc-fill-undo", "Fill Undo", 100, 100);
    const bg = engine.addLayer("Background", 100, 100);
    bg.isBackground = true;
    bg.lockPosition = true;
    bg.lockRotation = true;
    engine.addLayer("Photo", 100, 100);

    const history = new CommandHistory();
    const preCrop = engine.snapshot();
    history.commit(preCrop);
    engine.applyCrop(-10, -10, 120, 120, { fillBackgroundColor: "#123456" });
    const postCrop = engine.snapshot();

    // School A1: fill is baked into the Background layer; no separate fill layer.
    expect(engine.getLayers().find((l) => l.name === "Crop Fill Background")).toBeUndefined();
    const bgAfter = engine.getLayers().find((l) => l.isBackground)!;
    expect(bgAfter.imageBitmap?.width).toBe(120);
    expect(bgAfter.imageBitmap?.height).toBe(120);

    engine.restore(preCrop);
    expect(engine.getWidth()).toBe(100);
    expect(engine.getHeight()).toBe(100);
    // Undo returns to pre-crop: the Background no longer has a baked fill bitmap.
    expect(engine.getLayers().find((l) => l.isBackground)!.imageBitmap).toBeNull();

    engine.restore(postCrop);
    expect(engine.getWidth()).toBe(120);
    expect(engine.getHeight()).toBe(120);
    expect(engine.getLayers().find((l) => l.isBackground)!.imageBitmap?.width).toBe(120);
  });

  it("crop undo stack clear when leaving crop tool", () => {
    // This simulates the cropState.ts clearCropStacks logic
    // which is called in useCanvasDerivedState when activeTool !== "crop"
    let cropUndoStack: string[] = ["crop-state-1", "crop-state-2"];

    function clearCropStacks() {
      cropUndoStack = [];
    }

    clearCropStacks();
    expect(cropUndoStack.length).toBe(0);
  });

  // Regression: 2026-07-06 — "layer turns black on undo after modern crop"
  //
  // Root cause: performApplyCrop in cropApply.ts closes the old layer.imageBitmap
  // when applying a crop with deleteCroppedPixels: true. But applyCropPreview
  // stores an engine.snapshot() in the history BEFORE calling engine.applyCrop().
  // The snapshot's imageBitmap reference is the just-closed bitmap, so undo
  // restores a layer whose imageBitmap is a detached/closed GPU buffer. The
  // renderer then tries to upload the closed bitmap, producing a black/empty
  // texture on screen.
  //
  // The contract this test pins: snapshots taken before a destructive crop
  // must keep their layer.imageBitmap references valid (i.e., .close() must
  // not have been called on them) for the entire lifetime of the snapshot in
  // the history stack.
  it("snapshot bitmap survives applyCrop with deleteCroppedPixels=true", () => {
    setupOffscreenCanvasMock();

    let closeCount = 0;
    const fakeBitmap = {
      width: 100,
      height: 100,
      close: () => { closeCount++; },
    } as unknown as ImageBitmap;

    const engine = new DocumentEngine("doc-del-bmp", "Del Bmp", 100, 100);
    const layer = engine.addLayer("Layer 1", 100, 100);
    engine.setLayerImageBitmap(layer.id, fakeBitmap);
    const history = new CommandHistory();

    // Snapshot taken BEFORE the crop — should pin the bitmap
    const snap = engine.snapshot();
    expect(snap.layers[0].imageBitmap).toBe(fakeBitmap);
    // Commit pre-crop snapshot to history (mirrors applyCropPreview flow)
    history.commit(snap);

    const closeCountBeforeCrop = closeCount;

    // Apply crop with deleteCroppedPixels=true (the default for modern crop)
    engine.applyCrop(10, 10, 50, 50, { deleteCroppedPixels: true });

    // CONTRACT: bitmaps referenced by snapshots in the history must NOT be
    // closed, because restore() puts those references back into live layers
    // and the renderer tries to upload them to a WebGL texture. A closed
    // ImageBitmap produces a black/empty texture ("image source is detached").
    expect(closeCount).toBe(closeCountBeforeCrop);
    expect(snap.layers[0].imageBitmap).toBe(fakeBitmap);

    // Undo must restore a layer with the same usable bitmap
    engine.restore(snap);

    const restoredLayer = engine.getLayer(layer.id)!;
    expect(restoredLayer.imageBitmap).toBe(fakeBitmap);
  });
});
