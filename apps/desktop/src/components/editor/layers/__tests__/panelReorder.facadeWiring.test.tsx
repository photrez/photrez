// Call-site wiring for the in-panel drag reorder. The same-document drop must reach
// the Reorder funnel the menu / keyboard reorder actions already use, so the two entry
// points cannot drift.
//
// Mirrors metadataOps.facadeWiring.test.tsx for the harness (facadeRegistry mocked so
// the funnel is a spy, the flag is mutable, ownership is forced) and
// LayersPanel.reorder.e2e.test.tsx for the gesture (real LayersPanel + real drop-zone
// DOM events). The panel's own drop handler is what is driven here, never the funnel.

import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider } from "../../shell/EditorContext";
import { LayersPanel } from "../LayersPanel";
import { DragControllerProvider, useDragController } from "../../DragController";
import { WorkspaceManager } from "@/engine/workspace";
import type { LayerDragPayload } from "../../dragTypes";
import * as Toast from "../../Toast";

const h = vi.hoisted(() => ({
  facadeOn: true,
  owned: true,
  commitFacadeReorder: vi.fn(
    (): Promise<{ status: string; count?: number }> => Promise.resolve({ status: "applied", count: 1 }),
  ),
}));

vi.mock("@/lib/protocol/facadeRegistry", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isFacadeEnabled: () => h.facadeOn,
  commitFacadeReorder: h.commitFacadeReorder,
  MIXED_OWNERSHIP_MESSAGE: "mixed-selection",
}));

vi.mock("@/engine/document", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isFacadeOwnedLayer: () => h.owned,
}));

const ROW_H = 50;
const DOC_ID = "panel-reorder-doc";

function setup() {
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(DOC_ID, "Panel", 400, 300);
  ws.addDocument(session);
  session.engine.addLayer("Bottom");
  session.engine.addLayer("Middle");
  session.engine.addLayer("Top");

  const scheduler = { requestRender: vi.fn() };
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);

  let probe: ReturnType<typeof useDragController> | null = null;
  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as never} scheduler={scheduler as never}>
        <DragControllerProvider>
          <LayersPanel />
          {(() => {
            probe = useDragController();
            return null;
          })()}
        </DragControllerProvider>
      </EditorProvider>
    ),
    container,
  );

  // computeInsertionHint reads row rects; pin a deterministic 50px stack. Re-run
  // after any re-render (row elements are recreated).
  const stubRows = () => {
    container.querySelectorAll<HTMLElement>("[data-layer-idx]").forEach((row, i) => {
      const top = i * ROW_H;
      row.getBoundingClientRect = () =>
        ({
          x: 0,
          y: top,
          left: 0,
          top,
          right: 200,
          bottom: top + ROW_H,
          width: 200,
          height: ROW_H,
          toJSON: () => ({}),
        }) as DOMRect;
    });
  };
  stubRows();

  const fire = (type: string, clientY: number) => {
    const dz = container.querySelector<HTMLElement>("[data-layers-panel-drop-zone]")!;
    const evt = new Event(type, { bubbles: true, cancelable: true }) as any;
    evt.clientY = clientY;
    evt.clientX = 5;
    evt.dataTransfer = { setData: () => {}, types: [] };
    dz.dispatchEvent(evt);
  };

  const payloadFor = (name: string): LayerDragPayload => {
    const layer = session.engine.getLayers().find((l) => l.name === name)!;
    return {
      version: 1,
      sourceDocId: DOC_ID,
      layerId: layer.id,
      sourceName: layer.name,
      isAltPressed: false,
    };
  };

  const order = () => session.engine.getLayers().map((l) => l.name);
  const tick = () => new Promise((r) => setTimeout(r, 0));

  return { ws, session, probe: () => probe!, fire, payloadFor, order, stubRows, tick, dispose };
}

