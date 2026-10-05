// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * A stamped layer's raster was a GENUINE TypeScript pixel owner, and is now
 * seeded into the Rust store like every other composite destination.
 *
 * WHY IT IS NOT LIKE TEXT OR SHAPE. A shape or text layer is PARAMETRIC:
 * `shapeParams` / `textData` is the document state and `imageBitmap` is a cache
 * re-derived from it on every edit, so nothing accumulates pixels into the layer
 * and the raster can always be recomputed. A stamped layer is the opposite. It
 * is minted by `engine.addLayer` (document.ts:494) as a plain raster layer, has
 * NO params to re-derive from, and its content is a CPU-canvas composite of
 * every visible layer - `compositeAllLayers` (engine/layerComposite.ts:60-81)
 * reads each source layer's `imageBitmap` through `drawLayerToContext` and
 * `transferToImageBitmap`s the result, which `stampVisibleLayers` installs with
 * `setLayerImageBitmap` (document.ts:1410). Neither of those two engine methods
 * touches the Rust pixel store, so before the arm the composite existed ONLY in
 * TypeScript.
 *
 * WHY IT WAS A GAP RATHER THAN A DESIGN CHOICE. `seedCompositeCanonicalPixels`
 * (lib/paint/compositeCanonical.ts) exists to seed a composite destination, and
 * its header named exactly three ops - merge down, merge selected, flatten.
 * `stampVisibleLayers` produced the same kind of destination and was not among
 * them: the other three call `seedCompositeCanonicalPixels` and it did not, which
 * is why its convergence case read `RUST-HAS-NO-ENTRY` while the sibling MERGE
 * DOWN case read `CONVERGES` - the same assertion, two answers, because one op
 * was seeded and the other was not. It now calls the same helper, so both
 * convergence cases read `CONVERGES`.
 *
 * WHAT THE ARM IS. `seedCompositeCanonicalPixels` exactly as `flattenAllLayers`
 * uses it - the same three steps, including the third
 * (`syncLayerStoreToLayerRaster`) that re-asserts the store from the raster and
 * so DROPS the `Pixel` entry `write_region` just opened. That third step is the
 * cursor-bookkeeping answer: the host entry is a structural "Stamp Visible"
 * snapshot commit, and an undo press must step THAT entry, not a pixel entry the
 * gesture never earned. The host entry carries no `imperative` and no
 * `rustOwned`, for the same reason - and because a `rustOwned` entry routes its
 * pop through `fireGatedOnPixelTip`, spending a neighbouring stroke's step.
 *
 * The cases drive the real dispatch chain - a Ctrl+Shift+Alt+E keydown through
 * `handleLayerOpsKey` (canvas/keyboardShortcuts/layerOps.ts:108-119), which is
 * the shortcut the app binds - and read the store through the store's own
 * command, so "no entry" and "an entry that matches" cannot collapse.
 */
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { handleLayerOpsKey } from "@/components/editor/canvas/keyboardShortcuts/layerOps";
import { CommandHistory } from "@/engine/history";
import { createRustStoreEmulator, type RustStoreEmulator } from "@/lib/paint/__tests__/rustStoreEmulator";
import {
  CompositeCanvasStub,
  installCompositeCanvas,
  readStubProjection,
  stubGradientRaster as gradientRaster,
  stubRasterBitmap as rasterBitmap,
} from "@/lib/paint/__tests__/compositeCanvasStub";
import { toIpcBytes } from "@/lib/paint/storeCurrency";
import { installFacadeCommitShim } from "@/lib/protocol/facadeRegistry";
import { answerInvoke, record, resetStreams, streamFor } from "@/engine/__tests__/rustStreamEmulator";
import { settle as settleCursor, timesInvoked, waitForRust } from "@/engine/__tests__/historyCursorHarness";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args: Record<string, unknown>) => Promise<unknown>>;

const DOC = "docStamp";
const SIZE = 8;

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
 * The macrotask turn is not optional. The arm under test is
 * `void seedCompositeCanonicalPixels(...)` (the same fire-and-forget the merge
 * ops use), and its first statement is `await import("@tauri-apps/api/core")` -
 * a real module load that settles on a macrotask. Asserting synchronously after
 * the keydown would therefore pass against a seed that really happens, which is
 * the vacuous guard this file must not be. Verified: with a synchronous
 * assertion, arming `stampVisibleLayers` leaves this file green.
 */
async function flushAsync(rounds = 4): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    for (let j = 0; j < 12; j++) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

interface FakeLayer {
  id: string;
  name: string;
  type: string;
  visible: boolean;
  width: number;
  height: number;
  imageBitmap: ImageBitmap | null;
  transform: { x: number; y: number; scaleX: number; scaleY: number; rotation: number; flipH: boolean; flipV: boolean };
  opacity: number;
  blendMode: string;
  locked: boolean;
  isBackground: boolean;
}

/**
 * A document double for the two engine methods the stamp path uses, mirroring
 * the REAL ones on the two points that decide the verdict:
 *   - `addLayer` (document.ts:494) mints a layer and seeds NOTHING into the
 *     pixel store - it only touches the Rust layer graph;
 *   - `setLayerImageBitmap` (document.ts:1410) installs the raster and seeds
 *     NOTHING.
 * Both are reproduced rather than assumed, because if either seeded the store the
 * whole measurement would be an artefact of the double. `docId` is returned by
 * `getId` because that is what every store command is namespaced by.
 */
