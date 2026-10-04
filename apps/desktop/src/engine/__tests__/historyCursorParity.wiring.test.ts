// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * TS-vs-Rust history-cursor parity: the MEASUREMENT.
 *
 * Two undo stacks exist per document - TypeScript's `CommandHistory` and Rust's
 * `ProtocolEngine` - and nothing in production compares them. This drives the
 * REAL `useEditorCommands` undo/redo (both production call sites, tile path and
 * snapshot path) over a real `CommandHistory`, with the Rust stream emulated, and
 * reports the parity verdict after every step.
 *
 * The Rust side is emulated, so the emulator follows `PixelStoreRegistry`
 * exactly:
 *   - `rust_pixels_record_external` appends an `External` entry, cursor = len;
 *   - `apply_tile_patch` appends a `Pixel` entry, cursor = len;
 *   - `rust_pixels_undo` / `rust_pixels_redo` MOVE THE CURSOR for both modelled
 *     kinds. `ProtocolEngine::undo_pixel` steps the cursor and bumps the version
 *     for an `External` tip (crates/core/src/history.rs:229-233) exactly as it
 *     does for a `Pixel` tip (history.rs:219-223); only the tile yield differs -
 *     an External step produces no tiles, and `PixelStoreRegistry::undo_pixel`
 *     collapses the empty `(None, None, None)` to `None` AFTER the cursor has
 *     already moved (crates/core/src/pixel_store.rs:730-747). So "produced no
 *     tiles" must never be modelled as "did not move", which is the mistake an
 *     earlier revision of this file made and what these tests now pin.
 *   - `Snapshot` / `Native` entries are NOT modelled: `undo_pixel` leaves the
 *     cursor alone for those (history.rs:239), and no command the emulator
 *     handles can record one.
 *   - `rust_pixels_history_tip` answers `{total_depth, undo_depth, redo_depth,
 *     undo_tip_kind, redo_tip_kind}` — snake_case fields and camelCase kind
 *     strings, the serialization `HistoryTip` / `PayloadKind` actually produce.
 *
 * A probe that cannot report divergence proves nothing, so both directions are
 * load-bearing: neuter the comparison in `classifyHistoryCursorParity` and the
 * divergence cases go RED; delete either production call to
 * `observeHistoryCursorParity` in `useEditorCommands` and the wiring cases go
 * RED; remove the probe's bridge gate and the gate case goes RED.
 *
 * Every driving case runs with the bridge ON, because the probe only reads while
 * the bridge is recording - with it off the two stacks are not two views of one
 * history (see history.ts:500; only paint steps reach Rust, via the ungated
 * canonical writer), which the one bridge-OFF case pins instead.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { CommandHistory } from "../history";
import type { HistoryTilePatches } from "../history";
import type { DocumentModel } from "../types";
import { useEditorCommands } from "@/components/editor/useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "@/components/editor/dialogs/DialogProvider";
import {
  classifyHistoryCursorParity,
  probeHistoryCursorParity,
  readRustHistoryTip,
  type RustHistoryTip,
} from "../historyCursorParity";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));
// The undo path checks hasFacadeOwnedLayers() (Rust-owned history). Default off.
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => false),
  isFacadeOwnedLayer: vi.fn(() => false),
}));

const GATE_KEY = "photrez.historyBridge";

// ── Rust stream emulator ─────────────────────────────────────────────────────

type TipKind = "pixel" | "external";

interface RustStream {
  entries: TipKind[];
  cursor: number;
}

/** Per-document stream, mirroring PixelStoreRegistry's per-doc history. */
const streams = new Map<string, RustStream>();

function streamFor(docId: string): RustStream {
  let s = streams.get(docId);
  if (!s) {
    s = { entries: [], cursor: 0 };
    streams.set(docId, s);
  }
  return s;
}

/** Record an entry the way `record_external` / `apply_pixel_patch` do. */
function record(docId: string, kind: TipKind): void {
  const s = streamFor(docId);
  s.entries.length = s.cursor; // a new entry drops the redo branch
  s.entries.push(kind);
  s.cursor = s.entries.length;
}

