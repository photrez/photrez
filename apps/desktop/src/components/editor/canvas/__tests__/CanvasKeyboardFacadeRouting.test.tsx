// Wiring tests for the canvas KEYBOARD shortcut facade funnel routing. These fire
// REAL KeyboardEvents at window (the exact listener the
// production app registers) and assert the three migrated ops — Add Layer
// (Ctrl+Shift+N), Delete/Backspace, and Opacity (0-9) — route through the SAME
// funnels the panel/menu paths use, instead of calling the raw engine mutation
// directly.
//
// Regression these lock in: before this fix, the keyboard path bypassed the
// funnels and called engine.addLayer / engine.deleteLayer / engine.setLayerOpacity
// directly. With photrez.facade=1 and a facade-owned layer that (a) threw
// E_FACADE_OWNED AFTER a TS history.commit already ran (phantom history entry +
// uncaught exception — a Model-A violation) and (b) for add created a layer
// absent from the facade snapshot (erased by the next projection).
//
// NOTE on flag-OFF: the funnel's legacy branch mirrors the PANEL delete (same
// recordSnapshotHistory path), but routing the keyboard through the funnel is NOT
// byte-identical to the previous keyboard-only delete. Four deltas apply on the
// flag-OFF shipped path: history uses recordSnapshotHistory instead of
// history.commit (a), one extra engine.snapshot() per delete (b), Background in a
// 1-layer doc now shows a toast where the old keyboard path was silent (c), and
// the active-layer source is the activeLayerId() signal (d). The flag-OFF
// assertions below still verify the keyboard reaches the raw engine methods and
// commits history (via recordSnapshotHistory, not history.commit).

import { afterAll, afterEach, beforeEach, beforeAll, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider } from "../../shell/EditorContext";
import { useEditor } from "../../shell/EditorContext";
import { useCanvasKeyboard } from "../useCanvasKeyboard";
import { useLayerActions } from "../../layers/useLayerActions";
import { WorkspaceManager } from "@/engine/workspace";
import { getFacade, seedFacadeFromEngine, __resetFacadeRegistryForTests, MIXED_OWNERSHIP_MESSAGE } from "@/lib/protocol/facadeRegistry";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { showToast } from "@/components/editor/Toast";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@/components/editor/Toast", () => ({ showToast: vi.fn() }));

const toastMock = showToast as unknown as ReturnType<typeof vi.fn>;

// Real Rust facade engine (the same path the production app arms at boot).
let wasmModule: unknown | null = null;
beforeAll(async () => {
  wasmModule = await getWasmExportModule();
});

const OriginalOffscreenCanvas = (globalThis as any).OffscreenCanvas;
beforeAll(() => {
  if (typeof OffscreenCanvas === "undefined") {
    (globalThis as any).OffscreenCanvas = class {
      width: number;
      height: number;
      constructor(w: number, h: number) {
        this.width = w;
        this.height = h;
      }
      getContext(): any {
        return {
          save: vi.fn(),
          restore: vi.fn(),
          translate: vi.fn(),
          rotate: vi.fn(),
          scale: vi.fn(),
          drawImage: vi.fn(),
          globalAlpha: 1,
          globalCompositeOperation: "source-over",
          canvas: this,
        } as any;
      }
      transferToImageBitmap(): unknown {
        return { width: this.width, height: this.height, close: vi.fn() } as unknown as ImageBitmap;
      }
    };
  }
});
afterAll(() => {
  if (OriginalOffscreenCanvas) (globalThis as any).OffscreenCanvas = OriginalOffscreenCanvas;
});

