// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Bounds proof for the pixel bridge. Two claims, pinned against the recorded
// bounds, with no production change:
//
// 1. Per-commit full-document upload count. Every TS mutation the native
//    engine cannot observe re-pushes the whole canonical document through
//    `protocol_seed_canonical_native`. Recorded bound: a rapid duplicate
//    burst crosses the bridge exactly ONCE. Two mechanisms enforce it and
//    both are exercised here - the byte-identical payload guard in
//    canonicalSeed.repushCanonicalDocument skips the duplicate invoke and
//    slot write, and bridge.setCanonicalPending keeps a single last-writer-
//    wins slot so a duplicate re-fire of one commit registers no extra push.
//    Distinct commits stay at one upload each.
//
// 2. rust_pixels_write_region input bounds. The store validates before it
//    opens a history entry: the command rejects negative dims
//    (apps/desktop/src-tauri/src/paint_parity_cmds.rs), then
//    PixelStoreRegistry.write_region rejects zero dims, layer-bounds
//    overflow and byte-length mismatch (crates/core/src/pixel_store.rs), so
//    an invalid write must never reach it. These tests pin what production
//    actually sends: finite, positive dims inside the layer with a
//    length-matched payload, and NO write at all when nothing changed.
//
// Mock fidelity: the write handler below applies the same validation in the
// same order as the Rust store and rejects with a bare string, which is how
// Tauri v2 surfaces a Rust Err(String). Non-finite dims cannot deserialize
// into the command's i64 arguments, so they reject before any write on the
// real path too; the mock rejects them first for the same reason.

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { applyPaintBucketFill } from "@/components/editor/canvas/pointerTools/paintBucket";
import { syncFacadeVersionFromPixel } from "@/lib/protocol/facadeRegistry";
import { __resetNativeAuthorityForTests } from "../bridge";
import { repushCanonicalDocument, __resetCanonicalRepushForTests } from "../canonicalSeed";
import { WorkspaceManager } from "@/engine/workspace";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>;

// Partial mock: keep the real facade registry, record the pixel-commit
// version sync (facadeRegistry.syncFacadeVersionFromPixel).
vi.mock("@/lib/protocol/facadeRegistry", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/protocol/facadeRegistry")>();
  return { ...actual, syncFacadeVersionFromPixel: vi.fn() };
});

// Self-contained localStorage so this file runs in the node project too.
if (typeof (globalThis as { localStorage?: unknown }).localStorage === "undefined") {
  const __ls = new Map<string, string>();
  (globalThis as { localStorage: Storage }).localStorage = {
    getItem: (k: string) => (__ls.has(k) ? __ls.get(k) : null),
    setItem: (k: string, v: string) => {
      __ls.set(k, String(v));
    },
    removeItem: (k: string) => {
      __ls.delete(k);
    },
    clear: () => __ls.clear(),
    key: (i: number) => Array.from(__ls.keys())[i] ?? null,
    get length() {
      return __ls.size;
    },
  } as Storage;
}

// jsdom lacks ImageData; floodFill mutates .data in place.
class FakeImageData {
  data: Uint8ClampedArray;
  width: number;
  height: number;
  constructor(data: Uint8ClampedArray | number, w?: number, h?: number) {
    if (typeof data === "number") {
      this.width = data;
      this.height = w!;
      this.data = new Uint8ClampedArray(data * w! * 4);
    } else {
      this.width = w!;
      this.height = h!;
      this.data = data;
    }
  }
}
(globalThis as Record<string, unknown>).ImageData = FakeImageData;

const LAYER_W = 8;
const LAYER_H = 8;
const DOC_ID = "doc1";
const LAYER_ID = "L1";

const flush = () => new Promise<void>((r) => setTimeout(r, 0));

const seedInvokeCount = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "protocol_seed_canonical_native").length;

const writeCalls = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "rust_pixels_write_region");

