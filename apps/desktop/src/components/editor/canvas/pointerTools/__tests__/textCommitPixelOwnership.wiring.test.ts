// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * A text layer's raster must stay OUT of the Rust pixel store.
 *
 * WHY. A text layer is PARAMETRIC in exactly the way a shape layer is:
 * `layer.textData` is the document state and `layer.imageBitmap` is a cache
 * re-derived from it by `rasterizeText` on every edit
 * (`DocumentEngine.updateTextData`, engine/document.ts:936-949, which calls
 * `rasterizeText(normalized)` and hands the result to `replaceLayerBitmap`).
 * Nothing accumulates pixels into a text layer while it is text - the canvas
 * intercepts brush/eraser/bucket/gradient on a text layer and offers
 * "Convert Text to Pixels" instead (canvas/useCanvasPointerTools.ts:579-621),
 * and the conversion is an explicit type-flip, never a silent bake.
 *
 * WHAT WOULD BREAK IF IT DID. `stepRustCursor` (engine/history.ts) routes an
 * entry whose `imperative.rustOwned === true` through `fireGatedOnPixelTip`, so
 * the pop issues `rust_pixels_undo` for it. That is one shared per-document
 * cursor: a text pop would spend a step belonging to the neighbouring paint
 * stroke and the stroke's own Ctrl+Z would revert nothing. The cases below pin
 * the text commit's entry as a plain metadata commit, which is what keeps that
 * cursor honest.
 *
 * THE THREE ARMS, driven the way a user drives them: pointerdown -> pointermove
 * xN -> pointerup through `useCanvasPointerTools`, then the click-away that
 * commits the session, plus a real `CommandHistory` whose undo/redo pops are
 * asserted against a transport-faithful pixel store.
 */
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { CommandHistory } from "@/engine/history";
import { createRustStoreEmulator, type RustStoreEmulator } from "@/lib/paint/__tests__/rustStoreEmulator";
import { toIpcBytes } from "@/lib/paint/storeCurrency";
import { DEFAULT_TEXT_DATA, type TextData } from "@/engine/textTypes";
import { setPendingTextFlush } from "@/components/editor/canvas/pointerTools/textTool";
import { showToast } from "../../../Toast";
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

// Toast is how `startTextPointer` reports a failed create (textTool.ts:340-344).
// Captured rather than stubbed silently: a swallowed create would otherwise make
// this file measure nothing at all.
vi.mock("../../../Toast", () => ({ showToast: vi.fn() }));

/** Every command the gesture issued, in order. */
function issued(): string[] {
  return invokeMock.mock.calls.map((c) => c[0]);
}

/** Commands that can put bytes into (or move the cursor over) the Rust pixel store. */
function pixelStoreCommands(): string[] {
  return issued().filter((c) => c.startsWith("rust_pixels_"));
}

/**
 * Drain the async queue so a DEFERRED writer is observable.
 *
 * Both kinds of turn are needed, and the macrotask one is not optional. Every
 * armed pixel writer in this codebase reaches the transport through
 * `await import("@tauri-apps/api/core")` first (see
 * `lib/paint/compositeCanonical.ts:110`), and a dynamic import is a real module
 * load: it settles on a macrotask, not on a microtask. Draining microtasks alone
 * leaves that writer mid-flight, so an assertion made "after the gesture" would
 * pass against a write that really happens - which is exactly the vacuous guard
 * this file must not be. Verified: with microtasks only, an armed `Add Text`
 * commit passes this file green.
 */
async function flushAsync(rounds = 4): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    for (let j = 0; j < 12; j++) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

const LAYER_ID = "text-1";

/**
 * The text layer as the real engine holds it: a mutable node whose `textData`
 * is the document state. `imageBitmap` is replaced on every edit, exactly as
 * `updateTextData` does, so a test that reuses the same object still sees the
 * cache change under it.
 */
function makeTextLayer(): Record<string, unknown> {
  return {
    id: LAYER_ID,
    type: "text",
    name: "Text",
    width: 40,
    height: 24,
    visible: true,
    locked: false,
    opacity: 1,
    blendMode: "normal",
    imageBitmap: { width: 40, height: 24, close: () => {} },
    textData: { ...DEFAULT_TEXT_DATA } as TextData,
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
  };
}

