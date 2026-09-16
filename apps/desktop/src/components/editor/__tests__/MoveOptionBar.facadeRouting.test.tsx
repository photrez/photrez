// Call-site wiring tests for routing committed transform edits (position, rotation,
// reset, align, flip) away from the legacy TS engine mutators and toward the native
// TransformLayer commit path, plus the flag-OFF contract that must stay synchronous.
//
// The funnel boundary (`facadeCommitNumericTransform`) is replaced with a double that
// reproduces its real contract: the same guards (a layer already gone or locked at
// click time, or a value that equals the projected one -> false; a layer that
// disappears or locks between the click and the commit hop -> a rejected command), the
// same async rejection behaviour on a failed command, and on success it writes the
// committed transform back through the REAL snapshot projection
// (`engine.applyFacadeSnapshot`) instead of the legacy mutators - which is what the
// native arm's RenderDelta does in production. Everything else (the ownership flag,
// `resolveSelectionRoute`, the engine model, TS history) is real. The projection-based
// write is what makes the flip-twice case meaningful: the second click has to derive
// `flipH: false` from the flag the projection just wrote.
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { MoveOptionBar } from "../MoveOptionBar";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { useCanvasKeyboard } from "../canvas/useCanvasKeyboard";
import { useLayerActions } from "../layers/useLayerActions";
import { WorkspaceManager } from "@/engine/workspace";
import type { DocumentEngine } from "@/engine/document";
import type { Transform2D } from "@/engine/types";
import { isFacadeOwnedLayer } from "@/engine/document";
import { getFacade, seedFacadeFromEngine, __resetFacadeRegistryForTests, MIXED_OWNERSHIP_MESSAGE } from "@/lib/protocol/facadeRegistry";
import { getWasmExportModule } from "../wasmExport";
import { showToast } from "../Toast";

vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("@/components/editor/Toast", () => ({ showToast: vi.fn() }));

const { funnel, forcedLegacy } = vi.hoisted(() => ({
  funnel: vi.fn(),
  forcedLegacy: new Set<string>(),
}));

vi.mock("@/lib/protocol/facadeRegistry", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  facadeCommitNumericTransform: funnel,
}));

// Ownership is delegated to the real predicate except for ids listed in forcedLegacy,
// which are reported as legacy-owned. That is how a mixed selection is built: the
// routed batch must refuse the whole set before writing, because a legacy write on the
// facade-owned members throws E_FACADE_OWNED once the others already changed.
vi.mock("@/engine/document", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  const real = original.isFacadeOwnedLayer as (id: string) => boolean;
  return { ...original, isFacadeOwnedLayer: (id: string) => !forcedLegacy.has(id) && real(id) };
});

const toastMock = vi.mocked(showToast);

type Editor = ReturnType<typeof useEditor>;

