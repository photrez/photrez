/**
 * Snapshot-bridge end-to-end wiring tests for the useEditorCommands undo/redo
 * snapshot path. Exercises the REAL restoreHistorySnapshot gate + guarded set
 * closure with a real CommandHistory (External + Snapshot entries) and a
 * stateful engine mock.
 *
 * Why the bridge hop is stubbed (same rationale as
 * useEditorCommands.rapidUndoRace.test.ts): `restoreSnapshotBitmapsByToken`
 * dynamic-imports @tauri-apps/api/core and the vitest mock only reliably yields
 * the controlled `invoke` spy for a single call; the real async hop is not
 * needed to prove the TS-side gate/set-closure logic. The field-hint (#3) real
 * helper path is covered directly in historySnapshotBridge.wiring.test.ts.
 *
 * Coverage:
 *  - Hazard #1 (metadata-tip cursor gap): mixed [External, Snapshot] → undo×2 →
 *    redo×1 must NOT consult the Rust snapshot cursor on an External redo. The
 *    re-attach is gated on `isLastPoppedSnapshotEntry()` so the External step
 *    is Model-A authoritative.
 *  - Hazard #2 (op-counter vs non-undo producers): the set closure DROPS itself
 *    when a Fill/bake replaced the layer bitmap during the (simulated) invoke
 *    round-trip.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { useEditorCommands } from "../useEditorCommands";
import { CommandHistory } from "@/engine/history";
import { invoke } from "@tauri-apps/api/core";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import { releaseBitmapStore } from "@/engine/bitmapStore";
import type { DocumentModel } from "@/engine/types";

vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
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

function makeModel(bitmap: ImageBitmap | null): DocumentModel {
  return {
    id: "doc-1",
    layers: [{ id: "l1", imageBitmap: bitmap }],
    activeLayerId: "l1",
  } as unknown as DocumentModel;
}

/** Stateful engine that follows the real DocumentEngine snapshot/restore shape. */
function makeEngine(initial: DocumentModel) {
  let model: DocumentModel = { ...initial, layers: [...initial.layers] };
  return {
    getId: () => model.id,
    getActiveLayerId: () => model.activeLayerId,
    getLayer: (id: string) => model.layers.find((l) => l.id === id) ?? null,
    getLayers: () => model.layers,
    snapshot: (): DocumentModel => ({
      ...model,
      layers: model.layers.map((l) => ({ ...l })),
    }),
    restore: (snap: DocumentModel) => {
      model = { ...snap, layers: [...snap.layers] };
    },
    getPaintSurface: () => null,
    getModel: () => model,
  };
}

