// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * Closing the cursor drift: one host entry => exactly one Rust entry => exactly
 * one cursor step, in every configuration.
 *
 * THE DRIFT. Two recorders and one stepper, gated on two different flags, over one
 * stream:
 *   - `installFacadeCommitShim` (lib/protocol/facadeRegistry.ts) mirrors every
 *     commit that is not `alreadyRecordedInRust` into an `External` entry, under
 *     `isFacadeEnabled()` - `photrez.facade !== "0"`, so ON by default.
 *   - `CommandHistory.stepRustCursor` decided whether to step under
 *     `bridgeRecordsFor()` -> `historyBridgeEnabled()` - needs
 *     `photrez.historyBridge === "1"` AND Tauri, so OFF by default.
 * At those shipping defaults a metadata commit appended an entry no pop would ever
 * step, and the cursors separated by one per undone step.
 *
 * TWO DEFECTS, NOT ONE. `commit`'s bridge arm and the shim BOTH record a non-pixel
 * commit, and the shim cannot see that the bridge already did. With bridge ON and
 * facade ON, one host commit therefore appended TWO entries (for a metadata commit:
 * two `External`; for a TS-owned tile commit: one `Pixel` from `apply_tile_patch`
 * plus one `External` from the shim) while the pop issued a single step - so the
 * drift existed in the "everything on" configuration too, and grew faster.
 *
 * THE FIX, both halves:
 *   1. `commit` reports whether it recorded (`committedRecordedInBridge`), and the
 *      shim stands down when it did. The bridge and the shim are now mutually
 *      exclusive recorders: exactly one entry per host commit.
 *   2. `stepRustCursor` gained a shim arm, so a pop steps whenever the shim - not
 *      the bridge - is the recorder for that entry. Without it the entry the shim
 *      recorded still had no consumer.
 *
 * WHAT THIS DOES NOT CHANGE. The facade handoff still pops no host entry: it
 * returns before `history.undo()`, so it drives the native walker and never reaches
 * a pop. That is what keeps it from double-stepping - asserted per configuration
 * below. The tip-kind gate stays as defence in depth; with the cursors aligned it
 * no longer refuses on any legitimate path, and the matrix proves that.
 *
 * MOCK FIDELITY. The Rust side is emulated. The shim is NOT: the real
 * `installFacadeCommitShim` runs, over the real `CommandHistory`, through the real
 * `recordExternalTransitionFor`, and the only boundary replaced is the Tauri
 * transport - the same boundary `rustPixelUndoHandoff.wiring.test.ts` replaces.
 * `answerShimInvoke` adds the five protocol commands that path issues, each
 * answering what `PixelStoreRegistry` would, and delegates every pixel command to
 * the shared emulator so the cursor rules stay defined in one place.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { CommandHistory } from "../history";
import type { DocumentModel, LayerNode } from "../types";
import { useEditorCommands } from "@/components/editor/useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "@/components/editor/dialogs/DialogProvider";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import {
  answerInvoke,
  record,
  resetStreams,
  streamFor,
  tipFor,
} from "./rustStreamEmulator";
import { commitRustOwnedPaint, makePatches } from "./historyCursorFixtures";
import { cursorStepInvokes, settle, timesInvoked } from "./historyCursorHarness";
import { installFacadeCommitShim } from "@/lib/protocol/facadeRegistry";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => false),
  isFacadeOwnedLayer: vi.fn(() => false),
}));
// The handoff pops no host entry in these cases, and `handoffMovedCursor` reports
// false because the mocked handoff never drives the walker. The REAL handoff -
// including its fall-through-after-moving-the-cursor path - is driven in
// facadeHandoffDoubleStep.wiring.test.ts, which does not mock this module.
vi.mock("@/components/editor/facadeHistoryHandoff", () => ({
  runFacadeExternalHandoff: vi.fn(async () => false),
  handoffMovedCursor: vi.fn(() => false),
}));

const BRIDGE_GATE = "photrez.historyBridge";
const RUST_PIXELS = "photrez.rustPixels";
const FACADE = "photrez.facade";
const DOC = "doc-1";

/**
 * The transport boundary, extended for the shim's path. The five commands below
 * are what `recordExternalTransitionFor` reaches through the native authority
 * route; each answers the shape the real command produces. `recordExternalTransition`
 * is the one that matters: it appends an `External` entry to the same emulated
 * stream the cursor steps, which is what makes the shim a real recorder here.
 */
