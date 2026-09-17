// Call-site wiring for the Properties panel's Align, Distribute and numeric transform
// commits.
//
// With native transform authority enabled, none of them may call the legacy engine
// mutator on a facade-owned layer (it throws E_FACADE_OWNED), and none may write a TS
// history entry for an edit the native arm already recorded. The tests drive the real
// facade, the real snapshot projection and the real protocol arm; the counting boundary
// is the command envelope the native arm sends, captured on applyCommand.
//
// The flag-OFF assertions pin the call order the legacy branch has always had, because
// the routing decision has to stay synchronous and must not reorder snapshot / mutate /
// history.commit.
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi, type MockInstance } from "vitest";
import { render } from "solid-js/web";
import { PropertiesPanel } from "../PropertiesPanel";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { WorkspaceManager } from "@/engine/workspace";
import type { DocumentEngine } from "@/engine/document";
import type { Transform2D } from "@/engine/types";
import {
  MIXED_OWNERSHIP_MESSAGE,
  __resetFacadeRegistryForTests,
  facadeCommitNumericTransform,
  getFacade,
  seedFacadeFromEngine,
} from "@/lib/protocol/facadeRegistry";
import type { AlignMode } from "../layers/transformRouting";
import * as bridge from "@/lib/protocol/bridge";
import { getWasmExportModule } from "../wasmExport";
import { showToast } from "../Toast";

vi.mock("@/components/editor/Toast", () => ({ showToast: vi.fn() }));
vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));

const { forcedLegacy } = vi.hoisted(() => ({ forcedLegacy: new Set<string>() }));

// Ownership is the real predicate except for ids listed here, which report as
// legacy-owned. That is how a mixed selection is built.
vi.mock("@/engine/document", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  const real = original.isFacadeOwnedLayer as (id: string) => boolean;
  return { ...original, isFacadeOwnedLayer: (id: string) => !forcedLegacy.has(id) && real(id) };
});

const toastMock = vi.mocked(showToast);

// The panel reads its canvas size from the editor state, and the align targets below
// are the canvas edges and center of an 800x600 canvas holding 100x100 layers.
const DOC_W = 800;
const DOC_H = 600;
const LAYER_SIZE = 100;
const ALIGN_CASES: Array<[AlignMode, Partial<Transform2D>, number]> = [
  ["left", { x: 0 }, 0],
  ["center-h", { x: 350 }, 1],
  ["right", { x: 700 }, 2],
  ["top", { y: 0 }, 3],
  ["center-v", { y: 250 }, 4],
  ["bottom", { y: 500 }, 5],
];

let applySpy: MockInstance;

