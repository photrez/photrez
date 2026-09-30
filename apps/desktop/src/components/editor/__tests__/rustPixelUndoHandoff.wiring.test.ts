/**
 * Wiring test: the Rust history stream is the single executor for pixel
 * undo/redo, and the host only PROJECTS the result.
 *
 * The chain driven here is the production one, end to end:
 *   runFacadeExternalHandoff -> EditorFacade.undo -> bridge.applyCommand ->
 *   nativeProtocol -> Tauri invoke -> protocol_apply_command_native
 *
 * MOCK FIDELITY: only `@tauri-apps/api/core`'s `invoke` is mocked - the true
 * transport boundary. The `protocol_apply_command_native` reply below is the
 * EXACT JSON the engine emits, key for key, including the camelCase
 * `pixelPatches` wrapper around Rust-named `TilePatch` fields. That shape is
 * pinned on the Rust side by
 * `pixel_handoff_serializes_to_the_keys_the_host_parses` in
 * crates/core/src/pixel_store/tests.rs, so a mock that guessed a different
 * shape would fail here. The real `EditorFacade` and the real handoff run
 * unmocked, which is the point: a test that stubs the facade can be green while
 * the app is broken.
 *
 * What must hold:
 *   1. A pixel undo uploads exactly the tiles Rust returned (authoritative
 *      bytes), not any local memento.
 *   2. The handoff reports the step as FULLY HANDLED (returns true) so the
 *      caller cannot pop a second history entry for a step already taken.
 *   3. `rust_pixels_undo` is never invoked - the host must not run a second
 *      pixel step over the same cursor.
 *   4. A projection failure AFTER Rust moved the cursor still reports handled:
 *      "fall through" is only correct while the Rust command itself failed.
 */
import { describe, it, expect, vi, beforeAll, beforeEach, afterAll, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { CommandHistory } from "@/engine/history";
import * as backgroundFlagRouting from "@/lib/protocol/backgroundFlagRouting";

// The baseline record is module state the undo gate reads. These wrappers are
// optional-call so that reverting the PRODUCTION side of that record does not
// turn this fixture into a TypeError: a missing recorder must surface as the
// BEHAVIOURAL failure under test (the gate not refusing), which is the point of
// the RED proof, not as a harness crash that proves nothing.
const recordTestBaseline = (docId: string) =>
  backgroundFlagRouting.recordOpenBaselineEntry?.(docId);
const clearTestBaseline = (docId: string) =>
  backgroundFlagRouting.__clearOpenBaselineForTests?.(docId);
import { runFacadeExternalHandoff } from "../facadeHistoryHandoff";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/lib/protocol/facadeRegistry", async () => {
  const actual = await vi.importActual<typeof import("@/lib/protocol/facadeRegistry")>(
    "@/lib/protocol/facadeRegistry",
  );
  return { ...actual, confirmExternalCursor: vi.fn(async () => ({ ok: true })) };
});

const DOC_ID = "docPixelHandoff";
const invokeMock = invoke as unknown as Mock<(cmd: string, args?: unknown) => Promise<unknown>>;

// jsdom does not implement the ImageData Web API; WebView2 (the production
// host) does, and `applyRustTilesToSurface` builds one per tile. This installs
// the real contract - `new ImageData(Uint8ClampedArray, width, height)` exposing
// `.data/.width/.height` - so the test exercises the production call instead of
// a stub that would hide a real failure inside the `catch`.
class TestImageData {
  readonly data: Uint8ClampedArray;
  readonly width: number;
  readonly height: number;
  constructor(data: Uint8ClampedArray, width: number, height: number) {
    this.data = data;
    this.width = width;
    this.height = height;
  }
}
beforeAll(() => {
  (globalThis as unknown as { ImageData: unknown }).ImageData = TestImageData;
});
afterAll(() => {
  delete (globalThis as unknown as { ImageData?: unknown }).ImageData;
});

// Bytes the engine returns for the "before" tile of a stroke: 1x1, all zeros.
const BEFORE_TILE = { x: 0, y: 0, w: 1, h: 1, data: [0, 0, 0, 0] };

