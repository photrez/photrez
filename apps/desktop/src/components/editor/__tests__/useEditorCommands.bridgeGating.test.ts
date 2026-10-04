/**
 * Regression wiring test for the undo/redo cursor-sync gate.
 *
 * Coverage gap: historyBridge.wiring.test.ts only scopes history.ts — it CANNOT
 * catch a bypass where the undo/redo path calls rust_pixels_undo/rust_pixels_redo
 * UNGATED. This file drives the REAL undo/redo path (via the hook's returned
 * `undo`/`redo`) over a REAL `CommandHistory` — the pop owns the cursor step
 * (`CommandHistory.stepRustCursor`), so a fake history would make every
 * assertion here vacuous.
 *
 * A step fires only when Rust recorded the popped entry, and there are exactly
 * three ways that happens:
 *   1. the history bridge, for a METADATA entry — `rust_pixels_record_external`
 *      takes all-scalar args, so it really records;
 *   2. `imperative.rustOwned`, for a brush-shaped pixel entry — the canonical
 *      `rust_pixels_write_region` recorded it, whatever the bridge says (covered
 *      in `rustPixelPathOwnership.test.tsx`);
 *   3. TRANSITIONAL `photrez.rustPixels`, for a tile entry Rust does not own.
 *
 * Scenarios here:
 *   - default production (no flag, no gate, not Tauri) → NEVER called;
 *   - bridge ON + Tauri, METADATA entry → called once per direction;
 *   - bridge ON, TS-OWNED tile entry → NOT called, because the bridge's tile arm
 *     (`apply_tile_patch`) is rejected by the real command, so it recorded
 *     nothing to step;
 *   - TRANSITIONAL flag ON, tile entry → called once (the flag's last behaviour).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { useEditorCommands } from "../useEditorCommands";
import { CommandHistory } from "@/engine/history";
import type { HistoryTilePatches } from "@/engine/history";
import type { DocumentModel } from "@/engine/types";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";

// Mock the Tauri-runtime detector (shared by history.ts::historyBridgeEnabled).
vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(),
}));

// Invoke spy: the hook dynamic-imports @tauri-apps/api/core inside the undo
// tile path, so vi.mock intercepts the dynamic import too. `invoke` is the spy.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// onMount of useEditorCommands calls listen()/getVersion() when the runtime
// detector is true — mock them so mounting is side-effect-safe.
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(() => Promise.resolve("0.0.0")),
}));

// The undo path checks hasFacadeOwnedLayers() (Rust-owned history). Default off.
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => false),
  isFacadeOwnedLayer: vi.fn(() => false),
}));

const GATE_KEY = "photrez.historyBridge";
const RUST_PIXELS_KEY = "photrez.rustPixels";

/** Minimal model the real `CommandHistory.commit` records. */
const MODEL = {
  id: "doc-1",
  name: "d",
  width: 10,
  height: 10,
  layers: [],
  activeLayerId: null,
  selection: null,
  viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
  dirty: false,
} as unknown as DocumentModel;

type WirePatch = HistoryTilePatches;

const makePatches = (): WirePatch => ({
  layerId: "l1",
  surfaceWidth: 10,
  surfaceHeight: 10,
  before: [],
  after: [],
});

/**
 * A REAL `CommandHistory` holding one entry, because the pop is what owns the
 * Rust cursor step (`CommandHistory.stepRustCursor`). A hand-rolled `undo()`
 * that returned a snapshot without a pop would make every assertion below
 * vacuous: the step could not happen at all, so a zero would prove nothing about
 * the gate and a one would prove nothing about the gate either.
 */
function makeHistory(patches: WirePatch | null) {
  const history = new CommandHistory();
  history.attachDocIdGetter(() => "doc-1");
  history.commit(MODEL, "Op", patches ?? undefined);
  return history;
}