function tick(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

const unhandled: unknown[] = [];
const onUnhandled = (e: unknown) => unhandled.push(e);

// The facade-owned seed layer is created through the real Rust facade, so the bridge
// has to be armed with the same wasm package the app arms at boot.
beforeAll(async () => {
  await getWasmExportModule();
});

// -- Harness ------------------------------------------------------------------

function openDoc(docId: string) {
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(docId, "Routing", 800, 600);
  ws.addDocument(session);
  return { ws, engine: session.engine };
}

// Make `name` a facade-owned layer: created through the facade and projected back into
// the engine, which is how the app arms a routed layer.
async function seedOwnedLayer(engine: DocumentEngine, name = "Owned"): Promise<string> {
  const facade = getFacade(engine.getId());
  await seedFacadeFromEngine(engine as never, facade);
  engine.applyFacadeSnapshot(await facade.addLayer(name));
  const layer = engine.getLayers().find((l) => l.name === name);
  if (!layer) throw new Error("setup: facade-created layer did not project into the engine");
  engine.setActiveLayer(layer.id);
  return layer.id;
}

function transformOf(engine: DocumentEngine, layerId: string) {
  return engine.getLayer(layerId)!.transform;
}

// Unchanged-value guard copied field-for-field from the real funnel. A JSON.stringify
// compare would depend on key order and on which keys the patch happens to carry, so
// it would not reproduce the real contract (and could pass a test the app fails).
function transformDiffers(current: Transform2D, next: Transform2D): boolean {
  return (
    next.x !== current.x ||
    next.y !== current.y ||
    next.scaleX !== current.scaleX ||
    next.scaleY !== current.scaleY ||
    next.rotation !== current.rotation ||
    next.flipH !== current.flipH ||
    next.flipV !== current.flipV
  );
}

// What the funnel will actually send: a seam may pass a literal patch or a function
// of the transform its commit hop reads. Resolve it the way the funnel does.
function patchSent(callIndex: number, current: Transform2D): Record<string, unknown> {
  const arg = funnel.mock.calls[callIndex][2];
  return typeof arg === "function"
    ? (arg as (c: Transform2D) => Record<string, unknown>)(current)
    : (arg as Record<string, unknown>);
}

function clickAria(container: HTMLElement, aria: string): HTMLButtonElement {
  const btn = container.querySelector<HTMLButtonElement>(`button[aria-label="${aria}"]`);
  expect(btn, `button[aria-label="${aria}"]`).toBeTruthy();
  btn!.click();
  return btn!;
}

function clickByText(container: HTMLElement, text: string): HTMLButtonElement {
  const btn = Array.from(container.querySelectorAll("button")).find((b) => b.textContent === text);
  expect(btn, `button with text "${text}"`).toBeTruthy();
  (btn as HTMLButtonElement).click();
  return btn as HTMLButtonElement;
}

// The bar renders exactly three text fields, in order X, Y, rotation.
function submitField(container: HTMLElement, index: number, value: string): void {
  const input = container.querySelectorAll<HTMLInputElement>('input[type="text"]')[index];
  expect(input, `numeric field ${index}`).toBeTruthy();
  input.focus();
  input.dispatchEvent(new Event("focus", { bubbles: true }));
  input.value = value;
  input.dispatchEvent(new InputEvent("input", { bubbles: true }));
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
}

async function renderBar(ws: WorkspaceManager, layerId: string, extraIds: string[] = []) {
  const renderer = {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    resize: vi.fn(),
    resizeToViewport: vi.fn(),
  };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);
  let editor: Editor | undefined;
  function Harness() {
    editor = useEditor();
    return <MoveOptionBar />;
  }
  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as never} scheduler={scheduler as never}>
        <Harness />
      </EditorProvider>
    ),
    container,
  );
  // Selection is applied after mount: the provider adopts the document's active layer
  // while mounting, which would otherwise override it.
  editor!.setSelectedLayerId(layerId);
  editor!.setSelectedLayerIds([layerId, ...extraIds]);
  await tick();
  expect(editor!.selectedLayerId()).toBe(layerId);
  const select = async (primary: string, all: string[]) => {
    editor!.setSelectedLayerId(primary);
    editor!.setSelectedLayerIds(all);
    await tick();
    expect(editor!.selectedLayerId()).toBe(primary);
    expect(editor!.selectedLayerIds()).toEqual(all);
  };
  return { container, scheduler, dispose, select, getEditor: () => editor! };
}

function keyboardHarness(docId: string) {
  const { ws, engine } = openDoc(docId);
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);
  function Harness() {
    useLayerActions();
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
      <EditorProvider workspace={ws} renderer={renderer as never} scheduler={scheduler as never}>
        <Harness />
      </EditorProvider>
    ),
    container,
  );
  return { ws, engine, scheduler, dispose };
}

// Lock a facade-owned layer the way the native SetLocked arm does: through the
// snapshot projection. The legacy engine setter is blocked on owned layers, and a
// direct field write would be cleared by the next projection.
function lockThroughProjection(engine: DocumentEngine, layerId: string): void {
  const snapshot = getFacade(engine.getId()).snapshot as unknown as {
    version: number;
    layers: Array<Record<string, unknown>>;
  };
  engine.applyFacadeSnapshot({
    ...snapshot,
    version: snapshot.version + 1,
    layers: snapshot.layers.map((l) => (l.id === layerId ? { ...l, locked: true } : l)),
  } as never);
}

