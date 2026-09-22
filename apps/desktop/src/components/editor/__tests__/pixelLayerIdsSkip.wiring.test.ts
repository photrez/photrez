/**
 * pixelLayerIds upload-narrow wiring tests.
 *
 * Contract: SnapshotEntry.pixelLayerIds makes the upload SKIP decision
 * explicit - null = unknown (identity-map fallback, today's behavior),
 * [] = provably metadata-only (skip every upload even if the restored
 * bitmap object differs), [ids] = only those layers are upload candidates.
 *
 * Exercises the REAL useEditorCommands undo path with a real
 * CommandHistory and a stateful engine mock: the renderer.uploadImage spy
 * proves whether the production upload loop ran.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { useEditorCommands } from "../useEditorCommands";
import { CommandHistory } from "@/engine/history";
import { commitLayerTransformSession } from "@/components/editor/transformSession";
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

describe("pixelLayerIds upload narrow (bridge OFF)", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue({ version: 1, epoch: 0 });
    vi.mocked(isTauriRuntime).mockReset();
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    localStorage.clear();
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
  });

  afterEach(() => {
    releaseBitmapStore("doc-1");
    vi.restoreAllMocks();
  });

  it("metadata-only ([]) undo skips uploadImage even when the restored bitmap object differs", async () => {
    const bmBefore = fakeBitmap();
    const bmLive = fakeBitmap(); // distinct object: defeats the identity check

    const history = new CommandHistory();
    const engine = makeEngine(makeModel(bmBefore));

    // Metadata-only entry: no pixel layer changed, so the allowlist is empty.
    history.commit(makeModel(bmBefore), "Transform Layer", undefined, undefined, []);
    engine.restore(makeModel(bmLive)); // live model moved on (transform applied)

    const ctx = makeEditorContext(engine, history);
    mockUseEditor(ctx);
    const commands = useEditorCommands(() => {});

    commands.undo();
    await flush();

    expect(engine.getLayer("l1")!.imageBitmap).toBe(bmBefore);
    expect(ctx.renderer.uploadImage).not.toHaveBeenCalled();
  });

  it("unknown (null) undo falls back to the identity-map FULL 2-arg upload", async () => {
    const bmBefore = fakeBitmap();
    const bmLive = fakeBitmap(); // distinct object: identity check must NOT skip

    const history = new CommandHistory();
    const engine = makeEngine(makeModel(bmBefore));

    // Legacy producer: no allowlist, so null (unknown) is stored.
    history.commit(makeModel(bmBefore), "Move Layer");
    engine.restore(makeModel(bmLive));

    const ctx = makeEditorContext(engine, history);
    mockUseEditor(ctx);
    const commands = useEditorCommands(() => {});

    commands.undo();
    await flush();

    expect(engine.getLayer("l1")!.imageBitmap).toBe(bmBefore);
    expect(history.getLastPoppedPixelLayerIds()).toBeNull();
    expect(ctx.renderer.uploadImage).toHaveBeenCalledTimes(1);
    expect(ctx.renderer.uploadImage).toHaveBeenCalledWith("l1", bmBefore);
    expect(ctx.renderer.uploadImage.mock.calls[0]).toHaveLength(2);
  });

  it("[ids] allowlist uploads the named changed layer and skips the unnamed changed one", async () => {
    const bm1Before = fakeBitmap();
    const bm2Before = fakeBitmap();
    const bm1Live = fakeBitmap(); // distinct objects: both layers count as changed
    const bm2Live = fakeBitmap();

    const beforeModel = {
      id: "doc-1",
      layers: [
        { id: "l1", imageBitmap: bm1Before },
        { id: "l2", imageBitmap: bm2Before },
      ],
      activeLayerId: "l1",
    } as unknown as DocumentModel;
    const liveModel = {
      id: "doc-1",
      layers: [
        { id: "l1", imageBitmap: bm1Live },
        { id: "l2", imageBitmap: bm2Live },
      ],
      activeLayerId: "l1",
    } as unknown as DocumentModel;

    const history = new CommandHistory();
    const engine = makeEngine(beforeModel);

    // No production producer emits a non-empty [ids] today; commit() threads the
    // allowlist straight into the entry so the sweep branch is exercised as written.
    history.commit(beforeModel, "Selective Pixel Change", undefined, undefined, ["l1"]);
    engine.restore(liveModel);

    const ctx = makeEditorContext(engine, history);
    mockUseEditor(ctx);
    const commands = useEditorCommands(() => {});

    commands.undo();
    await flush();

    expect(history.getLastPoppedPixelLayerIds()).toEqual(["l1"]);
    // Named AND changed -> uploads.
    expect(ctx.renderer.uploadImage).toHaveBeenCalledTimes(1);
    expect(ctx.renderer.uploadImage).toHaveBeenCalledWith("l1", bm1Before);
    // Changed but NOT named -> the allowlist skips it (conjunction semantics).
    expect(ctx.renderer.uploadImage.mock.calls.some((c) => c[0] === "l2")).toBe(false);
  });

  it("commitLayerTransformSession threads [] as the sole metadata-only producer", () => {
    const bm = fakeBitmap();
    const history = new CommandHistory();
    const before = makeModel(bm);
    const liveTransform = { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false };
    const engineStub = {
      getId: () => "doc-1",
      getLayer: () => ({ id: "l1", transform: liveTransform }),
    };

    const ok = commitLayerTransformSession(
      {
        documentId: "doc-1",
        layerId: "l1",
        originalSnapshot: before,
        originalTransform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
        mode: "resize",
        lockRatio: false,
        startedAt: Date.now(),
      },
      engineStub as never,
      history,
    );

    expect(ok).toBe(true);
    const restored = history.undo(makeModel(bm));
    expect(restored).toBe(before);
    expect(history.getLastPoppedPixelLayerIds()).toEqual([]);
  });
});
