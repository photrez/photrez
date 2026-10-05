// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * One undo press must move the Rust cursor EXACTLY once, on the path where the facade
 * handoff moves it and then falls through to the host pop.
 *
 * THE PATH. `useEditorCommands` runs the facade handoff first (only when
 * `hasFacadeOwnedLayers()`); if it returns true the press is done and the host never
 * pops. It returns FALSE in two different situations, and only one of them leaves the
 * cursor where it was:
 *
 *   (a) Rust had no entry for this step (`rustStreamHoldsUserWork` refused, or the
 *       walker rejected with `E_EXTERNAL_PENDING`). The cursor is UNMOVED and the
 *       host pop must still step - the legacy store owns the entry.
 *   (b) The walker MOVED the cursor, `confirmExternalCursor` committed it, and the
 *       delta it got back was empty, so the branch reports unhandled
 *       (`facadeHistoryHandoff.ts`, the `lastHistoryDeltaWasEmpty` check). The host
 *       then pops a host entry whose recorder already had its counterpart consumed.
 *
 * (b) is reachable in production: a legacy `External` entry with no captured
 * after-state restores no layer changes, so `restore_external_layers` reports an
 * empty change set (`crates/core`, the external-restore arm) and the delta is empty.
 *
 * WHY IT MATTERED. The shim arm in `stepRustCursor` (added to close the cursor drift)
 * fires an UNGATED `rust_pixels_undo` for a metadata entry, on the grounds that the
 * shim recorded it and the tip therefore IS that entry. In case (b) that reasoning is
 * false - the handoff already consumed the entry - so one press issued TWO
 * `rust_pixels_undo` calls, the second landing on whatever entry sat below.
 * `handoffMovedCursor()` now reports the fact and `stepRustCursor` suppresses the
 * second step.
 *
 * MOCK FIDELITY. The Rust stream is the shared emulator (`rustStreamEmulator`), and
 * `facade.undo()` / `confirmExternalCursor` drive its real `step`, because that is
 * exactly what the native walker and the confirm command do. `facadeHistoryHandoff`
 * is NOT mocked - the fall-through this file exists to cover is decided inside it, and
 * mocking it is what let the defect through in the first place. The commit shim is not
 * installed: the subject here is the shim ARM's suppression, and its record is placed
 * on the stream directly, which `historyCursorDriftClosure.wiring.test.ts` proves for
 * real through the real shim.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

import { CommandHistory } from "@/engine/history";
import type { DocumentModel } from "@/engine/types";
import { useEditorCommands } from "../useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "@/components/editor/dialogs/DialogProvider";
import { invoke } from "@tauri-apps/api/core";
import { answerInvoke, record, resetStreams, streamFor } from "@/engine/__tests__/rustStreamEmulator";
import { cursorStepInvokes, settle } from "@/engine/__tests__/historyCursorHarness";
import { commitRustOwnedPaint } from "@/engine/__tests__/historyCursorFixtures";
import { handoffMovedCursor } from "../facadeHistoryHandoff";

const DOC = "doc-handoff-double-step";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));

// `hasFacadeOwnedLayers` must be TRUE or the handoff branch is never entered - which
// is precisely why every earlier fixture missed this defect.
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => true),
  isFacadeOwnedLayer: vi.fn(() => false),
}));

vi.mock("@/lib/protocol/bridge_emu", () => ({
  getHistoryQuery: vi.fn(async () => ({ entries: ["external"], cursor: 1 })),
}));

// The walker and the confirm command drive the emulated cursor, which is what they do
// for real: `facade.undo()` is the native history walker and `confirmExternalCursor`
// commits the cursor it moved.
vi.mock("@/lib/protocol/facadeRegistry", () => ({
  getFacade: vi.fn(),
  confirmExternalCursor: vi.fn(async () => {
    // One walker move, modelled on the emulator's own rules.
    const s = streamFor(DOC);
    if (s.cursor > 0) {
      s.cursor -= 1;
      s.version += 1;
    }
    return { ok: true };
  }),
  syncFacadeVersionFromPixel: vi.fn(),
  getExternalRecordSnapshot: vi.fn(() => null),
  parkExternalReplaySnapshot: vi.fn(),
  // Read by `stepRustCursor`'s shim arm, so the arm is genuinely armed here.
  isFacadeEnabled: vi.fn(() => true),
  historyDegraded: vi.fn(() => false),
}));