/**
 * `createMockEditorParams` ships a layer with no raster and no `textData`,
 * which is right for pointer-ROUTING tests and wrong here: `commitTextSession`
 * reads `layerTextData(layer)` and deletes the layer outright when the content
 * is empty, so a layer without textData would measure the empty-commit cleanup
 * instead of an "Add Text" commit. These overrides give the mock the two
 * production behaviours the commit path depends on, and nothing else.
 *
 * `preExisting` decides which of the two commit sites the gesture reaches. With
 * no layers on the canvas, `startTextPointer` finds no hit and takes the CREATE
 * branch ("Add Text", textTool.ts:463). With the layer already on the canvas,
 * the same pointer lands inside its box and takes the RE-EDIT branch
 * ("Edit Text", textTool.ts:496) - a different `history.commit` call with the
 * same ownership claim, so both are measured.
 */
function installTextEngine(mockEngine: unknown, preExisting = false): Record<string, unknown> {
  const layer = makeTextLayer();
  let deleted = false;
  let created = preExisting;
  const engine = mockEngine as Record<string, unknown>;
  engine.getLayer = (id: string) => (id === LAYER_ID && !deleted ? layer : null);
  engine.getLayers = () => (created && !deleted ? [layer] : []);
  engine.addTextLayer = () => { created = true; return layer; };
  engine.deleteLayer = () => { deleted = true; };
  // The re-derivation under measurement: params in, a FRESH raster out. If any
  // arm were added here, this is the seam it would have to write through, and
  // the assertion below counts the store commands it issues.
  engine.updateTextData = (id: string, data: TextData) => {
    if (id !== LAYER_ID) return;
    layer.textData = data;
    layer.imageBitmap = { width: 40, height: 24, close: () => {} };
  };
  engine.getLayerImageBitmap = () => layer.imageBitmap;
  return layer;
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
 * Type into the open session the way the edit overlay does: register the
 * production flush callback (`setPendingTextFlush`) rather than calling
 * `updateTextData` from the test. `commitTextSession` invokes it via
 * `flushPendingText` (textTool.ts:436), so the commit sees the typed content
 * through the same seam production uses - and an empty session would still hit
 * the empty-commit cleanup, which is what makes this a measurement of a real
 * "Add Text" rather than of a deleted layer.
 */
function typeIntoSession(content: string): void {
  setPendingTextFlush(LAYER_ID, (engine) => {
    const seam = engine as { updateTextData?: (id: string, data: TextData) => void };
    seam.updateTextData?.(LAYER_ID, {
      ...DEFAULT_TEXT_DATA,
      content,
      fontSize: 24,
      color: "#112233",
    });
  });
}

/**
 * One complete text gesture through the real dispatch chain: press, `moves`
 * drags, release, then the click-away that closes the session and commits it.
 * The click-away lands far outside the session's box, which is the branch
 * `startTextPointer` takes to finish an edit rather than keep it open
 * (textTool.ts:279-284).
 */
async function typeAndCommit(
  signals: Record<string, unknown>,
  disposeEngine: () => void,
  moves = 4,
): Promise<void> {
  const { tools, dispose: disposeTools } = makePointerTools(signals);
  try {
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));
    for (let i = 1; i <= moves; i++) {
      tools.onCanvasPointerMove(makePointerEvent({ clientX: 10 + i * 20, clientY: 60 }));
    }
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 10 + moves * 20, clientY: 60 }));
    await flushAsync();

    // The user types, then clicks empty canvas to finish.
    typeIntoSession("measure me");
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 400, clientY: 400 }));
    await flushAsync();
  } finally {
    disposeTools();
    disposeEngine();
  }
}

/**
 * Premise guard. `startTextPointer` swallows a failed layer creation into a
 * toast (textTool.ts:340-344), so a create that threw would leave no session, no
 * history entry and no pixel-store command - and every assertion below would
 * pass while measuring nothing.
 */
function expectNoCreateFailure(): void {
  const failures = (showToast as unknown as Mock).mock.calls.filter(
    (c) => typeof c[0] === "string" && c[0].startsWith("Text failed"),
  );
  expect(failures, "the text layer was really created, not swallowed by the error toast").toEqual([]);
}

