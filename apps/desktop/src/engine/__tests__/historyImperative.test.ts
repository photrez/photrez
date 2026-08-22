import { describe, expect, it } from "vitest";
import { CommandHistory, type HistoryTilePatches } from "../history";

function mkTile(fill: number) {
  return { x: 0, y: 0, width: 4, height: 4, data: new Uint8ClampedArray(64).fill(fill) };
}

/** Distinct imperative per stroke: layer tag encodes which stroke it belongs to. */
function patches(tag: string): HistoryTilePatches {
  return {
    layerId: `L-${tag}`,
    surfaceWidth: 256,
    surfaceHeight: 256,
    before: [mkTile(1)],
    after: [mkTile(2)],
  };
}

const SNAP = { layers: [], width: 10, height: 10 } as any;

describe("CommandHistory imperative entries are OWNED BY THEIR ENTRY (Fase 1)", () => {
  it("user scenario: strokes A,B,C — undo x3 is correct AND each redo replays ITS OWN after-tiles", () => {
    const h = new CommandHistory();
    const impA = patches("A");
    const impB = patches("B");
    const impC = patches("C");
    h.commit(SNAP, "Stroke A", impA);
    h.commit(SNAP, "Stroke B", impB);
    h.commit(SNAP, "Stroke C", impC);

    // undo x3 — each step exposes the entry's own BEFORE patches
    expect(h.undo(SNAP)).toBe(SNAP);
    expect(h.consumeLastUndoPatches()?.layerId).toBe("L-C");
    expect(h.undo(SNAP)).toBe(SNAP);
    expect(h.consumeLastUndoPatches()?.layerId).toBe("L-B");
    expect(h.undo(SNAP)).toBe(SNAP);
    expect(h.consumeLastUndoPatches()?.layerId).toBe("L-A");

    // THE BUG (2026-08-22): redo used to park a single live-state object,
    // so every redo replayed stroke C's tiles — A and B never came back.
    expect(h.redo(SNAP)).toBe(SNAP);
    expect(h.consumeLastRedoPatches()?.layerId).toBe("L-A");
    expect(h.redo(SNAP)).toBe(SNAP);
    expect(h.consumeLastRedoPatches()?.layerId).toBe("L-B");
    expect(h.redo(SNAP)).toBe(SNAP);
    expect(h.consumeLastRedoPatches()?.layerId).toBe("L-C");
  });

  it("imperative round-trips as the SAME object through undo+redo", () => {
    const h = new CommandHistory();
    const imp = patches("X");
    h.commit(SNAP, "S", imp);
    h.undo(SNAP);
    h.consumeLastUndoPatches();
    h.redo(SNAP);
    // The after-tiles replayed by redo ARE the committed entry's patches.
    expect(h.consumeLastRedoPatches()).toBe(imp);
  });

  it("undo consumes before-patches once; consume-once semantics hold", () => {
    const h = new CommandHistory();
    h.commit(SNAP, "S", patches("K"));
    h.undo(SNAP);
    expect(h.consumeLastUndoPatches()).toBeDefined();
    expect(h.consumeLastUndoPatches()).toBeUndefined();
  });

  it("snapshot-only entries expose no patches (legacy callers unaffected)", () => {
    const h = new CommandHistory();
    h.commit(SNAP, "Filter");
    expect(h.undo(SNAP)).toBe(SNAP);
    expect(h.consumeLastUndoPatches()).toBeUndefined();
    expect(h.redo(SNAP)).toBe(SNAP);
    expect(h.consumeLastRedoPatches()).toBeUndefined();
  });

  it("mixed entry kinds keep stacks consistent", () => {
    const h = new CommandHistory();
    h.commit(SNAP, "Filter"); // snapshot-only
    h.commit(SNAP, "Brush", patches("B1"));
    h.commit(SNAP, "Brush2", patches("B2"));

    h.undo(SNAP); // B2
    expect(h.consumeLastUndoPatches()?.layerId).toBe("L-B2");
    h.undo(SNAP); // B1
    expect(h.consumeLastUndoPatches()?.layerId).toBe("L-B1");
    h.undo(SNAP); // Filter
    expect(h.consumeLastUndoPatches()).toBeUndefined();

    expect(h.canUndo()).toBe(false);
    expect(h.canRedo()).toBe(true);
    // Redo order: oldest first — Filter (no patches), then B1.
    h.redo(SNAP);
    expect(h.consumeLastRedoPatches()).toBeUndefined();
    h.redo(SNAP);
    expect(h.consumeLastRedoPatches()?.layerId).toBe("L-B1");
    expect(h.getHistoryStack().length).toBe(4); // Open + 3
  });

  it("max depth eviction keeps imperative entries bounded", () => {
    const h = new CommandHistory(3);
    for (let i = 0; i < 6; i++) h.commit(SNAP, `B${i}`, patches(`D${i}`));
    let n = 0;
    while (h.canUndo()) {
      h.undo(SNAP);
      h.consumeLastUndoPatches();
      n++;
    }
    expect(n).toBe(3);
  });
});
