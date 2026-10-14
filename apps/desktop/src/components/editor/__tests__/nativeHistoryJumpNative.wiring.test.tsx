// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * The History-panel jump route against a real Pixel entry and a real barrier.
 *
 * The jump loop must read `handoffMovedCursor()` once per iteration, and the
 * pixel path must restore BYTES (the exact pre-stroke tile), not pass
 *     call counts on doubles; the barrier path must report BLOCKED, never
 *     success, and it must stop the jump instead of counting it as a step.
 *     Tauri v2 `invoke()` rejects with the bare error value - never resolves
 *     `{ok:false}` - so the mock rejects with the bare string, exactly as the
 *     real transport surfaces a Rust `Err(Value)`.
 */
import { describe, it, expect, vi, beforeAll, beforeEach, afterAll, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { CommandHistory } from "@/engine/history";
import { navigateHistoryNative } from "../nativeHistoryJump";
import { createNativeSeed, __resetNativeAuthorityForTests } from "@/lib/protocol/bridge";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const DOC_ID = "docJumpPixel";

// jsdom has no ImageData; install the real structural contract.
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

const BASELINE_ENTRY = {
  seq: 1,
  groupId: 1,
  origin: "native",
  label: "Set Background Flag",
  affectedLayerIds: [],
  versionBefore: 0,
  versionAfter: 1,
  memoryCostBytes: 0,
};
const PIXEL_ENTRY = {
  seq: 2,
  groupId: 2,
  origin: "native",
  label: "pixel",
  affectedLayerIds: ["L1"],
  versionBefore: 1,
  versionAfter: 2,
  memoryCostBytes: 1,
};

/** Emulator-shaped pre-stroke tile bytes the pixel entry hands back (mock
 *  fixture bytes, not a read of the pixel store). */
const PRE_STROKE_TILE = { x: 0, y: 0, w: 1, h: 1, data: [9, 8, 7, 255] };

function makeEngine(calls: string[], surface: unknown) {
  return {
    getId: () => DOC_ID,
    getLayer: (id: string) =>
      id === "L1" ? { id, width: 1, height: 1, imageBitmap: {} as ImageBitmap } : null,
    getLayers: () => [] as { id: string; imageBitmap: unknown }[],
    getPaintSurface: () => surface,
    ensureBitmapCurrent: vi.fn(async (docId: string, id: string) => {
      calls.push(`ensureBitmapCurrent:${docId}:${id}`);
    }),
    applyFacadeSnapshot: vi.fn(),
    restore: vi.fn(),
    snapshot: vi.fn(() => ({ layers: [] })),
  };
}

function makeSurface(calls: string[]) {
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

function makeHost(engine: unknown, history: CommandHistory) {
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => DOC_ID,
      notifyVisualChange: vi.fn(),
    },
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
    scheduler: { requestRender: vi.fn() },
  } as never;
}

