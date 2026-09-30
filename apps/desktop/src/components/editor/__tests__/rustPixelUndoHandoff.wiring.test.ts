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

function makeCtx(engine: unknown, calls: Calls) {
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveDocumentId: () => DOC_ID,
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
