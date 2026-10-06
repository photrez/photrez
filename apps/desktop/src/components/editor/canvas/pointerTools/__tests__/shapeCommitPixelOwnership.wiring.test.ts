// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * A shape layer's raster must stay OUT of the Rust pixel store.
 *
 * WHY. A shape layer is PARAMETRIC: `layer.shapeParams` is the document state and
 * `layer.imageBitmap` is a cache re-derived from it by `renderShapeToBitmap` on
 * every edit (`DocumentEngine.updateShapeParams`, engine/document.ts). Nothing
 * accumulates pixels into a shape layer while it is a shape - the canvas blocks
 * brush, eraser, paint bucket and gradient on one and offers "Convert to pixels"
 * instead (canvas/useCanvasPointerTools.ts). So a shape commit is a LAYER addition,
 * not a pixel edit, and giving it a Rust pixel history entry would assert a step
 * the gesture never performed.
 *
 * WHAT WOULD BREAK IF IT DID. `stepRustCursor` (engine/history.ts) routes an entry
 * whose `imperative.rustOwned === true` through `fireGatedOnPixelTip`, so the pop
 * issues `rust_pixels_undo` for it. That is one shared per-document cursor: a
 * shape pop would spend a step belonging to the neighbouring paint stroke and the
 * stroke's own Ctrl+Z would revert nothing. The cases below pin the shape commit's
 * entry as a plain metadata commit, which is what keeps that cursor honest.
 *
 * THE THREE ARMS, driven the way a user drives them: pointerdown -> pointermove xN
 * -> pointerup through `useCanvasPointerTools`, plus a real `CommandHistory` whose
 * undo/redo pops are asserted against a transport-faithful pixel store.
 */
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { pixelRegionDispatch, pixelSeedDispatch } from "@/lib/protocol/pixelSeedCall";
import { invoke } from "@tauri-apps/api/core";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { CommandHistory } from "@/engine/history";
import { createRustStoreEmulator, type RustStoreEmulator } from "@/lib/paint/__tests__/rustStoreEmulator";
import { toIpcBytes } from "@/lib/paint/storeCurrency";
import {
  createMockEditorParams,
  createPointerTools,
  makePointerEvent,
} from "@/__tests__/pointerRoutingHarness";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args: Record<string, unknown>) => Promise<unknown>>;

// The pointer hook asks the dialog provider for the shape/text conversion
// confirm; nothing here reaches it, so a no-op stand-in is enough.
vi.mock("../../../dialogs/DialogProvider", () => ({
  useDialog: () => ({ confirm: vi.fn().mockResolvedValue(false) }),
}));

/** Every command the gesture issued, in order. */
function issued(): string[] {
  return invokeMock.mock.calls.map((c) => c[0]);
}

/** Commands that can put bytes into (or move the cursor over) the Rust pixel store. */
function pixelStoreCommands(): string[] {
  return issued().filter((c) => c.startsWith("rust_pixels_"));
}

/**
 * Drain the microtask queue so a DEFERRED writer is observable. A pixel write
 * that is fired without awaiting resolves on a microtask, so an assertion made
 * synchronously after the gesture can pass against a write that really happened.
 */
async function flushMicrotasks(rounds = 12): Promise<void> {
  for (let i = 0; i < rounds; i++) await Promise.resolve();
}

function makePointerTools(signals: Record<string, unknown>) {
  mockUseEditor(signals);
  return createPointerTools({
    getCanvasContainerRef: () => document.createElement("div"),
    getCanvasRef: () => document.createElement("canvas"),
    isSpacePressed: () => false,
    isPanning: () => false,
    isAltPressed: () => false,
    stopMomentum: vi.fn(),
    fitToScreenAndRender: vi.fn(),
    commitBrushStroke: vi.fn(),
  });
}

/**
 * One complete shape drag through the real dispatch chain. `moves` pointermove
 * events are delivered between press and release, so a per-move writer would show
 * up as one command per move.
 */
async function dragShape(
  signals: Record<string, unknown>,
  disposeEngine: () => void,
  moves = 4,
): Promise<void> {
  const { tools, dispose: disposeTools } = makePointerTools(signals);
  try {
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));
    for (let i = 1; i <= moves; i++) {
      const at = 10 + i * 25;
      tools.onCanvasPointerMove(makePointerEvent({ clientX: at, clientY: 60 }));
    }
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 110, clientY: 60 }));
    await flushMicrotasks();
  } finally {
    disposeTools();
    disposeEngine();
  }
}

