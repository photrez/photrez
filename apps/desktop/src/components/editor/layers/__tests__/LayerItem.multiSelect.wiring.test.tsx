import { describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { DragControllerProvider } from "../../DragController";
import { LayersPanel } from "../LayersPanel";
import { WorkspaceManager } from "@/engine/workspace";

/**
 * A layer row has TWO selection entry points: the pointerup that ends its press-drag
 * gesture, and the click that every completed press then fires. Both call onSelect, so
 * one click runs the selector twice. A plain click is idempotent - it just sets the
 * active id - so the double call is invisible. The modifier branch is a TOGGLE, so the
 * second call undid the first and ctrl+clicking a second row selected nothing.
 *
 * That defect lives only in the wiring between the two handlers, so a test of the
 * selector in isolation proves nothing: it would pass on the broken tree. This drives
 * the real panel, with the real selector, using the real DOM event sequence.
 */
describe("LayerItem selection wiring", () => {
  function setup() {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("multi-select-doc", "Sel", 400, 300);
    ws.addDocument(session);
    session.engine.addLayer("Middle");
    session.engine.addLayer("Top");
    // addLayer inserts at the top, so rows read Top, Middle, Background.

    const container = document.createElement("div");
    document.body.appendChild(container);

    let editor: ReturnType<typeof useEditor> | null = null;
    const dispose = render(
      () => (
        <EditorProvider
          workspace={ws}
          renderer={{ uploadImage: vi.fn(), destroyTexture: vi.fn() } as any}
          scheduler={{ requestRender: vi.fn() } as any}
        >
          <DragControllerProvider>
            <LayersPanel />
            {(() => {
              editor = useEditor();
              return null;
            })()}
          </DragControllerProvider>
        </EditorProvider>
      ),
      container,
    );

    const rows = Array.from(container.querySelectorAll<HTMLElement>("[data-layer-idx]"));
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

    return {
      container,
      rows,
      dispose,
      ids: () => ws.getActiveEngine()!.getLayers().map((l) => l.id),
      selected: () => editor!.selectedLayerIds(),
    };
  }

  /** The event sequence a primary press on a row produces in a browser. */
  function pressRow(row: HTMLElement, init: { ctrlKey?: boolean; shiftKey?: boolean } = {}) {
    const base = { bubbles: true, button: 0, clientX: 10, clientY: 10, ...init };
    row.dispatchEvent(new PointerEvent("pointerdown", base));
    row.dispatchEvent(new PointerEvent("pointerup", base));
    row.dispatchEvent(new MouseEvent("click", { bubbles: true, ...init }));
  }

  /** The same, with a move past the drag threshold in the middle: a real drag. */
  function dragRow(row: HTMLElement, dropOn: HTMLElement) {
    row.dispatchEvent(
      new PointerEvent("pointerdown", { bubbles: true, button: 0, clientX: 10, clientY: 10 }),
    );
    row.dispatchEvent(
      new PointerEvent("pointermove", {
        bubbles: true,
        clientX: 10,
        clientY: dropOn.getBoundingClientRect().top + 35,
      }),
    );
    row.dispatchEvent(
      new PointerEvent("pointerup", {
        bubbles: true,
        clientX: 10,
        clientY: dropOn.getBoundingClientRect().top + 35,
      }),
    );
  }

  function withCtx(fn: (ctx: ReturnType<typeof setup>) => void) {
    const ctx = setup();
    try {
      fn(ctx);
    } finally {
      ctx.dispose();
      document.body.replaceChildren();
    }
  }

  it("ctrl+click on an unselected row adds it to the selection", () => {
    withCtx((ctx) => {
      const [top, middle] = ctx.ids();

      pressRow(ctx.rows[0]);
      expect(ctx.selected(), "plain click selects exactly one row").toEqual([top]);

      pressRow(ctx.rows[1], { ctrlKey: true });
      expect(
        [...ctx.selected()].sort(),
        "ctrl+click on an unselected row must ADD it, not cancel itself out",
      ).toEqual([middle, top].sort());
    });
  });

  it("ctrl+click on an already-selected row removes it", () => {
    withCtx((ctx) => {
      const [top, middle] = ctx.ids();

      pressRow(ctx.rows[0]);
      pressRow(ctx.rows[1], { ctrlKey: true });
      pressRow(ctx.rows[1], { ctrlKey: true });

      expect([...ctx.selected()].sort()).toEqual([top]);
      expect(ctx.selected()).not.toContain(middle);
    });
  });

  it("a plain click still replaces the selection with exactly that one row", () => {
    withCtx((ctx) => {
      const [top] = ctx.ids();

      pressRow(ctx.rows[0]);
      pressRow(ctx.rows[1]);
      expect(ctx.selected(), "a second plain click moves the selection").toEqual([ctx.ids()[1]]);

      pressRow(ctx.rows[0]);
      expect(ctx.selected()).toEqual([top]);
    });
  });

  it("a pointer drag reorders layers and does not select anything", () => {
    withCtx((ctx) => {
      const [topId, middleId, backgroundId] = ctx.ids();
      expect(topId).not.toBe(middleId);
      // Non-vacuous: a fresh document already reports its active layer as selected, so
      // "unchanged" below is a comparison between two real values, not two undefineds.
      const before = ctx.selected();
      expect(before.length).toBeGreaterThan(0);

      dragRow(ctx.rows[0], ctx.rows[1]);

      expect(ctx.ids(), "the drag still reorders").toEqual([middleId, topId, backgroundId]);
      expect(ctx.selected(), "ending a drag must not run the selector at all").toEqual(before);
    });
  });
});