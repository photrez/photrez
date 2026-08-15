import { describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider } from "../../shell/EditorContext";
import { LayersPanel } from "../LayersPanel";
import { DragControllerProvider, useDragController } from "../../DragController";
import { WorkspaceManager } from "@/engine/workspace";
import type { LayerDragPayload } from "../../dragTypes";

/**
 * End-to-end smoke test for the in-panel layer reorder fix.
 * The panel's drop handler must forward the tracked dropTarget (with
 * insertAt + insertPosition) to addLayerFromCrossDoc so the
 * same-doc reorder lands at the row the user aimed at.
 */
describe("LayersPanel →in-panel reorder end-to-end", () => {
  function setupWithMockedRects(rectsByIndex: Array<{ top: number; height: number }>) {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("reorder-doc", "Reorder", 400, 300);
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
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
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

    // computeInsertionHint iterates row rects to decide which row the
    // pointer is over. Stub each row's getBoundingClientRect to
    // produce a known layout so the test is deterministic.
    const rows = container.querySelectorAll<HTMLElement>("[data-layer-idx]");
    rows.forEach((row, i) => {
      const r = rectsByIndex[i] ?? { top: i * 50, height: 50 };
      row.getBoundingClientRect = () =>
        ({
          x: 0,
          y: r.top,
          left: 0,
          top: r.top,
          right: 200,
          bottom: r.top + r.height,
          width: 200,
          height: r.height,
          toJSON: () => ({}),
        }) as DOMRect;
    });

    return {
      ws,
      container,
      rows,
      probe: () => probe!,
      dispose,
    };
  }

  function fireDragEvent(
    target: Element,
    type: string,
    init: { clientY: number; clientX: number },
  ) {
    const dt = { setData: () => {}, types: [] } as any;
    const evt = new Event(type, { bubbles: true, cancelable: true }) as any;
    evt.clientY = init.clientY;
    evt.clientX = init.clientX;
    evt.dataTransfer = dt;
    target.dispatchEvent(evt);
    return dt;
  }

  function getPayload(ws: WorkspaceManager, sourceIdx: number): LayerDragPayload {
    const layer = ws.getActiveEngine()!.getLayers()[sourceIdx];
    return {
      version: 1,
      sourceDocId: "reorder-doc",
      layerId: layer.id,
      sourceName: layer.name,
      isAltPressed: false,
    };
  }

  it("reorders via tracked dropTarget with insertion above (Top →above Middle)", () => {
    // Layer order: [Top, Middle, Bottom, Background] (indices 0..3).
    // Each row 50px tall, stacked: row 0 = 0..50, row 1 = 50..100, etc.
    const ctx = setupWithMockedRects([
      { top: 0, height: 50 },
      { top: 50, height: 50 },
      { top: 100, height: 50 },
      { top: 150, height: 50 },
    ]);

    try {
      const initialOrder = ctx.ws.getActiveEngine()!.getLayers().map((l) => l.name);
      expect(initialOrder).toEqual(["Top", "Middle", "Bottom", "Background"]);

      const panelDz = ctx.container.querySelector<HTMLElement>("[data-layers-panel-drop-zone]")!;
      ctx.probe().beginLayerDrag(getPayload(ctx.ws, 0), null);

      // Dragover at upper-half of Middle row (y=60) →insertAt=1, above.
      fireDragEvent(panelDz, "dragover", { clientY: 60, clientX: 5 });
      const dt = ctx.probe().state().dropTarget;
      expect(dt?.type).toBe("layers-panel");
      if (dt?.type === "layers-panel") {
        expect(dt.insertAt).toBe(1);
        expect(dt.insertPosition).toBe("above");
      }

      fireDragEvent(panelDz, "drop", { clientY: 60, clientX: 5 });

      // Top (sourceIdx=0) above Middle (insertAt=1, above):
      // sourceIdx < insertAt, position=above →targetIdx = 0.
      // reorderLayer(0, 0) is a no-op (targetIdx === sourceIdx) →no-op.
      const afterOrder = ctx.ws.getActiveEngine()!.getLayers().map((l) => l.name);
      expect(afterOrder).toEqual(["Top", "Middle", "Bottom", "Background"]);
    } finally {
      ctx.dispose();
      document.body.replaceChildren();
    }
  });

  it("reorders via tracked dropTarget with insertion below (Top →below Bottom)", () => {
    const ctx = setupWithMockedRects([
      { top: 0, height: 50 },
      { top: 50, height: 50 },
      { top: 100, height: 50 },
      { top: 150, height: 50 },
    ]);

    try {
      const panelDz = ctx.container.querySelector<HTMLElement>("[data-layers-panel-drop-zone]")!;
      ctx.probe().beginLayerDrag(getPayload(ctx.ws, 0), null);

      // Dragover at lower-half of Bottom row (y=140) →insertAt=2, below.
      fireDragEvent(panelDz, "dragover", { clientY: 140, clientX: 5 });
      const dt = ctx.probe().state().dropTarget;
      expect(dt?.type).toBe("layers-panel");
      if (dt?.type === "layers-panel") {
        expect(dt.insertAt).toBe(2);
        expect(dt.insertPosition).toBe("below");
      }

      fireDragEvent(panelDz, "drop", { clientY: 140, clientX: 5 });

      // Top (sourceIdx=0) below Bottom (insertAt=2, below):
      // sourceIdx < insertAt, position=below →targetIdx = insertAt = 2.
      // reorderLayer(0, 2) →splice out index 0, insert at index 2
      // →[Middle, Bottom, Top, Background].
      const afterOrder = ctx.ws.getActiveEngine()!.getLayers().map((l) => l.name);
      expect(afterOrder).toEqual(["Middle", "Bottom", "Top", "Background"]);
    } finally {
      ctx.dispose();
      document.body.replaceChildren();
    }
  });

  it("reorders correctly when source sits after the insertion target", () => {
    const ctx = setupWithMockedRects([
      { top: 0, height: 50 },
      { top: 50, height: 50 },
      { top: 100, height: 50 },
      { top: 150, height: 50 },
    ]);

    try {
      const panelDz = ctx.container.querySelector<HTMLElement>("[data-layers-panel-drop-zone]")!;
      // Drag Middle (sourceIdx=1) above Top (insertAt=0, above).
      ctx.probe().beginLayerDrag(getPayload(ctx.ws, 1), null);
      fireDragEvent(panelDz, "dragover", { clientY: 10, clientX: 5 });
      fireDragEvent(panelDz, "drop", { clientY: 10, clientX: 5 });

      // Middle (sourceIdx=1) above Top (insertAt=0, above):
      // sourceIdx > insertAt, position=above →targetIdx = insertAt = 0.
      // reorderLayer(1, 0) →splice out index 1, insert at 0
      // →[Middle, Top, Bottom, Background].
      const afterOrder = ctx.ws.getActiveEngine()!.getLayers().map((l) => l.name);
      expect(afterOrder).toEqual(["Middle", "Top", "Bottom", "Background"]);
    } finally {
      ctx.dispose();
      document.body.replaceChildren();
    }
  });

  it("places source at top when dropped above the first row (regression: was routing to bottom)", () => {
    const ctx = setupWithMockedRects([
      { top: 0, height: 50 },
      { top: 50, height: 50 },
      { top: 100, height: 50 },
      { top: 150, height: 50 },
    ]);

    try {
      const panelDz = ctx.container.querySelector<HTMLElement>("[data-layers-panel-drop-zone]")!;
      // Pointer above the first row →insertAt=0, above. Before the
      // fix this fell through the loop and landed at lastIdx/below.
      ctx.probe().beginLayerDrag(getPayload(ctx.ws, 0), null);
      fireDragEvent(panelDz, "dragover", { clientY: -10, clientX: 5 });
      const dt = ctx.probe().state().dropTarget;
      expect(dt?.type).toBe("layers-panel");
      if (dt?.type === "layers-panel") {
        expect(dt.insertAt).toBe(0);
        expect(dt.insertPosition).toBe("above");
      }
    } finally {
      ctx.dispose();
      document.body.replaceChildren();
    }
  });

  it("never targets a drop below the Background (clamps hint to above bg)", () => {
    const ctx = setupWithMockedRects([
      { top: 0, height: 50 },
      { top: 50, height: 50 },
      { top: 100, height: 50 },
      { top: 150, height: 50 },
    ]);

    try {
      const panelDz = ctx.container.querySelector<HTMLElement>("[data-layers-panel-drop-zone]")!;
      // Dragover at the lower half of the Background row (y=175) would be
      // "below" the Background — clamp to "above" it instead.
      ctx.probe().beginLayerDrag(getPayload(ctx.ws, 0), null);
      fireDragEvent(panelDz, "dragover", { clientY: 175, clientX: 5 });
      const dt = ctx.probe().state().dropTarget;
      expect(dt?.type).toBe("layers-panel");
      if (dt?.type === "layers-panel") {
        expect(dt.insertAt).toBe(3);
        expect(dt.insertPosition).toBe("above");
      }

      fireDragEvent(panelDz, "drop", { clientY: 175, clientX: 5 });

      // Top lands just above the Background — a real, visible move.
      const afterOrder = ctx.ws.getActiveEngine()!.getLayers().map((l) => l.name);
      expect(afterOrder).toEqual(["Middle", "Bottom", "Top", "Background"]);
    } finally {
      ctx.dispose();
      document.body.replaceChildren();
    }
  });

  it("disables Move Down when a layer rests on the Background", () => {
    const ctx = setupWithMockedRects([
      { top: 0, height: 50 },
      { top: 50, height: 50 },
      { top: 100, height: 50 },
      { top: 150, height: 50 },
    ]);
    try {
      const chevrons = (rowIdx: number) => {
        // Chevrons live in the opacity-0 hover container, independent of
        // the lock button's DOM position.
        const chevronContainer = ctx.rows[rowIdx].querySelector(".opacity-0");
        const btns = Array.from(chevronContainer?.querySelectorAll<HTMLButtonElement>("button") ?? []);
        return { up: btns[0]!, down: btns[1]! };
      };
      // Bottom (idx 2, resting on bg) and Background (idx 3) → Move Down disabled.
      expect(chevrons(2).down.disabled).toBe(true);
      expect(chevrons(3).down.disabled).toBe(true);
      // Middle (idx 1) and Top (idx 0) can still move down.
      expect(chevrons(1).down.disabled).toBe(false);
      expect(chevrons(0).down.disabled).toBe(false);
      // Top (idx 0) and Background (idx 3) cannot move up.
      expect(chevrons(0).up.disabled).toBe(true);
      expect(chevrons(3).up.disabled).toBe(true);
      expect(chevrons(1).up.disabled).toBe(false);
    } finally {
      ctx.dispose();
      document.body.replaceChildren();
    }
  });
});

