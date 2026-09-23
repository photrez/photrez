// Transient selection-transform drag and mixed-selection rejection, asserted at the
// single boundary every protocol command crosses: bridge applyCommand.
//
// Two contracts a pure-function test cannot see:
//
//  1. A drag on a facade-owned selection writes NOTHING to the protocol while the
//     pointer is down (pointermove fires at frame rate) and leaves the TS model
//     untouched. The moving pixels come from the facade's transient transform state
//     and the renderer preview list; the only dispatch is the single TransformLayer
//     command at pointerup. A click that never moves dispatches nothing at all.
//
//  2. A selection spanning a facade-owned layer and a legacy layer is refused before
//     anything is dispatched, written, or repainted. The owned member must never take
//     a legacy write: that write is rejected only after the legacy members already
//     changed, which would strand the selection half-edited.
//
// Real hook, real DocumentEngine, real facade, real protocol arm.
//
// RED evidence (revert-and-restore per assertion group, each run: mutate -> focused
// run `bun run --filter photrez-desktop test --run transientSelection` FAIL -> restore
// with `git checkout --` -> `git diff` empty on the mutated file):
//  1. useSelectionTransformDrag.ts facade pointerup branch: added a
//     workspace.getActiveHistory().commit(...) call before `return` -> FAIL
//     "expected vi.fn() to not be called at all, but actually been called 1 times"
//     on the history.commit assertion -> restored, diff empty.
//  2. useSelectionTransformDrag.ts: duplicated facade.commitTransform() -> FAIL
//     "expected commitTransform to be called 1 times, but got 2 times" on the
//     one-commit assertion -> restored, diff empty.
//  3. engine/document.ts isFacadeOwnedLayer forced to false -> FAIL
//     "expected false to be true" on all three isFacadeOwnedLayer(ownedId) asserts
//     -> restored, diff empty.
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { createRoot, createSignal } from "solid-js";
import { DocumentEngine, isFacadeOwnedLayer } from "@/engine/document";
import type { LayerNode, Transform2D } from "@/engine/types";
import {
  MIXED_OWNERSHIP_MESSAGE,
  __resetFacadeRegistryForTests,
  getFacade,
  seedFacadeFromEngine,
} from "@/lib/protocol/facadeRegistry";
import { routeNumericTransformBatch } from "@/components/editor/layers/transformRouting";
import { showToast } from "@/components/editor/Toast";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { useSelectionTransformDrag } from "@/components/editor/useSelectionTransformDrag";
import type { CommandEnvelope } from "@/lib/protocol/types";

const { mockEditorState, commandLog } = vi.hoisted(() => ({
  mockEditorState: {} as Record<string, unknown>,
  commandLog: { types: [] as string[] },
}));

vi.mock("@/components/editor/shell/EditorContext", () => ({
  useEditor: () => mockEditorState,
}));

vi.mock("@/components/editor/Toast", () => ({ showToast: vi.fn() }));

// applyCommand is the one function every protocol command crosses, under the wasm arm
// and the native arm alike, so its call list IS the dispatch count.
vi.mock("@/lib/protocol/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/protocol/bridge")>();
  return {
    ...actual,
    applyCommand: (envelope: CommandEnvelope) => {
      commandLog.types.push(envelope.command.type);
      return actual.applyCommand(envelope);
    },
  };
});

const toastMock = vi.mocked(showToast);

let wasm: { protocol_reset: (docId: string) => void } | null = null;
const usedDocs: string[] = [];

beforeAll(async () => {
  wasm = await getWasmExportModule();
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "wasm");
  commandLog.types.length = 0;
});

