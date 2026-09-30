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
