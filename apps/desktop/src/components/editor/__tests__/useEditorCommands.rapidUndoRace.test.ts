/**
 * Regression wiring test for the slow-async-bmp re-attach race in
 * useEditorCommands.restoreHistorySnapshot (snapshot path, bridge ON).
 *
 * Coverage gap: historySnapshotBridge.wiring.test.ts only scopes history.ts
 * (restoreSnapshotBitmapsByToken) — it cannot catch a stale async re-attach in
 * useEditorCommands returning over the live model. The monotonic-op-counter
 * guard lives in the `set` callback of the snapshot-token re-attach; this file
 * drives the REAL undo path (via the hook's `undo`) and asserts the stale
 * (older) re-attach is DROPPED.
 *
 * Why the bridge seam is stubbed: the re-attach helper dynamic-imports
 * @tauri-apps/api/core and the vitest `vi.mock` only reliably yields the
 * controlled `invoke` spy for a SINGLE call — a second rapid undo's dynamic
 * import resolves to a different `invoke`, so it is impossible to script two
 * in-flight snapshot resolves through the real helper. Stubbing
 * `restoreSnapshotBitmapsByToken` captures the real guard `set` closures (the
 * exact code under test) and lets us control the resolution ORDER, which is the
 * whole point: undo2 (newer) resolves first, then undo1 (older/stale) resolves
 * late and must be dropped.
 *
 * Scenario (rapid double-undo under bridge ON):
 *   undo1 restores the model, then awaits its snapshot (never resolved yet).
 *   undo2 starts, restores the model (newer, synchronous), then awaits its own.
 *   undo2's re-attach resolves FIRST (applies the newer bitmap).
 *   undo1's stale re-attach resolves AFTER — without the guard it would
 *   overwrite the newer model's bitmap with the OLD before-1 bitmap. The guard
 *   drops it, so the live layer keeps the SECOND undo's bitmap.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { useEditorCommands } from "../useEditorCommands";
import { restoreSnapshotBitmapsByToken } from "@/engine/history";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";

vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(),
}));

// Stub ONLY the async bridge hop. Everything else in @/engine/history stays
// real (historyBridgeEnabled, CommandHistory, ...). `restoreSpy` is a single
// stable spy so the hook's named import resolves to a controllable function.
const { restoreSpy } = vi.hoisted(() => ({ restoreSpy: vi.fn() }));
vi.mock("@/engine/history", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/engine/history")>();
  return { ...actual, restoreSnapshotBitmapsByToken: restoreSpy };
});

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

function fakeBitmap(): ImageBitmap {
  return { width: 10, height: 10, close: vi.fn() } as unknown as ImageBitmap;
}

/** Minimal TS CommandHistory hitting the snapshot path (no tile patches). */
function makeHistory() {
  const snapshot = { layers: [], activeLayerId: null };
  return {
    canUndo: () => true,
    canRedo: () => true,
    undo: () => snapshot,
    redo: () => snapshot,
    consumeLastUndoPatches: () => null,
    consumeLastRedoPatches: () => null,
    // The re-attach gate runs only for Snapshot-typed entries; this mock drives
    // that path (bridge ON), so report a Snapshot entry.
    isLastPoppedSnapshotEntry: () => true,
  };
}

function makeEngine() {
  const layer = { id: "l1", basicAdjustment: undefined, imageBitmap: null } as unknown as {
    id: string;
    basicAdjustment: unknown;
    imageBitmap: ImageBitmap | null;
  };
  return {
    getId: () => "doc-1",
    getActiveLayerId: () => "l1",
    getLayer: (_id: string) => layer,
    getLayers: () => [layer],
    snapshot: () => ({ layers: [], activeLayerId: null }),
    restore: vi.fn(),
    getPaintSurface: () => null,
  };
}

function makeEditorContext(engine: Record<string, unknown>) {
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => makeHistory(),
      getActiveDocumentId: () => "doc-1",
      notifyVisualChange: vi.fn(),
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

describe("useEditorCommands rapid double-undo no stale re-attach (bridge ON)", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
    restoreSpy.mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("stale async re-attach from undo1 is DROPPED when undo2 restored first; live bitmap is undo2's", async () => {
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(true);

    const bitmapA = fakeBitmap(); // before-1 (older)
    const bitmapB = fakeBitmap(); // before-2 (newer)

    // Capture the REAL guard `set` closures the hook builds, one per undo. We
    // do not auto-apply them — the test resolves them in a controlled order.
    const setCallbacks: Array<(layerId: string, bitmap: ImageBitmap) => boolean> = [];
    restoreSpy.mockImplementation(
      (_direction: "undo" | "redo", _docId: string, _resolve: (t: string) => ImageBitmap | null, set: (l: string, b: ImageBitmap) => boolean) => {
        setCallbacks.push(set);
        return Promise.resolve(false);
      },
    );

    const engine = makeEngine();
    mockUseEditor(makeEditorContext(engine as unknown as Record<string, unknown>));

    const commands = useEditorCommands(() => {});

    // Rapid double-undo: both dispatched synchronously, no await between them.
    commands.undo();
    commands.undo();
    // Both restores ran (newer state applied synchronously), and we hold the 2
    // guard closures captured at dispatch time (opStart 1 and 2).
    expect((engine.restore as unknown as ReturnType<typeof vi.fn>).mock.calls.length).toBe(2);
    expect(setCallbacks.length).toBe(2);

    // undo2 (newer) re-attach resolves FIRST → applies its bitmap.
    expect(setCallbacks[1]("l1", bitmapB)).toBe(true);
    expect(engine.getLayer("l1").imageBitmap).toBe(bitmapB);

    // undo1 (older) re-attach resolves LATE — after undo2 already restored. The
    // stale re-attach must NOT overwrite the newer model's bitmap with bitmapA.
    expect(setCallbacks[0]("l1", bitmapA)).toBe(false);
    expect(engine.getLayer("l1").imageBitmap).toBe(bitmapB);
    expect(engine.getLayer("l1").imageBitmap).not.toBe(bitmapA);

    // Idempotent no-detach invariants still hold: nothing was closed.
    expect(bitmapA.close).not.toHaveBeenCalled();
    expect(bitmapB.close).not.toHaveBeenCalled();
  });
});