function fireNativeDragStart(
  source: Element,
  target: Element,
  type: string,
  init: { clientY: number; clientX: number },
) {
  const dt = { setData: () => {}, types: [] } as any;
  const evt = new Event(type, { bubbles: true, cancelable: true }) as any;
  evt.clientY = init.clientY;
  evt.clientX = init.clientX;
  evt.dataTransfer = dt;
  source.dispatchEvent(evt);
  return dt;
}

/**
 * Fire the actual DOM dragstart on a LayerItem row and let
 * its real onLayerDragStart handler run. This catches bugs in the
 * real production path that synthetic beginLayerDrag calls miss.
 */
describe("LayersPanel →real onLayerDragStart path", () => {
  function setupWithMockedRects() {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("reorder-doc", "Reorder", 400, 300);
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
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
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

    // Stub getBoundingClientRect on each row.
    const rows = container.querySelectorAll<HTMLElement>("[data-layer-idx]");
    rows.forEach((row, i) => {
      const top = i * 50;
      row.getBoundingClientRect = () =>
        ({
          x: 0,
          y: top,
          left: 0,
          top,
          right: 200,
          bottom: top + 50,
          width: 200,
          height: 50,
          toJSON: () => ({}),
        }) as DOMRect;
    });

    return { ws, container, rows, probe: () => probe!, dispose };
  }

  it("fires LayerItem pointer drag and triggers the panel drop handler end-to-end", () => {
    const ctx = setupWithMockedRects();
    try {
      const initialOrder = ctx.ws.getActiveEngine()!.getLayers().map((l) => l.name);
      expect(initialOrder).toEqual(["Top", "Middle", "Bottom", "Background"]);

      const topRow = ctx.rows[0]; // Top layer
      const bottomRow = ctx.rows[2]; // Bottom layer

      // 1. Pointerdown on the Top row and move past threshold to initiate drag.
      topRow.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 10, clientY: 10, button: 0 }));
      topRow.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true,
        clientX: 10,
        clientY: bottomRow.getBoundingClientRect().top + 35,
      }));

      // After drag start and move, dragController should be in "layer" mode with dropTarget set.
      const afterStart = ctx.probe().state();
      expect(afterStart.dragKind).toBe("layer");
      expect(afterStart.payload?.sourceName).toBe("Top");

      const dt = ctx.probe().state().dropTarget;
      expect(dt?.type).toBe("layers-panel");
      if (dt?.type === "layers-panel") {
        expect(dt.insertAt).toBe(2);
        expect(dt.insertPosition).toBe("below");
      }

      // 2. Pointerup to complete the drop.
      topRow.dispatchEvent(new PointerEvent("pointerup", {
        bubbles: true,
        clientX: 10,
        clientY: bottomRow.getBoundingClientRect().top + 35,
      }));

      // 3. Verify the engine layer order changed.
      const afterOrder = ctx.ws.getActiveEngine()!.getLayers().map((l) => l.name);
      // Top moved below Bottom → [Middle, Bottom, Top, Background].
      expect(afterOrder).toEqual(["Middle", "Bottom", "Top", "Background"]);

      // 4. dragController should be cleared.
      const afterDrop = ctx.probe().state();
      expect(afterDrop.dragKind).toBeNull();
    } finally {
      ctx.dispose();
      document.body.replaceChildren();
    }
  });
});