/** The editor context the hook body + useLayerActions destructure need. */
function makeEditorContext(patches: WirePatch | null, engine: Record<string, unknown>) {
  // One history for the whole context: `getActiveHistory` is read on every
  // command, and a fresh CommandHistory per read would commit a new entry each
  // time, so the pop under test would not be the entry these cases set up.
  const history = makeHistory(patches);
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => "doc-1",
      notifyVisualChange: () => {},
    },
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
    scheduler: { requestRender: vi.fn() },
    activeDocumentId: () => "doc-1",
    layerTransformSession: () => null,
    setLayerTransformSession: vi.fn(),
    activeTool: () => "brush",
    cropInteractionMode: () => "modern",
    canCropUndo: () => false,
    canCropRedo: () => false,
    canModernCropUndo: () => false,
    canModernCropRedo: () => false,
    layers: () => [],
    activeLayerId: () => null,
    selectedLayerIds: () => [],
    setSelectedLayerIds: vi.fn(),
    toggleLayerSelection: vi.fn(),
    rangeSelectLayers: vi.fn(),
    selectedLayerId: () => null,
    setSelectedLayerId: vi.fn(),
    textEditSession: () => null,
    setTextEditSession: vi.fn(),
    setStatusLoadingMessage: vi.fn(),
    setShowExportDialog: vi.fn(),
    setShowPrintDialog: vi.fn(),
    setShowResizeDialog: vi.fn(),
  };
}

function makeEngine() {
  const layer = { id: "l1", basicAdjustment: undefined, imageBitmap: undefined };
  return {
    getActiveLayerId: () => "l1",
    getLayer: () => layer,
    snapshot: () => ({ layers: [], activeLayerId: null }),
    getPaintSurface: () => null,
  };
}

// Let the async restoreHistorySnapshot's `await import(...)` + invoke chain settle.
const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

