/**
 * Ownership of the brush pixel path: Rust records it, and at most one site
 * moves the Rust cursor for one undo/redo step.
 *
 * TRANSITIONAL AUDIT (recorded, 2026-10-02). Every case below that names
 * `photrez.rustPixels` pins the FLAG's behaviour and is transitional: brush,
 * eraser, bucket, seeded fill, gradient and delete-pixels became unconditionally
 * Rust, deleting five of the flag's six production subjects. Exactly one read
 * survives - the undo/redo cursor-step arming in `CommandHistory.stepRustCursor`
 * (it used to be `rustOwned || rustPixelsFlag` inside the tile branch of
 * useEditorCommands) - so the flag matrix below is now the ONLY
 * flag-dependent coverage in the tree and must be DELETED when that last read
 * retires. The per-case TRANSITIONAL markers name the flag they pin; the
 * permanent replacements are `ownerConvergence.test.ts` (do the two live owners
 * agree?) and `pixelWriterCensus.test.ts` (is every writer declared?), neither of
 * which reads the flag.
 *
 * Part 1 - the TILE/PIXEL undo/redo routing matrix.
 * One pop of a `CommandHistory` entry is ONE Rust cursor step, fired by the pop
 * itself (`stepRustCursor`) whenever Rust recorded that entry; the tile branch
 * only READS that step's tiles. All eight flag x gate x runtime combinations are
 * pinned below, and they resolve on the flag alone - see the comment on ROWS for
 * why the gate cannot arm a step for this entry shape. A metadata-only step
 * carries no tile patches, so the tile branch is unreachable for it; the separate
 * block below covers what its pop does instead.
 *
 * Census reads go through flushPixelInvokeCensus(), which awaits in-flight
 * invokes first; a sleep is not a drain.
 *
 * Part 2 - Rust-owned entries: the TS twin is a cursor token, so one undo
 * fetches the pixels from Rust and never replays the memento it carries.
 *
 * Part 3 - the default-path gate: the brush commit path must not read
 * photrez.rustPixels at all, so a real pointer chain with the key absent still
 * reaches rust_pixels_write_region.
 *
 * Part 4 - the census oracle counterexample: history can grow through a command
 * the six-command census does not count.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import { ImageData as NodeImageData } from "canvas";
import { invoke } from "@tauri-apps/api/core";
import { flushPixelInvokeCensus, getPixelCensusSnapshot } from "@/lib/protocol/pixelInvokeCensus";
import { getPixelHistoryDepth } from "@/lib/protocol/pixelHistoryDepth";
import { invokePixelCommand } from "@/lib/protocol/bridge";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { WorkspaceManager } from "@/engine/workspace";
import { CommandHistory } from "@/engine/history";
import type { HistoryTilePatches } from "@/engine/history";
import type { DocumentModel } from "@/engine/types";
import { useEditorCommands } from "../useEditorCommands";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { CanvasViewport } from "../canvas/CanvasViewport";
import { c4PendingCommits, flushC4Commits } from "../useBrushOverlay";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";

// The runtime detector feeds historyBridgeEnabled() and the undo/redo gate.
vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(),
}));

// The undo/redo tile path dynamic-imports this module, so the mock intercepts
// the dynamic import too and `invoke` is the shared spy.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(() => Promise.resolve("0.0.0")),
}));

// The undo path checks hasFacadeOwnedLayers() (Rust-owned history). Default off.
// Real exports stay reachable: the mounted viewport below needs the actual
// DocumentEngine, while these two predicates stay stubbed for the matrix.
vi.mock("@/engine/document", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/engine/document")>();
  return {
    ...actual,
    hasFacadeOwnedLayers: vi.fn(() => false),
    isFacadeOwnedLayer: vi.fn(() => false),
  };
});

// -- Viewport boundaries the mounted CanvasViewport needs (allowed mocks: the
//    render/viewport layer only). useBrushOverlay, useCanvasPointerTools and
//    the region producer stay REAL so the pointer chain below is a real one.
vi.mock("../canvas/useViewportRenderer", () => ({
  useViewportRenderer: () => ({
    isFitTransition: () => false,
    fitToScreenAndRender: vi.fn(),
    resizeRenderer: vi.fn(),
  }),
}));
vi.mock("../canvas/usePanNavigation", () => ({
  usePanNavigation: () => ({
    isSpacePressed: () => false,
    setIsSpacePressed: vi.fn(),
    isPanning: () => false,
    setIsPanning: vi.fn(),
    stopMomentum: vi.fn(),
    handleWheel: vi.fn(),
    onViewportPointerDown: vi.fn(),
    onViewportPointerMove: vi.fn(),
    onViewportPointerUp: vi.fn(),
    onViewportPointerCancel: vi.fn(),
    onViewportLostPointerCapture: vi.fn(),
  }),
}));
vi.mock("../canvas/useCanvasDerivedState", () => ({
  useCanvasDerivedState: () => ({ cropSnapTargets: () => [] }),
}));
vi.mock("../canvas/useCanvasKeyboard", () => ({
  useCanvasKeyboard: vi.fn(),
}));

// -- jsdom shims (jsdom has no ImageData / OffscreenCanvas / createImageBitmap;
//    node-canvas backs <canvas>.getContext("2d")). FakeImageData extends the
//    node-canvas class because its 2D context rejects foreign ImageData
//    instances, and widens the constructor to any ArrayLike<number> - which is
//    how Rust tile payloads arrive.
class FakeImageData extends NodeImageData {
  constructor(data: ArrayLike<number> | number, width?: number, height?: number) {
    if (typeof data === "number") {
      super(data, width as number);
    } else {
      const bytes = data instanceof Uint8ClampedArray ? data : Uint8ClampedArray.from(data);
      super(bytes as Uint8ClampedArray<ArrayBuffer>, width as number, height as number);
    }
  }
}
if (typeof (globalThis as { ImageData?: unknown }).ImageData === "undefined") {
  (globalThis as { ImageData?: unknown }).ImageData = FakeImageData;
}
if (typeof (globalThis as { OffscreenCanvas?: unknown }).OffscreenCanvas === "undefined") {
  const stubOffscreen = function OffscreenCanvas(w: number, h: number) {
    const el = document.createElement("canvas");
    el.width = w;
    el.height = h;
    (el as HTMLCanvasElement & { transferToImageBitmap: () => ImageBitmap; close: () => void }).transferToImageBitmap =
      () => el as unknown as ImageBitmap;
    (el as HTMLCanvasElement & { transferToImageBitmap: () => ImageBitmap; close: () => void }).close = () => {};
    return el;
  };
  (globalThis as { OffscreenCanvas?: unknown }).OffscreenCanvas = stubOffscreen;
}
if (typeof (globalThis as { createImageBitmap?: unknown }).createImageBitmap === "undefined") {
  (globalThis as { createImageBitmap?: unknown }).createImageBitmap = async (source: unknown) => source;
}

const GATE_KEY = "photrez.historyBridge";
const RUST_PIXELS_KEY = "photrez.rustPixels";

/**
 * The tile memento exactly as `CommandHistory` records it. `rustOwned` is the
 * real field (see `HistoryTilePatches`): set by the commit that
 * rust_pixels_write_region already recorded in Rust, which makes the twin a
 * cursor token for the step rather than a pixel source.
 */
