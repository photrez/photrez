/**
 * History audit suite (regression 2026-06-18).
 *
 * User report: "saat memindahkan layer atau operasi lainnya kadang kayak
 * ke save dihistory kadang tidak". This test file exercises REAL
 * DocumentEngine + CommandHistory (no mocks) through the input-handler to
 * pin down exactly when an undo entry IS and IS NOT produced.
 *
 * The original bug was move-tool / selection-move pointerDown calling
 * history.commit() unconditionally. Clicks without drags pushed ghost
 * entries that "undo did nothing" — making the history feel unreliable
 * because consecutive undos appeared to skip steps with no visual change.
 *
 * Companion tests in input-handler-move.test.ts and input-handler-selection.test.ts
 * cover the same behavior at the unit level. This file proves the integration
 * works against the real engine + history classes.
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  handlePointerDown,
  handlePointerMove,
  handlePointerUp,
} from "../viewport/input-handler";
import { DocumentEngine } from "../engine/document";
import { CommandHistory } from "../engine/history";
import type { ToolContext } from "../viewport/input-handler";

function setupDoc() {
  const engine = new DocumentEngine("doc", "Test", 800, 600);
  const layer = engine.addLayer("Layer 1");
  engine.setActiveLayer(layer.id);
  const history = new CommandHistory();
  return { engine, layer, history };
}

function makeCtx(
  selectedLayerId: string | null,
  overrides: Partial<ToolContext> = {},
): ToolContext {
  return {
    fgColor: "#000",
    bgColor: "#fff",
    brushSize: 20,
    brushHardness: 1,
    brushOpacity: 1,
    paintSettings: { size: 20, hardness: 1, opacity: 1, flow: 1, smoothing: 0 },
    selectedLayerId,
    isAltPressed: false,
    isShiftPressed: false,
    isDragging: false,
    dragStart: { x: 0, y: 0 },
    dragCurrent: { x: 0, y: 0 },
    strokePoints: [],
    dragTool: null,
    ...overrides,
  };
}

const NOOP = () => {};

// ─── Move tool history audit ───

describe("history audit — move tool deferred commit", () => {
  let engine: DocumentEngine;
  let layer: ReturnType<DocumentEngine["addLayer"]>;
  let history: CommandHistory;

  beforeEach(() => {
    const setup = setupDoc();
    engine = setup.engine;
    layer = setup.layer;
    history = setup.history;
  });

  it("click without drag: undo stack stays at zero (no ghost entry)", () => {
    const ctx = makeCtx(layer.id);
    expect(history.getUndoCount()).toBe(0);

    handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
    handlePointerUp("move", 100, 50, engine, history, NOOP, ctx);

    expect(history.getUndoCount()).toBe(0);
    expect(history.canUndo()).toBe(false);
  });

  it("click + actual drag: input-handler does NOT commit history (useCanvasLayerDrag owns it)", () => {
    const ctx = makeCtx(layer.id);
    expect(history.getUndoCount()).toBe(0);

    handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
    handlePointerMove("move", 250, 200, engine, NOOP, ctx);
    handlePointerUp("move", 250, 200, engine, history, NOOP, ctx);

    // Move tool history is owned by useCanvasLayerDrag.onPointerUp.
    // input-handler just moves the layer via engine.moveLayer.
    expect(history.getUndoCount()).toBe(0);
    expect(ctx.pendingHistorySnapshot).toBeNull();
  });

  it("pending state is cleaned up after drag (no stale leak)", () => {
    const ctx = makeCtx(layer.id);

    handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
    handlePointerMove("move", 250, 200, engine, NOOP, ctx);
    handlePointerUp("move", 250, 200, engine, history, NOOP, ctx);

    expect(ctx.pendingHistorySnapshot).toBeNull();
    expect(ctx.pendingOriginalLayerPos).toBeNull();
  });

  it("five consecutive clicks-without-drag: undo stack stays at zero (no spam)", () => {
    const ctx = makeCtx(layer.id);
    for (let i = 0; i < 5; i++) {
      handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
      handlePointerUp("move", 100, 50, engine, history, NOOP, ctx);
    }
    expect(history.getUndoCount()).toBe(0);
  });

  it("three consecutive drags: input-handler commits ZERO history entries", () => {
    const ctx = makeCtx(layer.id);
    for (let i = 0; i < 3; i++) {
      handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
      handlePointerMove("move", 100 + (i + 1) * 20, 50 + (i + 1) * 20, engine, NOOP, ctx);
      handlePointerUp("move", 100 + (i + 1) * 20, 50 + (i + 1) * 20, engine, history, NOOP, ctx);
    }
    expect(history.getUndoCount()).toBe(0);
  });

  it("mixed clicks and drags: only drags produce undo entries via useCanvasLayerDrag", () => {
    const ctx = makeCtx(layer.id);

    // 2 clicks (no drag)
    handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
    handlePointerUp("move", 100, 50, engine, history, NOOP, ctx);
    handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
    handlePointerUp("move", 100, 50, engine, history, NOOP, ctx);
    expect(history.getUndoCount()).toBe(0);

    // 1 real drag — input-handler doesn't commit, useCanvasLayerDrag does
    handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
    handlePointerMove("move", 200, 150, engine, NOOP, ctx);
    handlePointerUp("move", 200, 150, engine, history, NOOP, ctx);
    expect(history.getUndoCount()).toBe(0);

    // 1 more click (no drag)
    handlePointerDown("move", 200, 150, engine, history, NOOP, ctx);
    handlePointerUp("move", 200, 150, engine, history, NOOP, ctx);
    expect(history.getUndoCount()).toBe(0);

    // 1 more drag
    handlePointerDown("move", 200, 150, engine, history, NOOP, ctx);
    handlePointerMove("move", 300, 250, engine, NOOP, ctx);
    handlePointerUp("move", 300, 250, engine, history, NOOP, ctx);
    expect(history.getUndoCount()).toBe(0);
  });

  it("drag back to original position: undo stack stays at zero", () => {
    const ctx = makeCtx(layer.id);
    const startX = layer.transform.x;
    const startY = layer.transform.y;

    handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
    // Drag away
    handlePointerMove("move", 200, 150, engine, NOOP, ctx);
    // Drag back to the EXACT original position
    // pointermove dx,dy = docX - dragStart.x, docY - dragStart.y where
    // dragStart was set to (100 - startX, 50 - startY) at pointerdown.
    // To restore (startX, startY) we need newX = startX → docX = dragStart.x + startX
    // dragStart.x = 100 - startX. So docX = 100 - startX + startX = 100. Symmetrically docY = 50.
    handlePointerMove("move", 100, 50, engine, NOOP, ctx);
    handlePointerUp("move", 100, 50, engine, history, NOOP, ctx);

    expect(layer.transform.x).toBe(startX);
    expect(layer.transform.y).toBe(startY);
    expect(history.getUndoCount()).toBe(0);
  });

  it("locked layer: no undo entries regardless of drag", () => {
    engine.setLayerLocked(layer.id, true);
    const ctx = makeCtx(layer.id);

    handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
    handlePointerMove("move", 250, 200, engine, NOOP, ctx);
    handlePointerUp("move", 250, 200, engine, history, NOOP, ctx);

    expect(history.getUndoCount()).toBe(0);
  });

  it("no selected layer: no undo entries regardless of pointer activity", () => {
    const ctx = makeCtx(null);

    handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
    handlePointerMove("move", 250, 200, engine, NOOP, ctx);
    handlePointerUp("move", 250, 200, engine, history, NOOP, ctx);

    expect(history.getUndoCount()).toBe(0);
  });

  it("undo after click-without-drag is a no-op (canUndo stays false)", () => {
    const ctx = makeCtx(layer.id);

    handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
    handlePointerUp("move", 100, 50, engine, history, NOOP, ctx);

    expect(history.canUndo()).toBe(false);
    expect(history.undo(engine.snapshot())).toBeNull();
  });

  it("undo + redo round-trip preserves both positions (via direct history.commit)", () => {
    // Use direct history.commit instead of input-handler move (input-handler
    // does not commit history for move tool — useCanvasLayerDrag owns it).
    const startX = layer.transform.x;
    const startY = layer.transform.y;

    history.commit(engine.snapshot());
    engine.moveLayer(layer.id, 250, 200);

    const afterDragX = layer.transform.x;
    const afterDragY = layer.transform.y;
    expect(afterDragX).not.toBe(startX);

    // Undo
    const prev = history.undo(engine.snapshot())!;
    engine.restore(prev);
    expect(engine.getLayer(layer.id)!.transform.x).toBe(startX);

    // Redo
    const next = history.redo(engine.snapshot())!;
    engine.restore(next);
    expect(engine.getLayer(layer.id)!.transform.x).toBe(afterDragX);
  });

  it("starting a new edit invalidates the redo stack (standard undo semantics)", () => {
    // Use direct history.commit instead of input-handler move tool calls.
    // Op 1: move layer
    history.commit(engine.snapshot());
    engine.moveLayer(layer.id, 200, 150);

    // Undo → redo available
    engine.restore(history.undo(engine.snapshot())!);
    expect(history.canRedo()).toBe(true);

    // New edit → redo cleared
    history.commit(engine.snapshot());
    engine.moveLayer(layer.id, 300, 250);

    expect(history.canRedo()).toBe(false);
  });
});

// ─── Selection-move history audit ───

describe("history audit — selection-move deferred commit", () => {
  let engine: DocumentEngine;
  let layer: ReturnType<DocumentEngine["addLayer"]>;
  let history: CommandHistory;

  beforeEach(() => {
    const setup = setupDoc();
    engine = setup.engine;
    layer = setup.layer;
    history = setup.history;
    // Pre-create a selection so move-selection branch can fire.
    engine.createSelection(50, 50, 200, 150);
  });

  it("click inside selection without drag: no undo entry", () => {
    const ctx = makeCtx(layer.id, {
      selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
      onSelectionMoved: NOOP,
    });

    handlePointerDown("selection", 100, 100, engine, history, NOOP, ctx);
    handlePointerUp("selection", 100, 100, engine, history, NOOP, ctx);

    expect(history.getUndoCount()).toBe(0);
  });

  it("drag selection by real distance: exactly 1 undo entry", () => {
    const ctx = makeCtx(layer.id, {
      selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
      onSelectionMoved: (x, y) => engine.createSelection(x, y, 200, 150),
    });

    handlePointerDown("selection", 100, 100, engine, history, NOOP, ctx);
    handlePointerUp("selection", 200, 200, engine, history, NOOP, ctx);

    expect(history.getUndoCount()).toBe(1);
  });

  it("click outside selection (draw mode): no undo entry on a quick click", () => {
    const ctx = makeCtx(layer.id, {
      selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
      onSelectionCreated: NOOP,
    });

    // Click outside selection bounds
    handlePointerDown("selection", 400, 400, engine, history, NOOP, ctx);
    handlePointerUp("selection", 400, 400, engine, history, NOOP, ctx);

    expect(history.getUndoCount()).toBe(0);
  });

  it("five clicks inside selection without drag: no undo spam", () => {
    const ctx = makeCtx(layer.id, {
      selectionBounds: { x: 50, y: 50, width: 200, height: 150 },
      onSelectionMoved: NOOP,
    });

    for (let i = 0; i < 5; i++) {
      handlePointerDown("selection", 100, 100, engine, history, NOOP, ctx);
      handlePointerUp("selection", 100, 100, engine, history, NOOP, ctx);
    }
    expect(history.getUndoCount()).toBe(0);
  });
});

// ─── Layer operation history audit (sanity: these should still commit immediately) ───

describe("history audit — eager-commit layer operations (sanity)", () => {
  let engine: DocumentEngine;
  let history: CommandHistory;

  beforeEach(() => {
    const setup = setupDoc();
    engine = setup.engine;
    history = setup.history;
  });

  it("addLayer with pre-commit produces exactly 1 undo entry", () => {
    history.commit(engine.snapshot());
    engine.addLayer("Layer 2");
    expect(history.getUndoCount()).toBe(1);
  });

  it("deleteLayer with pre-commit produces exactly 1 undo entry", () => {
    const l2 = engine.addLayer("Layer 2");
    history.commit(engine.snapshot());
    engine.deleteLayer(l2.id);
    expect(history.getUndoCount()).toBe(1);
  });

  it("reorderLayer with pre-commit produces exactly 1 undo entry", () => {
    engine.addLayer("Layer 2");
    history.commit(engine.snapshot());
    engine.reorderLayer(0, 1);
    expect(history.getUndoCount()).toBe(1);
  });

  it("setLayerOpacity with pre-commit produces exactly 1 undo entry", () => {
    const active = engine.getActiveLayerId()!;
    history.commit(engine.snapshot());
    engine.setLayerOpacity(active, 0.5);
    expect(history.getUndoCount()).toBe(1);
  });

  it("flipLayer with pre-commit produces exactly 1 undo entry", () => {
    const active = engine.getActiveLayerId()!;
    history.commit(engine.snapshot());
    engine.flipLayer(active, "h");
    expect(history.getUndoCount()).toBe(1);
  });

  it("undo restores pre-flip state correctly", () => {
    const active = engine.getActiveLayerId()!;
    const beforeFlip = engine.getLayer(active)!.transform.flipH;
    history.commit(engine.snapshot());
    engine.flipLayer(active, "h");
    expect(engine.getLayer(active)!.transform.flipH).toBe(!beforeFlip);

    const prev = history.undo(engine.snapshot())!;
    engine.restore(prev);
    expect(engine.getLayer(active)!.transform.flipH).toBe(beforeFlip);
  });

  it("undo restores pre-opacity state correctly", () => {
    const active = engine.getActiveLayerId()!;
    const beforeOpacity = engine.getLayer(active)!.opacity;
    history.commit(engine.snapshot());
    engine.setLayerOpacity(active, 0.42);
    expect(engine.getLayer(active)!.opacity).toBe(0.42);

    const prev = history.undo(engine.snapshot())!;
    engine.restore(prev);
    expect(engine.getLayer(active)!.opacity).toBe(beforeOpacity);
  });

  it("multiple layer operations stack correctly in history (5 ops → 5 entries)", () => {
    const l1 = engine.getActiveLayerId()!;

    history.commit(engine.snapshot()); engine.addLayer("L2");
    history.commit(engine.snapshot()); engine.addLayer("L3");
    history.commit(engine.snapshot()); engine.setLayerOpacity(l1, 0.7);
    history.commit(engine.snapshot()); engine.flipLayer(l1, "h");
    history.commit(engine.snapshot()); engine.setLayerVisibility(l1, false);

    expect(history.getUndoCount()).toBe(5);

    // Walking back through history must restore each state in reverse order.
    let cur = engine.snapshot();
    let prev = history.undo(cur)!;
    engine.restore(prev);
    expect(engine.getLayer(l1)!.visible).toBe(true);

    cur = engine.snapshot();
    prev = history.undo(cur)!;
    engine.restore(prev);
    expect(engine.getLayer(l1)!.transform.flipH).toBe(false);
  });
});

// ─── Undo-to-initial: complete round-trip back to the starting state ───

describe("history audit — undo-to-initial round-trip (user-reported regression)", () => {
  /**
   * User report follow-up:
   *   "kalau aksi yang sudah dilakukan banyak maka kalau diundo terus kan
   *    harusnya layer bakal kembali keposisi tengah/semula, tapi yang terjadi
   *    adalah tidak bisa diundo sampai titik awal"
   *
   * Root cause was multiple ghost-commit sites that pushed no-op entries to
   * the undo stack: input-handler.ts move/selection (fixed prior), the
   * useSelectionTransformDrag "move" handle, transformSession unconditional
   * commit on Apply, and MoveOptionBar field commits with no value change.
   *
   * These tests pin down: N real edits → exactly N history entries → undo N
   * times → layer is back at its original (0,0,1,1,0,false,false) transform.
   */

  let engine: DocumentEngine;
  let layer: ReturnType<DocumentEngine["addLayer"]>;
  let history: CommandHistory;

  beforeEach(() => {
    const setup = setupDoc();
    engine = setup.engine;
    layer = setup.layer;
    history = setup.history;
  });

  function undoAll() {
    while (history.canUndo()) {
      const prev = history.undo(engine.snapshot());
      if (!prev) break;
      engine.restore(prev);
    }
  }

  it("10 sequential move drags via input-handler: ZERO history entries (useCanvasLayerDrag owns it)", () => {
    const ctx = makeCtx(layer.id);
    const originX = layer.transform.x;
    const originY = layer.transform.y;

    for (let i = 0; i < 10; i++) {
      handlePointerDown("move", 100 + i * 5, 50 + i * 5, engine, history, NOOP, ctx);
      handlePointerMove("move", 200 + i * 5, 150 + i * 5, engine, NOOP, ctx);
      handlePointerUp("move", 200 + i * 5, 150 + i * 5, engine, history, NOOP, ctx);
    }
    // Move tool history is owned by useCanvasLayerDrag, not input-handler
    expect(history.getUndoCount()).toBe(0);
  });

  it("mixed clicks (no drag) + real drags: input-handler commits nothing", () => {
    const ctx = makeCtx(layer.id);
    const originX = layer.transform.x;
    const originY = layer.transform.y;

    // 3 ghost clicks + 2 real drags + 3 more ghost clicks + 2 real drags
    for (let i = 0; i < 3; i++) {
      handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
      handlePointerUp("move", 100, 50, engine, history, NOOP, ctx);
    }
    for (let i = 0; i < 2; i++) {
      handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
      handlePointerMove("move", 150 + i * 10, 100 + i * 10, engine, NOOP, ctx);
      handlePointerUp("move", 150 + i * 10, 100 + i * 10, engine, history, NOOP, ctx);
    }
    for (let i = 0; i < 3; i++) {
      handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
      handlePointerUp("move", 100, 50, engine, history, NOOP, ctx);
    }
    for (let i = 0; i < 2; i++) {
      handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
      handlePointerMove("move", 200 + i * 10, 200 + i * 10, engine, NOOP, ctx);
      handlePointerUp("move", 200 + i * 10, 200 + i * 10, engine, history, NOOP, ctx);
    }

    expect(history.getUndoCount()).toBe(0);
  });

  it("addLayer → move → flip → opacity round-trip back to fully initial", () => {
    const initialLayerCount = engine.getLayers().length;
    const initialOpacity = layer.opacity;
    const initialFlipH = layer.transform.flipH;
    const initialX = layer.transform.x;
    const initialY = layer.transform.y;

    // Op 1: add a new layer
    history.commit(engine.snapshot());
    engine.addLayer("Layer 2");

    // Op 2: move layer 1 (input-handler doesn't commit, op 1/3/4 handle it)
    const ctx = makeCtx(layer.id);
    handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
    handlePointerMove("move", 200, 150, engine, NOOP, ctx);
    handlePointerUp("move", 200, 150, engine, history, NOOP, ctx);

    // Op 3: flip layer 1 horizontally
    history.commit(engine.snapshot());
    engine.flipLayer(layer.id, "h");

    // Op 4: change opacity
    history.commit(engine.snapshot());
    engine.setLayerOpacity(layer.id, 0.42);

    // 3 commits (addLayer, flip, opacity) — input-handler move doesn't commit
    expect(history.getUndoCount()).toBe(3);

    undoAll();

    expect(engine.getLayers().length).toBe(initialLayerCount);
    expect(engine.getLayer(layer.id)!.transform.x).toBe(initialX);
    expect(engine.getLayer(layer.id)!.transform.y).toBe(initialY);
    expect(engine.getLayer(layer.id)!.transform.flipH).toBe(initialFlipH);
    expect(engine.getLayer(layer.id)!.opacity).toBe(initialOpacity);
  });

  it("ghost click between real edits: input-handler commits nothing (useCanvasLayerDrag owns it)", () => {
    const ctx = makeCtx(layer.id);

    // input-handler move drag #1
    handlePointerDown("move", 100, 50, engine, history, NOOP, ctx);
    handlePointerMove("move", 200, 150, engine, NOOP, ctx);
    handlePointerUp("move", 200, 150, engine, history, NOOP, ctx);

    // Ghost click (no drag)
    handlePointerDown("move", 200, 150, engine, history, NOOP, ctx);
    handlePointerUp("move", 200, 150, engine, history, NOOP, ctx);

    // input-handler move drag #2
    handlePointerDown("move", 200, 150, engine, history, NOOP, ctx);
    handlePointerMove("move", 300, 250, engine, NOOP, ctx);
    handlePointerUp("move", 300, 250, engine, history, NOOP, ctx);

    expect(history.getUndoCount()).toBe(0);
    expect(ctx.pendingHistorySnapshot).toBeNull();
  });

  it("undo + redo + new edit via direct history (non-input-handler operations)", () => {
    // This test uses explicit history.commit for non-move-tool operations.
    // input-handler move tool calls don't commit; use direct engine ops instead.
    // Op 1: move via direct engine call
    history.commit(engine.snapshot());
    engine.moveLayer(layer.id, 200, 150);
    expect(history.getUndoCount()).toBe(1);

    // Op 2: move again
    history.commit(engine.snapshot());
    engine.moveLayer(layer.id, 300, 250);
    expect(history.getUndoCount()).toBe(2);

    // Undo once
    engine.restore(history.undo(engine.snapshot())!);
    expect(history.canRedo()).toBe(true);

    // New edit → redo cleared
    history.commit(engine.snapshot());
    engine.moveLayer(layer.id, 400, 350);

    expect(history.canRedo()).toBe(false);
    expect(history.getUndoCount()).toBe(2);
  });

  it("respects MAX_HISTORY_DEPTH: oldest entry evicted, but recent ops still undo-able", () => {
    // Use direct engine.moveLayer + history.commit for predictable positions.
    const SMALL_DEPTH = 5;
    const h = new CommandHistory(SMALL_DEPTH);

    // 8 ops at predictable positions: x = (i+1)*10.
    for (let i = 0; i < 8; i++) {
      h.commit(engine.snapshot());
      engine.moveLayer(layer.id, (i + 1) * 10, 0);
    }

    expect(h.getUndoCount()).toBe(SMALL_DEPTH);

    // After 8 ops with max depth 5: oldest 3 evicted. Undo stack now holds
    // snapshots taken BEFORE ops #4..#8, i.e. x=30, x=40, x=50, x=60, x=70.
    // Undoing all 5 lands the layer at x=30 (the state captured before op #4).
    while (h.canUndo()) {
      engine.restore(h.undo(engine.snapshot())!);
    }
    expect(engine.getLayer(layer.id)!.transform.x).toBe(30);
  });

  it("undo through ghost-free history reaches the EXACT initial state (deep equality)", () => {
    const initialSnapshot = engine.snapshot();

    // Use direct history.commit instead of input-handler move.
    for (let i = 0; i < 7; i++) {
      history.commit(engine.snapshot());
      engine.moveLayer(layer.id, 150 + i * 7, 100 + i * 11);
    }

    undoAll();

    const final = engine.snapshot();
    expect(final.layers[0].transform).toEqual(initialSnapshot.layers[0].transform);
    expect(final.layers[0].opacity).toBe(initialSnapshot.layers[0].opacity);
    expect(final.layers[0].visible).toBe(initialSnapshot.layers[0].visible);
    expect(final.layers.length).toBe(initialSnapshot.layers.length);
  });

  it("undo + redo + undo round-trip preserves state exactly (no drift)", () => {
    // Use direct history.commit instead of input-handler move tool.
    history.commit(engine.snapshot());
    engine.moveLayer(layer.id, 250, 200);

    const afterDrag = engine.snapshot();

    // Undo → snapshot before drag.
    engine.restore(history.undo(engine.snapshot())!);
    const afterUndo = engine.snapshot();

    // Redo → back to after-drag.
    engine.restore(history.redo(engine.snapshot())!);
    const afterRedo = engine.snapshot();
    expect(afterRedo.layers[0].transform).toEqual(afterDrag.layers[0].transform);

    // Undo again → back to before-drag.
    engine.restore(history.undo(engine.snapshot())!);
    const afterSecondUndo = engine.snapshot();
    expect(afterSecondUndo.layers[0].transform).toEqual(
      afterUndo.layers[0].transform,
    );
  });
});