// Routes every native command the re-push probe exercises, mirroring the
// established canonical re-push harness.
function routeNative(): void {
  const open = new Set<string>();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const docId = (args.docId as string) ?? "default";
    switch (cmd) {
      case "rust_pixels_open_document":
        open.add(docId);
        return undefined;
      case "protocol_seed_native":
        if (!open.has(docId)) throw `document not open: ${docId}`;
        return JSON.stringify({ version: 0, layers: [] });
      case "protocol_seed_canonical_native":
        if (!open.has(docId)) throw `document not open: ${docId}`;
        return null;
      case "protocol_snapshot_native":
        return JSON.stringify({ version: 0, layers: [] });
      case "protocol_version_native":
        return 0;
      case "protocol_register_adapter_native":
        return null;
      case "protocol_apply_command_native":
        return JSON.stringify({ documentVersion: 1, delta: { baseVersion: 0, version: 1, changes: [] }, status: "ok" });
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

// Opens a real blank document (open-path layer + canonical seeds) and
// isolates the re-push traffic under test.
async function openDoc(docId: string) {
  const wm = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(docId, "D", LAYER_W * 100, LAYER_H * 75);
  wm.addDocument(session);
  await flush();
  invokeMock.mockClear();
  return session;
}

// Same validation order as the Rust write path: negative/zero/overflow dims
// and byte length are all rejected BEFORE any state changes.
function rejectInvalidWrite(args: Record<string, unknown>): void {
  const x = args.x as number;
  const y = args.y as number;
  const w = args.w as number;
  const h = args.h as number;
  const rgba = args.rgba as Uint8Array;
  const reject = (msg: string): never => {
    throw msg;
  };
  if (![x, y, w, h].every((v) => Number.isFinite(v))) {
    // Real path: NaN/Infinity cannot deserialize into the command's i64 args.
    return reject("invalid argument type: expected integer");
  }
  if (x < 0 || y < 0 || w === 0 || h === 0) return reject(`negative region dimensions: ${w}x${h}`);
  if (x + w > LAYER_W || y + h > LAYER_H) {
    return reject(`layer not initialized or region out of bounds: ${LAYER_ID}`);
  }
  if (rgba.byteLength !== w * h * 4) {
    return reject(`layer not initialized or region out of bounds: ${LAYER_ID}`);
  }
}

function makeFillFakes() {
  const surface = {
    context: {
      putImageData: vi.fn(),
      getImageData: vi.fn((_x: number, _y: number, w: number, h: number) => new FakeImageData(w, h)),
    },
    pixelEpoch: 0,
    pixelVersion: 0,
  } as never as {
    context: { putImageData: ReturnType<typeof vi.fn>; getImageData: ReturnType<typeof vi.fn> };
    pixelEpoch: number;
    pixelVersion: number;
  };
  const commit = vi.fn();
  const uploadSurfaceTiles = vi.fn();
  const layer = {
    id: LAYER_ID,
    width: LAYER_W,
    height: LAYER_H,
    locked: false,
    visible: true,
    lockTransparency: false,
    transform: { scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false, x: 0, y: 0 },
  };
  const engine: Record<string, unknown> = {
    getActiveLayerId: () => LAYER_ID,
    getLayer: (id: string) => (id === LAYER_ID ? layer : null),
    getSelection: () => null,
    getPaintSurface: (id: string) => (id === LAYER_ID ? surface : null),
    snapshot: () => ({ __snap: true }),
    getLayerImageBitmap: vi.fn(),
    setLayerImageBitmap: vi.fn(),
  };
  const workspace: Record<string, unknown> = {
    getActiveEngine: () => engine,
    getActiveHistory: () => ({ commit }),
    getActiveDocumentId: () => DOC_ID,
  };
  const editor: Record<string, unknown> = {
    activeTool: () => "paintBucket",
    workspace,
    renderer: { uploadSurfaceTiles },
    scheduler: { requestRender: vi.fn() },
    fgColor: () => "#ff0000",
    fillTolerance: () => 0,
    fillContiguous: () => true,
  };
  const ctx = {
    editor,
    getDocCoords: () => ({ x: 1, y: 1 }),
    getCanvasRef: () => ({ current: null }),
  } as never as Parameters<typeof applyPaintBucketFill>[0];
  return { surface, commit, uploadSurfaceTiles, ctx };
}

function routeFill(writeResult: { epoch: number; version: number }): void {
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    if (cmd === "rust_pixels_get_epoch") return 0;
    if (cmd === "rust_pixels_snapshot_layer") {
      return [{ x: 0, y: 0, w: LAYER_W, h: LAYER_H, data: new Array(LAYER_W * LAYER_H * 4).fill(0) }];
    }
    if (cmd === "rust_pixels_write_region") {
      rejectInvalidWrite(args);
      return {
        before: [{ x: 0, y: 0, w: LAYER_W, h: LAYER_H, data: new Array(LAYER_W * LAYER_H * 4).fill(0) }],
        after: [{ x: 0, y: 0, w: LAYER_W, h: LAYER_H, data: new Array(LAYER_W * LAYER_H * 4).fill(255) }],
        epoch: writeResult.epoch,
        version: writeResult.version,
      };
    }
    return undefined;
  });
}