function tick(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

// A routed batch awaits one command per member; every hop is a promise chain, so a few
// macrotask turns drain it.
async function flush(): Promise<void> {
  for (let i = 0; i < 4; i++) await tick();
}

// The native arm's outbound commits, in send order.
function transformCommands(): Array<{ id: string; transform: Transform2D }> {
  return applySpy.mock.calls
    .map((call) => (call[0] as { command?: { type?: string; id?: string; transform?: Transform2D } }).command)
    .filter(
      (c): c is { type: string; id: string; transform: Transform2D } =>
        c?.type === "transformLayer" && typeof c.id === "string" && c.transform !== undefined,
    )
    .map((c) => ({ id: c.id, transform: c.transform }));
}

// The panel's align grid renders exactly six buttons, in the order left, center-h,
// right, top, center-v, bottom.
function alignButton(container: HTMLElement, index: number): HTMLButtonElement {
  const grid = container.querySelector<HTMLElement>(".grid-cols-6");
  expect(grid, "align button grid").toBeTruthy();
  const buttons = Array.from(grid!.querySelectorAll("button"));
  expect(buttons).toHaveLength(6);
  return buttons[index];
}

// The distribute grid renders two buttons, horizontal then vertical.
function distributeButton(container: HTMLElement, index: number): HTMLButtonElement {
  const grid = container.querySelector<HTMLElement>(".mt-2.grid.grid-cols-2");
  expect(grid, "distribute button grid").toBeTruthy();
  const buttons = Array.from(grid!.querySelectorAll("button"));
  expect(buttons).toHaveLength(2);
  return buttons[index];
}

interface Panel {
  engine: DocumentEngine;
  ws: WorkspaceManager;
  container: HTMLElement;
  scheduler: { requestRender: () => void };
  dispose: () => void;
  select: (primary: string, all: string[]) => Promise<void>;
  history: () => NonNullable<ReturnType<WorkspaceManager["getActiveHistory"]>>;
}

function mountPanel(ws: WorkspaceManager): Panel {
  const renderer = {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    resize: vi.fn(),
    resizeToViewport: vi.fn(),
  };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);
  let editor: ReturnType<typeof useEditor> | undefined;
  function Harness() {
    editor = useEditor();
    return <PropertiesPanel />;
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
  // while mounting, which would otherwise override it. The canvas size the panel reads
  // is synced from the active document by the provider itself.
  const select = async (primary: string, all: string[]) => {
    editor!.setSelectedLayerId(primary);
    editor!.setSelectedLayerIds(all);
    await tick();
  };
  return {
    engine: ws.getActiveEngine() as DocumentEngine,
    ws,
    container,
    scheduler,
    dispose,
    select,
    history: () => ws.getActiveHistory()!,
  };
}

interface Placement {
  name: string;
  x: number;
  y: number;
}

// Make `positions` facade-owned layers: created through the facade and projected back
// into the engine, then placed through the routed commit path, which is how the app
// arms a routed layer.
async function seedOwnedLayers(engine: DocumentEngine, positions: Placement[]): Promise<string[]> {
  const facade = getFacade(engine.getId());
  await seedFacadeFromEngine(engine as never, facade);
  const ids: string[] = [];
  for (const placement of positions) {
    engine.applyFacadeSnapshot(await facade.addLayer(placement.name, LAYER_SIZE, LAYER_SIZE));
    const layer = engine.getLayers().find((l) => l.name === placement.name);
    if (!layer) throw new Error("setup: facade-created layer did not project into the engine");
    ids.push(layer.id);
  }
  // The native add arm may size a new layer from the document instead of from the
  // command, so the rendered box the expectations use is forced here.
  for (const id of ids) {
    const layer = engine.getLayer(id)!;
    const scaleX = LAYER_SIZE / layer.width;
    const scaleY = LAYER_SIZE / layer.height;
    if (scaleX !== 1 || scaleY !== 1) {
      if (!(await facadeCommitNumericTransform(engine, id, { scaleX, scaleY }))) {
        throw new Error("setup: could not size a facade-owned layer");
      }
    }
    const sized = engine.getLayer(id)!;
    if (
      Math.round(sized.width * sized.transform.scaleX) !== LAYER_SIZE ||
      Math.round(sized.height * sized.transform.scaleY) !== LAYER_SIZE
    ) {
      throw new Error("setup: seeded layer does not render at the expected size");
    }
  }
  for (let i = 0; i < ids.length; i++) {
    const { x, y } = positions[i];
    if (x === 0 && y === 0) continue;
    if (!(await facadeCommitNumericTransform(engine, ids[i], { x, y }))) {
      throw new Error("setup: could not place a facade-owned layer");
    }
  }
  const last = engine.getLayer(ids[ids.length - 1]);
  if (last) engine.setActiveLayer(last.id);
  return ids;
}

async function openOwnedPanel(docId: string, positions: Placement[]) {
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(docId, "Panel Routing", DOC_W, DOC_H);
  ws.addDocument(session);
  const engine = session.engine;
  const ids = await seedOwnedLayers(engine, positions);
  const panel = mountPanel(ws);
  await panel.select(ids[0], ids);
  return { panel, ids };
}

async function openLegacyPanel(docId: string, positions: Placement[]) {
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(docId, "Panel Legacy", DOC_W, DOC_H);
  ws.addDocument(session);
  const engine = session.engine;
  const ids = positions.map((placement) => {
    const layer = engine.addLayer(placement.name);
    engine.transformLayer(layer.id, { x: placement.x, y: placement.y });
    return layer.id;
  });
  const panel = mountPanel(ws);
  await panel.select(ids[0], ids);
  return { panel, ids, engine };
}

beforeAll(async () => {
  // The facade-owned seed layers are created through the real Rust facade, so the
  // bridge has to be armed with the same wasm package the app arms at boot.
  await getWasmExportModule();
});

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem("photrez.facadeAuthority", "wasm");
  forcedLegacy.clear();
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  toastMock.mockClear();
  applySpy = vi.spyOn(bridge, "applyCommand");
  applySpy.mockClear();
});

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  forcedLegacy.clear();
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  document.body.replaceChildren();
  vi.restoreAllMocks();
});