// ─── Transform session — empty-session apply must not commit ───

describe("history audit — transformSession apply with no change", () => {
  it("commitLayerTransformSession does NOT commit if transform is unchanged", async () => {
    const { commitLayerTransformSession } = await import(
      "../components/editor/transformSession"
    );

    const { engine, layer } = setupDoc();
    const history = { commit: vi.fn() };

    const originalTransform = { ...layer.transform };
    const originalSnapshot = engine.snapshot();
    const session = {
      documentId: engine.getId(),
      layerId: layer.id,
      originalSnapshot,
      originalTransform,
      mode: "resize" as const,
      lockRatio: false,
      startedAt: Date.now(),
    };

    // The session is "applied" but the layer transform was never modified.
    const ok = commitLayerTransformSession(session, engine, history);
    expect(ok).toBe(true);
    // Ghost commit prevention: no history entry produced.
    expect(history.commit).not.toHaveBeenCalled();
  });

  it("commitLayerTransformSession commits when transform changed even slightly", async () => {
    const { commitLayerTransformSession } = await import(
      "../components/editor/transformSession"
    );

    const { engine, layer } = setupDoc();
    const history = { commit: vi.fn() };

    const originalTransform = { ...layer.transform };
    const originalSnapshot = engine.snapshot();
    const session = {
      documentId: engine.getId(),
      layerId: layer.id,
      originalSnapshot,
      originalTransform,
      mode: "resize" as const,
      lockRatio: false,
      startedAt: Date.now(),
    };

    // Simulate a small mutation during the session.
    engine.transformLayer(layer.id, { x: layer.transform.x + 1 });

    const ok = commitLayerTransformSession(session, engine, history);
    expect(ok).toBe(true);
    expect(history.commit).toHaveBeenCalledTimes(1);
    expect(history.commit).toHaveBeenCalledWith(originalSnapshot, "Transform Layer", undefined, undefined, []);
  });

  it("each transform field change is detected: rotation, scale, flip", async () => {
    const { commitLayerTransformSession } = await import(
      "../components/editor/transformSession"
    );

    const fields = [
      { rotation: 45 },
      { scaleX: 1.5 },
      { scaleY: 2 },
      { flipH: true },
      { flipV: true },
    ];

    for (const mut of fields) {
      const { engine, layer } = setupDoc();
      const history = { commit: vi.fn() };
      const session = {
        documentId: engine.getId(),
        layerId: layer.id,
        originalSnapshot: engine.snapshot(),
        originalTransform: { ...layer.transform },
        mode: "resize" as const,
        lockRatio: false,
        startedAt: Date.now(),
      };

      engine.transformLayer(layer.id, mut);
      const ok = commitLayerTransformSession(session, engine, history);
      expect(ok).toBe(true);
      expect(history.commit).toHaveBeenCalledTimes(1);
    }
  });

  it("session for a different engine: returns false, no commit", async () => {
    const { commitLayerTransformSession } = await import(
      "../components/editor/transformSession"
    );

    const { engine, layer } = setupDoc();
    const otherEngine = new DocumentEngine("other-doc", "Other", 100, 100);
    const history = { commit: vi.fn() };
    const session = {
      documentId: "other-doc", // mismatch
      layerId: layer.id,
      originalSnapshot: engine.snapshot(),
      originalTransform: { ...layer.transform },
      mode: "resize" as const,
      lockRatio: false,
      startedAt: Date.now(),
    };

    const ok = commitLayerTransformSession(session, engine, history);
    expect(ok).toBe(false);
    expect(history.commit).not.toHaveBeenCalled();
    void otherEngine;
  });

  it("session whose layer was deleted: returns true, no commit (nothing to undo to)", async () => {
    const { commitLayerTransformSession } = await import(
      "../components/editor/transformSession"
    );

    const { engine, layer } = setupDoc();
    const history = { commit: vi.fn() };
    const session = {
      documentId: engine.getId(),
      layerId: layer.id,
      originalSnapshot: engine.snapshot(),
      originalTransform: { ...layer.transform },
      mode: "resize" as const,
      lockRatio: false,
      startedAt: Date.now(),
    };

    // Add a second layer first, then delete the original — deleteLayer
    // refuses to leave zero layers.
    engine.addLayer("Layer 2");
    engine.deleteLayer(layer.id);

    const ok = commitLayerTransformSession(session, engine, history);
    expect(ok).toBe(true);
    expect(history.commit).not.toHaveBeenCalled();
  });
});