describe("full-document canonical upload bound (per commit)", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockReset();
    __resetNativeAuthorityForTests();
    __resetCanonicalRepushForTests();
    localStorage.setItem("photrez.facadeAuthority", "native");
    routeNative();
  });
  afterEach(() => {
    localStorage.clear();
    __resetNativeAuthorityForTests();
    __resetCanonicalRepushForTests();
  });

  it("a rapid duplicate re-push burst uploads the document exactly once", async () => {
    const docId = "docUploadBound";
    const session = await openDoc(docId);
    const engine = session.engine as never;

    // Five identical re-pushes: the payload guard must collapse them to ONE
    // protocol_seed_canonical_native invoke (recorded bound).
    const REPS = 5;
    const pushes = Array.from({ length: REPS }, () => repushCanonicalDocument(docId, engine));
    await Promise.all(pushes);

    expect(seedInvokeCount()).toBe(1);
    expect(seedInvokeCount()).toBeLessThanOrEqual(REPS);
  });

  it("a duplicate re-fire of one commit adds no upload (at most one per commit)", async () => {
    const docId = "docUploadDupFire";
    const session = await openDoc(docId);
    const engine = session.engine as unknown as { addLayer(name: string): void };

    const first = repushCanonicalDocument(docId, engine as never);
    // Same commit re-fired with an unchanged model -> byte-identical payload.
    const duplicate = repushCanonicalDocument(docId, engine as never);
    engine.addLayer("Extra");
    const second = repushCanonicalDocument(docId, engine as never);
    await Promise.all([first, duplicate, second]);

    // Two state-changing commits -> two uploads; the duplicate re-fire adds none.
    expect(seedInvokeCount()).toBe(2);
  });
});

describe("rust_pixels_write_region input bounds", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    vi.mocked(syncFacadeVersionFromPixel).mockClear();
    localStorage.setItem("photrez.rustPixels", "1");
  });
  afterEach(() => {
    localStorage.removeItem("photrez.rustPixels");
  });

  it("issues only finite, positive, in-bounds dims and syncs the returned version", async () => {
    const { commit, ctx } = makeFillFakes();
    routeFill({ epoch: 1, version: 1 });

    applyPaintBucketFill(ctx, { pointerId: 1 } as PointerEvent);

    await vi.waitFor(() => expect(writeCalls().length).toBe(1), { timeout: 2000 });

    const write = writeCalls()[0][1] as unknown as {
      x: number;
      y: number;
      w: number;
      h: number;
      rgba: Uint8Array;
    };
    expect([write.x, write.y, write.w, write.h].every((v) => Number.isFinite(v))).toBe(true);
    expect(write.x).toBeGreaterThanOrEqual(0);
    expect(write.y).toBeGreaterThanOrEqual(0);
    expect(write.w).toBeGreaterThan(0);
    expect(write.h).toBeGreaterThan(0);
    expect(write.x + write.w).toBeLessThanOrEqual(LAYER_W);
    expect(write.y + write.h).toBeLessThanOrEqual(LAYER_H);
    expect(write.rgba.byteLength).toBe(write.w * write.h * 4);

    // One user action -> one history commit once the write lands.
    await vi.waitFor(() => expect(commit).toHaveBeenCalledTimes(1), { timeout: 2000 });

    // The pixel commit's returned version reaches the facade version sync, so
    // the next facade command is not rejected with E_VERSION_MISMATCH.
    expect(vi.mocked(syncFacadeVersionFromPixel)).toHaveBeenCalledWith(DOC_ID, 1);
  });

  it("a fill that changes nothing issues no write at all", async () => {
    const { commit, ctx } = makeFillFakes();
    invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      if (cmd === "rust_pixels_get_epoch") return 0;
      if (cmd === "rust_pixels_snapshot_layer") {
        // Layer already holds the exact fill colour -> the flood changes nothing.
        const data = new Array(LAYER_W * LAYER_H * 4).fill(0);
        for (let i = 0; i < data.length; i += 4) {
          data[i] = 255;
          data[i + 3] = 255;
        }
        return [{ x: 0, y: 0, w: LAYER_W, h: LAYER_H, data }];
      }
      if (cmd === "rust_pixels_write_region") {
        rejectInvalidWrite(args);
        return {
          before: [{ x: 0, y: 0, w: LAYER_W, h: LAYER_H, data: new Array(LAYER_W * LAYER_H * 4).fill(0) }],
          after: [{ x: 0, y: 0, w: LAYER_W, h: LAYER_H, data: new Array(LAYER_W * LAYER_H * 4).fill(255) }],
          epoch: 1,
          version: 1,
        };
      }
      return undefined;
    });

    applyPaintBucketFill(ctx, { pointerId: 1 } as PointerEvent);
    // The canonical fill is fire-and-forget; give it time to reach the write.
    await new Promise((r) => setTimeout(r, 30));

    expect(writeCalls().length).toBe(0);
    expect(commit).not.toHaveBeenCalled();
  });
});
