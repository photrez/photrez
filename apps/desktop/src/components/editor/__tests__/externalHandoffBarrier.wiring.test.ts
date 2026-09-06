/**
 * Wiring test for the external-history-handoff barrier (history-unification).
 *
 * Bug: when a facade undo/redo lands on a legacy (external) history entry, the
 * Rust walker sets a pending-external barrier and returns status:"external"
 * WITHOUT moving the cursor. The production path never cleared that barrier
 * (confirmExternalCursor / historyCursorCommit had no production caller), so
 * every subsequent facade command permanently rejected with E_EXTERNAL_PENDING
 * and the engine was wedged. The swallow at useEditorCommands also meant
 * historyDegraded was never set.
 *
 * This test fires the REAL undo/redo path (useEditorCommands.undo/redo ->
 * restoreHistorySnapshot -> runFacadeExternalHandoff) and asserts the barrier is
 * cleared on an external handoff, that a forced commit failure sets
 * historyDegraded (signaled; surfacing to the UI is a separate follow-up, there
 * is no consumer in this change), and that the facade branch is NOT entered when
 * there are no facade-owned layers.
 *
 * MOCK FIDELITY: localStorage is cleared, so photrez.facade is unset and these
 * tests exercise the emulator arm of historyCursorCommit (a faithful mirror of
 * the Rust predicate), NOT the real wasm binary. facade=1 in production requires
 * an armed wasm, so this is an emulator-only path - labeled as such and not
 * claimed to be the production wasm behavior. The gapped (non-dense) cases are
 * the crux: the old index-arithmetic predicate (cursor == seq) failed there and
 * left the wedge permanently set.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { useEditorCommands } from "../useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import * as facadeRegistry from "@/lib/protocol/facadeRegistry";
import * as bridge from "@/lib/protocol/bridge";
import { hasFacadeOwnedLayers } from "@/engine/document";

// Tauri runtime + dynamic imports used by the hook's tile path.
vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@/tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@/tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));

// Facade-branch gate. Per-test configurable so we can exercise both the
// branch-entered (facade-owned layers) and branch-skipped (no facade-owned
// layers) paths.
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => true),
  isFacadeOwnedLayer: vi.fn(() => false),
}));

const DOC_ID = "docWedge";

function makeEngineContext() {
  const engine = {
    getId: () => DOC_ID,
    getLayer: () => null,
    snapshot: () => ({ layers: [], activeLayerId: null }),
    applyFacadeSnapshot: vi.fn(),
  };
  // The legacy history returns null snapshots, so the real fall-through path
  // reaches history.undo/redo and early-returns on the null result (rather than
  // on the canUndo gate) - exercising the genuine fall-through, not a shortcut.
  const history = {
    canUndo: () => true,
    canRedo: () => true,
    undo: () => null,
    redo: () => null,
  };
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => DOC_ID,
      notifyVisualChange: vi.fn(),
    },
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
    scheduler: { requestRender: vi.fn() },
    activeDocumentId: () => DOC_ID,
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

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

// Builds a NON-DENSE (gapped) history stream for DOC_ID:
//   record A (seq1) -> addLayer X (seq2) -> undo seq2 -> record B (truncates
//   redo, so the next monotonic seq is 3) -> entries=[seq1(native), seq3(external)]
// The cursor ends at 2; entries[1].seq is 3, so entries[i].seq != i+1.
async function buildNonDenseStream(docId: string) {
  await facadeRegistry.recordExternalTransitionFor(docId, {
    label: "Legacy A",
    affectedLayerIds: [],
    snapshot: null,
  });
  const facade = facadeRegistry.getFacade(docId);
  await facade.addLayer("X");
  await facade.undo(); // undo the native addLayer X (no pending)
  await facadeRegistry.recordExternalTransitionFor(docId, {
    label: "Legacy B",
    affectedLayerIds: [],
    snapshot: null,
  });
  return facade;
}

describe("external history handoff barrier - facade undo/redo must clear the wedge (emulator path)", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    vi.mocked(hasFacadeOwnedLayers).mockReturnValue(true);
    localStorage.clear();
    facadeRegistry.__resetFacadeRegistryForTests();
    bridge.__resetEmulatedForTests();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("dense undo: external handoff clears the barrier so a following facade command succeeds", async () => {
    // Create a legacy (external) history entry for the doc.
    const rec = await facadeRegistry.recordExternalTransitionFor(DOC_ID, {
      label: "Legacy Edit",
      affectedLayerIds: [],
      snapshot: null,
    });
    expect(rec.ok).toBe(true);
    expect((await bridge.getHistoryQuery()).pendingExternal ?? null).toBeNull();

    const facade = facadeRegistry.getFacade(DOC_ID);
    mockUseEditor(makeEngineContext());
    const commands = useEditorCommands(() => {});

    // Fire the REAL undo path. This lands on the external entry, which sets the
    // barrier; the fix must clear it via confirmExternalCursor.
    commands.undo();
    await flush();

    // Barrier cleared (cursor committed past the external entry).
    expect((await bridge.getHistoryQuery()).pendingExternal ?? null).toBeNull();
    // A following facade command no longer rejects with E_EXTERNAL_PENDING.
    await expect(facade.addLayer("AfterHandoff")).resolves.not.toThrow();
    expect(facadeRegistry.historyDegraded()).toBeNull();
  });

  it("non-dense undo (gapped stream): cursor commit succeeds and the wedge is gone", async () => {
    // entries=[seq1(native), seq3(external)]; cursor=2. The old predicate
    // `cursor == seq` (2 == 3) is FALSE here, so it retained the barrier and
    // stuck historyDegraded forever. The fixed barrier-only predicate clears it.
    const facade = await buildNonDenseStream(DOC_ID);
    expect((await bridge.getHistoryQuery()).entries[1].seq).toBe(3);
    expect((await bridge.getHistoryQuery()).cursor).toBe(2);

    mockUseEditor(makeEngineContext());
    const commands = useEditorCommands(() => {});

    commands.undo();
    await flush();

    // Wedge cleared: barrier gone, cursor advanced past the external entry.
    expect((await bridge.getHistoryQuery()).pendingExternal ?? null).toBeNull();
    expect((await bridge.getHistoryQuery()).cursor).toBe(1);
    // End state changed: a following facade command now succeeds (no wedge).
    await expect(facade.addLayer("AfterWedge")).resolves.not.toThrow();
    expect(facadeRegistry.historyDegraded()).toBeNull();
  });

  it("non-dense redo (gapped stream): cursor commit succeeds in the redo direction", async () => {
    const facade = await buildNonDenseStream(DOC_ID);
    // Walk the cursor back onto the gapped external entry, then clear that
    // barrier so the cursor sits just below it for a redo handoff.
    await facade.undo(); // lands on seq3 (undo), pending set
    await facadeRegistry.confirmExternalCursor(DOC_ID, 3, "undo"); // clears, cursor=1
    expect((await bridge.getHistoryQuery()).pendingExternal ?? null).toBeNull();
    expect((await bridge.getHistoryQuery()).cursor).toBe(1);

    mockUseEditor(makeEngineContext());
    const commands = useEditorCommands(() => {});

    commands.redo();
    await flush();

    // Redo handoff on the gapped entry: barrier cleared, cursor advanced.
    expect((await bridge.getHistoryQuery()).pendingExternal ?? null).toBeNull();
    expect((await bridge.getHistoryQuery()).cursor).toBe(2);
    await expect(facade.addLayer("AfterRedoWedge")).resolves.not.toThrow();
    expect(facadeRegistry.historyDegraded()).toBeNull();
  });

  it("forced cursor-commit failure sets historyDegraded (signaled, not surfaced to UI in this change)", async () => {
    const commitSpy = vi
      .spyOn(bridge, "historyCursorCommit")
      .mockImplementation(() => {
        throw new Error("E_CURSOR_MISMATCH: forced");
      });

    await facadeRegistry.recordExternalTransitionFor(DOC_ID, {
      label: "Legacy Edit",
      affectedLayerIds: [],
      snapshot: null,
    });

    mockUseEditor(makeEngineContext());
    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();

    expect(commitSpy).toHaveBeenCalledTimes(1);
    const deg = facadeRegistry.historyDegraded();
    // HistoryDegraded has NO UI consumer in this change (surfacing is a separate
    // follow-up), so the failure is signaled internally but not shown to the
    // user - we must not claim non-silent behavior.
    expect(deg).not.toBeNull();
    expect(deg!.reason).toBe("CURSOR_COMMIT_FAILED");
  });

  it("no facade-owned layers: facade branch not entered (the hasFacadeOwnedLayers gate guards entry)", async () => {
    vi.mocked(hasFacadeOwnedLayers).mockReturnValue(false);
    const confirmSpy = vi.spyOn(facadeRegistry, "confirmExternalCursor");

    mockUseEditor(makeEngineContext());
    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();

    // With no facade-owned layers the branch is skipped entirely. This asserts
    // the hasFacadeOwnedLayers gate - not the production flag state - controls
    // entry; the production flag -> hasFacadeOwnedLayers mapping is covered
    // elsewhere.
    expect(confirmSpy).not.toHaveBeenCalled();
    expect(facadeRegistry.historyDegraded()).toBeNull();
  });
});
