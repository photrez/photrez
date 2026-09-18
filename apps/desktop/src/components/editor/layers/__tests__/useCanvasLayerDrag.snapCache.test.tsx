// Move-drag work-volume pins: snap targets built once per gesture, one render
// request per move. Counts only, no timing (headless timing is flaky).
//
// Real hook, real engine, real snap math; only the target builder is wrapped
// with a counter. Defeat directions: rebuilding per move turns the first case
// RED (N builds, not 1); a second render call per move turns the second case
// RED (2N, not N); dropping the outside-write subscription turns the third
// case RED (1 build, not 2); caching the snap decision turns the fourth RED.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { useCanvasLayerDrag } from "../useCanvasLayerDrag";
import { WorkspaceManager } from "@/engine/workspace";
import { ViewportCamera } from "../../../../viewport/viewportCamera";
import type { LayerNode } from "@/engine/types";
import type { ToolId } from "../../tools/toolTypes";
import type { SnapLine } from "@/viewport/smartGuides";

const { snapBuildCount } = vi.hoisted(() => ({ snapBuildCount: { n: 0 } }));

vi.mock("@/viewport/transformSnapTargets", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/viewport/transformSnapTargets")>();
  return {
    ...actual,
    buildTransformSnapTargets: (...args: Parameters<typeof actual.buildTransformSnapTargets>) => {
      snapBuildCount.n += 1;
      return actual.buildTransformSnapTargets(...args);
    },
  };
});

interface Ctx {
  ws: WorkspaceManager;
  canvasEl: HTMLElement;
  dragApi: ReturnType<typeof useCanvasLayerDrag>;
  setMoveSnapEnabled: (v: boolean) => void;
  setTool: (t: ToolId) => void;
  onSnapLinesChange: ReturnType<typeof vi.fn<(lines: SnapLine[]) => void>>;
  scheduler: { requestRender: ReturnType<typeof vi.fn> };
  dispose: () => void;
  container: HTMLElement;
}

function setup(): Ctx {
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument("snap-cache-doc", "Canvas", 800, 600);
  ws.addDocument(session);
  const a = session.engine.addLayer("Draggable") as LayerNode;
  a.transform.x = 100;
  a.transform.y = 100;
  a.width = 200;
  a.height = 200;
  // No bitmap on test layers, so the alpha hit test would fall through.
  vi.spyOn(session.engine, "sampleLayerAlpha").mockReturnValue(1);

  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const camera = new ViewportCamera();
  const container = document.createElement("div");
  document.body.appendChild(container);
  const canvasEl = document.createElement("div");
  canvasEl.style.position = "absolute";
  canvasEl.style.left = "0px";
  canvasEl.style.top = "0px";
  canvasEl.style.width = "800px";
  canvasEl.style.height = "600px";
  document.body.appendChild(canvasEl);

  const ctx = {} as Ctx;
  function Probe() {
    const ed = useEditor();
    ctx.onSnapLinesChange = vi.fn<(lines: SnapLine[]) => void>();
    ctx.dragApi = useCanvasLayerDrag({ onSnapLinesChange: ctx.onSnapLinesChange });
    ctx.setMoveSnapEnabled = (v: boolean) => ed.setMoveSnapEnabled(v);
    ctx.setTool = (t: ToolId) => ed.setActiveTool(t);
    return null;
  }
  ctx.dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as never} scheduler={scheduler as never} camera={camera}>
        <Probe />
      </EditorProvider>
    ),
    container,
  );
  canvasEl.addEventListener("pointerdown", (e) => ctx.dragApi?.handlePointerDown(e as PointerEvent));
  return { ...ctx, ws, canvasEl, scheduler, container };
}

function teardown(ctx: Ctx) {
  ctx.dispose();
  ctx.container.parentNode?.removeChild(ctx.container);
  ctx.canvasEl.parentNode?.removeChild(ctx.canvasEl);
  vi.restoreAllMocks();
}

function down(ctx: Ctx, x: number, y: number) {
  ctx.canvasEl.dispatchEvent(
    new PointerEvent("pointerdown", { bubbles: true, cancelable: true, button: 0, clientX: x, clientY: y }),
  );
}

function move(ctx: Ctx, x: number, y: number) {
  document.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, button: 0, clientX: x, clientY: y }));
}

function up(ctx: Ctx, x: number, y: number) {
  document.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, clientX: x, clientY: y }));
}

function lastSnapLines(ctx: Ctx): SnapLine[] {
  const calls = ctx.onSnapLinesChange.mock.calls;
  return calls[calls.length - 1]?.[0] ?? [];
}

describe("useCanvasLayerDrag snap-target cache + render count", () => {
  beforeEach(() => {
    snapBuildCount.n = 0;
  });

  it("builds the snap-target list once per gesture across N moves", () => {
    const ctx = setup();
    try {
      ctx.setMoveSnapEnabled(true);
      down(ctx, 150, 150);
      expect(ctx.dragApi.isDragging()).toBe(true);
      const N = 10;
      for (let i = 0; i < N; i++) move(ctx, 250 + i * 5, 200);
      up(ctx, 300, 200);
      expect(snapBuildCount.n).toBe(1);
    } finally {
      teardown(ctx);
    }
  });

  it("requests one render per move", () => {
    const ctx = setup();
    try {
      ctx.setMoveSnapEnabled(true);
      down(ctx, 150, 150);
      // Window the count to the moves only: down/up overhead is out of scope.
      const base = ctx.scheduler.requestRender.mock.calls.length;
      const N = 10;
      for (let i = 0; i < N; i++) move(ctx, 250 + i * 5, 200);
      expect(ctx.scheduler.requestRender.mock.calls.length - base).toBe(N);
      up(ctx, 300, 200);
    } finally {
      teardown(ctx);
    }
  });

  it("a layer added mid-gesture rebuilds the targets exactly once", () => {
    const ctx = setup();
    try {
      ctx.setMoveSnapEnabled(true);
      down(ctx, 150, 150);
      move(ctx, 250, 200);
      move(ctx, 255, 200);
      expect(snapBuildCount.n).toBe(1);
      ctx.ws.getActiveEngine()!.addLayer("Newcomer");
      move(ctx, 260, 200);
      move(ctx, 265, 200);
      move(ctx, 270, 200);
      up(ctx, 270, 200);
      expect(snapBuildCount.n).toBe(2);
      expect(ctx.dragApi.isDragging()).toBe(false);
    } finally {
      teardown(ctx);
    }
  });

  it("the snap switch still decides on every move, not from the cache", () => {
    const ctx = setup();
    try {
      const engine = ctx.ws.getEngine("snap-cache-doc")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      ctx.setMoveSnapEnabled(true);
      down(ctx, 150, 150);
      // Left edge lands 2px from the doc edge, inside the catch zone.
      move(ctx, 52, 150);
      expect(layer.transform.x).toBe(0);
      expect(lastSnapLines(ctx).length).toBeGreaterThan(0);

      // Switch off mid-gesture: same kind of move must land raw.
      ctx.setMoveSnapEnabled(false);
      move(ctx, 53, 150);
      expect(layer.transform.x).toBe(3);
      expect(lastSnapLines(ctx)).toEqual([]);

      // Switch back on: the held target list still applies, no rebuild.
      ctx.setMoveSnapEnabled(true);
      move(ctx, 52, 150);
      expect(layer.transform.x).toBe(0);
      expect(snapBuildCount.n).toBe(1);
      up(ctx, 52, 150);
    } finally {
      teardown(ctx);
    }
  });
});