type WirePatch = HistoryTilePatches;

const makePatches = (): WirePatch => ({
  layerId: "l1",
  surfaceWidth: 10,
  surfaceHeight: 10,
  before: [],
  after: [],
});

const tile = (x: number, y: number, rgba: number[]) => ({
  x,
  y,
  width: 1,
  height: 1,
  data: new Uint8ClampedArray(rgba),
});

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

/**
 * A REAL `CommandHistory` holding one entry, because the pop is what owns the
 * Rust cursor step (`CommandHistory.stepRustCursor`). A hand-rolled `undo()`
 * that returned a snapshot without a pop would make every count below vacuous:
 * the step could not happen at all.
 * `patches` null = a metadata-only entry (no `imperative`), which is exactly the
 * shape the tile branch can never reach.
 */
function makeHistory(patches: WirePatch | null) {
  const history = new CommandHistory();
  history.attachDocIdGetter(() => "doc-1");
  history.commit(MODEL, "Op", patches ?? undefined);
  return history;
}

/** Editor context + engine the hook body and the metadata path consume. */
function makeEditorContext(
  history: ReturnType<typeof makeHistory>,
  engine: Record<string, unknown>,
) {
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => "doc-1",
      notifyVisualChange: vi.fn(),
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
    getLayers: () => [] as { id: string }[],
    restore: vi.fn(),
    snapshot: () => ({ layers: [], activeLayerId: null }),
    getPaintSurface: () => null,
  };
}

// Two turns: the cursor-sync site does its own dynamic import after the first.
const flush = async () => {
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
};

function countInvokes(...commands: string[]): number {
  return (vi.mocked(invoke).mock.calls as [string][]).filter(([cmd]) =>
    commands.includes(cmd),
  ).length;
}

type Row = {
  rustPixels: "0" | "1";
  bridge: "0" | "1";
  tauri: boolean;
  expected: number;
};

