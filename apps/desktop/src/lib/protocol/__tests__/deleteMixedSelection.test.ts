// DeleteLayer UX guard (post-approval refinement): MIXED ownership selection
// is rejected ATOMICALLY — zero mutation, zero history entry, zero protocol
// command, with a user-facing explanation. Single-owned and all-legacy paths
// unchanged.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRoot } from "solid-js";

const { useEditorMock } = vi.hoisted(() => ({ useEditorMock: vi.fn() }));
vi.mock("@/components/editor/shell/EditorContext", () => ({ useEditor: () => useEditorMock() }));

const owned = new Set<string>();
vi.mock("@/engine/document", () => ({
  isFacadeOwnedLayer: (id: string) => owned.has(id),
  hasFacadeOwnedLayers: () => owned.size > 0,
}));

// REAL registry is used so the handler exercises the REAL
// resolveSelectionRoute (ADR 0009 helper). Only its facade dependency is
// spied per-test via getFacade(...).deleteLayer.

vi.mock("@/components/editor/layers/layerOperations", () => ({
  deleteMultipleLayers: vi.fn(() => true),
  flattenAllLayers: vi.fn(),
  mergeActiveLayerDown: vi.fn(),
  mergeSelectedLayers: vi.fn(),
  duplicateMultipleLayers: vi.fn(),
  stampVisibleLayers: vi.fn(),
}));
vi.mock("@/components/editor/Toast", () => ({ showToast: vi.fn() }));

import { useLayerActions } from "@/components/editor/layers/useLayerActions";
import { getFacade, MIXED_OWNERSHIP_MESSAGE } from "@/lib/protocol/facadeRegistry";
import * as bridge from "@/lib/protocol/bridge";
import { deleteMultipleLayers } from "@/components/editor/layers/layerOperations";
import { showToast } from "@/components/editor/Toast";
const toastMock = showToast as unknown as ReturnType<typeof vi.fn>;

function setup(opts: { facadeFlag: boolean; ids: IdSpec[] }) {
  localStorage.setItem("photrez.facade", opts.facadeFlag ? "1" : "0");
  owned.clear();
  for (const l of opts.ids) if (l.owned) owned.add(l.id);

  let alive = true;
  const engine = {
    getId: () => "docMx",
    getLayers: () => opts.ids.map((l) => ({ id: l.id })),
    getLayer: (id: string) => {
      const hit = opts.ids.find((l) => l.id === id);
      return hit && alive ? { id, locked: false, isBackground: false } : null;
    },
    getActiveLayerId: () => "other",
    applyFacadeSnapshot: vi.fn(),
    deleteLayer: vi.fn((id: string) => {
      if (id === "victim") alive = false;
    }),
    snapshot: () => ({ layers: [] }),
  };
  const history = { commit: vi.fn() };

  createRoot(() => {
    useEditorMock.mockReturnValue({
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      renderer: { destroyTexture: vi.fn() },
      scheduler: { requestRender: vi.fn() },
      selectedLayerIds: () => opts.ids.map((l) => l.id),
      selectedLayerId: () => opts.ids[0].id,
      setSelectedLayerId: vi.fn(),
      activeLayerId: () => opts.ids[0].id,
      textEditSession: () => null,
      setTextEditSession: vi.fn(),
      textSessionEditor: () => null,
      layerTransformSession: () => null,
      setLayerTransformSession: vi.fn(),
      layers: () => [],
      setStatusLoadingMessage: vi.fn(),
      toggleLayerSelection: vi.fn(),
      rangeSelectLayers: vi.fn(),
      moveAutoSelect: () => false,
      setMoveAutoSelect: vi.fn(),
    });
  });

  return { engine, history, toastMock };
}

interface IdSpec {
  id: string;
  owned: boolean;
}

function ids(spec: Array<[string, boolean]>): IdSpec[] {
  return spec.map(([id, owned]) => ({ id, owned }));
}

beforeEach(() => {
  vi.spyOn(bridge, "applyCommand").mockImplementation(() => {
    throw new Error("E_EXTERNAL_PENDING: protocol must not be touched in these routing tests");
  });
});
afterEach(() => {
  localStorage.removeItem("photrez.facade");
  __reset();
  vi.restoreAllMocks();
});

function __reset() {
  // registry mock has no state beyond stubs; nothing to clear
}

describe("DeleteLayer UX guard — mixed ownership selection rejected atomically", () => {
  it("mixed selection: ZERO protocol commands, ZERO facade deletes, ZERO legacy commits/deletes; shared message toast", async () => {
    const mod = await import("@/components/editor/layers/useLayerActions");
    const { engine, history } = setup({
      facadeFlag: true,
      ids: ids([
        ["victim", true],
        ["bg", false],
        ["other2", false],
      ]),
    });
    const facade = getFacade("docMx");
    const delSpy = vi.spyOn(facade, "deleteLayer");

    mod.useLayerActions().handleDeleteActiveLayer();

    expect(delSpy).not.toHaveBeenCalled();            // zero facade deletes
    expect(engine.applyFacadeSnapshot).not.toHaveBeenCalled(); // zero projection/mutation
    expect(engine.deleteLayer).not.toHaveBeenCalled();          // zero legacy mutation
    expect(history.commit).not.toHaveBeenCalled();              // zero history entries
    expect(deleteMultipleLayers).not.toHaveBeenCalled();        // legacy multi untouched
    expect(toastMock).toHaveBeenCalledTimes(1);
    expect(String(toastMock.mock.calls[0][0])).toBe(MIXED_OWNERSHIP_MESSAGE);
  });

  it("all-owned selection under flag ON still deletes via facade (unchanged)", async () => {
    const mod = await import("@/components/editor/layers/useLayerActions");
    const { engine } = setup({
      facadeFlag: true,
      ids: ids([
        ["a", true],
        ["b", true],
      ]),
    });
    const facade = getFacade("docMx");
    vi.spyOn(facade, "deleteLayer").mockReturnValue({ version: 5, layers: [] } as never);

    mod.useLayerActions().handleDeleteActiveLayer();

    expect(facade.deleteLayer).toHaveBeenCalledTimes(2);
    expect(engine.applyFacadeSnapshot).toHaveBeenCalled();
    expect(engine.deleteLayer).not.toHaveBeenCalled();
  });

  it("all-legacy selection unchanged (legacy multi path even under flag ON)", async () => {
    const mod = await import("@/components/editor/layers/useLayerActions");
    const { engine, history } = setup({
      facadeFlag: true,
      ids: ids([
        ["bg", false],
        ["other2", false],
      ]),
    });
    const facade = getFacade("docMx");
    const delSpy = vi.spyOn(facade, "deleteLayer");

    mod.useLayerActions().handleDeleteActiveLayer();
    expect(delSpy).not.toHaveBeenCalled();
    expect(deleteMultipleLayers).toHaveBeenCalled();
    void engine;
    void history;
  });
});
