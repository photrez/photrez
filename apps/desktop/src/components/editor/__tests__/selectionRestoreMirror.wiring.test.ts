// Undo/redo must mirror the RESTORED host selection into the native shadow.
//
// engine.restore(snapshot) replaces model.selection from the popped history
// snapshot, but the native shadow's selection is only touched by a native
// selection command. Without a mirror the shadow keeps its pre-undo rect, and the
// next native selectAll/invert (which reads the engine's own active selection)
// diverges from the host. restoreHistorySnapshot dispatches the same serialized
// funnel (mirrorRestoredSelection) in BOTH directions.
//
// MOCK FIDELITY: the native transport is routed to the real TS emulator
// (emulateApply), so the shadow state asserted is the production emulator's and
// the funnel/commit helpers are the real ones. hasFacadeOwnedLayers is forced
// false so the legacy restore path (where the mirror lives) is exercised; the
// facade-handoff branch returns before restore and is covered by the handoff
// tests.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useEditorCommands } from "../useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import * as bridge from "@/lib/protocol/bridge";
import { nativeProtocol } from "@/lib/protocol/nativeClient";
import { CONTRACT_VERSION } from "@/lib/protocol/types";
import { __resetFacadeRegistryForTests } from "@/lib/protocol/facadeRegistry";

vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: () => false,
  runTauriWindowAction: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));
vi.mock("@/engine/document", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  hasFacadeOwnedLayers: () => false,
}));

const DOC_ID = "restore-doc";
const FULL = { x: 0, y: 0, width: 800, height: 600, angle: 0 };

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

function routeNativeToEmu(): void {
  vi.spyOn(nativeProtocol, "protocol_version_native").mockImplementation(async () =>
    bridge.emulateGetSnapshot().version,
  );
  vi.spyOn(nativeProtocol, "protocol_snapshot_native").mockImplementation(async () =>
    JSON.stringify(bridge.emulateGetSnapshot()),
  );
  vi.spyOn(nativeProtocol, "protocol_apply_command_native").mockImplementation(
    async (json: string) => {
      const env = JSON.parse(json) as {
        contractVersion: number;
        expectedVersion?: number;
        command: Record<string, unknown>;
      };
      return JSON.stringify(
        bridge.emulateApply({
          contractVersion: env.contractVersion,
          expectedVersion: env.expectedVersion,
          command: env.command as never,
        }),
      );
    },
  );
}

// A fake engine whose restore() applies the snapshot's selection exactly like
// DocumentEngine (model.selection <- snapshot.selection). The host stays the
// visual authority; only the native shadow is asserted.
function makeContext() {
  const state: { sel: unknown } = { sel: null };
  const engine = {
    getId: () => DOC_ID,
    getSelection: () => state.sel,
    restore: (snap: { selection?: unknown }) => {
      state.sel = snap?.selection ?? null;
    },
    snapshot: () => ({ layers: [], activeLayerId: null, selection: state.sel }),
    getLayers: () => [],
  };
  const history = {
    canUndo: () => true,
    canRedo: () => true,
    // Undo restores the pre-select-all state (no selection); redo restores the
    // select-all state (full canvas). This makes the mirror fire in BOTH
    // directions with a different payload each way.
    undo: () => ({ layers: [], activeLayerId: null, selection: null }),
    redo: () => ({ layers: [], activeLayerId: null, selection: { ...FULL } }),
    consumeLastUndoPatches: () => null,
    consumeLastRedoPatches: () => null,
    isLastPoppedSnapshotEntry: () => false,
  };
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => DOC_ID,
      notifyVisualChange: vi.fn(),
      isFull: () => false,
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

describe("undo/redo mirrors the restored selection into the native shadow", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "native");
    bridge.__resetEmulatedForTests();
    bridge.__resetNativeAuthorityForTests();
    __resetFacadeRegistryForTests();
    routeNativeToEmu();
  });

  afterEach(() => {
    localStorage.removeItem("photrez.facade");
    localStorage.removeItem("photrez.facadeAuthority");
    vi.restoreAllMocks();
  });

  it("undo clears the shadow, redo restores it (both directions tracked)", async () => {
    // Pre-seed the shadow with a full-canvas selection, as if a routed select-all
    // had already synced it. The emulator needs explicit dims for selectAll.
    bridge.setEmuDocumentDims(800, 600);
    bridge.emulateApply({
      contractVersion: CONTRACT_VERSION,
      command: { type: "selectAll" },
    });
    expect(bridge.getEmuSelection()).toEqual(FULL);

    mockUseEditor(makeContext());
    const commands = useEditorCommands(() => {});

    // Undo restores the pre-select state (null) and must clear the shadow.
    commands.undo();
    await flush();
    expect(bridge.getEmuSelection()).toBeNull();

    // Redo restores the select-all state (full canvas) and must re-seed the shadow.
    commands.redo();
    await flush();
    expect(bridge.getEmuSelection()).toEqual(FULL);
  });

  it("photrez.facade=0 opt-out: undo/redo never dispatch a mirror (opt-out path byte-identical)", async () => {
    localStorage.setItem("photrez.facade", "0");
    bridge.setEmuDocumentDims(800, 600);
    bridge.emulateApply({
      contractVersion: CONTRACT_VERSION,
      command: { type: "selectAll" },
    });

    mockUseEditor(makeContext());
    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();

    // No mirror ran, so the shadow is exactly the pre-seeded select-all state.
    expect(bridge.getEmuSelection()).toEqual(FULL);
  });
});