// The funnel double: real guards, projection-based write, async resolution. It does
// NOT model the per-document commit queue: overlapping commits are serialized inside
// the real funnel (see facadeNumericCommitRace.test.ts), and a double cannot prove a
// behavior it replaces.
function installFunnelDouble(): void {
  funnel.mockImplementation(
    async (
      engine: DocumentEngine,
      layerId: string,
      patch: Record<string, unknown> | ((current: Transform2D) => Record<string, unknown>),
    ): Promise<boolean> => {
      // Issue-time guard, in the same synchronous stretch the real funnel uses (an
      // async body runs to its first await before anything else can).
      const atIssue = engine.getLayer(layerId);
      if (!atIssue || atIssue.locked) return false;
      await Promise.resolve();
      // Hop-time re-check: the real funnel reports a vanished or newly-locked layer
      // as an error, never as a no-op that hides the lost edit.
      const layer = engine.getLayer(layerId);
      if (!layer) throw new Error(`Layer ${layerId} no longer exists.`);
      if (layer.locked) throw new Error(`Layer ${layerId} is locked.`);
      const next = {
        ...layer.transform,
        ...(typeof patch === "function" ? patch(layer.transform) : patch),
      };
      if (!transformDiffers(layer.transform, next)) return false;
      const snapshot = getFacade(engine.getId()).snapshot as unknown as {
        version: number;
        layers: Array<Record<string, unknown>>;
      };
      const entry = snapshot.layers.find((l) => l.id === layerId);
      if (!entry) return false;
      // The real arm advances the facade snapshot before handing the projection back,
      // so the committed values are written there too: a projection carrying only one
      // layer would otherwise reset its siblings to the values the fake left stale.
      Object.assign(entry, {
        x: next.x,
        y: next.y,
        scaleX: next.scaleX,
        scaleY: next.scaleY,
        rotation: next.rotation,
        flipH: next.flipH,
        flipV: next.flipV,
      });
      snapshot.version += 1;
      // The real arm keeps the facade's expected-version cursor in step with the
      // snapshot it advanced; leaving it behind would let a later hop in this double
      // pass a version check the real arm would fail.
      getFacade(engine.getId()).renderedVersion = snapshot.version;
      engine.applyFacadeSnapshot({ ...snapshot } as never);
      return true;
    },
  );
}

beforeEach(() => {
  localStorage.clear();
  forcedLegacy.clear();
  unhandled.length = 0;
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  toastMock.mockClear();
  process.on("unhandledRejection", onUnhandled);
  installFunnelDouble();
});

afterEach(() => {
  process.off("unhandledRejection", onUnhandled);
  localStorage.removeItem("photrez.facade");
  forcedLegacy.clear();
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  document.body.replaceChildren();
  vi.restoreAllMocks();
  funnel.mockReset();
});

