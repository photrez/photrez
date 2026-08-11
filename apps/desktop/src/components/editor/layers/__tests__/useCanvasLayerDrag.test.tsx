import { describe, it, expect, vi, type Mock } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { useCanvasLayerDrag } from "../useCanvasLayerDrag";
import { WorkspaceManager } from "@/engine/workspace";
import { ViewportCamera } from "../../../../viewport/viewportCamera";
import { useDragController, type DragState } from "../../DragController";
import type { LayerNode } from "@/engine/types";
import type { ToolId } from "../../tools/toolTypes";
import type { SnapLine } from "@/viewport/smartGuides";

function makeLayer(name: string, x: number, y: number, w = 100, h = 100): LayerNode {
  return {
    id: `layer-${name}`,
    name,
    type: "raster",
    visible: true,
    opacity: 1,
    locked: false,
    blendMode: "normal",
    transform: { x, y, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    width: w,
    height: h,
    imageBitmap: null,
  };
}

interface TestApi {
  dragApi: ReturnType<typeof useCanvasLayerDrag>;
  dcState: () => DragState;
  setTool: (t: ToolId) => void;
  setMoveSnapEnabled: (v: boolean) => void;
  setMoveAutoSelect: (v: boolean) => void;
  setSelectedLayerId: (id: string | null) => void;
  onSnapLinesChange: Mock<(lines: SnapLine[]) => void>;
  onHudUpdate: Mock<(hud: any) => void>;
}

describe("useCanvasLayerDrag (wiring: click+drag in canvas moves layer)", () => {
  type LayerCtx = ReturnType<typeof setupWithLayer>;

  function setupWithLayer(): {
    ws: WorkspaceManager;
    canvasEl: HTMLElement;
    testApi: TestApi;
    dispose: () => void;
    container: HTMLElement;
  } {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("wiring-canvas", "Canvas", 800, 600);
    ws.addDocument(session);
    const a = session.engine.addLayer("Draggable") as LayerNode;
    a.transform.x = 100;
    a.transform.y = 100;
    a.width = 200;
    a.height = 200;
    // Test layers carry no bitmap, so sampleLayerAlpha would report 0
    // (transparent) and the alpha-aware hit-test would skip every layer.
    // Force opaque so these wiring tests exercise the drag mechanics.
    vi.spyOn(session.engine, "sampleLayerAlpha").mockReturnValue(1);

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

    const testApi: TestApi = {} as TestApi;
    function Probe() {
      const ed = useEditor();
      const dc = useDragController();
      testApi.onSnapLinesChange = vi.fn<(lines: SnapLine[]) => void>();
      testApi.onHudUpdate = vi.fn<(hud: any) => void>();
      testApi.dragApi = useCanvasLayerDrag({
        onSnapLinesChange: testApi.onSnapLinesChange,
        onHudUpdate: testApi.onHudUpdate,
      });
      testApi.dcState = () => dc.state();
      testApi.setTool = (t: ToolId) => ed.setActiveTool(t);
      testApi.setMoveSnapEnabled = (v: boolean) => ed.setMoveSnapEnabled(v);
      testApi.setMoveAutoSelect = (v: boolean) => ed.setMoveAutoSelect(v);
      testApi.setSelectedLayerId = (id: string | null) => ed.setSelectedLayerId(id);
      return null;
    }
    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any} camera={camera}>
          <Probe />
        </EditorProvider>
      ),
      container,
    );
    canvasEl.addEventListener("pointerdown", (e) => testApi.dragApi?.handlePointerDown(e as PointerEvent));
    return { ws, canvasEl, testApi, dispose, container };
  }

  function teardown(ctx: LayerCtx) {
    ctx.dispose();
    ctx.container.parentNode?.removeChild(ctx.container);
    ctx.canvasEl.parentNode?.removeChild(ctx.canvasEl);
    vi.restoreAllMocks();
  }

  type TwoDocCtx = ReturnType<typeof setupWithTwoDocs>;
  function setupWithTwoDocs() {
    const ws = new WorkspaceManager();
    const source = WorkspaceManager.createBlankDocument("doc-a", "A", 800, 600);
    const target = WorkspaceManager.createBlankDocument("doc-b", "B", 800, 600);
    ws.addDocument(source);
    ws.addDocument(target);
    ws.switchDocument("doc-a");
    const a = source.engine.addLayer("Draggable") as LayerNode;
    a.transform.x = 100;
    a.transform.y = 100;
    a.width = 200;
    a.height = 200;
    // Test layers carry no bitmap, so sampleLayerAlpha would report 0
    // (transparent) and the alpha-aware hit-test would skip every layer.
    // Force opaque so these wiring tests exercise the drag mechanics.
    vi.spyOn(source.engine, "sampleLayerAlpha").mockReturnValue(1);
    vi.spyOn(target.engine, "sampleLayerAlpha").mockReturnValue(1);

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

    const testApi: TestApi = {} as TestApi;
    function Probe() {
      const ed = useEditor();
      const dc = useDragController();
      testApi.onSnapLinesChange = vi.fn<(lines: SnapLine[]) => void>();
      testApi.dragApi = useCanvasLayerDrag({ onSnapLinesChange: testApi.onSnapLinesChange });
      testApi.dcState = () => dc.state();
      testApi.setTool = (t: ToolId) => ed.setActiveTool(t);
      testApi.setMoveSnapEnabled = (v: boolean) => ed.setMoveSnapEnabled(v);
      testApi.setMoveAutoSelect = (v: boolean) => ed.setMoveAutoSelect(v);
      testApi.setSelectedLayerId = (id: string | null) => ed.setSelectedLayerId(id);
      return null;
    }
    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any} camera={camera}>
          <Probe />
        </EditorProvider>
      ),
      container,
    );
    canvasEl.addEventListener("pointerdown", (e) => testApi.dragApi?.handlePointerDown(e as PointerEvent));
    return { ws, canvasEl, testApi, dispose, container, renderer, scheduler };
  }

  // ── Behavioral: full drag cycle ──

  it("click+drag in canvas translates the layer's transform.x and transform.y", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      expect(layer.transform.x).toBe(100);
      expect(layer.transform.y).toBe(100);

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, cancelable: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));

      // Delta (100,50) applied to layer at (100,100) → (200,150)
      expect(layer.transform.x).toBe(200);
      expect(layer.transform.y).toBe(150);
      expect(ctx.testApi.dragApi.isDragging()).toBe(true);

      document.dispatchEvent(new PointerEvent("pointerup", {
        bubbles: true, button: 0, clientX: 200, clientY: 150,
      }));
      expect(ctx.testApi.dragApi.isDragging()).toBe(false);
    } finally {
      teardown(ctx);
    }
  });

  it("pointerdown on empty canvas area (no layer) does not start drag", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      const startX = layer.transform.x;
      const startY = layer.transform.y;

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, cancelable: true, button: 0, clientX: 900, clientY: 700,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 710, clientY: 510,
      }));
      expect(ctx.testApi.dragApi.isDragging()).toBe(false);
      expect(layer.transform.x).toBe(startX);
      expect(layer.transform.y).toBe(startY);
    } finally {
      teardown(ctx);
    }
  });

  it("hovering a document tab during canvas layer drag freezes the current visual position until cleanup", () => {
    vi.useFakeTimers();
    const ctx = setupWithTwoDocs();
    const tabEl = document.createElement("div");
    const originalElementFromPoint = (document as any).elementFromPoint;
    try {
      const sourceLayer = ctx.ws.getEngine("doc-a")!.getLayers().find((l) => l.name === "Draggable")!;
      tabEl.setAttribute("data-document-tab", "doc-b");
      document.body.appendChild(tabEl);
      (document as any).elementFromPoint = vi.fn()
        .mockReturnValueOnce(null)
        .mockReturnValue(tabEl);

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, cancelable: true, button: 0, clientX: 150, clientY: 150,
      }));

      // First move — normal (elementFromPoint returns null → move)
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 20, clientY: 20,
      }));
      expect(sourceLayer.transform.x).toBe(-30);
      expect(sourceLayer.transform.y).toBe(-30);

      // Second move — tab hover (elementFromPoint returns tabEl → freeze)
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 20, clientY: 20,
      }));
      expect(sourceLayer.transform.x).toBe(-30);
      expect(sourceLayer.transform.y).toBe(-30);

      vi.advanceTimersByTime(500);
      expect(ctx.ws.getActiveDocumentId()).toBe("doc-b");
      expect(sourceLayer.transform.x).toBe(-30);
      expect(sourceLayer.transform.y).toBe(-30);

      document.dispatchEvent(new PointerEvent("pointercancel", {
        bubbles: true, button: 0, clientX: 20, clientY: 20,
      }));
      expect(sourceLayer.transform.x).toBe(100);
      expect(sourceLayer.transform.y).toBe(100);
    } finally {
      if (originalElementFromPoint) {
        (document as any).elementFromPoint = originalElementFromPoint;
      } else {
        delete (document as any).elementFromPoint;
      }
      tabEl.parentNode?.removeChild(tabEl);
      vi.useRealTimers();
      teardown(ctx);
    }
  });

  it("same-doc drag: useCanvasLayerDrag commits exactly ONE history entry", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      const history = ctx.ws.getActiveHistory()!;

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, cancelable: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));
      document.dispatchEvent(new PointerEvent("pointerup", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));

      expect(layer.transform.x).toBe(200);
      expect(layer.transform.y).toBe(150);
      expect(history.getUndoCount()).toBe(1);
    } finally {
      teardown(ctx);
    }
  });

  it("same-doc drag undo restores the PRE-DRAG transform", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      const history = ctx.ws.getActiveHistory()!;
      const startX = layer.transform.x;
      const startY = layer.transform.y;

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, cancelable: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));
      document.dispatchEvent(new PointerEvent("pointerup", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));

      expect(history.getUndoCount()).toBe(1);
      const prev = history.undo(engine.snapshot());
      expect(prev).not.toBeNull();
      engine.restore(prev!);
      const restored = engine.getLayer(layer.id)!;
      expect(restored.transform.x).toBe(startX);
      expect(restored.transform.y).toBe(startY);
    } finally {
      teardown(ctx);
    }
  });

  it("mid-drag active-doc switch commits to source doc's history, not target", () => {
    const ctx = setupWithTwoDocs();
    try {
      const sourceEngine = ctx.ws.getEngine("doc-a")!;
      const sourceLayer = sourceEngine.getLayers().find((l) => l.name === "Draggable")!;
      const sourceHistory = ctx.ws.getHistory("doc-a")!;
      const targetHistory = ctx.ws.getHistory("doc-b")!;
      expect(sourceHistory.getUndoCount()).toBe(0);
      expect(targetHistory.getUndoCount()).toBe(0);

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, cancelable: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));

      ctx.ws.switchDocument("doc-b");

      document.dispatchEvent(new PointerEvent("pointerup", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));

      expect(sourceLayer.transform.x).toBe(200);
      expect(sourceHistory.getUndoCount()).toBe(1);
      expect(targetHistory.getUndoCount()).toBe(0);
    } finally {
      teardown(ctx);
    }
  });

  // ════════════════════════════════════════════════════════════════════════════
  // Implementation Contract: drag signal state machine
  // ════════════════════════════════════════════════════════════════════════════

  it("state contract: isDragging() is false before any interaction", () => {
    const ctx = setupWithLayer();
    try {
      expect(ctx.testApi.dragApi.isDragging()).toBe(false);
    } finally {
      teardown(ctx);
    }
  });

  it("state contract: pointerDown sets isDragging=true, pointerUp resets to false", () => {
    const ctx = setupWithLayer();
    try {
      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      expect(ctx.testApi.dragApi.isDragging()).toBe(true);

      document.dispatchEvent(new PointerEvent("pointerup", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      expect(ctx.testApi.dragApi.isDragging()).toBe(false);
    } finally {
      teardown(ctx);
    }
  });

  it("state contract: pointerCancel restores original transform and resets isDragging", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      const startX = layer.transform.x;
      const startY = layer.transform.y;

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));
      expect(layer.transform.x).not.toBe(startX);
      expect(ctx.testApi.dragApi.isDragging()).toBe(true);

      document.dispatchEvent(new PointerEvent("pointercancel", { bubbles: true, button: 0 }));
      expect(layer.transform.x).toBe(startX);
      expect(layer.transform.y).toBe(startY);
      expect(ctx.testApi.dragApi.isDragging()).toBe(false);
    } finally {
      teardown(ctx);
    }
  });

  // ════════════════════════════════════════════════════════════════════════════
  // Implementation Contract: guard conditions + listener isolation
  // ════════════════════════════════════════════════════════════════════════════
  // Setiap guard harus mencegah drag DAN mencegah registrasi document listener.
  // Jika guard return tapi listeners terdaftar, stray pointermove bisa menyebabkan
  // silent mutation.

  it("guard: right-click (button=2) on layer does NOT start drag (no listeners)", () => {
    const ctx = setupWithLayer();
    const addSpy = vi.spyOn(document, "addEventListener");
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      const startX = layer.transform.x;
      addSpy.mockClear();

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 2, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 2, clientX: 250, clientY: 200,
      }));
      expect(ctx.testApi.dragApi.isDragging()).toBe(false);
      expect(layer.transform.x).toBe(startX);
      // Implementation: guard return sebelum addEventListener → no listeners
      expect(addSpy).not.toHaveBeenCalledWith("pointermove", expect.any(Function));
      expect(addSpy).not.toHaveBeenCalledWith("pointerup", expect.any(Function));
    } finally {
      addSpy.mockRestore();
      teardown(ctx);
    }
  });

  it("guard: pointerDown on [data-handle] element does NOT start drag (no listeners)", () => {
    const ctx = setupWithLayer();
    const addSpy = vi.spyOn(document, "addEventListener");
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      const startX = layer.transform.x;
      addSpy.mockClear();

      const handleEl = document.createElement("div");
      handleEl.setAttribute("data-handle", "se");
      ctx.canvasEl.appendChild(handleEl);

      handleEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));
      expect(ctx.testApi.dragApi.isDragging()).toBe(false);
      expect(layer.transform.x).toBe(startX);
      expect(addSpy).not.toHaveBeenCalledWith("pointermove", expect.any(Function));
    } finally {
      addSpy.mockRestore();
      teardown(ctx);
    }
  });

  it("guard: non-move tool (brush) does NOT start drag (no listeners)", () => {
    const ctx = setupWithLayer();
    const addSpy = vi.spyOn(document, "addEventListener");
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      const startX = layer.transform.x;
      addSpy.mockClear();

      ctx.testApi.setTool("brush");

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));
      expect(ctx.testApi.dragApi.isDragging()).toBe(false);
      expect(layer.transform.x).toBe(startX);
      expect(addSpy).not.toHaveBeenCalledWith("pointermove", expect.any(Function));
    } finally {
      addSpy.mockRestore();
      teardown(ctx);
    }
  });

  it("guard: locked layer does NOT start drag (no listeners)", () => {
    const ctx = setupWithLayer();
    const addSpy = vi.spyOn(document, "addEventListener");
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      layer.locked = true;
      // Lock semua layer agar findLayerAt tidak menemukan apa pun
      // (Background / position-lock juga di-skip sejak guard diperkuat)
      engine.getLayers().forEach((l) => { l.locked = true; });
      const startX = layer.transform.x;
      addSpy.mockClear();

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));
      expect(ctx.testApi.dragApi.isDragging()).toBe(false);
      expect(layer.transform.x).toBe(startX);
      expect(addSpy).not.toHaveBeenCalledWith("pointermove", expect.any(Function));
    } finally {
      addSpy.mockRestore();
      teardown(ctx);
    }
  });

  // ════════════════════════════════════════════════════════════════════════════
  // Implementation Contract: event listener lifecycle
  // ════════════════════════════════════════════════════════════════════════════

  it("lifecycle: pointerDown registers document pointermove/up/cancel listeners", () => {
    const addSpy = vi.spyOn(document, "addEventListener");
    const ctx = setupWithLayer();
    try {
      addSpy.mockClear();
      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));

      expect(addSpy).toHaveBeenCalledWith("pointermove", expect.any(Function));
      expect(addSpy).toHaveBeenCalledWith("pointerup", expect.any(Function));
      expect(addSpy).toHaveBeenCalledWith("pointercancel", expect.any(Function));
    } finally {
      teardown(ctx);
      addSpy.mockRestore();
    }
  });

  it("lifecycle: pointerUp removes document pointermove/up/cancel listeners", () => {
    const addSpy = vi.spyOn(document, "addEventListener");
    const removeSpy = vi.spyOn(document, "removeEventListener");
    const ctx = setupWithLayer();
    try {
      addSpy.mockClear();
      removeSpy.mockClear();

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      const moveFn = addSpy.mock.calls.find(c => c[0] === "pointermove")?.[1];
      const upFn = addSpy.mock.calls.find(c => c[0] === "pointerup")?.[1];
      const cancelFn = addSpy.mock.calls.find(c => c[0] === "pointercancel")?.[1];

      document.dispatchEvent(new PointerEvent("pointerup", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));

      expect(removeSpy).toHaveBeenCalledWith("pointermove", moveFn);
      expect(removeSpy).toHaveBeenCalledWith("pointerup", upFn);
      expect(removeSpy).toHaveBeenCalledWith("pointercancel", cancelFn);
    } finally {
      teardown(ctx);
      addSpy.mockRestore();
      removeSpy.mockRestore();
    }
  });

  // ════════════════════════════════════════════════════════════════════════════
  // Implementation Contract: DragController integration
  // ════════════════════════════════════════════════════════════════════════════

  it("integration: pointerDown calls beginLayerDrag with correct payload via dcState", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      expect(ctx.testApi.dcState().dragKind).toBeNull();

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));

      // beginLayerDrag sets dragKind="layer" + payload with layer metadata
      expect(ctx.testApi.dcState().dragKind).toBe("layer");
      const payload = ctx.testApi.dcState().payload;
      expect(payload).toBeTruthy();
      expect(payload!.layerId).toBe(layer.id);
      expect(payload!.sourceDocId).toBe("wiring-canvas");
      expect(payload!.sourceName).toBe("Draggable");
    } finally {
      teardown(ctx);
    }
  });

  it("integration: pointerUp calls endDrag and clears dropTarget", () => {
    const ctx = setupWithLayer();
    try {
      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      expect(ctx.testApi.dcState().dragKind).toBe("layer");

      document.dispatchEvent(new PointerEvent("pointerup", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));

      expect(ctx.testApi.dcState().dragKind).toBeNull();
      expect(ctx.testApi.dcState().dropTarget).toBeNull();
    } finally {
      teardown(ctx);
    }
  });

  it("integration: pointerCancel calls endDrag and clears dropTarget", () => {
    const ctx = setupWithLayer();
    try {
      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      expect(ctx.testApi.dcState().dragKind).toBe("layer");

      document.dispatchEvent(new PointerEvent("pointercancel", { bubbles: true, button: 0 }));

      expect(ctx.testApi.dcState().dragKind).toBeNull();
      expect(ctx.testApi.dcState().dropTarget).toBeNull();
    } finally {
      teardown(ctx);
    }
  });

  // ════════════════════════════════════════════════════════════════════════════
  // Implementation Contract: auto-select OFF
  // ════════════════════════════════════════════════════════════════════════════
  // Regression path 2026-07-03: auto-select OFF harus drag layer yang terpilih
  // (selectedLayerId), bukan layer di bawah kursor. Test ini diverifikasi dengan
  // memeriksa layer mana yang bergerak — tidak cukup hanya isDragging().

  it("auto-select OFF: selected layer moves instead of hit layer", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      // Layer kedua — posisinya di DALAM Draggable (140,140) → (190,190)
      const underCursor = engine.addLayer("UnderCursor") as LayerNode;
      underCursor.transform.x = 140;
      underCursor.transform.y = 140;
      underCursor.width = 50;
      underCursor.height = 50;

      // Pilih Draggable (bukan yang di bawah kursor)
      const selected = engine.getLayers().find((l) => l.name === "Draggable")!;
      ctx.testApi.setSelectedLayerId(selected.id);
      ctx.testApi.setMoveAutoSelect(false);
      // pointerDown di (150,150) — kena UnderCursor (140→190, 140→190)
      // Tapi auto-select OFF, jadi SELECTED layer (Draggable) yang harus bergerak
      expect(selected.transform.x).toBe(100);
      expect(underCursor.transform.x).toBe(140);

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 200, clientY: 150,
      }));

      // Delta (50,0) → Draggable bergerak ke (150,100)
      expect(selected.transform.x).toBe(150);
      expect(selected.transform.y).toBe(100);
      // UnderCursor TIDAK boleh bergerak
      expect(underCursor.transform.x).toBe(140);
      expect(underCursor.transform.y).toBe(140);
    } finally {
      teardown(ctx);
    }
  });

  it("auto-select OFF + no selection: click on layer does NOT start drag", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      ctx.testApi.setSelectedLayerId(null);
      ctx.testApi.setMoveAutoSelect(false);
      const startX = layer.transform.x;

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));

      expect(ctx.testApi.dragApi.isDragging()).toBe(false);
      expect(layer.transform.x).toBe(startX);
    } finally {
      teardown(ctx);
    }
  });

  it("auto-select OFF + locked selected layer: falls through to hit layer", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      // Layer kedua yang tidak terkunci — tepat di bawah kursor
      const underCursor = engine.addLayer("UnderCursor") as LayerNode;
      underCursor.transform.x = 140;
      underCursor.transform.y = 140;
      underCursor.width = 50;
      underCursor.height = 50;

      // Selected layer (Draggable) dikunci
      const selected = engine.getLayers().find((l) => l.name === "Draggable")!;
      selected.locked = true;
      ctx.testApi.setSelectedLayerId(selected.id);
      ctx.testApi.setMoveAutoSelect(false);

      // Klik di (150,150) — kena UnderCursor (140→190, 140→190) yang tidak terkunci
      // Auto-select OFF code: selectedLayer locked → skip block → pakai hit layer
      // findLayerAt: Draggable locked → skip → UnderCursor unlocked → hit
      // Maka drag pada UnderCursor
      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));

      expect(ctx.testApi.dragApi.isDragging()).toBe(true);
      // UnderCursor belum bergerak (baru pointerDown)
      expect(underCursor.transform.x).toBe(140);
    } finally {
      teardown(ctx);
    }
  });

  // ════════════════════════════════════════════════════════════════════════════
  // Implementation Contract: same-doc tab drop reverts
  // ════════════════════════════════════════════════════════════════════════════

  it("same-doc tab drop: pointerUp on same doc tab reverts to start position", () => {
    const ctx = setupWithTwoDocs();
    const tabEl = document.createElement("div");
    const originalElementFromPoint = (document as any).elementFromPoint;
    try {
      const sourceEngine = ctx.ws.getEngine("doc-a")!;
      const sourceLayer = sourceEngine.getLayers().find((l) => l.name === "Draggable")!;
      tabEl.setAttribute("data-document-tab", "doc-a");
      document.body.appendChild(tabEl);

      // Step 1: pointerDown starts drag
      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      expect(ctx.testApi.dragApi.isDragging()).toBe(true);

      // Step 2: normal move (elementFromPoint returns null via setup mock)
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 200, clientY: 150,
      }));
      // The setup.ts mock always returns null, so this should be a normal move
      expect(sourceLayer.transform.x).toBe(150); // delta (50,0)
      expect(sourceLayer.transform.y).toBe(100);

      // Step 3: override elementFromPoint to return the same-doc tab element
      (document as any).elementFromPoint = vi.fn().mockReturnValue(tabEl);

      // Move 3 — tab hover: freeze position, set dropTarget = {type:"tab", docId:"doc-a"}
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 200, clientY: 150,
      }));
      expect(sourceLayer.transform.x).toBe(150); // frozen
      expect(sourceLayer.transform.y).toBe(100);

      // Verify dropTarget was set to the same-doc tab
      const dcState = ctx.testApi.dcState();
      expect(dcState.dropTarget).toEqual({ type: "tab", docId: "doc-a" });

      // Step 4: drop on same doc tab → revert to start position
      document.dispatchEvent(new PointerEvent("pointerup", {
        bubbles: true, button: 0, clientX: 200, clientY: 150,
      }));

      expect(sourceLayer.transform.x).toBe(100);
      expect(sourceLayer.transform.y).toBe(100);
      expect(ctx.testApi.dragApi.isDragging()).toBe(false);
    } finally {
      if (originalElementFromPoint) {
        (document as any).elementFromPoint = originalElementFromPoint;
      } else {
        delete (document as any).elementFromPoint;
      }
      tabEl.parentNode?.removeChild(tabEl);
      teardown(ctx);
    }
  });

  // ════════════════════════════════════════════════════════════════
  // Implementation Contract: cross-doc canvas drop (plan acceptance:
  // "drag A → canvas B → added at cursor")
  // ════════════════════════════════════════════════════════════════

  it("cross-doc canvas drop: hover tab B then drop on B canvas adds layer, restores source", () => {
    vi.useFakeTimers();
    const ctx = setupWithTwoDocs();
    const tabEl = document.createElement("div");
    const originalElementFromPoint = (document as any).elementFromPoint;
    try {
      const sourceEngine = ctx.ws.getEngine("doc-a")!;
      const targetEngine = ctx.ws.getEngine("doc-b")!;
      const sourceLayer = sourceEngine.getLayers().find((l) => l.name === "Draggable")!;
      const targetStartCount = targetEngine.getLayers().length;
      // Set a bitmap so uploadImage is exercised
      const fakeBitmap = { width: 200, height: 200, close: vi.fn() } as unknown as ImageBitmap;
      sourceEngine.setLayerImageBitmap(sourceLayer.id, fakeBitmap);
      vi.clearAllMocks();
      tabEl.setAttribute("data-document-tab", "doc-b");
      document.body.appendChild(tabEl);
      (document as any).elementFromPoint = vi.fn().mockReturnValue(tabEl);

      // Step 1: pointerDown starts drag on the source layer
      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, cancelable: true, button: 0, clientX: 150, clientY: 150,
      }));
      expect(ctx.testApi.dragApi.isDragging()).toBe(true);

      // Step 2: move over the tab → starts 500ms hover-to-switch
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 20, clientY: 20,
      }));
      expect(ctx.testApi.dcState().dropTarget).toEqual({ type: "tab", docId: "doc-b" });

      // Step 3: hover fires → active doc becomes doc-b
      vi.advanceTimersByTime(500);
      expect(ctx.ws.getActiveDocumentId()).toBe("doc-b");

      // Step 4: move over the (now target) canvas → cross-doc canvas drop target
      (document as any).elementFromPoint = vi.fn().mockReturnValue(null);
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 300, clientY: 300,
      }));
      expect(ctx.testApi.dcState().dropTarget).toEqual({ type: "canvas" });

      // Step 5: drop on the target canvas → cross-doc add
      document.dispatchEvent(new PointerEvent("pointerup", {
        bubbles: true, button: 0, clientX: 300, clientY: 300,
      }));

      // Target doc gained exactly one layer (the copied source layer)
      expect(targetEngine.getLayers().length).toBe(targetStartCount + 1);
      // Source layer in doc-a was restored to its pre-drag position
      expect(sourceLayer.transform.x).toBe(100);
      expect(sourceLayer.transform.y).toBe(100);
      // Drop target cleared after the drop
      expect(ctx.testApi.dcState().dropTarget).toBeNull();
      expect(ctx.testApi.dragApi.isDragging()).toBe(false);
      // uploadImage was called for the NEW layer (cross-doc → new id)
      const added = targetEngine.getLayers().find((l) => l.name === "Draggable" && l.id !== sourceLayer.id)!;
      expect(ctx.renderer!.uploadImage).toHaveBeenCalledWith(added.id, added.imageBitmap);
      expect(ctx.scheduler!.requestRender).toHaveBeenCalled();
    } finally {
      if (originalElementFromPoint) {
        (document as any).elementFromPoint = originalElementFromPoint;
      } else {
        delete (document as any).elementFromPoint;
      }
      tabEl.parentNode?.removeChild(tabEl);
      vi.useRealTimers();
      teardown(ctx);
    }
  });

  // ════════════════════════════════════════════════════════════════
  // Implementation Contract: layer detection order
  // ════════════════════════════════════════════════════════════════════════════

  it("findLayerAt hits the topmost (first) visible unlocked layer", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      // addLayer inserts di index 0 (top). Layer yang terakhir ditambah ada di TOP.
      // UnderCursor (140,140,50,50) ada di DALAM Draggable (100,100,200,200)
      const topLayer = engine.addLayer("TopLayer") as LayerNode;
      topLayer.transform.x = 140;
      topLayer.transform.y = 140;
      topLayer.width = 50;
      topLayer.height = 50;

      // findLayerAt iterates top→bottom (index 0 first)
      // Klik di (150,150) — kena kedua layer, TopLayer (index 0) harus kena duluan
      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 200, clientY: 150,
      }));

      // Delta (50,0) → TopLayer bergerak
      expect(topLayer.transform.x).toBe(190);
      expect(topLayer.transform.y).toBe(140);
      // Draggable (di bawah) TIDAK bergerak
      const draggable = engine.getLayers().find((l) => l.name === "Draggable")!;
      expect(draggable.transform.x).toBe(100);
      expect(draggable.transform.y).toBe(100);
    } finally {
      teardown(ctx);
    }
  });

  it("findLayerAt skips invisible layers", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      // Layer di atas Draggable tapi invisible → harus di-skip
      const invisibleLayer = engine.addLayer("Invisible") as LayerNode;
      invisibleLayer.transform.x = 140;
      invisibleLayer.transform.y = 140;
      invisibleLayer.width = 50;
      invisibleLayer.height = 50;
      invisibleLayer.visible = false;

      // Klik di (150,150) — kena Invisible (skip karena tidak visible)
      // → lanjut ke Draggable (100,100,200,200) → hit
      const draggable = engine.getLayers().find((l) => l.name === "Draggable")!;
      expect(draggable.transform.x).toBe(100);

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 200, clientY: 150,
      }));

      // Delta (50,0) → Draggable bergerak (bukan Invisible)
      expect(draggable.transform.x).toBe(150);
      // Invisible tidak bergerak
      expect(invisibleLayer.transform.x).toBe(140);
    } finally {
      teardown(ctx);
    }
  });

  // ════════════════════════════════════════════════════════════════════════════
  // Implementation Contract: parametric (text/shape) layers are box-hittable
  // in the drag path — same rule as the click-select path (isBoxHittable).
  // @bug 2026-08-09: "clicking text/shape selects Background" — alpha-aware
  // fall-through must NOT apply to text/shape boxes, or dragging the
  // transparent padding of a text layer would grab the opaque Background
  // beneath it.
  // ════════════════════════════════════════════════════════════════════════════

  it("drag path: text layer is box-hittable — transparent box padding still drags the text layer", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const textLayer = engine.addLayer("Hello") as LayerNode;
      textLayer.type = "text";
      textLayer.transform.x = 100;
      textLayer.transform.y = 100;
      textLayer.width = 200;
      textLayer.height = 200;
      // Real text bitmaps are transparent across most of their box (line-height
      // padding around the glyphs). Simulate that: alpha is 0 exactly on this
      // layer, 1 elsewhere (opaque Background, opaque other layers).
      vi.spyOn(engine, "sampleLayerAlpha").mockImplementation((id) => (id === textLayer.id ? 0 : 1));

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      expect(ctx.testApi.dragApi.isDragging()).toBe(true);

      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 200, clientY: 150,
      }));

      // Delta (50,0) applied to the TEXT layer → (150,100). Without the
      // box-hittable rule the click would fall through to the Background.
      expect(textLayer.transform.x).toBe(150);
      expect(textLayer.transform.y).toBe(100);
    } finally {
      teardown(ctx);
    }
  });

  it("drag path: raster layer keeps alpha-aware fall-through — transparent pixel picks the layer beneath", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const rasterLayer = engine.addLayer("Photo") as LayerNode;
      rasterLayer.type = "raster";
      rasterLayer.transform.x = 100;
      rasterLayer.transform.y = 100;
      rasterLayer.width = 200;
      rasterLayer.height = 200;
      // Transparent exactly on the raster layer; opaque elsewhere.
      vi.spyOn(engine, "sampleLayerAlpha").mockImplementation((id) => (id === rasterLayer.id ? 0 : 1));

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 200, clientY: 150,
      }));

      // The raster layer is skipped (alpha 0) → the "Draggable" layer beneath
      // becomes the drag target instead.
      expect(rasterLayer.transform.x).toBe(100);
      expect(rasterLayer.transform.y).toBe(100);
      const draggable = engine.getLayers().find((l) => l.name === "Draggable")!;
      expect(draggable.transform.x).toBe(150);
      expect(draggable.transform.y).toBe(100);
    } finally {
      teardown(ctx);
    }
  });

  // ════════════════════════════════════════════════════════════════════════════
  // Implementation Contract: position-locked / Background layers cannot be moved
  // (regression: ghost snap guides appeared because transformLayer silently
  // discarded the position change while the drag still emitted snap lines)
  // ════════════════════════════════════════════════════════════════════════════

  it("findLayerAt picks up position-locked (lockPosition) layers — drag starts, no snap, no move", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      layer.lockPosition = true;
      const startX = layer.transform.x;
      const startY = layer.transform.y;

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      expect(ctx.testApi.dragApi.isDragging()).toBe(true);

      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));

      // Engine transformLayer blocks position change → layer unmoved
      expect(layer.transform.x).toBe(startX);
      expect(layer.transform.y).toBe(startY);
      // lockPosition guard clears snap lines immediately — no snap guides
      expect(ctx.testApi.onSnapLinesChange).toHaveBeenCalledWith([]);
    } finally {
      teardown(ctx);
    }
  });

  it("findLayerAt picks up Background — drag starts, no snap, no move", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      layer.isBackground = true;
      layer.lockPosition = true;
      layer.lockRotation = true;
      const startX = layer.transform.x;
      const startY = layer.transform.y;

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      expect(ctx.testApi.dragApi.isDragging()).toBe(true);

      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));

      // LockPosition blocks engine.transformLayer → position unchanged
      expect(layer.transform.x).toBe(startX);
      expect(layer.transform.y).toBe(startY);
      // Same-doc guard clears snap lines — no snap guides during Background drag
      expect(ctx.testApi.onSnapLinesChange).toHaveBeenCalledWith([]);
    } finally {
      teardown(ctx);
    }
  });

  it("auto-select OFF + position-locked selected layer: falls through to hit layer", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const underCursor = engine.addLayer("UnderCursor") as LayerNode;
      underCursor.transform.x = 140;
      underCursor.transform.y = 140;
      underCursor.width = 50;
      underCursor.height = 50;

      const selected = engine.getLayers().find((l) => l.name === "Draggable")!;
      selected.lockPosition = true; // position-locked, but NOT fully locked
      ctx.testApi.setSelectedLayerId(selected.id);
      ctx.testApi.setMoveAutoSelect(false);

      // Klik di (150,150) — kena UnderCursor yang tidak terkunci.
      // findLayerAt skip Draggable (lockPosition) → hit UnderCursor.
      // Auto-select OFF block skip karena selected lockPosition.
      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 200, clientY: 150,
      }));

      expect(ctx.testApi.dragApi.isDragging()).toBe(true);
      // UnderCursor bergerak (delta 50,0), Draggable (lockPosition) diam
      expect(underCursor.transform.x).toBe(190);
      expect(selected.transform.x).toBe(100);
    } finally {
      teardown(ctx);
    }
  });

  it("normal layer drag still invokes onSnapLinesChange (contrast / harness sanity)", () => {
    const ctx = setupWithLayer();
    try {
      const engine = ctx.ws.getEngine("wiring-canvas")!;
      const layer = engine.getLayers().find((l) => l.name === "Draggable")!;
      ctx.testApi.setMoveSnapEnabled(true);

      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 250, clientY: 200,
      }));

      expect(ctx.testApi.dragApi.isDragging()).toBe(true);
      expect(ctx.testApi.onSnapLinesChange).toHaveBeenCalled();
    } finally {
      teardown(ctx);
    }
  });

  it("layer drag emits onHudUpdate with deltaX/deltaY during move and null on pointerup", () => {
    const ctx = setupWithLayer();
    try {
      ctx.canvasEl.dispatchEvent(new PointerEvent("pointerdown", {
        bubbles: true, button: 0, clientX: 150, clientY: 150,
      }));
      document.dispatchEvent(new PointerEvent("pointermove", {
        bubbles: true, button: 0, clientX: 200, clientY: 180,
      }));

      expect(ctx.testApi.onHudUpdate).toHaveBeenCalledWith(expect.objectContaining({
        mode: "move",
        clientX: 200,
        clientY: 180,
        deltaX: 50,
        deltaY: 30,
      }));

      document.dispatchEvent(new PointerEvent("pointerup", {
        bubbles: true, button: 0, clientX: 200, clientY: 180,
      }));

      expect(ctx.testApi.onHudUpdate).toHaveBeenLastCalledWith(null);
    } finally {
      teardown(ctx);
    }
  });
});