function makeEditorContext(engine: ReturnType<typeof makeEngine>, history: CommandHistory) {
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
    activeLayerId: () => engine.getActiveLayerId(),
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

describe("useEditorCommands snapshot-bridge undo/redo (bridge ON)", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
    restoreSpy.mockReset();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue({ version: 1, epoch: 0 });
    vi.mocked(isTauriRuntime).mockReset();
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    localStorage.clear();
  });

  afterEach(() => {
    releaseBitmapStore("doc-1");
    vi.restoreAllMocks();
  });

  // ── Hazard #1: mixed [External, Snapshot] undo×2 then redo×1 ──
  it("hazard#1: External undo/redo NEVER consult the Rust snapshot cursor (no wrong-step re-attach)", async () => {
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    // The re-attach hop is stubbed but still observable: we assert WHEN it is
    // reached. In the counter's scenario the wrong-step comes from calling
    // redo_snapshot on an External redo (cursor points at the Snapshot ahead).
    restoreSpy.mockImplementation(() => Promise.resolve(false));

    const bm0 = fakeBitmap(); // state0 (base)
    const bm1 = fakeBitmap(); // state1 (E1.after / S1.before)
    const bm2 = fakeBitmap(); // state2 (S1.after) — the WRONG-step bitmap

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    const engine = makeEngine(makeModel(bm0)); // live state0

    // E1 = plain External metadata commit (undo-point = state0).
    history.commit(makeModel(bm0), "E1");
    engine.restore(makeModel(bm1)); // apply E1 -> live state1
    // S1 = Snapshot entry (undo-point = state1, after = state2).
    history.recordSnapshotHistory(makeModel(bm1), makeModel(bm2), "S1");
    engine.restore(makeModel(bm2)); // apply S1 -> live state2

    mockUseEditor(makeEditorContext(engine, history));
    const commands = useEditorCommands(() => {});

    // undo ×2
    commands.undo(); // pops S1 (Snapshot) -> Model restores state1 (bm1)
    await flush();
    expect(engine.getLayer("l1")!.imageBitmap).toBe(bm1);
    // Exactly one re-attach dispatch, and only for the Snapshot step.
    expect(restoreSpy).toHaveBeenCalledTimes(1);
    expect(restoreSpy.mock.calls[0][0]).toBe("undo");

    commands.undo(); // pops E1 (External) -> Model restores state0 (bm0), NO re-attach
    await flush();
    expect(engine.getLayer("l1")!.imageBitmap).toBe(bm0);
    expect(restoreSpy).toHaveBeenCalledTimes(1); // still only the S1 undo

    // redo ×1 — redo of the EXTERNAL E1 entry. Model-A restore is authoritative
    // (state1, bm1). Without the isSnapshotEntry gate the redo would call
    // redo_snapshot and re-attach bm2 — the WRONG-step bitmap. The gate keeps
    // the Rust snapshot cursor untouched, so restoreSpy is NOT invoked again.
    commands.redo();
    await flush();
    expect(engine.getLayer("l1")!.imageBitmap).toBe(bm1);
    expect(engine.getLayer("l1")!.imageBitmap).not.toBe(bm2);
    expect(restoreSpy).toHaveBeenCalledTimes(1); // External redo never re-attaches
    expect(restoreSpy.mock.calls[0][0]).toBe("undo"); // only the S1 undo ever ran
  });

  // ── Hazard #2: a non-undo producer replacing a bitmap mid-round-trip is NOT overwritten ──
  it("hazard#2: a Fill/bake replacing the bitmap is NOT overwritten by the in-flight re-attach (drop-check)", async () => {
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(true);

    const bm0 = fakeBitmap(); // before (undo bitmap)
    const bm1 = fakeBitmap(); // after (S1.after)
    const producer = fakeBitmap(); // the fresh bitmap a Fill/bake produced

    let capturedSet:
      | ((layerId: string, bitmap: ImageBitmap, field?: "imageBitmap" | "baseImageBitmap") => boolean)
      | null = null;
    restoreSpy.mockImplementation(
      (_direction: "undo" | "redo", _docId: string, _resolve: (t: string) => ImageBitmap | null, set: (l: string, b: ImageBitmap, f?: "imageBitmap" | "baseImageBitmap") => boolean) => {
        capturedSet = set;
        return Promise.resolve(false);
      },
    );

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    const engine = makeEngine(makeModel(bm0)); // live state0

    history.recordSnapshotHistory(makeModel(bm0), makeModel(bm1), "S1");
    engine.restore(makeModel(bm1)); // apply S1 -> live state1

    mockUseEditor(makeEditorContext(engine, history));
    const commands = useEditorCommands(() => {});

    // Undo S1 -> Model restores state0 (bm0) synchronously; the re-attach set
    // closure is captured (stub) for the layer with expected bitmap = bm0.
    commands.undo();
    await flush();
    expect(engine.getLayer("l1")!.imageBitmap).toBe(bm0);
    expect(capturedSet).toBeTruthy();

    // Positive control: no interleaving producer -> the re-attach applies.
    expect(capturedSet!("l1", bm0, "imageBitmap")).toBe(true);

    // Simulate a non-undo bitmap producer committing inside the round-trip.
    engine.getLayer("l1")!.imageBitmap = producer;
    expect(engine.getLayer("l1")!.imageBitmap).toBe(producer);

    // The now-stale re-attach would set layer.imageBitmap = bm0 (the pre-undo
    // bitmap), clobbering the producer's fresh bitmap. The drop-check sees the
    // current bitmap != expected(bm0) and returns false.
    expect(capturedSet!("l1", bm0, "imageBitmap")).toBe(false);
    expect(engine.getLayer("l1")!.imageBitmap).toBe(producer);
    expect(engine.getLayer("l1")!.imageBitmap).not.toBe(bm0);
  });

  // ── FINDING 1: a Snapshot entry survives the undo→redo stack hop ──
  it("finding1: a Snapshot redo survives a stack hop so the re-attach runs as a REDO", async () => {
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    // Stub hop is still observable: we assert WHEN the re-attach is reached.
    restoreSpy.mockImplementation(() => Promise.resolve(false));

    const bm0 = fakeBitmap(); // state0 (S1.before)
    const bm1 = fakeBitmap(); // state1 (S1.after)

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    const engine = makeEngine(makeModel(bm0)); // live state0

    history.recordSnapshotHistory(makeModel(bm0), makeModel(bm1), "S1");
    engine.restore(makeModel(bm1)); // apply S1 -> live state1

    mockUseEditor(makeEditorContext(engine, history));
    const commands = useEditorCommands(() => {});

    commands.undo(); // pop S1 (Snapshot) -> Model restores state0 (bm0)
    await flush();
    expect(engine.getLayer("l1")!.imageBitmap).toBe(bm0);
    // The undo re-attach is reached exactly once.
    expect(restoreSpy).toHaveBeenCalledTimes(1);
    expect(restoreSpy.mock.calls[0][0]).toBe("undo");

    // Redo S1. The redo twin kept snapshotType (Finding-1 fix), so the re-attach
    // gate is reached and runs as a REDO (not dead). BEFORE the fix the redo twin
    // was untyped -> isLastPoppedSnapshotEntry()===false -> gate skipped, and
    // this assertion would fail (restoreSpy stays at 1 call).
    commands.redo();
    await flush();
    expect(engine.getLayer("l1")!.imageBitmap).toBe(bm1);
    expect(restoreSpy).toHaveBeenCalledTimes(2);
    expect(restoreSpy.mock.calls[1][0]).toBe("redo");
  });

  // ── FINDING 3: FAIL-CLOSED drop-check (hazard #2 hardening) ──
  it("finding3: FAIL-CLOSED — a layer absent from expectedBitmaps drops the re-attach", async () => {
    localStorage.setItem(GATE_KEY, "1");
    vi.mocked(isTauriRuntime).mockReturnValue(true);

    const bm0 = fakeBitmap(); // before (undo bitmap)
    const bm1 = fakeBitmap(); // after (S1.after)

    let capturedSet:
      | ((layerId: string, bitmap: ImageBitmap, field?: "imageBitmap" | "baseImageBitmap") => boolean)
      | null = null;
    restoreSpy.mockImplementation(
      (_direction: "undo" | "redo", _docId: string, _resolve: (t: string) => ImageBitmap | null, set: (l: string, b: ImageBitmap, f?: "imageBitmap" | "baseImageBitmap") => boolean) => {
        capturedSet = set;
        return Promise.resolve(false);
      },
    );

    const history = new CommandHistory();
    history.attachDocIdGetter(() => "doc-1");
    const engine = makeEngine(makeModel(bm0)); // live state0

    history.recordSnapshotHistory(makeModel(bm0), makeModel(bm1), "S1");
    engine.restore(makeModel(bm1)); // apply S1 -> live state1

    mockUseEditor(makeEditorContext(engine, history));
    const commands = useEditorCommands(() => {});

    commands.undo();
    await flush();
    expect(engine.getLayer("l1")!.imageBitmap).toBe(bm0);
    expect(capturedSet).toBeTruthy();

    // A NEW layer appears (created by a non-undo producer mid round-trip). It is
    // NOT in expectedBitmaps, so the fail-closed check must DROP the re-attach
    // rather than apply it with zero verification (a wrong-bitmap overwrite).
    const made = fakeBitmap();
    engine.getLayers().push({ id: "lNEW", imageBitmap: made } as unknown as DocumentModel["layers"][number]);
    expect(capturedSet!("lNEW", made, "imageBitmap")).toBe(false);
    // The pre-existing layer (still verified) is NOT affected by the new layer.
    expect(capturedSet!("l1", bm0, "imageBitmap")).toBe(true);
  });
});