// TRANSITIONAL (photrez.rustPixels gating; delete with the flag).
//
// The matrix collapsed from eight rows to two, and that collapse IS the finding.
// For a TS-OWNED tile entry (`imperative` without `rustOwned`) the bridge's only
// recording arm is `apply_tile_patch`, and the real command REJECTS the host's
// tile shape - `TileUploadLike` {x, y, width, height, data} against
// `TilePatchWire` {x, y, w, h, data}, no serde alias - so it mints no Pixel entry
// (proved in Rust by `host_tile_shape_is_rejected_at_the_wire_and_records_nothing`,
// and honoured in `historyCursorParity.wiring.test.ts`'s emulator). No Rust entry
// means no cursor step, so neither the bridge gate nor the runtime can arm one,
// and the flag is the only thing left that can. The bridge x runtime dimension
// for this entry shape is not "uncovered": it is vacuous, and the metadata block
// below is where the bridge genuinely decides.
const ROWS: Row[] = [
  { rustPixels: "1", bridge: "0", tauri: true, expected: 1 },
  { rustPixels: "1", bridge: "1", tauri: true, expected: 1 },
  { rustPixels: "1", bridge: "0", tauri: false, expected: 1 },
  { rustPixels: "1", bridge: "1", tauri: false, expected: 1 },
  { rustPixels: "0", bridge: "1", tauri: true, expected: 0 },
  { rustPixels: "0", bridge: "1", tauri: false, expected: 0 },
  { rustPixels: "0", bridge: "0", tauri: true, expected: 0 },
  { rustPixels: "0", bridge: "0", tauri: false, expected: 0 },
];

function applyRowFlags(flags: { rustPixels: "0" | "1"; bridge: "0" | "1" }): void {
  localStorage.setItem(RUST_PIXELS_KEY, flags.rustPixels);
  localStorage.setItem(GATE_KEY, flags.bridge);
}

// TRANSITIONAL (photrez.rustPixels gating; delete with the flag). The flag is
// the subject of every row, so the whole matrix dies with it.
describe("TRANSITIONAL TILE/PIXEL undo/redo routing matrix (photrez.rustPixels x bridge gate x runtime)", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  for (const row of ROWS) {
    const label =
      `rustPixels=${row.rustPixels} historyBridge=${row.bridge} ` +
      `runtime=${row.tauri ? "tauri" : "jsdom"}`;

    it(`${label}: exactly ${row.expected} cursor-sync invoke per direction`, async () => {
      applyRowFlags(row);
      vi.mocked(isTauriRuntime).mockReturnValue(row.tauri);
      const engine = makeEngine();
      mockUseEditor(makeEditorContext(makeHistory(makePatches()), engine));
      vi.mocked(invoke).mockResolvedValue({ tiles: [], epoch: 1, version: 1 });
      // Drain the commit's fire-and-forget record before reading the baseline.
      await flush();
      const censusBefore = (await flushPixelInvokeCensus()).entries.length;

      const commands = useEditorCommands(() => {});
      commands.undo();
      await flush();
      expect(countInvokes("rust_pixels_undo"), "undo fires only its own command").toBe(
        row.expected,
      );
      expect(countInvokes("rust_pixels_redo"), "undo must not fire redo").toBe(0);

      vi.mocked(invoke).mockClear();
      commands.redo();
      await flush();
      expect(countInvokes("rust_pixels_redo"), "redo fires only its own command").toBe(
        row.expected,
      );
      expect(countInvokes("rust_pixels_undo"), "redo must not fire undo").toBe(0);

      const census = await flushPixelInvokeCensus();
      const delta = census.entries
        .slice(censusBefore)
        .filter(
          (entry) =>
            entry.command === "rust_pixels_undo" || entry.command === "rust_pixels_redo",
        );
      expect(delta.map((entry) => entry.command)).toEqual(
        row.expected === 1 ? ["rust_pixels_undo", "rust_pixels_redo"] : [],
      );
      expect(delta.every((entry) => entry.phase === "resolved")).toBe(true);
    });
  }
});