describe("navigateHistoryNative over a native pixel stream", () => {
  beforeEach(() => {
    localStorage.clear();
    __resetNativeAuthorityForTests();
    invokeMock.mockReset();
  });

  afterEach(() => {
    localStorage.clear();
    __resetNativeAuthorityForTests();
    vi.restoreAllMocks();
  });

  it("a jump onto a Pixel entry restores the exact pre-stroke bytes", async () => {
    const calls: string[] = [];
    const surface = makeSurface(calls);
    const engine = makeEngine(calls, surface);
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_ID);
    let cursor = 2;
    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "rust_pixels_open_document":
          return undefined;
        case "protocol_seed_native":
          return JSON.stringify({ version: 0, layers: [] });
        case "protocol_seed_canonical_native":
          return null;
        case "protocol_snapshot_native":
          return JSON.stringify({ version: 0, layers: [] });
        case "protocol_version_native":
          return "2";
        case "protocol_register_adapter_native":
          return null;
        case "protocol_history_query_native":
          return JSON.stringify({
            cursor,
            lastSeq: 2,
            degradedHint: false,
            pendingExternal: null,
            entries: [BASELINE_ENTRY, PIXEL_ENTRY],
          });
        case "protocol_apply_command_native": {
          cursor -= 1;
          return JSON.stringify({
            documentVersion: 1,
            delta: { baseVersion: 1, version: 1, changes: [] },
            pixelPatches: { layerId: "L1", tiles: [PRE_STROKE_TILE], epoch: 7, version: 1 },
          });
        }
        default:
          throw `unexpected invoke: ${cmd}`;
      }
    });
    await createNativeSeed(DOC_ID, 0, []);

    // floor is derived from the stream: entries[0] is the baseline, so panel
    // index 0 targets cursor 1, one undo step from the current cursor 2.
    const result = await navigateHistoryNative(makeHost(engine, history), 0);

    expect(result.blocked).toBe(false);
    expect(result.completedSteps).toBe(1);
    const puts = calls.filter((c) => c.startsWith("putImageData:"));
    expect(puts).toEqual(["putImageData:1x1@0,0=9,8,7,255"]);
    expect(cursor).toBe(1);
    expect(calls).toContain(`ensureBitmapCurrent:${DOC_ID}:L1`);
  });

  it("a bare-string E_EXTERNAL_PENDING rejection stops the jump and reports blocked", async () => {
    const calls: string[] = [];
    const surface = makeSurface(calls);
    const engine = makeEngine(calls, surface);
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_ID);
    let applyCalls = 0;
    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "2";
        case "protocol_history_query_native":
          return JSON.stringify({
            cursor: 2,
            lastSeq: 2,
            degradedHint: false,
            pendingExternal: null,
            entries: [BASELINE_ENTRY, PIXEL_ENTRY],
          });
        case "protocol_apply_command_native":
          applyCalls += 1;
          // Tauri v2 rejects with the bare error value, never {ok:false}.
          throw "E_EXTERNAL_PENDING";
        default:
          throw `unexpected invoke: ${cmd}`;
      }
    });

    const warns: unknown[] = [];
    vi.spyOn(console, "warn").mockImplementation((...args: unknown[]) => {
      warns.push(args[0]);
    });

    const result = await navigateHistoryNative(makeHost(engine, history), 0);

    expect(applyCalls).toBe(1);
    expect(result.blocked).toBe(true);
    expect(result.completedSteps).toBe(0);
    // The handoff reports every rejection as "handled, cursor did not move", so
    // the jump's reason no longer names the barrier that caused the rejection.
    expect(result.reason).toContain("the native cursor did not move");
    expect(calls.filter((c) => c.startsWith("putImageData:"))).toEqual([]);
    expect(warns.some((w) => String(w).includes("blocked"))).toBe(true);
  });

  it("a non-barrier walker rejection is a handled no-op: the jump stops and no legacy fallback runs", async () => {
    const calls: string[] = [];
    const surface = makeSurface(calls);
    const engine = makeEngine(calls, surface);
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_ID);
    const earlier = { id: "docJumpPixel", name: "n" } as never;
    history.commit(earlier, "Add Layer");
    let applyCalls = 0;
    invokeMock.mockImplementation(async (cmd: string): Promise<unknown> => {
      switch (cmd) {
        case "protocol_version_native":
          return "2";
        case "protocol_history_query_native":
          return JSON.stringify({
            cursor: 2,
            lastSeq: 2,
            degradedHint: false,
            pendingExternal: null,
            entries: [BASELINE_ENTRY, PIXEL_ENTRY],
          });
        case "protocol_apply_command_native": {
          applyCalls += 1;
          throw "E_UNKNOWN_ROUTE";
        }
        default:
          throw `unexpected invoke: ${cmd}`;
      }
    });

    const result = await navigateHistoryNative(makeHost(engine, history), 0);

    // The walker was actually driven (the rejection is not swallowed before dispatch),
    // but the legacy TS pop it used to fall through to is retired. A TS entry is
    // available to pop - history.commit above recorded one - and is NOT consumed.
    expect(applyCalls).toBe(1);
    expect(engine.restore).not.toHaveBeenCalled();
    expect(result.blocked).toBe(true);
    expect(result.completedSteps).toBe(0);
    expect(result.reason).toContain("the native cursor did not move");
  });
});