describe("MoveOptionBar routes committed transforms through the native commit (flag ON)", () => {
  it("setup: a facade-created layer is facade-owned and starts at the origin", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { engine } = openDoc("setup-check");
    const id = await seedOwnedLayer(engine);
    expect(isFacadeOwnedLayer(id)).toBe(true);
    expect(transformOf(engine, id).x).toBe(0);
  });

  it("X submit: exactly one native commit, no legacy mutator, no TS history entry", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-x");
    const id = await seedOwnedLayer(engine);
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const flipSpy = vi.spyOn(engine, "flipLayer");
    const historySpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const undoBefore = ws.getActiveHistory()!.getUndoCount();
    const bar = await renderBar(ws, id);

    submitField(bar.container, 0, "150");
    await tick();

    expect(funnel).toHaveBeenCalledTimes(1);
    expect(funnel.mock.calls[0][1]).toBe(id);
    expect(funnel.mock.calls[0][2]).toEqual({ x: 150 });
    expect(transformSpy).not.toHaveBeenCalled();
    expect(flipSpy).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    expect(ws.getActiveHistory()!.getUndoCount()).toBe(undoBefore);
    // Read back from the engine model: the value is only there once the committed
    // transform came back through the projection.
    expect(transformOf(engine, id).x).toBe(150);
    expect(transformOf(engine, id).y).toBe(0);
    expect(bar.scheduler.requestRender).toHaveBeenCalled();
    bar.dispose();
  });

  it("rotation submit: exactly one native commit carrying only the angle", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-rot");
    const id = await seedOwnedLayer(engine);
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const historySpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const bar = await renderBar(ws, id);

    submitField(bar.container, 2, "33");
    await tick();

    expect(funnel).toHaveBeenCalledTimes(1);
    expect(funnel.mock.calls[0][2]).toEqual({ rotation: 33 });
    expect(transformSpy).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    expect(transformOf(engine, id).rotation).toBe(33);
    bar.dispose();
  });

  it("Reset: one native commit and the transform returns to default", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-reset");
    const id = await seedOwnedLayer(engine);
    const notifySpy = vi.spyOn(ws, "notifyVisualChange");
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const historySpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const bar = await renderBar(ws, id);
    submitField(bar.container, 0, "150");
    await tick();
    expect(transformOf(engine, id).x).toBe(150);
    funnel.mockClear();

    clickByText(bar.container, "Reset");
    await tick();

    expect(funnel).toHaveBeenCalledTimes(1);
    expect(funnel.mock.calls[0][2]).toEqual({
      x: 0,
      y: 0,
      scaleX: 1,
      scaleY: 1,
      rotation: 0,
      flipH: false,
      flipV: false,
    });
    expect(transformSpy).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    expect(notifySpy).toHaveBeenCalled();
    expect(ws.getActiveHistory()!.getUndoCount()).toBe(0);
    expect(transformOf(engine, id).x).toBe(0);
    bar.dispose();
  });

  it("Align: one native commit per aligned layer and no combined TS entry", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-align");
    const id = await seedOwnedLayer(engine);
    const otherId = await seedOwnedLayer(engine, "Owned2");
    const bar = await renderBar(ws, id, [otherId]);
    // Move both layers off the align target through the routed path first.
    submitField(bar.container, 0, "50");
    await tick();
    expect(transformOf(engine, id).x).toBe(50);
    await bar.select(otherId, [otherId]);
    submitField(bar.container, 0, "60");
    await tick();
    expect(transformOf(engine, otherId).x).toBe(60);
    await bar.select(id, [id, otherId]);
    funnel.mockClear();
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const historySpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    expect(bar.getEditor().selectedLayerIds()).toEqual([id, otherId]);

    clickAria(bar.container, "Align left");
    await tick();
    await tick();

    expect(transformOf(engine, id).x).toBe(0);
    expect(transformOf(engine, otherId).x).toBe(0);
    // One native entry per aligned layer where the legacy path wrote a single
    // combined entry: routed batch undo granularity differs by design.
    expect(funnel).toHaveBeenCalledTimes(2);
    expect(funnel.mock.calls.map((c) => (c[2] as { x?: number }).x)).toEqual([0, 0]);
    expect(transformSpy).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    expect(ws.getActiveHistory()!.getUndoCount()).toBe(0);
    bar.dispose();
  });

  it("Flip H twice returns the transform to its original state", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-flip-twice");
    const id = await seedOwnedLayer(engine);
    const bar = await renderBar(ws, id);
    submitField(bar.container, 0, "150");
    await tick();
    const original = { ...transformOf(engine, id) };
    funnel.mockClear();
    const flipSpy = vi.spyOn(engine, "flipLayer");

    clickAria(bar.container, "Flip H");
    await tick();
    expect(funnel).toHaveBeenCalledTimes(1);
    expect(patchSent(0, original)).toEqual({ flipH: true });
    expect(transformOf(engine, id).flipH).toBe(true);

    const afterFirst = { ...transformOf(engine, id) };
    clickAria(bar.container, "Flip H");
    await tick();

    // The second click has to negate the flag the projection wrote, not the one the
    // click before it read.
    expect(funnel).toHaveBeenCalledTimes(2);
    expect(patchSent(1, afterFirst)).toEqual({ flipH: false });
    expect(flipSpy).not.toHaveBeenCalled();
    expect(transformOf(engine, id)).toEqual(original);
    expect(ws.getActiveHistory()!.getUndoCount()).toBe(0);
    bar.dispose();
  });

  it("two flip clicks in one tick both apply; neither is swallowed as unchanged", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-flip-rapid");
    const id = await seedOwnedLayer(engine);
    const original = { ...transformOf(engine, id) };
    const bar = await renderBar(ws, id);

    // No tick between the clicks: the second one reads the same projected flag the
    // first one did, so it must send what the FIRST commit actually projected.
    clickAria(bar.container, "Flip H");
    clickAria(bar.container, "Flip H");
    await tick();
    await tick();

    expect(funnel).toHaveBeenCalledTimes(2);
    expect(toastMock).not.toHaveBeenCalled();
    expect(unhandled).toHaveLength(0);
    expect(transformOf(engine, id)).toEqual(original);
    bar.dispose();
  });

  it("Flip V routes the vertical flag only", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-flip-v");
    const id = await seedOwnedLayer(engine);
    const original = { ...transformOf(engine, id) };
    const bar = await renderBar(ws, id);

    clickAria(bar.container, "Flip V");
    await tick();

    expect(funnel).toHaveBeenCalledTimes(1);
    expect(patchSent(0, original)).toEqual({ flipV: true });
    expect(transformOf(engine, id).flipV).toBe(true);
    expect(transformOf(engine, id).flipH).toBe(false);
    bar.dispose();
  });

  it("rejected commit: error toast, no visual-change notify, no unhandled rejection", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-reject");
    const id = await seedOwnedLayer(engine);
    const before = { ...transformOf(engine, id) };
    funnel.mockRejectedValueOnce(new Error("E_EXTERNAL_PENDING"));
    const notifySpy = vi.spyOn(ws, "notifyVisualChange");
    const bar = await renderBar(ws, id);

    clickAria(bar.container, "Flip H");
    await tick();

    expect(unhandled).toHaveLength(0);
    expect(toastMock).toHaveBeenCalledWith(expect.stringContaining("E_EXTERNAL_PENDING"), "error");
    expect(notifySpy).not.toHaveBeenCalled();
    expect(transformOf(engine, id)).toEqual(before);
    bar.dispose();
  });

  it("funnel resolving false (locked or unchanged layer): no success refresh", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-false");
    const id = await seedOwnedLayer(engine);
    funnel.mockImplementationOnce(async () => false);
    const notifySpy = vi.spyOn(ws, "notifyVisualChange");
    const bar = await renderBar(ws, id);

    submitField(bar.container, 0, "150");
    await tick();

    expect(funnel).toHaveBeenCalledTimes(1);
    expect(notifySpy).not.toHaveBeenCalled();
    expect(transformOf(engine, id).x).toBe(0);
    bar.dispose();
  });

  it("locked layer: nothing is dispatched and the legacy mutator stays untouched", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-locked");
    const id = await seedOwnedLayer(engine);
    const barProbe = await renderBar(ws, id);
    submitField(barProbe.container, 0, "150");
    await tick();
    barProbe.dispose();
    lockThroughProjection(engine, id);
    expect(engine.getLayer(id)!.locked).toBe(true);
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const flipSpy = vi.spyOn(engine, "flipLayer");
    const historySpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    funnel.mockClear();
    const bar = await renderBar(ws, id);

    clickAria(bar.container, "Flip H");
    clickByText(bar.container, "Reset");
    clickAria(bar.container, "Align left");
    submitField(bar.container, 0, "220");
    await tick();

    expect(funnel).not.toHaveBeenCalled();
    expect(transformSpy).not.toHaveBeenCalled();
    expect(flipSpy).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    expect(ws.getActiveHistory()!.getUndoCount()).toBe(0);
    expect(transformOf(engine, id).x).toBe(150);
    bar.dispose();
  });

  it("no-op submit and Reset at default dispatch nothing", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-noop");
    const id = await seedOwnedLayer(engine);
    const historySpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const bar = await renderBar(ws, id);

    submitField(bar.container, 0, "0");
    clickByText(bar.container, "Reset");
    await tick();

    expect(funnel).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    expect(ws.getActiveHistory()!.getUndoCount()).toBe(0);
    bar.dispose();
  });

  it("mixed ownership selection: toast, zero dispatch, zero mutation", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-mixed");
    const id = await seedOwnedLayer(engine);
    const otherId = await seedOwnedLayer(engine, "Owned2");
    const bar = await renderBar(ws, id, [otherId]);
    submitField(bar.container, 0, "50");
    await tick();
    await bar.select(otherId, [otherId]);
    submitField(bar.container, 0, "60");
    await tick();
    await bar.select(id, [id, otherId]);
    forcedLegacy.add(otherId);
    funnel.mockClear();
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const historySpy = vi.spyOn(ws.getActiveHistory()!, "commit");

    clickAria(bar.container, "Align left");
    await tick();

    expect(toastMock).toHaveBeenCalledWith(MIXED_OWNERSHIP_MESSAGE, "error");
    expect(funnel).not.toHaveBeenCalled();
    expect(transformSpy).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    expect(unhandled).toHaveLength(0);
    expect(transformOf(engine, id).x).toBe(50);
    expect(transformOf(engine, otherId).x).toBe(60);
    bar.dispose();
  });

  // The seam's job is to forward every submit, in submit order. The commit queue
  // itself lives in the funnel and is proven against the real arm in
  // facadeNumericCommitRace.test.ts; a double cannot prove what it replaces.
  it("two rapid submits both reach the funnel, in submit order", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-rapid");
    const id = await seedOwnedLayer(engine);
    const bar = await renderBar(ws, id);

    submitField(bar.container, 0, "150");
    submitField(bar.container, 0, "220");
    await tick();
    await tick();

    expect(funnel).toHaveBeenCalledTimes(2);
    expect(toastMock).not.toHaveBeenCalled();
    expect(unhandled).toHaveLength(0);
    expect(funnel.mock.calls.map((c) => (c[2] as { x?: number }).x)).toEqual([150, 220]);
    expect(transformOf(engine, id).x).toBe(220);
    bar.dispose();
  });

  it("tool round-trip leaves no orphan facade transform session and still routes", async () => {
    localStorage.setItem("photrez.facade", "1");
    const { ws, engine } = openDoc("route-roundtrip");
    const id = await seedOwnedLayer(engine);
    const facade = getFacade(engine.getId());
    const bar = await renderBar(ws, id);

    clickAria(bar.container, "Flip H");
    await tick();
    expect(facade.transientTransformActive()).toBe(false);

    bar.getEditor().setActiveTool("brush");
    bar.getEditor().setActiveTool("move");
    await tick();
    expect(facade.transientTransformActive()).toBe(false);
    expect(transformOf(engine, id).flipH).toBe(true);
    funnel.mockClear();

    const projected = { ...transformOf(engine, id) };
    clickAria(bar.container, "Flip H");
    await tick();

    expect(funnel).toHaveBeenCalledTimes(1);
    expect(patchSent(0, projected)).toEqual({ flipH: false });
    expect(transformOf(engine, id).flipH).toBe(false);
    expect(facade.transientTransformActive()).toBe(false);
    bar.dispose();
  });
});

