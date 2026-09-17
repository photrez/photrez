// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Transform-session keyboard on facade-owned layers (photrez.facade=1).
//
// Production reachability: NO production code creates a non-null session.
// Every production setLayerTransformSession call passes null (editorState
// creation, toolLifecycle, useLayerActions, LayersPanel,
// SelectionTransformOverlay, LeftToolRail, DocumentTabsBar, EditorContext,
// TransformOptionBar, EditorShell, useSelectionTransformDrag,
// useEditorCommands); session object literals appear only in tests. The live
// selection drag (useSelectionTransformDrag.handlePointerDown) builds drag
// state plus the facade transient slot, never a session.
//
// So the Ctrl+Z/Y + Enter + Escape path in handleTransformSessionKey cannot
// meet an owned layer in production. This test pins the backstop for the day
// a session ever becomes ownable: with a fabricated session on an owned
// layer, the legacy engine writes refuse LOUD (E_FACADE_OWNED) instead of
// diverging silently, and the Enter path (history commit only, mirrored by
// the commit shim) mutates no engine state. The unowned-layer case proves
// the test is not vacuous: the same keys apply there without throwing.

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { handleTransformSessionKey } from "@/components/editor/canvas/keyboardShortcuts/transformSession";
import {
  getFacade,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import type { LayerTransformSession } from "@/components/editor/tools/editorState";

beforeAll(async () => {
  await getWasmExportModule();
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "wasm");
});

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  vi.restoreAllMocks();
});

async function setupOwnedDoc(id: string) {
  const engine = new DocumentEngine(id, id, 800, 600);
  const facade = getFacade(id);
  await seedFacadeFromEngine(engine as never, facade);
  const snap = await facade.addLayer("Owned", 100, 100, 0);
  engine.applyFacadeSnapshot(snap as never);
  const ownedId = facade.snapshot.layers.find((l) => l.name === "Owned")!.id;
  return { engine, ownedId };
}

function makeSession(engine: DocumentEngine, layerId: string): LayerTransformSession {
  return {
    documentId: engine.getId(),
    layerId,
    originalSnapshot: engine.snapshot(),
    originalTransform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    mode: "resize",
    lockRatio: false,
    startedAt: Date.now(),
  };
}

function makeCtx(session: LayerTransformSession | null) {
  const setSession = vi.fn();
  const editor = {
    layerTransformSession: () => session,
    setLayerTransformSession: setSession,
    undoTransformWithCurrent: () => ({
      transform: { x: 11, y: 22, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    }),
    redoTransformWithCurrent: () => ({
      transform: { x: 33, y: 44, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    }),
    clearTransformStacks: vi.fn(),
    scheduler: { requestRender: vi.fn() },
  };
  return { ctx: { editor } as never, editor, setSession };
}

const keyZ = () => new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true });
const keyEnter = () => new KeyboardEvent("keydown", { key: "Enter", bubbles: true });
const keyEsc = () => new KeyboardEvent("keydown", { key: "Escape", bubbles: true });

describe("transform-session keyboard on a facade-owned layer (photrez.facade=1)", () => {
  it("Ctrl+Z refuses loud and mutates nothing", async () => {
    const { engine, ownedId } = await setupOwnedDoc("docSessOwn");
    const history = new CommandHistory();
    const session = makeSession(engine, ownedId);
    const { ctx } = makeCtx(session);
    const before = { ...engine.getLayer(ownedId)!.transform };

    expect(() => handleTransformSessionKey(ctx, keyZ(), engine, history)).toThrow(/E_FACADE_OWNED/);
    expect(engine.getLayer(ownedId)!.transform).toEqual(before);
  });

  it("Enter commits history only and mutates no engine transform", async () => {
    const { engine, ownedId } = await setupOwnedDoc("docSessEnter");
    const commit = vi.fn();
    const session = makeSession(engine, ownedId);
    // The commit helper skips an unchanged transform (ghost-entry guard), so
    // stage the session as changed without touching the engine.
    session.originalTransform = { x: 5, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false };
    const { ctx, setSession } = makeCtx(session);
    const before = { ...engine.getLayer(ownedId)!.transform };

    const handled = handleTransformSessionKey(ctx, keyEnter(), engine, { commit } as never);

    expect(handled).toBe(true);
    expect(commit).toHaveBeenCalledTimes(1);
    expect(engine.getLayer(ownedId)!.transform).toEqual(before);
    expect(setSession).toHaveBeenCalledWith(null);
  });

  it("Escape refuses loud and mutates nothing", async () => {
    const { engine, ownedId } = await setupOwnedDoc("docSessEsc");
    const history = new CommandHistory();
    const session = makeSession(engine, ownedId);
    const { ctx } = makeCtx(session);
    const before = { ...engine.getLayer(ownedId)!.transform };

    expect(() => handleTransformSessionKey(ctx, keyEsc(), engine, history)).toThrow(/E_FACADE_OWNED/);
    expect(engine.getLayer(ownedId)!.transform).toEqual(before);
  });
});

describe("transform-session keyboard on an unowned layer (non-vacuous control)", () => {
  it("Ctrl+Z applies without throwing", async () => {
    localStorage.setItem("photrez.facade", "0");
    const engine = new DocumentEngine("docSessLegacy", "docSessLegacy", 800, 600);
    const layer = engine.addLayer("Plain", 100, 100);
    const history = new CommandHistory();
    const session = makeSession(engine, layer.id);
    const { ctx } = makeCtx(session);

    const handled = handleTransformSessionKey(ctx, keyZ(), engine, history);

    expect(handled).toBe(true);
    expect(engine.getLayer(layer.id)!.transform.x).toBe(11);
    expect(engine.getLayer(layer.id)!.transform.y).toBe(22);
  });
});

describe("photrez.facade=0 opt-out is byte-identical", () => {
  it("Ctrl+Z applies without throwing", async () => {
    localStorage.setItem("photrez.facade", "0");
    const engine = new DocumentEngine("docSessOff", "docSessOff", 800, 600);
    const layer = engine.addLayer("Plain", 100, 100);
    const history = new CommandHistory();
    const session = makeSession(engine, layer.id);
    const { ctx } = makeCtx(session);

    const handled = handleTransformSessionKey(ctx, keyZ(), engine, history);

    expect(handled).toBe(true);
    expect(engine.getLayer(layer.id)!.transform.x).toBe(11);
  });
});