async function answerShimInvoke(cmd: string, args: unknown): Promise<unknown> {
  const a = (args ?? {}) as Record<string, unknown>;
  switch (cmd) {
    case "protocol_register_adapter_native":
    case "protocol_seed_native":
      return JSON.stringify({ version: 0, layers: [] });
    case "protocol_seed_canonical_native":
      return JSON.stringify({ version: 0, layers: [] });
    case "protocol_version_native":
      return 1;
    case "protocol_apply_command_native": {
      const envelope = JSON.parse(String(a.envelopeJson)) as {
        command?: { type?: string };
      };
      if (envelope.command?.type === "recordExternalTransition") {
        record(String(a.docId), "external");
        const s = streamFor(String(a.docId));
        return JSON.stringify({ documentVersion: s.version, externalSeq: s.entries.length });
      }
      return JSON.stringify({ documentVersion: streamFor(String(a.docId)).version });
    }
    default:
      return answerInvoke(cmd, args);
  }
}

function makeModel(paint: string): DocumentModel {
  return {
    id: DOC,
    name: paint,
    width: 10,
    height: 10,
    layers: [],
    activeLayerId: null,
    selection: null,
    viewport: { panX: 0, py: 0, zoom: 1, rotation: 0 },
    dirty: false,
  } as unknown as DocumentModel;
}

/**
 * An engine whose model really changes and really restores, so the acceptance
 * criterion ("the model is back to its pre-change state") is a fact about state
 * and not about a call count.
 */
function makeEngine() {
  let current = makeModel("before");
  return {
    getId: () => DOC,
    getActiveLayerId: () => "l1",
    getLayer: () => null,
    getLayers: () => current.layers,
    snapshot: (): DocumentModel => ({ ...current, layers: [...current.layers] }),
    restore: (snap: DocumentModel) => {
      current = { ...snap, layers: [...snap.layers] };
    },
    getPaintSurface: () => null,
    getModel: () => current,
    ensureBitmapCurrent: vi.fn(),
    invalidatePaintSurface: vi.fn(),
    transformLayer: vi.fn(),
    /** The mutation a metadata op performs after its commit. */
    mutate: (name: string) => {
      current = makeModel(name);
    },
  };
}

function mountCommands() {
  const history = new CommandHistory();
  history.attachDocIdGetter(() => DOC);
  const engine = makeEngine();
  const renderer = { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() };
  mockUseEditor({
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => DOC,
      notifyVisualChange: vi.fn(),
    },
    renderer,
    scheduler: { requestRender: vi.fn() },
    activeDocumentId: () => DOC,
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
    setSelectedLayerId: vi.fn(),
    textEditSession: () => null,
    setTextEditSession: vi.fn(),
    setStatusLoadingMessage: vi.fn(),
    setShowExportDialog: vi.fn(),
    setShowPrintDialog: vi.fn(),
    setShowResizeDialog: vi.fn(),
  });
  return { history, engine, renderer, commands: useEditorCommands(() => {}) };
}

async function until(predicate: () => boolean, timeoutMs = 3000): Promise<void> {
  const start = Date.now();
  while (!predicate()) {
    if (Date.now() - start > timeoutMs) throw new Error("until: timed out");
    await new Promise<void>((r) => setTimeout(r, 5));
  }
  await settle();
}

/** Drive one metadata op (commit, then mutate) exactly as a legacy op does. */
async function metadataOp(history: CommandHistory, engine: ReturnType<typeof makeEngine>) {
  history.commit(engine.snapshot(), "Add Layer");
  engine.mutate("after");
  await settle();
}

/** The shim's entry for this commit has landed in the stream. */
async function shimRecorded(count: number) {
  await until(() => streamFor(DOC).entries.length >= count);
}

/**
 * One undo press, waited on by the tile path's upload. `settle()` alone is a single
 * macrotask, and the tip-kind gate puts a read in front of the step, so a gated
 * press spans more than one - waiting on the observable effect instead keeps a case
 * from passing or failing on a guessed macrotask count.
 */
async function pressUndo(
  commands: { undo: () => void },
  renderer: { uploadSurfaceTiles: { mock: { calls: unknown[][] } } },
  expectedUploads: number,
): Promise<void> {
  commands.undo();
  await until(() => renderer.uploadSurfaceTiles.mock.calls.length >= expectedUploads);
}