// A metadata-only step carries no tile patches, so it never reaches the tile
// branch. Its cursor step belongs to the POP (CommandHistory.stepRustCursor),
// which fires exactly when Rust recorded an entry for it - the bridge - and
// never otherwise. Before that, the step lived in the tile branch, so these rows
// read 0 for a bridge-ON metadata pop: the drift this effort fixed.
describe("a metadata-only pop steps the Rust cursor exactly when Rust recorded the step", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  const combos = [
    // Bridge ON: `commit` appended an External entry for this metadata step, so
    // the pop has a Rust counterpart and must consume it.
    { rustPixels: "1", bridge: "1", tauri: true, expected: 1 },
    { rustPixels: "0", bridge: "1", tauri: true, expected: 1 },
    // Bridge OFF: Rust holds no entry for a metadata step at all. Stepping would
    // consume somebody else's, so the cursor must not move.
    { rustPixels: "1", bridge: "0", tauri: false, expected: 0 },
    { rustPixels: "1", bridge: "0", tauri: true, expected: 0 },
  ] as const;

  for (const combo of combos) {
    const label =
      `rustPixels=${combo.rustPixels} historyBridge=${combo.bridge} ` +
      `runtime=${combo.tauri ? "tauri" : "jsdom"}`;

    it(`${label}: ${combo.expected} cursor-sync invoke(s) while the model restore still runs`, async () => {
      applyRowFlags(combo);
      vi.mocked(isTauriRuntime).mockReturnValue(combo.tauri);
      const engine = makeEngine();
      mockUseEditor(makeEditorContext(makeHistory(null), engine));
      vi.mocked(invoke).mockResolvedValue({ tiles: [], epoch: 1, version: 1 });
      await flush();
      const censusBefore = (await flushPixelInvokeCensus()).entries.length;

      const commands = useEditorCommands(() => {});
      commands.undo();
      await flush();
      commands.redo();
      await flush();

      // Non-vacuity: the step completed its model restore, so the count below
      // means "the pop stepped (or did not)", not "the step aborted early".
      expect(engine.restore).toHaveBeenCalledTimes(2);
      expect(countInvokes("rust_pixels_undo")).toBe(combo.expected);
      expect(countInvokes("rust_pixels_redo")).toBe(combo.expected);

      const census = await flushPixelInvokeCensus();
      const delta = census.entries
        .slice(censusBefore)
        .filter(
          (entry) =>
            entry.command === "rust_pixels_undo" || entry.command === "rust_pixels_redo",
        );
      expect(delta.map((entry) => entry.command)).toEqual(
        combo.expected === 1 ? ["rust_pixels_undo", "rust_pixels_redo"] : [],
      );
    });
  }
});

// -- Rust-owned pixel entries: drained in lockstep, never re-painted ----------
// `rustOwned` marks a TS history entry whose Pixel step Rust ALREADY recorded
// (rust_pixels_write_region). Such a twin is a cursor token: it says a step
// exists, and the step's pixels are whatever Rust returns. Replaying its own
// memento would repaint the surface from bytes no store holds, so that fallback
// is refused for a Rust-owned entry - it stays available for every entry Rust
// does not own (the transitional rows below).

const MEMENTO_TILE = tile(0, 0, [7, 7, 7, 255]);
const RUST_TILE = { x: 3, y: 4, w: 1, h: 1, data: [1, 2, 3, 255] };

function uploadCalls(ctx: ReturnType<typeof makeEditorContext>): unknown[][] {
  return (ctx.renderer.uploadSurfaceTiles as unknown as { mock: { calls: unknown[][] } }).mock.calls;
}

describe("a Rust-owned pixel entry takes its pixels from Rust, never from its memento", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("one undo fetches from Rust with the flag at its default, and uploads only Rust's tiles", async () => {
    const ctx = makeEditorContext(
      makeHistory({ ...makePatches(), rustOwned: true, before: [MEMENTO_TILE], after: [MEMENTO_TILE] }),
      makeEngine(),
    );
    mockUseEditor(ctx);
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === "rust_pixels_undo"
        ? { tiles: [RUST_TILE], epoch: 4, version: 9 }
        : { version: 1 },
    );

    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();

    expect(countInvokes("rust_pixels_undo"), "the Rust-owned entry steps the Rust cursor once").toBe(1);
    const uploads = uploadCalls(ctx);
    expect(uploads).toHaveLength(1);
    const tiles = uploads[0][3] as { x: number; y: number }[];
    expect(tiles.map((t) => [t.x, t.y]), "Rust's tile, not the memento's").toEqual([[3, 4]]);
  });

  it("Rust has no tiles for the step: the stale memento is refused instead of replayed", async () => {
    const ctx = makeEditorContext(
      makeHistory({ ...makePatches(), rustOwned: true, before: [MEMENTO_TILE], after: [MEMENTO_TILE] }),
      makeEngine(),
    );
    mockUseEditor(ctx);
    vi.mocked(invoke).mockResolvedValue({ tiles: [], epoch: 4, version: 9 });

    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();

    expect(countInvokes("rust_pixels_undo")).toBe(1);
    expect(
      uploadCalls(ctx).every((call) => ((call[3] as unknown[]) ?? []).length === 0),
      "no stale memento bytes reached the surface or the GPU",
    ).toBe(true);
  });

  // TRANSITIONAL (photrez.rustPixels gating; delete with the flag). This case
  // pins flag-OFF memento replay: the fallback exists only while an entry can
  // lack `rustOwned`, which the unconditional-Rust ops make unreachable.
  it("TRANSITIONAL keeps photrez.rustPixels-OFF memento replay for entries Rust does not own (delete when the flag is retired)", async () => {
    const ctx = makeEditorContext(
      makeHistory({ ...makePatches(), before: [MEMENTO_TILE], after: [MEMENTO_TILE] }),
      makeEngine(),
    );
    mockUseEditor(ctx);
    vi.mocked(invoke).mockResolvedValue({ tiles: [], epoch: 4, version: 9 });

    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();

    expect(countInvokes("rust_pixels_undo"), "no Rust cursor step for a TS-owned entry").toBe(0);
    const uploads = uploadCalls(ctx);
    expect(uploads).toHaveLength(1);
    const tiles = uploads[0][3] as { x: number; y: number }[];
    expect(tiles.map((t) => [t.x, t.y]), "the memento still replays").toEqual([[0, 0]]);
  });
});