type Calls = string[];

/** Reply for protocol_apply_command_native carrying a pixel undo step. */
function pixelUndoResultJson(): string {
  return JSON.stringify({
    documentVersion: 1,
    // A pixel step changes no layer metadata, so the delta is EMPTY. The tiles
    // ride pixelPatches. This is the exact combination that used to make the
    // host fall through and undo a second time.
    delta: { baseVersion: 0, version: 1, changes: [] },
    pixelPatches: {
      layerId: "L1",
      tiles: [BEFORE_TILE],
      epoch: 7,
      version: 1,
    },
  });
}

function makeEngine(layerId: string, surface: unknown, calls: Calls) {
  return {
    getId: () => DOC_ID,
    getLayer: (id: string) =>
      id === layerId ? { id, width: 1, height: 1, imageBitmap: {} as ImageBitmap } : null,
    getLayers: () => [],
    getPaintSurface: () => surface,
    ensureBitmapCurrent: vi.fn(async (docId: string, id: string) => {
      calls.push(`ensureBitmapCurrent:${docId}:${id}`);
    }),
    applyFacadeSnapshot: vi.fn(),
  };
}

function makeCtx(engine: unknown, calls: Calls, history?: unknown) {
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveDocumentId: () => DOC_ID,
      getActiveHistory: () => history ?? null,
      notifyVisualChange: vi.fn(),
    },
    renderer: {
      uploadImage: vi.fn(),
      uploadSurfaceTiles: vi.fn((...args: unknown[]) => {
        calls.push("uploadSurfaceTiles");
        void args;
      }),
    },
    scheduler: { requestRender: vi.fn() },
  } as unknown as Parameters<typeof runFacadeExternalHandoff>[0];
}

/** Paint surface double recording the putImageData calls it receives. */
function makeSurface(calls: Calls) {
  return {
    pixelEpoch: 0,
    pixelVersion: undefined as number | undefined,
    context: {
      putImageData: (img: { width: number; height: number; data: Uint8ClampedArray }, x: number, y: number) => {
        calls.push(`putImageData:${img.width}x${img.height}@${x},${y}=${Array.from(img.data).join(",")}`);
      },
    },
  };
}

