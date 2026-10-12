// The undo/redo REFUSAL, driven through the shipped predicate.
//
// The host refuses an undo/redo that has no user work behind it. It reads the
// shared stream's cursor through `getHistoryQuery` and compares it to a
// one-entry document-open baseline floor. The undo button is enabled
// unconditionally under facade ownership, so the refusal is the ONLY thing
// standing between an extra keypress and undoing the document-open entry -
// which is why it must be exercised against the real function.
//
// This test calls `rustStreamHoldsUserWork` itself. A test-local copy of
// `cursor > floor` would keep passing even if the shipped predicate changed, so
// it is deliberately not done here.
//
// What it proves about the host-owned document-size pair (the Phase 1 change):
// a crop's External step is a REAL handled step, so it must not be swallowed,
// and undoing down past every user entry must end in a refusal - not in a silent
// undo of the baseline.

// What it proves about the refusal being HANDLED (the empty-stream answer): a
// document the native engine holds but whose stream carries nothing above the
// document-open baseline is the DEFAULT shape of every opened project - seeding
// records no history entry. Its press must be answered where it is decided,
// because the executor a fall-through would reach cannot restore a snapshot of
// facade-owned layers.

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { createNativeSeed, __resetNativeAuthorityForTests } from "@/lib/protocol/bridge";
import {
  getFacade,
  confirmExternalCursor,
  recordExternalTransitionFor,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { recordOpenBaselineEntry } from "@/lib/protocol/backgroundFlagRouting";
import { rustStreamHoldsUserWork } from "../facadeHistoryHandoff";
import { useEditorCommands } from "../useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { CommandHistory } from "@/engine/history";
import { DocumentEngine, facadeOwnedIdsOf } from "@/engine/document";
import { showToast } from "../Toast";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import type { EditorContextValue } from "../shell/EditorContext";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));
// Spy on the toast, keep the rest of the module: the dispatcher reports a failed
// restore through it, and that report is what must not happen.
vi.mock("../Toast", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../Toast")>()),
  showToast: vi.fn(),
}));
const invokeMock = invoke as unknown as Mock<
  (cmd: string, args?: Record<string, unknown>) => Promise<unknown>
>;

if (typeof (globalThis as { localStorage?: unknown }).localStorage === "undefined") {
  const __ls = new Map<string, string>();
  (globalThis as { localStorage: Storage }).localStorage = {
    getItem: (k: string) => (__ls.has(k) ? (__ls.get(k) as string) : null),
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

/**
 * A stream whose cursor the refusal reads back, driven by a real
 * `RecordExternalTransition` / `Undo` / `CursorCommit` sequence. The cursor is
 * moved by the CursorCommit, exactly as the production two-phase handoff does,
 * so the value the refusal sees is produced by the same ordering the app uses.
 */
function routeStream(
  cursorAt: number[],
  opts: { baseline?: boolean; rejectExternalRecord?: boolean } = {},
): { cursors: number[] } {
  const cursors = cursorAt;
  const withBaseline = opts.baseline !== false;
  const rejectExternalRecord = opts.rejectExternalRecord === true;
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
      case "protocol_apply_command_native": {
        const env = JSON.parse((args.envelopeJson as string) ?? "{}") as {
          command: { type: string };
        };
        if (env.command.type === "recordExternalTransition") {
          if (rejectExternalRecord) throw "E_STUB: the external record was refused";
          cursors.push(cursors[cursors.length - 1] + 1);
          return JSON.stringify({
            documentVersion: cursors.length,
            delta: { baseVersion: 0, version: cursors.length, changes: [] },
            status: "external-recorded",
            externalSeq: cursors.length,
          });
        }
        if (env.command.type === "undo" || env.command.type === "redo") {
          return JSON.stringify({
            documentVersion: cursors.length,
            delta: { baseVersion: 0, version: cursors.length, changes: [] },
            status: "external",
            externalSeq: cursors.length,
          });
        }
        return JSON.stringify({
          documentVersion: cursors.length,
          delta: { baseVersion: 0, version: cursors.length, changes: [] },
          status: "ok",
        });
      }
      case "protocol_history_cursor_commit_native": {
        const seq = Number(args.seq ?? 0);
        const direction = String(args.direction ?? "undo");
        const idx = cursors.indexOf(seq);
        if (idx < 0) throw "E_CURSOR_MISMATCH";
        if (direction === "undo") cursors.splice(idx, 1);
        else cursors.splice(idx + 1, 0, seq);
        return JSON.stringify({
          documentVersion: cursors.length,
          delta: { baseVersion: 0, version: cursors.length, changes: [] },
          status: "external-confirmed",
          externalSeq: seq,
        });
      }
      case "protocol_history_query_native":
        return JSON.stringify({
          cursor: cursors.length,
          lastSeq: cursors.length,
          degradedHint: false,
          pendingExternal: null,
          entries: cursors.map((s, i) => ({ seq: s, groupId: s, origin: "native", label: withBaseline && i === 0 ? "Set Background Flag" : "e", affectedLayerIds: [], versionBefore: 0, versionAfter: 1, memoryCostBytes: 0 })),
        });
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
  return { cursors };
}

function editorFor(docId: string): EditorContextValue {
  return {
    workspace: {
      getActiveDocumentId: () => docId,
      getActiveEngine: () => null,
    },
  } as unknown as EditorContextValue;
}

/** The claims ledger is process-global; a leftover claim from another case would arm the dispatch gate here. */
function clearFacadeOwnedClaims(): void {
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
}

/**
 * A facade-OWNED document that also holds a legacy TS entry recorded before the
 * claim landed - the mixed shape. The host pop this entry feeds cannot restore
 * it: `DocumentEngine.restore` refuses a snapshot whose layers the facade owns.
 */
function ownedDocumentWithLegacyEntry(docId: string): {
  engine: DocumentEngine;
  history: CommandHistory;
} {
  const engine = new DocumentEngine(docId, "Owned", 8, 8);
  const history = new CommandHistory();
  history.attachDocIdGetter(() => docId);
  history.commit(engine.snapshot(), "Legacy: before ownership");
  engine.addLayer("Owned");
  engine.adoptFacadeOwnership();
  return { engine, history };
}

/** Everything a press could disturb that the user would see. */
function layerVector(engine: DocumentEngine): string {
  return JSON.stringify(
    engine.getLayers().map((l) => ({
      id: l.id,
      name: l.name,
      x: l.transform.x,
      y: l.transform.y,
      scaleX: l.transform.scaleX,
      scaleY: l.transform.scaleY,
      rotation: l.transform.rotation,
      opacity: l.opacity,
      visible: l.visible,
    })),
  );
}

/** The native commands the press actually issued, by envelope command type. */
function appliedCommandTypes(): string[] {
  return invokeMock.mock.calls.flatMap(([cmd, args]) => {
    if (cmd !== "protocol_apply_command_native") return [];
    const env = JSON.parse(
      ((args as { envelopeJson?: string } | undefined)?.envelopeJson) ?? "{}",
    ) as { command?: { type?: string } };
    return env.command?.type ? [env.command.type] : [];
  });
}

/** How many times the native stream has been read on this wire. */
function historyQueries(): number {
  return invokeMock.mock.calls.filter(([cmd]) => cmd === "protocol_history_query_native").length;
}

function toastCalls(): number {
  return (showToast as unknown as { mock?: { calls: unknown[] } }).mock?.calls.length ?? 0;
}

/** Poll until the fire-and-forget press is observably over, or it plainly stalled. */
async function untilSettled(done: () => boolean, timeoutMs = 5000): Promise<void> {
  const start = Date.now();
  while (!done()) {
    if (Date.now() - start > timeoutMs) return;
    await flush();
  }
}

function dispatcherFor(
  docId: string,
  engine: DocumentEngine,
  history: CommandHistory,
): Record<string, unknown> {
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => docId,
      notifyVisualChange: vi.fn(),
    },
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
    scheduler: { requestRender: vi.fn() },
    activeDocumentId: () => docId,
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
  };
}

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

