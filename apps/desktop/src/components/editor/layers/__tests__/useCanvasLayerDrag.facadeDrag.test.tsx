// Canvas Move-tool layer drag while the layers are owned by the native editor state.
//
// Two contracts are pinned here, and both are invisible to a pure-function test:
//  1. ZERO protocol calls during the gesture. pointermove fires 50+ fps, so one
//     command per frame is the failure this path exists to prevent. The only
//     dispatch is the commit at pointerup, asserted at the single boundary every
//     protocol command crosses (bridge applyCommand), not at a facade stub.
//  2. The facade's one transient transform slot comes down on EVERY exit. The
//     numeric commit funnel refuses while the slot is held, so a leaked slot turns
//     every later numeric edit of that document into a permanent refusal.
//
// Real hook, real DocumentEngine, real snapshot projection, real protocol arm.
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi, type Mock } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { useCanvasLayerDrag } from "../useCanvasLayerDrag";
import { WorkspaceManager } from "@/engine/workspace";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { ViewportCamera } from "../../../../viewport/viewportCamera";
import {
  __resetFacadeRegistryForTests,
  facadeCommitNumericTransform,
  getFacade,
  peekFacade,
  removeFacade,
  seedFacadeFromEngine,
  transformPreview,
  type FacadeTransformPreview,
} from "@/lib/protocol/facadeRegistry";
import { routeNumericTransformBatch } from "../transformRouting";
import { showToast } from "../../Toast";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import type { CommandEnvelope } from "@/lib/protocol/types";
import type { Transform2D } from "@/engine/types";

vi.mock("../../Toast", () => ({ showToast: vi.fn() }));

// applyCommand is the one function every protocol command crosses, under the wasm
// arm and the native arm alike, so its call list IS the dispatch count.
const { commandLog } = vi.hoisted(() => ({ commandLog: { types: [] as string[] } }));

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

const DOC = "canvas-drag-facade";

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

/** Every way a layer drag can end without applying itself. */
type Exit =
  | "pointercancel"
  | "escape"
  | "lostpointercapture"
  | "layer-identity-change"
  | "no-active-engine"
  | "unmount";

interface Ctx {
  ws: WorkspaceManager;
  engine: DocumentEngine;
  canvasEl: HTMLElement;
  dragApi: ReturnType<typeof useCanvasLayerDrag>;
  setSelectedLayerIds: (ids: string[]) => void;
  onHudUpdate: Mock<(hud: unknown) => void>;
  dispose: () => void;
  /** Protocol command types dispatched since the last reset(). */
  commands: () => string[];
  reset: () => void;
  owned: string[];
  legacyId: string;
}

/**
 * A document whose layers are (or are not) owned by the native editor state.
 * `owned` layers are created through the facade, which is what marks them owned in
 * the projection; `legacy` adds one plain engine layer for the mixed-selection
 * case. Flag off means nothing is owned and nothing is seeded.
 */