describe("MoveOptionBar - keeps photrez.facade-OFF behavior (legacy path synchronous)", () => {
  it("Flip H click mutates and commits inside the click handler", async () => {
    localStorage.setItem("photrez.facade", "0");
    const { ws, engine } = openDoc("off-flip");
    const id = engine.addLayer("Plain").id;
    const flipSpy = vi.spyOn(engine, "flipLayer");
    const historySpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const bar = await renderBar(ws, id);

    clickAria(bar.container, "Flip H");

    // No await between the click and these assertions: deferring the legacy commit by
    // one microtask is exactly what broke the first routing attempt.
    expect(flipSpy).toHaveBeenCalledTimes(1);
    expect(flipSpy).toHaveBeenCalledWith(id, "h");
    expect(historySpy).toHaveBeenCalledTimes(1);
    expect(funnel).not.toHaveBeenCalled();
    expect(transformOf(engine, id).flipH).toBe(true);
    bar.dispose();
  });

  it("X submit mutates once and commits exactly one history entry inside the handler", async () => {
    localStorage.setItem("photrez.facade", "0");
    const { ws, engine } = openDoc("off-x");
    const id = engine.addLayer("Plain").id;
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const historySpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const undoBefore = ws.getActiveHistory()!.getUndoCount();
    const bar = await renderBar(ws, id);

    submitField(bar.container, 0, "150");

    expect(transformSpy).toHaveBeenCalledTimes(1);
    expect(transformSpy).toHaveBeenCalledWith(id, expect.objectContaining({ x: 150 }));
    expect(historySpy).toHaveBeenCalledTimes(1);
    expect(funnel).not.toHaveBeenCalled();
    expect(ws.getActiveHistory()!.getUndoCount()).toBe(undoBefore + 1);
    bar.dispose();
  });

  it("Reset and rotation each commit synchronously", async () => {
    localStorage.setItem("photrez.facade", "0");
    const { ws, engine } = openDoc("off-reset");
    const id = engine.addLayer("Plain").id;
    engine.transformLayer(id, { x: 50, y: 30, rotation: 20 });
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const historySpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const undoBefore = ws.getActiveHistory()!.getUndoCount();
    const bar = await renderBar(ws, id);
    transformSpy.mockClear();
    historySpy.mockClear();

    clickByText(bar.container, "Reset");
    expect(historySpy).toHaveBeenCalledTimes(1);
    expect(transformSpy).toHaveBeenCalledTimes(1);

    submitField(bar.container, 2, "45");
    expect(historySpy).toHaveBeenCalledTimes(2);
    expect(transformSpy).toHaveBeenCalledTimes(2);
    expect(transformOf(engine, id).rotation).toBe(45);
    expect(ws.getActiveHistory()!.getUndoCount()).toBe(undoBefore + 2);
    bar.dispose();
  });

  it("Align captures the history snapshot before mutating, with the commit call after", async () => {
    localStorage.setItem("photrez.facade", "0");
    const { ws, engine } = openDoc("off-align");
    const id = engine.addLayer("Plain").id;
    engine.transformLayer(id, { x: 50, y: 30 });
    const snapshotSpy = vi.spyOn(engine, "snapshot");
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const historySpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    transformSpy.mockClear();
    const bar = await renderBar(ws, id);

    clickAria(bar.container, "Align left");

    // The snapshot is captured before the write, so the recorded state is the
    // pre-mutation one even though history.commit() runs afterwards.
    expect(snapshotSpy.mock.invocationCallOrder[0]).toBeLessThan(
      transformSpy.mock.invocationCallOrder[0],
    );
    const recorded = historySpy.mock.calls[0][0] as unknown as {
      layers: Array<{ id: string; transform: { x: number } }>;
    };
    expect(recorded.layers.find((l) => l.id === id)!.transform.x).toBe(50);
    expect(transformOf(engine, id).x).toBe(0);
    expect(historySpy).toHaveBeenCalledTimes(1);
    expect(funnel).not.toHaveBeenCalled();
    bar.dispose();
  });
});