describe("PropertiesPanel transform actions with native transform authority on", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facade", "1");
  });

  it.each(ALIGN_CASES)(
    "Align %s: one native command per owned layer carrying the absolute transform",
    async (type: AlignMode, expected: Partial<Transform2D>, buttonIndex: number) => {
      const { panel, ids } = await openOwnedPanel(`pp-align-${type}`, [
        { name: "Owned A", x: 50, y: 50 },
        { name: "Owned B", x: 50, y: 50 },
      ]);
      const base = { ...panel.engine.getLayer(ids[0])!.transform };
      const transformSpy = vi.spyOn(panel.engine, "transformLayer");
      const historySpy = vi.spyOn(panel.history(), "commit");
      const undoBefore = panel.history().getUndoCount();
      applySpy.mockClear();

      alignButton(panel.container, buttonIndex).click();
      await flush();

      const commanded = transformCommands();
      expect(commanded.map((c) => c.id)).toEqual(ids);
      expect(commanded.map((c) => c.transform)).toEqual([
        { ...base, ...expected },
        { ...base, ...expected },
      ]);
      expect(transformSpy).not.toHaveBeenCalled();
      // The native arm owns the history entry for these commits.
      expect(historySpy).not.toHaveBeenCalled();
      expect(panel.history().getUndoCount()).toBe(undoBefore);
      // The committed result is read from the model the renderer reads, not the DOM.
      expect(panel.engine.getLayer(ids[0])!.transform).toEqual({ ...base, ...expected });
      expect(panel.engine.getLayer(ids[1])!.transform).toEqual({ ...base, ...expected });
      panel.dispose();
    },
  );

  it.each([
    ["h" as const, "x" as const, 0],
    ["v" as const, "y" as const, 1],
  ])(
    "Distribute %s: one native command per moving member with the spread position",
    async (axis: "h" | "v", field: "x" | "y", buttonIndex: number) => {
      const along = [0, 10, 20, 400];
      const positions: Placement[] = along.map((value, i) =>
        axis === "h"
          ? { name: `Owned ${i}`, x: value, y: 0 }
          : { name: `Owned ${i}`, x: 0, y: value },
      );
      const { panel, ids } = await openOwnedPanel(`pp-dist-${axis}`, positions);
      const transformSpy = vi.spyOn(panel.engine, "transformLayer");
      const historySpy = vi.spyOn(panel.history(), "commit");
      applySpy.mockClear();

      distributeButton(panel.container, buttonIndex).click();
      await flush();

      // The two anchors keep their position; the two inner members are spread between
      // them (100 wide layers in a 500 long span leave a 100 gap split in three).
      const commanded = transformCommands();
      expect(commanded.map((c) => c.id)).toEqual([ids[1], ids[2]]);
      expect(commanded.map((c) => c.transform[field])).toEqual([133, 267]);
      expect(transformSpy).not.toHaveBeenCalled();
      expect(historySpy).not.toHaveBeenCalled();
      expect(panel.history().getUndoCount()).toBe(0);
      expect(panel.engine.getLayer(ids[1])!.transform[field]).toBe(133);
      expect(panel.engine.getLayer(ids[2])!.transform[field]).toBe(267);
      expect(panel.engine.getLayer(ids[0])!.transform[field]).toBe(0);
      expect(panel.engine.getLayer(ids[3])!.transform[field]).toBe(400);
      panel.dispose();
    },
  );

  it("mixed ownership: one toast, nothing moves, no dispatch, no history", async () => {
    const { panel, ids } = await openOwnedPanel("pp-mixed", [
      { name: "Owned A", x: 50, y: 50 },
      { name: "Owned B", x: 50, y: 50 },
    ]);
    forcedLegacy.add(ids[1]);
    const transformSpy = vi.spyOn(panel.engine, "transformLayer");
    const historySpy = vi.spyOn(panel.history(), "commit");
    applySpy.mockClear();

    alignButton(panel.container, 0).click();
    await flush();

    expect(toastMock).toHaveBeenCalledWith(MIXED_OWNERSHIP_MESSAGE, "error");
    expect(transformCommands()).toHaveLength(0);
    expect(transformSpy).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    expect(panel.engine.getLayer(ids[0])!.transform.x).toBe(50);
    expect(panel.engine.getLayer(ids[1])!.transform.x).toBe(50);
    panel.dispose();
  });

  it("Align with a layer already at the target edge: no dispatch, no history entry", async () => {
    const { panel, ids } = await openOwnedPanel("pp-align-noop", [
      { name: "Owned A", x: 0, y: 50 },
      { name: "Owned B", x: 0, y: 50 },
    ]);
    const transformSpy = vi.spyOn(panel.engine, "transformLayer");
    const historySpy = vi.spyOn(panel.history(), "commit");
    applySpy.mockClear();

    alignButton(panel.container, 0).click();
    await flush();

    expect(transformCommands()).toHaveLength(0);
    expect(transformSpy).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    expect(panel.engine.getLayer(ids[0])!.transform.x).toBe(0);
    expect(panel.engine.getLayer(ids[1])!.transform.x).toBe(0);
    panel.dispose();
  });

  it("numeric transform field paths route too: a flip on an owned layer writes one native command", async () => {
    const { panel, ids } = await openOwnedPanel("pp-numeric-route", [
      { name: "Owned A", x: 50, y: 50 },
      { name: "Owned B", x: 50, y: 50 },
    ]);
    const transformSpy = vi.spyOn(panel.engine, "transformLayer");
    const historySpy = vi.spyOn(panel.history(), "commit");
    applySpy.mockClear();
    await panel.select(ids[0], [ids[0]]);

    const flip = panel.container.querySelector<HTMLButtonElement>("button[aria-label='Flip horizontal']");
    expect(flip, "Flip horizontal button").toBeTruthy();
    flip!.click();
    await flush();

    const commanded = transformCommands();
    expect(commanded.map((c) => c.id)).toEqual([ids[0]]);
    expect(commanded[0].transform.flipH).toBe(true);
    expect(transformSpy).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    expect(panel.history().getUndoCount()).toBe(0);
    expect(panel.engine.getLayer(ids[0])!.transform.flipH).toBe(true);
    panel.dispose();
  });
});