describe("in-panel drag reorder call-site routing", () => {
  afterEach(() => {
    document.body.replaceChildren();
    vi.restoreAllMocks();
    h.facadeOn = true;
    h.owned = true;
    h.commitFacadeReorder.mockReset();
    h.commitFacadeReorder.mockImplementation(() => Promise.resolve({ status: "applied", count: 1 }));
  });

  it("flag ON + owned layer: the drop routes to commitFacadeReorder(id, toIndex); legacy engine.reorderLayer and TS history stay untouched", async () => {
    const ctx = setup();
    try {
      const reorderSpy = vi.spyOn(ctx.session.engine, "reorderLayer");
      const historySpy = vi.spyOn(ctx.session.history, "commit");
      const before = ctx.session.history.getUndoCount();
      const topId = ctx.session.engine.getLayers()[0].id;

      // Top (idx 0) dropped on the lower half of the Bottom row (idx 2):
      // insertAt=2 below, source above the hint -> lands at index 2.
      ctx.probe().beginLayerDrag(ctx.payloadFor("Top"), null);
      ctx.fire("dragover", 140);
      expect(ctx.probe().state().dropTarget).toEqual({ type: "layers-panel", insertAt: 2, insertPosition: "below" });
      ctx.fire("drop", 140);
      await ctx.tick();

      expect(h.commitFacadeReorder).toHaveBeenCalledTimes(1);
      expect(h.commitFacadeReorder).toHaveBeenCalledWith(expect.any(Object), topId, 2);
      expect(reorderSpy).not.toHaveBeenCalled();
      expect(historySpy).not.toHaveBeenCalled();
      expect(ctx.session.history.getUndoCount()).toBe(before);
      // The drop site must not also apply the order itself: the routed command
      // owns the mutation, so nothing has moved synchronously here.
      expect(ctx.order()).toEqual(["Top", "Middle", "Bottom", "Background"]);
    } finally {
      ctx.dispose();
    }
  });

  it("flag OFF: legacy engine.reorderLayer runs synchronously inside the handler, one history entry, drop position honored", () => {
    h.facadeOn = false;
    const ctx = setup();
    try {
      const reorderSpy = vi.spyOn(ctx.session.engine, "reorderLayer");
      const historySpy = vi.spyOn(ctx.session.history, "commit");

      ctx.probe().beginLayerDrag(ctx.payloadFor("Top"), null);
      ctx.fire("dragover", 140);
      ctx.fire("drop", 140);

      // No await between dispatchEvent and these assertions: the flag-off path
      // must not push the legacy commit past the event handler.
      expect(h.commitFacadeReorder).not.toHaveBeenCalled();
      expect(reorderSpy).toHaveBeenCalledTimes(1);
      expect(reorderSpy).toHaveBeenCalledWith(0, 2);
      expect(historySpy).toHaveBeenCalledTimes(1);
      expect(ctx.order()).toEqual(["Middle", "Bottom", "Top", "Background"]);
    } finally {
      ctx.dispose();
    }
  });

  it("flag ON but the layer is not facade-owned: legacy path, zero funnel calls", async () => {
    h.owned = false;
    const ctx = setup();
    try {
      const reorderSpy = vi.spyOn(ctx.session.engine, "reorderLayer");
      ctx.probe().beginLayerDrag(ctx.payloadFor("Top"), null);
      ctx.fire("dragover", 140);
      ctx.fire("drop", 140);
      await ctx.tick();

      expect(h.commitFacadeReorder).not.toHaveBeenCalled();
      expect(reorderSpy).toHaveBeenCalledTimes(1);
      expect(ctx.order()).toEqual(["Middle", "Bottom", "Top", "Background"]);
    } finally {
      ctx.dispose();
    }
  });

  it("funnel falls back to legacy: one reorderLayer + one history entry after the route says legacy", async () => {
    h.commitFacadeReorder.mockImplementation(() => Promise.resolve({ status: "legacy" }));
    const ctx = setup();
    try {
      const reorderSpy = vi.spyOn(ctx.session.engine, "reorderLayer");
      ctx.probe().beginLayerDrag(ctx.payloadFor("Top"), null);
      ctx.fire("dragover", 140);
      ctx.fire("drop", 140);
      await ctx.tick();

      expect(h.commitFacadeReorder).toHaveBeenCalledTimes(1);
      expect(reorderSpy).toHaveBeenCalledTimes(1);
      expect(reorderSpy).toHaveBeenCalledWith(0, 2);
      expect(ctx.order()).toEqual(["Middle", "Bottom", "Top", "Background"]);
    } finally {
      ctx.dispose();
    }
  });

  it("mixed ownership: refuse the whole gesture with the existing toast, move nothing", async () => {
    h.commitFacadeReorder.mockImplementation(() => Promise.resolve({ status: "mixed-rejected" }));
    const toastSpy = vi.spyOn(Toast, "showToast");
    const ctx = setup();
    try {
      const reorderSpy = vi.spyOn(ctx.session.engine, "reorderLayer");
      const historySpy = vi.spyOn(ctx.session.history, "commit");
      const before = ctx.session.history.getUndoCount();

      ctx.probe().beginLayerDrag(ctx.payloadFor("Top"), null);
      ctx.fire("dragover", 140);
      ctx.fire("drop", 140);
      await ctx.tick();

      expect(toastSpy).toHaveBeenCalledWith("mixed-selection", "error");
      expect(reorderSpy).not.toHaveBeenCalled();
      expect(historySpy).not.toHaveBeenCalled();
      expect(ctx.session.history.getUndoCount()).toBe(before);
      expect(ctx.order()).toEqual(["Top", "Middle", "Bottom", "Background"]);
    } finally {
      ctx.dispose();
    }
  });

  it("a rejected funnel surfaces one error toast and leaves the order alone", async () => {
    h.commitFacadeReorder.mockImplementation(() => Promise.reject(new Error("E_EXTERNAL_PENDING")));
    const toastSpy = vi.spyOn(Toast, "showToast");
    const ctx = setup();
    try {
      const reorderSpy = vi.spyOn(ctx.session.engine, "reorderLayer");
      ctx.probe().beginLayerDrag(ctx.payloadFor("Top"), null);
      ctx.fire("dragover", 140);
      ctx.fire("drop", 140);
      await ctx.tick();

      expect(toastSpy).toHaveBeenCalledWith(expect.stringContaining("E_EXTERNAL_PENDING"), "error");
      expect(reorderSpy).not.toHaveBeenCalled();
      expect(ctx.order()).toEqual(["Top", "Middle", "Bottom", "Background"]);
    } finally {
      ctx.dispose();
    }
  });

  it("no-op drop (same row): routed path dispatches nothing and writes no history entry", async () => {
    const ctx = setup();
    try {
      const reorderSpy = vi.spyOn(ctx.session.engine, "reorderLayer");
      const historySpy = vi.spyOn(ctx.session.history, "commit");
      const before = ctx.session.history.getUndoCount();

      // Top (idx 0) dropped on the upper half of Middle (insertAt=1, above)
      // resolves back to index 0 - the gesture is a no-op.
      ctx.probe().beginLayerDrag(ctx.payloadFor("Top"), null);
      ctx.fire("dragover", 60);
      ctx.fire("drop", 60);
      await ctx.tick();

      expect(h.commitFacadeReorder).not.toHaveBeenCalled();
      expect(reorderSpy).not.toHaveBeenCalled();
      expect(historySpy).not.toHaveBeenCalled();
      expect(ctx.session.history.getUndoCount()).toBe(before);
      expect(ctx.order()).toEqual(["Top", "Middle", "Bottom", "Background"]);
    } finally {
      ctx.dispose();
    }
  });

  it("no-op drop, flag OFF: legacy moves nothing (its undo point is still taken pre-check)", () => {
    h.facadeOn = false;
    const ctx = setup();
    try {
      const reorderSpy = vi.spyOn(ctx.session.engine, "reorderLayer");
      const historySpy = vi.spyOn(ctx.session.history, "commit");
      const before = ctx.session.history.getUndoCount();

      ctx.probe().beginLayerDrag(ctx.payloadFor("Top"), null);
      ctx.fire("dragover", 60);
      ctx.fire("drop", 60);

      // Pinned deliberately: the pre-facade handler commits BEFORE the
      // targetIdx === sourceIdx check, so an unchanged drop still records an
      // undo entry. Flag-off byte identity keeps it; the routed path does not.
      expect(reorderSpy).not.toHaveBeenCalled();
      expect(historySpy).toHaveBeenCalledTimes(1);
      expect(ctx.session.history.getUndoCount()).toBe(before + 1);
      expect(ctx.order()).toEqual(["Top", "Middle", "Bottom", "Background"]);
    } finally {
      ctx.dispose();
    }
  });

  it("drop with no tracked insertion hint (missing insertAt) routes to the end of the stack", async () => {
    const ctx = setup();
    try {
      const reorderSpy = vi.spyOn(ctx.session.engine, "reorderLayer");
      const topId = ctx.session.engine.getLayers()[0].id;

      ctx.probe().beginLayerDrag(ctx.payloadFor("Top"), null);
      // No dragover: the handler must cope with a bare layers-panel target.
      ctx.probe().setDropTarget({ type: "layers-panel" });
      ctx.fire("drop", 140);
      await ctx.tick();

      // Bare hint falls back to "end of the stack" (the pre-existing contract).
      expect(h.commitFacadeReorder).toHaveBeenCalledTimes(1);
      expect(h.commitFacadeReorder).toHaveBeenCalledWith(expect.any(Object), topId, 3);
      expect(reorderSpy).not.toHaveBeenCalled();
    } finally {
      ctx.dispose();
    }
  });

  it("round trip: reorder down then back up restores the original order (flag OFF, legacy math)", () => {
    h.facadeOn = false;
    const ctx = setup();
    try {
      const reorderSpy = vi.spyOn(ctx.session.engine, "reorderLayer");
      const original = ctx.order();

      ctx.probe().beginLayerDrag(ctx.payloadFor("Top"), null);
      ctx.fire("dragover", 140);
      ctx.fire("drop", 140);
      expect(ctx.order()).toEqual(["Middle", "Bottom", "Top", "Background"]);

      // Row elements are recreated by the re-render; re-pin their rects.
      ctx.stubRows();
      ctx.probe().beginLayerDrag(ctx.payloadFor("Top"), null);
      // Top now sits at idx 2: drop on the upper half of Middle (idx 0).
      ctx.fire("dragover", 10);
      ctx.fire("drop", 10);

      expect(reorderSpy).toHaveBeenLastCalledWith(2, 0);
      expect(ctx.order()).toEqual(original);
    } finally {
      ctx.dispose();
    }
  });

  it("round trip: reorder down then back up dispatches the mirrored index (flag ON)", async () => {
    const ctx = setup();
    try {
      const topId = ctx.session.engine.getLayers()[0].id;
      ctx.probe().beginLayerDrag(ctx.payloadFor("Top"), null);
      ctx.fire("dragover", 140);
      ctx.fire("drop", 140);
      await ctx.tick();

      // Projection stand-in for the routed arm's ordered restatement, so the
      // second gesture is measured from the reordered stack like the real one.
      ctx.session.engine.reorderLayer(0, 2);
      ctx.stubRows();
      h.commitFacadeReorder.mockClear();

      ctx.probe().beginLayerDrag(ctx.payloadFor("Top"), null);
      ctx.fire("dragover", 10);
      ctx.fire("drop", 10);
      await ctx.tick();

      expect(h.commitFacadeReorder).toHaveBeenCalledTimes(1);
      expect(h.commitFacadeReorder).toHaveBeenCalledWith(expect.any(Object), topId, 0);
      ctx.session.engine.reorderLayer(2, 0);
      expect(ctx.order()).toEqual(["Top", "Middle", "Bottom", "Background"]);
    } finally {
      ctx.dispose();
    }
  });
});