describe("rust pixel undo/redo is the single executor; the host projects it", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockReset();
  });

  afterEach(() => {
    localStorage.clear();
  });

  it("uploads the tiles Rust returned and does not take a second pixel step", async () => {
    const calls: Calls = [];
    const surface = makeSurface(calls);
    const engine = makeEngine("L1", surface, calls);
    const ctx = makeCtx(engine, calls);

    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "1";
        case "protocol_apply_command_native":
          return pixelUndoResultJson();
        default:
          throw `unexpected invoke in the pixel undo path: ${cmd}`;
      }
    });

    const handled = await runFacadeExternalHandoff(ctx, "undo");

    // 1. The handoff reports the step as fully handled. If this were false the
    //    caller would pop the TS history store for a step Rust already took.
    expect(handled).toBe(true);

    // 2. The authoritative tiles reached the GPU upload, in the renderer's
    //    width/height shape, at the layer's dims.
    const renderer = (ctx as unknown as { renderer: { uploadSurfaceTiles: Mock } }).renderer;
    expect(renderer.uploadSurfaceTiles).toHaveBeenCalledTimes(1);
    const [layerId, w, h, tiles] = renderer.uploadSurfaceTiles.mock.calls[0] as [
      string, number, number, { x: number; y: number; width: number; height: number; data: Uint8ClampedArray }[],
    ];
    expect(layerId).toBe("L1");
    expect([w, h]).toEqual([1, 1]);
    expect(tiles).toHaveLength(1);
    expect(tiles[0].width).toBe(1);
    expect(tiles[0].height).toBe(1);
    expect(Array.from(tiles[0].data)).toEqual(BEFORE_TILE.data);

    // 3. The derived surface got the same bytes and was stamped with the exact
    //    epoch/version the engine reported, so a later cache read is a no-op.
    expect(calls).toContain("putImageData:1x1@0,0=0,0,0,0");
    expect(surface.pixelEpoch).toBe(7);
    expect(surface.pixelVersion).toBe(1);

    // 4. The model bitmap is repaired from the canonical buffer.
    expect(calls).toContain(`ensureBitmapCurrent:${DOC_ID}:L1`);

    // 5. No second pixel step: rust_pixels_undo must never appear on this path,
    //    and the only command issued was the one protocol command.
    const invoked = invokeMock.mock.calls.map((c) => c[0]);
    expect(invoked).not.toContain("rust_pixels_undo");
    expect(invoked).not.toContain("rust_pixels_redo");
    expect(invoked.filter((c) => c === "protocol_apply_command_native")).toHaveLength(1);
  });

  it("treats a pixel step with NO tiles as done, so the host cannot undo twice", async () => {
    const calls: Calls = [];
    const surface = makeSurface(calls);
    const engine = makeEngine("L1", surface, calls);
    const ctx = makeCtx(engine, calls);

    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "1";
        case "protocol_apply_command_native":
          return JSON.stringify({
            documentVersion: 1,
            delta: { baseVersion: 0, version: 1, changes: [] },
            // Present but empty: Rust moved the cursor, it just had no dirty
            // region. Presence is the signal, not the tile count.
            pixelPatches: { layerId: "L1", tiles: [], epoch: 7, version: 1 },
          });
        default:
          throw `unexpected invoke in the pixel undo path: ${cmd}`;
      }
    });

    const handled = await runFacadeExternalHandoff(ctx, "undo");

    // An empty-tile pixel step is still a taken step: presence is the signal.
    expect(handled).toBe(true);
    expect(calls).not.toContain("uploadSurfaceTiles");
    const invoked = invokeMock.mock.calls.map((c) => c[0]);
    expect(invoked).not.toContain("rust_pixels_undo");
  });

  it("reports the step handled when the projection throws AFTER Rust moved its cursor", async () => {
    const calls: Calls = [];
    // putImageData is the first thing the projection does with Rust's tiles.
    // It throws here, i.e. AFTER the Rust command resolved and moved the cursor.
    const surface = {
      pixelEpoch: 0,
      pixelVersion: undefined as number | undefined,
      context: {
        putImageData: () => {
          throw new Error("paint surface lost");
        },
      },
    };
    const engine = makeEngine("L1", surface, calls);
    const ctx = makeCtx(engine, calls);

    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "1";
        case "protocol_apply_command_native":
          return pixelUndoResultJson();
        default:
          throw `unexpected invoke in the pixel undo path: ${cmd}`;
      }
    });

    const handled = await runFacadeExternalHandoff(ctx, "undo");

    // Rust owns the step now. Reporting false would make the caller pop a TS
    // entry AND fire rust_pixels_undo - two more undo steps for one press.
    expect(handled).toBe(true);
    const invoked = invokeMock.mock.calls.map((c) => c[0]);
    expect(invoked).not.toContain("rust_pixels_undo");
    expect(invoked).not.toContain("rust_pixels_redo");
  });

  it("still falls through to the TS store when Rust steps a metadata entry", async () => {
    const calls: Calls = [];
    const surface = makeSurface(calls);
    const engine = makeEngine("L1", surface, calls);
    const ctx = makeCtx(engine, calls);

    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "1";
        case "protocol_apply_command_native":
          // Non-empty delta, no pixelPatches key: a metadata restore, which the
          // handoff already owns. It must NOT be mistaken for a pixel step.
          return JSON.stringify({
            documentVersion: 1,
            delta: {
              baseVersion: 0,
              version: 1,
              changes: [{ kind: "upsert", layer: { id: "L1", name: "L1", width: 1, height: 1, opacity: 1, visible: true } }],
            },
          });
        default:
          throw `unexpected invoke: ${cmd}`;
      }
    });

    const handled = await runFacadeExternalHandoff(ctx, "undo");

    // The non-empty delta branch still owns a metadata step.
    expect(handled).toBe(true);
    expect(calls).not.toContain("uploadSurfaceTiles");
  });
});