describe("undo/redo refusal: the shipped predicate, not a reimplementation", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockReset();
    __resetNativeAuthorityForTests();
    __resetFacadeRegistryForTests();
    clearFacadeOwnedClaims();
  });
  afterEach(() => {
    localStorage.clear();
    __resetNativeAuthorityForTests();
    __resetFacadeRegistryForTests();
    clearFacadeOwnedClaims();
    vi.restoreAllMocks();
  });

  it("refuses the press that would undo the document-open baseline", async () => {
    const docId = "docRefuse";
    // One entry: the document-open baseline. Nothing the user did.
    routeStream([1]);
    await createNativeSeed(docId, 0, []);
    recordOpenBaselineEntry(docId);

    const editor = editorFor(docId);
    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(false);
  });

  it("allows the press while a user entry sits below the cursor", async () => {
    const docId = "docAllow";
    // Baseline + one user entry (a crop).
    routeStream([1, 2]);
    await createNativeSeed(docId, 0, []);
    recordOpenBaselineEntry(docId);

    const editor = editorFor(docId);
    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(true);
  });

  it("walks a crop stream down to the floor, and the last press is refused", async () => {
    const docId = "docWalk";
    // Baseline + two user entries (crop, then crop again).
    const { cursors } = routeStream([1, 2, 3]);
    await createNativeSeed(docId, 0, []);
    recordOpenBaselineEntry(docId);
    const editor = editorFor(docId);

    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(true);

    // Undo the topmost entry through the real two-phase handoff: the walker
    // steps the cursor only once the host CONFIRMS, which is what the shipped
    // `runFacadeExternalHandoff` does.
    const facade = getFacade(docId);
    await facade.undo();
    expect(facade.lastExternalHandoff).not.toBeNull();
    const first = confirmExternalCursor(
      docId,
      facade.lastExternalHandoff!.seq,
      facade.lastExternalHandoff!.direction,
    );
    expect((await first).ok).toBe(true);
    expect(cursors.length).toBe(2);
    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(true);

    // Undo the second entry: only the baseline is left.
    await facade.undo();
    expect(facade.lastExternalHandoff).not.toBeNull();
    const second = confirmExternalCursor(
      docId,
      facade.lastExternalHandoff!.seq,
      facade.lastExternalHandoff!.direction,
    );
    expect((await second).ok).toBe(true);
    expect(cursors.length).toBe(1);
    // The press past the last user entry must be REFUSED: the engine would otherwise
    // undo the document-open entry.
    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(false);

    // Undo stays refused on a further press: the engine's cursor is already at the
    // floor, so no amount of pressing invents user work. This is the property a
    // frozen cursor would break, and it is why the refusal must read the real
    // cursor rather than a cached count.
    expect(await rustStreamHoldsUserWork(editor, "undo")).toBe(false);
  });

  it("assumes work when the baseline cannot be proven (never refuses wrongly)", async () => {
    const docId = "docNoBaseline";
    routeStream([1], { baseline: false });
    await createNativeSeed(docId, 0, []);
    // deliberately NOT recorded: exercises the "floor is 0" branch

    // floor 0 with cursor 1 -> one entry below, which IS user work here.
    expect(await rustStreamHoldsUserWork(editorFor(docId), "undo")).toBe(true);
  });
});

