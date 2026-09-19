import { describe, it, expect, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { useCanvasPointerTools } from "../useCanvasPointerTools";
import { WorkspaceManager } from "@/engine/workspace";
import { ViewportCamera } from "../../../../viewport/viewportCamera";
import type { ToolId } from "../../tools/toolTypes";

vi.mock("../../dialogs/DialogProvider", async (importOriginal) => {
  const mod = await importOriginal<typeof import("../../dialogs/DialogProvider")>();
  return { ...mod, useDialog: () => ({ confirm: vi.fn(() => Promise.resolve(false)) }) };
});

interface TestApi {
  tools: ReturnType<typeof useCanvasPointerTools>;
  setTool: (t: ToolId) => void;
}

function setupSelection() {
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument("sel-escape", "Sel", 800, 600);
  ws.addDocument(session);
  const engine = session.engine;

  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const camera = new ViewportCamera();
  const container = document.createElement("div");
  document.body.appendChild(container);
  const canvasEl = document.createElement("div");
  document.body.appendChild(canvasEl);
  const rect = { left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600, x: 0, y: 0 };
  vi.spyOn(canvasEl, "getBoundingClientRect").mockReturnValue(rect as DOMRect);

  const testApi: TestApi = {} as TestApi;
  function Probe() {
    const ed = useEditor();
    testApi.tools = useCanvasPointerTools({
      getCanvasContainerRef: () => canvasEl as unknown as HTMLDivElement,
      getCanvasRef: () => undefined,
      isSpacePressed: () => false,
      isPanning: () => false,
      isAltPressed: () => false,
      stopMomentum: () => {},
      fitToScreenAndRender: () => {},
      commitBrushStroke: () => {},
    });
    testApi.setTool = (t: ToolId) => ed.setActiveTool(t);
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
  testApi.setTool("selection");
  return { ws, engine, testApi, dispose, container, canvasEl };
}

function teardown(ctx: ReturnType<typeof setupSelection>) {
  ctx.dispose();
  ctx.container.parentNode?.removeChild(ctx.container);
  ctx.canvasEl.parentNode?.removeChild(ctx.canvasEl);
  vi.restoreAllMocks();
}

function pointer(kind: "pointerdown" | "pointermove" | "pointerup", x: number, y: number): PointerEvent {
  return new PointerEvent(kind, { bubbles: true, cancelable: true, button: 0, clientX: x, clientY: y, pointerId: 1 });
}

describe("selection draw Escape cancel (wiring: real hook + real engine)", () => {
  it("Escape mid-draw cancels the gesture: release commits nothing", () => {
    const ctx = setupSelection();
    try {
      const createSpy = vi.spyOn(ctx.engine, "createSelection");
      ctx.testApi.tools.onCanvasPointerDown(pointer("pointerdown", 100, 100));
      const N = 5;
      for (let i = 1; i <= N; i++) {
        ctx.testApi.tools.onCanvasPointerMove(pointer("pointermove", 100 + 20 * i, 100 + 10 * i));
      }
      // Transient preview is live, but nothing is committed yet.
      expect(ctx.testApi.tools.selectionBox()).not.toBeNull();
      expect(createSpy).not.toHaveBeenCalled();
      expect(ctx.engine.getSelection()).toBeNull();

      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      expect(ctx.testApi.tools.selectionBox()).toBeNull();

      ctx.testApi.tools.onCanvasPointerUp(pointer("pointerup", 200, 150));
      // The cancelled draw leaves zero committed selection.
      expect(createSpy).not.toHaveBeenCalled();
      expect(ctx.engine.getSelection()).toBeNull();
    } finally {
      teardown(ctx);
    }
  });

  it("control: uninterrupted draw commits exactly once at pointerup", () => {
    const ctx = setupSelection();
    try {
      const createSpy = vi.spyOn(ctx.engine, "createSelection");
      ctx.testApi.tools.onCanvasPointerDown(pointer("pointerdown", 100, 100));
      for (let i = 1; i <= 5; i++) {
        ctx.testApi.tools.onCanvasPointerMove(pointer("pointermove", 100 + 20 * i, 100 + 10 * i));
      }
      expect(createSpy).not.toHaveBeenCalled();
      ctx.testApi.tools.onCanvasPointerUp(pointer("pointerup", 200, 150));
      expect(createSpy).toHaveBeenCalledTimes(1);
      expect(ctx.engine.getSelection()).not.toBeNull();
    } finally {
      teardown(ctx);
    }
  });

  it("Escape mid-draw with a committed selection keeps it in both engine and signal", () => {
    const ctx = setupSelection();
    try {
      ctx.engine.createSelection(10, 10, 60, 40);
      const committed = ctx.engine.getSelection();
      expect(committed).not.toBeNull();
      const createSpy = vi.spyOn(ctx.engine, "createSelection");
      // Start a fresh draw outside the committed box; the engine keeps the
      // committed selection until a release replaces it.
      ctx.testApi.tools.onCanvasPointerDown(pointer("pointerdown", 300, 300));
      for (let i = 1; i <= 5; i++) {
        ctx.testApi.tools.onCanvasPointerMove(pointer("pointermove", 300 + 20 * i, 300 + 10 * i));
      }
      expect(ctx.engine.getSelection()).toEqual(committed);

      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      // The signal syncs back from the engine instead of going blind null.
      expect(ctx.testApi.tools.selectionBox()).toEqual({
        x: committed!.x,
        y: committed!.y,
        w: committed!.width,
        h: committed!.height,
        angle: committed!.angle,
        shape: committed!.shape,
      });
      expect(ctx.engine.getSelection()).toEqual(committed);

      ctx.testApi.tools.onCanvasPointerUp(pointer("pointerup", 400, 350));
      expect(createSpy).not.toHaveBeenCalled();
      expect(ctx.engine.getSelection()).toEqual(committed);
    } finally {
      teardown(ctx);
    }
  });
});
