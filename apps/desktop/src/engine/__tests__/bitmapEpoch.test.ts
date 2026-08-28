import { describe, it, expect, vi, beforeEach } from "vitest";
import { DocumentEngine } from "../document";
import { createSnapshot, restoreSnapshot } from "../snapshot";
import type { LayerNode } from "../types";

function makeLayer(overrides: Partial<LayerNode> = {}): LayerNode {
  return {
    id: "l1",
    name: "Layer",
    type: "raster",
    visible: true,
    opacity: 1,
    locked: false,
    blendMode: "normal",
    width: 10,
    height: 10,
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    imageBitmap: { width: 10, height: 10, close: vi.fn() } as unknown as ImageBitmap,
    ...overrides,
  };
}

describe("bitmapEpoch — basic semantics", () => {
  it("bitmapEpoch starts undefined on new layer", () => {
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    const layer = engine.addLayer("L1");
    expect(layer.bitmapEpoch).toBeUndefined();
  });

  it("replaceLayerBitmap does NOT set bitmapEpoch", () => {
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    engine.addLayer("L1");
    const layer = engine.getLayers()[0]!;
    const newBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, newBitmap);
    expect(layer.bitmapEpoch).toBeUndefined();
    expect(layer.imageBitmap).toBe(newBitmap);
  });

  it("setLayerImageBitmap does NOT set bitmapEpoch", () => {
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    engine.addLayer("L1");
    const layer = engine.getLayers()[0]!;
    const newBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(layer.id, newBitmap);
    expect(layer.bitmapEpoch).toBeUndefined();
  });
});

describe("bitmapEpoch — snapshot roundtrip", () => {
  it("createSnapshot preserves bitmapEpoch", () => {
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    engine.addLayer("L1");
    const layer = engine.getLayers()[0]!;
    layer.bitmapEpoch = 42;
    const snap = createSnapshot(engine.getModel());
    expect(snap.layers[0].bitmapEpoch).toBe(42);
  });

  it("restoreSnapshot clears bitmapEpoch to undefined", () => {
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    engine.addLayer("L1");
    const layer = engine.getLayers()[0]!;
    layer.bitmapEpoch = 42;
    const snap = createSnapshot(engine.getModel());
    snap.layers[0].bitmapEpoch = 42;
    const restored = restoreSnapshot(snap);
    expect(restored.layers[0].bitmapEpoch).toBeUndefined();
  });

  it("engine.restore clears bitmapEpoch on all layers", () => {
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    engine.addLayer("L1");
    const layer = engine.getLayers()[0]!;
    layer.bitmapEpoch = 42;
    const snap = engine.snapshot();
    snap.layers[0].bitmapEpoch = 42;
    engine.restore(snap);
    const restoredLayer = engine.getLayers()[0]!;
    expect(restoredLayer.bitmapEpoch).toBeUndefined();
  });
});

describe("bitmapEpoch — ensureBitmapCurrent", () => {
  it("ensureBitmapCurrent is a no-op when no Rust store exists", async () => {
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    engine.addLayer("L1");
    const layer = engine.getLayers()[0]!;
    // No rust_pixels_get_epoch mock — will throw → return early
    await engine.ensureBitmapCurrent("doc", layer.id);
    expect(layer.bitmapEpoch).toBeUndefined();
  });

  it("ensureBitmapCurrent is a no-op when bitmapEpoch matches rustEpoch", async () => {
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    engine.addLayer("L1");
    const layer = engine.getLayers()[0]!;
    layer.bitmapEpoch = 5;
    // Mock invoke to return epoch 5
    const mockInvoke = vi.fn(async (cmd: string) => {
      if (cmd === "rust_pixels_get_epoch") return 5;
      return undefined;
    });
    vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
    await engine.ensureBitmapCurrent("doc", layer.id);
    expect(layer.bitmapEpoch).toBe(5); // unchanged
  });
});