afterEach(() => {
  for (const id of usedDocs) wasm?.protocol_reset(id);
  usedDocs.length = 0;
  commandLog.types.length = 0;
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  vi.clearAllMocks();
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
// workspace/scheduler/selection from the context, so the gesture lifecycle and the
// protocol arm under it are both the production ones.
async function harness(docId: string) {
  usedDocs.push(docId);
  const engine = new DocumentEngine(docId, "TransientSelection", 800, 600);
  const facade = getFacade(docId);
  await seedFacadeFromEngine(engine as never, facade);
  engine.applyFacadeSnapshot(await facade.addLayer("Owned"));
  const ownedId = engine.getLayers()[0].id;
  const history = { commit: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const workspace = {
    getActiveEngine: () => engine,
    getActiveHistory: () => history,
    notifyVisualChange: vi.fn(),
  };
  const [selectedLayerId] = createSignal<string | null>(ownedId);
  const [layers] = createSignal<readonly LayerNode[]>(engine.getLayers());

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

  return {
    engine,
    facade,
    ownedId,
    history,
    scheduler,
    transformOf: (id: string): Transform2D => ({ ...engine.getLayer(id)!.transform }),
    beginGesture: () => {
      drag.handlePointerDown(pointer(), "move");
      expect(drag.dragState()).not.toBeNull();
    },
    drag: () => drag,
    dispose: () => dispose(),
  };
}

describe("transient selection transform drag on a facade-owned selection", () => {
  it("holds the transient slot, sends no protocol command while the pointer is down, and leaves the model untouched", async () => {
    const h = await harness("transient-drag-moves");
    const before = h.transformOf(h.ownedId);
    commandLog.types.length = 0; // the harness seeded the layer through the facade

    expect(isFacadeOwnedLayer(h.ownedId)).toBe(true);
    h.beginGesture();
    expect(h.facade.transientTransformActive()).toBe(true);

    for (let i = 1; i <= 5; i++) {
      h.drag().handlePointerMove(pointer({ clientX: 100 + i * 8, clientY: 100 + i * 4 }));
    }

    expect(commandLog.types).toEqual([]);
    expect(h.transformOf(h.ownedId)).toEqual(before);
    h.dispose();
  });

  it("dispatches exactly one transformLayer command at pointerup and releases the transient slot", async () => {
    const h = await harness("transient-drag-commit");
    const before = h.transformOf(h.ownedId);
    // Counted at the commit call site, not at the dispatch: one pointerup must
    // produce exactly one facade commit, which is the one Rust history entry.
    const commitSpy = vi.spyOn(h.facade, "commitTransform");
    commandLog.types.length = 0;

    expect(isFacadeOwnedLayer(h.ownedId)).toBe(true);
    h.beginGesture();
    h.drag().handlePointerMove(pointer({ clientX: 140, clientY: 120 }));
    await h.drag().handlePointerUp(pointer({ clientX: 140, clientY: 120 }));

    expect(commandLog.types).toEqual(["transformLayer"]);
    expect(commitSpy).toHaveBeenCalledTimes(1);
    // Rust owns history for this path: the facade branch at
    // useSelectionTransformDrag.ts:503-538 returns before the TS history.commit at
    // :558, so the TS history mock must see nothing from this gesture.
    expect(h.history.commit).not.toHaveBeenCalled();
    expect(h.transformOf(h.ownedId).x).toBe(before.x + 40);
    expect(h.transformOf(h.ownedId).y).toBe(before.y + 20);
    expect(h.facade.transientTransformActive()).toBe(false);
    h.dispose();
  });

  it("dispatches nothing for a click that never moves", async () => {
    const h = await harness("transient-drag-click");
    const before = h.transformOf(h.ownedId);
    commandLog.types.length = 0;

    h.beginGesture();
    await h.drag().handlePointerUp(pointer());

    expect(commandLog.types).toEqual([]);
    expect(h.transformOf(h.ownedId)).toEqual(before);
    expect(h.facade.transientTransformActive()).toBe(false);
    h.dispose();
  });
});

describe("mixed selection rejection", () => {
  it("refuses a facade-owned + legacy selection before any dispatch, write, or repaint", async () => {
    const h = await harness("mixed-selection");
    // Created straight on the engine, after the facade snapshot, so the ownership set
    // never sees it: this is what makes the selection mixed rather than all-owned.
    const legacy = h.engine.addLayer("Legacy");
    commandLog.types.length = 0; // drop the harness's own facade addLayer
    // Scope note: this test drives the shared refusal funnel
    // (routeNumericTransformBatch) directly. Caller respect - that the production
    // click handlers route here and never fall through to a legacy write - is proven
    // at the real call sites instead: the MoveOptionBar align click in
    // components/editor/__tests__/MoveOptionBar.facadeRouting.test.tsx:588
    // ("mixed ownership selection: toast, zero dispatch, zero mutation"; asserts no
    // legacy engine.transformLayer and no TS history.commit at :602-611), and the
    // PropertiesPanel align/distribute handlers (PropertiesPanel.tsx:104 and :129) in
    // components/editor/__tests__/PropertiesPanel.facadeAlignDistribute.test.tsx:331
    // ("mixed ownership: one toast, nothing moves, no dispatch, no history").
    const ownedBefore = h.transformOf(h.ownedId);
    const legacyBefore = h.transformOf(legacy.id);
    expect(isFacadeOwnedLayer(h.ownedId)).toBe(true);
    expect(isFacadeOwnedLayer(legacy.id)).toBe(false);

    const status = await routeNumericTransformBatch(
      h.engine,
      [
        { layerId: h.ownedId, patch: { x: 500 } },
        { layerId: legacy.id, patch: { x: 600 } },
      ],
      { requestRender: h.scheduler.requestRender, notifyVisualChange: vi.fn() },
    );

    expect(commandLog.types).toEqual([]);
    expect(h.transformOf(h.ownedId)).toEqual(ownedBefore);
    expect(h.transformOf(legacy.id)).toEqual(legacyBefore);
    expect(h.scheduler.requestRender).not.toHaveBeenCalled();
    expect(status).toBe("mixed-rejected");
    expect(toastMock).toHaveBeenCalledWith(MIXED_OWNERSHIP_MESSAGE, "error");
    h.dispose();
  });
});