// ── Harness ──────────────────────────────────────────────────────────────────
// Mounts the REAL useCanvasKeyboard hook (which now calls useLayerActions() and
// routes through the shared funnels). Returns the live editor + engine so tests
// can seed facade-owned layers and read post-keystroke state.
function makeKeyboardHarness(docId = "kb-facade") {
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(docId, "KB Facade", 800, 600);
  ws.addDocument(session);
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);
  let captured: ReturnType<typeof useEditor> | undefined;
  let capturedLayerActions: ReturnType<typeof useLayerActions> | undefined;

  function Harness() {
    const editor = useEditor();
    captured = editor;
    capturedLayerActions = useLayerActions();
    useCanvasKeyboard({
      isSpacePressed: () => false,
      setIsSpacePressed: vi.fn(),
      isAltPressed: () => false,
      setIsAltPressed: vi.fn(),
      isPanning: () => false,
      setIsPanning: vi.fn(),
      stopMomentum: vi.fn(),
      fitToScreenAndRender: vi.fn(),
      syncViewport: vi.fn(),
      getCanvasContainerRef: () => undefined,
    });
    return null;
  }

  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
        <Harness />
      </EditorProvider>
    ),
    container,
  );

  return {
    ws,
    session,
    renderer,
    scheduler,
    container,
    getEditor: () => captured!,
    getLayerActions: () => capturedLayerActions!,
    dispose: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

// Seeds a facade from the engine then adds `name` through the facade and projects
// the snapshot into the engine, leaving that layer facade-owned.
async function seedFacadeOwnedLayer(engine: any, name: string) {
  const facade = getFacade(engine.getId());
  seedFacadeFromEngine(engine, facade);
  const snap = await facade.addLayer(name);
  engine.applyFacadeSnapshot(snap);
  return engine.getLayers().find((l: any) => l.name === name)!;
}

function fireKey(opts: KeyboardEventInit) {
  window.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...opts }));
}

beforeEach(() => {
  localStorage.clear();
  __resetFacadeRegistryForTests();
  const clearFn = (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests;
  if (typeof clearFn === "function") clearFn();
  toastMock.mockClear();
});

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  vi.restoreAllMocks();
});