/**
 * Move the cursor one step. `ProtocolEngine::undo_pixel` steps it for a `Pixel`
 * tip (history.rs:219-223) AND for an `External` tip (history.rs:229-233); the
 * difference between the two is only the tile yield, which `undo_pixel` discards
 * for an External step AFTER the move (pixel_store.rs:745). So the cursor moves
 * for every kind this emulator can hold.
 */
function step(docId: string, direction: "undo" | "redo"): void {
  const s = streamFor(docId);
  const idx = direction === "undo" ? s.cursor - 1 : s.cursor;
  if (idx < 0 || idx >= s.entries.length) return;
  s.cursor = direction === "undo" ? s.cursor - 1 : s.cursor + 1;
}

/** The exact wire shape `rust_pixels_history_tip` serializes. */
function tipFor(docId: string): RustHistoryTip {
  const s = streamFor(docId);
  return {
    total_depth: s.entries.length,
    undo_depth: s.cursor,
    redo_depth: s.entries.length - s.cursor,
    undo_tip_kind: s.cursor > 0 ? (s.entries[s.cursor - 1] ?? null) : null,
    redo_tip_kind: s.entries[s.cursor] ?? null,
  };
}

// ── Harness (shaped like useEditorCommands.snapshotBridge.test.ts) ───────────

function gateOn(on: boolean) {
  vi.mocked(isTauriRuntime).mockReturnValue(on);
  if (on) localStorage.setItem(GATE_KEY, "1");
  else localStorage.removeItem(GATE_KEY);
}

const createMockModel = (name: string): DocumentModel => ({
  id: "doc-1",
  name,
  width: 10,
  height: 10,
  layers: [],
  activeLayerId: null,
  selection: null,
  viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
  dirty: false,
});

const makePatches = (): HistoryTilePatches => ({
  layerId: "l1",
  surfaceWidth: 10,
  surfaceHeight: 10,
  before: [],
  after: [],
});

/** Stateful engine following the real DocumentEngine snapshot/restore shape. */
function makeEngine() {
  let model: DocumentModel = createMockModel("live");
  return {
    getId: () => "doc-1",
    // The tile path's Rust cursor sync is gated on an active layer id
    // (useEditorCommands.ts:507), so it must report one for the cursor to move.
    getActiveLayerId: () => "l1",
    getLayer: () => null,
    getLayers: () => model.layers,
    snapshot: (): DocumentModel => ({ ...model, layers: [...model.layers] }),
    restore: (snap: DocumentModel) => {
      model = { ...snap, layers: [...snap.layers] };
    },
    getPaintSurface: () => null,
    getModel: () => model,
    ensureBitmapCurrent: vi.fn(),
    invalidatePaintSurface: vi.fn(),
  };
}