// -- Default-path gate (real brush pointer chain, flag never set) ------------
// The matrix above set photrez.rustPixels explicitly. This gate covers the state
// every user is actually in: the key is absent, so nothing may be gated on it.
// One real pointer chain must reach rust_pixels_write_region, record exactly one
// Pixel entry, and leave exactly one owner of the stroke (no apply_tile_patch
// twin). The verdict comes from the six-command census (one monotonic order per
// state-changing pixel command) plus the store emulator below, which counts raw
// IPC writes independently of the census.

const SIX_COMMAND_ALLOWLIST: readonly string[] = [
  "rust_pixels_write_region",
  "rust_pixels_record_external",
  "rust_pixels_undo",
  "rust_pixels_redo",
  "apply_tile_patch",
  "rust_pixels_record_snapshot",
];

const DOC_PX = 256;

type StoredWrite = { x: number; y: number; w: number; h: number; rgba: Uint8Array };

/**
 * In-memory Rust pixel store behind the mocked `invoke`. It answers the
 * commands the brush commit issues AND counts every rust_pixels_write_region
 * that reaches the IPC boundary, so "no late invoke" has an oracle that does
 * not depend on the census recorder itself.
 */
function createPixelStore(width: number, height: number) {
  const buffer = new Uint8ClampedArray(width * height * 4);
  const writes: StoredWrite[] = [];
  const rejections: string[] = [];
  // Per-document bookkeeping, mirroring the real store: the unified pixel
  // history stream lives on the document (crates/core/src/pixel_store.rs:715-734)
  // and SURVIVES a layer removal, which drops storage only
  // (crates/core/src/pixel_store.rs:197-200). That asymmetry is what the
  // oracle counterexample test below reads.
  // Entries carry their payload kind, because the unified stream also holds
  // metadata (native) entries the six-command census never sees: a document's
  // total depth is NOT the number of Pixel steps it can undo.
  const docs = new Map<string, { history: { kind: "pixel" | "native"; layerId: string }[]; removedLayers: Set<string> }>();
  const docOf = (id: string) => {
    const hit = docs.get(id);
    if (hit) return hit;
    const fresh = { history: [] as { kind: "pixel" | "native"; layerId: string }[], removedLayers: new Set<string>() };
    docs.set(id, fresh);
    return fresh;
  };
  let epoch = 0;

  const readTile = (x: number, y: number, w: number, h: number) => {
    const data: number[] = [];
    for (let row = 0; row < h; row++) {
      const src = ((y + row) * width + x) * 4;
      for (let i = 0; i < w * 4; i++) data.push(buffer[src + i]);
    }
    return { x, y, w, h, data };
  };

  vi.mocked(invoke).mockImplementation(async (command: string, args?: unknown) => {
    const a = (args ?? {}) as Record<string, unknown>;
    if (command === "rust_pixels_open_document") {
      docOf(String(a.docId));
      return null;
    }
    if (command === "rust_pixels_get_epoch") {
      // A removed layer rejects with the bare string Tauri surfaces
      // (paint_parity_cmds.rs:310-316 -> pixel_store.rs:396-400). Layers the
      // harness never modeled keep the resolve path: the path gates seed
      // the store from the TS bitmap without an init call, and changing that
      // answer would move their rehydrate branch.
      if (docs.get(String(a.docId))?.removedLayers.has(String(a.layerId))) {
        return Promise.reject(`layer not initialized: ${String(a.layerId)}`);
      }
      return epoch;
    }
    if (command === "rust_pixels_remove_layer") {
      docOf(String(a.docId)).removedLayers.add(String(a.layerId));
      return null;
    }
    if (command === "rust_pixels_history_depth") {
      const doc = docs.get(String(a.docId));
      if (!doc) return Promise.reject(`document not open: ${String(a.docId)}`);
      return {
        total_depth: doc.history.length,
        undo_depth: doc.history.length,
        redo_depth: 0,
        affected_layer_ids: [...new Set(doc.history.map((e) => e.layerId))].sort(),
      };
    }
    if (command === "protocol_apply_command_native") {
      // Mirrors protocol_native_cmds.rs:43-61: the doc must already be open,
      // and the envelope lands in the SAME unified history stream. This
      // command is deliberately NOT one of the six census'd ones
      // (pixelInvokeCensus.ts:30-37), which is the whole point below: history
      // can grow while the census reads zero.
      const storeDoc = docs.get(String(a.docId));
      if (!storeDoc) return Promise.reject(`document not open: ${String(a.docId)}`);
      let envelope: { command?: Record<string, unknown> };
      try {
        envelope = JSON.parse(String(a.envelopeJson)) as { command?: Record<string, unknown> };
      } catch (err) {
        return Promise.reject(`E_ENVELOPE_PARSE: ${String(err)}`);
      }
      // `Command` is internally tagged (crates/core/src/command.rs:58-60), so
      // the affected layer id rides at `command.id` - the same id the real
      // apply records on the history entry (document_core.rs begin_forward).
      const body = envelope.command;
      storeDoc.history.push({ kind: "native", layerId: typeof body?.id === "string" ? body.id : "" });
      return JSON.stringify({ documentVersion: storeDoc.history.length, delta: {} });
    }
    if (command === "rust_pixels_init") {
      buffer.set((a.bytes as Uint8Array).subarray(0, buffer.length));
      return null;
    }
    if (command === "rust_pixels_snapshot_layer") return [readTile(0, 0, width, height)];
    if (command === "rust_pixels_write_region") {
      const x = a.x as number;
      const y = a.y as number;
      const w = a.w as number;
      const h = a.h as number;
      const rgba = a.rgba as Uint8Array;
      if (!Number.isInteger(x) || !Number.isInteger(y) || !Number.isInteger(w) || !Number.isInteger(h) || w <= 0 || h <= 0) {
        rejections.push(`invalid dimensions ${x},${y},${w},${h}`);
        return Promise.reject("Invalid region dimensions");
      }
      if (x < 0 || y < 0 || x + w > width || y + h > height) {
        rejections.push(`region out of bounds ${x},${y},${w},${h}`);
        return Promise.reject("Region outside layer bounds");
      }
      if (rgba.length !== w * h * 4) {
        rejections.push(`length mismatch ${rgba.length} != ${w * h * 4}`);
        return Promise.reject("Invalid region length");
      }
      const storeDoc = docOf(String(a.docId));
      if (storeDoc.removedLayers.has(String(a.layerId))) {
        rejections.push(`layer not initialized: ${String(a.layerId)}`);
        return Promise.reject("layer not initialized or region out of bounds");
      }
      const before = readTile(x, y, w, h);
      for (let row = 0; row < h; row++) {
        const dst = ((y + row) * width + x) * 4;
        buffer.set(rgba.subarray(row * w, (row + 1) * w), dst);
      }
      const after = readTile(x, y, w, h);
      writes.push({ x, y, w, h, rgba });
      // One write_region opens one canonical Pixel history entry
      // (paint_parity_cmds.rs:318-322).
      storeDoc.history.push({ kind: "pixel", layerId: String(a.layerId) });
      epoch += 1;
      return { before: [before], after: [after], epoch, version: epoch };
    }
    // Every other allowlisted command (record_external, apply_tile_patch,
    // record_snapshot, undo, redo) only has to resolve so its census entry
    // reaches a terminal phase.
    return { version: epoch };
  });

  return {
    writes,
    rejections,
    seed: (pixels: Uint8ClampedArray) => buffer.set(pixels.subarray(0, buffer.length)),
    epoch: () => epoch,
    /** Entries of one payload kind in a document's unified stream. */
    entriesOfKind: (id: string, kind: "pixel" | "native") =>
      docs.get(id)?.history.filter((e) => e.kind === kind).length ?? 0,
    /** Pixel history entries the emulator recorded for a document. */
    historyDepth: (id: string) => docs.get(id)?.history.length ?? 0,
    /** FNV-1a over the canonical buffer - a stroke must move it. */
    hash: (): string => {
      let h = 0x811c9dc5;
      for (let i = 0; i < buffer.length; i++) {
        h ^= buffer[i];
        h = Math.imul(h, 0x01000193) >>> 0;
      }
      return h.toString(16).padStart(8, "0");
    },
  };
}