describe("a shape drag never touches the Rust pixel store", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    localStorage.clear();
    // Shipping defaults, stated explicitly because `isFacadeEnabled` reads
    // `!== "0"`: an ABSENT key means the facade is ON, so a case named for the
    // no-mirror configuration must set it, not clear it.
    localStorage.setItem("photrez.facade", "0");
  });

  afterEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it("issues ZERO pixel-store commands across press, every move, and release", async () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    (mockEngine as any).addShapeLayer = vi.fn(() => ({
      id: "shape-1", type: "shape", name: "Shape 1", width: 40, height: 24, imageBitmap: {},
    }));
    const history = signals.workspace.getActiveHistory();

    await dragShape(signals as Record<string, unknown>, dispose, 6);

    // Premise: the gesture really was a committed shape, not a swallowed click.
    expect(history.commit, "the shape gesture committed").toHaveBeenCalledTimes(1);
    expect(history.commit).toHaveBeenCalledWith(expect.anything(), "Add Shape");
    expect(
      pixelStoreCommands(),
      "a parametric layer's raster is re-derived from shapeParams, so the gesture must not open a Rust pixel entry for it",
    ).toEqual([]);
  });

  it("the live drag preview writes nothing and records no history entry", async () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    (mockEngine as any).addShapeLayer = vi.fn(() => ({
      id: "shape-1", type: "shape", name: "Shape 1", width: 40, height: 24, imageBitmap: {},
    }));
    const history = signals.workspace.getActiveHistory();
    const { tools, dispose: disposeTools } = makePointerTools(signals as Record<string, unknown>);
    try {
      tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));
      for (let i = 1; i <= 8; i++) {
        tools.onCanvasPointerMove(makePointerEvent({ clientX: 10 + i * 12, clientY: 40 + i * 3 }));
        // Flushed BEFORE the assertions, because the only shape of writer this
        // test must catch is an awaited one: a deferred `pixelInvoke` resolves on
        // a microtask, so asserting synchronously would pass against a preview
        // that really does write, one tick later.
        await flushMicrotasks();
        // Per-move, so a writer that fires on only some moves is still caught
        // rather than hidden by a total at the end of the drag.
        expect(pixelStoreCommands(), `preview move ${i} wrote to the Rust pixel store`).toEqual([]);
        expect(history.commit, `preview move ${i} recorded a history entry`).not.toHaveBeenCalled();
      }
    } finally {
      disposeTools();
      dispose();
    }
  });

  it("the committed entry carries no imperative payload, so its pop cannot take the pixel-writer arm", async () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("shape");
    (mockEngine as any).addShapeLayer = vi.fn(() => ({
      id: "shape-1", type: "shape", name: "Shape 1", width: 40, height: 24, imageBitmap: {},
    }));
    const history = signals.workspace.getActiveHistory();

    await dragShape(signals as Record<string, unknown>, dispose);

    const call = history.commit.mock.calls.at(-1)!;
    // `stepRustCursor` reads `entry.imperative?.rustOwned === true` and
    // `entry.imperative !== undefined`. Either one being truthy would route the
    // shape's pop through `fireGatedOnPixelTip` and spend a Rust cursor step the
    // gesture never earned, so both are pinned absent here.
    expect(
      { imperative: call[2], alreadyRecordedInRust: call[3] },
      "a shape commit is a layer addition: no tile memento and no claim that Rust already recorded a pixel step",
    ).toEqual({ imperative: undefined, alreadyRecordedInRust: undefined });
  });
});

