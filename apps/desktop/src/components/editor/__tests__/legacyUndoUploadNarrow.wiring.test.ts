/**
 * Wiring test: legacy undo/redo re-uploads only layers whose bitmap changed.
 *
 * Production path: useEditorCommands.restoreHistorySnapshot used to call
 * renderer.uploadImage for EVERY layer on every undo/redo, even when the
 * restored snapshot shares the same bitmap object (snapshots reuse the
 * immutable ImageBitmap reference, see engine/snapshot.ts). The narrow keeps
 * an id-to-bitmap map from before engine.restore and skips layers whose
 * bitmap is identical afterwards (same precedent as facadeHistoryHandoff).
 *
 * MOCK FIDELITY: the fake engine.restore mirrors the real restoreSnapshot
 * ref-sharing (fresh layer objects, same imageBitmap refs), so an identity
 * skip here means identical pixels, exactly as in production.
 */
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { useEditorCommands } from "../useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => false),
  isFacadeOwnedLayer: vi.fn(() => false),
}));

const DOC_ID = "docUndoNarrow";

type Bmp = { tag: string };
const bmp = (tag: string): Bmp => ({ tag });

function makeLayer(id: string, imageBitmap: Bmp | null, x = 0) {
  return {
    id,
    name: id,
    type: "raster",
    visible: true,
    opacity: 1,
    locked: false,
    isBackground: false,
    blendMode: "normal",
    transform: { x, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    width: 100,
    height: 100,
    imageBitmap,
  };
}

function makeHarness(undoSnapshot: { layers: ReturnType<typeof makeLayer>[] }) {
  let live: ReturnType<typeof makeLayer>[] = [];
  const uploadImage = vi.fn();
  const engine = {
    getId: () => DOC_ID,
    getLayers: () => live,
    getLayer: (id: string) => live.find((l) => l.id === id) ?? null,
    getSelection: () => null,
    snapshot: () => ({ layers: live, activeLayerId: null }),
    // Mirrors the real restoreSnapshot: fresh layer objects, same bitmap refs.
    restore: vi.fn((snap: { layers: ReturnType<typeof makeLayer>[] }) => {
      live = snap.layers.map((l) => ({ ...l, transform: { ...l.transform } }));
    }),
    notifyVisualChange: vi.fn(),
  };
  const history = {
    canUndo: () => true,
    canRedo: () => true,
    undo: () => undoSnapshot,
    redo: () => null,
    consumeLastUndoPatches: () => null,
    consumeLastRedoPatches: () => null,
  };
  const ctx = {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => DOC_ID,
      notifyVisualChange: vi.fn(),
    },
    renderer: { uploadImage, uploadSurfaceTiles: vi.fn() },
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
  return {
    ctx,
    engine,
    uploadImage,
    setLive: (layers: ReturnType<typeof makeLayer>[]) => {
      live = layers;
    },
  };
}

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

describe("legacy undo upload narrow (single-layer undo uploads one layer)", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    localStorage.clear();
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("single-layer pixel undo uploads only the changed layer", async () => {
    const beforeB = bmp("b");
    const beforeC = bmp("c");
    const live = [
      makeLayer("l1", bmp("a-new")),
      makeLayer("l2", beforeB),
      makeLayer("l3", beforeC),
    ];
    // Undo restores l1 to its old bitmap; l2/l3 keep the same object.
    const undone = [
      makeLayer("l1", bmp("a-old")),
      makeLayer("l2", beforeB),
      makeLayer("l3", beforeC),
    ];
    const h = makeHarness({ layers: undone });
    h.setLive(live);
    mockUseEditor(h.ctx);
    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();
    const ids = (h.uploadImage.mock.calls as [string, unknown][]).map((c) => c[0]);
    expect(ids).toEqual(["l1"]);
    // Final state keeps the restored pixels for every layer.
    expect(h.engine.getLayers().map((l) => l.id)).toEqual(["l1", "l2", "l3"]);
    expect(h.engine.getLayer("l2")!.imageBitmap).toBe(beforeB);
    expect(h.engine.getLayer("l3")!.imageBitmap).toBe(beforeC);
  });

  it("transform-only undo uploads nothing (no pixel changed)", async () => {
    // A transform never replaces the bitmap, so both sides share one object.
    const sharedA = bmp("a");
    const beforeB = bmp("b");
    const beforeC = bmp("c");
    const live = [
      makeLayer("l1", sharedA, 50),
      makeLayer("l2", beforeB),
      makeLayer("l3", beforeC),
    ];
    // Undo moves l1 back; every bitmap object is shared with the live model.
    const undone = [
      makeLayer("l1", sharedA, 0),
      makeLayer("l2", beforeB),
      makeLayer("l3", beforeC),
    ];
    const h = makeHarness({ layers: undone });
    h.setLive(live);
    mockUseEditor(h.ctx);
    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();
    expect(h.uploadImage).not.toHaveBeenCalled();
    expect(h.engine.getLayer("l1")!.transform.x).toBe(0);
  });

  it("undo of a layer delete re-uploads the restored layer", async () => {
    const sharedA = bmp("a");
    const beforeB = bmp("b");
    // Deleting l2 never touched l1, so the live and snapshot l1 share one object.
    const live = [makeLayer("l1", sharedA)];
    const undone = [makeLayer("l1", sharedA), makeLayer("l2", beforeB)];
    const h = makeHarness({ layers: undone });
    h.setLive(live);
    mockUseEditor(h.ctx);
    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();
    const ids = (h.uploadImage.mock.calls as [string, unknown][]).map((c) => c[0]);
    // l1 is unchanged (shared ref) so it is skipped; the re-added l2 uploads.
    expect(ids).toEqual(["l2"]);
  });

  it("changed layers re-upload FULL (no dirty rect): the entry carries ids, not regions", async () => {
    // Fallback contract: history snapshots share whole bitmap objects and the
    // entry records no changed region (region-carrying paint entries take the
    // tile path and never reach this sweep), so FULL is the only covering
    // upload. A full-bounds PATCH would add a scratch-canvas copy for the same
    // bytes, so it is not used either. Pinned 2-arg so a future rect must be
    // a provable sub-region, never a guess.
    const beforeB = bmp("b");
    const live = [makeLayer("l1", bmp("a-new")), makeLayer("l2", beforeB)];
    const undone = [makeLayer("l1", bmp("a-old")), makeLayer("l2", beforeB)];
    const h = makeHarness({ layers: undone });
    h.setLive(live);
    mockUseEditor(h.ctx);
    const commands = useEditorCommands(() => {});
    commands.undo();
    await flush();
    expect(h.uploadImage).toHaveBeenCalledTimes(1);
    expect(h.uploadImage).toHaveBeenCalledWith("l1", expect.anything());
    for (const call of h.uploadImage.mock.calls) expect(call.length).toBe(2);
  });
});