// ─── Snapshot independence: mutation after snapshot doesn't change it ───

describe("history audit — snapshot/restore independence", () => {
  it("mutating engine after snapshot() does NOT change the snapshot", () => {
    const { engine, layer, history } = setupDoc();

    history.commit(engine.snapshot());
    const originalX = layer.transform.x;
    engine.moveLayer(layer.id, 999, 999);

    // Undo should restore the original x (snapshot was independent).
    const prev = history.undo(engine.snapshot())!;
    engine.restore(prev);
    expect(engine.getLayer(layer.id)!.transform.x).toBe(originalX);
  });

  it("mutating the SAME layer reference after snapshot doesn't poison history", () => {
    const { engine, layer, history } = setupDoc();

    history.commit(engine.snapshot());
    // Aggressive: also mutate the original layer object reference.
    (layer.transform as unknown as { x: number; [key: string]: unknown }).x = 12345;
    engine.moveLayer(layer.id, 999, 999);

    const prev = history.undo(engine.snapshot())!;
    engine.restore(prev);
    // The snapshot captured x=0 (initial), not the mid-test mutation.
    expect(engine.getLayer(layer.id)!.transform.x).toBe(0);
  });

  it("100 ops fit in default history (50 evicted) — undo bottoms out at evicted boundary", () => {
    const { engine, layer, history } = setupDoc();

    // Direct commits with predictable positions: x = i+1 after op #i.
    for (let i = 0; i < 100; i++) {
      history.commit(engine.snapshot());
      engine.moveLayer(layer.id, i + 1, 0);
    }

    // Default MAX_HISTORY_DEPTH is 50 — exactly 50 retained after 100 ops.
    expect(history.getUndoCount()).toBe(50);

    // Oldest 50 entries evicted. Undoing all 50 lands the layer at the
    // state captured before op #51, i.e. x=50 (op #50's result).
    while (history.canUndo()) {
      engine.restore(history.undo(engine.snapshot())!);
    }
    expect(engine.getLayer(layer.id)!.transform.x).toBe(50);

    // Critical sanity: layer is NOT at original (x=0). Initial state was
    // evicted — this is the documented MAX_HISTORY_DEPTH trade-off.
    expect(engine.getLayer(layer.id)!.transform.x).not.toBe(0);
  });

  it("staying under MAX_HISTORY_DEPTH preserves initial state for undo-all", () => {
    const { engine, layer, history } = setupDoc();

    // 49 ops — within the default 50 cap, so the initial state survives.
    for (let i = 0; i < 49; i++) {
      history.commit(engine.snapshot());
      engine.moveLayer(layer.id, (i + 1) * 5, 0);
    }
    expect(history.getUndoCount()).toBe(49);

    while (history.canUndo()) {
      engine.restore(history.undo(engine.snapshot())!);
    }
    // 49 < 50 → initial S0 still on the stack → undo back to initial.
    expect(engine.getLayer(layer.id)!.transform.x).toBe(0);
  });
});
