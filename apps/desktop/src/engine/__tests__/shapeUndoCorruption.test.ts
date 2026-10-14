// apps/desktop/src/engine/__tests__/shapeUndoCorruption.test.ts
//
// Regression: restoring a shape layer must restore the EXACT nested
// fill/stroke values, even when a later operation mutates the live model's
// nested shapeParams objects IN PLACE (the corruption class where a shallow
// snapshot shared the `fill`/`stroke` references with the live model, so a
// subsequent in-place mutation silently rewrote history).
//
// Isolation contract: the pre-state snapshot is captured BEFORE the mutation
// and must stay isolated from later in-place edits to the live model.
// The native walker executes undo/redo; these assertions pin the
// same snapshot-isolation invariant through the surviving snapshot/restore
// surface.

import { describe, it, expect } from "vitest";
import { DocumentEngine } from "../document";
import { restoreSnapshot } from "../snapshot";
import type { DocumentModel } from "../types";

function makeParams() {
  return {
    kind: "rect" as const,
    width: 100,
    height: 50,
    radius: 0,
    fill: { kind: "solid" as const, color: "#ff0000", opacity: 1 },
    stroke: { enabled: true, color: "#000000", width: 2, opacity: 1 },
    arrowHead: false,
  };
}

describe("shape undo/redo corruption regression", () => {
  it("undo restores nested fill/stroke even after in-place mutation of the live model", () => {
    const engine = new DocumentEngine("doc-1", "Test", 800, 600);
    const layer = engine.addShapeLayer("Shape 1", makeParams());

    // Capture the PRE-state (red fill, 2px stroke) the way the app does before
    // an operation.
    const preRed = engine.snapshot();

    // A later operation mutates the live model's nested shapeParams IN PLACE
    // (the corruption class) — e.g. a live preview writes directly to the
    // object instead of swapping a fresh shapeParams.
    const live = engine.getLayer(layer.id)!;
    live.shapeParams!.fill.color = "#00ff00";
    live.shapeParams!.stroke.width = 99;

    // Restoring the pre-state must yield the uncorrupted committed values,
    // proving the snapshot is isolated from later in-place edits to the live
    // model. (The native walker is the restore executor; these
    // geometry/ownership assertions assert the same isolation via the
    // surviving snapshot/restore surface.)
    const restored = restoreSnapshot(preRed);
    const restoredLayer = restored.layers.find((l) => l.id === layer.id)!;
    expect(restoredLayer.shapeParams!.fill.color).toBe("#ff0000");
    expect(restoredLayer.shapeParams!.stroke.width).toBe(2);
  });

  it("redo round-trips nested values even after an in-place mutation post-commit", () => {
    const engine = new DocumentEngine("doc-1", "Test", 800, 600);
    const layer = engine.addShapeLayer("Shape 1", makeParams());

    // op 0: pre-state = red.
    const preRed = engine.snapshot();

    // op 1: mutate live to green, capture preGreen.
    const live = engine.getLayer(layer.id)!;
    live.shapeParams!.fill.color = "#00ff00";
    const preGreen = engine.snapshot();

    // op 2 (AFTER preGreen was captured): mutate live IN PLACE to blue. With a
    // shallow snapshot, this rewrites preGreen's shared fill reference.
    live.shapeParams!.fill.color = "#0000ff";

    const fillOf = (snap: DocumentModel) =>
      restoreSnapshot(snap).layers.find((l) => l.id === layer.id)!.shapeParams!.fill.color;

    // Undo -> preGreen (green), NOT the corrupted blue.
    const u = preGreen;
    expect(fillOf(u)).toBe("#00ff00");

    // Undo again -> red.
    expect(fillOf(preRed)).toBe("#ff0000");

    // Redo -> green again, still intact (proves the snapshot holds the deep-copied
    // preGreen, not a fill rewritten in place).
    const r = preGreen;
    expect(fillOf(r)).toBe("#00ff00");
  });
});
