// apps/desktop/src/engine/__tests__/snapshotShapeParams.test.ts
//
// Regression: createSnapshot must deep-copy a shape layer's nested
// `shapeParams.fill` / `shapeParams.stroke` objects. A shallow spread shares
// those nested references with the live model, so a later in-place mutation
// (e.g. `params.fill.color = ...`) corrupts the undo/redo history snapshot.

import { describe, it, expect } from "vitest";
import { DocumentEngine } from "../document";
import { createSnapshot } from "../snapshot";

describe("snapshot shapeParams deep copy", () => {
  it("isolates nested fill/stroke objects from live mutation", () => {
    const engine = new DocumentEngine("doc-1", "Test Doc", 100, 100);
    const params = {
      kind: "rect" as const,
      width: 100,
      height: 50,
      radius: 0,
      fill: { kind: "solid" as const, color: "#ff0000", opacity: 1 },
      stroke: { enabled: true, color: "#000000", width: 2, opacity: 1 },
      arrowHead: false,
    };
    const layer = engine.addShapeLayer("Shape 1", params);
    const live = engine.getLayer(layer.id)!;

    // Snapshot BEFORE mutating the live layer.
    const snap = createSnapshot(engine.snapshot());
    const snapLayer = snap.layers.find((l) => l.id === layer.id)!;

    // Mutate the nested objects of the live layer in place.
    live.shapeParams!.fill.color = "#00ff00";
    live.shapeParams!.stroke.width = 99;

    // The snapshot must be untouched (independent object references).
    expect(snapLayer.shapeParams!.fill.color).toBe("#ff0000");
    expect(snapLayer.shapeParams!.stroke.width).toBe(2);
  });
});