// ── depth lockstep ────────────────────────────────────────────────────────
// A pixel step Rust already recorded has a TS twin that is a cursor token: it
// must be drained in lockstep with the Rust cursor or the TS depth stops
// describing the real history. Defect under test (measured in the real app at
// 70ea270, artifact sha256 93eb399406c5709442ae3f768aef1bc9dd6baa00ea26b524eb61d210f1ac80ef
// case 2): 5 strokes then 5 undos returned the Rust cursor to its start
// (6 -> 1) while ts_undo stayed at 5 and canUndo() kept reporting true, so a
// 6th press was dispatched and walked the cursor 1 -> 0, past the start.
//
// The Rust stream here holds ONLY the 5 pixel entries, matching what this fix
// owns. A document-open metadata entry would be a separate, legitimate step.

/** One tile per step, distinct bytes so a wrong-step projection is visible. */
function pixelStepTile(n: number) {
  return { x: 0, y: 0, w: 1, h: 1, data: [n, n, n, 255] };
}

/**
 * Rust reply for a pixel undo/redo of entry `n` (1-based), cursor advanced.
 * `data` crosses the wire as a plain number array; the TS memento twin the same
 * step carries is typed Uint8ClampedArray, so each side converts its own way.
 */
function pixelStepJson(n: number) {
  return JSON.stringify({
    documentVersion: n,
    delta: { baseVersion: n - 1, version: n, changes: [] },
    pixelPatches: { layerId: "L1", tiles: [pixelStepTile(n)], epoch: n, version: n },
  });
}

/** Same tile in the memento shape `HistoryTilePatches` requires. */
function mementoTile(n: number) {
  return {
    x: 0,
    y: 0,
    width: 1,
    height: 1,
    data: Uint8ClampedArray.from(pixelStepTile(n).data),
  };
}

/** Rust reply for a step past the end of the stream: nothing to take. */
function exhaustedJson() {
  return JSON.stringify({
    documentVersion: 0,
    delta: { baseVersion: 0, version: 0, changes: [] },
  });
}

