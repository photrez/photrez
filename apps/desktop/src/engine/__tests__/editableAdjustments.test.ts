import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { DocumentEngine } from "../document";
import { duplicateLayerNode } from "../layerFactory";
import { performApplyCrop } from "../cropApply";
import { createSnapshot, restoreSnapshot } from "../snapshot";

function setupOffscreenCanvasMock() {
  const MockConstructor = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    this.getContext = vi.fn((type: string) => {
      if (type === "2d") {
        return {
          drawImage: vi.fn(),
          clearRect: vi.fn(),
          save: vi.fn(),
          restore: vi.fn(),
          translate: vi.fn(),
          rotate: vi.fn(),
          scale: vi.fn(),
          putImageData: vi.fn(),
          getImageData: vi.fn(() => ({
            data: new Uint8ClampedArray(w * h * 4),
          })),
        };
      }
      return null;
    });
    this.transferToImageBitmap = vi.fn(() => {
      return {
        width: this.width,
        height: this.height,
        close: vi.fn(),
      } as unknown as ImageBitmap;
    });
  };

  vi.stubGlobal("OffscreenCanvas", MockConstructor as unknown as typeof OffscreenCanvas);
}

describe("Editable Basic Adjustments", () => {
  beforeEach(() => {
    setupOffscreenCanvasMock();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("applyBasicAdjustment stores the adjustment param non-destructively (imageBitmap unchanged)", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);

    // Initial state: no adjustments
    expect(layer.basicAdjustment).toBeUndefined();
    expect(layer.hasAdjustments ?? false).toBe(false);

    // First adjustment
    const adj1 = { brightness: 10, contrast: 20, saturation: -30 };
    engine.applyBasicAdjustment(layer.id, adj1);

    expect(layer.basicAdjustment).toEqual(adj1);
    expect(layer.hasAdjustments).toBe(true);
    // Non-destructive: the base bitmap is untouched — the renderer applies the
    // adjustment in the fragment shader via u_adjustment.
    expect(layer.imageBitmap).toBe(initialBitmap);
    expect(layer.baseImageBitmap).toBeNull();
  });

  it("subsequent applyBasicAdjustment calls replace the stored param (absolute, non-destructive)", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);

    // First adjustment
    const adj1 = { brightness: 10, contrast: 20, saturation: -30 };
    engine.applyBasicAdjustment(layer.id, adj1);
    expect(layer.basicAdjustment).toEqual(adj1);
    expect(layer.imageBitmap).toBe(initialBitmap);

    // Second adjustment
    const adj2 = { brightness: -10, contrast: 0, saturation: 50 };
    engine.applyBasicAdjustment(layer.id, adj2);

    // Each call sets the absolute params; the same base bitmap is referenced.
    expect(layer.basicAdjustment).toEqual(adj2);
    expect(layer.imageBitmap).toBe(initialBitmap);
  });

  it("clearBasicAdjustments resets the stored param without touching the bitmap", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);

    // Apply adjustment
    const adj = { brightness: 10, contrast: 20, saturation: -30 };
    engine.applyBasicAdjustment(layer.id, adj);

    // Clear adjustment
    engine.clearBasicAdjustments(layer.id);

    expect(layer.imageBitmap).toBe(initialBitmap);
    expect(layer.basicAdjustment).toBeUndefined();
    expect(layer.hasAdjustments).toBe(false);
  });

  it("commitBasicAdjustment bakes the param into the bitmap and drops it (destructive)", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);

    engine.applyBasicAdjustment(layer.id, { brightness: 10, contrast: 0, saturation: 0 });

    // Commit (simulates slider release)
    engine.commitBasicAdjustment(layer.id);

    // Adjustment is now baked into a fresh bitmap; the param is gone so the
    // next paint shows raw colors.
    expect(layer.basicAdjustment).toBeUndefined();
    expect(layer.hasAdjustments).toBe(false);
    expect(layer.imageBitmap).not.toBe(initialBitmap); // baked into a new bitmap
    // The old bitmap must NOT be closed — an undo snapshot may still reference it.
    expect(initialBitmap.close).not.toHaveBeenCalled();
  });

  it("commitBasicAdjustment with a zero adjustment just drops the param (no wasted bake)", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);

    engine.applyBasicAdjustment(layer.id, { brightness: 0, contrast: 0, saturation: 0 });
    engine.commitBasicAdjustment(layer.id);

    expect(layer.basicAdjustment).toBeUndefined();
    // No-op: the original bitmap is kept (no new baked bitmap allocated).
    expect(layer.imageBitmap).toBe(initialBitmap);
  });

  it("setLayerImageBitmap keeps basicAdjustment as a non-destructive layer effect", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);

    // Apply adjustment
    const adj = { brightness: 10, contrast: 20, saturation: -30 };
    engine.applyBasicAdjustment(layer.id, adj);

    // Replace the bitmap (paint commit / fill)
    const newBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, newBitmap);

    expect(layer.imageBitmap).toBe(newBitmap);
    expect(layer.baseImageBitmap).toBeNull();
    // Adjustment persists — the shader re-applies it on top of the new bitmap.
    expect(layer.basicAdjustment).toEqual(adj);
  });

  it("duplicateLayerNode deep-copies basicAdjustment and clones baseImageBitmap", () => {
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    const baseBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    const layer = {
      id: "layer-1",
      name: "Original",
      type: "raster" as const,
      visible: true,
      opacity: 1.0,
      locked: false,
      blendMode: "normal" as const,
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      width: 100,
      height: 100,
      imageBitmap: initialBitmap,
      baseImageBitmap: baseBitmap,
      basicAdjustment: { brightness: 10, contrast: 20, saturation: -30 },
      hasAdjustments: true,
    };

    const duplicate = duplicateLayerNode(layer);

    expect(duplicate.name).toBe("Original copy");
    expect(duplicate.basicAdjustment).toEqual(layer.basicAdjustment);
    expect(duplicate.basicAdjustment).not.toBe(layer.basicAdjustment); // deep copy
    expect(duplicate.baseImageBitmap).not.toBeNull();
    expect(duplicate.baseImageBitmap).not.toBe(layer.baseImageBitmap); // cloned
  });

  it("performApplyCrop with deleteCropped=true bakes adjustments by clearing baseImageBitmap and basicAdjustment", () => {
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    const baseBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    const layers = [
      {
        id: "layer-1",
        name: "Original",
        type: "raster" as const,
        visible: true,
        opacity: 1.0,
        locked: false,
        blendMode: "normal" as const,
        transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
        width: 100,
        height: 100,
        imageBitmap: initialBitmap,
        baseImageBitmap: baseBitmap,
        basicAdjustment: { brightness: 10, contrast: 20, saturation: -30 },
        hasAdjustments: true,
      },
    ];

    performApplyCrop(layers, 10, 10, 80, 80, { deleteCroppedPixels: true });

    const croppedLayer = layers[0];
    expect(croppedLayer.baseImageBitmap).toBeNull();
    expect(croppedLayer.basicAdjustment).toBeUndefined();
    expect(croppedLayer.hasAdjustments).toBe(false);
  });

  it("createSnapshot and restoreSnapshot preserve basicAdjustment param and imageBitmap", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);

    // Apply adjustment
    const adj = { brightness: 10, contrast: 20, saturation: -30 };
    engine.applyBasicAdjustment(layer.id, adj);

    // Create snapshot
    const snapshot = createSnapshot(engine.snapshot());
    expect(snapshot.layers[0].basicAdjustment).toEqual(adj);
    expect(snapshot.layers[0].imageBitmap).toBe(initialBitmap);

    // Mutate original
    const newAdj = { brightness: 40, contrast: 50, saturation: 60 };
    engine.applyBasicAdjustment(layer.id, newAdj);

    // Restore snapshot
    const restored = restoreSnapshot(snapshot);
    expect(restored.layers[0].basicAdjustment).toEqual(adj);
    expect(restored.layers[0].imageBitmap).toBe(initialBitmap);
  });

  // --- Edge Case Tests ---

  it("Edge Case: applyBasicAdjustment works on locked layers (Background compatible)", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);
    engine.setLayerLocked(layer.id, true);

    const adj = { brightness: 10, contrast: 20, saturation: -30 };
    engine.applyBasicAdjustment(layer.id, adj);

    expect(layer.basicAdjustment).toEqual(adj);
    expect(layer.hasAdjustments).toBe(true);
    // Locked layers still benefit from non-destructive GPU adjustment.
    expect(layer.imageBitmap).toBe(initialBitmap);
  });

  it("Edge Case: applyBasicAdjustment immediately returns on empty layers (null bitmap)", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");

    expect(layer.imageBitmap).toBeNull();

    const adj = { brightness: 10, contrast: 20, saturation: -30 };
    engine.applyBasicAdjustment(layer.id, adj);

    expect(layer.baseImageBitmap).toBeUndefined();
    expect(layer.basicAdjustment).toBeUndefined();
  });

  it("Edge Case: adjusting values back to zero resets hasAdjustments but keeps imageBitmap intact", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);

    // Adjust to non-zero
    engine.applyBasicAdjustment(layer.id, { brightness: 10, contrast: 20, saturation: -30 });
    expect(layer.hasAdjustments).toBe(true);

    // Adjust back to all zeros
    engine.applyBasicAdjustment(layer.id, { brightness: 0, contrast: 0, saturation: 0 });
    expect(layer.hasAdjustments).toBe(false);
    expect(layer.basicAdjustment).toEqual({ brightness: 0, contrast: 0, saturation: 0 });
    expect(layer.imageBitmap).toBe(initialBitmap); // still in the editing session
  });

  it("Edge Case: non-destructive crop does NOT bake adjustments", () => {
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    const baseBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    const layers = [
      {
        id: "layer-1",
        name: "Original",
        type: "raster" as const,
        visible: true,
        opacity: 1.0,
        locked: false,
        blendMode: "normal" as const,
        transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
        width: 100,
        height: 100,
        imageBitmap: initialBitmap,
        baseImageBitmap: baseBitmap,
        basicAdjustment: { brightness: 10, contrast: 20, saturation: -30 },
        hasAdjustments: true,
      },
    ];

    performApplyCrop(layers, 10, 10, 80, 80, { deleteCroppedPixels: false });

    const croppedLayer = layers[0];
    expect(croppedLayer.baseImageBitmap).toBe(baseBitmap);
    expect(croppedLayer.basicAdjustment).toEqual({ brightness: 10, contrast: 20, saturation: -30 });
    expect(croppedLayer.hasAdjustments).toBe(true);
  });

  // --- Regression tests for audit bug fixes ---

  it("restore() does not close any bitmaps (snapshot safety — redo stack may reference them)", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);

    // Snapshot A — captures initialBitmap
    const snapA = engine.snapshot();

    // Apply adjustment → new imageBitmap, caches initialBitmap as baseImageBitmap
    engine.applyBasicAdjustment(layer.id, { brightness: 10, contrast: 0, saturation: 0 });
    const adjustedBitmap = layer.imageBitmap!;

    // Snapshot B — captures adjustedBitmap + baseImageBitmap
    const snapB = engine.snapshot();

    // Restore to snapA — must NOT close any bitmap because the redo stack
    // (snapB) still references adjustedBitmap.  Closing it would cause
    // "image source is detached" on subsequent drawImage calls.
    engine.restore(snapA);
    expect(adjustedBitmap.close).not.toHaveBeenCalled();
    expect(initialBitmap.close).not.toHaveBeenCalled();
    // Re-fetch layer — restore() replaces the model, old reference is stale
    const restored = engine.getLayer(layer.id)!;
    expect(restored.imageBitmap).toBe(initialBitmap);
  });

  it("setLayerImageBitmap preserves basicAdjustment (non-destructive layer effect, no reset on paint)", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);

    // Apply adjustment → stores param, does not touch the bitmap
    engine.applyBasicAdjustment(layer.id, { brightness: 10, contrast: 0, saturation: 0 });
    expect(layer.basicAdjustment).toEqual({ brightness: 10, contrast: 0, saturation: 0 });

    // Overwrite with a new bitmap (simulates paint commit)
    const paintBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, paintBitmap);

    // The previous bitmap must NOT be closed — undo snapshots may still reference it.
    expect(initialBitmap.close).not.toHaveBeenCalled();
    // Replacing the layer content must NOT drop the adjustment — it is a layer-
    // level effect re-applied by the shader on top of the new bitmap.
    expect(layer.imageBitmap).toBe(paintBitmap);
    expect(layer.basicAdjustment).toEqual({ brightness: 10, contrast: 0, saturation: 0 });
    expect(layer.baseImageBitmap).toBeNull();
  });

  it("clearBasicAdjustments notifies visual change (not just structural change)", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);

    const visualCb = vi.fn();
    engine.onVisualChange(visualCb);

    engine.applyBasicAdjustment(layer.id, { brightness: 10, contrast: 0, saturation: 0 });
    visualCb.mockClear();

    engine.clearBasicAdjustments(layer.id);

    // clearBasicAdjustments must trigger visual change (bitmap was restored)
    expect(visualCb).toHaveBeenCalled();
    expect(layer.imageBitmap).toBe(initialBitmap);
    expect(layer.baseImageBitmap).toBeNull();
    expect(layer.basicAdjustment).toBeUndefined();
  });

  it("restore() redo after undo does not close bitmaps referenced by the redo snapshot", () => {
    const engine = new DocumentEngine("doc-redo", "Redo Test", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);

    // Snapshot S0 — pre-adjustment state
    const snapS0 = engine.snapshot();

    // Apply adjustment → stores param, imageBitmap stays the original bitmap
    engine.applyBasicAdjustment(layer.id, { brightness: 20, contrast: 0, saturation: 0 });
    expect(layer.basicAdjustment).toEqual({ brightness: 20, contrast: 0, saturation: 0 });
    expect(layer.imageBitmap).toBe(initialBitmap);

    // Snapshot S1 — post-adjustment state
    const snapS1 = engine.snapshot();

    // Undo → restore S0
    engine.restore(snapS0);
    expect(initialBitmap.close).not.toHaveBeenCalled();
    const afterUndo = engine.getLayer(layer.id)!;
    expect(afterUndo.imageBitmap).toBe(initialBitmap);
    expect(afterUndo.basicAdjustment).toBeUndefined();

    // Redo → restore S1 (initialBitmap must NOT be closed — it's snapS1.imageBitmap)
    engine.restore(snapS1);
    const afterRedo = engine.getLayer(layer.id)!;
    expect(afterRedo.imageBitmap).toBe(initialBitmap);
    expect(afterRedo.basicAdjustment).toEqual({ brightness: 20, contrast: 0, saturation: 0 });
    expect(initialBitmap.close).not.toHaveBeenCalled();
  });

  it("restore() after setLayerImageBitmap does not produce broken bitmap references", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const layer = engine.addLayer("Layer 1");
    const initialBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, initialBitmap);

    // Apply adjustment — stores param, imageBitmap stays the original bitmap
    engine.applyBasicAdjustment(layer.id, { brightness: 10, contrast: 0, saturation: 0 });
    const snapAfterAdj = engine.snapshot();

    // Paint commit replaces the bitmap but keeps the adjustment (non-destructive)
    const paintBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, paintBitmap);
    const snapAfterPaint = engine.snapshot();

    // Undo paint — restore to snapAfterAdj
    engine.restore(snapAfterAdj);
    const restored1 = engine.getLayer(layer.id)!;
    expect(restored1.imageBitmap).toBe(initialBitmap);
    expect(restored1.basicAdjustment).toEqual({ brightness: 10, contrast: 0, saturation: 0 });

    // Redo paint — restore to snapAfterPaint (adjustment persisted through paint)
    engine.restore(snapAfterPaint);
    const restored2 = engine.getLayer(layer.id)!;
    expect(restored2.basicAdjustment).toEqual({ brightness: 10, contrast: 0, saturation: 0 });
    expect(restored2.imageBitmap).toBe(paintBitmap);
  });
});
