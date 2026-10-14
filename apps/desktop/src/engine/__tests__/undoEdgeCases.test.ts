// SPDX-License-Identifier: AGPL-3.0-or-later
//
// RETAINED cases from the pre-native-walker undoEdgeCases file: the two describe blocks
// whose subject is `engine.restore()` (NOT the deleted TS-stack pop). CommandHistory.undo/redo
// is gone (native walker owns undo/redo), so every block that called `history.undo()`
// is removed — its premise (TS pop) no longer exists. The `engine.restore` contract
// (restore marks dirty layers, does not close bitmaps) is unchanged product behavior.
import { describe, it, expect, vi } from "vitest";
import { DocumentEngine } from "../document";

describe("undo edge cases — cancel transform session (engine.restore directly)", () => {
  const g = (engine: DocumentEngine, id: string) => engine.getLayer(id)!;

  it("cancel after move restores original position and verifies dirtyLayerIds are set", () => {
    const engine = new DocumentEngine("doc-cancel-move", "Cancel Move", 800, 600);
    const layer = engine.addLayer("Layer 1");
    const originalSnapshot = engine.snapshot();

    engine.moveLayer(layer.id, 150, 75);
    engine.restore(originalSnapshot);

    expect(g(engine, layer.id).transform.x).toBe(0);
    expect(g(engine, layer.id).transform.y).toBe(0);
    expect(engine.getDirtyLayerIds()).toContain(layer.id);
  });

  it("cancel after resize restores original scale and marks layers dirty", () => {
    const engine = new DocumentEngine("doc-cancel-resize", "Cancel Resize", 800, 600);
    const layer = engine.addLayer("Layer 1");
    g(engine, layer.id).transform.scaleX = 1;
    g(engine, layer.id).transform.scaleY = 1;

    const originalSnapshot = engine.snapshot();
    engine.transformLayer(layer.id, { scaleX: 2.5, scaleY: 1.8 });
    engine.restore(originalSnapshot);

    expect(g(engine, layer.id).transform.scaleX).toBe(1);
    expect(g(engine, layer.id).transform.scaleY).toBe(1);
    expect(engine.getDirtyLayerIds()).toContain(layer.id);
  });

  it("cancel after resize+rotate restores all original properties", () => {
    const engine = new DocumentEngine("doc-cancel-combo", "Cancel Combo", 800, 600);
    const layer = engine.addLayer("Layer 1");
    const ref = g(engine, layer.id);
    ref.transform.x = 10;
    ref.transform.y = 20;
    ref.transform.scaleX = 1.5;
    ref.transform.rotation = 15;

    const originalSnapshot = engine.snapshot();
    engine.transformLayer(layer.id, {
      x: 200, y: 300, scaleX: 3, scaleY: 2, rotation: 90,
    });
    engine.restore(originalSnapshot);

    const restored = g(engine, layer.id);
    expect(restored.transform.x).toBe(10);
    expect(restored.transform.y).toBe(20);
    expect(restored.transform.scaleX).toBe(1.5);
    expect(restored.transform.scaleY).toBe(1);
    expect(restored.transform.rotation).toBe(15);
    expect(engine.getDirtyLayerIds()).toContain(layer.id);
  });

  it("restore does not close any imageBitmap references", () => {
    const bitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    const engine = new DocumentEngine("doc-cancel-bmp", "Cancel BMP", 800, 600);
    const layer = engine.addLayer("Layer 1", 100, 100);
    engine.setLayerImageBitmap(layer.id, bitmap);

    const newBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    const originalSnapshot = engine.snapshot();
    engine.setLayerImageBitmap(layer.id, newBitmap);
    engine.restore(originalSnapshot);

    expect(bitmap.close).not.toHaveBeenCalled();
    expect(newBitmap.close).not.toHaveBeenCalled();
    expect(g(engine, layer.id).imageBitmap).toBe(bitmap);
    expect(engine.getDirtyLayerIds()).toContain(layer.id);
  });
});

describe("undo edge cases — dirtyLayerIds after engine.restore()", () => {
  it("restore marks all existing layers as dirty", () => {
    const engine = new DocumentEngine("doc-dirty-restore", "Dirty Restore", 800, 600);
    const l1 = engine.addLayer("Layer 1");
    const l2 = engine.addLayer("Layer 2");

    const snap = engine.snapshot();
    engine.addLayer("Layer 3");

    engine.clearDirty();
    expect(engine.getDirtyLayerIds()).toHaveLength(0);

    engine.restore(snap);
    const dirtyIds = engine.getDirtyLayerIds();
    expect(dirtyIds).toContain(l1.id);
    expect(dirtyIds).toContain(l2.id);
    expect(dirtyIds).not.toContain(engine.getLayers().find((l) => l.name === "Layer 3")?.id);
  });

  it("restore after crop marks all post-crop layers as dirty", () => {
    const engine = new DocumentEngine("doc-dirty-crop", "Dirty Crop", 800, 600);
    engine.addLayer("Layer 1");

    const snap = engine.snapshot();
    engine.applyCrop(50, 50, 400, 300);
    engine.clearDirty();

    engine.restore(snap);
    const dirtyIds = engine.getDirtyLayerIds();
    for (const layer of engine.getLayers()) {
      expect(dirtyIds).toContain(layer.id);
    }
  });

  it("restore after deleteLayer marks remaining layers as dirty", () => {
    const engine = new DocumentEngine("doc-dirty-delete", "Dirty Delete", 800, 600);
    engine.addLayer("Layer 1");
    const l2 = engine.addLayer("Layer 2");

    const snap = engine.snapshot();
    engine.deleteLayer(l2.id);
    engine.clearDirty();

    engine.restore(snap);
    const dirtyIds = engine.getDirtyLayerIds();
    for (const layer of engine.getLayers()) {
      expect(dirtyIds).toContain(layer.id);
    }
  });

  it("restore after reorderLayer marks all layers as dirty", () => {
    const engine = new DocumentEngine("doc-dirty-reorder", "Dirty Reorder", 800, 600);
    engine.addLayer("A");
    engine.addLayer("B");
    engine.addLayer("C");

    const snap = engine.snapshot();
    engine.reorderLayer(0, 2);
    engine.clearDirty();

    engine.restore(snap);
    const dirtyIds = engine.getDirtyLayerIds();
    for (const layer of engine.getLayers()) {
      expect(dirtyIds).toContain(layer.id);
    }
  });

  it("restore clears dirty flags for layers no longer in the model", () => {
    const engine = new DocumentEngine("doc-dirty-removed", "Dirty Removed", 800, 600);
    const l1 = engine.addLayer("Layer 1");
    const snap = engine.snapshot();

    engine.clearDirty();
    engine.addLayer("Layer 2");
    engine.markLayerDirty("non-existent-id" as string);

    engine.restore(snap);
    const dirtyIds = engine.getDirtyLayerIds();
    expect(dirtyIds).not.toContain("non-existent-id");
    expect(dirtyIds).toContain(l1.id);
  });
});
