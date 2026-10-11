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
import type { DocumentEngine } from "@/engine/document";
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
  localStorage.setItem("photrez.facadeAuthority", "wasm");
  __resetFacadeRegistryForTests();
  const clearFn = (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests;
  if (typeof clearFn === "function") clearFn();
  toastMock.mockClear();
});

afterEach(() => {
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  vi.restoreAllMocks();
});

describe("canvas keyboard shortcuts route through facade funnels (flag ON)", () => {
  it("Ctrl+Shift+N: routes Add Layer through facade.addLayer (no raw engine.addLayer, no phantom history)", async () => {
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

describe("canvas keyboard delete multi-select routing", () => {

  it("Delete multi-select: flag ON all facade-owned routes facade.deleteLayer per layer (no raw engine)", async () => {
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

describe("canvas keyboard facade routing - add then delete leaves no orphan state (flag ON)", () => {
  it("add then delete in a single session leaves no facade/engine orphan and history stays clean under flag ON", async () => {
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

// --- Arrow-key nudge ----------------------------------------------------------
// The nudge is the one committed transform edit a user repeats by holding the key, so
// the routed value has to be resolved per commit hop instead of read at keydown. It
// also used to be the only committed transform site with no ownership guard: with
// native authority on, engine.moveLayer moved the TS model while the native arm kept
// the old transform, and the next routed commit compared against a value the native
// model never had.
describe("canvas keyboard arrow nudge routes through the numeric transform commit", () => {
  // Every hop is a real protocol command on the document's commit chain.
  async function settle(done: () => boolean, maxTicks = 500): Promise<void> {
    for (let i = 0; i < maxTicks && !done(); i++) await new Promise((r) => setTimeout(r, 0));
  }

  function xy(engine: DocumentEngine, id: string) {
    const t = engine.getLayer(id)!.transform;
    return { x: t.x, y: t.y };
  }

  // A facade-owned layer with the Move tool active, on native authority.
  async function nudgeHarness(docId: string) {
    const h = makeKeyboardHarness(docId);
    await new Promise((r) => setTimeout(r, 0));
    const engine = h.session.engine;
    const layer = await seedFacadeOwnedLayer(engine, "Nudge");
    engine.setActiveLayer(layer.id);
    h.getEditor().setActiveTool("move");
    return { h, editor: h.getEditor(), engine, id: layer.id };
  }

  it("owned layer: one native commit, no engine.moveLayer, no TS history entry", async () => {
    const { h, editor, engine, id } = await nudgeHarness("nudge-single");
    const commitSpy = vi.spyOn(getFacade(engine.getId()), "commitTransform");
    const moveSpy = vi.spyOn(engine, "moveLayer");
    const history = editor.workspace.getActiveHistory()!;
    const historySpy = vi.spyOn(history, "commit");
    const start = xy(engine, id);

    fireKey({ key: "ArrowRight" });
    await settle(() => xy(engine, id).x === start.x + 1);

    expect(commitSpy).toHaveBeenCalledTimes(1);
    expect(moveSpy).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    expect(history.getUndoCount()).toBe(0);
    expect(xy(engine, id)).toEqual({ x: start.x + 1, y: start.y });
    expect(h.scheduler.requestRender).toHaveBeenCalled();
    h.dispose();
  });

  it("key repeat: 10 keydowns issued in one tick all land (start + 10px)", async () => {
    const { h, editor, engine, id } = await nudgeHarness("nudge-repeat");
    const commitSpy = vi.spyOn(getFacade(engine.getId()), "commitTransform");
    const moveSpy = vi.spyOn(engine, "moveLayer");
    const history = editor.workspace.getActiveHistory()!;
    const start = xy(engine, id);

    // One press plus nine OS repeats, dispatched with no await between them: each
    // hop must add its step to what the previous hop projected, so a value read at
    // keydown time would send the same absolute position ten times and land once.
    for (let i = 0; i < 10; i++) fireKey({ key: "ArrowRight", repeat: i > 0 });
    await settle(() => xy(engine, id).x === start.x + 10);

    expect(xy(engine, id)).toEqual({ x: start.x + 10, y: start.y });
    expect(commitSpy).toHaveBeenCalledTimes(10);
    expect(moveSpy).not.toHaveBeenCalled();
    expect(history.getUndoCount()).toBe(0);
    h.dispose();
  });

  it("mixed multi-select: one toast, neither layer moves", async () => {
    const h = makeKeyboardHarness("nudge-mixed");
    await new Promise((r) => setTimeout(r, 0));
    const editor = h.getEditor();
    const engine = h.session.engine;
    const owned = await seedFacadeOwnedLayer(engine, "Owned");
    const legacy = engine.addLayer("LegacyLayer");
    engine.setActiveLayer(owned.id);
    editor.setSelectedLayerIds([owned.id, legacy.id]);
    editor.setActiveTool("move");
    const commitSpy = vi.spyOn(getFacade(engine.getId()), "commitTransform");
    const moveSpy = vi.spyOn(engine, "moveLayer");
    const historySpy = vi.spyOn(editor.workspace.getActiveHistory()!, "commit");
    const before = { owned: xy(engine, owned.id), legacy: xy(engine, legacy.id) };

    fireKey({ key: "ArrowRight" });
    await new Promise((r) => setTimeout(r, 0));

    expect(toastMock).toHaveBeenCalledWith(MIXED_OWNERSHIP_MESSAGE, "error");
    expect(commitSpy).not.toHaveBeenCalled();
    expect(moveSpy).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    expect(xy(engine, owned.id)).toEqual(before.owned);
    expect(xy(engine, legacy.id)).toEqual(before.legacy);
    h.dispose();
  });

  it("position-locked layer is skipped silently", async () => {
    const { h, engine, id } = await nudgeHarness("nudge-lock-on");
    const facade = getFacade(engine.getId());
    engine.applyFacadeSnapshot(await facade.setLayerLocked(id, "position", true));
    const commitSpy = vi.spyOn(facade, "commitTransform");
    const start = xy(engine, id);

    fireKey({ key: "ArrowRight" });
    await new Promise((r) => setTimeout(r, 0));

    expect(commitSpy).not.toHaveBeenCalled();
    expect(xy(engine, id)).toEqual(start);
    expect(toastMock).not.toHaveBeenCalled();
    h.dispose();
  });

  it("a live pointer gesture holding the transform slot refuses the nudge out loud", async () => {
    const { h, engine, id } = await nudgeHarness("nudge-gesture");
    const facade = getFacade(engine.getId());
    // Exactly what a handle drag does between pointerdown and pointerup.
    facade.beginTransform(id, { ...engine.getLayer(id)!.transform });
    facade.updateTransform({ ...engine.getLayer(id)!.transform, x: 500 });
    const moveSpy = vi.spyOn(engine, "moveLayer");
    const start = xy(engine, id);

    fireKey({ key: "ArrowRight" });
    await settle(() => toastMock.mock.calls.length > 0);

    expect(moveSpy).not.toHaveBeenCalled();
    expect(xy(engine, id)).toEqual(start);
    expect(toastMock).toHaveBeenCalledWith(expect.stringContaining("transform gesture"), "error");
    facade.cancelTransform();
    h.dispose();
  });

  it("tool round-trip leaves no orphan state and the next nudge still lands", async () => {
    const { h, editor, engine, id } = await nudgeHarness("nudge-roundtrip");
    const commitSpy = vi.spyOn(getFacade(engine.getId()), "commitTransform");
    const history = editor.workspace.getActiveHistory()!;

    fireKey({ key: "ArrowUp" });
    await settle(() => xy(engine, id).y === -1);

    editor.setActiveTool("brush");
    editor.setActiveTool("move");
    fireKey({ key: "ArrowUp" });
    await settle(() => xy(engine, id).y === -2);

    expect(xy(engine, id)).toEqual({ x: 0, y: -2 });
    expect(commitSpy).toHaveBeenCalledTimes(2);
    expect(history.getUndoCount()).toBe(0);
    h.dispose();
  });
});
