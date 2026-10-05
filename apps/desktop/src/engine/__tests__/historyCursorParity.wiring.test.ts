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
 * This file drives the production undo/redo for both restore paths (tile and
 * snapshot/metadata) and reports the parity verdict after every step. It is the
 * measurement that found the metadata-undo drift: with the cursor sync living
 * inside the tile branch, an External (metadata) pop restored the host model
 * without stepping the Rust cursor, the two cursors separated by one per undone
 * metadata step, and the next pixel step consumed that un-stepped entry and
 * reverted nothing. `CommandHistory.undo()`/`redo()` now own the step, so the
 * mixed `[external, pixel]` sequence below must report `in-sync` where it
 * reported `diverged`.
 *
 * A probe that cannot report divergence proves nothing, so both directions are
 * load-bearing: neuter the comparison in `classifyHistoryCursorParity` and the
 * divergence cases go RED; delete either production call to
 * `observeHistoryCursorParity` in `useEditorCommands` and the wiring cases go
 * RED; remove the probe's bridge gate and the gate case goes RED.
 *
 * Every driving case runs with the bridge ON, because the probe only reads while
 * the bridge is recording - with it off the two stacks are not two views of one
 * history (`CommandHistory.commit` records only inside `if
 * (historyBridgeEnabled())`, and only paint steps reach Rust at all, via the
 * ungated canonical writer), which the one bridge-OFF case pins instead.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { CommandHistory } from "../history";
import type { HistoryTilePatches } from "../history";
import type { DocumentModel } from "../types";
import { useEditorCommands } from "@/components/editor/useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "@/components/editor/dialogs/DialogProvider";
import { runFacadeExternalHandoff } from "@/components/editor/facadeHistoryHandoff";
import { hasFacadeOwnedLayers } from "@/engine/document";
import {
  classifyHistoryCursorParity,
  probeHistoryCursorParity,
  readRustHistoryTip,
  type RustHistoryTip,
} from "../historyCursorParity";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import { flushPixelInvokeCensus, pendingCount } from "@/lib/protocol/pixelInvokeCensus";
import {
  answerInvoke,
  record,
  resetStreams,
  streamFor,
  tipFor,
} from "./rustStreamEmulator";
import { commitRustOwnedPaint, makePatches } from "./historyCursorFixtures";
import {
  cursorStepInvokes,
  driveStep,
  parityWarns,
  settle,
  tick,
  timesInvoked,
  waitFor,
  waitForRust,
} from "./historyCursorHarness";

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
// The facade handoff is a collaborator with its own coverage
// (rustPixelUndoHandoff.wiring.test.ts). Here it only has to be able to say
// "handled", so the early-return case can prove the dispatcher returns without
// popping a host entry. Default false = "fell through", i.e. never taken.
vi.mock("@/components/editor/facadeHistoryHandoff", () => ({
  runFacadeExternalHandoff: vi.fn(async () => false),
  // This mock's handoff never drives the native walker, so it never moved the
  // cursor - which is exactly what the dispatcher reads here. The real
  // fall-through-AFTER-moving path is driven in facadeHandoffDoubleStep.wiring.test.ts.
  handoffMovedCursor: vi.fn(() => false),
}));

const GATE_KEY = "photrez.historyBridge";

// The Rust stream emulator (stream rules, cursor steps, wire-shape fidelity) lives
// in ./rustStreamEmulator so those rules have one home.
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

/** Commit a metadata step and WAIT for its fire-and-forget record to land. */
async function commitMetadata(history: CommandHistory, label = "Add Layer") {
  history.commit(createMockModel(label), label);
  await settle();
}

/** Stateful engine following the real DocumentEngine snapshot/restore shape. */
function makeEngine(layer?: { id: string; transform?: unknown }) {
  let model: DocumentModel = createMockModel("live");
  return {
    getId: () => "doc-1",
    getActiveLayerId: () => "l1",
    getLayer: () => layer ?? null,
    getLayers: () => model.layers,
    snapshot: (): DocumentModel => ({ ...model, layers: [...model.layers] }),
    restore: (snap: DocumentModel) => {
      model = { ...snap, layers: [...snap.layers] };
    },
    getPaintSurface: () => null,
    getModel: () => model,
    ensureBitmapCurrent: vi.fn(),
    invalidatePaintSurface: vi.fn(),
    // The transform mini-undo branch calls this on its way out; without it the
    // fire-and-forget undo rejects unhandled and vitest fails the run.
    transformLayer: vi.fn(),
  };
}