describe("a text gesture never touches the Rust pixel store", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    (showToast as unknown as Mock).mockClear();
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

  it("issues ZERO pixel-store commands across press, every move, release and the commit", async () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    installTextEngine(mockEngine, false);
    const history = signals.workspace.getActiveHistory();

    await typeAndCommit(signals as Record<string, unknown>, dispose, 6);

    // Premises: the layer was really created, and the gesture really was a
    // committed text layer rather than the empty-commit cleanup (which would
    // delete the layer and record nothing).
    expectNoCreateFailure();
    expect(history.commit, "the text gesture committed").toHaveBeenCalledTimes(1);
    expect(history.commit).toHaveBeenCalledWith(expect.anything(), "Add Text");
    expect(
      pixelStoreCommands(),
      "a parametric text layer's raster is re-derived from textData, so the gesture must not open a Rust pixel entry for it",
    ).toEqual([]);
  });

  it("re-editing an existing text layer opens no pixel-store entry either", async () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    installTextEngine(mockEngine, true);
    const history = signals.workspace.getActiveHistory();

    await typeAndCommit(signals as Record<string, unknown>, dispose, 4);

    // The OTHER commit site: the same raster, reached through the hit-test
    // re-edit branch ("Edit Text", textTool.ts:496) instead of the create branch
    // above ("Add Text", :463). Both record a plain metadata commit, and both
    // must leave the store alone - otherwise text would have two owners
    // depending on how the user opened the layer.
    expectNoCreateFailure();
    expect(history.commit, "the re-edit committed").toHaveBeenCalledTimes(1);
    expect(history.commit).toHaveBeenCalledWith(expect.anything(), "Edit Text");
    expect(
      pixelStoreCommands(),
      "a re-edit re-rasterises from textData just like the create does",
    ).toEqual([]);
  });

  it("the live drag preview writes nothing and records no history entry", async () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    installTextEngine(mockEngine);
    const history = signals.workspace.getActiveHistory();
    const { tools, dispose: disposeTools } = makePointerTools(signals as Record<string, unknown>);
    try {
      tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));
      for (let i = 1; i <= 8; i++) {
        tools.onCanvasPointerMove(makePointerEvent({ clientX: 10 + i * 12, clientY: 40 + i * 3 }));
        // Flushed BEFORE the assertions, because an armed writer is deferred -
        // see `flushAsync`. Asserting synchronously would pass against a preview
        // that really does write, one tick later.
        await flushAsync();
        // Per-move, so a writer that fires on only some moves is still caught
        // rather than hidden by a total at the end of the drag.
        expect(pixelStoreCommands(), `preview move ${i} wrote to the Rust pixel store`).toEqual([]);
        expect(history.commit, `preview move ${i} recorded a history entry`).not.toHaveBeenCalled();
      }
      expectNoCreateFailure();
    } finally {
      disposeTools();
      dispose();
    }
  });

  it("the committed entry carries no imperative payload, so its pop cannot take the pixel-writer arm", async () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    installTextEngine(mockEngine);
    const history = signals.workspace.getActiveHistory();

    await typeAndCommit(signals as Record<string, unknown>, dispose);

    expectNoCreateFailure();
    const call = history.commit.mock.calls.at(-1)!;
    // `stepRustCursor` reads `entry.imperative?.rustOwned === true` and
    // `entry.imperative !== undefined`. Either one being truthy would route the
    // text commit's pop through `fireGatedOnPixelTip` and spend a Rust cursor
    // step the gesture never earned, so both are pinned absent here.
    expect(
      { imperative: call[2], alreadyRecordedInRust: call[3] },
      "a text commit is a layer addition: no tile memento and no claim that Rust already recorded a pixel step",
    ).toEqual({ imperative: undefined, alreadyRecordedInRust: undefined });
  });
});