describe("a shape commit between two paint strokes leaves both neighbours undoable", () => {
  const DOC = "shapeCommitOwnership";
  const SIZE = 8;
  const LAYER = "layer-paint";
  let store: RustStoreEmulator;

  const solid = (r: number, g: number, b: number): Uint8ClampedArray => {
    const buf = new Uint8ClampedArray(SIZE * SIZE * 4);
    for (let i = 0; i < buf.length; i += 4) {
      buf[i] = r; buf[i + 1] = g; buf[i + 2] = b; buf[i + 3] = 255;
    }
    return buf;
  };
  const opaque = solid(0, 0, 0);

  /** One painted step on the layer, exactly as the Rust pixel store records one. */
  async function paintStroke(bytes: Uint8ClampedArray): Promise<void> {
    await store.invoke(
      "rust_pixels_write_region",
      pixelRegionDispatch(DOC, LAYER, 0, 0, SIZE, SIZE, toIpcBytes(bytes)),
    );
  }

  function snapshot() {
    return { id: DOC, layers: [], activeLayerId: LAYER, width: SIZE, height: SIZE } as never;
  }

  /** A real `rustOwned` pixel twin, the shape the brush/fill/bucket commits use. */
  function twin(before: Uint8ClampedArray, after: Uint8ClampedArray) {
    const tile = (px: Uint8ClampedArray) => ({ x: 0, y: 0, width: SIZE, height: SIZE, data: px });
    return { layerId: LAYER, surfaceWidth: SIZE, surfaceHeight: SIZE, before: [tile(before)], after: [tile(after)], rustOwned: true };
  }

  beforeEach(async () => {
    localStorage.clear();
    // Both recorders off, which is the configuration where a cursor step is
    // attributable to exactly one host entry: the history bridge needs the Tauri
    // runtime as well as its key, and the facade shim mirrors metadata commits.
    localStorage.setItem("photrez.facade", "0");
    invokeMock.mockReset();
    store = createRustStoreEmulator();
    invokeMock.mockImplementation(((cmd: string, args: Record<string, unknown>, options?: any) =>
      store.invoke(cmd, args, options)) as never);
    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = undefined;
    // Seed the layer's canonical entry from real bytes so the strokes below move
    // a live store rather than a stub.
    await store.invoke("rust_pixels_init", pixelSeedDispatch(DOC, LAYER, SIZE, SIZE, toIpcBytes(opaque)));
  });

  afterEach(() => {
    store.dispose();
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it("undoing the shape spends no cursor step; both paint strokes still undo in order", async () => {
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC);

    const first = solid(255, 0, 0);
    const second = solid(0, 255, 0);

    // Paint, shape, paint - the ordering the shape tool produces on a canvas the
    // user is also painting on: one stroke, the shape between two strokes.
    await paintStroke(first);
    history.commit(snapshot(), "Brush Stroke", twin(opaque, first), true);
    history.commit(snapshot(), "Add Shape");
    await paintStroke(second);
    history.commit(snapshot(), "Brush Stroke", twin(first, second), true);

    expect(history.getUndoCount(), "premise: three host entries").toBe(3);

    // Pop 1 - newest paint stroke.
    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(issued().filter((c) => c === "rust_pixels_undo"), "pop 1 stepped the cursor once").toHaveLength(1);
    expect(store.pixelAt(LAYER, 0, 0), "pop 1 reverted the newest stroke back to the first").toEqual([255, 0, 0, 255]);

    // Pop 2 - the SHAPE commit. This is the press the mixed-producer shape loses:
    // it must not move the shared cursor, or the stroke below it becomes
    // unreachable.
    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(
      issued().filter((c) => c === "rust_pixels_undo"),
      "the shape's pop issued no cursor step, so no neighbour's entry was consumed",
    ).toHaveLength(1);
    expect(store.pixelAt(LAYER, 0, 0), "the shape's pop changed no pixels").toEqual([255, 0, 0, 255]);

    // Pop 3 - the first paint stroke, still reachable.
    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(issued().filter((c) => c === "rust_pixels_undo"), "pop 3 stepped the cursor").toHaveLength(2);
    expect(store.pixelAt(LAYER, 0, 0), "pop 3 reverted the first stroke").toEqual([0, 0, 0, 255]);
  });

  it("the shape commit records nothing into the Rust stream either", () => {
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC);
    invokeMock.mockClear();

    history.commit(snapshot(), "Add Shape");

    expect(
      issued(),
      "a layer addition reaches Rust through the layer graph, not by appending a pixel or external history entry",
    ).toEqual([]);
  });

  // THE DEFEAT, kept in the file so the case above cannot rot into a claim the
  // harness happens to satisfy. This commits the shape the way an ARMED producer
  // would - `rustOwned: true`, so the pop takes `fireGatedOnPixelTip` - and asserts
  // the neighbour's stroke is what gets eaten. If this ever stops reproducing the
  // loss, the guard above is measuring nothing and this file says so.
  it("DEFEAT: a rustOwned shape twin DOES spend the neighbour stroke's cursor step", async () => {
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC);

    const first = solid(255, 0, 0);
    const second = solid(0, 255, 0);

    await paintStroke(first);
    history.commit(snapshot(), "Brush Stroke", twin(opaque, first), true);
    // The armed shape commit: identical label and layer graph work, but it claims
    // a Rust pixel step it never performed.
    history.commit(
      snapshot(),
      "Add Shape",
      { layerId: LAYER, surfaceWidth: SIZE, surfaceHeight: SIZE, before: [], after: [], rustOwned: true },
      true,
    );
    await paintStroke(second);
    history.commit(snapshot(), "Brush Stroke", twin(first, second), true);

    history.undo(snapshot());
    await history.takeLastCursorStep();
    invokeMock.mockClear();

    // The shape's pop. `rustOwned` sends it down the pixel-writer arm, so it
    // issues a step the gesture never earned - against the stroke beneath it.
    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(
      issued().filter((c) => c === "rust_pixels_undo"),
      "the armed shape twin consumed a cursor step",
    ).toHaveLength(1);
    // The pixels that step moved were the STROKE's: the armed shape's pop reverted
    // the stroke underneath it in one press. Compare the unarmed case above, where
    // the shape's pop left these pixels at red.
    expect(
      store.pixelAt(LAYER, 0, 0),
      "the armed shape's pop reverted the stroke beneath it, not any shape pixel",
    ).toEqual([0, 0, 0, 255]);

    // The stroke that lost its step: its own pop still issues a step, but there is
    // no entry left behind it, so nothing is reverted a second time. Two presses
    // produced one stroke's worth of change. That is the data loss this file
    // exists to prevent.
    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(
      issued().filter((c) => c === "rust_pixels_undo"),
      "the stroke beneath the armed shape still issues a step",
    ).toHaveLength(2);
    expect(
      store.pixelAt(LAYER, 0, 0),
      "but that step had no entry left to undo, so the pixels are unchanged",
    ).toEqual([0, 0, 0, 255]);
  });
});