function makeEditorContext(
  engine: ReturnType<typeof makeEngine>,
  history: CommandHistory,
  overrides: Record<string, unknown> = {},
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
    ...overrides,
  };
}

/** Mount the production command hook over a real history + emulated Rust stream. */
function mountCommands(
  engineOverrides: {
    layer?: { id: string; transform?: unknown };
    ctx?: Record<string, unknown>;
    /** Skip `attachDocIdGetter` - the `editorOpenImage.loadProjectFile` shape. */
    noDocIdGetter?: boolean;
  } = {},
) {
  const history = new CommandHistory();
  if (!engineOverrides.noDocIdGetter) history.attachDocIdGetter(() => "doc-1");
  const engine = makeEngine(engineOverrides.layer);
  const ctx = makeEditorContext(engine, history, engineOverrides.ctx);
  mockUseEditor(ctx);
  return { history, engine, ctx, commands: useEditorCommands(() => {}) };
}

/** Read Rust's cursor through the production reader, then classify it. */
async function verdictFor(
  docId: string,
  history: CommandHistory,
): Promise<ReturnType<typeof classifyHistoryCursorParity>> {
  return (await probeHistoryCursorParity(docId, history.getUndoCount())).verdict;
}

describe("history cursor parity: TS undo depth vs the Rust cursor", () => {
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    resetStreams();
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<
      typeof DialogProviderModule.useDialog
    >);
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(console, "info").mockImplementation(() => {});

    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) => answerInvoke(cmd, args));
    vi.mocked(runFacadeExternalHandoff).mockResolvedValue(false);
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
    await commitRustOwnedPaint(history, createMockModel("Paint"));
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
    // (it records only inside `if (historyBridgeEnabled())`), so Rust never hears
    // about a metadata step. Rust is NOT
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
    await commitRustOwnedPaint(history, createMockModel("Paint"));
    await waitForRust("doc-1", 1);
    const live = await probeHistoryCursorParity("doc-1", history.getUndoCount());
    expect(live.observing, "bridge ON: the probe reads").toBe(true);
    expect(live.tip).not.toBeNull();
    expect(timesInvoked("rust_pixels_history_tip")).toBe(before + 1);
  });

  it("bridge ON, Pixel step: the cursors stay isomorphic across undo and redo", async () => {
    gateOn(true);
    const { history, commands } = mountCommands();

    await commitMetadata(history);
    await commitRustOwnedPaint(history, createMockModel("Paint"));
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

  it("bridge ON, External step: the metadata pop steps the cursor like any other pop", async () => {
    gateOn(true);
    const { history, commands } = mountCommands();

    // [pixel, external]: the second undo pops a metadata entry, which carries no
    // tile patches at all. `ProtocolEngine::undo_pixel` still steps the cursor for
    // an External tip (history.rs:229-233) and only the TILE yield is empty, so
    // the cursor must drop from 2 to 1.
    await commitRustOwnedPaint(history, createMockModel("Paint"));
    await commitMetadata(history);
    await waitForRust("doc-1", 2);
    expect(streamFor("doc-1").entries).toEqual(["pixel", "external"]);
    expect(streamFor("doc-1").cursor).toBe(2);

    // Undo #1 pops the External (metadata) entry. This step used to restore the
    // host model and NOTHING else: the cursor sync sat inside the tile branch,
    // which a metadata entry never reaches, so the cursor stayed at 2 while the
    // TS stack fell to 1.
    await driveStep(commands.undo);
    expect(history.getUndoCount()).toBe(1);
    expect(streamFor("doc-1").cursor, "the metadata pop stepped the Rust cursor too").toBe(1);
    expect(parityWarns(), "a cursor that tracks the host depth is not a divergence").toEqual([]);
    expect(await verdictFor("doc-1", history)).toBe("in-sync");

    // Undo #2 pops the Pixel entry: the tile path takes Rust's tiles for its own
    // step, and the cursor drops to 0.
    await driveStep(commands.undo);
    expect(timesInvoked("rust_pixels_undo"), "one cursor step per pop").toBe(2);
    expect(history.getUndoCount()).toBe(0);
    expect(streamFor("doc-1").cursor).toBe(0);
    const tip = await readRustHistoryTip("doc-1");
    expect(tip).toEqual({
      total_depth: 2,
      undo_depth: 0,
      redo_depth: 2,
      undo_tip_kind: null,
      redo_tip_kind: "pixel",
    });
  });

  it("ACCEPTANCE bridge ON, mixed [external, pixel]: both pops step, so the verdict is in-sync", async () => {
    // The sequence the drift was measured on. Undo #1 pops the Pixel entry (tile
    // path), undo #2 pops the External entry (metadata path). Before the fix,
    // undo #2 left the Rust cursor at 2 against a TS depth of 0 - verdict
    // "diverged" - and the drift then silently swallowed the NEXT paint step.
    gateOn(true);
    const { history, commands } = mountCommands();

    await commitMetadata(history);
    await commitRustOwnedPaint(history, createMockModel("Paint"));
    await waitForRust("doc-1", 2);
    expect(streamFor("doc-1").entries).toEqual(["external", "pixel"]);
    expect(await verdictFor("doc-1", history), "both stacks recorded both steps").toBe("in-sync");

    await driveStep(commands.undo);
    expect(streamFor("doc-1").cursor).toBe(1);
    expect(await verdictFor("doc-1", history)).toBe("in-sync");

    await driveStep(commands.undo);
    expect(history.getUndoCount()).toBe(0);
    expect(streamFor("doc-1").cursor, "the metadata pop moved it as well").toBe(0);
    expect(
      await verdictFor("doc-1", history),
      "was \"diverged\" while the cursor sync lived inside the tile branch",
    ).toBe("in-sync");
    expect(parityWarns(), "no step of this sequence reported divergence").toEqual([]);

    // And back the other way, so redo is measured on the same stream.
    await driveStep(commands.redo);
    expect(timesInvoked("rust_pixels_redo"), "one redo step, no double step").toBe(1);
    expect(await verdictFor("doc-1", history)).toBe("in-sync");
    await driveStep(commands.redo);
    expect(timesInvoked("rust_pixels_redo"), "the metadata redo steps too").toBe(2);
    expect(streamFor("doc-1").cursor).toBe(2);
    expect(await verdictFor("doc-1", history)).toBe("in-sync");
    expect(parityWarns()).toEqual([]);
  });

  it("DEFEAT a pixel undo takes exactly ONE cursor step per press, in both directions", async () => {
    // The failure the old NOTE in CommandHistory.undo() warned about ("Doing it
    // here would cause DOUBLE UNDO"): if the pop steps the cursor AND the tile
    // branch issues its own invoke, one press takes two steps and walks the
    // cursor two entries back - reverting a step the user never asked for.
    gateOn(true);
    const { history, commands } = mountCommands();
    await commitRustOwnedPaint(history, createMockModel("Paint"));
    await waitForRust("doc-1", 1);

    await driveStep(commands.undo);
    expect(timesInvoked("rust_pixels_undo"), "one press, one cursor step").toBe(1);
    expect(streamFor("doc-1").cursor).toBe(0);

    await driveStep(commands.redo);
    expect(timesInvoked("rust_pixels_redo")).toBe(1);
    expect(timesInvoked("rust_pixels_undo"), "redo never re-steps the undo direction").toBe(1);
    expect(streamFor("doc-1").cursor).toBe(1);
  });

  it("two pixel undos fired back-to-back: step N is not issued until step N-1 lands, and each press takes its OWN tiles", async () => {
    // Rapid input. Both pops happen in one turn, so both cursor steps want to be
    // in flight at once. Two decrements commute, so the final cursor proves
    // nothing about ORDER - what proves it is (a) the second invoke is not even
    // issued while the first is outstanding, and (b) each press receives the
    // tiles of the step it popped, not whichever invoke answered first.
    gateOn(true);
    const { history, commands, ctx } = mountCommands();
    await commitRustOwnedPaint(history, createMockModel("Paint1"));
    await commitRustOwnedPaint(history, createMockModel("Paint2"));
    await waitForRust("doc-1", 2);

    const held: Array<() => void> = [];
    let released = 0;
    /** Release every step that appears until the census reports nothing pending.
     *  A step this file never gets back (a press that issues two, as the pre-fix
     *  dispatcher did) would otherwise stay in flight and hang the census drain
     *  that every later case performs. */
    const releaseAll = async () => {
      for (let round = 0; round < 20; round++) {
        while (released < held.length) held[released++]();
        await tick();
        if (pendingCount() === 0) return;
      }
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string, args: unknown) => {
      // The cursor moves when the command is ISSUED, as the real command does;
      // only the response is held, so the held promise stands for a slow answer.
      const answer = await answerInvoke(cmd, args);
      if (cmd !== "rust_pixels_undo") return answer;
      const seq = held.length;
      return new Promise((resolve) =>
        held.push(() =>
          resolve({
            layer_id: "l1",
            // Identifiable per step: press 1 must upload seq 0, press 2 seq 1.
            tiles: [{ x: seq, y: 0, w: 1, h: 1, data: [seq, 0, 0, 255] }],
            epoch: 1,
            version: 1,
          }),
        ),
      );
    });

    commands.undo();
    commands.undo();
    await waitFor(() => held.length >= 1);
    try {
      // The ordering claim, checked while the first answer is still outstanding:
      // an unserialised fire-and-forget step would already have issued the second
      // invoke by now, and both round-trips would be racing the same mutex.
      expect(
        timesInvoked("rust_pixels_undo"),
        "the second step waits behind the first instead of racing it",
      ).toBe(1);
    } finally {
      // Releasing in a finally, not after the assertion: a failing ordering claim
      // must not leave an invoke in flight, because every later case drains the
      // census and would hang on it instead of reporting its own verdict.
      await releaseAll();
    }
    await settle();

    expect(timesInvoked("rust_pixels_undo")).toBe(2);
    expect(history.getUndoCount()).toBe(0);
    expect(streamFor("doc-1").cursor, "two pops, two steps").toBe(0);
    const uploads = (ctx.renderer.uploadSurfaceTiles as unknown as { mock: { calls: unknown[][] } })
      .mock.calls;
    expect(
      uploads.map((call) => (call[3] as { x: number }[])[0]?.x),
      "each press uploaded the tiles of the step it popped, in pop order",
    ).toEqual([0, 1]);
    expect(await verdictFor("doc-1", history)).toBe("in-sync");
  });

  describe("early returns that pop no CommandHistory entry leave the Rust cursor alone", () => {
    // Each of these returns BEFORE the history pop, so nothing owns a cursor step
    // for the press. If the step were fired by the dispatcher rather than by the
    // pop, these are exactly the presses that would consume a Rust entry nobody
    // asked to undo.
    //
    // NOT `settle()`: this file's own harness comment says one macrotask is not
    // enough under parallel workers, and a step that IS fired but lands late
    // would read here as "no invoke, cursor unmoved" - precisely the failure these
    // cases exist to catch. `expectQuietly` waits for the step that must NOT come.
    beforeEach(() => {
      gateOn(true);
    });

    /** Let every pending task run, then prove no cursor step appeared. */
    const expectNoCursorStep = async (why: string) => {
      await settle();
      await tick();
      await settle();
      expect(cursorStepInvokes(), why).toBe(0);
      expect(streamFor("doc-1").cursor, `${why}: the Rust cursor did not move`).toBe(1);
    };

    it("the transform mini-undo (a separate mini stack) steps nothing", async () => {
      const { history, commands } = mountCommands({
        layer: { id: "l1", transform: { x: 4, y: 4, scale: 2 } },
        ctx: {
          layerTransformSession: () => ({ layerId: "l1" }),
          undoTransformWithCurrent: () => ({ transform: { x: 1, y: 1, scale: 1 } }),
          redoTransformWithCurrent: () => ({ transform: { x: 2, y: 2, scale: 1 } }),
        },
      });
      history.commit(createMockModel("Meta"), "Add Layer");
      await waitForRust("doc-1", 1);

      commands.undo();
      await expectNoCursorStep("transform mini-undo");
      commands.redo();
      await expectNoCursorStep("transform mini-redo");
      expect(history.getUndoCount(), "no host entry popped either").toBe(1);
    });

    it("cancelling an active transform session steps nothing", async () => {
      // `cancelActiveTransformSession()` returns before the pop too, and it is the
      // branch a live resize/rotate session takes on every Ctrl+Z: the session is
      // torn down and the gesture's own restore runs instead of a history step.
      // The REAL `cancelLayerTransformSession` is used here (it is a module
      // import, not a context value), driven by a session that really belongs to
      // this engine, so the branch is entered for the production reason.
      const { history, commands, engine } = mountCommands({
        layer: { id: "l1", transform: { x: 4, y: 4, scale: 1 } },
        ctx: {
          layerTransformSession: () => ({
            layerId: "l1",
            documentId: "doc-1",
            originalSnapshot: createMockModel("pre-gesture"),
            originalTransform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
          }),
          setLayerTransformSession: vi.fn(),
          // The mini stack is EMPTY, so this branch must fall through to the
          // cancel branch - which is the branch under test.
          undoTransformWithCurrent: () => null,
          redoTransformWithCurrent: () => null,
        },
      });
      const restore = vi.spyOn(engine, "restore");
      history.commit(createMockModel("Meta"), "Add Layer");
      await waitForRust("doc-1", 1);

      commands.undo();
      await expectNoCursorStep("cancelled transform session");
      expect(restore, "the session really was cancelled through the model").toHaveBeenCalledTimes(1);
      expect(history.getUndoCount()).toBe(1);
    });

    it("the modern-crop undo (a separate crop stack) steps nothing", async () => {
      const { history, commands } = mountCommands({
        ctx: {
          activeTool: () => "crop",
          cropInteractionMode: () => "modern",
          canModernCropUndo: () => true,
          canModernCropRedo: () => true,
          undoModernCrop: () => ({ frame: { x: 1, y: 1, w: 4, h: 4 }, transform: { x: 0, y: 0, scale: 1 } }),
          redoModernCrop: () => ({ frame: { x: 2, y: 2, w: 4, h: 4 }, transform: { x: 0, y: 0, scale: 1 } }),
          setModernCropFrame: vi.fn(),
          setModernCropImageTransform: vi.fn(),
        },
      });
      history.commit(createMockModel("Meta"), "Add Layer");
      await waitForRust("doc-1", 1);

      commands.undo();
      await expectNoCursorStep("modern crop undo");
      commands.redo();
      await expectNoCursorStep("modern crop redo");
      expect(history.getUndoCount()).toBe(1);
    });

    it("the classic crop undo (a separate crop stack) steps nothing", async () => {
      const { history, commands } = mountCommands({
        ctx: {
          activeTool: () => "crop",
          cropInteractionMode: () => "classic",
          canCropUndo: () => true,
          canCropRedo: () => true,
          undoLastCrop: () => ({ rect: { x: 1, y: 1, w: 4, h: 4 }, rotation: 0 }),
          redoCrop: () => ({ rect: { x: 2, y: 2, w: 4, h: 4 }, rotation: 0 }),
          setCropRect: vi.fn(),
          setCropRotation: vi.fn(),
        },
      });
      history.commit(createMockModel("Meta"), "Add Layer");
      await waitForRust("doc-1", 1);

      commands.undo();
      await expectNoCursorStep("classic crop undo");
      commands.redo();
      await expectNoCursorStep("classic crop redo");
      expect(history.getUndoCount()).toBe(1);
    });

    it("the facade handoff that reports itself handled steps nothing and pops nothing", async () => {
      // The handoff drives the Rust cursor through the native walker, so its own
      // path is not a cursor step this file can measure. What it must NOT do is
      // fall through into a host pop on top of the step Rust already took.
      vi.mocked(hasFacadeOwnedLayers).mockReturnValue(true);
      vi.mocked(runFacadeExternalHandoff).mockResolvedValue(true);
      const { history, commands } = mountCommands();
      history.commit(createMockModel("Meta"), "Add Layer");
      await waitForRust("doc-1", 1);

      commands.undo();
      await expectNoCursorStep("facade handoff that reported handled");
      expect(runFacadeExternalHandoff).toHaveBeenCalledTimes(1);
      expect(history.getUndoCount(), "no host entry popped").toBe(1);
    });
  });

  it("bridge ON, a TS-OWNED tile commit records through apply_tile_patch, so its pop steps exactly once", async () => {
    // The bridge's tile arm is `apply_tile_patch`. The host sends `TileUploadLike`
    // = {x, y, width, height, data} and the command's `TilePatchWire` ALIASES
    // `width`/`height` onto its `w`/`h`, so the memento really does mint a Pixel
    // entry (pinned in Rust by
    // `both_tile_shapes_are_accepted_at_the_wire_and_mint_exactly_one_entry`).
    // The emulator above accepts the same shape, so this case measures the real
    // contract and not a convenient stub.
    //
    // Before the alias the pop had no counterpart and stepped nothing. Re-arming
    // the arm is only sound if it still cannot step TWICE, so the invoke count per
    // direction is asserted, not just the cursor position.
    gateOn(true);
    const { history, commands, ctx } = mountCommands();

    history.commit(createMockModel("Meta"), "Add Layer");
    history.commit(createMockModel("Text"), "Text", makePatches(), false);
    await waitForRust("doc-1", 2);
    expect(
      streamFor("doc-1").entries,
      "the bridge's tile arm really recorded a Pixel entry",
    ).toEqual(["external", "pixel"]);
    expect(timesInvoked("apply_tile_patch"), "the host-shape memento reached the command").toBe(1);

    // Undo the TS-owned tile entry: Rust holds its Pixel entry, so the pop consumes
    // exactly that one. ONE invoke for the press - the tile branch only AWAITS the
    // step the pop fired (rustTileProjection), it never issues a second.
    await driveStep(commands.undo);
    expect(cursorStepInvokes(), "the tile pop steps the cursor").toBe(1);
    expect(timesInvoked("rust_pixels_undo"), "one press, one step - no double step").toBe(1);
    expect(streamFor("doc-1").cursor).toBe(1);
    expect(await verdictFor("doc-1", history), "2 TS entries, 2 Rust entries").toBe("in-sync");
    expect(parityWarns(), "a cursor that tracks the host depth is not a divergence").toEqual([]);

    // The entry is not rustOwned, so its own memento still reaches the surface:
    // the tile fetch is not armed for it, and that is unchanged by the step.
    const uploads = (ctx.renderer.uploadSurfaceTiles as unknown as { mock: { calls: unknown[][] } })
      .mock.calls;
    expect(
      (uploads[0]?.[3] as { x: number }[]).map((t) => t.x),
      "the entry's own memento replayed",
    ).toEqual([0]);

    // And the metadata entry beneath it still steps, so the cursor tracks the
    // steps Rust actually holds: 2 TS entries, 2 Rust entries, cursor 2 -> 0.
    await driveStep(commands.undo);
    expect(cursorStepInvokes(), "the metadata pop steps too").toBe(2);
    expect(streamFor("doc-1").cursor).toBe(0);
    expect(await verdictFor("doc-1", history)).toBe("in-sync");

    // The step the tile pop left unconsumed (no Rust fetch armed for it) must not
    // surface on a later pop: the next pop drops the stale handle, so redo issues
    // exactly one redo step and never an undo.
    await driveStep(commands.redo);
    expect(timesInvoked("rust_pixels_redo"), "one redo step").toBe(1);
    expect(
      timesInvoked("rust_pixels_undo"),
      "the unconsumed step is not re-issued - still the two undos above",
    ).toBe(2);
    expect(streamFor("doc-1").cursor).toBe(1);
  });

  it("bridge ON, a TS-OWNED tile pop: the parity probe reads AFTER the step's cursor move lands", async () => {
    // The tile branch's fetch gate is `rustOwned || photrez.rustPixels`, and a
    // bridge-ON TS-owned tile pop leaves it CLOSED - so `projectRustTiles` returns
    // early and never claims the step, while the pop still fired one. The probe
    // fires immediately afterwards over the same registry mutex, so unless the
    // branch SEQUENCES the read behind that step it reads a cursor that has not
    // moved yet: `undo_depth` one too high against a host depth that already fell.
    //
    // What is asserted is the CURSOR THE READ MEETS, sampled at the moment the
    // probe's invoke is issued. Invoke ORDER cannot express this - the pop fires
    // the step synchronously, so the step's invoke is always issued first whether
    // or not the branch awaits it - and neither can the verdict: the emulator
    // answers equally-delayed calls in order, so an unsequenced read still happens
    // to land after the move (see the ORDERING note in rustStreamEmulator.ts).
    // The sampling is the invariant, and it is what makes this case RED without the
    // `lastCursorStepSettled()` await.
    gateOn(true);
    const { history, commands } = mountCommands();
    history.commit(createMockModel("Meta"), "Add Layer");
    history.commit(createMockModel("Text"), "Text", makePatches(), false);
    await waitForRust("doc-1", 2);
    expect(streamFor("doc-1").cursor, "both cursors start at the end").toBe(2);

    const cursorAtRead: number[] = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "rust_pixels_history_tip") cursorAtRead.push(streamFor("doc-1").cursor);
      return answerInvoke(cmd, args);
    });

    await driveStep(commands.undo);

    expect(timesInvoked("rust_pixels_undo"), "the tile pop fired a step").toBe(1);
    expect(cursorAtRead.length, "the tile branch did read the cursor").toBe(1);
    expect(
      cursorAtRead[0],
      "the read met the POST-step cursor, so undo_depth can match the host depth",
    ).toBe(1);
    expect(parityWarns(), "a cursor that tracks the host depth is not a divergence").toEqual([]);
    expect(await verdictFor("doc-1", history), "2 TS entries, 2 Rust entries").toBe("in-sync");
  });

  it("the Rust tip kinds are reported per direction and flip when the cursor moves", async () => {
    gateOn(true);
    const { history } = mountCommands();
    await commitMetadata(history);
    await commitRustOwnedPaint(history, createMockModel("Paint"));
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


  it("a history with NO doc-id getter records nothing, so it steps nothing", async () => {
    // `editorOpenImage.loadProjectFile` builds `new CommandHistory()` with no
    // `attachDocIdGetter` (every other production history attaches one), and that
    // is the File>Open path - so every image opened from disk has this shape.
    // `commit` records into Rust only inside `historyBridgeEnabled() && docIdGetter`,
    // so this history records NOTHING while the bridge is on. If the pop stepped
    // anyway it would consume the nearest entry Rust does hold for this document:
    // a brush stroke's Pixel entry, recorded by the ungated `rust_pixels_write_region`.
    gateOn(true);
    const { history, commands } = mountCommands({ noDocIdGetter: true });
    history.commit(createMockModel("Meta"), "Add Layer");
    await settle();
    expect(
      timesInvoked("rust_pixels_record_external"),
      "a getterless history records nothing, exactly like commit does",
    ).toBe(0);
    expect(history.getUndoCount(), "the host stack still holds the step").toBe(1);

    commands.undo();
    await settle();
    expect(cursorStepInvokes(), "no Rust entry to consume, so no step").toBe(0);
    expect(streamFor("doc-1").cursor, "the Rust cursor did not move").toBe(0);
    commands.redo();
    await settle();
    expect(cursorStepInvokes(), "the redo pop steps nothing either").toBe(0);
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