async function setup(opts: { owned: number; legacy?: boolean; flag?: boolean }): Promise<Ctx> {
  usedDocs.push(DOC);
  const flagOn = opts.flag !== false;
  if (!flagOn) localStorage.removeItem("photrez.facade");

  const engine = new DocumentEngine(DOC, "Canvas", 800, 600);
  const ws = new WorkspaceManager();
  ws.addDocument({
    engine,
    history: new CommandHistory(),
    displayName: "Canvas",
    sourcePath: null,
    dirty: false,
  });
  ws.switchDocument(DOC);
  // Test layers carry no bitmap, so the alpha-aware hit test would fall through.
  vi.spyOn(engine, "sampleLayerAlpha").mockReturnValue(1);

  const owned: string[] = [];
  if (flagOn) {
    const facade = getFacade(DOC);
    await seedFacadeFromEngine(engine as never, facade);
    for (let i = 0; i < opts.owned; i++) {
      const name = `Owned ${i}`;
      engine.applyFacadeSnapshot(await facade.addLayer(name, 200, 200));
      const layer = engine.getLayers().find((l) => l.name === name);
      if (!layer) throw new Error("setup: facade-created layer did not project into the engine");
      owned.push(layer.id);
      // Park it where the harness clicks, through the real commit seam.
      await facadeCommitNumericTransform(engine, layer.id, { x: 100, y: 100 });
    }
  } else {
    for (let i = 0; i < opts.owned; i++) {
      const layer = engine.addLayer(`Owned ${i}`, 200, 200);
      layer.transform.x = 100;
      layer.transform.y = 100;
      owned.push(layer.id);
    }
  }
  const legacyId = opts.legacy ? engine.addLayer("Legacy", 200, 200).id : "";
  if (legacyId) {
    const l = engine.getLayer(legacyId)!;
    l.transform.x = 100;
    l.transform.y = 400;
  }

  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const camera = new ViewportCamera();
  const container = document.createElement("div");
  document.body.appendChild(container);
  const canvasEl = document.createElement("div");
  canvasEl.setAttribute("data-canvas-container", "true");
  canvasEl.style.position = "absolute";
  canvasEl.style.left = "0px";
  canvasEl.style.top = "0px";
  canvasEl.style.width = "800px";
  canvasEl.style.height = "600px";
  document.body.appendChild(canvasEl);

  let dragApi!: ReturnType<typeof useCanvasLayerDrag>;
  let setSelectedLayerIds!: (ids: string[]) => void;
  const onHudUpdate = vi.fn<(hud: unknown) => void>();

  function Probe() {
    const ed = useEditor();
    dragApi = useCanvasLayerDrag({ onHudUpdate });
    setSelectedLayerIds = (ids: string[]) => ed.setSelectedLayerIds(ids);
    return null;
  }
  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as never} scheduler={scheduler as never} camera={camera}>
        <Probe />
      </EditorProvider>
    ),
    container,
  );
  canvasEl.addEventListener("pointerdown", (e) => dragApi?.handlePointerDown(e as PointerEvent));

  const ctx: Ctx = {
    ws,
    engine,
    canvasEl,
    dragApi,
    setSelectedLayerIds,
    onHudUpdate,
    dispose,
    commands: () => [...commandLog.types],
    reset: () => {
      commandLog.types.length = 0;
    },
    owned,
    legacyId,
  };
  ctx.reset();
  return ctx;
}

function pointerdown(ctx: Ctx, clientX: number, clientY: number, pointerId = 1) {
  ctx.canvasEl.dispatchEvent(
    new PointerEvent("pointerdown", { bubbles: true, cancelable: true, button: 0, clientX, clientY, pointerId }),
  );
}
function pointermove(ctx: Ctx, clientX: number, clientY: number, pointerId = 1) {
  document.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX, clientY, pointerId }));
}
function pointerup(ctx: Ctx, clientX = 0, clientY = 0, pointerId = 1) {
  document.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, clientX, clientY, pointerId }));
}
function pointercancel(ctx: Ctx) {
  void ctx;
  document.dispatchEvent(new PointerEvent("pointercancel", { bubbles: true, pointerId: 1 }));
}
/** Let every queued microtask/macrotask run, so a deferred dispatch cannot hide. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/**
 * End the live gesture through one non-committing exit, then prove the facade slot
 * is free by landing a later numeric edit through the REAL funnel.
 */
async function abandon(ctx: Ctx, exit: Exit) {
  switch (exit) {
    case "pointercancel":
      pointercancel(ctx);
      break;
    case "escape":
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
      break;
    case "lostpointercapture":
      // Registered on document in the capture phase; dispatching at document reaches it.
      document.dispatchEvent(new PointerEvent("lostpointercapture", { pointerId: 1 }));
      break;
    case "layer-identity-change":
      // A Delete landing mid-drag: the projection drops the layer from the model, so
      // the next pointermove can no longer address it and no pointerup commit follows.
      await deleteThroughFacade(ctx, ctx.owned[0]);
      pointermove(ctx, 200, 200);
      break;
    case "no-active-engine": {
      // The document goes away under the gesture: the commit branch is skipped, so
      // only the teardown can release what pointerdown took.
      const spy = vi.spyOn(ctx.ws, "getEngine").mockReturnValue(null);
      pointerup(ctx, 200, 200);
      spy.mockRestore();
      break;
    }
    case "unmount":
      ctx.dispose();
      break;
  }
  await settle();
  expect(ctx.commands(), `${exit} dispatched a command instead of dropping the gesture`).toEqual([]);
  if (exit !== "unmount") {
    expect(ctx.dragApi.isDragging(), `${exit} left the gesture live`).toBe(false);
  }
  expect(peekFacade(DOC)?.transientTransformActive(), `${exit} leaked the facade slot`).toBeFalsy();
  expect(transformPreview(), `${exit} left a preview behind`).toEqual([]);
  // The regression a leaked slot causes: this refusal would be permanent. The
  // dragged layer may itself be gone (identity change), so edit the other one.
  const survivor = ctx.owned[1] ?? ctx.owned[0];
  await expect(facadeCommitNumericTransform(ctx.engine, survivor, { x: 55 })).resolves.toBe(true);
  expect(ctx.engine.getLayer(survivor)!.transform.x).toBe(55);
}

