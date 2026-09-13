// Call-site wiring for the Select-menu selection commands
// (edit.select-all / edit.deselect / edit.invert-selection) in
// useEditorCommands. Mirrors the metadata wiring pattern: the commit helpers are
// mocked spies, the facade flag is controlled through localStorage, and each
// test asserts BOTH that the mirror helper fires (flag ON) and that the host
// engine mutation still runs, plus that flag OFF leaves the legacy path
// untouched.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useEditorCommands } from "../useEditorCommands";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";

const h = vi.hoisted(() => ({
  commitFacadeSetSelection: vi.fn(() => Promise.resolve({ status: "applied" })),
  commitFacadeClearSelection: vi.fn(() => Promise.resolve({ status: "applied" })),
  commitFacadeSelectAll: vi.fn(() => Promise.resolve({ status: "applied" })),
  commitFacadeInvertSelection: vi.fn(() => Promise.resolve({ status: "applied" })),
}));

vi.mock("@/lib/protocol/facadeRegistry", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  commitFacadeSetSelection: h.commitFacadeSetSelection,
  commitFacadeClearSelection: h.commitFacadeClearSelection,
  commitFacadeSelectAll: h.commitFacadeSelectAll,
  commitFacadeInvertSelection: h.commitFacadeInvertSelection,
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));
vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: () => false, runTauriWindowAction: vi.fn() }));
vi.mock("@/engine/document", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  hasFacadeOwnedLayers: () => false,
  isFacadeOwnedLayer: () => false,
}));

const tick = () => new Promise((r) => setTimeout(r, 0));

function makeEngine(selection: unknown) {
  return {
    getSelection: () => selection,
    getId: () => "doc-1",
    getActiveLayerId: () => null,
    getLayer: () => null,
    selectAll: vi.fn(),
    clearSelection: vi.fn(),
    invertSelection: vi.fn(),
  };
}

function makeEditorContext(engine: Record<string, unknown>) {
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => ({ commit: () => {}, canUndo: () => false, canRedo: () => false }),
      getActiveDocumentId: () => "doc-1",
      getActiveSession: () => null,
      notifyVisualChange: () => {},
      isFull: () => false,
    },
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
    scheduler: { requestRender: vi.fn() },
    activeDocumentId: () => "doc-1",
    layerTransformSession: () => null,
    setLayerTransformSession: vi.fn(),
    activeTool: () => "selection",
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
    selectionEditMode: () => false,
    setSelectionEditMode: vi.fn(),
    setStatusLoadingMessage: vi.fn(),
    setShowExportDialog: vi.fn(),
    setShowPrintDialog: vi.fn(),
    setShowResizeDialog: vi.fn(),
  };
}

beforeEach(() => {
  vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
});

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  vi.restoreAllMocks();
  h.commitFacadeClearSelection.mockClear();
  h.commitFacadeSelectAll.mockClear();
  h.commitFacadeInvertSelection.mockClear();
});

describe("useEditorCommands selection commands (mirror dispatch)", () => {
  it("edit.select-all: flag ON mirrors selectAll AND still runs the host engine op", async () => {
    localStorage.setItem("photrez.facade", "1");
    const engine = makeEngine(null);
    mockUseEditor(makeEditorContext(engine));
    const commands = useEditorCommands(() => {});

    commands.execute("edit.select-all");
    await tick();

    expect(engine.selectAll).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeSelectAll).toHaveBeenCalledTimes(1);
  });

  it("edit.select-all: flag OFF runs the host engine op only", () => {
    const engine = makeEngine(null);
    mockUseEditor(makeEditorContext(engine));
    const commands = useEditorCommands(() => {});

    commands.execute("edit.select-all");

    expect(engine.selectAll).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeSelectAll).not.toHaveBeenCalled();
  });

  it("edit.deselect: flag ON mirrors clearSelection AND still runs the host engine op", async () => {
    localStorage.setItem("photrez.facade", "1");
    const engine = makeEngine({ x: 1, y: 2, width: 3, height: 4, angle: 0 });
    mockUseEditor(makeEditorContext(engine));
    const commands = useEditorCommands(() => {});

    commands.execute("edit.deselect");
    await tick();

    expect(engine.clearSelection).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeClearSelection).toHaveBeenCalledTimes(1);
  });

  it("edit.invert-selection: flag ON mirrors invertSelection AND still runs the host engine op", async () => {
    localStorage.setItem("photrez.facade", "1");
    const engine = makeEngine({ x: 1, y: 2, width: 3, height: 4, angle: 0 });
    mockUseEditor(makeEditorContext(engine));
    const commands = useEditorCommands(() => {});

    commands.execute("edit.invert-selection");
    await tick();

    expect(engine.invertSelection).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeInvertSelection).toHaveBeenCalledTimes(1);
  });
});