describe("a text commit between two paint strokes leaves both neighbours undoable", () => {
  const DOC = "textCommitOwnership";
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
    await store.invoke("rust_pixels_write_region", {
      docId: DOC, layerId: LAYER, x: 0, y: 0, w: SIZE, h: SIZE,
      rgba: toIpcBytes(bytes),
    });
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
    invokeMock.mockImplementation(((cmd: string, args: Record<string, unknown>) =>
      store.invoke(cmd, args)) as never);
    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = undefined;
    // Seed the layer's canonical entry from real bytes so the strokes below move
    // a live store rather than a stub.
    await store.invoke("rust_pixels_init", {
      docId: DOC, layerId: LAYER, width: SIZE, height: SIZE, bytes: toIpcBytes(opaque),
    });
  });

  afterEach(() => {
    store.dispose();
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it("undoing the text spends no cursor step; both paint strokes still undo in order", async () => {
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC);

    const first = solid(255, 0, 0);
    const second = solid(0, 255, 0);

    // Paint, text, paint - the ordering the text tool produces on a canvas the
    // user is also painting on: one stroke, the text between two strokes.
    await paintStroke(first);
    history.commit(snapshot(), "Brush Stroke", twin(opaque, first), true);
    history.commit(snapshot(), "Add Text");
    await paintStroke(second);
    history.commit(snapshot(), "Brush Stroke", twin(first, second), true);

    expect(history.getUndoCount(), "premise: three host entries").toBe(3);

    // Pop 1 - newest paint stroke.
    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(issued().filter((c) => c === "rust_pixels_undo"), "pop 1 stepped the cursor once").toHaveLength(1);
    expect(store.pixelAt(LAYER, 0, 0), "pop 1 reverted the newest stroke back to the first").toEqual([255, 0, 0, 255]);

    // Pop 2 - the TEXT commit. This is the press the mixed-producer text loses:
    // it must not move the shared cursor, or the stroke below it becomes
    // unreachable.
    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(
      issued().filter((c) => c === "rust_pixels_undo"),
      "the text's pop issued no cursor step, so no neighbour's entry was consumed",
    ).toHaveLength(1);
    expect(store.pixelAt(LAYER, 0, 0), "the text's pop changed no pixels").toEqual([255, 0, 0, 255]);

    // Pop 3 - the first paint stroke, still reachable.
    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(issued().filter((c) => c === "rust_pixels_undo"), "pop 3 stepped the cursor").toHaveLength(2);
    expect(store.pixelAt(LAYER, 0, 0), "pop 3 reverted the first stroke").toEqual([0, 0, 0, 255]);
  });

  it("the text commit records nothing into the Rust stream either", () => {
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC);
    invokeMock.mockClear();

    history.commit(snapshot(), "Add Text");

    expect(
      issued(),
      "a layer addition reaches Rust through the layer graph, not by appending a pixel or external history entry",
    ).toEqual([]);
  });

  // THE DEFEAT, kept in the file so the case above cannot rot into a claim the
  // harness happens to satisfy. This commits the text the way an ARMED producer
  // would - `rustOwned: true`, so the pop takes `fireGatedOnPixelTip` - and
  // asserts the neighbour's stroke is what gets eaten. If this ever stops
  // reproducing the loss, the guard above is measuring nothing and this file
  // says so.
  it("DEFEAT: a rustOwned text twin DOES spend the neighbour stroke's cursor step", async () => {
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC);

    const first = solid(255, 0, 0);
    const second = solid(0, 255, 0);

    await paintStroke(first);
    history.commit(snapshot(), "Brush Stroke", twin(opaque, first), true);
    // The armed text commit: identical label and layer graph work, but it claims
    // a Rust pixel step it never performed.
    history.commit(
      snapshot(),
      "Add Text",
      { layerId: LAYER, surfaceWidth: SIZE, surfaceHeight: SIZE, before: [], after: [], rustOwned: true },
      true,
    );
    await paintStroke(second);
    history.commit(snapshot(), "Brush Stroke", twin(first, second), true);

    history.undo(snapshot());
    await history.takeLastCursorStep();
    invokeMock.mockClear();

    // The text's pop. `rustOwned` sends it down the pixel-writer arm, so it
    // issues a step the gesture never earned - against the stroke beneath it.
    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(
      issued().filter((c) => c === "rust_pixels_undo"),
      "the armed text twin consumed a cursor step",
    ).toHaveLength(1);
    // The pixels that step moved were the STROKE's: the armed text's pop reverted
    // the stroke underneath it in one press. Compare the unarmed case above,
    // where the text's pop left these pixels at red.
    expect(
      store.pixelAt(LAYER, 0, 0),
      "the armed text's pop reverted the stroke beneath it, not any text pixel",
    ).toEqual([0, 0, 0, 255]);

    // The stroke that lost its step: its own pop still issues a step, but there
    // is no entry left behind it, so nothing is reverted a second time. Two
    // presses produced one stroke's worth of change. That is the data loss this
    // file exists to prevent.
    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(
      issued().filter((c) => c === "rust_pixels_undo"),
      "the stroke beneath the armed text still issues a step",
    ).toHaveLength(2);
    expect(
      store.pixelAt(LAYER, 0, 0),
      "but that step had no entry left to undo, so the pixels are unchanged",
    ).toEqual([0, 0, 0, 255]);
  });
});