describe("canvas keyboard shortcuts route through facade funnels (flag ON)", () => {
  it("Ctrl+Shift+N: routes Add Layer through facade.addLayer (no raw engine.addLayer, no phantom history)", async () => {
    localStorage.setItem("photrez.facade", "1");
    const h = makeKeyboardHarness();
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    const facade = getFacade(engine.getId());

    const addSpy = vi.spyOn(facade, "addLayer");
    const engineAddSpy = vi.spyOn(engine, "addLayer");
    const commitSpy = vi.spyOn(editor.workspace.getActiveHistory()!, "commit");
    const recordSpy = vi.spyOn(editor.workspace.getActiveHistory()!, "recordSnapshotHistory");
    const before = engine.getLayers().length;

    fireKey({ key: "N", code: "KeyN", ctrlKey: true, shiftKey: true });
    await new Promise((r) => setTimeout(r, 0));

    // Routed through the migrated funnel -> facade owns the add.
    expect(addSpy).toHaveBeenCalledTimes(1);
    expect(engineAddSpy).not.toHaveBeenCalled();
    // No TS history entry written by the keyboard path (facade path is history-less).
    expect(commitSpy).not.toHaveBeenCalled();
    expect(recordSpy).not.toHaveBeenCalled();
    // The layer still appears in the engine (projected snapshot).
    expect(engine.getLayers().length).toBe(before + 1);
    h.dispose();
  });

  it("Delete: routes through EditorClient.deleteLayer (no raw engine.deleteLayer, no phantom history, no E_FACADE_OWNED)", async () => {
    localStorage.setItem("photrez.facade", "1");
    const h = makeKeyboardHarness();
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    const victim = await seedFacadeOwnedLayer(engine, "Victim");
    engine.setActiveLayer(victim.id);

    const facade = getFacade(engine.getId());
    const facadeDeleteSpy = vi.spyOn(facade, "deleteLayer");
    const engineDeleteSpy = vi.spyOn(engine, "deleteLayer");
    const commitSpy = vi.spyOn(editor.workspace.getActiveHistory()!, "commit");
    const recordSpy = vi.spyOn(editor.workspace.getActiveHistory()!, "recordSnapshotHistory");

    fireKey({ key: "Delete" });
    await new Promise((r) => setTimeout(r, 0));

    // Routed through the migrated funnel -> facade delete, never the raw engine.
    expect(facadeDeleteSpy).toHaveBeenCalledTimes(1);
    expect(facadeDeleteSpy).toHaveBeenCalledWith(victim.id);
    expect(engineDeleteSpy).not.toHaveBeenCalled();
    // No commit-then-throw: the raw engine call that would raise E_FACADE_OWNED
    // is never reached, so no history entry was written before any throw.
    expect(commitSpy).not.toHaveBeenCalled();
    expect(recordSpy).not.toHaveBeenCalled();
    expect(editor.workspace.getActiveHistory()!.getUndoCount()).toBe(0);
    // Projection removed the layer from the engine view.
    expect(engine.getLayer(victim.id)).toBeUndefined();
    expect(h.renderer.destroyTexture).toHaveBeenCalledWith(victim.id);
    h.dispose();
  });

  it("Backspace: routes through the same delete funnel as Delete", async () => {
    localStorage.setItem("photrez.facade", "1");
    const h = makeKeyboardHarness();
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    const victim = await seedFacadeOwnedLayer(engine, "Victim2");
    engine.setActiveLayer(victim.id);

    const facade = getFacade(engine.getId());
    const facadeDeleteSpy = vi.spyOn(facade, "deleteLayer");
    const engineDeleteSpy = vi.spyOn(engine, "deleteLayer");

    fireKey({ key: "Backspace" });
    await new Promise((r) => setTimeout(r, 0));

    expect(facadeDeleteSpy).toHaveBeenCalledTimes(1);
    expect(engineDeleteSpy).not.toHaveBeenCalled();
    expect(editor.workspace.getActiveHistory()!.getUndoCount()).toBe(0);
    expect(engine.getLayer(victim.id)).toBeUndefined();
    h.dispose();
  });

  it("digit 5: routes Opacity through facade.setOpacity (no raw engine.setLayerOpacity, no phantom history)", async () => {
    localStorage.setItem("photrez.facade", "1");
    const h = makeKeyboardHarness();
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    const layer = await seedFacadeOwnedLayer(engine, "OpacityLayer");
    engine.setActiveLayer(layer.id);
    // Set opacity via the facade (raw engine.setLayerOpacity throws E_FACADE_OWNED
    // on a facade-owned layer under flag ON, so we cannot use it here).
    const facade = getFacade(engine.getId());
    engine.applyFacadeSnapshot(await facade.setOpacity(layer.id, 0.3));

    const setOpacitySpy = vi.spyOn(facade, "setOpacity");
    const rawSetOpacitySpy = vi.spyOn(engine, "setLayerOpacity");
    const commitSpy = vi.spyOn(editor.workspace.getActiveHistory()!, "commit");

    fireKey({ key: "5" });
    await new Promise((r) => setTimeout(r, 0));

    expect(setOpacitySpy).toHaveBeenCalledTimes(1);
    expect(setOpacitySpy).toHaveBeenCalledWith(layer.id, 0.5);
    expect(rawSetOpacitySpy).not.toHaveBeenCalled();
    expect(commitSpy).not.toHaveBeenCalled();
    expect(editor.workspace.getActiveHistory()!.getUndoCount()).toBe(0);
    expect(engine.getLayer(layer.id)?.opacity).toBeCloseTo(0.5);
    h.dispose();
  });
});