// TRANSITIONAL - `photrez.rustPixels` gating. Every case below that names the
// flag pins the flag's ON behaviour, which survives only because one production
// read remains: the undo/redo tile-branch routing in useEditorCommands
// (`rustOwned || rustPixelsFlag`). The other five former subjects (brush,
// eraser, bucket, seeded fill, gradient) were deleted when those ops became
// unconditionally Rust, so this file is the ONLY remaining flag-dependent
// coverage and it must be DELETED when that last read retires - not kept "just in
// case". The permanent owner-agreement oracle is `ownerConvergence.test.ts`; the
// permanent writer enumeration is `pixelWriterCensus.test.ts`. Neither depends on
// this flag, and neither should be made to.
describe("useEditorCommands undo/redo cursor-sync gate — history bridge", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("default production: invoke is NEVER called with rust_pixels_undo/redo (bridge OFF)", async () => {
    // Defaults: rustPixels absent, bridge gate absent, isTauriRuntime false.
    mockUseEditor(makeEditorContext(makePatches(), makeEngine()));

    const commands = useEditorCommands(() => {});
    commands.undo();
    commands.redo();
    await flush();

    const undoRedoCalls = (vi.mocked(invoke).mock.calls as [string][]).filter(
      ([cmd]) => cmd === "rust_pixels_undo" || cmd === "rust_pixels_redo",
    );
    expect(undoRedoCalls).toHaveLength(0);
  });

  it("bridge gate ON in Tauri: a METADATA pop invokes once per direction", async () => {
    // A metadata entry is the bridge's ONE arm that really records:
    // `rust_pixels_record_external` takes all-scalar args, so serde accepts it
    // and Rust holds a matching External entry for the pop to consume. Its tile
    // counterpart is the next case.
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    mockUseEditor(makeEditorContext(null, makeEngine()));
    vi.mocked(invoke).mockResolvedValue({ tiles: [], epoch: 1, version: 1 });
    await flush();
    vi.mocked(invoke).mockClear();

    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();
    expect(invoke).toHaveBeenCalledWith("rust_pixels_undo", { docId: "doc-1", layerId: "" });

    vi.mocked(invoke).mockClear();
    commands.redo();
    await flush();
    expect(invoke).toHaveBeenCalledWith("rust_pixels_redo", { docId: "doc-1", layerId: "" });
  });

  it("bridge gate ON, TS-OWNED tile entry: NO invoke, because the bridge never recorded it", async () => {
    // The bridge's tile arm is `apply_tile_patch`, and the host sends
    // `TileUploadLike` {x, y, width, height, data} while the command deserializes
    // `TilePatchWire` {x, y, w, h, data} with no serde alias - serde rejects it
    // ("missing field `w`", pinned in Rust by
    // `host_tile_shape_is_rejected_at_the_wire_and_records_nothing`). No Pixel
    // entry exists, so the pop has nothing to consume and must not step: a step
    // here would eat the nearest entry Rust does hold.
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    mockUseEditor(makeEditorContext(makePatches(), makeEngine()));
    vi.mocked(invoke).mockResolvedValue({ tiles: [], epoch: 1, version: 1 });
    await flush();
    vi.mocked(invoke).mockClear();

    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();

    const undoRedoCalls = (vi.mocked(invoke).mock.calls as [string][]).filter(
      ([cmd]) => cmd === "rust_pixels_undo" || cmd === "rust_pixels_redo",
    );
    expect(undoRedoCalls, "the rejected record means there is no entry to step").toEqual([]);
  });

  // TRANSITIONAL (photrez.rustPixels gating; delete with the flag): pins flag-ON
  // tile-branch routing, the last behaviour that still reads the key.
  it("TRANSITIONAL rustPixels ON: the tile path fires exactly ONE undo/redo invoke and the census records it", async () => {
    localStorage.setItem(RUST_PIXELS_KEY, "1");
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    mockUseEditor(makeEditorContext(makePatches(), makeEngine()));
    vi.mocked(invoke).mockResolvedValue({ tiles: [], epoch: 1, version: 1 });
    // The commit's fire-and-forget `apply_tile_patch` reaches the census one
    // dynamic import later; drain it BEFORE reading the baseline, or it lands in
    // the undo's census slice and this case reads the commit, not the step.
    await flush();
    const censusBefore = (await flushPixelInvokeCensus()).entries.length;

    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();

    // rustPixels ON fires the tile-path cursor sync and skips the bridge-only
    // site, so one undo is exactly one Rust cursor step (no double step).
    const undoRedoCalls = (vi.mocked(invoke).mock.calls as [string][]).filter(
      ([cmd]) => cmd === "rust_pixels_undo" || cmd === "rust_pixels_redo",
    );
    expect(undoRedoCalls).toHaveLength(1);
    expect(undoRedoCalls[0][0]).toBe("rust_pixels_undo");

    const census = await flushPixelInvokeCensus();
    expect(census.entries.slice(censusBefore)).toEqual([
      { order: expect.any(Number), command: "rust_pixels_undo", phase: "resolved" },
    ]);
  });

  // TRANSITIONAL (photrez.rustPixels gating; delete with the flag): its last two
  // assertions assert the flag does NOT arm the bridge predicate, which is a
  // statement about flag independence and therefore dies with the flag.
  it("TRANSITIONAL historyBridgeEnabled predicate: default false, gate+tauri true, gate+non-tauri false", async () => {
    const { historyBridgeEnabled } = await import("@/engine/history");

    expect(historyBridgeEnabled()).toBe(false); // no gate, not tauri

    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    expect(historyBridgeEnabled()).toBe(false);

    vi.mocked(isTauriRuntime).mockReturnValue(true);
    expect(historyBridgeEnabled()).toBe(true);

    localStorage.removeItem(GATE_KEY);
    expect(historyBridgeEnabled()).toBe(false);

    // rustPixels is an independent flag — it must NOT flip the bridge predicate.
    localStorage.setItem(RUST_PIXELS_KEY, "1");
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    expect(historyBridgeEnabled()).toBe(true);
  });
});
