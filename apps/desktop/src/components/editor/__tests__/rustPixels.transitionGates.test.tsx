/**
 * Routing matrix for the TILE/PIXEL undo/redo cursor-sync step.
 *
 * Two production sites can fire rust_pixels_undo / rust_pixels_redo for ONE
 * step (both inside the tile-patch branch of useEditorCommands):
 *   - the photrez.rustPixels tile path: reads the flag with no runtime gate,
 *   - the photrez.historyBridge cursor-sync site: needs historyBridgeEnabled(),
 *     i.e. the gate key AND the Tauri runtime, and it is skipped while the flag
 *     is on so a step can never take two Rust cursor steps.
 * Exactly one site may fire per direction, so all eight
 * flag x gate x runtime combinations are pinned here:
 *
 *   flag 1 (any gate, any runtime) -> 1: the tile path fires and the bridge
 *     site stays skipped.
 *   flag 0 + gate 1 + Tauri -> 1: only the bridge site owns the cursor step.
 *   flag 0 + gate 1 + jsdom -> 0: the gate predicate also needs the runtime.
 *   flag 0 + gate 0 (any runtime) -> 0: no site is armed.
 *
 * A metadata-only step carries no tile patches, so both sites are unreachable
 * for it; those rows assert zero invokes AND that the model restore still ran
 * (otherwise a zero would only prove the step aborted early).
 *
 * Census reads go through flushPixelInvokeCensus(), which awaits in-flight
 * invokes first; a sleep is not a drain.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { useEditorCommands } from "../useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";

// The runtime detector feeds historyBridgeEnabled() and the undo/redo gate.
vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(),
}));

// The undo/redo tile path dynamic-imports this module, so the mock intercepts
// the dynamic import too and `invoke` is the shared spy.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

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

/** Minimal TS CommandHistory shape; `patches` null = metadata-only step. */
function makeHistory(patches: WirePatch | null) {
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

/** Editor context + engine the hook body and the metadata path consume. */
function makeEditorContext(
  history: ReturnType<typeof makeHistory>,
  engine: Record<string, unknown>,
) {
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
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

function makeEngine() {
  const layer = { id: "l1", basicAdjustment: undefined, imageBitmap: undefined };
  return {
    getActiveLayerId: () => "l1",
    getLayer: () => layer,
    getLayers: () => [] as { id: string }[],
    restore: vi.fn(),
    snapshot: () => ({ layers: [], activeLayerId: null }),
    getPaintSurface: () => null,
  };
}

// Two turns: the cursor-sync site does its own dynamic import after the first.
const flush = async () => {
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
};

function countInvokes(...commands: string[]): number {
  return (vi.mocked(invoke).mock.calls as [string][]).filter(([cmd]) =>
    commands.includes(cmd),
  ).length;
}

type Row = {
  rustPixels: "0" | "1";
  bridge: "0" | "1";
  tauri: boolean;
  expected: number;
};

const ROWS: Row[] = [
  { rustPixels: "1", bridge: "0", tauri: true, expected: 1 },
  { rustPixels: "1", bridge: "1", tauri: true, expected: 1 },
  { rustPixels: "1", bridge: "0", tauri: false, expected: 1 },
  { rustPixels: "1", bridge: "1", tauri: false, expected: 1 },
  { rustPixels: "0", bridge: "1", tauri: true, expected: 1 },
  { rustPixels: "0", bridge: "1", tauri: false, expected: 0 },
  { rustPixels: "0", bridge: "0", tauri: true, expected: 0 },
  { rustPixels: "0", bridge: "0", tauri: false, expected: 0 },
];

function applyRowFlags(flags: { rustPixels: "0" | "1"; bridge: "0" | "1" }): void {
  localStorage.setItem(RUST_PIXELS_KEY, flags.rustPixels);
  localStorage.setItem(GATE_KEY, flags.bridge);
}

describe("TILE/PIXEL undo/redo routing matrix (flag x bridge gate x runtime)", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  for (const row of ROWS) {
    const label =
      `rustPixels=${row.rustPixels} historyBridge=${row.bridge} ` +
      `runtime=${row.tauri ? "tauri" : "jsdom"}`;

    it(`${label}: exactly ${row.expected} cursor-sync invoke per direction`, async () => {
      applyRowFlags(row);
      vi.mocked(isTauriRuntime).mockReturnValue(row.tauri);
      const engine = makeEngine();
      mockUseEditor(makeEditorContext(makeHistory(makePatches()), engine));
      vi.mocked(invoke).mockResolvedValue({ tiles: [], epoch: 1, version: 1 });
      const censusBefore = (await flushPixelInvokeCensus()).entries.length;

      const commands = useEditorCommands(() => {});
      commands.undo();
      await flush();
      expect(countInvokes("rust_pixels_undo"), "undo fires only its own command").toBe(
        row.expected,
      );
      expect(countInvokes("rust_pixels_redo"), "undo must not fire redo").toBe(0);

      vi.mocked(invoke).mockClear();
      commands.redo();
      await flush();
      expect(countInvokes("rust_pixels_redo"), "redo fires only its own command").toBe(
        row.expected,
      );
      expect(countInvokes("rust_pixels_undo"), "redo must not fire undo").toBe(0);

      const census = await flushPixelInvokeCensus();
      const delta = census.entries
        .slice(censusBefore)
        .filter(
          (entry) =>
            entry.command === "rust_pixels_undo" || entry.command === "rust_pixels_redo",
        );
      expect(delta.map((entry) => entry.command)).toEqual(
        row.expected === 1 ? ["rust_pixels_undo", "rust_pixels_redo"] : [],
      );
      expect(delta.every((entry) => entry.phase === "resolved")).toBe(true);
    });
  }
});

describe("metadata-only steps fire no cursor-sync invoke from either site", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  const combos = [
    { rustPixels: "1", bridge: "1", tauri: true },
    { rustPixels: "0", bridge: "1", tauri: true },
    { rustPixels: "1", bridge: "0", tauri: false },
  ] as const;

  for (const combo of combos) {
    const label =
      `rustPixels=${combo.rustPixels} historyBridge=${combo.bridge} ` +
      `runtime=${combo.tauri ? "tauri" : "jsdom"}`;

    it(`${label}: zero invokes while the model restore still runs`, async () => {
      applyRowFlags(combo);
      vi.mocked(isTauriRuntime).mockReturnValue(combo.tauri);
      const engine = makeEngine();
      mockUseEditor(makeEditorContext(makeHistory(null), engine));
      vi.mocked(invoke).mockResolvedValue({ tiles: [], epoch: 1, version: 1 });
      const censusBefore = (await flushPixelInvokeCensus()).entries.length;

      const commands = useEditorCommands(() => {});
      commands.undo();
      await flush();
      commands.redo();
      await flush();

      // Non-vacuity: the step completed its model restore, so the zero below
      // means "no site fired", not "the step aborted before either site".
      expect(engine.restore).toHaveBeenCalledTimes(2);
      expect(countInvokes("rust_pixels_undo", "rust_pixels_redo")).toBe(0);

      const census = await flushPixelInvokeCensus();
      const delta = census.entries
        .slice(censusBefore)
        .filter(
          (entry) =>
            entry.command === "rust_pixels_undo" || entry.command === "rust_pixels_redo",
        );
      expect(delta).toEqual([]);
    });
  }
});