function makeEditorContext(engine: ReturnType<typeof makeEngine>, history: CommandHistory) {
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

/** Mount the production command hook over a real history + emulated Rust stream. */
function mountCommands() {
  const history = new CommandHistory();
  history.attachDocIdGetter(() => "doc-1");
  const engine = makeEngine();
  mockUseEditor(makeEditorContext(engine, history));
  return { history, engine, commands: useEditorCommands(() => {}) };
}

/** Read Rust's cursor through the production reader, then classify it. */
async function verdictFor(
  docId: string,
  history: CommandHistory,
): Promise<ReturnType<typeof classifyHistoryCursorParity>> {
  return (await probeHistoryCursorParity(docId, history.getUndoCount())).verdict;
}

const waitFor = async (pred: () => boolean, ms = 3000) => {
  const start = Date.now();
  while (!pred()) {
    if (Date.now() - start > ms) throw new Error("waitFor timeout");
    await new Promise((r) => setTimeout(r, 5));
  }
};

/** Wait until the emulated Rust stream for `docId` holds `count` entries. */
const waitForRust = async (docId: string, count: number) => {
  await waitFor(() => streamFor(docId).entries.length >= count);
  await settle();
};

/**
 * Drain the fire-and-forget bridge invokes. `commit` reaches Rust through a
 * dynamic `import()`, so one macrotask is not enough under parallel workers;
 * cases that assert a specific Rust cursor use `waitForRust` above.
 */
async function settle() {
  await new Promise<void>((r) => setTimeout(r, 0));
  await flushPixelInvokeCensus();
}

/** Count the invokes the emulator saw for `command`. */
const timesInvoked = (command: string): number =>
  vi.mocked(invoke).mock.calls.filter((c) => c[0] === command).length;

/** Only the probe's own divergence warns, not the hook's unrelated setup noise. */
const parityWarns = (): string[] =>
  vi
    .mocked(console.warn)
    .mock.calls.map((c) => String(c[0]))
    .filter((m) => m.includes("history-cursor-parity"));

/**
 * Fire the production undo/redo command and wait for the probe read it triggers.
 * `commands.undo`/`redo` are `() => execute("edit.undo")` - they return void and
 * `restoreHistorySnapshot` runs fire-and-forget, so completion is observed, not
 * awaited. The cursor read is the last thing each production path does, so its
 * arrival marks the step as done.
 */
async function driveStep(run: () => void): Promise<void> {
  const before = timesInvoked("rust_pixels_history_tip");
  run();
  await waitFor(() => timesInvoked("rust_pixels_history_tip") > before);
  await settle();
}

describe("history cursor parity: TS undo depth vs the Rust cursor", () => {
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    streams.clear();
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<
      typeof DialogProviderModule.useDialog
    >);
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(console, "info").mockImplementation(() => {});

    vi.mocked(invoke).mockImplementation(async (cmd: string, args: any) => {
      const docId = (args?.docId ?? "doc-1") as string;
      switch (cmd) {
        case "rust_pixels_open_document":
          streamFor(docId);
          return undefined;
        case "rust_pixels_record_external":
          record(docId, "external");
          return { tiles: [], epoch: 0, version: 0 };
        case "apply_tile_patch":
          record(docId, "pixel");
          return { layer_id: args?.layerId, tiles: [], epoch: 0, version: 0 };
        case "rust_pixels_undo":
          step(docId, "undo");
          return { layer_id: args?.layerId, tiles: [], epoch: 0, version: 0 };
        case "rust_pixels_redo":
          step(docId, "redo");
          return { layer_id: args?.layerId, tiles: [], epoch: 0, version: 0 };
        case "rust_pixels_history_tip":
          return tipFor(docId);
        default:
          throw new Error(`emulator: unhandled command ${cmd}`);
      }
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  // ── Wiring: the probe must run because the PRODUCTION path called it ───────
  it("both production call sites run the probe (delete either and this goes RED)", async () => {
    gateOn(true);
    const { history, commands } = mountCommands();
    expect(commands.undo).toBeTypeOf("function");
    expect(commands.redo).toBeTypeOf("function");

    history.commit(createMockModel("Meta"), "Add Layer");
    history.commit(createMockModel("Paint"), "Brush", makePatches(), false);
    await waitForRust("doc-1", 2);
    expect(timesInvoked("rust_pixels_history_tip")).toBe(0);

    // Undo #1 pops the Pixel entry -> the TILE restore path's probe call site.
    await driveStep(commands.undo);
    expect(timesInvoked("rust_pixels_history_tip")).toBe(1);
    // Undo #2 pops the External entry -> the SNAPSHOT restore path's call site.
    await driveStep(commands.undo);
    expect(timesInvoked("rust_pixels_history_tip")).toBe(2);
    // And the redo path reads it too.
    await driveStep(commands.redo);
    expect(timesInvoked("rust_pixels_history_tip")).toBe(3);
  });

  it("the probe is reached through the production commit funnel, not a stub", async () => {
    gateOn(true);
    const { history } = mountCommands();

    history.commit(createMockModel("Meta"), "Add Layer");
    await waitForRust("doc-1", 1);

    // Proof the real funnel fired the real command names into the emulator.
    const census = await flushPixelInvokeCensus();
    expect(census.entries.some((e) => e.command === "rust_pixels_record_external")).toBe(true);
    expect(streamFor("doc-1").entries).toEqual(["external"]);
  });

  // ── The measurement ───────────────────────────────────────────────────────
  it("bridge OFF (production default): the probe does not run at all", async () => {
    // The shipping default. `commit` fires no `rust_pixels_record_external`
    // (history.ts:500), so Rust never hears about a metadata step. Rust is NOT
    // empty in general - the canonical writer `rust_pixels_write_region` is
    // ungated, so paint steps still land in its stream - but a metadata step
    // separates the two cursors by construction, which is the configured design
    // rather than a drift. So the probe does not read, and says nothing.
    gateOn(false);
    const { history, commands } = mountCommands();

    history.commit(createMockModel("Meta"), "Add Layer");
    await settle();
    expect(timesInvoked("rust_pixels_record_external")).toBe(0);
    expect(history.getUndoCount(), "the TS stack holds the step").toBe(1);
    expect(streamFor("doc-1").entries, "Rust recorded nothing for the metadata step").toEqual([]);

    // The production undo/redo call sites still fire the observer; the gate is
    // inside the probe, so neither call site had to know about it.
    const before = timesInvoked("rust_pixels_history_tip");
    commands.undo();
    await settle();
    commands.redo();
    await settle();
    expect(timesInvoked("rust_pixels_history_tip"), "no cursor read").toBe(before);
    expect(parityWarns()).toEqual([]);
    expect(vi.mocked(console.info).mock.calls.map((c) => String(c[0]))).not.toContain(
      expect.stringContaining("history-cursor-parity"),
    );
    // The undo itself is unaffected: the host stack still popped.
    expect(history.getUndoCount()).toBe(1);
  });

  it("the gate is the bridge flag: same call, bridge ON reads the cursor", async () => {
    // The other half of the guard test: defeat the early return (make the probe
    // read unconditionally, or never read) and one of these two halves reddens.
    gateOn(false);
    const { history } = mountCommands();
    history.commit(createMockModel("Meta"), "Add Layer");
    await settle();
    const before = timesInvoked("rust_pixels_history_tip");

    const gated = await probeHistoryCursorParity("doc-1", history.getUndoCount());
    expect(gated.observing, "bridge OFF: the probe declines to read").toBe(false);
    expect(gated.tip).toBeNull();
    expect(timesInvoked("rust_pixels_history_tip")).toBe(before);

    gateOn(true);
    history.commit(createMockModel("Paint"), "Brush", makePatches(), false);
    await waitForRust("doc-1", 1);
    const live = await probeHistoryCursorParity("doc-1", history.getUndoCount());
    expect(live.observing, "bridge ON: the probe reads").toBe(true);
    expect(live.tip).not.toBeNull();
    expect(timesInvoked("rust_pixels_history_tip")).toBe(before + 1);
  });

  it("bridge ON, Pixel step: the cursors stay isomorphic across undo and redo", async () => {
    gateOn(true);
    const { history, commands } = mountCommands();

    history.commit(createMockModel("Meta"), "Add Layer");
    history.commit(createMockModel("Paint"), "Brush", makePatches(), false);
    await waitForRust("doc-1", 2);
    expect(history.getUndoCount()).toBe(2);
    expect(streamFor("doc-1").entries).toEqual(["external", "pixel"]);

    // The tile restore path calls rust_pixels_undo AND pops the host entry, so
    // both cursors move by one: measured, not assumed.
    await driveStep(commands.undo);
    expect(timesInvoked("rust_pixels_undo"), "the tile path stepped the Rust cursor").toBe(1);
    expect(history.getUndoCount()).toBe(1);
    expect(streamFor("doc-1").cursor).toBe(1);
    expect(await verdictFor("doc-1", history)).toBe("in-sync");

    await driveStep(commands.redo);
    expect(timesInvoked("rust_pixels_redo")).toBe(1);
    expect(history.getUndoCount()).toBe(2);
    expect(streamFor("doc-1").cursor).toBe(2);
    expect(await verdictFor("doc-1", history)).toBe("in-sync");
  });

  it("bridge ON, External step: rust_pixels_undo moves the cursor even with no tiles", async () => {
    gateOn(true);
    const { history, commands } = mountCommands();

    // [pixel, external]: the second undo's cursor sync lands while an External
    // entry is the tip. `undo_pixel` still steps the cursor (history.rs:229-233)
    // and only the TILE yield is empty, so the cursor must drop from 2 to 1.
    history.commit(createMockModel("Paint"), "Brush", makePatches(), false);
    history.commit(createMockModel("Meta"), "Add Layer");
    await waitForRust("doc-1", 2);
    expect(streamFor("doc-1").entries).toEqual(["pixel", "external"]);
    expect(streamFor("doc-1").cursor).toBe(2);

    // Undo #1 pops the External (metadata) entry: the snapshot restore path does
    // NOT step the Rust cursor, so the two cursors separate here - and the probe
    // the production path fired says so.
    await driveStep(commands.undo);
    expect(history.getUndoCount()).toBe(1);
    expect(streamFor("doc-1").cursor, "the metadata path never calls rust_pixels_undo").toBe(2);
    await waitFor(() => parityWarns().length === 1);
    expect(vi.mocked(console.warn).mock.calls.at(-1)?.[1]).toMatchObject({
      docId: "doc-1",
      direction: "undo",
      tsUndoDepth: 1,
      rustUndoDepth: 2,
      rustUndoTipKind: "external",
    });

    // Undo #2 pops the Pixel entry: the tile path DOES call rust_pixels_undo, and
    // that call's tip is the External entry. Rust moves the cursor anyway.
    await driveStep(commands.undo);
    expect(timesInvoked("rust_pixels_undo")).toBe(1);
    expect(history.getUndoCount()).toBe(0);
    expect(
      streamFor("doc-1").cursor,
      "an External tip still moves the cursor; only the tile yield is empty",
    ).toBe(1);
    const tip = await readRustHistoryTip("doc-1");
    expect(tip).toEqual({
      total_depth: 2,
      undo_depth: 1,
      redo_depth: 1,
      undo_tip_kind: "pixel",
      redo_tip_kind: "external",
    });
  });

  it("the Rust tip kinds are reported per direction and flip when the cursor moves", async () => {
    gateOn(true);
    const { history } = mountCommands();
    history.commit(createMockModel("Meta"), "Add Layer");
    history.commit(createMockModel("Paint"), "Brush", makePatches(), false);
    await waitForRust("doc-1", 2);

    const atEnd = await readRustHistoryTip("doc-1");
    expect(atEnd).toEqual({
      total_depth: 2,
      undo_depth: 2,
      redo_depth: 0,
      undo_tip_kind: "pixel",
      redo_tip_kind: null,
    });

    streamFor("doc-1").cursor = 1;
    const afterUndo = await readRustHistoryTip("doc-1");
    expect(afterUndo.undo_tip_kind, "undo now points at the External entry").toBe("external");
    expect(afterUndo.redo_tip_kind, "redo points at the Pixel entry").toBe("pixel");
    expect(afterUndo.undo_depth).toBe(1);
    expect(afterUndo.redo_depth).toBe(1);
  });

  // ── The comparison ─────────────────────────────────────────────────────────
  it("DEFEAT: a mismatched depth is reported as diverged, never as in-sync", () => {
    const tip: RustHistoryTip = {
      total_depth: 5,
      undo_depth: 5,
      redo_depth: 0,
      undo_tip_kind: "pixel",
      redo_tip_kind: null,
    };
    // Rust says 5 undoable steps, the host stack says 0. That is a divergence.
    expect(classifyHistoryCursorParity(tip, 0)).toBe("diverged");
    expect(classifyHistoryCursorParity(tip, 4)).toBe("diverged");
    expect(classifyHistoryCursorParity(tip, 6)).toBe("diverged");
    // And the same reading at the SAME depth is in-sync, so the pair of cases
    // cannot both pass unless the comparison actually reads both numbers.
    expect(classifyHistoryCursorParity(tip, 5)).toBe("in-sync");
  });

  it("a depth-only reading that contradicts the tip kinds is unknown, not agreement", () => {
    const good: RustHistoryTip = {
      total_depth: 3,
      undo_depth: 2,
      redo_depth: 1,
      undo_tip_kind: "external",
      redo_tip_kind: "pixel",
    };
    expect(classifyHistoryCursorParity(good, 2)).toBe("in-sync");
    // Branches that do not partition the stream.
    expect(classifyHistoryCursorParity({ ...good, redo_depth: 2 }, 2)).toBe("unknown");
    // A direction with depth 0 that still names an entry, and vice versa.
    expect(classifyHistoryCursorParity({ ...good, undo_tip_kind: null }, 2)).toBe("unknown");
    expect(classifyHistoryCursorParity({ ...good, redo_tip_kind: null }, 2)).toBe("unknown");
    expect(classifyHistoryCursorParity({ ...good, redo_depth: 0 }, 2)).toBe("unknown");
  });

  it("an unusable Rust reading is unknown, never in-sync", () => {
    const good: RustHistoryTip = {
      total_depth: 1,
      undo_depth: 1,
      redo_depth: 0,
      undo_tip_kind: "external",
      redo_tip_kind: null,
    };
    expect(classifyHistoryCursorParity(null, 1)).toBe("unknown");
    expect(classifyHistoryCursorParity(undefined, 1)).toBe("unknown");
    expect(classifyHistoryCursorParity({ ...good, undo_depth: NaN }, 1)).toBe("unknown");
    expect(classifyHistoryCursorParity({ ...good, undo_depth: -1 }, 1)).toBe("unknown");
    expect(classifyHistoryCursorParity({ ...good, undo_depth: 1.5 }, 1)).toBe("unknown");
    expect(classifyHistoryCursorParity({ ...good, total_depth: NaN }, 1)).toBe("unknown");
    expect(classifyHistoryCursorParity(good, NaN)).toBe("unknown");
    expect(classifyHistoryCursorParity(good, -1)).toBe("unknown");
  });

  it("a malformed wire response rejects instead of reading as an empty cursor", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ undo_depth: null } as any);
    await expect(readRustHistoryTip("doc-1")).rejects.toThrow(/malformed .* response/);
    vi.mocked(invoke).mockResolvedValueOnce(null as any);
    await expect(readRustHistoryTip("doc-1")).rejects.toThrow(/malformed .* response/);
  });

  // ── The probe cannot silently report health it has no data for ────────────
  it("DEFEAT: a read that fails is unknown and says so, never in-sync and never silent", async () => {
    gateOn(true);
    vi.mocked(invoke).mockImplementationOnce(() => Promise.reject(new Error("command not found")));

    const reading = await probeHistoryCursorParity("doc-1", 3);
    expect(reading.verdict).toBe("unknown");
    expect(reading.tip).toBeNull();

    // The same failure through the production fire-and-forget wrapper: one info
    // line naming the blindness, and NO warn (undo/redo did not fail).
    vi.mocked(invoke).mockImplementationOnce(() => Promise.reject(new Error("command not found")));
    const { observeHistoryCursorParity } = await import("../historyCursorParity");
    observeHistoryCursorParity("doc-1", 3, "undo");
    await waitFor(() => vi.mocked(console.info).mock.calls.length === 1);
    expect(vi.mocked(console.warn)).not.toHaveBeenCalled();
    expect(vi.mocked(console.info).mock.calls[0][0]).toContain("no usable reading");
  });

  it("a probe overtaken by a newer step drops itself instead of pairing a stale depth", async () => {
    gateOn(true);
    const { history } = mountCommands();
    history.commit(createMockModel("Meta"), "Add Layer");
    await waitForRust("doc-1", 1);
    const before = timesInvoked("rust_pixels_history_tip");

    // Hold the first reading open, then start a second one. `HistoryTip` carries
    // no version/sequence, so a stale read is undetectable from its payload: the
    // monotonic op counter is the only thing that can catch it.
    const gates: Array<() => void> = [];
    vi.mocked(invoke).mockImplementation(
      (cmd: string) =>
        new Promise((resolve) => {
          if (cmd !== "rust_pixels_history_tip") return resolve(undefined as any);
          gates.push(() => resolve(tipFor("doc-1") as any));
        }),
    );

    const first = probeHistoryCursorParity("doc-1", history.getUndoCount());
    await waitFor(() => timesInvoked("rust_pixels_history_tip") > before);

    // A step arriving while that read is outstanding: refused, not piled on.
    const second = await probeHistoryCursorParity("doc-1", history.getUndoCount());
    expect(timesInvoked("rust_pixels_history_tip") - before).toBe(1);
    expect(second.verdict).toBe("unknown");

    gates[0]?.();
    expect((await first).verdict, "the overtaken reading reports nothing").toBe("unknown");
    expect(parityWarns(), "a superseded probe must not warn about divergence").toEqual([]);
  });

  it("nothing to measure (no document) is unknown and fires no invoke", async () => {
    // Bridge ON, so this isolates the `!docId` early return rather than the gate.
    gateOn(true);
    const before = timesInvoked("rust_pixels_history_tip");
    expect((await probeHistoryCursorParity("", 3)).verdict).toBe("unknown");
    expect(timesInvoked("rust_pixels_history_tip")).toBe(before);
  });
});
