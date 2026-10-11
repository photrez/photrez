// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * The invariant this file pins: one host entry => exactly one Rust entry => exactly
 * one cursor step.
 *
 * THE DRIFT, for the record. `stepRustCursor` originally had no consumer for the
 * `External` entries `installFacadeCommitShim` records, so a metadata commit appended
 * an entry no pop ever stepped and the two cursors separated by one per undone step.
 * A second, commit-time recorder (the `photrez.historyBridge` arm) was retired along
 * the way; what remains is a single recorder (the shim) and a single consumer (the
 * pop's shim arm).
 *
 * WHAT THIS PINS NOW.
 *   1. The configuration matrix over `photrez.facade`: every
 *      cell yields ONE entry per host commit and ONE step per pop, or - with the shim
 *      off - no entry and no step.
 *   2. A declined facade handoff plus a pop is still ONE step.
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
import { commitRustOwnedPaint } from "./historyCursorFixtures";
import { cursorStepInvokes, settle } from "./historyCursorHarness";
import { installFacadeCommitShim } from "@/lib/protocol/facadeRegistry";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => false),
  facadeOwnedIdsOf: vi.fn(() => undefined),
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
    // the shim mirrors unconditionally, so
    // no flag write is needed — it is installed and armed for every case.
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
  it("MATRIX facade ON: the shim is the only recorder, ONE entry and ONE step", async () => {
    const { history, engine, commands } = mountCommands();
    await metadataOp(history, engine);
    await shimRecorded(1);

    expect(streamFor(DOC).entries, "the shim's entry is the only one").toEqual(["external"]);

    commands.undo();
    await until(() => cursorStepInvokes() >= 1);
    expect(cursorStepInvokes(), "the shim arm issued exactly one step").toBe(1);
    expect(streamFor(DOC).cursor, "cursor fully drained").toBe(0);
    expect(streamFor(DOC).cursor).toBe(history.getUndoCount());
  });

  // the "nothing records" configuration is
  // unconstructible — the shipped shim mirrors every commit unconditionally.
  // The counterpart-less REFUSAL is still pinned where it is still reachable:
  // rustOwnedExternalTipReachable.wiring.test.ts plants the foreign tip by hand
  // over a file that installs no recorder.
  it("MATRIX: the facade handoff pops no host entry, so it never double-steps", async () => {
    // The handoff is mocked to decline, which is the fall-through case; the point
    // pinned here is that `stepRustCursor` is reached ONLY from a pop, so the
    // handoff's own walker move and a host step cannot both happen for one press.
    // Every configuration that issues a step issues exactly one, asserted above;
    // this case pins the structural half - a declined handoff plus a pop is still
    // ONE step.
    const { history, engine, commands } = mountCommands();
    await metadataOp(history, engine);
    await shimRecorded(1);

    commands.undo();
    await until(() => cursorStepInvokes() >= 1);
    commands.redo();
    await until(() => cursorStepInvokes() >= 2);

    expect(cursorStepInvokes(), "one step per press, undo and redo alike").toBe(2);
    expect(streamFor(DOC).cursor, "the redo put the entry back").toBe(1);
  });

  it("MATRIX: a TS-owned tile commit also yields ONE entry and ONE step", async () => {
    // The shim mirrors tile commits too, so one host commit must still produce
    // exactly one Rust entry and one step.
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
      "ONE host entry produced exactly ONE Rust entry",
    ).toBe(1);

    commands.undo();
    await until(() => cursorStepInvokes() >= 1);
    expect(cursorStepInvokes(), "exactly one step").toBe(1);
    await settle();
  });

  it("MATRIX rustOwned: entryArm fires, and neither recorder appends a second entry", async () => {
    // The mirror of the co-firing cell: a Rust-OWNED entry. entryArm is true via
    // its first clause and shimArm is false (`rustOwned !== true`), so the pop takes
    // the entryArm path. The shim records nothing for a `rustOwned` commit
    // (`alreadyRecordedInRust`), so this entry's counterpart is the `Pixel` entry
    // `rust_pixels_write_region` already appended - and an ungated step over it is
    // correct.
    const { history, engine, renderer, commands } = mountCommands();
    await commitRustOwnedPaint(history, engine.snapshot());
    await until(() => streamFor(DOC).entries.length >= 1);

    expect(streamFor(DOC).entries, "the pixel writer recorded the only entry").toEqual(["pixel"]);

    await pressUndo(commands, renderer, 1);
    expect(cursorStepInvokes(), "exactly one step").toBe(1);
    expect(streamFor(DOC).cursor, "the stroke's own Pixel entry was consumed").toBe(0);
    expect(streamFor(DOC).cursor).toBe(history.getUndoCount());
  });

});

/** A committed model for the tile case; `mountCommands`' engine is not needed. */
function engineLikeModel(): DocumentModel {
  return makeModel("before");
}
