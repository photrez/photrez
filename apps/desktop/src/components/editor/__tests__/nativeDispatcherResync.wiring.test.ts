/**
 * The native-authority press path consumes CommandResult only, and the host
 * history stack is a derived view of protocol_history_query_native.
 *
 * (a) After a press, canUndo/canRedo come from the query, not the TS stack:
 *     destroying the TS stack must not change them.
 * (c) The press projects the returned {delta, pixelPatches?, externalToken?}
 *     onto the engine and re-syncs from the query.
 *
 * MOCK FIDELITY: the retired `photrez.facadeAuthority=wasm` flag used to
 * select the emulator arm of the protocol bridge. Production dispatch is
 * native-only now; this suite drives the emulator explicitly through the
 * shared seam below — the same arms the retired wasm arm fell through to,
 * reached now via an installed spy instead of a retired flag. A faithful
 * mirror of the Rust predicate, not the real wasm binary.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { useEditorCommands } from "../useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import * as facadeRegistry from "@/lib/protocol/facadeRegistry";
import * as bridge from "@/lib/protocol/bridge";
import { hasFacadeOwnedLayers } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { hasOpenBaselineEntry } from "@/lib/protocol/backgroundFlagRouting";
import { installEmuBridgeSpies } from "@/lib/protocol/__tests__/emulatorBridgeRoute";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn(() => false) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => true),
  facadeOwnedIdsOf: vi.fn(() => new Set<string>(["owned-layer"])),
  isFacadeOwnedLayer: vi.fn(() => false),
}));

import { isTauriRuntime } from "@/lib/desktop/tauriWindow";

const DOC_ID = "docS2";
const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

function makeEditorContext(history: CommandHistory) {
  const engine = {
    getId: () => DOC_ID,
    getLayer: () => null,
    getLayers: () => [],
    snapshot: () => ({ layers: [], activeLayerId: null }),
    applyFacadeSnapshot: vi.fn(),
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

describe("undo/redo press consumes CommandResult and the stack derives from the query", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    vi.mocked(hasFacadeOwnedLayers).mockReturnValue(true);
    installEmuBridgeSpies(bridge);
    facadeRegistry.__resetFacadeRegistryForTests();
    bridge.__resetEmulatedForTests();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("killing the TS stack cannot change canUndo/canRedo after a resync from the query", async () => {
    const facade = facadeRegistry.getFacade(DOC_ID);
    await facade.addLayer("X");
    await facade.addLayer("Y");
    const query = await bridge.getHistoryQuery(DOC_ID);
    expect(query.cursor).toBe(2);

    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_ID);
    await history.resyncFromNativeQuery();

    const floor = hasOpenBaselineEntry(DOC_ID) ? 1 : 0;
    const expected = query.cursor > floor;
    expect(history.canUndo()).toBe(expected);
    expect(history.canRedo()).toBe(query.entries.length > query.cursor);

    history.clear(); // the TS stacks are dropped entirely
    expect(history.canUndo(), "counts must derive from the query, not the stack").toBe(expected);
    expect(history.canRedo()).toBe(query.entries.length > query.cursor);
    expect(history.getUndoCount()).toBe(Math.max(0, query.cursor - floor));
  });

  it("a single undo press projects the returned delta and re-syncs from the query", async () => {
    const facade = facadeRegistry.getFacade(DOC_ID);
    await facade.addLayer("X");
    const before = await bridge.getHistoryQuery(DOC_ID);
    expect(before.cursor).toBe(1);

    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_ID);
    // Populate the native query view first. `canUndo()` reads the cached
    // native query (`nativeQueryAuthoritative()` returns false when the cache
    // is empty), so without this the press's `isEnabled("edit.undo")` gate
    // returns false and `restoreHistorySnapshot` never runs. In production
    // `history.commit()` refreshes the same cache via its settle-then-resync;
    // this test drives facade commands only, so it performs the resync
    // explicitly before pressing.
    await history.resyncFromNativeQuery();
    const editor = makeEditorContext(history);
    mockUseEditor(editor);
    const commands = useEditorCommands(() => {});

    commands.undo();
    await flush();
    await flush();

    const after = await bridge.getHistoryQuery(DOC_ID);
    expect(after.cursor, "the native cursor moved").toBe(0);
    expect(editor.workspace.getActiveEngine().applyFacadeSnapshot).toHaveBeenCalled();
    // The host stack now mirrors the query a fresh read sees:
    const floor = hasOpenBaselineEntry(DOC_ID) ? 1 : 0;
    expect(history.canUndo()).toBe(after.cursor > floor);
    expect(history.canRedo()).toBe(after.entries.length > after.cursor);
  });
});