/** A walker move plus an EMPTY delta - the fall-through-after-moving case (b). */
function makeFacade(emptyDelta: boolean) {
  return {
    lastExternalHandoff: { seq: 1, direction: "undo" as const },
    lastExternalToken: null,
    lastHistoryDeltaWasEmpty: emptyDelta,
    undo: vi.fn(async () => ({ version: 1, layers: [] })),
    redo: vi.fn(async () => ({ version: 1, layers: [] })),
  };
}

function makeModel(name: string): DocumentModel {
  return {
    id: DOC,
    name,
    width: 10,
    height: 10,
    layers: [],
    activeLayerId: null,
    selection: null,
    viewport: { panX: 0, py: 0, zoom: 1, rotation: 0 },
    dirty: false,
  } as unknown as DocumentModel;
}

function mountCommands() {
  const history = new CommandHistory();
  history.attachDocIdGetter(() => DOC);
  let model = makeModel("before");
  const engine = {
    getId: () => DOC,
    getActiveLayerId: () => null,
    getLayer: () => null,
    getLayers: () => model.layers,
    snapshot: (): DocumentModel => model,
    restore: (snap: DocumentModel) => {
      model = snap;
    },
    getPaintSurface: () => null,
    getModel: () => model,
    applyFacadeSnapshot: vi.fn(),
    applyExternalRasterRestore: vi.fn(),
    ensureBitmapCurrent: vi.fn(),
    invalidatePaintSurface: vi.fn(),
  };
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

/**
 * A minimal editor double for driving the handoff directly.
 *
 * `getActiveDocumentId` is NOT optional: `rustStreamHoldsUserWork` resolves an absent
 * or unreadable doc id to "assume there is work" (fail-open, deliberately), so a double
 * without it never exercises the refusal branch. The engine must expose the surface the
 * handoff reads (`getLayers`, `snapshot`, the two apply methods), because the restore
 * sweep runs before the delta check - a thinner stub makes the branch bail early and
 * report `handled`, which is not the path under test.
 */
function editorDouble() {
  let model = makeModel("before");
  const engine = {
    getId: () => DOC,
    getActiveLayerId: () => null,
    getLayer: () => null,
    getLayers: () => model.layers,
    snapshot: (): DocumentModel => model,
    restore: (snap: DocumentModel) => {
      model = snap;
    },
    getPaintSurface: () => null,
    getModel: () => model,
    applyFacadeSnapshot: vi.fn(),
    applyExternalRasterRestore: vi.fn(),
    ensureBitmapCurrent: vi.fn(),
    invalidatePaintSurface: vi.fn(),
  };
  return {
    workspace: {
      getActiveDocumentId: () => DOC,
      getActiveEngine: () => engine,
      notifyVisualChange: vi.fn(),
    },
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
    scheduler: { requestRender: vi.fn() },
  } as never;
}

async function until(predicate: () => boolean, timeoutMs = 3000): Promise<void> {
  const start = Date.now();
  while (!predicate()) {
    if (Date.now() - start > timeoutMs) throw new Error("until: timed out");
    await new Promise<void>((r) => setTimeout(r, 5));
  }
  await settle();
}

/** A metadata commit plus the `External` entry the shim records for it. */
function commitMetadataShimRecorded(history: CommandHistory): void {
  history.commit(makeModel("before"), "Add Layer");
  record(DOC, "external");
}

describe("facade handoff: moved the cursor, then fell through", () => {
  beforeEach(async () => {
    resetStreams();
    localStorage.clear();
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<
      typeof DialogProviderModule.useDialog
    >);
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(console, "info").mockImplementation(() => {});
    vi.mocked(invoke).mockReset().mockImplementation((cmd: string, args: unknown) =>
      answerInvoke(cmd, args) as Promise<unknown>,
    );
    // The registry mock is module state, so a case that latches `historyDegraded`
    // ON would silently disarm the shim arm for every case that runs after it - which
    // reads as "press 2 did not step" rather than as the leak it actually is.
    const registry = vi.mocked(await import("@/lib/protocol/facadeRegistry"));
    // Both are Solid accessors whose own signature takes an optional value; the mocks
    // replace the READ, which is what the arms call.
    registry.historyDegraded.mockReset().mockImplementation(
      (() => false) as unknown as typeof registry.historyDegraded,
    );
    registry.isFacadeEnabled.mockReset().mockImplementation(
      (() => true) as unknown as typeof registry.isFacadeEnabled,
    );
    registry.confirmExternalCursor.mockClear();
    const bridgeEmu = vi.mocked(await import("@/lib/protocol/bridge_emu"));
    bridgeEmu.getHistoryQuery.mockReset().mockResolvedValue({
      entries: ["external"],
      cursor: 1,
    } as never);
    handoffMovedCursor(); // drain any flag a previous case left
  });

  it("the handoff reports it moved the cursor even though it returns false", async () => {
    const { runFacadeExternalHandoff } = await import("../facadeHistoryHandoff");
    const facadeRegistry = await import("@/lib/protocol/facadeRegistry");
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeFacade(true) as never);
    record(DOC, "external"); // the work the walker will consume

    const handled = await runFacadeExternalHandoff(editorDouble(), "undo");

    expect(handled, "the branch reports unhandled on an empty delta").toBe(false);
    expect(
      handoffMovedCursor(),
      "but it DID move the cursor - the caller must not step again",
    ).toBe(true);
  });

  it("one press moves the cursor exactly once: the handoff's move, and no host step", async () => {
    const facadeRegistry = await import("@/lib/protocol/facadeRegistry");
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeFacade(true) as never);
    const { history, engine, commands } = mountCommands();
    commitMetadataShimRecorded(history);
    engine.getModel().name = "after";
    await settle();

    expect(streamFor(DOC).entries, "the shim's entry is on the stream").toEqual(["external"]);
    expect(streamFor(DOC).cursor).toBe(1);
    expect(cursorStepInvokes(), "nothing has stepped yet").toBe(0);

    // The production press: the handoff moves the cursor and falls through, then the
    // host pops. The pop's shim arm is armed (facade ON, not degraded, metadata entry).
    commands.undo();
    await until(() => streamFor(DOC).cursor === 0);
    await settle();
    expect(
      history.getUndoCount(),
      "the host popped its metadata entry, so the pop really happened",
    ).toBe(0);

    expect(
      cursorStepInvokes(),
      "NO host `rust_pixels_undo` for this press - the handoff already consumed it",
    ).toBe(0);
    expect(
      streamFor(DOC).cursor,
      "one cursor move total, from the handoff",
    ).toBe(0);
  });

  it("case (a) still steps: the handoff refused WITHOUT moving the cursor", async () => {
    // The other fall-through. `rustStreamHoldsUserWork` refuses before driving the
    // walker, so the cursor is untouched and the host pop owns the step. Suppressing
    // here would strand every facade-owned undo, so the flag must NOT be set.
    const facadeRegistry = await import("@/lib/protocol/facadeRegistry");
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeFacade(false) as never);
    const bridgeEmu = await import("@/lib/protocol/bridge_emu");
    vi.mocked(bridgeEmu.getHistoryQuery).mockResolvedValue({
      entries: [],
      cursor: 0,
    } as never);

    const { runFacadeExternalHandoff } = await import("../facadeHistoryHandoff");
    const { history, engine, commands } = mountCommands();
    commitMetadataShimRecorded(history);
    await settle();

    const handled = await runFacadeExternalHandoff(editorDouble(), "undo");
    expect(handled, "nothing above the baseline: unhandled").toBe(false);
    expect(
      handoffMovedCursor(),
      "and the cursor was NOT moved, so the pop must still step",
    ).toBe(false);
    expect(streamFor(DOC).cursor, "the cursor is where it was").toBe(1);

    history.undo(engine.getModel());
    await until(() => cursorStepInvokes() >= 1);
    expect(cursorStepInvokes(), "the host pop issued the one step").toBe(1);

    // ── Press 2: the SAME history, but with nothing left to pop ──────────────
    // This is the instance-side leak. The flag is now consumed (the pop above read
    // it), so press 2 must pop normally OR be refused by canUndo() - but either way
    // it must leave the flag false for press 3.
    commands.undo();
    await settle();
    expect(
      cursorStepInvokes(),
      "press 2 has nothing to pop, so it adds no step",
    ).toBe(1);

    // ── Press 3: a fresh commit, then a pop that MUST step ───────────────────
    // If press 1's flag had survived its pop, this pop would read it and skip.
    commitMetadataShimRecorded(history);
    await settle();
    // Length 1, not 2: press 2's undo dropped the redo branch, so a new record
    // truncates the stream to the cursor - which is the emulator's real rule.
    expect(streamFor(DOC).entries.length, "the new commit is on the stream").toBe(1);
    commands.undo();
    await until(() => cursorStepInvokes() >= 2);
    expect(
      cursorStepInvokes(),
      "press 3 steps: no flag leaked across the refused press",
    ).toBe(2);
  });

  it("a latched historyDegraded takes the shim arm OFF, so no ungated step is possible", async () => {
    // `historyDegraded` is the shim's own `recordExternalTransitionFor` early-return,
    // and it is a LATCH: `setHistoryDegraded` is only ever cleared by
    // `__resetFacadeRegistryForTests`, so one failed external transition turns external
    // recording off for the rest of the session. `stepRustCursor`'s shim arm reads the
    // same latch, because the arm takes the UNGATED `fire()` path - ungated and
    // unrecorded together would consume whatever entry sat below.
    //
    // This file mocks `facadeRegistry` (it must, to drive the walker), which is what
    // makes the latch reachable at all: production exposes no setter for it.
    const facadeRegistry = await import("@/lib/protocol/facadeRegistry");
    // `historyDegraded` is a Solid accessor, so its own signature takes an optional
    // value; the mock replaces the READ, which is what the arm calls.
    vi.mocked(facadeRegistry.historyDegraded).mockImplementation(
      (() => true) as unknown as typeof facadeRegistry.historyDegraded,
    );

    const { history, engine } = mountCommands();
    commitMetadataShimRecorded(history);
    await settle();
    expect(streamFor(DOC).entries, "the entry is on the stream").toEqual(["external"]);

    history.undo(engine.getModel());
    await settle();

    expect(
      cursorStepInvokes(),
      "no step at all: with recording latched off, nothing stands behind an ungated one",
    ).toBe(0);
    expect(streamFor(DOC).cursor, "and the cursor is untouched").toBe(1);
  });

  it("the degraded clause gates the SHIM arm only: a rustOwned pop still steps", async () => {
    // The cell the counter flagged as uncovered. `historyDegraded()` is one conjunct
    // of the SHIM arm; `entryArm`'s first clause is `imperative.rustOwned === true`,
    // which no flag can turn off. A Rust-OWNED stroke is its own recorder -
    // `rust_pixels_write_region` appended the `Pixel` entry with nothing else involved -
    // so refusing it would strand the undo of the user's own paint.
    const facadeRegistry = await import("@/lib/protocol/facadeRegistry");
    vi.mocked(facadeRegistry.historyDegraded).mockImplementation(
      (() => true) as unknown as typeof facadeRegistry.historyDegraded,
    );
    // The handoff must DECLINE, or it reports handled and the host never pops - which
    // would make this case pass for the wrong reason.
    const bridgeEmu = await import("@/lib/protocol/bridge_emu");
    vi.mocked(bridgeEmu.getHistoryQuery).mockResolvedValue({
      entries: [],
      cursor: 0,
    } as never);
    const { history, engine, commands } = mountCommands();
    await commitRustOwnedPaint(history, engine.getModel());
    await settle();
    expect(streamFor(DOC).entries, "the stroke's own Pixel entry").toEqual(["pixel"]);

    commands.undo();
    await until(() => cursorStepInvokes() >= 1);

    expect(cursorStepInvokes(), "the pixel-writer arm is unaffected by the latch").toBe(1);
    expect(streamFor(DOC).cursor, "the stroke's entry was consumed").toBe(0);
  });

  it("LEAK: a press that moves the cursor but cannot pop must not arm the NEXT pop", async () => {
    // The instance-side leak, and it needs all three of: the handoff MOVED the cursor,
    // it fell through, and the host then REFUSED to pop. `canRestore` is false whenever
    // the TS stack is empty, which is exactly reachable here: the stream holds an entry
    // (so the walker has work) with no host entry to pair it with.
    //
    // `noteFacadeCursorMoved` is one-shot and only a pop consumes it, so arming the
    // history at the handoff call site leaves the flag set across a press that never
    // reached `stepRustCursor` - and the next press that DOES pop reads it and skips its
    // own step. One host entry permanently unstepped.
    const facadeRegistry = await import("@/lib/protocol/facadeRegistry");
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeFacade(true) as never);
    // An entry in the stream with NOTHING in the host stack.
    record(DOC, "external");
    const { history, commands } = mountCommands();
    expect(history.getUndoCount(), "the host stack starts empty").toBe(0);
    await settle();

    // ── Press 1: the handoff moves the cursor, falls through, and the host
    //    cannot pop, so the armed flag is never consumed. ─────────────────────
    commands.undo();
    await until(() => streamFor(DOC).cursor === 0);
    await settle();
    expect(cursorStepInvokes(), "press 1 popped nothing, so it stepped nothing").toBe(0);
    expect(history.getUndoCount(), "and the host stack is still empty").toBe(0);

    // ── Press 2: a real host entry, which MUST be stepped ───────────────────
    // The refusal is re-armed, so press 2's handoff declines BEFORE the walker and the
    // flag stays false. Without this the mock would still claim cursor 1 from press 1
    // while the real stream sits at 0, the handoff would "move" a cursor that did not
    // move, and the pop would be suppressed for a reason that has nothing to do with
    // the leak under test.
    const bridgeEmu = await import("@/lib/protocol/bridge_emu");
    vi.mocked(bridgeEmu.getHistoryQuery).mockResolvedValue({
      entries: [],
      cursor: 0,
    } as never);
    commitMetadataShimRecorded(history);
    await settle();
    expect(history.getUndoCount(), "there is now something to pop").toBe(1);

    commands.undo();
    await settle();
    await settle();
    expect(
      cursorStepInvokes(),
      "press 2 steps: the flag from press 1 must not outlive the press that set it",
    ).toBe(1);
    expect(history.getUndoCount(), "and it popped").toBe(0);
  });

  it("STALE FLAG: a handled press leaves nothing for the next press's early return", async () => {
    // The one-shot clear has to sit BEFORE every early return, and this is the only
    // sequence that proves it.
    //
    // Press 1 is HANDLED: the walker runs (which sets the module flag) and a non-empty
    // delta returns true. `useEditorCommands` returns immediately on true and never
    // calls `handoffMovedCursor()` - so the flag is still set when press 1 ends.
    //
    // Press 2 then takes the `rustStreamHoldsUserWork` refusal, which returns BEFORE
    // the walker and therefore before any interior clear. With the clear at its old
    // position, press 2's caller read the stale true, told the history the handoff had
    // moved the cursor, and press 2's pop skipped its own step - one host entry
    // permanently unstepped.
    const facadeRegistry = await import("@/lib/protocol/facadeRegistry");
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeFacade(false) as never);
    const bridgeEmu = await import("@/lib/protocol/bridge_emu");
    vi.mocked(bridgeEmu.getHistoryQuery).mockResolvedValue({
      entries: ["external"],
      cursor: 1,
    } as never);

    const { history, engine, commands } = mountCommands();
    commitMetadataShimRecorded(history);
    await settle();

    // ── Press 1: handled by the handoff (non-empty delta) ────────────────────
    commands.undo();
    await settle();
    await settle();
    expect(
      cursorStepInvokes(),
      "press 1 consumed the press - no host step at all",
    ).toBe(0);
    expect(
      history.getUndoCount(),
      "and the host stack is untouched, which is what 'handled' means: the caller " +
        "returned before the pop",
    ).toBe(1);
    // The module flag is deliberately NOT read here: production never reads it on a
    // handled press, which is precisely how it could go stale.

    // ── Press 2: the walker refusal, an early return ─────────────────────────
    vi.mocked(bridgeEmu.getHistoryQuery).mockResolvedValue({
      entries: [],
      cursor: 0,
    } as never);

    commands.undo();
    await settle();
    await settle();

    expect(
      cursorStepInvokes(),
      "press 2 STILL steps: the stale flag from press 1 must not suppress it",
    ).toBe(1);
    expect(history.getUndoCount(), "and it popped its host entry").toBe(0);
    expect(streamFor(DOC).cursor, "and its entry was consumed").toBe(0);
  });
});