describe("one host entry => one Rust entry => one cursor step", () => {
  beforeEach(() => {
    localStorage.clear();
    resetStreams();
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<
      typeof DialogProviderModule.useDialog
    >);
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(console, "info").mockImplementation(() => {});
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation((c: string, a: unknown) => answerShimInvoke(c, a));
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    // The facade flag defaults ON (`!== "0"`), so the shim is installed and armed
    // unless a case says otherwise.
    localStorage.setItem(FACADE, "1");
    installFacadeCommitShim({
      getEngine: () => ({ getId: () => DOC, getLayers: () => [{ id: "l1" }] }),
      getDocId: () => DOC,
    });
  });

  afterEach(async () => {
    await settle();
    await new Promise<void>((r) => setTimeout(r, 0));
    vi.restoreAllMocks();
  });

  // ── The acceptance criterion: the sequence the reviewer counted ────────────
  it("ACCEPTANCE delete-then-paint: a delete, a stroke, then Ctrl+Z reverts the STROKE's own pixels", async () => {
    // The gap the metadata-then-paint case above did not cover. A layer delete goes
    // through `recordSnapshotHistory` (`useLayerActions.ts`, "Delete Layer"), which
    // is a SNAPSHOT-typed host entry - and `stepRustCursor` returns for a
    // snapshot-typed entry before every arm, so no host pop ever steps an entry
    // recorded for one.
    //
    // The commit shim DOES mirror that commit into an `External` entry, and the
    // mirror is load-bearing: its consumer is the FACADE WALKER (`facade.undo()` +
    // `confirmExternalCursor`), not `rust_pixels_undo`. Without it the WASM cursor
    // never advanced for such a delete and the stranded TS undo-point made
    // `engine.restore()` throw `E_FACADE_OWNED` - see
    // snapshotHistoryMirror.wiring.test.ts.
    //
    // So the entry above the stroke's `Pixel` one is real, and the stroke's pop must
    // still reach its OWN pixels. `hasFacadeOwnedLayers()` is FALSE in this fixture,
    // so nothing walks the delete's entry here and the Rust cursor legitimately stays
    // one above the host stack after undoing the stroke - that residual is the walker
    // path's to consume, and pretending otherwise here would encode a fiction. What
    // this case pins is the part that IS the host's: the stroke's pop steps its own
    // `Pixel` entry rather than inheriting the delete's `External` as its tip.
    //
    // The witness is the CURSOR, not the tile upload: `useEditorCommands` calls
    // `uploadSurfaceTiles` unconditionally and passes the host's local memento when
    // the Rust step yields nothing, so a tile upload proves nothing about which entry
    // the stream consumed.
    const { history, engine, renderer, commands } = mountCommands();
    const layers = [{ id: "l1", name: "Paint", visible: true }] as unknown as LayerNode[];
    engine.getModel().layers = layers;

    // 1. Delete the layer: a snapshot-typed host commit, mirrored by the shim.
    history.recordSnapshotHistory(engine.snapshot(), engine.snapshot(), "Delete Layer");
    engine.getModel().layers = [];
    await settle();
    await until(() => streamFor(DOC).entries.length >= 1);
    expect(
      streamFor(DOC).entries,
      "the shim's External mirror for the delete (the facade walker's to consume)",
    ).toEqual(["external"]);
    expect(history.getUndoCount(), "the host still holds the delete").toBe(1);

    // 2. Paint a stroke on what is left.
    await commitRustOwnedPaint(history, engine.snapshot());
    await until(() => streamFor(DOC).entries.length >= 2);
    expect(streamFor(DOC).entries, "the stroke's own Pixel entry sits above it").toEqual([
      "external",
      "pixel",
    ]);

    // 3. Ctrl+Z. The stroke's own pixels must revert.
    commands.undo();
    await until(() => cursorStepInvokes() >= 1);
    await settle();

    expect(
      (renderer.uploadSurfaceTiles.mock.calls[0][3] as unknown[]).length,
      "the stroke reverted with its OWN tiles",
    ).toBeGreaterThan(0);
    expect(
      cursorStepInvokes(),
      "exactly one step, issued against the stroke's own entry",
    ).toBe(1);
    expect(
      streamFor(DOC).cursor,
      "it consumed the STROKE's entry (cursor 2 -> 1), not the delete's External",
    ).toBe(1);
    expect(
      history.getUndoCount(),
      "one undo popped ONLY the stroke - the delete is still on the host stack",
    ).toBe(1);
    expect(
      tipFor(DOC).undo_tip_kind,
      "and the Rust tip is still the delete's External, which the facade walker owns",
    ).toBe("external");
  });

  it("ACCEPTANCE delete-then-paint, inverse: undoing the delete brings the layer back", async () => {
    // The other direction. Two undos: the stroke, then the delete - whose undo-point
    // is the PRE-delete `before` state, so the layer must be present again and the
    // cursors must still agree.
    const { history, engine, renderer, commands } = mountCommands();
    engine.getModel().layers = [
      { id: "l1", name: "Paint", visible: true },
    ] as unknown as LayerNode[];

    history.recordSnapshotHistory(engine.snapshot(), engine.snapshot(), "Delete Layer");
    engine.getModel().layers = [];
    await settle();
    await until(() => streamFor(DOC).entries.length >= 1);
    await commitRustOwnedPaint(history, engine.snapshot());
    await until(() => streamFor(DOC).entries.length >= 2);
    await settle();

    commands.undo(); // the stroke
    await until(() => renderer.uploadSurfaceTiles.mock.calls.length >= 1);
    commands.undo(); // the delete
    await until(() => history.getUndoCount() === 0);

    expect(
      engine.getModel().layers.map((l) => l.id),
      "the deleted layer is back",
    ).toEqual(["l1"]);
    expect(history.getUndoCount(), "the host stack is drained").toBe(0);
    expect(
      streamFor(DOC).cursor,
      "the Rust cursor sits on the delete's External entry, which is the facade " +
        "WALKER's to consume - a snapshot-typed pop steps nothing itself, by design",
    ).toBe(1);
  });

  // ── Step 4: the acceptance criterion ───────────────────────────────────────
  it("ACCEPTANCE: after a metadata change, one Ctrl+Z restores the model AND both cursors agree", async () => {
    const { history, engine, commands } = mountCommands();
    expect(engine.getModel().name, "the pre-change state is what the user sees").toBe("before");

    await metadataOp(history, engine);
    expect(engine.getModel().name, "the op changed the model").toBe("after");
    await shimRecorded(1);
    expect(streamFor(DOC).cursor, "the shim recorded the metadata step").toBe(1);
    expect(history.getUndoCount(), "and the host stack holds it").toBe(1);

    commands.undo();
    await until(() => cursorStepInvokes() >= 1);

    expect(
      engine.getModel().name,
      "the model is back to its pre-change state",
    ).toBe("before");
    expect(history.getUndoCount(), "the host stack popped it").toBe(0);
    expect(
      streamFor(DOC).cursor,
      "and the Rust cursor agrees with the host stack - this is the drift, closed",
    ).toBe(history.getUndoCount());
  });

  it("ACCEPTANCE: a paint step after a metadata step still reverts its own pixels", async () => {
    // The user-visible symptom the gate alone left behind: the metadata undo now
    // steps, so the cursor is back on the paint entry and the second Ctrl+Z
    // reverts real pixels instead of being refused.
    const { history, engine, renderer, commands } = mountCommands();
    await commitRustOwnedPaint(history, engine.snapshot());
    await until(() => streamFor(DOC).entries.length >= 1);
    await metadataOp(history, engine);
    await shimRecorded(2);
    expect(streamFor(DOC).entries).toEqual(["pixel", "external"]);

    commands.undo(); // the metadata step
    await until(() => cursorStepInvokes() >= 1);
    expect(engine.getModel().name).toBe("before");
    expect(streamFor(DOC).cursor, "the cursor is back on the paint entry").toBe(1);

    await (async () => {
      commands.undo();
      await until(() => renderer.uploadSurfaceTiles.mock.calls.length >= 1);
    })();

    expect(streamFor(DOC).cursor, "and the paint entry was consumed too").toBe(0);
    expect(
      (renderer.uploadSurfaceTiles.mock.calls[0][3] as unknown[]).length,
      "so the paint step reverted with its OWN tiles - not refused",
    ).toBeGreaterThan(0);
  });

  // ── The configuration matrix ───────────────────────────────────────────────
  it("MATRIX bridge ON + facade ON: the shim stands down, so ONE entry and ONE step", async () => {
    localStorage.setItem(BRIDGE_GATE, "1");
    const { history, engine, commands } = mountCommands();
    await metadataOp(history, engine);
    await until(() => timesInvoked("rust_pixels_record_external") >= 1 || streamFor(DOC).entries.length >= 1);

    expect(
      timesInvoked("rust_pixels_record_external"),
      "the bridge recorded the metadata step",
    ).toBe(1);
    expect(
      streamFor(DOC).entries,
      "and the shim did NOT add a second entry - one host entry, one Rust entry",
    ).toEqual(["external"]);

    commands.undo();
    await until(() => cursorStepInvokes() >= 1);
    expect(cursorStepInvokes(), "exactly one step").toBe(1);
    expect(streamFor(DOC).cursor, "cursor fully drained").toBe(0);
    expect(streamFor(DOC).cursor).toBe(history.getUndoCount());
  });

  it("MATRIX bridge ON + facade OFF: the bridge is the only recorder, ONE entry and ONE step", async () => {
    localStorage.setItem(BRIDGE_GATE, "1");
    localStorage.setItem(FACADE, "0");
    const { history, engine, commands } = mountCommands();
    await metadataOp(history, engine);
    await until(() => timesInvoked("rust_pixels_record_external") >= 1);

    expect(timesInvoked("rust_pixels_record_external"), "the bridge recorded").toBe(1);
    expect(streamFor(DOC).entries, "one entry").toEqual(["external"]);

    commands.undo();
    await until(() => cursorStepInvokes() >= 1);
    expect(cursorStepInvokes(), "exactly one step").toBe(1);
    expect(streamFor(DOC).cursor).toBe(0);
    expect(streamFor(DOC).cursor).toBe(history.getUndoCount());
  });

  it("MATRIX bridge OFF + facade ON: the shim is the only recorder, ONE entry and ONE step", async () => {
    const { history, engine, commands } = mountCommands();
    await metadataOp(history, engine);
    await shimRecorded(1);

    expect(
      timesInvoked("rust_pixels_record_external"),
      "the bridge recorded nothing",
    ).toBe(0);
    expect(streamFor(DOC).entries, "the shim's entry is the only one").toEqual(["external"]);

    commands.undo();
    await until(() => cursorStepInvokes() >= 1);
    expect(cursorStepInvokes(), "the shim arm issued exactly one step").toBe(1);
    expect(streamFor(DOC).cursor, "cursor fully drained").toBe(0);
    expect(streamFor(DOC).cursor).toBe(history.getUndoCount());
  });

  it("MATRIX bridge OFF + facade OFF: nothing records, so nothing steps", async () => {
    localStorage.setItem(FACADE, "0");
    const { history, engine, commands } = mountCommands();
    await metadataOp(history, engine);
    await settle();

    expect(timesInvoked("rust_pixels_record_external"), "no recorder fired").toBe(0);
    expect(streamFor(DOC).entries, "the stream is empty").toEqual([]);

    commands.undo();
    await settle();
    expect(cursorStepInvokes(), "and a pop with no counterpart issues no step").toBe(0);
    expect(streamFor(DOC).cursor).toBe(0);
    expect(history.getUndoCount(), "the host stack still popped - the host is the authority").toBe(0);
  });

  it("MATRIX: the facade handoff pops no host entry, so it never double-steps", async () => {
    // The handoff is mocked to decline, which is the fall-through case; the point
    // pinned here is that `stepRustCursor` is reached ONLY from a pop, so the
    // handoff's own walker move and a host step cannot both happen for one press.
    // Every configuration that issues a step issues exactly one, asserted above;
    // this case pins the structural half - a declined handoff plus a pop is still
    // ONE step.
    localStorage.setItem(BRIDGE_GATE, "1");
    const { history, engine, commands } = mountCommands();
    await metadataOp(history, engine);
    await until(() => timesInvoked("rust_pixels_record_external") >= 1);

    commands.undo();
    await until(() => cursorStepInvokes() >= 1);
    commands.redo();
    await until(() => cursorStepInvokes() >= 2);

    expect(cursorStepInvokes(), "one step per press, undo and redo alike").toBe(2);
    expect(streamFor(DOC).cursor, "the redo put the entry back").toBe(1);
  });

  it("MATRIX: a TS-owned tile commit also yields ONE entry and ONE step per configuration", async () => {
    // The shim mirrors tile commits too, so this is the case where the old code
    // recorded a Pixel (bridge) AND an External (shim) for one host entry.
    for (const bridge of [false, true]) {
      resetStreams();
      vi.mocked(invoke).mockClear();
      localStorage.removeItem(BRIDGE_GATE);
      if (bridge) localStorage.setItem(BRIDGE_GATE, "1");

      const { history, renderer, commands } = mountCommands();
      const m = engineLikeModel();
      history.commit(m, "Text", {
        layerId: "l1",
        surfaceWidth: 10,
        surfaceHeight: 10,
        before: [{ x: 0, y: 0, width: 2, height: 2, data: new Uint8ClampedArray(8) }],
        after: [{ x: 0, y: 0, width: 2, height: 2, data: new Uint8ClampedArray(8) }],
      });
      await until(() => streamFor(DOC).entries.length >= 1);
      await settle();

      expect(
        streamFor(DOC).entries.length,
        `bridge ${bridge ? "ON" : "OFF"}: ONE host entry produced exactly ONE Rust entry`,
      ).toBe(1);

      commands.undo();
      await until(() => cursorStepInvokes() >= 1);
      expect(
        cursorStepInvokes(),
        `bridge ${bridge ? "ON" : "OFF"}: exactly one step`,
      ).toBe(1);
      await settle();
    }
  });

  it("MATRIX rustPixels ON + facade ON, a TILE commit: entryArm and shimArm co-fire", async () => {
    // The one cell where the two arms overlap. Both predicates are private, so this
    // pins their INPUTS and then the consequence they jointly produce:
    //
    //   entryArm = `entry.imperative?.rustOwned === true || (entry.imperative !== undefined
    //               && rustPixelsFlagEnabled())`
    //   shimArm  = `!facadeMoved && entry.rustRecordFailed !== true && !bridgeArm &&
    //               entry.imperative?.rustOwned !== true && isFacadeEnabled() &&
    //               !historyDegraded()`
    //
    // The first clause of entryArm needs `imperative`, so this must be a TILE commit.
    // It used to drive `metadataOp`, which has no `imperative` - entryArm was FALSE
    // there and only shimArm fired, so the cell never tested what its name claimed.
    localStorage.setItem(RUST_PIXELS, "1");
    const { history, engine, commands } = mountCommands();
    // rustPixels ON but NOT rustOwned: the second entryArm clause needs an imperative
    // and the first must stay false, so the shim arm is not excluded by rustOwned.
    history.commit(engine.snapshot(), "Text", makePatches());
    engine.getModel().name = "after";
    await shimRecorded(1);

    // shimArm's inputs: bridge off (flag unset), facade on, not degraded, not rustOwned.
    expect(
      streamFor(DOC).entries,
      "the shim recorded the tile commit's counterpart - shimArm's recorder",
    ).toEqual(["external"]);
    // entryArm's inputs: the popped entry is an imperative tile commit and the flag is on.
    expect(
      history.getUndoCount(),
      "one host entry to pop, and it is the tile commit",
    ).toBe(1);

    commands.undo();
    await until(() => cursorStepInvokes() >= 1);

    expect(
      cursorStepInvokes(),
      "exactly ONE step: the co-firing arms resolve to a single fire, never two",
    ).toBe(1);
    expect(streamFor(DOC).cursor, "the shim's entry was consumed").toBe(0);
    expect(streamFor(DOC).cursor).toBe(history.getUndoCount());
  });

  it("MATRIX rustPixels ON + facade ON, a METADATA commit: only shimArm fires", async () => {
    // The contrast that makes the cell above meaningful: a metadata commit has no
    // `imperative`, so entryArm is false and only shimArm fires.
    localStorage.setItem(RUST_PIXELS, "1");
    const { history, engine, commands } = mountCommands();
    await metadataOp(history, engine);
    await shimRecorded(1);
    expect(streamFor(DOC).entries).toEqual(["external"]);

    commands.undo();
    await until(() => cursorStepInvokes() >= 1);
    expect(cursorStepInvokes(), "exactly one step").toBe(1);
    expect(streamFor(DOC).cursor, "the shim's entry was consumed").toBe(0);
    expect(streamFor(DOC).cursor).toBe(history.getUndoCount());
  });

  it("MATRIX bridge ON + rustOwned: entryArm fires even though the bridge is the recorder", async () => {
    // The mirror of the co-firing cell: a Rust-OWNED entry with the bridge on. entryArm
    // is true via its first clause, and bridgeArm is true too, so `if (bridgeArm ||
    // shimArm)` takes the UNGATED path even though this entry's recorder was the pixel
    // writer, not the bridge and not the shim. The bridge records nothing for a
    // `rustOwned` commit (`alreadyRecordedInRust`), so this entry's counterpart is the
    // `Pixel` entry `rust_pixels_write_region` already appended - and an ungated step
    // over it is correct.
    localStorage.setItem(BRIDGE_GATE, "1");
    const { history, engine, renderer, commands } = mountCommands();
    await commitRustOwnedPaint(history, engine.snapshot());
    await until(() => streamFor(DOC).entries.length >= 1);

    expect(streamFor(DOC).entries, "the pixel writer recorded the only entry").toEqual(["pixel"]);
    expect(
      timesInvoked("rust_pixels_record_external"),
      "and neither the bridge nor the shim added a second",
    ).toBe(0);

    await pressUndo(commands, renderer, 1);
    expect(cursorStepInvokes(), "exactly one step").toBe(1);
    expect(streamFor(DOC).cursor, "the stroke's own Pixel entry was consumed").toBe(0);
    expect(streamFor(DOC).cursor).toBe(history.getUndoCount());
  });

  it("a failed bridge record on ONE entry does not disarm the NEXT one", async () => {
    // The per-entry marker, versus the history-wide latch it replaced. The bridge is
    // ON here, so `commit` is the recorder and the shim stands down: this pop takes the
    // bridge arm. A history-wide failure condition would put EVERY later bridge-arm pop
    // on the gated path, and the gate REFUSES an `External` tip - so one rejected record
    // would stop the cursor advancing for the rest of the session, on entries whose tip
    // was perfectly correct.
    localStorage.setItem(BRIDGE_GATE, "1");
    let rejectNext = true;
    const { history, engine, commands } = mountCommands();
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) => {
      if (cmd === "rust_pixels_record_external" && rejectNext) {
        rejectNext = false;
        return Promise.reject(new Error("no IPC in jsdom: rust_pixels_record_external"));
      }
      return answerShimInvoke(cmd, args) as Promise<unknown>;
    });

    // Commit 1: its bridge record is rejected.
    await metadataOp(history, engine);
    await until(() => streamFor(DOC).entries.length >= 1 || rejectNext === false);
    await settle();
    expect(streamFor(DOC).entries, "commit 1's record never landed").toEqual([]);

    // Commit 2: identical, and its record succeeds.
    await metadataOp(history, engine);
    await until(() => streamFor(DOC).entries.length >= 1);
    expect(streamFor(DOC).entries, "commit 2's counterpart is on the stream").toEqual([
      "external",
    ]);

    // Undo commit 2. Its tip IS its own entry and nothing is wrong with it, so it MUST
    // step - a history-wide latch would push it onto the gate and refuse.
    //
    // NOT REDUNDANT with the hop case below: this one only ever pops the ORIGINAL entry
    // objects, so it reads `rustRecordFailed` straight off the object `commit`
    // constructed and cannot see a hop. It pins the marker's SCOPE (one entry, not one
    // history); the hop case pins its SURVIVAL across an undo->redo rebuild. Both fail
    // independently - deleting either one loses a distinct defect.
    commands.undo();
    await settle();
    await settle();
    await settle();
    expect(
      cursorStepInvokes(),
      "commit 2 steps ungated: commit 1's failure is scoped to commit 1",
    ).toBe(1);
    expect(streamFor(DOC).cursor, "and its entry was consumed").toBe(0);
  });

  it("a failed bridge record survives an undo->redo hop: the redo takes the GATED path", async () => {
    // Both stack hops rebuild the entry as a fresh object literal, copying
    // `imperative`, `snapshotType` and `pixelLayerIds`. A new field that is not copied
    // is silently dropped by that rebuild - which is exactly what happened to
    // `imperative` and `snapshotType` before they were propagated.
    //
    // A's Rust record is rejected, so NOTHING exists in the stream behind it. Undo pops
    // A - the ORIGINAL object, marker intact. That undo pushes A's twin onto the redo
    // stack, and the redo pops THAT object: if the marker did not travel, the redo reads
    // `undefined`, takes the bridge arm's UNGATED path, and steps.
    //
    // The distinguishing witness is the TIP at redo time, so the case places a FOREIGN
    // entry in the stream first. On an empty stream the gate cannot tell and fires either
    // way - that is the gate's deliberate fail-open (see `fireGatedOnPixelTip`), and it
    // is why an earlier draft of this case could not fail at all.
    localStorage.setItem(BRIDGE_GATE, "1");
    let rejectNext = true;
    const { history, engine, commands } = mountCommands();
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) => {
      if (cmd === "rust_pixels_record_external" && rejectNext) {
        rejectNext = false;
        return Promise.reject(new Error("no IPC in jsdom: rust_pixels_record_external"));
      }
      return answerShimInvoke(cmd, args) as Promise<unknown>;
    });

    // A: rejected, so it has NO counterpart in the stream.
    await metadataOp(history, engine);
    await until(() => rejectNext === false);
    await settle();
    expect(streamFor(DOC).entries, "A's record never landed").toEqual([]);

    // Undo pops A. Gated by the marker, and the stream is empty, so the gate fires
    // fail-open - but with nothing to move over, the cursor does not budge.
    commands.undo();
    await settle();
    await settle();
    expect(history.getUndoCount(), "the host stack is drained").toBe(0);
    expect(streamFor(DOC).cursor, "and an empty stream cannot be stepped").toBe(0);

    // A foreign branch, owned by no host entry: two External entries, then the cursor
    // walked back one so a redo branch EXISTS. The branch matters because the gate reads
    // `redo_tip_kind` for a redo, and that is null whenever the cursor sits at the end -
    // in which case the gate fail-opens and the case cannot fail. With the cursor one
    // short of the end, `redo_tip_kind` is a real non-pixel value the gate must refuse.
    record(DOC, "external");
    record(DOC, "external");
    await settle();
    expect(streamFor(DOC).cursor, "two foreign entries recorded").toBe(2);
    await invoke("rust_pixels_undo", { docId: DOC, layerId: "" });
    await settle();
    expect(
      tipFor(DOC).redo_tip_kind,
      "a redo branch exists, so the gate has a real tip to refuse",
    ).toBe("external");
    const stepsBeforeRedo = cursorStepInvokes();

    // The hop. Redo pops A's twin.
    commands.redo();
    await settle();
    await settle();
    await settle();

    expect(
      cursorStepInvokes(),
      "the redo is GATED and the gate REFUSES a tip that is not A's - the marker " +
        "survived the undo->redo hop",
    ).toBe(stepsBeforeRedo);
    expect(
      streamFor(DOC).cursor,
      "so the cursor never advanced over the foreign redo branch",
    ).toBe(1);
  });

  it("MATRIX bridge ON + facade ON, a snapshot-typed delete: the shim stands down", async () => {
    localStorage.setItem(BRIDGE_GATE, "1");
    localStorage.setItem(FACADE, "1");
    const { history, engine } = mountCommands();

    history.recordSnapshotHistory(engine.snapshot(), engine.snapshot(), "Delete Layer");
    await until(() => timesInvoked("rust_pixels_record_snapshot") >= 1);
    await settle();

    expect(
      timesInvoked("rust_pixels_record_snapshot"),
      "the bridge recorded the delete as a Snapshot entry",
    ).toBe(1);
    // The stream shows ONLY the shim's mirror, because the shared emulator
    // deliberately does not model Snapshot entries - `undo_pixel` refuses to move the
    // cursor for one, so a Snapshot entry has no cursor arithmetic for the emulator to
    // own (see rustStreamEmulator.ts). So an empty stream here IS "the bridge is the
    // sole recorder", and a non-empty one is the double-record.
    expect(
      streamFor(DOC).entries,
      "ONE host commit, ONE Rust entry: the shim must NOT add its External mirror on " +
        "top of the bridge's Snapshot one",
    ).toEqual([]);
  });

  it("MATRIX bridge ON + facade OFF, TS-owned tile: the bridge is the only recorder", async () => {
    localStorage.setItem(BRIDGE_GATE, "1");
    localStorage.setItem(FACADE, "0");
    const { history, engine, renderer, commands } = mountCommands();
    history.commit(engine.snapshot(), "Text", {
      layerId: "l1",
      surfaceWidth: 10,
      surfaceHeight: 10,
      before: [{ x: 0, y: 0, width: 2, height: 2, data: new Uint8ClampedArray(8) }],
      after: [{ x: 0, y: 0, width: 2, height: 2, data: new Uint8ClampedArray(8) }],
    });
    await until(() => streamFor(DOC).entries.length >= 1);
    await settle();

    expect(
      timesInvoked("apply_tile_patch"),
      "the bridge recorded the tile step as a Pixel entry",
    ).toBe(1);
    expect(streamFor(DOC).entries, "exactly one entry").toEqual(["pixel"]);

    await pressUndo(commands, renderer, 1);
    expect(cursorStepInvokes(), "exactly one step").toBe(1);
    expect(streamFor(DOC).cursor).toBe(0);
    expect(streamFor(DOC).cursor).toBe(history.getUndoCount());
  });
});

/** A committed model for the tile case; `mountCommands`' engine is not needed. */
function engineLikeModel(): DocumentModel {
  return makeModel("before");
}
