/**
 * Wiring test for the native-authority heal re-push on history restore.
 *
 * The external-handoff heal re-push was MOVED OUT of confirmExternalCursor (which
 * now only clears the pending-external barrier) into the handoff-fallthrough branch
 * of useEditorCommands.restoreHistorySnapshot, firing AFTER engine.restore(snapshot).
 * The point of that timing: the re-pushed canonical payload must reflect the
 * POST-restore engine state, not the pre-restore snapshot. This test drives the REAL
 * undo path (commands.undo -> restoreHistorySnapshot -> runFacadeExternalHandoff
 * fallthrough -> engine.restore) and asserts the re-push fires exactly once with a
 * payload whose dims equal the restored snapshot dims, and that it is a no-op when
 * native authority is off.
 *
 * Driver: native authority + a native-seeded doc + an EMPTY-STACK undo. The facade
 * handoff sees "Rust had no entry" (status:"ok", empty delta) and falls through to
 * the legacy TS restore - the canonical documented fallthrough case
 * (useEditorCommands: runFacadeExternalHandoff returns false => handoffFellThrough).
 *
 * MOCK FIDELITY: the re-push payload is built from the live engine AFTER
 * engine.restore, so we assert CONTENT (JSON dims), not just spy firing - spy-only
 * would hide a wrong-timed payload.
 */
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { useEditorCommands } from "../useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import * as bridge from "@/lib/protocol/bridge";
import { __resetFacadeRegistryForTests } from "@/lib/protocol/facadeRegistry";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => true),
  isFacadeOwnedLayer: vi.fn(() => false),
}));

const DOC_ID = "docRestoreRepush";

const invokeMock = invoke as unknown as Mock<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>;

// Dims the engine reports BEFORE restore - chosen to differ from the restored
// snapshot so a correct (post-restore) re-push is distinguishable from a
// stale (pre-restore) one.
const PRE_RESTORE_W = 800;
const PRE_RESTORE_H = 600;
// Dims the legacy TS history entry restores - the re-push MUST reflect these.
const RESTORED_W = 1234;
const RESTORED_H = 567;

function routeNative(): void {
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
      case "protocol_apply_command_native":
        // Empty-stack undo: status "ok", empty delta => handoff falls through.
        return JSON.stringify({ documentVersion: 2, delta: { baseVersion: 1, version: 2, changes: [] }, status: "ok" });
      case "protocol_history_cursor_commit_native":
        return JSON.stringify({ documentVersion: 2, delta: { baseVersion: 1, version: 2, changes: [] }, status: "external-confirmed", externalSeq: 1 });
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

function makeEngineContext() {
  const engine = {
    _w: PRE_RESTORE_W,
    _h: PRE_RESTORE_H,
    getId: () => DOC_ID,
    getName: () => "D",
    getWidth: () => engine._w,
    getHeight: () => engine._h,
    getLayers: () => [{ id: "bg" }],
    getSelection: () => null,
    snapshot: () => ({ layers: [{ id: "bg" }], activeLayerId: null }),
    applyFacadeSnapshot: vi.fn(),
    // Restore updates the engine dims so a post-restore re-push reads the
    // RESTORED dims (the whole point of moving the re-push below engine.restore).
    restore: vi.fn((snap: { width: number; height: number }) => {
      engine._w = snap.width;
      engine._h = snap.height;
    }),
  };
  const history = {
    canUndo: () => true,
    canRedo: () => true,
    // Real snapshot with the RESTORED dims; no tile patches (tile fast-path
    // skipped) and no isLastPoppedSnapshotEntry (snapshot-token block skipped).
    undo: () => ({ layers: [{ id: "bg" }], width: RESTORED_W, height: RESTORED_H, activeLayerId: null }),
    redo: () => null,
    // Must exist: restoreHistorySnapshot calls consumeLastUndoPatches() before
    // engine.restore; an absent method would throw and skip the restore. Returning
    // null means the tile fast-path is skipped.
    consumeLastUndoPatches: () => null,
  };
  const ctx = {
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
  return { ctx, engine, history };
}

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

describe("history-restore heal re-push (handoff-fallthrough, native authority)", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    localStorage.clear();
    invokeMock.mockReset();
    bridge.__resetNativeAuthorityForTests();
    bridge.__resetEmulatedForTests();
    __resetFacadeRegistryForTests();
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("native-authority: on fallthrough restore, re-pushes canonical with RESTORED dims (not pre-restore)", async () => {
    localStorage.setItem("photrez.facadeAuthority", "native");
    routeNative();
    // Native seed arms the facade's native routing so its empty-stack undo routes
    // to the native engine and returns status:"ok" with an empty delta => fallthrough.
    await bridge.createNativeSeed(DOC_ID, 0, []);

    const seedSpy = vi
      .spyOn(bridge, "seedNativeCanonical")
      .mockResolvedValue(undefined as never);

    const { ctx, engine: dbgEngine } = makeEngineContext();
    const restoreSpy = vi.spyOn(dbgEngine, "restore");
    mockUseEditor(ctx);
    const commands = useEditorCommands(() => {});

    commands.undo();
    await flush();

    // engine.restore must run (proves the handoff fell through to legacy restore,
    // which is the only path that reaches the heal re-push).
    expect(restoreSpy).toHaveBeenCalled();

    // The re-push fired exactly once.
    expect(seedSpy).toHaveBeenCalledTimes(1);

    // Payload CONTENT: dims must equal the restored snapshot (1234x567), proving
    // the re-push ran AFTER engine.restore (pre-restore would be 800x600).
    const payload = JSON.parse(seedSpy.mock.calls[0][1] as string);
    expect(payload.id).toBe(DOC_ID);
    expect(payload.width).toBe(RESTORED_W);
    expect(payload.height).toBe(RESTORED_H);
    expect(payload.layers.map((l: { id: string }) => l.id)).toEqual(["bg"]);
  });

  it("native-authority OFF: fallthrough restore triggers no re-push", async () => {
    // localStorage cleared => isNativeAuthority() false.
    routeNative();
    await bridge.createNativeSeed(DOC_ID, 0, []);

    const seedSpy = vi
      .spyOn(bridge, "seedNativeCanonical")
      .mockResolvedValue(undefined as never);

    const { ctx } = makeEngineContext();
    mockUseEditor(ctx);
    const commands = useEditorCommands(() => {});

    commands.undo();
    await flush();

    expect(seedSpy).not.toHaveBeenCalled();
  });
});
