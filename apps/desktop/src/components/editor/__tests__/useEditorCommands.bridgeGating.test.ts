/**
 * Regression wiring test for the undo/redo cursor-sync gate in useEditorCommands.
 *
 * Coverage gap: historyBridge.wiring.test.ts only scopes
 * history.ts — it CANNOT catch a bypass where useEditorCommands calls
 * rust_pixels_undo/rust_pixels_redo UNGATED on the tile undo/redo path. This
 * file drives the REAL undo/redo path (via the hook's returned `undo`/`redo`)
 * and asserts that the Tauri `invoke` is ONLY fired when the history bridge is
 * enabled (the only thing that creates a Rust history entry on commit).
 *
 * Scenarios:
 *   1. Default production (no photrez.rustPixels, no bridge gate,
 *      isTauriRuntime false) → invoke is NEVER called with
 *      rust_pixels_undo/rust_pixels_redo (the old bug: it always fired).
 *   2. Bridge gate "1" + tauri → invoke("rust_pixels_undo", ...) / redo called.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { useEditorCommands } from "../useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";

// Mock the Tauri-runtime detector (shared by history.ts::historyBridgeEnabled).
vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(),
}));

// Invoke spy: the hook dynamic-imports @tauri-apps/api/core inside the undo
// tile path, so vi.mock intercepts the dynamic import too. `invoke` is the spy.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// onMount of useEditorCommands calls listen()/getVersion() when the runtime
// detector is true — mock them so mounting is side-effect-safe.
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(() => Promise.resolve("0.0.0")),
}));

// The undo path checks hasFacadeOwnedLayers() (Rust-owned history). Default off.
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => false),
  isFacadeOwnedLayer: vi.fn(() => false),
}));

const GATE_KEY = "photrez.historyBridge";
const RUST_PIXELS_KEY = "photrez.rustPixels";

type WirePatch = {
  layerId: string;
  surfaceWidth: number;
  surfaceHeight: number;
  before: { x: number; y: number; w: number; h: number; data: number[] }[];
  after: { x: number; y: number; w: number; h: number; data: number[] }[];
};

const makePatches = (): WirePatch => ({
  layerId: "l1",
  surfaceWidth: 10,
  surfaceHeight: 10,
  before: [],
  after: [],
});

/** Minimal TS CommandHistory shape the undo tile path consumes. */
function makeHistory(patches: WirePatch) {
  const snapshot = { layers: [], activeLayerId: null };
  return {
    canUndo: () => true,
    canRedo: () => true,
    undo: () => snapshot,
    redo: () => snapshot,
    consumeLastUndoPatches: () => patches,
    consumeLastRedoPatches: () => patches,
  };
}

/** The editor context the hook body + useLayerActions destructure need. */
function makeEditorContext(patches: WirePatch, engine: Record<string, unknown>) {
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => makeHistory(patches),
      getActiveDocumentId: () => "doc-1",
      notifyVisualChange: () => {},
    },
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
    scheduler: { requestRender: vi.fn() },
    activeDocumentId: () => "doc-1",
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

function makeEngine() {
  const layer = { id: "l1", basicAdjustment: undefined, imageBitmap: undefined };
  return {
    getActiveLayerId: () => "l1",
    getLayer: () => layer,
    snapshot: () => ({ layers: [], activeLayerId: null }),
    getPaintSurface: () => null,
  };
}

// Let the async restoreHistorySnapshot's `await import(...)` + invoke chain settle.
const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

describe("useEditorCommands undo/redo cursor-sync gate — history bridge", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("default production: invoke is NEVER called with rust_pixels_undo/redo (bridge OFF)", async () => {
    // Defaults: rustPixels absent, bridge gate absent, isTauriRuntime false.
    mockUseEditor(makeEditorContext(makePatches(), makeEngine()));

    const commands = useEditorCommands(() => {});
    commands.undo();
    commands.redo();
    await flush();

    const undoRedoCalls = (vi.mocked(invoke).mock.calls as [string][]).filter(
      ([cmd]) => cmd === "rust_pixels_undo" || cmd === "rust_pixels_redo",
    );
    expect(undoRedoCalls).toHaveLength(0);
  });

  it("bridge gate ON in Tauri: invoke is called once per undo/redo cursor-sync", async () => {
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    mockUseEditor(makeEditorContext(makePatches(), makeEngine()));
    vi.mocked(invoke).mockResolvedValue({ tiles: [], epoch: 1, version: 1 });

    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();
    expect(invoke).toHaveBeenCalledWith("rust_pixels_undo", { docId: "doc-1", layerId: "l1" });

    vi.mocked(invoke).mockClear();
    commands.redo();
    await flush();
    expect(invoke).toHaveBeenCalledWith("rust_pixels_redo", { docId: "doc-1", layerId: "l1" });
  });

  it("historyBridgeEnabled predicate: default false, gate+tauri true, gate+non-tauri false", async () => {
    const { historyBridgeEnabled } = await import("@/engine/history");

    expect(historyBridgeEnabled()).toBe(false); // no gate, not tauri

    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    expect(historyBridgeEnabled()).toBe(false);

    vi.mocked(isTauriRuntime).mockReturnValue(true);
    expect(historyBridgeEnabled()).toBe(true);

    localStorage.removeItem(GATE_KEY);
    expect(historyBridgeEnabled()).toBe(false);

    // rustPixels is an independent flag — it must NOT flip the bridge predicate.
    localStorage.setItem(RUST_PIXELS_KEY, "1");
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    expect(historyBridgeEnabled()).toBe(true);
  });
});
