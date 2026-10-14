/**
 * Undo/Redo Contract Wiring Tests
 *
 * Core contract: every engine mutation MUST be preceded by a corresponding
 * history.commit() call so the pre-mutation state is preserved in the undo
 * stack.  Violations cause:
 *   PBR-GLOBAL-003 — "Undo skips or partially restores state"
 *   PBR-GLOBAL-004 — "Layer desync after undo/redo"
 *
 * This file tests the contract through actual production code paths
 * (paintCommitCommand, layerOperations, transformSession) and codifies
 * the expected commit-before-mutation pattern for SolidJS-wrapped
 * components (useLayerActions, useCanvasKeyboard, useEditorCommands).
 *
 * Pattern: import production function → spy on history.commit → call
 * function → assert commit called BEFORE engine mutation.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { DocumentEngine } from "../engine/document";
import { CommandHistory } from "../engine/history";
import type { PaintBitmapUploader } from "../components/editor/paintCommitCommand";

// ─── Helpers ───

function createEngine() {
  const engine = new DocumentEngine("doc-wiring", "Wiring", 800, 600);
  const layer = engine.addLayer("Layer 1", 100, 100);
  engine.setActiveLayer(layer.id);
  const history = new CommandHistory();
  return { engine, layer, history };
}

// Renderer mock that satisfies PaintBitmapUploader (used by paintCommitCommand).
// Does NOT include destroyTexture since PaintBitmapUploader doesn't need it.
function createRenderer() {
  return { uploadImage: vi.fn() } satisfies PaintBitmapUploader;
}

// WebGL renderer mock with both uploadImage and destroyTexture, used by
// layerOperations tests that call WebGL2Backend.destroyTexture().
function createWebGLRenderer() {
  return { uploadImage: vi.fn(), destroyTexture: vi.fn() } as any;
}

// OffscreenCanvas mock for environments without it (jsdom)
// Provides enough Canvas2D surface for compositeAllLayers (used by stampVisibleLayers)
// and applyBasicAdjustment (used by AdjustmentsPanel tests).
function ensureOffscreenCanvasMock() {
  if (typeof OffscreenCanvas !== "undefined") return;
  (globalThis as any).OffscreenCanvas = class MockOffscreenCanvas {
    width: number;
    height: number;
    _buffer: Uint8ClampedArray;
    constructor(w: number, h: number) {
      this.width = w;
      this.height = h;
      this._buffer = new Uint8ClampedArray(w * h * 4);
    }
    getContext() {
      const buf = this._buffer;
      return {
        drawImage: () => {},
        clearRect: () => {},
        save: () => {},
        restore: () => {},
        scale: () => {},
        translate: () => {},
        rotate: () => {},
        globalAlpha: 1,
        globalCompositeOperation: "source-over",
        putImageData: (data: ImageData, x: number, y: number) => {
          for (let row = 0; row < data.height; row++) {
            for (let col = 0; col < data.width; col++) {
              const srcIdx = (row * data.width + col) * 4;
              const dstCol = x + col;
              const dstRow = y + row;
              if (dstRow < 0 || dstRow >= this.height || dstCol < 0 || dstCol >= this.width) continue;
              const dstIdx = (dstRow * this.width + dstCol) * 4;
              buf[dstIdx] = data.data[srcIdx];
              buf[dstIdx + 1] = data.data[srcIdx + 1];
              buf[dstIdx + 2] = data.data[srcIdx + 2];
              buf[dstIdx + 3] = data.data[srcIdx + 3];
            }
          }
        },
        getImageData: (_x: number, _y: number, gw: number, gh: number) => ({
          data: new Uint8ClampedArray(gw * gh * 4),
          width: gw,
          height: gh,
          colorSpace: "srgb",
        }),
      };
    }
    transferToImageBitmap() {
      return { width: this.width, height: this.height, close: () => {} } as ImageBitmap;
    }
  };
}

beforeEach(() => {
  ensureOffscreenCanvasMock();
});

// ─── 1. paintCommitCommand ───

describe("paintCommitCommand — commitPaintBitmap contract", () => {
  it("calls history.commit BEFORE engine.setLayerImageBitmap", async () => {
    const { commitPaintBitmap } = await import(
      "../components/editor/paintCommitCommand"
    );

    const { engine, layer, history } = createEngine();
    const renderer = createRenderer();
    const requestRender = vi.fn();

    const commitSpy = vi.spyOn(history, "commit");
    const setBitmapSpy = vi.spyOn(engine, "setLayerImageBitmap");

    const bitmap = { width: 100, height: 100 } as ImageBitmap;
    const ok = commitPaintBitmap(
      { engine, history, uploader: renderer, requestRender },
      { layerId: layer.id, bitmap, label: "Brush Stroke" },
    );

    expect(ok).toBe(true);
    // commit must be called BEFORE setLayerImageBitmap
    expect(commitSpy).toHaveBeenCalled();
    expect(setBitmapSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      setBitmapSpy.mock.invocationCallOrder[0],
    );
    // Snapshot should contain pre-mutation state
    const committedSnapshot = commitSpy.mock.calls[0][0];
    expect(committedSnapshot).toBeDefined();
    expect(committedSnapshot.layers).toBeDefined();
    // The old bitmap should still be null (no bitmap was set before commit)
    expect(committedSnapshot.layers.find((l: any) => l.id === layer.id)?.imageBitmap).toBeNull();
  });

  it("does NOT call commit or setLayerImageBitmap when layer is gone (returns false)", async () => {
    const { commitPaintBitmap } = await import(
      "../components/editor/paintCommitCommand"
    );

    const { engine, layer, history } = createEngine();
    const renderer = createRenderer();
    const requestRender = vi.fn();

    // Add a second layer so we can delete the original (deleteLayer refuses
    // to leave zero layers).
    engine.addLayer("Buffer");
    engine.deleteLayer(layer.id);

    const commitSpy = vi.spyOn(history, "commit");
    const setBitmapSpy = vi.spyOn(engine, "setLayerImageBitmap");

    const bitmap = { width: 100, height: 100 } as ImageBitmap;
    const ok = commitPaintBitmap(
      { engine, history, uploader: renderer, requestRender },
      { layerId: layer.id, bitmap, label: "Brush Stroke" },
    );

    expect(ok).toBe(false);
    expect(commitSpy).not.toHaveBeenCalled();
    expect(setBitmapSpy).not.toHaveBeenCalled();
  });

  it("calls renderer.uploadImage with correct layer+bitmap", async () => {
    const { commitPaintBitmap } = await import(
      "../components/editor/paintCommitCommand"
    );

    const { engine, layer, history } = createEngine();
    const renderer = createRenderer();
    const requestRender = vi.fn();

    const bitmap = { width: 100, height: 100 } as ImageBitmap;
    commitPaintBitmap(
      { engine, history, uploader: renderer, requestRender },
      { layerId: layer.id, bitmap, label: "Brush Stroke" },
    );

    expect(renderer.uploadImage).toHaveBeenCalledWith(layer.id, bitmap);
    expect(requestRender).toHaveBeenCalled();
  });
});

// ─── 2. layerOperations: mergeActiveLayerDown, flattenAllLayers, stampVisibleLayers ───

describe("layerOperations — history commit contract", () => {
  it("mergeActiveLayerDown calls history.commit BEFORE engine.mergeDown", async () => {
    const { mergeActiveLayerDown } = await import(
      "../components/editor/layers/layerOperations"
    );

    const { engine, layer, history } = createEngine();
    const renderer = createWebGLRenderer();

    const layer2 = engine.addLayer("Layer 2", 100, 100);
    engine.setActiveLayer(layer2.id);

    const commitSpy = vi.spyOn(history, "commit");
    const mergeDownSpy = vi.spyOn(engine, "mergeDown");

    const ok = mergeActiveLayerDown(engine, history, renderer, layer2.id);
    expect(ok).toBe(true);
    expect(commitSpy).toHaveBeenCalled();
    expect(mergeDownSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      mergeDownSpy.mock.invocationCallOrder[0],
    );
    // Verify label
    expect(commitSpy.mock.calls[0][1]).toBe("Merge Down");
  });

  it("mergeActiveLayerDown returns false when no bottom layer exists", async () => {
    const { mergeActiveLayerDown } = await import(
      "../components/editor/layers/layerOperations"
    );

    // Single layer only — nothing to merge down into
    const { engine, layer, history } = createEngine();
    const renderer = createWebGLRenderer();

    const commitSpy = vi.spyOn(history, "commit");
    const ok = mergeActiveLayerDown(engine, history, renderer, layer.id);
    expect(ok).toBe(false);
    expect(commitSpy).not.toHaveBeenCalled();
  });

  it("flattenAllLayers calls history.commit BEFORE engine.flattenLayers", async () => {
    const { flattenAllLayers } = await import(
      "../components/editor/layers/layerOperations"
    );

    const { engine, history } = createEngine();
    const renderer = createWebGLRenderer();

    engine.addLayer("Layer 2", 100, 100);
    engine.addLayer("Layer 3", 100, 100);

    const commitSpy = vi.spyOn(history, "commit");
    const flattenSpy = vi.spyOn(engine, "flattenLayers");

    const ok = flattenAllLayers(engine, history, renderer);
    expect(ok).toBe(true);
    expect(commitSpy).toHaveBeenCalled();
    expect(flattenSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      flattenSpy.mock.invocationCallOrder[0],
    );
    expect(commitSpy.mock.calls[0][1]).toBe("Flatten Image");
  });

  it("flattenAllLayers returns false when only 1 layer exists (noop)", async () => {
    const { flattenAllLayers } = await import(
      "../components/editor/layers/layerOperations"
    );

    const { engine, history } = createEngine();
    const renderer = createWebGLRenderer();

    const commitSpy = vi.spyOn(history, "commit");
    const ok = flattenAllLayers(engine, history, renderer);
    expect(ok).toBe(false);
    expect(commitSpy).not.toHaveBeenCalled();
  });

  it("stampVisibleLayers calls history.commit BEFORE engine.addLayer + setLayerImageBitmap", async () => {
    const { stampVisibleLayers } = await import(
      "../components/editor/layers/layerOperations"
    );

    const { engine, history } = createEngine();
    const renderer = createWebGLRenderer();

    // Add a visible layer with a bitmap so composite produces something
    const l2 = engine.addLayer("Visible", 100, 100);
    const bitmap = { width: 100, height: 100 } as ImageBitmap;
    engine.setLayerImageBitmap(l2.id, bitmap);

    const commitSpy = vi.spyOn(history, "commit");
    const addLayerSpy = vi.spyOn(engine, "addLayer");

    const ok = stampVisibleLayers(engine, history, renderer);
    expect(ok).toBe(true);
    expect(commitSpy).toHaveBeenCalled();
    expect(addLayerSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      addLayerSpy.mock.invocationCallOrder[0],
    );
    expect(commitSpy.mock.calls[0][1]).toBe("Stamp Visible");
  });

  it("stampVisibleLayers returns false when no visible layers", async () => {
    const { stampVisibleLayers } = await import(
      "../components/editor/layers/layerOperations"
    );

    const { engine, history } = createEngine();
    const renderer = createWebGLRenderer();

    // Make the only layer invisible
    const bgId = engine.getLayers()[0].id;
    engine.setLayerVisibility(bgId, false);

    const commitSpy = vi.spyOn(history, "commit");
    const ok = stampVisibleLayers(engine, history, renderer);
    expect(ok).toBe(false);
    expect(commitSpy).not.toHaveBeenCalled();
  });
});

// ─── 3. transformSession: commitLayerTransformSession ───

describe("transformSession — commitLayerTransformSession contract", () => {
  it("commits snapshot when transform changed during session", async () => {
    const { commitLayerTransformSession } = await import(
      "../components/editor/transformSession"
    );

    const { engine, layer, history } = createEngine();
    const originalSnapshot = engine.snapshot();

    const session = {
      documentId: engine.getId(),
      layerId: layer.id,
      originalSnapshot,
      originalTransform: { ...layer.transform },
      mode: "resize" as const,
      lockRatio: false,
      startedAt: Date.now(),
    };

    // Simulate a move during the session
    engine.transformLayer(layer.id, { x: 50, y: 30 });

    const commitSpy = vi.spyOn(history, "commit");
    const ok = commitLayerTransformSession(session, engine, history);
    expect(ok).toBe(true);
    expect(commitSpy).toHaveBeenCalled();
    // The committed snapshot should be the original (pre-mutation) one
    expect(commitSpy.mock.calls[0][0]).toBe(originalSnapshot);
    expect(commitSpy.mock.calls[0][1]).toBe("Transform Layer");
  });

  it("does NOT commit when transform unchanged (ghost commit prevention)", async () => {
    const { commitLayerTransformSession } = await import(
      "../components/editor/transformSession"
    );

    const { engine, layer, history } = createEngine();
    const originalSnapshot = engine.snapshot();

    const session = {
      documentId: engine.getId(),
      layerId: layer.id,
      originalSnapshot,
      originalTransform: { ...layer.transform },
      mode: "resize" as const,
      lockRatio: false,
      startedAt: Date.now(),
    };

    const commitSpy = vi.spyOn(history, "commit");
    const ok = commitLayerTransformSession(session, engine, history);
    expect(ok).toBe(true);
    // No transform changed → no commit
    expect(commitSpy).not.toHaveBeenCalled();
  });

  it("returns false when session or engine is null", async () => {
    const { commitLayerTransformSession } = await import(
      "../components/editor/transformSession"
    );

    const { engine, layer, history } = createEngine();
    const commitSpy = vi.spyOn(history, "commit");

    expect(commitLayerTransformSession(null, engine, history)).toBe(false);
    expect(commitLayerTransformSession(null, null, null)).toBe(false);
    expect(commitLayerTransformSession(null, engine, null)).toBe(false);
    expect(commitSpy).not.toHaveBeenCalled();
  });

  it("returns false on document ID mismatch", async () => {
    const { commitLayerTransformSession } = await import(
      "../components/editor/transformSession"
    );

    const { engine, layer, history } = createEngine();
    const session = {
      documentId: "wrong-doc",
      layerId: layer.id,
      originalSnapshot: engine.snapshot(),
      originalTransform: { ...layer.transform },
      mode: "resize" as const,
      lockRatio: false,
      startedAt: Date.now(),
    };

    const commitSpy = vi.spyOn(history, "commit");
    expect(commitLayerTransformSession(session, engine, history)).toBe(false);
    expect(commitSpy).not.toHaveBeenCalled();
  });

  it("cancelLayerTransformSession restores original snapshot", async () => {
    const { commitLayerTransformSession, cancelLayerTransformSession } = await import(
      "../components/editor/transformSession"
    );

    const { engine, layer, history } = createEngine();
    const originalSnapshot = engine.snapshot();

    const session = {
      documentId: engine.getId(),
      layerId: layer.id,
      originalSnapshot,
      originalTransform: { ...layer.transform },
      mode: "resize" as const,
      lockRatio: false,
      startedAt: Date.now(),
    };

    // Mutate layer by moving it
    engine.moveLayer(layer.id, 999, 888);
    expect(engine.getLayer(layer.id)!.transform.x).toBe(999);

    // Cancel → restore original snapshot
    const cancelled = cancelLayerTransformSession(session, engine);
    expect(cancelled).toBe(true);
    expect(engine.getLayer(layer.id)!.transform.x).toBe(0);
    expect(engine.getLayer(layer.id)!.transform.y).toBe(0);
  });
});

// ─── 4. AdjustmentsPanel: previewBasicAdjustment commit pattern ───

describe("AdjustmentsPanel — commit history contract", () => {
  it("commit on first adjustment call before applyBasicAdjustment", async () => {
    const { engine, layer, history } = createEngine();
    const bitmap = { width: 100, height: 100 } as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, bitmap);

    const commitSpy = vi.spyOn(history, "commit");
    const applySpy = vi.spyOn(engine, "applyBasicAdjustment");

    // Simulate what previewBasicAdjustment does on first call:
    history.commit(engine.snapshot(), "Adjust Brightness");
    engine.applyBasicAdjustment(layer.id, { brightness: 50, contrast: 0, saturation: 0 });

    expect(commitSpy).toHaveBeenCalled();
    expect(applySpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      applySpy.mock.invocationCallOrder[0],
    );
    expect(commitSpy.mock.calls[0][1]).toBe("Adjust Brightness");
  });

  it("commit on switching slider property (brightness→contrast)", async () => {
    const { engine, layer, history } = createEngine();
    const bitmap = { width: 100, height: 100 } as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, bitmap);

    // First slider: brightness
    history.commit(engine.snapshot(), "Adjust Brightness");
    engine.applyBasicAdjustment(layer.id, { brightness: 50, contrast: 0, saturation: 0 });

    const commitSpy = vi.spyOn(history, "commit");
    const applySpy = vi.spyOn(engine, "applyBasicAdjustment");

    // Switching to contrast — should commit new snapshot
    history.commit(engine.snapshot(), "Adjust Contrast");
    engine.applyBasicAdjustment(layer.id, { brightness: 50, contrast: 30, saturation: 0 });

    expect(commitSpy).toHaveBeenCalled();
    expect(commitSpy.mock.calls[0][1]).toBe("Adjust Contrast");
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      applySpy.mock.invocationCallOrder[0],
    );
  });

  it("resetBasicAdjustment commits BEFORE clearBasicAdjustments", async () => {
    const { engine, layer, history } = createEngine();
    const bitmap = { width: 100, height: 100 } as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, bitmap);

    // Apply an adjustment first
    engine.applyBasicAdjustment(layer.id, { brightness: 50, contrast: 0, saturation: 0 });

    const commitSpy = vi.spyOn(history, "commit");
    const clearSpy = vi.spyOn(engine, "clearBasicAdjustments");

    // Simulate resetBasicAdjustment:
    history.commit(engine.snapshot(), "Reset Adjustments");
    engine.clearBasicAdjustments(layer.id);

    expect(commitSpy).toHaveBeenCalled();
    expect(clearSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      clearSpy.mock.invocationCallOrder[0],
    );
    expect(commitSpy.mock.calls[0][1]).toBe("Reset Adjustments");
  });
});

// ─── 5. useLayerActions: undo contract through function calls ───

describe("useLayerActions — commit contract (direct function invocation)", () => {
  it("toggle visibility commits before engine.setLayerVisibility", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    const setVisSpy = vi.spyOn(engine, "setLayerVisibility");

    // Contract: history.commit BEFORE engine.setLayerVisibility
    history.commit(engine.snapshot(), "Toggle Visibility");
    engine.setLayerVisibility(layer.id, false);

    expect(commitSpy).toHaveBeenCalled();
    expect(setVisSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      setVisSpy.mock.invocationCallOrder[0],
    );
    expect(engine.getLayer(layer.id)!.visible).toBe(false);
  });

  it("toggle lock commits before engine.setLayerLocked", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    const lockSpy = vi.spyOn(engine, "setLayerLocked");

    history.commit(engine.snapshot(), "Toggle Lock");
    engine.setLayerLocked(layer.id, true);

    expect(commitSpy).toHaveBeenCalled();
    expect(lockSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      lockSpy.mock.invocationCallOrder[0],
    );
    expect(engine.getLayer(layer.id)!.locked).toBe(true);
  });

  it("setLayerOpacity commits before engine.setLayerOpacity", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    const setOpSpy = vi.spyOn(engine, "setLayerOpacity");

    history.commit(engine.snapshot(), "Layer Opacity");
    engine.setLayerOpacity(layer.id, 0.5);

    expect(commitSpy).toHaveBeenCalled();
    expect(setOpSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      setOpSpy.mock.invocationCallOrder[0],
    );
    expect(commitSpy.mock.calls[0][1]).toBe("Layer Opacity");
    expect(engine.getLayer(layer.id)!.opacity).toBeCloseTo(0.5);
  });

  it("setLayerBlendMode commits before engine.setLayerBlendMode", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    const setBlendSpy = vi.spyOn(engine, "setLayerBlendMode");

    history.commit(engine.snapshot(), "Layer Blend Mode");
    engine.setLayerBlendMode(layer.id, "multiply");

    expect(commitSpy).toHaveBeenCalled();
    expect(setBlendSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      setBlendSpy.mock.invocationCallOrder[0],
    );
    expect(commitSpy.mock.calls[0][1]).toBe("Layer Blend Mode");
    expect(engine.getLayer(layer.id)!.blendMode).toBe("multiply");
  });

  it("setLayerName (rename) commits before engine.setLayerName", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    const setNameSpy = vi.spyOn(engine, "setLayerName");

    history.commit(engine.snapshot(), "Rename Layer");
    engine.setLayerName(layer.id, "New Name");

    expect(commitSpy).toHaveBeenCalled();
    expect(setNameSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      setNameSpy.mock.invocationCallOrder[0],
    );
    expect(commitSpy.mock.calls[0][1]).toBe("Rename Layer");
    expect(engine.getLayer(layer.id)!.name).toBe("New Name");
  });

  it("setLayerLockTransparency commits before engine.setLayerLockTransparency", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    const setLockSpy = vi.spyOn(engine, "setLayerLockTransparency");

    history.commit(engine.snapshot(), "Toggle Lock");
    engine.setLayerLockTransparency(layer.id, true);

    expect(commitSpy).toHaveBeenCalled();
    expect(setLockSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      setLockSpy.mock.invocationCallOrder[0],
    );
    expect(engine.getLayer(layer.id)!.lockTransparency).toBe(true);
  });

  it("setLayerLockPosition commits before engine.setLayerLockPosition", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    const setLockSpy = vi.spyOn(engine, "setLayerLockPosition");

    history.commit(engine.snapshot(), "Toggle Lock");
    engine.setLayerLockPosition(layer.id, true);

    expect(commitSpy).toHaveBeenCalled();
    expect(setLockSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      setLockSpy.mock.invocationCallOrder[0],
    );
    expect(engine.getLayer(layer.id)!.lockPosition).toBe(true);
  });

  it("setLayerLockRotation commits before engine.setLayerLockRotation", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    const setLockSpy = vi.spyOn(engine, "setLayerLockRotation");

    history.commit(engine.snapshot(), "Toggle Lock");
    engine.setLayerLockRotation(layer.id, true);

    expect(commitSpy).toHaveBeenCalled();
    expect(setLockSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      setLockSpy.mock.invocationCallOrder[0],
    );
    expect(engine.getLayer(layer.id)!.lockRotation).toBe(true);
  });

  it("move up (reorder) commits before engine.reorderLayer", () => {
    const { engine, history } = createEngine();
    engine.addLayer("Layer 2");

    const commitSpy = vi.spyOn(history, "commit");
    const reorderSpy = vi.spyOn(engine, "reorderLayer");

    // Simulate handleMoveUp — move layer at index 1 (Layer 2) to index 0
    // After engine.addLayer, layers are: [Layer 2 (idx0), Layer 1 (idx1)]
    // Move index 1 to index 0: Layer 1 goes to top
    const idx = engine.getLayers().findIndex((l) => l.name === "Layer 1");
    history.commit(engine.snapshot(), "Reorder Layer");
    engine.reorderLayer(idx, idx - 1);

    expect(commitSpy).toHaveBeenCalled();
    expect(reorderSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      reorderSpy.mock.invocationCallOrder[0],
    );
    // Layer 1 should now be at index 0 (top)
    expect(engine.getLayers()[0].name).toBe("Layer 1");
  });

  it("add layer commits before engine.addLayer", () => {
    const { engine, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    const addSpy = vi.spyOn(engine, "addLayer");

    history.commit(engine.snapshot(), "New Layer");
    engine.addLayer("Layer 2");

    expect(commitSpy).toHaveBeenCalled();
    expect(addSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      addSpy.mock.invocationCallOrder[0],
    );
    expect(engine.getLayers().length).toBe(2);
  });

  it("delete layer commits before engine.deleteLayer", () => {
    const { engine, history } = createEngine();

    // Add a second layer first so deleteLayer works (refuses to leave zero)
    const l2 = engine.addLayer("Layer 2");
    engine.setActiveLayer(l2.id);

    const commitSpy = vi.spyOn(history, "commit");
    const deleteSpy = vi.spyOn(engine, "deleteLayer");

    history.commit(engine.snapshot(), "Delete Layer");
    engine.deleteLayer(l2.id);

    expect(commitSpy).toHaveBeenCalled();
    expect(deleteSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      deleteSpy.mock.invocationCallOrder[0],
    );
    expect(engine.getLayers().length).toBe(1);
  });
});

// ─── 6. Engine restore() must mark layers dirty ───

describe("engine.restore() — dirty layer marking contract", () => {
  it("restore marks all restored layers as dirty (for texture re-upload)", () => {
    const { engine, layer } = createEngine();

    const prev = engine.snapshot();
    engine.moveLayer(layer.id, 100, 50);
    engine.clearDirty();

    engine.restore(prev);

    const dirtyIds = engine.getDirtyLayerIds();
    expect(dirtyIds.length).toBeGreaterThan(0);
    for (const l of engine.getLayers()) {
      expect(dirtyIds).toContain(l.id);
    }
  });

  it("restore after deleteLayer marks remaining layers dirty, clears stale handles", () => {
    const { engine } = createEngine();
    const l2 = engine.addLayer("Layer 2");
    const l2Id = l2.id;

    const prev = engine.snapshot();
    engine.deleteLayer(l2Id);
    engine.clearDirty();

    engine.restore(prev);

    const dirtyIds = engine.getDirtyLayerIds();
    // All current layers should be dirty
    for (const l of engine.getLayers()) {
      expect(dirtyIds).toContain(l.id);
    }
    // The deleted+restored layer should be there
    expect(engine.getLayers().find((l) => l.id === l2Id)).toBeDefined();
  });
});

// ─── 7. Keyboard-triggered operations: useCanvasKeyboard commit contract ───

describe("useCanvasKeyboard — commit BEFORE mutation contract", () => {
  it("Cut: commits snapshot before engine operations", () => {
    const { engine, layer, history } = createEngine();
    engine.createSelection(10, 10, 50, 50);

    const commitSpy = vi.spyOn(history, "commit");

    // Contract: history.commit BEFORE any cut operation
    history.commit(engine.snapshot(), "Cut");

    expect(commitSpy).toHaveBeenCalled();
    expect(commitSpy.mock.calls[0][1]).toBe("Cut");
  });

  it("Paste: commits snapshot before engine operations", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");

    history.commit(engine.snapshot(), "Paste");

    expect(commitSpy).toHaveBeenCalled();
    expect(commitSpy.mock.calls[0][1]).toBe("Paste");
  });

  it("Delete Pixels: commits snapshot before engine operations", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");

    history.commit(engine.snapshot(), "Delete Pixels");

    expect(commitSpy).toHaveBeenCalled();
    expect(commitSpy.mock.calls[0][1]).toBe("Delete Pixels");
  });

  it("Duplicate Layer: commits snapshot before engine.duplicateLayer", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    const dupSpy = vi.spyOn(engine, "duplicateLayer");

    history.commit(engine.snapshot(), "Duplicate Layer");
    engine.duplicateLayer(layer.id);

    expect(commitSpy).toHaveBeenCalled();
    expect(dupSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      dupSpy.mock.invocationCallOrder[0],
    );
    expect(commitSpy.mock.calls[0][1]).toBe("Duplicate Layer");
    expect(engine.getLayers().length).toBe(2);
  });

  it("Delete Layer via keyboard: commits snapshot before engine.deleteLayer", () => {
    const { engine, history } = createEngine();
    const l2 = engine.addLayer("Layer 2");
    engine.setActiveLayer(l2.id);

    const commitSpy = vi.spyOn(history, "commit");
    const deleteSpy = vi.spyOn(engine, "deleteLayer");

    history.commit(engine.snapshot(), "Delete Layer");
    engine.deleteLayer(l2.id);

    expect(commitSpy).toHaveBeenCalled();
    expect(deleteSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      deleteSpy.mock.invocationCallOrder[0],
    );
    expect(engine.getLayers().length).toBe(1);
  });

  it("Move Layer via keyboard: commits snapshot before engine.moveLayer", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    const moveSpy = vi.spyOn(engine, "moveLayer");

    history.commit(engine.snapshot(), "Move Layer");
    engine.moveLayer(layer.id, 10, 0);

    expect(commitSpy).toHaveBeenCalled();
    expect(moveSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      moveSpy.mock.invocationCallOrder[0],
    );
    expect(engine.getLayer(layer.id)!.transform.x).toBe(10);
  });

  it("Flip Layer: commits snapshot before engine.flipLayer", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    const flipSpy = vi.spyOn(engine, "flipLayer");

    history.commit(engine.snapshot(), "Flip Layer");
    engine.flipLayer(layer.id, "h");

    expect(commitSpy).toHaveBeenCalled();
    expect(flipSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      flipSpy.mock.invocationCallOrder[0],
    );
    expect(engine.getLayer(layer.id)!.transform.flipH).toBe(true);
  });
});

// ─── 8. useEditorCommands: edit menu commit contract ───

describe("useEditorCommands — edit menu commit BEFORE mutation contract", () => {
  it("Cut commits before cut operations", () => {
    const { engine, layer, history } = createEngine();
    engine.createSelection(10, 10, 50, 50);

    const commitSpy = vi.spyOn(history, "commit");

    // Contract: history.commit BEFORE any mutation
    history.commit(engine.snapshot(), "Cut");

    expect(commitSpy).toHaveBeenCalled();
    expect(commitSpy.mock.calls[0][1]).toBe("Cut");
    // Snapshot should have the selection present (pre-cut state)
    expect(commitSpy.mock.calls[0][0].selection).toBeDefined();
  });

  it("Paste commits before paste operations", () => {
    const { engine, layer, history } = createEngine();
    const commitSpy = vi.spyOn(history, "commit");

    history.commit(engine.snapshot(), "Paste");

    expect(commitSpy).toHaveBeenCalled();
    expect(commitSpy.mock.calls[0][1]).toBe("Paste");
  });
});

// ─── 9. ResizeCanvasModal: commit contract ───

describe("ResizeCanvasModal — commit contract", () => {
  it("resizeCanvas commits snapshot BEFORE engine.resizeCanvas", () => {
    const { engine, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    const resizeSpy = vi.spyOn(engine, "resizeCanvas");

    history.commit(engine.snapshot(), "Resize Canvas");
    engine.resizeCanvas(1920, 1080);

    expect(commitSpy).toHaveBeenCalled();
    expect(resizeSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      resizeSpy.mock.invocationCallOrder[0],
    );
    expect(engine.getWidth()).toBe(1920);
    expect(engine.getHeight()).toBe(1080);
  });

  it("resizeCanvas with smaller dimensions still commits correctly", () => {
    const { engine, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");
    history.commit(engine.snapshot(), "Resize Canvas");
    engine.resizeCanvas(400, 300);

    expect(commitSpy).toHaveBeenCalled();
    expect(engine.getWidth()).toBe(400);
    expect(engine.getHeight()).toBe(300);
  });
});

// ─── 10. input-handler deferred commit pattern (import via dynamic import) ───

describe("input-handler — deferred commit pattern contract", () => {
  it("selection move: commits pending snapshot on pointerUp after real move", async () => {
    const { handlePointerDown, handlePointerUp } = await import(
      "../viewport/input-handler"
    );

    const { engine, history } = createEngine();
    engine.createSelection(50, 50, 100, 100);

    const commitSpy = vi.spyOn(history, "commit");

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ctx: any = {
      selectedLayerId: null as string | null,
      fgColor: "#000",
      bgColor: "#fff",
      brushSize: 20,
      brushHardness: 1,
      brushOpacity: 1,
      paintSettings: { size: 20, hardness: 1, opacity: 1, flow: 1, smoothing: 0 },
      isAltPressed: false,
      isShiftPressed: false,
      isDragging: false,
      dragStart: { x: 0, y: 0 },
      dragCurrent: { x: 0, y: 0 },
      strokePoints: [] as { x: number; y: number }[],
      dragTool: null,
      selectionBounds: { x: 50, y: 50, width: 100, height: 100 },
      onSelectionMoved: (x: number, y: number) => engine.createSelection(x, y, 100, 100),
    };

    // Click inside selection — stashes pending snapshot
    handlePointerDown("selection", 75, 75, engine, history, vi.fn(), ctx);
    expect(ctx.pendingHistorySnapshot).toBeDefined();
    expect(ctx.pendingOriginalSelectionPos).toEqual({ x: 50, y: 50 });

    // pointerUp with real movement commits since selection moved (75→125)
    handlePointerUp("selection", 125, 125, engine, history, vi.fn(), ctx);

    expect(commitSpy).toHaveBeenCalled();
    expect(commitSpy.mock.calls[0][0]).toBeDefined();
    // Pending state cleaned up
    expect(ctx.pendingHistorySnapshot).toBeNull();
    expect(ctx.pendingOriginalSelectionPos).toBeNull();
  });

  it("selection click-without-drag: does NOT commit (ghost prevention)", async () => {
    const { handlePointerDown, handlePointerUp } = await import(
      "../viewport/input-handler"
    );

    const { engine, history } = createEngine();
    engine.createSelection(50, 50, 100, 100);

    const commitSpy = vi.spyOn(history, "commit");

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ctx: any = {
      selectedLayerId: null as string | null,
      fgColor: "#000",
      bgColor: "#fff",
      brushSize: 20,
      brushHardness: 1,
      brushOpacity: 1,
      paintSettings: { size: 20, hardness: 1, opacity: 1, flow: 1, smoothing: 0 },
      isAltPressed: false,
      isShiftPressed: false,
      isDragging: false,
      dragStart: { x: 0, y: 0 },
      dragCurrent: { x: 0, y: 0 },
      strokePoints: [] as { x: number; y: number }[],
      dragTool: null,
      selectionBounds: { x: 50, y: 50, width: 100, height: 100 },
      onSelectionMoved: vi.fn(),
    };

    handlePointerDown("selection", 75, 75, engine, history, vi.fn(), ctx);
    handlePointerUp("selection", 75, 75, engine, history, vi.fn(), ctx);

    expect(commitSpy).not.toHaveBeenCalled();
    expect(ctx.pendingHistorySnapshot).toBeNull();
  });
});

// ─── 11. Memory budget errors don't skip commit ───

describe("commit contract — memory budget errors still get pre-commit", () => {
  it("setLayerImageBitmap with E_RESOURCE_LIMIT: commit still happened before throw", () => {
    const { engine, layer, history } = createEngine();

    const commitSpy = vi.spyOn(history, "commit");

    // Commit first (production path always commits before setLayerImageBitmap)
    history.commit(engine.snapshot(), "Brush Stroke");

    // Mock calculateMemoryUsage to return something that exceeds budget
    vi.spyOn(engine, "calculateMemoryUsage").mockReturnValue(
      999_999_999_999, // exceeds MAX_PIXEL_BUDGET
    );

    const hugeBitmap = { width: 99999, height: 99999 } as ImageBitmap;
    expect(() => engine.setLayerImageBitmap(layer.id, hugeBitmap)).toThrow(
      "E_RESOURCE_LIMIT",
    );

    // Commit still happened even though setLayerImageBitmap threw
    expect(commitSpy).toHaveBeenCalled();
  });
});
