// apps/desktop/src/engine/__tests__/shapeUndoCorruption.test.ts
//
// Regression: undo/redo of a shape layer must restore the EXACT nested
// fill/stroke values, even when a later operation mutates the live model's
// nested shapeParams objects IN PLACE (the corruption class where a shallow
// snapshot shared the `fill`/`stroke` references with the live model, so a
// subsequent in-place mutation silently rewrote history).
//
// History contract: `commit(preState)` stores the state to return to on undo,
// and `undo(current)` pops it. So the pre-state snapshot is captured BEFORE the
// mutation and must stay isolated from later in-place edits to the live model.

import { describe, it, expect } from "vitest";
import { DocumentEngine } from "../document";
import { CommandHistory } from "../history";
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
    const history = new CommandHistory();

    // Capture the PRE-state (red fill, 2px stroke) the way the app does before
    // an operation, and commit it for undo.
    const preRed = engine.snapshot();
    history.commit(preRed);

    // A later operation mutates the live model's nested shapeParams IN PLACE
    // (the corruption class) — e.g. a live preview writes directly to the
    // object instead of swapping a fresh shapeParams.
    const live = engine.getLayer(layer.id)!;
    live.shapeParams!.fill.color = "#00ff00";
    live.shapeParams!.stroke.width = 99;

    // Undo -> should restore the committed pre-state, uncorrupted by the
    // in-place mutation above.
    const previous = history.undo(engine.snapshot());
    expect(previous).not.toBeNull();
    const restored = restoreSnapshot(previous!);
    const restoredLayer = restored.layers.find((l) => l.id === layer.id)!;
    expect(restoredLayer.shapeParams!.fill.color).toBe("#ff0000");
    expect(restoredLayer.shapeParams!.stroke.width).toBe(2);
  });

  it("redo round-trips nested values even after an in-place mutation post-commit", () => {
    const engine = new DocumentEngine("doc-1", "Test", 800, 600);
    const layer = engine.addShapeLayer("Shape 1", makeParams());
    const history = new CommandHistory();

    // op 0: pre-state = red.
    const preRed = engine.snapshot();
    history.commit(preRed);

    // op 1: mutate live to green, commit preGreen.
    const live = engine.getLayer(layer.id)!;
    live.shapeParams!.fill.color = "#00ff00";
    const preGreen = engine.snapshot();
    history.commit(preGreen);

    // op 2 (AFTER preGreen was committed): mutate live IN PLACE to blue. With a
    // shallow snapshot, this rewrites preGreen's shared fill reference.
    live.shapeParams!.fill.color = "#0000ff";

    // The app restores the undo/redo result into the live model between steps
    // (see useEditorCommands.ts -> engine.restore(history.undo(...))). Without
    // that step the redo stack would receive the wrong `currentState`, so the
    // test must simulate it to exercise the real redo contract.
    const applyUndo = () => {
      const prev = history.undo(engine.snapshot())!;
      engine.restore(restoreSnapshot(prev));
      return prev;
    };
    const applyRedo = () => {
      const next = history.redo(engine.snapshot())!;
      engine.restore(restoreSnapshot(next));
      return next;
    };
    const fillOf = (snap: DocumentModel) =>
      restoreSnapshot(snap).layers.find((l) => l.id === layer.id)!.shapeParams!.fill.color;

    // Undo -> should restore preGreen (green), NOT the corrupted blue.
    const u = applyUndo();
    expect(fillOf(u)).toBe("#00ff00");

    // Undo again -> red.
    const u2 = applyUndo();
    expect(fillOf(u2)).toBe("#ff0000");

    // Redo -> green again, still intact (proves redo returns the deep-copied
    // preGreen, not a snapshot whose fill was rewritten in place).
    const r = applyRedo();
    expect(fillOf(r)).toBe("#00ff00");
  });
});