describe("the refusal is HANDLED: the press never reaches a host executor", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockReset();
    __resetNativeAuthorityForTests();
    __resetFacadeRegistryForTests();
    clearFacadeOwnedClaims();
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    vi.spyOn(console, "info").mockImplementation(() => {});
    vi.spyOn(console, "warn").mockImplementation(() => {});
  });
  afterEach(() => {
    localStorage.clear();
    __resetNativeAuthorityForTests();
    __resetFacadeRegistryForTests();
    clearFacadeOwnedClaims();
    vi.restoreAllMocks();
  });

  /**
   * The four properties every answered-empty press must hold, asserted against
   * one real press through the real dispatcher.
   *
   * `expect(undoSpy).not.toHaveBeenCalled()` is the load-bearing one: it is what a
   * fall-through cannot avoid. The legacy entry stays spendable, the document
   * keeps its layers, nothing is driven on the native side, and the user is told
   * nothing - because there was nothing to undo.
   */
  async function expectPressAnswered(docId: string, cursors: number[]): Promise<void> {
    const { engine, history } = ownedDocumentWithLegacyEntry(docId);
    expect(
      facadeOwnedIdsOf(docId)?.size,
      "premise: the ACTIVE document owns a facade layer, so the dispatch gate is on",
    ).toBeGreaterThan(0);

    const vectorBefore = layerVector(engine);
    const undoSpy = vi.spyOn(history, "undo");
    mockUseEditor(dispatcherFor(docId, engine, history));
    const commands = useEditorCommands(() => {});
    const queriesBefore = historyQueries();

    commands.undo();
    // `restoreHistorySnapshot` is fire-and-forget, so the assertions below would
    // otherwise run before the press had done anything - a test that passes
    // because nothing happened yet. The press is over once it has read the
    // native stream a second time (the handoff's refusal read plus the resync),
    // or once it reported a failure, whichever comes first.
    await untilSettled(
      () =>
        undoSpy.mock.calls.length > 0 ||
        toastCalls() > 0 ||
        historyQueries() >= queriesBefore + 2,
    );
    // The pop runs before the restore it feeds, so a press that reached the host
    // executor needs a few more turns before its failure can surface.
    for (let i = 0; i < 10; i++) await flush();

    expect(undoSpy, "the host pop never ran: no legacy entry was spent").not.toHaveBeenCalled();
    expect(
      showToast,
      "nothing is reported: the shipped 'Undo failed: E_FACADE_OWNED' must not fire",
    ).not.toHaveBeenCalled();
    expect(layerVector(engine), "the layer vector is byte-identical").toBe(vectorBefore);
    expect(cursors, "the native cursor did not move").toEqual([1]);
    expect(
      appliedCommandTypes(),
      "the press was answered before the walker: no undo/redo reached the engine",
    ).not.toContain("undo");
    expect(appliedCommandTypes()).not.toContain("redo");
  }

  it("an owned document whose stream holds only the document-open baseline", async () => {
    const docId = "docEmptyPress";
    const { cursors } = routeStream([1]);
    await createNativeSeed(docId, 0, []);
    recordOpenBaselineEntry(docId);

    await expectPressAnswered(docId, cursors);
  });

  it("the same press when the session's records never reached the stream", async () => {
    // The degraded shape: the shim's `recordExternalTransitionFor` is refused, so
    // the work this session did exists only on the host and the stream still shows
    // the document-open baseline alone. The stream is what the refusal reads, and
    // it says the same thing here - which is why the answer must not depend on how
    // the stream got there.
    const docId = "docDegradedPress";
    const { cursors } = routeStream([1], { rejectExternalRecord: true });
    await createNativeSeed(docId, 0, []);
    recordOpenBaselineEntry(docId);

    const rec = await recordExternalTransitionFor(docId, {
      label: "Legacy Edit",
      affectedLayerIds: [],
      snapshot: null,
    });
    expect(rec.ok, "premise: the external record was refused").toBe(false);
    expect(cursors, "premise: the stream never learned about the session's work").toEqual([1]);

    await expectPressAnswered(docId, cursors);
  });
});