describe("the TS twin drains in lockstep with a Rust pixel step", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockReset();
  });

  afterEach(() => {
    localStorage.clear();
  });

  /** Real CommandHistory holding `n` rustOwned pixel twins, newest last. */
  function makeTwinHistory(n: number) {
    const history = new CommandHistory();
    for (let i = 1; i <= n; i++) {
      history.commit(
        { layers: [], activeLayerId: "L1" } as never,
        "Brush Stroke",
        {
          layerId: "L1",
          surfaceWidth: 1,
          surfaceHeight: 1,
          before: [mementoTile(i)],
          after: [mementoTile(i)],
          rustOwned: true,
        },
        true,
      );
    }
    return history;
  }

  it("five rustOwned steps then six undos: depth tracks the cursor, the 6th changes nothing", async () => {
    const calls: Calls = [];
    const history = makeTwinHistory(5);
    const surface = makeSurface(calls);
    const engine = makeEngine("L1", surface, calls);
    const ctx = makeCtx(engine, calls, history);

    // The Rust stream: one pixel entry per stroke, consumed newest-first.
    let cursor = 5;
    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "1";
        case "protocol_apply_command_native":
          if (cursor === 0) return exhaustedJson();
          cursor -= 1;
          return pixelStepJson(cursor + 1);
        default:
          throw `unexpected invoke in the pixel undo path: ${cmd}`;
      }
    });

    // Non-vacuity: five twins exist and nothing has been drained yet.
    expect(history.getUndoCount()).toBe(5);
    expect(history.canUndo()).toBe(true);

    for (let i = 1; i <= 5; i++) {
      const handled = await runFacadeExternalHandoff(ctx, "undo");
      expect(handled, `undo ${i} is a pixel step Rust took`).toBe(true);
      expect(history.getUndoCount(), `TS depth after undo ${i}`).toBe(5 - i);
    }

    // (a) After 5 undos the TS depth and the Rust cursor agree at the start.
    expect(cursor, "Rust cursor back at its start").toBe(0);
    expect(history.canUndo(), "no pixel work left, so the gate is closed").toBe(false);
    // The drained entries move to the redo stack, not the bin: canRedo() must
    // still describe the work the Rust cursor holds.
    expect(history.getRedoCount()).toBe(5);
    expect(history.canRedo()).toBe(true);

    // (b) The 6th press: Rust has nothing left, so the step is refused.
    const callsBefore = calls.length;
    const uploadsBefore = (
      ctx.renderer.uploadSurfaceTiles as unknown as { mock: { calls: unknown[][] } }
    ).mock.calls.length;
    const sixth = await runFacadeExternalHandoff(ctx, "undo");

    expect(sixth, "a step past the start is NOT claimed").toBe(false);
    expect(cursor, "the Rust cursor did not move").toBe(0);
    expect(calls.slice(callsBefore), "no pixel projection ran").toEqual([]);
    expect(
      (ctx.renderer.uploadSurfaceTiles as unknown as { mock: { calls: unknown[][] } }).mock.calls.length,
      "no tile upload ran",
    ).toBe(uploadsBefore);
    expect(history.getUndoCount(), "the TS depth is unchanged").toBe(0);
    expect(
      invokeMock.mock.calls.map((c) => c[0]),
      "this path never runs a second pixel executor",
    ).not.toContain("rust_pixels_undo");
    expect(
      invokeMock.mock.calls.map((c) => c[0]),
      "nor redo",
    ).not.toContain("rust_pixels_redo");
  });

  it("a redo step drains the redo stack so canRedo() stops lying in the other direction", async () => {
    const calls: Calls = [];
    const history = makeTwinHistory(2);
    const surface = makeSurface(calls);
    const engine = makeEngine("L1", surface, calls);
    const ctx = makeCtx(engine, calls, history);

    let cursor = 0;
    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "1";
        case "protocol_apply_command_native":
          cursor += 1;
          return pixelStepJson(cursor);
        default:
          throw `unexpected invoke in the pixel redo path: ${cmd}`;
      }
    });

    await runFacadeExternalHandoff(ctx, "undo");
    await runFacadeExternalHandoff(ctx, "undo");
    expect(history.getUndoCount()).toBe(0);
    expect(history.getRedoCount(), "both drained entries sit on the redo stack").toBe(2);

    const handled = await runFacadeExternalHandoff(ctx, "redo");
    expect(handled, "redo is a pixel step Rust took").toBe(true);
    expect(history.getRedoCount(), "the redo twin drained too").toBe(1);
    expect(history.canRedo()).toBe(true);
    expect(history.getUndoCount(), "and it moved to the undo stack").toBe(1);
  });

  it("a metadata step drains nothing: the TS store still owns that entry", async () => {
    const calls: Calls = [];
    // No rustOwned mark: a TS-owned metadata entry the TS store must execute.
    const history = new CommandHistory();
    history.commit({ layers: [], activeLayerId: "L1" } as never, "Add Layer");
    const surface = makeSurface(calls);
    const engine = makeEngine("L1", surface, calls);
    const ctx = makeCtx(engine, calls, history);

    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "1";
        case "protocol_apply_command_native":
          return JSON.stringify({
            documentVersion: 1,
            delta: {
              baseVersion: 0,
              version: 1,
              changes: [{ kind: "upsert", layer: { id: "L1", name: "L1", width: 1, height: 1, opacity: 1, visible: true } }],
            },
          });
        default:
          throw `unexpected invoke: ${cmd}`;
      }
    });

    const handled = await runFacadeExternalHandoff(ctx, "undo");

    expect(handled, "the metadata step is the handoff's own").toBe(true);
    expect(history.getUndoCount(), "the TS entry survived for the TS store").toBe(1);
    expect(history.canUndo()).toBe(true);
  });
});