let setTool: (tool: string) => void = () => {};
let setFgColor: (color: string) => void = () => {};
let setZoom: (zoom: number) => void = () => {};
let setPan: (pan: { x: number; y: number }) => void = () => {};

const PathConsumer = () => {
  const editor = useEditor();
  setTool = editor.setActiveTool;
  setFgColor = editor.setFgColor;
  setZoom = editor.setZoom;
  setPan = editor.setPan;
  return null;
};

let store: ReturnType<typeof createPixelStore>;
let pathContainer: HTMLDivElement;
let pathDispose: (() => void) | undefined;
let rectStub: ReturnType<typeof vi.spyOn> | undefined;

function mountPathViewport() {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const workspace = new WorkspaceManager();
  // Partial renderer/scheduler doubles, typed permissively the same way
  // CanvasViewport.test.tsx does: EditorProvider's props require the full
  // backend surface, which these gates never touch.
  const renderer: Record<string, unknown> = {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    uploadSurfaceTiles: vi.fn(),
  };
  const scheduler: Record<string, unknown> = { requestRender: vi.fn() };
  const session = WorkspaceManager.createBlankDocument("doc-owner", "Owner", DOC_PX, DOC_PX);
  workspace.addDocument(session);
  pathDispose = render(
    () => (
      <EditorProvider
        workspace={workspace}
        renderer={renderer as never}
        scheduler={scheduler as never}
      >
        <PathConsumer />
        <CanvasViewport />
      </EditorProvider>
    ),
    container,
  );
  pathContainer = container;
  return { session, container };
}