describe("canvas keyboard shortcuts legacy routing (flag OFF, byte-identical)", () => {
  it("Ctrl+Shift+N: flag OFF calls raw engine.addLayer + commits history (no facade)", async () => {
    localStorage.setItem("photrez.facade", "0");
    const h = makeKeyboardHarness();
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    const facade = getFacade(engine.getId());
    const facadeAddSpy = vi.spyOn(facade, "addLayer");
    const engineAddSpy = vi.spyOn(engine, "addLayer");
    const commitSpy = vi.spyOn(editor.workspace.getActiveHistory()!, "commit");
    const before = engine.getLayers().length;

    fireKey({ key: "N", code: "KeyN", ctrlKey: true, shiftKey: true });

    expect(facadeAddSpy).not.toHaveBeenCalled();
    expect(engineAddSpy).toHaveBeenCalledTimes(1);
    expect(commitSpy).toHaveBeenCalledTimes(1);
    expect(engine.getLayers().length).toBe(before + 1);
    expect(editor.workspace.getActiveHistory()!.canUndo()).toBe(true);
    h.dispose();
  });

  it("Delete: flag OFF calls raw engine.deleteLayer + records history (no facade)", async () => {
    localStorage.setItem("photrez.facade", "0");
    const h = makeKeyboardHarness();
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    const top = engine.addLayer("Top");
    engine.setActiveLayer(top.id);
    const initialCount = engine.getLayers().length;

    const facade = getFacade(engine.getId());
    const facadeDeleteSpy = vi.spyOn(facade, "deleteLayer");
    const engineDeleteSpy = vi.spyOn(engine, "deleteLayer");
    const history = editor.workspace.getActiveHistory()!;
    const commitSpy = vi.spyOn(history, "commit");
    const recordSpy = vi.spyOn(history, "recordSnapshotHistory");

    fireKey({ key: "Delete" });

    expect(facadeDeleteSpy).not.toHaveBeenCalled();
    expect(engineDeleteSpy).toHaveBeenCalledTimes(1);
    expect(engine.getLayers()).toHaveLength(initialCount - 1);
    expect(engine.getLayer(top.id)).toBeUndefined();
    expect(editor.workspace.getActiveHistory()!.canUndo()).toBe(true);
    // Delta (a): the funnel's flag-OFF delete records via recordSnapshotHistory,
    // NOT the old history.commit - this is an honest delta vs the old keyboard path.
    expect(recordSpy).toHaveBeenCalledTimes(1);
    expect(commitSpy).not.toHaveBeenCalled();
    h.dispose();
  });

  it("Delete: flag OFF on a 1-layer (Background) doc shows the Background toast (keyboard now matches panel)", async () => {
    localStorage.setItem("photrez.facade", "0");
    const h = makeKeyboardHarness();
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    // The blank doc has a single Background layer, which is the active layer.
    expect(engine.getLayers()).toHaveLength(1);
    const onlyLayer = engine.getLayer(engine.getActiveLayerId()!);
    expect(onlyLayer?.isBackground).toBe(true);
    const before = engine.getLayers().length;

    fireKey({ key: "Delete" });

    // Delta (c): the old keyboard path silently did nothing on a 1-layer doc;
    // routing through the funnel now surfaces the same toast the panel shows.
    expect(toastMock).toHaveBeenCalledWith("Cannot delete the Background layer", "warn");
    expect(engine.getLayers()).toHaveLength(before); // nothing deleted
    h.dispose();
  });

  it("digit 5: flag OFF calls raw engine.setLayerOpacity + commits history (no facade)", async () => {
    localStorage.setItem("photrez.facade", "0");
    const h = makeKeyboardHarness();
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    const layer = engine.addLayer("Top");
    engine.setActiveLayer(layer.id);

    const facade = getFacade(engine.getId());
    const setOpacitySpy = vi.spyOn(facade, "setOpacity");
    const rawSetOpacitySpy = vi.spyOn(engine, "setLayerOpacity");
    const commitSpy = vi.spyOn(editor.workspace.getActiveHistory()!, "commit");

    fireKey({ key: "5" });
    await new Promise((r) => setTimeout(r, 0));

    expect(setOpacitySpy).not.toHaveBeenCalled();
    expect(rawSetOpacitySpy).toHaveBeenCalledTimes(1);
    expect(rawSetOpacitySpy).toHaveBeenCalledWith(layer.id, 0.5);
    expect(commitSpy).toHaveBeenCalledTimes(1);
    expect(engine.getLayer(layer.id)?.opacity).toBeCloseTo(0.5);
    h.dispose();
  });

  it("digit 5 twice: flag OFF skips history on the repeated (no-op) digit", async () => {
    localStorage.setItem("photrez.facade", "0");
    const h = makeKeyboardHarness();
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    const layer = engine.addLayer("Top");
    engine.setActiveLayer(layer.id);
    engine.setLayerOpacity(layer.id, 0.5);

    fireKey({ key: "5" }); // same opacity -> no-op, no commit
    expect(editor.workspace.getActiveHistory()!.getUndoCount()).toBe(0);
    fireKey({ key: "7" }); // different -> commit
    await new Promise((r) => setTimeout(r, 0));
    expect(engine.getLayer(layer.id)?.opacity).toBeCloseTo(0.7);
    expect(editor.workspace.getActiveHistory()!.getUndoCount()).toBe(1);
    fireKey({ key: "7" }); // repeat -> no-op
    expect(editor.workspace.getActiveHistory()!.getUndoCount()).toBe(1);
    h.dispose();
  });
});