function makeEngine(docId: string = DOC): {
  engine: {
    getId: () => string;
    getWidth: () => number;
    getHeight: () => number;
    getLayers: () => FakeLayer[];
    getLayer: (id: string) => FakeLayer | null;
    getActiveLayerId: () => string | null;
    snapshot: () => unknown;
    addLayer: (name: string, w: number, h: number) => FakeLayer;
    setLayerImageBitmap: (id: string, bitmap: ImageBitmap) => void;
    invalidatePaintSurface: Mock;
    markCompositeStoreProjection: Mock;
    notifyVisualChange: Mock;
    isShapeLayer: (id: string) => boolean;
    isTextLayer: (id: string) => boolean;
  };
  layers: FakeLayer[];
  snapshotCalls: number;
} {
  const layers: FakeLayer[] = [];
  const state = { snapshotCalls: 0 };
  const engine = {
    getId: () => docId,
    getWidth: () => SIZE,
    getHeight: () => SIZE,
    getLayers: () => layers,
    getLayer: (id: string) => layers.find((l) => l.id === id) ?? null,
    getActiveLayerId: () => layers[0]?.id ?? null,
    snapshot: () => { state.snapshotCalls++; return { id: docId, layers: [] }; },
    addLayer: (name: string, w: number, h: number) => {
      const layer: FakeLayer = {
        id: `layer-stamp-${layers.length + 1}`,
        name, type: "raster", visible: true, width: w, height: h,
        imageBitmap: null,
        transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
        opacity: 1, blendMode: "normal", locked: false, isBackground: false,
      };
      layers.unshift(layer);
      return layer;
    },
    setLayerImageBitmap: (id: string, bitmap: ImageBitmap) => {
      const layer = layers.find((l) => l.id === id);
      if (layer) layer.imageBitmap = bitmap;
    },
    // The seed's THIRD step reaches these. `syncLayerStoreToLayerRaster` calls
    // `invalidatePaintSurface` unguarded, so a double without it aborts the seed
    // one line BEFORE `rust_pixels_resize_layer` and the outer catch swallows
    // the TypeError - which would leave the re-assertion, the entry drop and
    // `markCompositeStoreProjection` silently untested while every assertion
    // here still passed. Recording the calls makes the step OBSERVABLE, so a
    // future abort shows up as a missing call rather than as silence.
    invalidatePaintSurface: vi.fn(),
    markCompositeStoreProjection: vi.fn(),
    notifyVisualChange: vi.fn(),
    isShapeLayer: () => false,
    isTextLayer: () => false,
  };
  return { engine, layers, snapshotCalls: state.snapshotCalls };
}

/** The renderer surface the layer ops touch: GPU bookkeeping only. */
function makeRenderer(): unknown {
  return { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn(), destroyTexture: vi.fn() };
}

/** One visible source layer carrying a real, non-uniform raster. */
function sourceLayer(id: string, seed: number): FakeLayer {
  return {
    id, name: id, type: "raster", visible: true, width: SIZE, height: SIZE,
    imageBitmap: rasterBitmap(SIZE, SIZE, gradientRaster(SIZE, SIZE, seed)),
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    opacity: 1, blendMode: "normal", locked: false, isBackground: false,
  };
}

/** The editor surface `handleLayerOpsKey` reads for the stamp branch. */
function makeKeyboardCtx(engine: unknown, renderer: unknown): unknown {
  return {
    editor: {
      renderer,
      scheduler: { requestRender: vi.fn() },
      layerTransformSession: () => null,
      workspace: { notifyVisualChange: vi.fn(), getActiveEngine: () => engine },
      selectedLayerIds: () => [],
      setSelectedLayerId: vi.fn(),
      setActiveTool: vi.fn(),
      setShowTransformControls: vi.fn(),
    },
    options: {},
    layerActions: { handleAddLayer: vi.fn(), handleDeleteActiveLayer: vi.fn() },
  };
}

/**
 * THE REAL DISPATCH CHAIN: a Ctrl+Shift+Alt+E keydown, which is the shortcut
 * `layerOps.ts:108-119` binds to "Stamp Visible". Returns the id of the layer the
 * gesture minted.
 */
function pressStampVisible(
  engine: { getLayers: () => FakeLayer[]; snapshot: () => unknown },
  history: unknown,
  renderer: unknown,
): string | null {
  const before = engine.getLayers().length;
  const handled = handleLayerOpsKey(
    makeKeyboardCtx(engine, renderer) as never,
    { key: "e", code: "KeyE", ctrlKey: true, shiftKey: true, altKey: true, preventDefault: vi.fn(), stopPropagation: vi.fn() } as never,
    engine as never,
    history as never,
    "e",
    true,
  );
  expect(handled, "the shortcut was consumed").toBe(true);
  const layers = engine.getLayers();
  return layers.length > before ? layers[0].id : null;
}

