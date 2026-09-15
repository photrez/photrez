// Regression: every path that ends a facade transform drag WITHOUT committing must
// release the facade's one transient transform slot.
//
// The numeric commit funnel refuses while that slot is held (beginning a commit
// would take the slot from a live gesture). If an abandoned gesture leaks the slot,
// the refusal becomes permanent: every later numeric edit of that document (option
// bar fields, Reset, Align, Flip, the properties panel) throws "release the current
// handle first" while no handle is visible, and the only recovery is an unrelated
// gesture or reopening the document.
//
// Each test below abandons a real gesture through one teardown path, then proves a
// numeric commit still lands through the REAL funnel and the REAL protocol arm.
import { beforeAll, beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { createRoot, createSignal } from "solid-js";
import { DocumentEngine } from "@/engine/document";
import type { LayerNode, Transform2D } from "@/engine/types";
import {
  __resetFacadeRegistryForTests,
  facadeCommitNumericTransform,
  getFacade,
  peekFacade,
  removeFacade,
  seedFacadeFromEngine,
} from "@/lib/protocol/facadeRegistry";
import { getWasmExportModule } from "../wasmExport";
import { useSelectionTransformDrag } from "../useSelectionTransformDrag";

const { mockEditorState } = vi.hoisted(() => ({
  mockEditorState: {} as Record<string, unknown>,
}));

vi.mock("../shell/EditorContext", () => ({
  useEditor: () => mockEditorState,
}));

let wasm: { protocol_reset: (docId: string) => void } | null = null;
const usedDocs: string[] = [];

beforeAll(async () => {
  wasm = await getWasmExportModule();
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
});

afterEach(() => {
  for (const id of usedDocs) wasm?.protocol_reset(id);
  usedDocs.length = 0;
  localStorage.removeItem("photrez.facade");
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  vi.restoreAllMocks();
});

function pointer(overrides: Partial<PointerEvent> = {}): PointerEvent {
  return {
    button: 0,
    clientX: 100,
    clientY: 100,
    pointerId: 1,
    shiftKey: false,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    target: document.createElement("div"),
    currentTarget: document.createElement("div"),
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
    ...overrides,
  } as unknown as PointerEvent;
}

// Real engine + real facade behind a stub editor context: the hook only reads
// workspace/scheduler/selection from the context, so the drag lifecycle and the
// protocol arm below it are both the production ones.
async function harness(docId: string) {
  usedDocs.push(docId);
  const engine = new DocumentEngine(docId, "Abandon", 800, 600);
  const facade = getFacade(docId);
  await seedFacadeFromEngine(engine as never, facade);
  engine.applyFacadeSnapshot(await facade.addLayer("Dragged"));
  engine.applyFacadeSnapshot(await facade.addLayer("Other"));
  const [draggedId, otherId] = engine.getLayers().map((l) => l.id);
  const history = { commit: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const workspace = {
    getActiveEngine: () => engine,
    getActiveHistory: () => history,
    notifyVisualChange: vi.fn(),
  };
  const [selectedLayerId, setSelectedLayerId] = createSignal<string | null>(draggedId);
  const [layers, setLayers] = createSignal<readonly LayerNode[]>(engine.getLayers());

  let drag!: ReturnType<typeof useSelectionTransformDrag>;
  let dispose!: () => void;
  createRoot((d) => {
    dispose = d;
    Object.assign(mockEditorState, {
      workspace,
      renderer: { uploadImage: vi.fn(), destroyTexture: vi.fn() },
      scheduler,
      selectedLayerId,
      layers,
      zoom: () => 1,
      pan: () => ({ x: 0, y: 0 }),
      activeTool: () => "move",
      hoverHandle: () => null,
      setHoverHandle: vi.fn(),
      moveSnapEnabled: () => false,
      hoverPos: () => null,
      setHoverPos: vi.fn(),
      layerTransformSession: () => null,
      setLayerTransformSession: vi.fn(),
      commitTransformState: vi.fn(),
      constrainRatio: () => true,
    });
    drag = useSelectionTransformDrag({ getSvgRef: () => undefined });
  });

  const transformOf = (id: string): Transform2D => engine.getLayer(id)!.transform;
  // Begin a facade drag on `draggedId`, exactly as pointerdown on a handle does.
  const beginGesture = () => {
    drag.handlePointerDown(pointer(), "move");
    expect(drag.dragState()).not.toBeNull();
    expect(facade.transientTransformActive()).toBe(true);
  };

  return {
    engine,
    facade,
    draggedId,
    otherId,
    scheduler,
    transformOf,
    beginGesture,
    drag: () => drag,
    selectOtherLayer: () => {
      setSelectedLayerId(otherId);
      setLayers([...engine.getLayers()]);
    },
    hideEngine: () => {
      workspace.getActiveEngine = () => undefined as never;
    },
    showEngine: () => {
      workspace.getActiveEngine = () => engine;
    },
    dispose: () => dispose(),
  };
}

describe("abandoned facade transform drags release the transient commit slot", () => {
  // Outcome first: a leaked slot is the bug, and the refusal message is what the
  // user would see on every later numeric edit.
  async function commitOutcome(h: Awaited<ReturnType<typeof harness>>, patch: Partial<Transform2D>) {
    return facadeCommitNumericTransform(h.engine, h.draggedId, patch).then(
      (ok) => (ok ? "applied" : "noop"),
      (e: Error) => e.message,
    );
  }

  it("setup: a live gesture blocks a numeric commit (the refusal this guards)", async () => {
    const h = await harness("abandon-setup");
    h.beginGesture();

    await expect(commitOutcome(h, { x: 10 })).resolves.toContain("transform gesture is in progress");
    h.dispose();
  });

  it("a mid-drag selection switch releases the slot and a numeric commit still lands", async () => {
    const h = await harness("abandon-selection-switch");
    h.beginGesture();

    // The selection moves while the pointer is down, so the next pointermove sees a
    // different layer and drops the drag. No pointerup for that gesture ever arrives.
    h.selectOtherLayer();
    h.drag().handlePointerMove(pointer());
    expect(h.drag().dragState()).toBeNull();

    await expect(commitOutcome(h, { x: 40 })).resolves.toBe("applied");
    expect(h.facade.transientTransformActive()).toBe(false);
    expect(h.transformOf(h.draggedId).x).toBe(40);
    h.dispose();
  });

  it("lost pointer capture releases the slot and a numeric commit still lands", async () => {
    const h = await harness("abandon-lost-capture");
    h.beginGesture();

    h.drag().handleLostPointerCapture(pointer());
    expect(h.drag().dragState()).toBeNull();

    await expect(commitOutcome(h, { rotation: 25 })).resolves.toBe("applied");
    expect(h.facade.transientTransformActive()).toBe(false);
    expect(h.transformOf(h.draggedId).rotation).toBe(25);
    h.dispose();
  });

  it("pointerup with no active engine releases the slot and a numeric commit still lands", async () => {
    const h = await harness("abandon-no-engine");
    h.beginGesture();

    // The document goes away under the gesture: the commit branch is skipped, so only
    // the teardown can release what pointerdown took.
    h.hideEngine();
    await h.drag().handlePointerUp(pointer());
    h.showEngine();
    expect(h.drag().dragState()).toBeNull();

    await expect(commitOutcome(h, { y: 60 })).resolves.toBe("applied");
    expect(h.facade.transientTransformActive()).toBe(false);
    expect(h.transformOf(h.draggedId).y).toBe(60);
    h.dispose();
  });

  it("releasing after the document was closed does not recreate the facade it evicted", async () => {
    const docId = "abandon-after-close";
    const h = await harness(docId);
    h.beginGesture();

    // Document close evicts the facade so a reopened id gets a fresh one. The
    // teardown that follows must release, not resurrect.
    removeFacade(docId);
    h.drag().handleLostPointerCapture(pointer());

    // The only claim: the teardown released nothing that was still there and did not
    // put a facade back on a closed document id.
    expect(peekFacade(docId)).toBeUndefined();
    h.dispose();
  });
});
