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

// The dispatch gate asks what the claims ledger holds for the ACTIVE document,
// so the mock must answer per document or the handoff branch is never entered -
// which is precisely why every earlier fixture missed this defect.
vi.mock("@/engine/document", () => ({
  facadeOwnedIdsOf: vi.fn(() => new Set<string>(["owned-layer"])),
  isFacadeOwnedLayer: vi.fn(() => false),
}));

vi.mock("@/lib/protocol/bridge_emu", () => ({
  getHistoryQuery: vi.fn(async () => ({
    entries: streamFor(DOC).entries,
    cursor: streamFor(DOC).cursor,
  })),
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
function makeFacade(emptyDelta: boolean, handoffDirection: "undo" | "redo" = "undo") {
  return {
    lastExternalHandoff: { seq: 1, direction: handoffDirection },
    lastExternalToken: null,
    lastHistoryDeltaWasEmpty: emptyDelta,
    undo: vi.fn(async () => ({ version: 1, layers: [] })),
    redo: vi.fn(async () => ({ version: 1, layers: [] })),
  };
}

/**
 * A walker whose native command is REJECTED: the remaining decline that reports
 * unhandled WITHOUT moving the cursor.
 *
 * It replaces the stream refusal these cases used to force. A refusal is now a
 * HANDLED no-op (nothing above the document-open baseline), so it can no longer
 * produce a press that declines the handoff and still pops the host stack - and
 * the flag-leak subject needs exactly that shape.
 */
function makeRejectingFacade() {
  return {
    lastExternalHandoff: null,
    lastExternalToken: null,
    lastHistoryDeltaWasEmpty: true,
    undo: vi.fn(async () => {
      throw new Error("E_STUB: the native command was rejected");
    }),
    redo: vi.fn(async () => {
      throw new Error("E_STUB: the native command was rejected");
    }),
  };
}

/** Answer the refusal predicate with the stream the emulator actually holds. */
async function useRealHistoryQuery(): Promise<void> {
  const bridgeEmu = await import("@/lib/protocol/bridge_emu");
  vi.mocked(bridgeEmu.getHistoryQuery).mockImplementation(async () => ({
    entries: streamFor(DOC).entries,
    cursor: streamFor(DOC).cursor,
  }) as never);
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
    // A Solid accessor whose own signature takes an optional value; the mock
    // replaces the READ, which is what the arms call.
    registry.historyDegraded.mockReset().mockImplementation(
      (() => false) as unknown as typeof registry.historyDegraded,
    );
    registry.confirmExternalCursor.mockClear();
    const bridgeEmu = vi.mocked(await import("@/lib/protocol/bridge_emu"));
    bridgeEmu.getHistoryQuery.mockReset().mockImplementation(async () => ({
      entries: streamFor(DOC).entries,
      cursor: streamFor(DOC).cursor,
    }) as never);
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

  it("a refused press is handled and spends nothing: no pop, no step, no flag", async () => {
    // The refusal branch. `rustStreamHoldsUserWork` refuses before driving the
    // walker, so the cursor is untouched - and that refusal is now the ANSWER:
    // a document whose stream holds nothing above the document-open baseline has
    // nothing to undo, exactly as the native executor reports at cursor 0. The
    // host pop could only spend a legacy entry and fail to restore it.
    const facadeRegistry = await import("@/lib/protocol/facadeRegistry");
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeFacade(false) as never);
    const bridgeEmu = await import("@/lib/protocol/bridge_emu");
    vi.mocked(bridgeEmu.getHistoryQuery).mockResolvedValue({
      entries: [],
      cursor: 0,
    } as never);

    const { runFacadeExternalHandoff } = await import("../facadeHistoryHandoff");
    const { history, engine } = mountCommands();
    commitMetadataShimRecorded(history);
    await settle();

    const handled = await runFacadeExternalHandoff(editorDouble(), "undo");
    expect(handled, "nothing above the baseline: handled, not a fall-through").toBe(true);
    expect(
      handoffMovedCursor(),
      "and the cursor was NOT moved, so nothing downstream may claim a step",
    ).toBe(false);
    expect(streamFor(DOC).cursor, "the cursor is where it was").toBe(1);
    expect(cursorStepInvokes(), "the refusal issued no step").toBe(0);

    // ── The press that DOES pop ────────────────────────────────────────────
    // The next decline the handoff can still report is a rejected native
    // command. It leaves the same flag false, so this pop must still step: a
    // handled press must arm nothing for the press after it.
    await useRealHistoryQuery();
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeRejectingFacade() as never);

    history.undo(engine.getModel());
    await until(() => cursorStepInvokes() >= 1);
    expect(cursorStepInvokes(), "the host pop issued the one step").toBe(1);
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
    // The pop is driven directly: what this cell measures is `stepRustCursor`'s arms,
    // and a press would now be answered by the refusal on this stream rather than
    // reach them at all.
    const { history, engine } = mountCommands();
    await commitRustOwnedPaint(history, engine.getModel());
    await settle();
    expect(streamFor(DOC).entries, "the stroke's own Pixel entry").toEqual(["pixel"]);

    history.undo(engine.getModel());
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
    const undoSpy = vi.spyOn(history, "undo");
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
    // The handoff declines WITHOUT moving the cursor (the native command is
    // rejected), so the pop below owns the step and the flag stays false.
    // Without this the mock would still claim cursor 1 from press 1 while the
    // real stream sits at 0, the handoff would "move" a cursor that did not
    // move, and the pop would be suppressed for a reason that has nothing to do
    // with the leak under test.
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeRejectingFacade() as never);
    await useRealHistoryQuery();
    commitMetadataShimRecorded(history);
    await settle();
    expect(history.getUndoCount(), "there is now something to pop").toBe(1);

    commands.undo();
    await until(() => cursorStepInvokes() >= 1);
    expect(
      cursorStepInvokes(),
      "press 2 steps: the flag from press 1 must not outlive the press that set it",
    ).toBe(1);
    expect(undoSpy, "and it popped its host entry").toHaveBeenCalled();
  });

  it("STALE FLAG: a handled press leaves nothing for the next press's early return", async () => {
    // The one-shot clear has to sit BEFORE every early return, and this is the only
    // sequence that proves it.
    //
    // Press 1 is HANDLED: the walker runs (which sets the module flag) and a non-empty
    // delta returns true. `useEditorCommands` returns immediately on true and never
    // calls `handoffMovedCursor()` - so the flag is still set when press 1 ends.
    //
    // Press 2 then declines - the walker, and the cursor with it - before any
    // interior clear. With the clear at its old position, press 2's caller read
    // the stale true, told the history the handoff had moved the cursor, and
    // press 2's pop skipped its own step - one host entry permanently unstepped.
    const facadeRegistry = await import("@/lib/protocol/facadeRegistry");
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeFacade(false) as never);
    const bridgeEmu = await import("@/lib/protocol/bridge_emu");
    vi.mocked(bridgeEmu.getHistoryQuery).mockResolvedValue({
      entries: ["external"],
      cursor: 1,
    } as never);

    const { history, engine, commands } = mountCommands();
    const undoSpy = vi.spyOn(history, "undo");
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

    // ── Press 2: a decline that leaves the cursor alone ──────────────────────
    // The stream needs work of its own: a press that finds nothing undoable
    // natively never reaches the pop, and then there is no step to suppress.
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeRejectingFacade() as never);
    await useRealHistoryQuery();
    record(DOC, "external");
    await settle();
    expect(history.canUndo(), "premise: there is something to pop").toBe(true);

    commands.undo();
    await until(() => cursorStepInvokes() >= 1);

    expect(
      cursorStepInvokes(),
      "press 2 STILL steps: the stale flag from press 1 must not suppress it",
    ).toBe(1);
    expect(undoSpy, "and it popped its host entry").toHaveBeenCalled();
  });

  it("depth >= 2 case (b): one undo press moves the cursor 3 -> 2, never 3 -> 1", async () => {
    // The permanent guard for the shallow-fixture blind spot. At depth 1 the second
    // cursor step is absorbed by the boundary (the cursor cannot move below 0), so
    // only an invoke count could catch a double-step there. At depth >= 2 the
    // double-step is a MEASURED cursor move - one press measured 3 -> 2 (handoff
    // walker) -> 1 (ungated host step) - and this case pins the class, not just the
    // current machinery: whatever happens to `facadeCursorConsumed` next, one press
    // must move the cursor EXACTLY one step on this path.
    //
    // The TIP is a case-(b) External entry: one with no captured after-state, which
    // `restore_external_layers` answers with `(seq, Vec::new())` - the cursor moves
    // but the delta is EMPTY (crates/core/src/document_core.rs:750-762). The facade
    // double reports exactly that (a walker move + `lastHistoryDeltaWasEmpty`), and
    // `confirmExternalCursor` commits the move the way the real confirm does.
    const facadeRegistry = await import("@/lib/protocol/facadeRegistry");
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeFacade(true) as never);
    const { history, engine, commands } = mountCommands();
    // Stream depth 3, each entry the shim-recorded External a metadata commit mints.
    for (let i = 0; i < 3; i++) {
      commitMetadataShimRecorded(history);
      await settle();
    }
    expect(streamFor(DOC).entries, "the shim's entries are on the stream").toEqual([
      "external",
      "external",
      "external",
    ]);
    expect(streamFor(DOC).cursor, "stream depth 3").toBe(3);
    expect(history.getUndoCount(), "the host stack mirrors them").toBe(3);
    expect(cursorStepInvokes(), "nothing has stepped yet").toBe(0);

    // The production press: the handoff's walker moves the cursor (confirm commits
    // it, 3 -> 2), the empty delta reports unhandled, and the host pops its own
    // entry without adding a second cursor step.
    commands.undo();
    await until(() => history.getUndoCount() === 2);
    await settle();
    await settle();

    expect(history.getUndoCount(), "the host popped its metadata entry").toBe(2);
    expect(
      cursorStepInvokes(),
      "case (b) at depth 3: NO host `rust_pixels_undo` - the handoff already consumed the tip",
    ).toBe(0);
    expect(
      streamFor(DOC).cursor,
      "exactly one cursor step for the press: 3 -> 2, never 3 -> 1",
    ).toBe(2);
  });

  it("depth >= 2 case (b), REDO mirror: one redo press moves the cursor 1 -> 2, never 1 -> 3", async () => {
    // The same class in the redo direction. `restoreHistorySnapshot` serves both
    // directions, but the redo pop, the `facade.redo()` walker arm, and the confirm's
    // cursor move (UP, not down) are separate wiring, so the mirror earns its keep.
    const facadeRegistry = await import("@/lib/protocol/facadeRegistry");
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeFacade(true) as never);
    const { history, commands } = mountCommands();
    for (let i = 0; i < 3; i++) {
      commitMetadataShimRecorded(history);
      await settle();
    }
    // Build the redo branch with two UNDO presses through the same real production
    // press: each handoff moves the cursor DOWN (the confirm commits it) and falls
    // through, and each host pop is suppressed. That keeps the native view resynced
    // (a direct history.undo() would not, and the cached query would then LIE about
    // the stacks) and doubles as two more guard repetitions.
    commands.undo();
    await until(() => streamFor(DOC).cursor === 2);
    await settle();
    commands.undo();
    await until(() => streamFor(DOC).cursor === 1);
    await settle();
    expect(history.getUndoCount(), "one host entry left below").toBe(1);
    expect(history.getRedoCount(), "two redo entries above").toBe(2);
    expect(
      cursorStepInvokes(),
      "the prep presses are suppressed too: cursor 3 -> 2 -> 1, no host step",
    ).toBe(0);

    // The case-(b) REDO shape: the walker reports a redo handoff on an External entry
    // with no captured after-state (document_core.rs:750-762 counts the post-state
    // gap and returns an empty delta for this direction too), and the confirm commits
    // an UP move.
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(
      makeFacade(true, "redo") as never,
    );
    vi.mocked(facadeRegistry.confirmExternalCursor).mockImplementation(async () => {
      const s = streamFor(DOC);
      if (s.cursor < s.entries.length) {
        s.cursor += 1;
        s.version += 1;
      }
      return { ok: true };
    });

    commands.redo();
    await until(() => history.getRedoCount() === 1);
    await settle();
    await settle();

    expect(history.getRedoCount(), "the host popped its redo entry").toBe(1);
    expect(
      cursorStepInvokes(),
      "case (b) REDO at depth 3: NO host `rust_pixels_redo` - the handoff already consumed the entry",
    ).toBe(0);
    expect(
      streamFor(DOC).cursor,
      "exactly one cursor step for the redo press: 1 -> 2, never 1 -> 3",
    ).toBe(2);
  });
});