describe("canvas keyboard delete multi-select routing", () => {
  it("Delete multi-select: flag OFF routes legacy deleteMultipleLayers (two raw engine.deleteLayer calls, no facade)", async () => {
    localStorage.setItem("photrez.facade", "0");
    const h = makeKeyboardHarness();
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    const a = engine.addLayer("A");
    const b = engine.addLayer("B");
    editor.setSelectedLayerIds([a.id, b.id]);
    const before = engine.getLayers().length; // bg + A + B

    const facade = getFacade(engine.getId());
    const facadeDeleteSpy = vi.spyOn(facade, "deleteLayer");
    const engineDeleteSpy = vi.spyOn(engine, "deleteLayer");

    fireKey({ key: "Delete" });

    expect(facadeDeleteSpy).not.toHaveBeenCalled();
    expect(engineDeleteSpy).toHaveBeenCalledTimes(2); // legacy deleteMultipleLayers preserved
    expect(engine.getLayers()).toHaveLength(before - 2);
    expect(engine.getLayer(a.id)).toBeUndefined();
    expect(engine.getLayer(b.id)).toBeUndefined();
    h.dispose();
  });

  it("Delete multi-select: flag ON all facade-owned routes facade.deleteLayer per layer (no raw engine)", async () => {
    localStorage.setItem("photrez.facade", "1");
    const h = makeKeyboardHarness();
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    const a = await seedFacadeOwnedLayer(engine, "A");
    const b = await seedFacadeOwnedLayer(engine, "B");
    engine.setActiveLayer(a.id);
    editor.setSelectedLayerIds([a.id, b.id]);

    const facade = getFacade(engine.getId());
    const facadeDeleteSpy = vi.spyOn(facade, "deleteLayer");
    const engineDeleteSpy = vi.spyOn(engine, "deleteLayer");

    fireKey({ key: "Delete" });
    await new Promise((r) => setTimeout(r, 0));

    expect(facadeDeleteSpy).toHaveBeenCalledTimes(2);
    expect(engineDeleteSpy).not.toHaveBeenCalled();
    expect(engine.getLayer(a.id)).toBeUndefined();
    expect(engine.getLayer(b.id)).toBeUndefined();
    h.dispose();
  });

  it("Delete multi-select: flag ON mixed ownership is rejected with a toast and deletes nothing", async () => {
    localStorage.setItem("photrez.facade", "1");
    const h = makeKeyboardHarness();
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    const owned = await seedFacadeOwnedLayer(engine, "Owned");
    const legacy = engine.addLayer("Legacy");
    editor.setSelectedLayerIds([owned.id, legacy.id]);

    const facade = getFacade(engine.getId());
    const facadeDeleteSpy = vi.spyOn(facade, "deleteLayer");
    const engineDeleteSpy = vi.spyOn(engine, "deleteLayer");
    const before = engine.getLayers().length;

    fireKey({ key: "Delete" });

    // Mixed selection is rejected atomically (ADR 0008/0009 contract) - no partial
    // mutation and the MIXED_OWNERSHIP_MESSAGE toast is surfaced.
    expect(facadeDeleteSpy).not.toHaveBeenCalled();
    expect(engineDeleteSpy).not.toHaveBeenCalled();
    expect(engine.getLayers()).toHaveLength(before);
    expect(toastMock).toHaveBeenCalledWith(MIXED_OWNERSHIP_MESSAGE, "warn");
    h.dispose();
  });
});