describe("MEASURED: stamp visible seeds its composite into the Rust pixel store", () => {
  let store: RustStoreEmulator;
  let restoreCanvas: (() => void) | undefined;

  beforeEach(() => {
    localStorage.clear();
    // Both recorders off: the measurement is about the two pixel owners, and a
    // facade mirror would add a third writer to account for. The armed
    // configuration is covered by its own describe block below.
    localStorage.setItem("photrez.facade", "0");
    invokeMock.mockReset();
    store = createRustStoreEmulator();
    invokeMock.mockImplementation(((cmd: string, args: Record<string, unknown>) =>
      store.invoke(cmd, args)) as never);
    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = undefined;
    restoreCanvas = installCompositeCanvas();
  });

  afterEach(() => {
    restoreCanvas?.();
    restoreCanvas = undefined;
    store.dispose();
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it("emits exactly ONE canonical whole-layer write, the census WRITER site for the op", async () => {
    const { engine, layers } = makeEngine();
    layers.push(sourceLayer("layer-under", 1));
    const history = new CommandHistory(8);

    const stampedId = pressStampVisible(engine, history, makeRenderer());
    await flushAsync();

    expect(stampedId, "premise: the shortcut really composited a new layer").not.toBeNull();
    // THE COUNT IS THE POINT, not just the presence. A seeded store alone
    // satisfies "the owners agree", so convergence would still pass with the
    // canonical write removed - which is why the composite-destination cases in
    // ownerConvergence.test.ts carry a permanent write-counting assertion beside
    // each convergence verdict. One gesture must cost exactly one canonical
    // write, and this is that assertion for stamp visible.
    const writes = store.calls.filter((c) => c.cmd === "rust_pixels_write_region");
    expect(writes.length, "STAMP emits exactly one canonical write").toBe(1);
    expect(
      { x: writes[0].args.x, y: writes[0].args.y, w: writes[0].args.w, h: writes[0].args.h },
      "the write covers the whole destination layer, because a composite destination has no prior content to diff against",
    ).toEqual({ x: 0, y: 0, w: SIZE, h: SIZE });
  });

  // PERFORMANCE. One stamp is a FIXED number of Rust invocations regardless of how
  // many layers went into the composite: the destination is ONE layer, and the
  // helper issues one probe, one init, one whole-layer write and one projection
  // re-assert. Nothing here is per-source-layer - the compositing itself is CPU
  // work over the source bitmaps and never crosses the transport.
  it("costs a FIXED number of invocations, none of them per composited layer", async () => {
    const one = createRustStoreEmulator();
    const many = createRustStoreEmulator();
    try {
      const oneEngine = makeEngine();
      oneEngine.layers.push(sourceLayer("layer-under", 1));
      invokeMock.mockImplementation(((cmd: string, args: Record<string, unknown>) =>
        one.invoke(cmd, args)) as never);
      pressStampVisible(oneEngine.engine, new CommandHistory(8), makeRenderer());
      await flushAsync();
      const oneCalls = one.calls.length;

      const manyEngine = makeEngine();
      for (let i = 0; i < 6; i++) manyEngine.layers.push(sourceLayer(`layer-${i}`, i + 1));
      invokeMock.mockImplementation(((cmd: string, args: Record<string, unknown>) =>
        many.invoke(cmd, args)) as never);
      pressStampVisible(manyEngine.engine, new CommandHistory(8), makeRenderer());
      await flushAsync();
      const manyCalls = many.calls.length;

      expect(
        { oneLayer: oneCalls, sixLayers: manyCalls },
        "six times the composited layers costs the same transport traffic",
      ).toEqual({ oneLayer: oneCalls, sixLayers: oneCalls });
    } finally {
      one.dispose();
      many.dispose();
    }
  });

  // The arm is a fire-and-forget `void` promise, so the question of whether it
  // settles at all is not observable from the gesture. A stamp that threw would
  // leave the store unseeded AND lose the user's layer, so the post-arm cases
  // below assert the settled state rather than the command list.
  it("settles rather than leaving the destination unseeded", async () => {
    const { engine, layers } = makeEngine();
    layers.push(sourceLayer("layer-under", 1));
    const history = new CommandHistory(8);

    const stampedId = pressStampVisible(engine, history, makeRenderer())!;
    await flushAsync(8);

    const storeRead = await store
      .invoke("rust_pixels_snapshot_layer", { docId: DOC, layerId: stampedId })
      .then(() => "HAS-ENTRY" as const, (err: unknown) => `NO-ENTRY: ${String(err)}`);
    expect(storeRead, "the arm's promise settled and seeded the destination").toBe("HAS-ENTRY");
  });

  // THE SEED'S THIRD STEP, observed rather than assumed. This step is what drops
  // the `Pixel` entry the write just opened, so the structural entry stays the
  // undo tip - the whole cursor-bookkeeping reason the seed has three steps. Its
  // engine-side contract is pinned in Rust, on the real shared cursor, by
  // `pixel_store::composite_destination_seed_tests`.
  //
  // What THIS case proves is narrower and was previously unproven: the third step
  // actually RUNS in this fixture. It used not to. `syncLayerStoreToLayerRaster`
  // calls `invalidatePaintSurface` unguarded, so a double lacking it raised a
  // TypeError one line before `rust_pixels_resize_layer`, the seed's outer catch
  // swallowed it, and the re-assertion never happened - invisibly, because the
  // store was already seeded by step 2 and every other assertion still passed.
  // So the assertion is on the COMMAND, which is what a swallowed abort removes.
  it("the re-assertion runs: it re-asserts the store from the raster and retires the row", async () => {
    const { engine, layers } = makeEngine();
    layers.push(sourceLayer("layer-under", 1));
    const history = new CommandHistory(8);

    const stampedId = pressStampVisible(engine, history, makeRenderer())!;
    await flushAsync(8);

    expect(
      store.calls.filter((c) => c.cmd === "rust_pixels_resize_layer").length,
      "the third step reached rust_pixels_resize_layer - a swallowed abort before it would leave this at 0",
    ).toBe(1);
    expect(
      engine.invalidatePaintSurface,
      "the projection was invalidated, which is the call the missing method used to abort on",
    ).toHaveBeenCalled();
    expect(
      engine.markCompositeStoreProjection,
      "the destination's store row is marked a projection, so the engine retires it with the layer",
    ).toHaveBeenCalledWith(stampedId);
  });

  // THE CONVERGENCE ASSERTION. Not "an entry exists" - the two owners must hold
  // the SAME BYTES, read through the store's own command so a comparison can
  // never read the emulator's private buffer and call agreement proven.
  it("the Rust store holds the composite's exact bytes, so the two owners agree", async () => {
    const { engine, layers } = makeEngine();
    layers.push(sourceLayer("layer-under", 1));
    const history = new CommandHistory(8);

    const stampedId = pressStampVisible(engine, history, makeRenderer())!;
    const stamped = engine.getLayer(stampedId)!;
    await flushAsync();

    // Premise: the destination carries a raster, and it is not blank. Refusing an
    // all-zero or short read is what keeps the next assertion a measurement
    // rather than a comparison of two empty buffers - the `null - null = 0` trap
    // `ownerConvergence.test.ts` records.
    const { bytes, distinct } = readStubProjection(
      stamped.imageBitmap!, stamped.width, stamped.height,
    );
    expect(
      distinct,
      "premise: the composite is non-uniform, so 'agrees with nothing' cannot pass",
    ).toBeGreaterThan(1);

    const tiles = (await store.invoke("rust_pixels_snapshot_layer", {
      docId: DOC, layerId: stampedId,
    })) as Array<{ x: number; y: number; w: number; h: number; data: number[] }>;
    expect(Array.isArray(tiles) && tiles.length > 0, "the store has a layer to read").toBe(true);
    // Dimensions derived from the TILES, not from the TS layer, so the comparison
    // cannot borrow the answer from the side it is checking.
    const width = Math.max(...tiles.map((t) => t.x + t.w));
    const height = Math.max(...tiles.map((t) => t.y + t.h));
    expect({ width, height }, "the store's dimensions match the destination's")
      .toEqual({ width: stamped.width, height: stamped.height });

    const stored = new Uint8ClampedArray(tiles[0].data);
    expect(stored.length, "the store read is a full layer").toBe(width * height * 4);
    expect(
      Array.from(stored),
      "the Rust store holds the composite's exact bytes, not merely an entry",
    ).toEqual(Array.from(bytes));
  });

  // THE DEFEAT for the convergence assertion above: seed the store with the WRONG
  // bytes and the byte comparison must catch it. Without this, a stub that wrote
  // zeros would satisfy "an entry exists" and the convergence case would be
  // measuring the stub rather than the arm.
  it("DEFEAT: a store seeded with different bytes FAILS the byte comparison", async () => {
    const { engine, layers } = makeEngine();
    layers.push(sourceLayer("layer-under", 1));
    const history = new CommandHistory(8);
    const stampedId = pressStampVisible(engine, history, makeRenderer())!;
    const stamped = engine.getLayer(stampedId)!;
    await flushAsync();

    const { bytes } = readStubProjection(stamped.imageBitmap!, stamped.width, stamped.height);

    // Overwrite the store with a different, still non-empty raster.
    const wrong = new Uint8ClampedArray(bytes);
    wrong[0] = (wrong[0] + 1) & 0xff;
    store.seed(stampedId, stamped.width, stamped.height, wrong);

    const tiles = (await store.invoke("rust_pixels_snapshot_layer", {
      docId: DOC, layerId: stampedId,
    })) as Array<{ data: number[] }>;
    const stored = new Uint8ClampedArray(tiles[0].data);
    // The entry EXISTS here - so a presence-only assertion would pass - yet the
    // two owners disagree, and the comparison says so.
    expect(tiles.length, "an entry exists, so presence alone proves nothing").toBeGreaterThan(0);
    expect(
      Array.from(stored),
      "one wrong byte is caught: the comparison is byte-exact, not a presence check",
    ).not.toEqual(Array.from(bytes));
  });

  // THE FALSIFIABILITY PROOF FOR THE MEASUREMENT ITSELF.
  //
  // A NO-ENTRY verdict is only a measurement if NO-ENTRY is caused by the missing
  // THE FALSIFIABILITY PROOF, restated for the armed state.
  //
  // The convergence case above reads the store through the store's own command.
  // That is only a measurement if the read can also come back EMPTY - a probe
  // that always found a layer would make convergence unfalsifiable. So: a layer
  // the stamp never touched must still read as NO-ENTRY, through the identical
  // probe. This is what makes the HAS-ENTRY above mean "the arm seeded it" rather
  // than "the emulator invents layers".
  it("DEFEAT: a layer the stamp never touched still reads NO-ENTRY, so HAS-ENTRY means the arm seeded it", async () => {
    const { engine, layers } = makeEngine();
    layers.push(sourceLayer("layer-under", 1));
    const history = new CommandHistory(8);
    const stampedId = pressStampVisible(engine, history, makeRenderer())!;
    await flushAsync();

    // The composite's own destination: seeded.
    const seeded = await store
      .invoke("rust_pixels_snapshot_layer", { docId: DOC, layerId: stampedId })
      .then(() => "HAS-ENTRY" as const, () => "NO-ENTRY" as const);
    expect(seeded, "the destination was seeded").toBe("HAS-ENTRY");

    // A layer id that was never a destination of this or any gesture, read
    // through the SAME probe. The source layer was composited but never seeded -
    // it is a source, not a destination - so this is NO-ENTRY, and its being
    // NO-ENTRY is what gives the HAS-ENTRY above its meaning.
    const sourceId = "layer-under";
    const untouched = await store
      .invoke("rust_pixels_snapshot_layer", { docId: DOC, layerId: sourceId })
      .then(() => "HAS-ENTRY" as const, () => "NO-ENTRY" as const);
    expect(
      untouched,
      "the source layer is NOT a composite destination, so the arm must not have seeded it - " +
      "and this NO-ENTRY is what proves the probe can report one",
    ).toBe("NO-ENTRY");
  });

  it("the host entry is a plain structural commit, so its pop cannot take the pixel-writer arm", async () => {
    const { engine, layers } = makeEngine();
    layers.push(sourceLayer("layer-under", 1));
    const history = new CommandHistory(8);
    const commit = vi.fn();
    const stubHistory = { commit, attachDocIdGetter: vi.fn(), undo: vi.fn(), redo: vi.fn() };

    const stampedId = pressStampVisible(engine, stubHistory, makeRenderer());
    await flushAsync();

    expect(stampedId, "premise: the shortcut composited").not.toBeNull();
    expect(commit, "one host entry for the gesture, even with the arm seeding pixels").toHaveBeenCalledTimes(1);
    expect(commit).toHaveBeenCalledWith(expect.anything(), "Stamp Visible");
    const call = commit.mock.calls.at(-1)!;
    expect(
      { imperative: call[2], alreadyRecordedInRust: call[3] },
      "a stamped layer is added, not painted: no tile memento and no claim that Rust already recorded a pixel step",
    ).toEqual({ imperative: undefined, alreadyRecordedInRust: undefined });
  });

  // NEGATIVE: the composite is built once, on the keypress, and the seeding that
  // follows is a single whole-layer write. Nothing about the gesture is
  // incremental, so a per-move or per-layer writer would show up here as extra
  // write_region calls. Pinned because the arm is a `void` promise: a future
  // change that made it incremental would not fail any convergence assertion.
  it("issues no write per composited layer and no write before the keypress returns", async () => {
    const { engine, layers } = makeEngine();
    for (let i = 0; i < 4; i++) layers.push(sourceLayer(`layer-${i}`, i + 1));
    const history = new CommandHistory(8);

    const stampedId = pressStampVisible(engine, history, makeRenderer());

    // SYNCHRONOUSLY, before any flush: the keypress itself must not have written.
    // The arm is deferred by construction, so this asserts the deferral rather
    // than the absence of a writer.
    expect(
      store.calls.filter((c) => c.cmd === "rust_pixels_write_region").length,
      "the synchronous keypress issued no write; the arm is deferred",
    ).toBe(0);

    await flushAsync();

    const writes = store.calls.filter((c) => c.cmd === "rust_pixels_write_region");
    expect(
      writes.length,
      "four composited layers still cost exactly one write - the destination is ONE layer",
    ).toBe(1);
    expect(
      { layerId: writes[0].args.layerId, stampedId },
      "the single write targets the composite's own destination layer, not a source",
    ).toEqual({ layerId: stampedId, stampedId });
  });
});

describe("a stamp between two paint strokes leaves both neighbours undoable", () => {
  const DOC_CURSOR = "docStampCursor";
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

  async function paintStroke(bytes: Uint8ClampedArray): Promise<void> {
    await store.invoke("rust_pixels_write_region", {
      docId: DOC_CURSOR, layerId: LAYER, x: 0, y: 0, w: SIZE, h: SIZE, rgba: toIpcBytes(bytes),
    });
  }

  function snapshot() {
    return { id: DOC_CURSOR, layers: [], activeLayerId: LAYER, width: SIZE, height: SIZE } as never;
  }

  function twin(before: Uint8ClampedArray, after: Uint8ClampedArray) {
    const tile = (px: Uint8ClampedArray) => ({ x: 0, y: 0, width: SIZE, height: SIZE, data: px });
    return { layerId: LAYER, surfaceWidth: SIZE, surfaceHeight: SIZE, before: [tile(before)], after: [tile(after)], rustOwned: true };
  }

  beforeEach(async () => {
    localStorage.clear();
    localStorage.setItem("photrez.facade", "0");
    invokeMock.mockReset();
    store = createRustStoreEmulator();
    invokeMock.mockImplementation(((cmd: string, args: Record<string, unknown>) =>
      store.invoke(cmd, args)) as never);
    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = undefined;
    await store.invoke("rust_pixels_init", {
      docId: DOC_CURSOR, layerId: LAYER, width: SIZE, height: SIZE, bytes: toIpcBytes(opaque),
    });
  });

  afterEach(() => {
    store.dispose();
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it("undoing the stamp spends no cursor step; both paint strokes still undo in order", async () => {
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_CURSOR);

    const first = solid(255, 0, 0);
    const second = solid(0, 255, 0);

    // Paint, stamp, paint - what a user does on a canvas they are also painting.
    await paintStroke(first);
    history.commit(snapshot(), "Brush Stroke", twin(opaque, first), true);
    history.commit(snapshot(), "Stamp Visible");
    await paintStroke(second);
    history.commit(snapshot(), "Brush Stroke", twin(first, second), true);

    expect(history.getUndoCount(), "premise: three host entries").toBe(3);

    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(issued().filter((c) => c === "rust_pixels_undo"), "pop 1 stepped the cursor once").toHaveLength(1);
    expect(store.pixelAt(LAYER, 0, 0), "pop 1 reverted the newest stroke back to the first").toEqual([255, 0, 0, 255]);

    // Pop 2 - the STAMP. It must not move the shared cursor, or the stroke below
    // it becomes unreachable.
    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(
      issued().filter((c) => c === "rust_pixels_undo"),
      "the stamp's pop issued no cursor step, so no neighbour's entry was consumed",
    ).toHaveLength(1);
    expect(store.pixelAt(LAYER, 0, 0), "the stamp's pop changed no pixels").toEqual([255, 0, 0, 255]);

    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(issued().filter((c) => c === "rust_pixels_undo"), "pop 3 stepped the cursor").toHaveLength(2);
    expect(store.pixelAt(LAYER, 0, 0), "pop 3 reverted the first stroke").toEqual([0, 0, 0, 255]);
  });

  // THE DEFEAT. Same ordering, but the stamp's entry is committed the way an
  // ARMED producer would commit it - `rustOwned: true`, so the pop takes
  // `fireGatedOnPixelTip` and issues a step the gesture never earned. If this
  // ever stops reproducing the loss, the guard above measures nothing.
  //
  // This is the hazard the arm has to respect. Seeding the composite through
  // `seedCompositeCanonicalPixels` opens a `Pixel` entry on the SAME per-document
  // cursor (it ends with `syncLayerStoreToLayerRaster` precisely to drop it), so
  // a stamp arm that skipped that step - or that marked the HOST entry
  // `rustOwned` - would spend the neighbouring stroke's step and cost the user a
  // stroke's worth of undo.
  it("DEFEAT: a rustOwned stamp twin DOES spend the neighbour stroke's cursor step", async () => {
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_CURSOR);

    const first = solid(255, 0, 0);
    const second = solid(0, 255, 0);

    await paintStroke(first);
    history.commit(snapshot(), "Brush Stroke", twin(opaque, first), true);
    history.commit(
      snapshot(),
      "Stamp Visible",
      { layerId: LAYER, surfaceWidth: SIZE, surfaceHeight: SIZE, before: [], after: [], rustOwned: true },
      true,
    );
    await paintStroke(second);
    history.commit(snapshot(), "Brush Stroke", twin(first, second), true);

    history.undo(snapshot());
    await history.takeLastCursorStep();
    invokeMock.mockClear();

    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(
      issued().filter((c) => c === "rust_pixels_undo"),
      "the armed stamp twin consumed a cursor step",
    ).toHaveLength(1);
    expect(
      store.pixelAt(LAYER, 0, 0),
      "the armed stamp's pop reverted the stroke beneath it, not any stamped pixel",
    ).toEqual([0, 0, 0, 255]);

    // The stroke that lost its step: its pop still issues a step, but there is
    // no entry left behind it. Two presses produced one stroke's worth of change.
    history.undo(snapshot());
    await history.takeLastCursorStep();
    expect(
      issued().filter((c) => c === "rust_pixels_undo"),
      "the stroke beneath the armed stamp still issues a step",
    ).toHaveLength(2);
    expect(
      store.pixelAt(LAYER, 0, 0),
      "but that step had no entry left to undo, so the pixels are unchanged",
    ).toEqual([0, 0, 0, 255]);
  });
});

/**
 * THE SHIPPING DEFAULT: the facade commit mirror ACTIVE.
 *
 * Every other case in this file sets `photrez.facade = "0"`, which is correct for
 * isolating the two pixel owners - but `isFacadeEnabled()` reads `!== "0"`, so the
 * mirror is ON in the shipping app. Under exactly this configuration the mirror
 * records an `External` entry for every non-pixel commit, and its
 * `committedRecordedInBridge` / `alreadyRecordedInRust` stand-downs are what keep
 * that to ONE entry per host entry. Stamp is a non-pixel commit, so it is
 * squarely in the mirror's path and those stand-downs are load-bearing for it.
 *
 * The invariant asserted here is the whole three-part chain: ONE host commit
 * gives ONE Rust entry, which gives AT MOST ONE cursor step. The real
 * `installFacadeCommitShim` runs over the real `CommandHistory` through the real
 * `recordExternalTransitionFor`; only the Tauri transport is replaced, which is
 * the same boundary `historyCursorDriftClosure.wiring.test.ts` replaces and for
 * the same reason.
 */
describe("with the facade mirror ACTIVE (the shipping default), one stamp is one entry and one step", () => {
  const DOC_FACADE = "docStampFacade";
  let store: RustStoreEmulator;
  let restoreCanvas: (() => void) | undefined;
  let liveLayers: Array<{ id: string }> = [];

  beforeEach(async () => {
    localStorage.clear();
    // ON, and stated explicitly: an ABSENT key also means enabled, so a case named
    // for the armed configuration must set it rather than rely on the default.
    localStorage.setItem("photrez.facade", "1");
    invokeMock.mockReset();
    resetStreams();
    store = createRustStoreEmulator();
    liveLayers = [];
    // The shim is module-sticky, so it is installed once for the file with a
    // per-test layer holder - the same arrangement
    // `addLayerPaintableStore.wiring.test.tsx` uses, and for the same reason.
    installFacadeCommitShim({
      getEngine: () => ({ getId: () => DOC_FACADE, getLayers: () => liveLayers }),
      getDocId: () => DOC_FACADE,
    });
    invokeMock.mockImplementation(((cmd: string, args: Record<string, unknown>) =>
      answerFacadeInvoke(cmd, args)) as never);
    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = undefined;
    restoreCanvas = installCompositeCanvas();
    // The shim's `recordExternalTransitionFor` needs the protocol surface, and the
    // census drain is what makes a fire-and-forget mirror observable rather than
    // merely pending.
    await settleCursor();
  });

  afterEach(async () => {
    // Let any fire-and-forget mirror record land before the stream is torn down.
    await settleCursor();
    restoreCanvas?.();
    restoreCanvas = undefined;
    resetStreams();
    store.dispose();
    localStorage.clear();
    vi.restoreAllMocks();
  });

  /**
   * The pixel-store emulator for the composite's own commands, plus the one
   * protocol command the facade mirror issues. Tauri v2 REJECTS for a Rust `Err`
   * and resolves for Ok, so an unknown command is answered as a rejection rather
   * than a permissive `undefined` - a mock that resolved everything would make
   * every recorder look armed.
   */
  /**
   * The transport boundary, split across the two emulators this repo already has,
   * and the split is deliberate rather than convenient:
   *
   *   - the CURSOR surface (`rust_pixels_undo` / `_redo` / `_history_tip`) is
   *     answered by `rustStreamEmulator`, which models the per-document entry
   *     STREAM the shim records into and the pop steps. It is the only emulator
   *     that models the one-macrotask delay before a cursor move, which is what
   *     makes a step observable at all.
   *   - the PIXEL-DATA surface (`_init` / `_write_region` / `_get_epoch` /
   *     `_snapshot_layer`) is answered by `rustStoreEmulator`, which holds real
   *     bytes and rejects where Rust rejects.
   *
   * In the app both surfaces are one `PixelStoreRegistry`. Splitting them here
   * means the store's `write_region` does NOT append to the stream, so the entry
   * assertion below counts the MIRROR's records and not the composite's write -
   * which is the quantity the one-entry-per-host-commit rule is about. Stated
   * explicitly because a reader would otherwise assume one emulator.
   */
  function answerFacadeInvoke(cmd: string, args: Record<string, unknown>): Promise<unknown> {
    switch (cmd) {
      // The protocol commands the mirror's path reaches. `recordExternalTransition`
      // is the one that matters: it appends an `External` entry to the same
      // emulated stream the cursor steps, which is what makes the shim a real
      // recorder here rather than a call counter. The rest answer the shapes the
      // real commands produce, so the path reaches the record instead of
      // short-circuiting on an envelope error.
      case "protocol_register_adapter_native":
      case "protocol_seed_native":
      case "protocol_seed_canonical_native":
        return Promise.resolve(JSON.stringify({ version: 0, layers: [] }));
      case "protocol_version_native":
        return Promise.resolve(1);
      case "protocol_apply_command_native": {
        const envelope = JSON.parse(String(args.envelopeJson)) as {
          command?: { type?: string };
        };
        if (envelope.command?.type === "recordExternalTransition") {
          record(String(args.docId), "external");
          const s = streamFor(String(args.docId));
          return Promise.resolve(JSON.stringify({ documentVersion: s.version, externalSeq: s.entries.length }));
        }
        return Promise.resolve(JSON.stringify({ documentVersion: streamFor(String(args.docId)).version }));
      }
      // The cursor surface, from the stream emulator that models it.
      case "rust_pixels_undo":
      case "rust_pixels_redo":
      case "rust_pixels_history_tip":
        return answerInvoke(cmd, args);
      default:
        return store.invoke(cmd, args);
    }
  }

  it("one host commit, one mirrored Rust entry, and the stamp's pop spends exactly one cursor step", async () => {
    const { engine, layers } = makeEngine(DOC_FACADE);
    layers.push(sourceLayer("layer-under", 1));
    liveLayers = layers.map((l) => ({ id: l.id }));

    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_FACADE);
    const commitSpy = vi.spyOn(history, "commit");

    const stampedId = pressStampVisible(engine, history, makeRenderer());
    await flushAsync();
    // The shim's mirror is fire-and-forget behind a dynamic import, so the entry
    // lands some ticks after the gesture returns.
    await waitForRust(DOC_FACADE, 1);

    // ONE host entry for the gesture. The arm seeds PIXELS, which must not add a
    // second host entry. Two recorders for one commit is the drift the
    // stand-downs exist to prevent: the stream would gain an entry per undone
    // step while the pop issues a single step.
    expect(commitSpy, "exactly one host commit for one stamp").toHaveBeenCalledTimes(1);

    // ONE Rust entry, mirrored as `external`. A second one would mean two
    // recorders ran for one commit, and the single pop step could not keep up -
    // the stream would gain an entry per undone step.
    expect(
      streamFor(DOC_FACADE).entries,
      "the facade mirror recorded exactly one External entry for the stamp",
    ).toEqual(["external"]);

    // The composite still reached the store: the arm is orthogonal to the mirror.
    const storeRead = await store
      .invoke("rust_pixels_snapshot_layer", { docId: DOC_FACADE, layerId: stampedId })
      .then(() => "HAS-ENTRY" as const, () => "NO-ENTRY" as const);
    expect(storeRead, "the composite was seeded regardless of the mirror").toBe("HAS-ENTRY");

    // EXACTLY ONE cursor step, and it lands on the mirror's OWN `External` entry -
    // not on a neighbour's, and not twice.
    //
    // One step is the correct count here, and the reason is the shim arm of
    // the shim arm of `stepRustCursor` (engine/historyCursorStep.ts): when the
    // shim is the recorder for an entry, the pop MUST step it, or the entry the
    // mirror recorded would have no consumer and the two cursors would separate.
    // The DEFEAT case in the previous describe block pins the OTHER failure - an
    // entry marked `rustOwned` spends a step the gesture never earned - and that
    // is unreachable here precisely because the host entry carries no
    // `imperative`.
    history.undo(engine.snapshot() as never);
    await history.takeLastCursorStep();
    await waitForRust(DOC_FACADE, 1);
    await flushAsync(2);
    expect(
      timesInvoked("rust_pixels_undo") + timesInvoked("rust_pixels_redo"),
      "one press, one cursor step - not zero (the mirror's entry needs a consumer) and not two",
    ).toBe(1);

    // The step consumed the mirror's own entry: the stream's cursor is back at the
    // bottom, which is the observable proof that it did not eat a neighbour's.
    expect(
      { cursor: streamFor(DOC_FACADE).cursor, depth: streamFor(DOC_FACADE).entries.length },
      "the pop moved the cursor over the mirror's External entry, leaving no entry stranded above it",
    ).toEqual({ cursor: 0, depth: 1 });
  });

  // THE DEFEAT, and the reason it is not a contrived mutation of the shim.
  //
  // `stampVisibleLayers` commits BEFORE it mutates, and the arm it now calls only
  // writes PIXELS - so under the mirror there is exactly one recorder per gesture
  // by construction. The way to make that claim falsifiable is to remove the
  // thing that makes it true and watch the accounting break: with the mirror
  // switched OFF, the stamp's pop issues NO cursor step at all, because there is
  // then no `External` entry for the shim arm to step and the host entry carries
  // no `imperative`.
  //
  // So the two rows together are the invariant: the step count TRACKS the entry
  // count. One entry => one step. Zero entries => zero steps. A regression that
  // made stamp's pop step unconditionally would show up as one step with the
  // facade off, against the zero this case pins.
  it("DEFEAT: with the mirror OFF the same gesture issues NO cursor step, so the count tracks the entries", async () => {
    localStorage.setItem("photrez.facade", "0");
    resetStreams();
    const { engine, layers } = makeEngine(DOC_FACADE);
    layers.push(sourceLayer("layer-under", 1));

    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_FACADE);
    const stampedId = pressStampVisible(engine, history, makeRenderer());
    await flushAsync();

    // Premise: the gesture was the same one - the arm still seeded the composite,
    // so this differs from the armed row in the RECORDER, not in the operation.
    expect(stampedId, "premise: the shortcut composited").not.toBeNull();
    const storeRead = await store
      .invoke("rust_pixels_snapshot_layer", { docId: DOC_FACADE, layerId: stampedId })
      .then(() => "HAS-ENTRY" as const, () => "NO-ENTRY" as const);
    expect(storeRead, "premise: the composite was still seeded with the mirror off").toBe("HAS-ENTRY");

    expect(
      streamFor(DOC_FACADE).entries,
      "no recorder ran, so the stream is empty",
    ).toEqual([]);

    history.undo(engine.snapshot() as never);
    await history.takeLastCursorStep();
    await flushAsync(2);
    expect(
      timesInvoked("rust_pixels_undo") + timesInvoked("rust_pixels_redo"),
      "no entry, no step - the count follows the entries rather than being unconditional",
    ).toBe(0);
  });
});