describe("Ctrl+G / Ctrl+Shift+G share the option bar routing", () => {
  it("flag ON: Ctrl+G commits through the native path and never calls engine.flipLayer", async () => {
    localStorage.setItem("photrez.facade", "1");
    const h = keyboardHarness("kb-on");
    const id = await seedOwnedLayer(h.engine);
    h.engine.setActiveLayer(id);
    const original = { ...transformOf(h.engine, id) };
    const flipSpy = vi.spyOn(h.engine, "flipLayer");
    const historySpy = vi.spyOn(h.ws.getActiveHistory()!, "commit");
    const undoBefore = h.ws.getActiveHistory()!.getUndoCount();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "g", ctrlKey: true, bubbles: true }));

    // Legacy guard order holds: the engine write and the TS commit stay unreached even
    // after the routed dispatch settles.
    expect(flipSpy).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    await tick();
    expect(funnel).toHaveBeenCalledTimes(1);
    expect(patchSent(0, original)).toEqual({ flipH: true });
    expect(transformOf(h.engine, id).flipH).toBe(true);
    expect(h.ws.getActiveHistory()!.getUndoCount()).toBe(undoBefore);

    const afterFirst = { ...transformOf(h.engine, id) };
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "g", ctrlKey: true, shiftKey: true, bubbles: true }),
    );
    await tick();

    expect(funnel).toHaveBeenCalledTimes(2);
    expect(patchSent(1, afterFirst)).toEqual({ flipV: true });
    expect(transformOf(h.engine, id).flipV).toBe(true);
    expect(transformOf(h.engine, id).flipH).toBe(true);
    expect(unhandled).toHaveLength(0);
    h.dispose();
  });

  it("flag OFF: Ctrl+G flips through the legacy engine synchronously", () => {
    localStorage.setItem("photrez.facade", "0");
    const h = keyboardHarness("kb-off");
    const id = h.engine.addLayer("Plain").id;
    h.engine.setActiveLayer(id);
    const flipSpy = vi.spyOn(h.engine, "flipLayer");
    const historySpy = vi.spyOn(h.ws.getActiveHistory()!, "commit");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "g", ctrlKey: true, bubbles: true }));

    expect(flipSpy).toHaveBeenCalledWith(id, "h");
    expect(historySpy).toHaveBeenCalledTimes(1);
    expect(funnel).not.toHaveBeenCalled();
    expect(transformOf(h.engine, id).flipH).toBe(true);
    h.dispose();
  });
});