// ── the open baseline: the cursor must not walk in front of history ────────
// The Rust stream of every document opens at cursor 1, not 0. That first entry
// is the factory's own SetBackgroundFlag commit (WorkspaceManager.
// createBlankDocument -> commitFacadeBackgroundFlag), not a user edit, so it is
// the floor of history: undoing past it means undoing the document's creation.
//
// Defect under test (measured in the real app at 9403949, artifact sha256
// b62a22d57a3388933d1e139b5e9a09a1b61bfe513f17ee71b1633c6a4b28b28b case 1):
// after 5 strokes and 5 undos everything is correct (ts_undo 5 -> 0, canUndo
// false, Rust cursor back at 1), but the 6th Ctrl+Z still stepped that baseline
// entry and walked the cursor 1 -> 0, stealing a redo slot so 5 redos reached
// only the 4-stroke state.

/**
 * The facade's own history stream, as `getHistoryQuery` reports it. Entry 1 is
 * the document-open baseline (the factory's SetBackgroundFlag commit); every
 * entry above it is user work.
 */
function historyQueryJson(cursor: number, total: number): string {
  return JSON.stringify({
    cursor,
    lastSeq: total,
    degradedHint: false,
    pendingExternal: null,
    entries: Array.from({ length: total }, (_, i) => ({
      seq: i + 1,
      groupId: "g1",
      origin: "native",
      label: i === 0 ? "Set Background Flag" : "pixel",
      affectedLayerIds: ["L1"],
      versionBefore: i,
      versionAfter: i + 1,
      memoryCostBytes: 0,
      payloadRef: null,
    })),
  });
}

/** Rust reply for stepping the open baseline: a real entry, real cursor move. */
function baselineStepJson() {
  return JSON.stringify({
    documentVersion: 1,
    delta: { baseVersion: 0, version: 1, changes: [] },
    // A metadata entry produces a real delta (the background flag is restated),
    // which is exactly why it is claimed as a handled step today.
    pixelPatches: undefined,
  });
}