describe("PropertiesPanel transform actions - keeps photrez.facade=0 opt-out behavior (native transform authority off)", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facade", "0");
  });

  it("Align left: snapshot before the writes, one history entry after them, no native command", async () => {
    const { panel, ids, engine } = await openLegacyPanel("pp-off-align", [
      { name: "Plain A", x: 50, y: 50 },
      { name: "Plain B", x: 50, y: 50 },
    ]);
    const snapshotSpy = vi.spyOn(engine, "snapshot");
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const historySpy = vi.spyOn(panel.history(), "commit");
    const undoBefore = panel.history().getUndoCount();
    applySpy.mockClear();

    alignButton(panel.container, 0).click();
    await flush();

    // The snapshot is captured before any write, so the recorded state is the
    // pre-mutation one even though the commit call lands after the writes - the same
    // order the panel action has always had.
    expect(snapshotSpy.mock.invocationCallOrder[0]).toBeLessThan(
      transformSpy.mock.invocationCallOrder[0],
    );
    expect(transformSpy.mock.invocationCallOrder[transformSpy.mock.calls.length - 1]).toBeLessThan(
      historySpy.mock.invocationCallOrder[0],
    );
    expect(transformSpy).toHaveBeenCalledTimes(2);
    expect(historySpy).toHaveBeenCalledTimes(1);
    expect(historySpy.mock.calls[0][1]).toBe("Align left");
    const recorded = historySpy.mock.calls[0][0] as unknown as {
      layers: Array<{ id: string; transform: { x: number } }>;
    };
    expect(recorded.layers.find((l) => l.id === ids[0])!.transform.x).toBe(50);
    // Zero seam calls: the routed decision never fires while the flag is off.
    expect(transformCommands()).toHaveLength(0);
    expect(engine.getLayer(ids[0])!.transform.x).toBe(0);
    expect(panel.history().getUndoCount()).toBe(undoBefore + 1);
    panel.dispose();
  });

  it("Distribute horizontally: the writes land before the single history entry", async () => {
    const { panel, ids, engine } = await openLegacyPanel("pp-off-dist", [
      { name: "Plain A", x: 0, y: 0 },
      { name: "Plain B", x: 10, y: 0 },
      { name: "Plain C", x: 20, y: 0 },
      { name: "Plain D", x: 400, y: 0 },
    ]);
    const snapshotSpy = vi.spyOn(engine, "snapshot");
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const historySpy = vi.spyOn(panel.history(), "commit");
    applySpy.mockClear();

    distributeButton(panel.container, 0).click();
    await flush();

    expect(snapshotSpy.mock.invocationCallOrder[0]).toBeLessThan(
      transformSpy.mock.invocationCallOrder[0],
    );
    expect(transformSpy.mock.invocationCallOrder[transformSpy.mock.calls.length - 1]).toBeLessThan(
      historySpy.mock.invocationCallOrder[0],
    );
    expect(transformSpy.mock.calls.map((call) => (call[1] as Transform2D).x)).toEqual([133, 267]);
    expect(historySpy).toHaveBeenCalledTimes(1);
    expect(historySpy.mock.calls[0][1]).toBe("Distribute Horizontally");
    expect(transformCommands()).toHaveLength(0);
    expect(engine.getLayer(ids[1])!.transform.x).toBe(133);
    expect(engine.getLayer(ids[2])!.transform.x).toBe(267);
    expect(panel.scheduler.requestRender).toHaveBeenCalled();
    panel.dispose();
  });

  it("Distribute with nothing to move still records the legacy entry, as it always has", async () => {
    const { panel, engine } = await openLegacyPanel("pp-off-dist-noop", [
      { name: "Plain A", x: 0, y: 0 },
      { name: "Plain B", x: 100, y: 0 },
      { name: "Plain C", x: 200, y: 0 },
      { name: "Plain D", x: 300, y: 0 },
    ]);
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const historySpy = vi.spyOn(panel.history(), "commit");
    applySpy.mockClear();

    distributeButton(panel.container, 0).click();
    await flush();

    expect(transformSpy).not.toHaveBeenCalled();
    expect(historySpy).toHaveBeenCalledTimes(1);
    expect(historySpy.mock.calls[0][1]).toBe("Distribute Horizontally");
    expect(transformCommands()).toHaveLength(0);
    panel.dispose();
  });

  it("Align with nothing to move commits nothing and renders nothing", async () => {
    const { panel, engine } = await openLegacyPanel("pp-off-align-noop", [
      { name: "Plain A", x: 0, y: 50 },
      { name: "Plain B", x: 0, y: 50 },
    ]);
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const historySpy = vi.spyOn(panel.history(), "commit");
    applySpy.mockClear();

    alignButton(panel.container, 0).click();
    await flush();

    expect(transformSpy).not.toHaveBeenCalled();
    expect(historySpy).not.toHaveBeenCalled();
    expect(transformCommands()).toHaveLength(0);
    panel.dispose();
  });

  it("numeric transform field paths keep the legacy commit-before-mutate order", async () => {
    const { panel, ids, engine } = await openLegacyPanel("pp-off-numeric", [
      { name: "Plain A", x: 50, y: 50 },
    ]);
    const transformSpy = vi.spyOn(engine, "transformLayer");
    const historySpy = vi.spyOn(panel.history(), "commit");
    applySpy.mockClear();
    await panel.select(ids[0], [ids[0]]);

    const flip = panel.container.querySelector<HTMLButtonElement>("button[aria-label='Flip horizontal']");
    expect(flip, "Flip horizontal button").toBeTruthy();
    flip!.click();
    await flush();

    // The numeric path records the history entry before it mutates, which is the
    // opposite of the align/distribute order above; both are preserved as they are.
    expect(historySpy.mock.invocationCallOrder[0]).toBeLessThan(
      transformSpy.mock.invocationCallOrder[0],
    );
    expect(historySpy.mock.calls[0][1]).toBe("Flip Horizontal");
    expect(engine.getLayer(ids[0])!.transform.flipH).toBe(true);
    expect(transformCommands()).toHaveLength(0);
    panel.dispose();
  });
});