describe("canvas keyboard delete parity with panel funnel", () => {
  it("keystroke Delete and a direct panel-funnel call produce identical end state (flag OFF)", async () => {
    const runDelete = async (via: "key" | "funnel") => {
      const h = makeKeyboardHarness(`kb-parity-${via}`);
      await new Promise((r) => setTimeout(r, 0));
      const editor = h.getEditor();
      const engine = h.session.engine;
      const top = engine.addLayer("Top");
      engine.setActiveLayer(top.id);
      if (via === "key") fireKey({ key: "Delete" });
      else h.getLayerActions().handleDeleteActiveLayer();
      // Compare the meaningful end state (not the auto-generated layer id, which
      // differs per harness doc id): one layer remains and it is the Background.
      const activeLayer = engine.getLayer(engine.getActiveLayerId()!);
      const after = { count: engine.getLayers().length, activeIsBackground: !!activeLayer?.isBackground };
      h.dispose();
      return after;
    };
    const viaKey = await runDelete("key");
    const viaFunnel = await runDelete("funnel");
    // Both paths converge on the same end state: one layer removed, Background now active.
    expect(viaKey.count).toBe(viaFunnel.count);
    expect(viaKey.activeIsBackground).toBe(viaFunnel.activeIsBackground);
    expect(viaKey.count).toBe(1);
    expect(viaKey.activeIsBackground).toBe(true);
  });
});

describe("canvas keyboard facade routing - add then delete leaves no orphan state (flag ON)", () => {
  it("add then delete in a single session leaves no facade/engine orphan and history stays clean under flag ON", async () => {
    localStorage.setItem("photrez.facade", "1");
    const h = makeKeyboardHarness();
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    const initial = engine.getLayers().length;
    const initialIds = new Set(engine.getLayers().map((l) => l.id));

    // Add via keyboard (facade)
    fireKey({ key: "N", code: "KeyN", ctrlKey: true, shiftKey: true });
    await new Promise((r) => setTimeout(r, 0));
    expect(engine.getLayers().length).toBe(initial + 1);
    // The newly-added layer is the one not present before the add (the blank
    // doc's original layer is the Background and cannot be deleted, so we must
    // target the actual added layer — it is not index 0).
    const added = engine.getLayers().find((l) => !initialIds.has(l.id))!;

    // Switch tool (round-trip) — must not leak listeners/state.
    editor.setActiveTool("move");
    editor.setActiveTool("brush");

    // Delete the just-added facade layer via keyboard (facade).
    engine.setActiveLayer(added.id);
    const beforeDelete = engine.getLayers().length;
    fireKey({ key: "Delete" });
    await new Promise((r) => setTimeout(r, 0));

    expect(engine.getLayers().length).toBe(beforeDelete - 1);
    expect(engine.getLayer(added.id)).toBeUndefined();
    // No phantom history across the round-trip.
    expect(editor.workspace.getActiveHistory()!.getUndoCount()).toBe(0);
    h.dispose();
  });
});