function makeLayerCanvas(fill: string): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = DOC_PX;
  canvas.height = DOC_PX;
  const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;
  ctx.fillStyle = fill;
  ctx.fillRect(0, 0, DOC_PX, DOC_PX);
  return canvas;
}

/** Paint layer + matching store seed, then install the cached paint surface. */
function preparePaintLayer(session: ReturnType<typeof WorkspaceManager.createBlankDocument>) {
  const layerId = session.engine.getLayers()[0].id;
  const bitmap = makeLayerCanvas("#ffffff");
  session.engine.setLayerImageBitmap(layerId, bitmap as unknown as ImageBitmap);
  session.engine.setActiveLayer(layerId);
  const pixels = (bitmap.getContext("2d") as CanvasRenderingContext2D).getImageData(
    0,
    0,
    DOC_PX,
    DOC_PX,
  ).data;
  store.seed(pixels);
  const surface = session.engine.getPaintSurface(layerId);
  if (!surface) throw new Error("paint surface not created (layer bitmap missing?)");
  return { layerId, surface };
}

function pathCanvas(): HTMLCanvasElement {
  const c = pathContainer.querySelector("canvas:not([data-overlay-canvas])") as HTMLCanvasElement | null;
  if (!c) throw new Error("viewport canvas not found");
  return c;
}

function firePointer(type: string, el: Element, clientX: number, clientY: number, pointerId = 11) {
  el.dispatchEvent(
    new PointerEvent(type, {
      bubbles: true,
      cancelable: true,
      button: 0,
      pointerId,
      clientX,
      clientY,
    }),
  );
}

async function tick(ms = 0) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

/** One real brush pointer chain: down -> move -> up, ending in a c4 enqueue. */
function brushChain(el: Element, from: [number, number], to: [number, number]) {
  firePointer("pointerdown", el, from[0], from[1]);
  firePointer("pointermove", el, to[0], to[1]);
  firePointer("pointerup", el, to[0], to[1]);
}

function setupPathHarness() {
  vi.mocked(invoke).mockReset();
  vi.mocked(isTauriRuntime).mockReturnValue(true);
  // History bridge armed: whatever history fires shows up in the census, so a
  // zero count can never hide behind a gate that was closed the whole time.
  localStorage.clear();
  localStorage.setItem(GATE_KEY, "1");
  localStorage.removeItem("photrez.tileCommit");
  localStorage.removeItem("photrez.canonicalCommit");
  store = createPixelStore(DOC_PX, DOC_PX);
  rectStub = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    left: 0, top: 0, right: DOC_PX, bottom: DOC_PX, width: DOC_PX, height: DOC_PX,
    x: 0, y: 0, toJSON: () => ({}),
  } as DOMRect);
  Element.prototype.setPointerCapture = vi.fn();
  Element.prototype.releasePointerCapture = vi.fn();
}

function teardownPathHarness() {
  pathDispose?.();
  pathDispose = undefined;
  pathContainer?.parentNode?.removeChild(pathContainer);
  rectStub?.mockRestore();
  rectStub = undefined;
  localStorage.clear();
}