/**
 * Remove a layer the way production does: the facade command plus the
 * authoritative projection into the TS model. Keeps document versions honest,
 * which a hand-written snapshot edit would not.
 */
async function deleteThroughFacade(ctx: Ctx, goneId: string) {
  const facade = getFacade(DOC);
  ctx.engine.applyFacadeSnapshot(await facade.deleteLayer(goneId));
  ctx.reset();
}

const transformOf = (ctx: Ctx, id: string): Transform2D => ({ ...ctx.engine.getLayer(id)!.transform });

/** Routed drag on the first owned layer with `frames` preview writes and the slot taken. */
async function beginRoutedDrag(ctx: Ctx, frames = 3) {
  pointerdown(ctx, 150, 150);
  expect(ctx.dragApi.isDragging()).toBe(true);
  expect(peekFacade(DOC)!.transientTransformActive()).toBe(true);
  ctx.reset();
  for (let i = 1; i <= frames; i++) {
    pointermove(ctx, 150 + i * 10, 150 + i * 5);
    expect(ctx.commands()).toEqual([]);
  }
}

describe("canvas layer drag on native-owned layers", () => {
  it("pointermove writes the renderer's preview channel with zero protocol calls", async () => {
    const ctx = await setup({ owned: 1 });
    try {
      const id = ctx.owned[0];
      const before = transformOf(ctx, id);
      pointerdown(ctx, 150, 150);
      ctx.reset();

      pointermove(ctx, 190, 200);
      // The model is untouched mid-gesture, so it cannot be what the user sees.
      expect(transformOf(ctx, id)).toEqual(before);
      // This is the signal EditorShell's render scheduler merges into the outgoing
      // RenderState (applyFacadePreviews), i.e. the pixels during the drag.
      expect(transformPreview()).toEqual([
        { layerId: id, transform: { ...before, x: before.x + 40, y: before.y + 50 } },
      ] satisfies FacadeTransformPreview[]);
      expect(ctx.commands()).toEqual([]);

      pointermove(ctx, 230, 250);
      expect(transformPreview()[0]?.transform).toMatchObject({ x: before.x + 80, y: before.y + 100 });

      await settle();
      // Including anything the two frames may have queued: still zero.
      expect(ctx.commands()).toEqual([]);

      pointerup(ctx, 230, 250);
      await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer"]));
      expect(transformOf(ctx, id)).toEqual({ ...before, x: before.x + 80, y: before.y + 100 });
      expect(transformPreview()).toEqual([]);
    } finally {
      ctx.dispose();
    }
  });

  it("a full gesture dispatches EXACTLY ONE command and gains no legacy history entry", async () => {
    const ctx = await setup({ owned: 1 });
    try {
      const id = ctx.owned[0];
      const history = ctx.ws.getHistory(DOC)!;
      const before = transformOf(ctx, id);
      const undoBefore = history.getUndoCount();
      pointerdown(ctx, 150, 150);
      ctx.reset();

      for (let i = 1; i <= 12; i++) {
        pointermove(ctx, 150 + i, 150 + i * 2);
        expect(ctx.commands()).toEqual([]);
      }
      await settle();
      expect(ctx.commands()).toEqual([]);

      pointerup(ctx, 162, 174);
      // Hard invariant: the commit is the whole gesture's only dispatch.
      await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer"]));
      expect(transformOf(ctx, id)).toEqual({ ...before, x: before.x + 12, y: before.y + 24 });
      expect(history.getUndoCount()).toBe(undoBefore);
      expect(peekFacade(DOC)!.transientTransformActive()).toBe(false);
    } finally {
      ctx.dispose();
    }
  });

  it("a click without movement dispatches nothing and writes no history entry", async () => {
    const ctx = await setup({ owned: 1 });
    try {
      const id = ctx.owned[0];
      const history = ctx.ws.getHistory(DOC)!;
      const undoBefore = history.getUndoCount();
      const before = transformOf(ctx, id);
      pointerdown(ctx, 150, 150);
      ctx.reset();
      pointerup(ctx, 150, 150);
      await settle();
      expect(ctx.commands()).toEqual([]);
      expect(transformOf(ctx, id)).toEqual(before);
      expect(history.getUndoCount()).toBe(undoBefore);
    } finally {
      ctx.dispose();
    }
  });

  it("flag OFF: the same chain still mutates the model synchronously per frame and writes its history entry", async () => {
    const ctx = await setup({ owned: 1, flag: false });
    try {
      const id = ctx.owned[0];
      const history = ctx.ws.getHistory(DOC)!;
      const undoBefore = history.getUndoCount();
      pointerdown(ctx, 150, 150);
      ctx.reset();

      pointermove(ctx, 190, 200);
      // Synchronous, no await: the legacy gesture must not gain a microtask hop.
      expect(transformOf(ctx, id).x).toBe(140);
      expect(transformOf(ctx, id).y).toBe(150);
      pointermove(ctx, 230, 250);
      expect(transformOf(ctx, id).x).toBe(180);
      expect(transformPreview()).toEqual([]);
      expect(ctx.commands()).toEqual([]);

      pointerup(ctx, 230, 250);
      expect(transformOf(ctx, id).x).toBe(180);
      expect(history.getUndoCount()).toBe(undoBefore + 1);
      expect(history.getHistoryStack().some((h) => h.label === "Move Layer")).toBe(true);
    } finally {
      ctx.dispose();
    }
  });

  it("mid-drag Escape dispatches nothing, leaves the model at the pre-drag value, and clears the preview", async () => {
    const ctx = await setup({ owned: 1 });
    try {
      const id = ctx.owned[0];
      const before = transformOf(ctx, id);
      await beginRoutedDrag(ctx);
      expect(transformPreview().length).toBe(1);

      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));

      expect(ctx.dragApi.isDragging()).toBe(false);
      expect(ctx.commands()).toEqual([]);
      expect(transformOf(ctx, id)).toEqual(before);
      expect(transformPreview()).toEqual([]);
      expect(ctx.onHudUpdate).toHaveBeenLastCalledWith(null);
    } finally {
      ctx.dispose();
    }
  });

  it.each((["pointercancel", "escape", "lostpointercapture", "layer-identity-change", "no-active-engine", "unmount"] as Exit[]).map((exit) => ({ exit })))(
    "abandoning the drag through $exit releases the slot and a later numeric commit still lands",
    async ({ exit }) => {
      const ctx = await setup({ owned: 2 });
      try {
        const id = ctx.owned[0];
        const before = transformOf(ctx, id);
        await beginRoutedDrag(ctx);
        await abandon(ctx, exit);
        if (exit === "layer-identity-change") {
          // The gesture's own layer is what disappeared, so there is no post-drag
          // transform to compare; abandon() already proved nothing dispatched and the
          // slot is free.
          expect(ctx.engine.getLayer(id)).toBeUndefined();
        } else {
          // The abandoned gesture applied nothing: its layer stayed where it was.
          expect(transformOf(ctx, id)).toEqual(before);
        }
      } finally {
        ctx.dispose();
      }
    },
  );

  it("pointerup commits once and leaves the slot free for the next edit", async () => {
    const ctx = await setup({ owned: 2 });
    try {
      await beginRoutedDrag(ctx);
      pointerup(ctx, 180, 165);
      await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer"]));
      expect(transformOf(ctx, ctx.owned[0]).x).toBe(130);
      expect(peekFacade(DOC)!.transientTransformActive()).toBe(false);
      await expect(facadeCommitNumericTransform(ctx.engine, ctx.owned[0], { x: 7 })).resolves.toBe(true);
    } finally {
      ctx.dispose();
    }
  });

  it("multi-layer: every owned layer commits once, none is applied partially", async () => {
    const ctx = await setup({ owned: 2 });
    try {
      const [a, b] = ctx.owned;
      const beforeA = transformOf(ctx, a);
      const beforeB = transformOf(ctx, b);
      ctx.setSelectedLayerIds([a, b]);
      pointerdown(ctx, 150, 150);
      ctx.reset();

      pointermove(ctx, 170, 160);
      expect(transformPreview().map((p) => p.layerId).sort()).toEqual([a, b].sort());
      expect(transformOf(ctx, a)).toEqual(beforeA);
      expect(transformOf(ctx, b)).toEqual(beforeB);
      expect(ctx.commands()).toEqual([]);
      await settle();
      expect(ctx.commands()).toEqual([]);

      pointerup(ctx, 170, 160);
      // One command per owned layer, and neither lands without the other.
      await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer", "transformLayer"]));
      expect(transformOf(ctx, a)).toEqual({ ...beforeA, x: beforeA.x + 20, y: beforeA.y + 10 });
      expect(transformOf(ctx, b)).toEqual({ ...beforeB, x: beforeB.x + 20, y: beforeB.y + 10 });
    } finally {
      ctx.dispose();
    }
  });

  it("mixed ownership: the gesture is refused atomically with a toast, and nothing moves", async () => {
    const ctx = await setup({ owned: 1, legacy: true });
    try {
      const [a] = ctx.owned;
      const before = transformOf(ctx, a);
      const legacyBefore = transformOf(ctx, ctx.legacyId);
      ctx.setSelectedLayerIds([a, ctx.legacyId]);

      pointerdown(ctx, 150, 150);
      pointermove(ctx, 190, 200);

      expect(ctx.dragApi.isDragging()).toBe(false);
      expect(showToast).toHaveBeenCalledWith(expect.stringContaining("mixed selection"), "error");
      expect(ctx.commands()).toEqual([]);
      expect(transformOf(ctx, a)).toEqual(before);
      expect(transformOf(ctx, ctx.legacyId)).toEqual(legacyBefore);
      expect(peekFacade(DOC)!.transientTransformActive()).toBe(false);
    } finally {
      ctx.dispose();
    }
  });

  it("a numeric edit issued during the drag is a toast, and the drag still commits", async () => {
    const ctx = await setup({ owned: 2 });
    try {
      const [dragged, other] = ctx.owned;
      const before = transformOf(ctx, dragged);
      await beginRoutedDrag(ctx);

      const status = await routeNumericTransformBatch(
        ctx.engine,
        [{ layerId: other, patch: { x: 999 } }],
        { requestRender: () => {}, notifyVisualChange: () => {} },
      );
      expect(status).toBe("error");
      expect(showToast).toHaveBeenCalledWith(
        expect.stringContaining("transform gesture is in progress"),
        "error",
      );
      // The refusal is what keeps both edits alive: nothing was applied mid-gesture.
      expect(transformOf(ctx, other).x).not.toBe(999);

      pointerup(ctx, 180, 165);
      await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer"]));
      expect(transformOf(ctx, dragged).x).toBe(before.x + 30);
    } finally {
      ctx.dispose();
    }
  });

  it("document close during the drag does not resurrect the facade it evicted", async () => {
    const ctx = await setup({ owned: 1 });
    try {
      await beginRoutedDrag(ctx);
      removeFacade(DOC);
      pointercancel(ctx);
      expect(peekFacade(DOC)).toBeUndefined();
      expect(transformPreview()).toEqual([]);
    } finally {
      ctx.dispose();
    }
  });

  it("a second gesture leaves no orphan preview and no held slot", async () => {
    const ctx = await setup({ owned: 1 });
    try {
      await beginRoutedDrag(ctx);
      pointerup(ctx, 180, 165);
      await vi.waitFor(() => expect(transformOf(ctx, ctx.owned[0]).x).toBe(130));
      expect(transformPreview()).toEqual([]);
      expect(peekFacade(DOC)!.transientTransformActive()).toBe(false);

      const before = transformOf(ctx, ctx.owned[0]);
      pointerdown(ctx, 180, 180);
      ctx.reset();
      pointermove(ctx, 200, 200);
      expect(transformPreview()).toEqual([
        { layerId: ctx.owned[0], transform: { ...before, x: before.x + 20, y: before.y + 20 } },
      ]);
      pointerup(ctx, 200, 200);
      await vi.waitFor(() => expect(transformOf(ctx, ctx.owned[0]).x).toBe(before.x + 20));
      expect(transformPreview()).toEqual([]);
    } finally {
      ctx.dispose();
    }
  });
});