describe("the undo dispatch will not step the Rust cursor past the open baseline", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockReset();
    // Module-global, like every other registry in this app: each test starts
    // from "no baseline recorded" so the over-gating guards below really do
    // exercise the floor-is-0 branch.
    clearTestBaseline(DOC_ID);
  });

  afterEach(() => {
    localStorage.clear();
    clearTestBaseline(DOC_ID);
  });

  it("five rustOwned steps then a 6th undo: nothing moves, and redo round-trips", async () => {
    const calls: Calls = [];
    const history = new CommandHistory();
    for (let i = 1; i <= 5; i++) {
      history.commit(
        { layers: [], activeLayerId: "L1" } as never,
        "Brush Stroke",
        {
          layerId: "L1",
          surfaceWidth: 1,
          surfaceHeight: 1,
          before: [mementoTile(i)],
          after: [mementoTile(i)],
          rustOwned: true,
        },
        true,
      );
    }
    const surface = makeSurface(calls);
    const engine = makeEngine("L1", surface, calls);
    const ctx = makeCtx(engine, calls, history);

    // The document-open baseline occupies history position 1, so the stream
    // starts at cursor 1 and the five pixel entries are positions 2..6. Record
    // it the way the factory does (a real SetBackgroundFlag apply), because the
    // gate asks "does this doc HAVE a baseline", not "assume it does".
    recordTestBaseline(DOC_ID);
    let cursor = 6;
    const applyNativeUndo = () => {
      if (cursor <= 1) return baselineStepJson();
      cursor -= 1;
      return pixelStepJson(cursor);
    };
    // The dispatch gate reads the facade's OWN stream: entries[] is the whole
    // history (6 = 1 baseline + 5 pixel) and cursor is the position in it.
    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "1";
        case "protocol_history_query_native":
          return historyQueryJson(cursor, 6);
        case "protocol_apply_command_native":
          return applyNativeUndo();
        default:
          throw `unexpected invoke in the pixel undo path: ${cmd}`;
      }
    });

    for (let i = 1; i <= 5; i++) {
      expect(await runFacadeExternalHandoff(ctx, "undo"), `undo ${i}`).toBe(true);
    }
    expect(cursor, "Rust cursor back at the baseline").toBe(1);
    expect(history.getUndoCount(), "TS depth drained with the cursor").toBe(0);
    expect(history.canUndo(), "the gate is closed").toBe(false);
    expect(history.getRedoCount(), "the twins are redoable, not binned").toBe(5);

    // The 6th press: nothing sits above the baseline, so the Rust cursor must
    // not move. Everything below is asserted against a snapshot taken first.
    const before6 = {
      cursor,
      undoCount: history.getUndoCount(),
      redoCount: history.getRedoCount(),
      calls: calls.length,
      uploads: (ctx.renderer.uploadSurfaceTiles as unknown as { mock: { calls: unknown[][] } }).mock.calls.length,
      // The depth probe is a READ, and reading is exactly what the gate is
      // supposed to do. What must not happen is a cursor command.
      cursorCommands: invokeMock.mock.calls.filter((c) => c[0] === "protocol_apply_command_native").length,
      invokeCount: invokeMock.mock.calls.length,
    };

    const handled6 = await runFacadeExternalHandoff(ctx, "undo");

    expect(handled6, "the press past the baseline is refused, not claimed").toBe(false);
    expect(cursor, "the baseline entry was NOT stepped").toBe(1);
    expect(history.getUndoCount(), "TS depth unchanged").toBe(before6.undoCount);
    expect(history.getRedoCount(), "no redo slot stolen").toBe(before6.redoCount);
    expect(calls.slice(before6.calls), "no pixel projection ran").toEqual([]);
    expect(
      (ctx.renderer.uploadSurfaceTiles as unknown as { mock: { calls: unknown[][] } }).mock.calls.length,
      "no tile upload ran",
    ).toBe(before6.uploads);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "protocol_apply_command_native").length,
      "the press never issued a cursor command",
    ).toBe(before6.cursorCommands);
    // The six-command census counts state-changing pixel commands. This path
    // never issues any (the cursor is the only executor), and a refused press
    // must not add one.
    const SIX_COMMAND_ALLOWLIST = [
      "rust_pixels_write_region",
      "rust_pixels_record_external",
      "rust_pixels_undo",
      "rust_pixels_redo",
      "apply_tile_patch",
      "rust_pixels_record_snapshot",
    ];
    expect(
      invokeMock.mock.calls
        .slice(before6.invokeCount)
        .map((c) => c[0])
        .filter((c) => SIX_COMMAND_ALLOWLIST.includes(c)),
      "no six-command census entry was produced",
    ).toEqual([]);

    // Round trip: five redos must return to the 5-stroke state, which requires
    // that no extra redo slot was banked by the 6th press.
    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "1";
        case "protocol_history_query_native":
          return historyQueryJson(cursor, 6);
        case "protocol_apply_command_native":
          cursor += 1;
          return pixelStepJson(cursor);
        default:
          throw `unexpected invoke in the pixel redo path: ${cmd}`;
      }
    });

    for (let i = 1; i <= 5; i++) {
      expect(await runFacadeExternalHandoff(ctx, "redo"), `redo ${i}`).toBe(true);
    }
    expect(cursor, "back at the 5-stroke position").toBe(6);
    expect(history.getUndoCount(), "five twins back on the undo stack").toBe(5);
    expect(history.getRedoCount(), "nothing left to redo").toBe(0);
    expect(history.canUndo()).toBe(true);
    expect(history.canRedo()).toBe(false);
  });

  // ── over-gating guards ──────────────────────────────────────────────────
  // The dispatch gate must refuse ONLY the open baseline. Everything else in
  // the Rust stream is user work, including entries that deliberately have no
  // TS twin (routed canvas ops, transforms, External steps) - a document whose
  // only work was a routed resize has canUndo() === false, so a TS-gate
  // implementation would swallow that undo. See routedCanvasUndo.wiring.test.ts
  // driving exactly that shape with an empty TS stack.

  it("a facade-only entry above the baseline still steps: no TS twin, must not be gated", async () => {
    const calls: Calls = [];
    // TS store EMPTY - this is the routed-canvas shape. The gate must still let
    // Rust undo its own entry.
    const history = new CommandHistory();
    const surface = makeSurface(calls);
    const engine = makeEngine("L1", surface, calls);
    const ctx = makeCtx(engine, calls, history);
    expect(history.canUndo(), "PROBE the TS store is empty here").toBe(false);

    // NO baseline recorded: this document never got one, so the undo floor is 0
    // and every entry in the stream is user work. That is the over-gating guard.
    let cursor = 3; // a routed canvas entry + a Native entry + one more
    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "1";
        case "protocol_history_query_native":
          return historyQueryJson(cursor, 3);
        case "protocol_apply_command_native":
          cursor -= 1;
          return JSON.stringify({
            documentVersion: cursor,
            delta: {
              baseVersion: cursor - 1,
              version: cursor,
              changes: [{ kind: "upsert", layer: { id: "L1", name: "L1", width: 4, height: 4, opacity: 1, visible: true } }],
            },
          });
        default:
          throw `unexpected invoke: ${cmd}`;
      }
    });

    expect(await runFacadeExternalHandoff(ctx, "undo"), "a user entry above the baseline is claimed").toBe(true);
    expect(cursor, "the cursor stepped that entry").toBe(2);
    expect(history.canUndo(), "PROBE nothing was drained (no twin), as required").toBe(false);
  });

  it("an External entry with no TS twin still steps the cursor", async () => {
    const calls: Calls = [];
    const history = new CommandHistory();
    const surface = makeSurface(calls);
    const engine = makeEngine("L1", surface, calls);
    const ctx = makeCtx(engine, calls, history);

    let cursor = 2;
    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "1";
        case "protocol_history_query_native":
          return historyQueryJson(cursor, 2);
        case "protocol_apply_command_native":
          cursor -= 1;
          // status "external" is how the walker reports a legacy host entry; the
          // artifact's gradient case (case 5) is exactly this shape.
          return JSON.stringify({
            documentVersion: cursor,
            delta: { baseVersion: cursor - 1, version: cursor, changes: [] },
            status: "external",
            externalSeq: 1,
          });
        default:
          throw `unexpected invoke: ${cmd}`;
      }
    });

    // The External branch calls confirmExternalCursor, mocked to ok at module scope.
    // It returns FALSE on purpose here: an External step with an empty delta
    // clears the barrier but must NOT claim the model restore was finished
    // (see the branch comment in facadeHistoryHandoff). What this test owns is
    // that the gate let the step through at all - the cursor moved.
    const handled = await runFacadeExternalHandoff(ctx, "undo");

    expect(cursor, "the External entry above the baseline still stepped the cursor").toBe(1);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "protocol_apply_command_native").length,
      "a cursor command was issued",
    ).toBe(1);
    // Documented existing behaviour, pinned so a future change is deliberate.
    expect(handled, "an empty-delta External step does not claim the restore").toBe(false);
  });

  it("a depth probe failure never blocks an undo (fail-open, not fail-closed)", async () => {
    const calls: Calls = [];
    const history = new CommandHistory();
    const surface = makeSurface(calls);
    const engine = makeEngine("L1", surface, calls);
    const ctx = makeCtx(engine, calls, history);

    let cursor = 2;
    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "1";
        case "protocol_history_query_native":
          // The real command rejects with a bare string on an unknown doc.
          throw "document not open: docPixelHandoff";
        case "protocol_apply_command_native":
          cursor -= 1;
          return JSON.stringify({
            documentVersion: cursor,
            delta: {
              baseVersion: cursor - 1,
              version: cursor,
              changes: [{ kind: "upsert", layer: { id: "L1", name: "L1", width: 1, height: 1, opacity: 1, visible: true } }],
            },
          });
        default:
          throw `unexpected invoke: ${cmd}`;
      }
    });

    expect(await runFacadeExternalHandoff(ctx, "undo"), "a probe failure must not swallow the undo").toBe(true);
    expect(cursor, "the cursor still stepped").toBe(1);
  });
});