describe("brush strokes are Rust-canonical with photrez.rustPixels at its default", () => {
  beforeEach(setupPathHarness);
  afterEach(teardownPathHarness);

  it("one pointer chain writes one region, records one Pixel entry, and moves the canonical bytes", async () => {
    // Default state for every user: the key was never set. The brush Rust path
    // is NOT gated on it, so this chain must reach rust_pixels_write_region.
    localStorage.removeItem(RUST_PIXELS_KEY);
    const { session } = mountPathViewport();
    await tick();
    setZoom(1);
    setPan({ x: 0, y: 0 });
    setTool("brush");
    setFgColor("#ff0000");
    preparePaintLayer(session);
    const canvas = pathCanvas();

    // Census entries are file-global: pin this test's own starting order so
    // every count below only sees what THIS chain recorded.
    const baseline = await flushPixelInvokeCensus();
    const startOrder = baseline.entries.length
      ? baseline.entries[baseline.entries.length - 1].order
      : 0;
    const seedHash = store.hash();

    brushChain(canvas, [40, 40], [70, 60]);
    expect(c4PendingCommits(), "the stroke enqueued a deferred Rust commit").toBe(1);
    await flushC4Commits();
    expect(c4PendingCommits(), "the commit drained").toBe(0);

    // Non-vacuity: nothing is left in flight, so the census read below cannot
    // race past a pending invoke.
    expect(getPixelCensusSnapshot().pending, "marker read needs zero in-flight invokes").toBe(0);

    // Independent oracle first (raw IPC writes, census or not).
    expect(store.writes.length, "exactly one rust_pixels_write_region reached the store").toBe(1);
    expect(
      store.entriesOfKind("doc-owner", "pixel"),
      "one Pixel entry in the Rust history (the stream also holds native entries the census never sees)",
    ).toBe(1);
    expect(store.hash(), "the canonical buffer now holds the stroke").not.toBe(seedHash);
    expect(store.rejections).toEqual([]);

    const census = await flushPixelInvokeCensus();
    expect(census.pending).toBe(0);
    const delta = census.entries.filter((e) => e.order > startOrder);
    expect(
      delta.filter((e) => e.command === "rust_pixels_write_region" && e.phase === "resolved"),
      "one resolved region write in the six-command census",
    ).toHaveLength(1);
    // One owner only: the TS twin records the entry, a second write must not.
    expect(delta.filter((e) => e.command === "apply_tile_patch")).toEqual([]);
    expect(delta.every((e) => SIX_COMMAND_ALLOWLIST.includes(e.command))).toBe(true);
  });
});
describe("oracle counterexample: removed-layer history the six-command census never counts", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    localStorage.clear();
  });
  afterEach(() => {
    localStorage.clear();
  });

  it("keeps the enablement event blocked: epoch rejects, depth reads 1 twice, census window reads 0", async () => {
    createPixelStore(DOC_PX, DOC_PX);
    const docId = "doc-oracle";
    const layerId = "layer-oracle";

    await invokePixelCommand("rust_pixels_open_document", { docId });

    // Census entries are file-global: pin this test's own starting order so
    // the zero below only sees what THIS window recorded.
    const baseline = await flushPixelInvokeCensus();
    const startOrder = baseline.entries.length
      ? baseline.entries[baseline.entries.length - 1].order
      : 0;

    // Seed one metadata history entry through the NON-census native apply -
    // the exact "census 0 + history PRESENT" counterexample shape
    // (pixelInvokeCensus.ts counts exactly six commands and
    // protocol_apply_command_native is not one of them).
    const seeded = await invokePixelCommand("protocol_apply_command_native", {
      docId,
      envelopeJson: JSON.stringify({
        contractVersion: 1,
        command: { type: "setBackgroundFlag", id: layerId },
      }),
    });
    expect(seeded, "the native apply resolved").toBeTruthy();

    await invokePixelCommand("rust_pixels_remove_layer", { docId, layerId });

    // The epoch probe rejects the removed layer with the bare string the Rust
    // command surfaces (paint_parity_cmds.rs:310-316 -> pixel_store.rs:396-400).
    await expect(
      invokePixelCommand("rust_pixels_get_epoch", { docId, layerId }),
    ).rejects.toBe(`layer not initialized: ${layerId}`);

    // Two identical read-only depth calls: same answer, still reporting the
    // entry that remove_layer left behind in the document-level stream
    // (pixel_store.rs:197-200 drops storage only; :715-734 is read-only).
    const depthA = await getPixelHistoryDepth(docId);
    const depthB = await getPixelHistoryDepth(docId);
    expect(depthA.total_depth, "total_depth > 0 after remove_layer").toBe(1);
    expect(depthB, "two identical read-only calls agree").toEqual(depthA);
    expect(depthA.affected_layer_ids, "the surviving entry still names the removed layer").toEqual([layerId]);

    const census = await flushPixelInvokeCensus();
    const censusWrites = census.entries.filter(
      (e) => e.order > startOrder && SIX_COMMAND_ALLOWLIST.includes(e.command),
    );
    expect(
      censusWrites.map((e) => e.command),
      "the six-command census reads zero over this window",
    ).toEqual([]);

    // The enablement-event precondition this counterexample defends: a zero
    // census must NEVER unblock the event on its own - it also needs an empty
    // Rust history, which the depth probe reports as 1 here.
    const eventUnblocked = censusWrites.length === 0 && depthA.total_depth === 0;
    expect(
      eventUnblocked,
      "census 0 does not unblock the event while total_depth > 0",
    ).toBe(false);

    // Recorder-aliveness after the zero: one harness-driven allowlisted command
    // lands, so the zero above came from "no census command ran", never from a
    // recorder that cannot record.
    const postOrder = census.entries.length ? census.entries[census.entries.length - 1].order : startOrder;
    await invokePixelCommand("apply_tile_patch", { docId, layerId, before: [], after: [] });
    const alive = await flushPixelInvokeCensus();
    expect(
      alive.entries.filter((e) => e.order > postOrder).map((e) => e.command),
      "the census recorder is alive",
    ).toEqual(["apply_tile_patch"]);
  });
});
