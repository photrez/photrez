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
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";
import { handoffMovedCursor } from "../facadeHistoryHandoff";

async function settle() {
  await new Promise<void>((r) => setTimeout(r, 0));
  await flushPixelInvokeCensus();
}

/** Count the cursor-step invokes in EITHER direction. */
const cursorStepInvokes = (): number =>
  vi.mocked(invoke).mock.calls.filter(
    (c) => c[0] === "rust_pixels_undo" || c[0] === "rust_pixels_redo",
  ).length;

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

  it("the handoff reports it moved the cursor and is handled, so no host step follows", async () => {
    const { runFacadeExternalHandoff } = await import("../facadeHistoryHandoff");
    const facadeRegistry = await import("@/lib/protocol/facadeRegistry");
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeFacade(true) as never);
    record(DOC, "external"); // the work the walker will consume

    const handled = await runFacadeExternalHandoff(editorDouble(), "undo");
    await settle();

    // The legacy TS pop is retired, so the empty-delta branch no longer
    // falls through - it is a HANDLED no-op. The cursor move is still recorded on
    // the one-shot flag, which is what keeps the native jump loop from double-stepping.
    expect(handled, "the branch reports handled: no fall-through to a host pop").toBe(true);
    expect(
      handoffMovedCursor(),
      "but it DID move the cursor - the caller must not step again",
    ).toBe(true);
    expect(streamFor(DOC).cursor, "the walker's single move: 1 -> 0").toBe(0);
    expect(cursorStepInvokes(), "and no host rust_pixels_undo fired").toBe(0);
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

    // The production press: the handoff walks the cursor (1 -> 0) and is handled -
    // no host pop can step a second time on the same entry.
    commands.undo();
    await until(() => streamFor(DOC).cursor === 0);
    await settle();
    expect(
      history.getUndoCount(),
      "the handoff consumed the entry, so the cursor is at the floor",
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

  it("a refused press is a handled no-op: no step, cursor untouched", async () => {
    // A cursor-0 press is answered by the handoff as a handled no-op
    // (nothing above the baseline), not a fall-through to a host pop. The legacy
    // `rustPixels`/stepper machinery the earlier cases drove is retired, so the
    // only invariant left is the handled-nothing contract.
    const facadeRegistry = await import("@/lib/protocol/facadeRegistry");
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(makeRejectingFacade() as never);
    const bridgeEmu = await import("@/lib/protocol/bridge_emu");
    vi.mocked(bridgeEmu.getHistoryQuery).mockResolvedValue({
      entries: [],
      cursor: 0,
    } as never);

    const { runFacadeExternalHandoff } = await import("../facadeHistoryHandoff");
    const handled = await runFacadeExternalHandoff(editorDouble(), "undo");
    await settle();

    expect(handled, "the refusal is handled, not a fall-through").toBe(true);
    expect(handoffMovedCursor(), "the cursor was NOT moved").toBe(false);
    expect(streamFor(DOC).cursor, "the cursor is where it was").toBe(0);
    expect(cursorStepInvokes(), "the refusal issued no step").toBe(0);
  });

  // NOTE: the "LEAK"/"STALE FLAG" cases here historically pinned
  // `noteFacadeCursorMoved` being read by the now-deleted host pop in
  // restoreHistorySnapshot. That read site is gone, so the leak class
  // is structurally impossible; the one-shot clearing-at-entry invariant now lives
  // with the native jump loop that still reads `handoffMovedCursor()`.

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
    const { history, commands } = mountCommands();
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
    expect(history.getUndoCount(), "the native view reflects the depth").toBe(3);
    expect(cursorStepInvokes(), "nothing has stepped yet").toBe(0);

    // The production press: the handoff's walker moves the cursor (confirm commits
    // it, 3 -> 2) and is handled - no host pop steps a second time.
    commands.undo();
    await until(() => history.getUndoCount() === 2);
    await settle();
    await settle();

    expect(history.getUndoCount(), "the handoff consumed one entry").toBe(2);
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
    expect(history.getUndoCount(), "one entry left below the cursor").toBe(1);
    expect(history.getRedoCount(), "two entries above").toBe(2);
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

    expect(history.getRedoCount(), "the handoff consumed the redo entry").toBe(1);
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