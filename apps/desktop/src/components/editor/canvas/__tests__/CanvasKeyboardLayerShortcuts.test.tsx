import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createEffect } from "solid-js";
import { render } from "solid-js/web";
import { EditorProvider } from "../../shell/EditorContext";
import { useEditor } from "../../shell/EditorContext";
import { useCanvasKeyboard } from "../useCanvasKeyboard";
import { useEditorCommands, dispatchEditorCommand } from "../../useEditorCommands";
import { clearRegistry } from "../../keyboardRegistry";
import { WorkspaceManager } from "@/engine/workspace";
import { easeOutCubic } from "@/viewport/easing";
import * as Toast from "../../Toast";

function installOffscreenCanvasMock(bitmap: ImageBitmap) {
  vi.stubGlobal("OffscreenCanvas", class {
    width: number;
    height: number;

    constructor(width: number, height: number) {
      this.width = width;
      this.height = height;
    }

    getContext() {
      return {
        drawImage: vi.fn(),
        save: vi.fn(),
        restore: vi.fn(),
        translate: vi.fn(),
        rotate: vi.fn(),
        scale: vi.fn(),
        set globalAlpha(_value: number) {},
        set globalCompositeOperation(_value: string) {},
      };
    }

    transferToImageBitmap() {
      return bitmap;
    }
  });
}

function KeyboardHarness() {
  useCanvasKeyboard({
    isSpacePressed: () => false,
    setIsSpacePressed: vi.fn(),
    isAltPressed: () => false,
    setIsAltPressed: vi.fn(),
    isPanning: () => false,
    setIsPanning: vi.fn(),
    stopMomentum: vi.fn(),
    fitToScreenAndRender: vi.fn(),
    syncViewport: vi.fn(),
    getCanvasContainerRef: () => undefined,
  });

  return null;
}

function ZoomKeyboardHarness(props: {
  fitToScreenAndRender: (animated?: boolean) => void;
  stopMomentum: () => void;
  syncViewport: () => void;
  containerRef: HTMLDivElement;
  captureEditor: (editor: ReturnType<typeof useEditor>) => void;
}) {
  const editor = useEditor();
  props.captureEditor(editor);
  useCanvasKeyboard({
    isSpacePressed: () => false,
    setIsSpacePressed: vi.fn(),
    isAltPressed: () => false,
    setIsAltPressed: vi.fn(),
    isPanning: () => false,
    setIsPanning: vi.fn(),
    stopMomentum: props.stopMomentum,
    fitToScreenAndRender: props.fitToScreenAndRender,
    syncViewport: props.syncViewport,
    getCanvasContainerRef: () => props.containerRef,
  });

  return null;
}

function renderKeyboardHarness(session: ReturnType<typeof WorkspaceManager.createBlankDocument>) {
  const ws = new WorkspaceManager();
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);

  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
        <KeyboardHarness />
      </EditorProvider>
    ),
    container,
  );

  ws.addDocument(session);

  return {
    ws,
    renderer,
    scheduler,
    dispose: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

describe("canvas layer keyboard shortcuts", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    clearRegistry();
  });

  // ─── Zoom keyboard wiring tests ──────────────────────────────────────────
  // These tests verify that Ctrl+=, Ctrl+-, Ctrl+1, and Ctrl+0 dispatch
  // through useEditorCommands to the real camera.  The camera animation is
  // ticked manually because requestAnimationFrame doesn't run in jsdom.

  /** Harness that mounts both useEditorCommands (zoom shortcuts) and
   *  useCanvasKeyboard (Ctrl+0 fit-to-screen, pan, etc.).  Captures the
   *  editor context so tests can read camera state directly. */
  function ZoomCommandHarness(props: {
    captureEditor: (e: ReturnType<typeof useEditor>) => void;
    fitToScreenAndRender?: (animated?: boolean) => void;
  }) {
    const editor = useEditor();
    props.captureEditor(editor);
    useEditorCommands(() => undefined);
    useCanvasKeyboard({
      isSpacePressed: () => false,
      setIsSpacePressed: vi.fn(),
      isAltPressed: () => false,
      setIsAltPressed: vi.fn(),
      isPanning: () => false,
      setIsPanning: vi.fn(),
      stopMomentum: vi.fn(),
      fitToScreenAndRender: props.fitToScreenAndRender ?? vi.fn(),
      syncViewport: vi.fn(),
      getCanvasContainerRef: () => undefined,
    });
    return null;
  }

  function tickCamera(camera: import("@/viewport/viewportCamera").ViewportCamera): number {
    const startTime = performance.now();
    let iterations = 0;
    while (camera.isAnimating() && iterations < 100) {
      camera.tick(startTime + 300);
      iterations++;
    }
    return iterations;
  }

  it("Ctrl+= triggers zoom-in animation through useEditorCommands", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("zoom-in-doc", "Zoom In", 800, 600);
    ws.addDocument(session);
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    const container = document.createElement("div");
    document.body.appendChild(container);
    let captured: ReturnType<typeof useEditor> | undefined;

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <ZoomCommandHarness captureEditor={(e) => { captured = e; }} />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    const editor = captured!;
    editor.camera.setViewportSize(1000, 700);
    editor.camera.setState({ x: 0, y: 0, zoom: 1 });

    window.dispatchEvent(new KeyboardEvent("keydown", {
      key: "=", ctrlKey: true, bubbles: true,
    }));

    expect(editor.camera.isAnimating()).toBe(true);
    tickCamera(editor.camera);
    expect(editor.camera.isAnimating()).toBe(false);
    expect(editor.camera.getState().zoom).toBeCloseTo(1.25, 2);

    dispose();
  });

  it("Ctrl+- triggers zoom-out animation through useEditorCommands", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("zoom-out-doc", "Zoom Out", 800, 600);
    ws.addDocument(session);
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    const container = document.createElement("div");
    document.body.appendChild(container);
    let captured: ReturnType<typeof useEditor> | undefined;

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <ZoomCommandHarness captureEditor={(e) => { captured = e; }} />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    const editor = captured!;
    editor.camera.setViewportSize(1000, 700);
    editor.camera.setState({ x: 0, y: 0, zoom: 1 });

    window.dispatchEvent(new KeyboardEvent("keydown", {
      key: "-", ctrlKey: true, bubbles: true,
    }));

    expect(editor.camera.isAnimating()).toBe(true);
    tickCamera(editor.camera);
    expect(editor.camera.isAnimating()).toBe(false);
    expect(editor.camera.getState().zoom).toBeCloseTo(0.8, 2);

    dispose();
  });

  it("Ctrl+1 triggers actual-size animation from zoomed state", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("actual-size-doc", "Actual Size", 800, 600);
    ws.addDocument(session);
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    const container = document.createElement("div");
    document.body.appendChild(container);
    let captured: ReturnType<typeof useEditor> | undefined;

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <ZoomCommandHarness captureEditor={(e) => { captured = e; }} />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    const editor = captured!;
    editor.camera.setViewportSize(1000, 700);
    editor.camera.setState({ x: 100, y: 80, zoom: 2.5 });

    window.dispatchEvent(new KeyboardEvent("keydown", {
      key: "1", ctrlKey: true, bubbles: true,
    }));

    expect(editor.camera.isAnimating()).toBe(true);
    tickCamera(editor.camera);
    expect(editor.camera.isAnimating()).toBe(false);
    expect(editor.camera.getState().zoom).toBeCloseTo(1.0, 2);

    dispose();
  });

  it("Ctrl+P: always preventDefault (never WebView2 browser print), opens app print dialog only when no modal is open", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("print-doc", "Print", 800, 600);
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    const container = document.createElement("div");
    document.body.appendChild(container);
    let captured: ReturnType<typeof useEditor> | undefined;
    const dialogOpen: boolean[] = [];

    // Observe the shared showPrintDialog signal (setShowPrintDialog is not
    // exposed on the useEditor object identity, so spy-on-object is unusable).
    function PrintDialogStateHarness(props: { onChange: (open: boolean) => void }) {
      const { showPrintDialog } = useEditor();
      createEffect(() => props.onChange(showPrintDialog()));
      return null;
    }

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <ZoomCommandHarness captureEditor={(e) => { captured = e; }} />
          <PrintDialogStateHarness onChange={(open) => dialogOpen.push(open)} />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    // addDocument AFTER mount: workspace.onChange -> syncState sets the
    // activeDocumentId signal required by requiresDocument("file.print").
    ws.addDocument(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    const editor = captured!;
    dialogOpen.length = 0;

    try {
      // No modal open: Ctrl+P swallows the key and opens the app print dialog.
      const ev1 = new KeyboardEvent("keydown", { key: "p", ctrlKey: true, bubbles: true, cancelable: true });
      window.dispatchEvent(ev1);
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(ev1.defaultPrevented).toBe(true);
      expect(dialogOpen).toContain(true);

      // Modal open: command is suppressed, but the key MUST still be
      // preventDefault'ed so WebView2's default browser print dialog never
      // appears (regression: handler returned without preventDefault while
      // aria-modal was open).
      editor.setShowPrintDialog(false);
      dialogOpen.length = 0;
      const modal = document.createElement("div");
      modal.setAttribute("aria-modal", "true");
      document.body.appendChild(modal);

      const ev2 = new KeyboardEvent("keydown", { key: "p", ctrlKey: true, bubbles: true, cancelable: true });
      window.dispatchEvent(ev2);
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(ev2.defaultPrevented).toBe(true);
      expect(dialogOpen).not.toContain(true);
    } finally {
      dispose();
    }
  });

  it("Ctrl+= still works when a range slider has focus (regression: isEditableTarget)", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("range-focus-doc", "Range Focus", 800, 600);
    ws.addDocument(session);
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    const container = document.createElement("div");
    document.body.appendChild(container);
    let captured: ReturnType<typeof useEditor> | undefined;

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <ZoomCommandHarness captureEditor={(e) => { captured = e; }} />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    const editor = captured!;
    editor.camera.setViewportSize(1000, 700);
    editor.camera.setState({ x: 0, y: 0, zoom: 1 });

    // Place a range input and focus it — simulates the user having just
    // dragged a brightness/opacity slider, leaving focus on the <input>.
    const slider = document.createElement("input");
    slider.type = "range";
    slider.setAttribute("aria-label", "Bright");
    container.appendChild(slider);
    slider.focus();
    expect(document.activeElement).toBe(slider);

    window.dispatchEvent(new KeyboardEvent("keydown", {
      key: "=", ctrlKey: true, bubbles: true,
    }));

    // Zoom must NOT be blocked by isEditableTarget(range input)
    expect(editor.camera.isAnimating()).toBe(true);
    tickCamera(editor.camera);
    expect(editor.camera.getState().zoom).toBeCloseTo(1.25, 2);

    dispose();
  });

  it("rapid Ctrl+= presses only result in final zoom (animation cancel)", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("rapid-doc", "Rapid Zoom", 800, 600);
    ws.addDocument(session);
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    const container = document.createElement("div");
    document.body.appendChild(container);
    let captured: ReturnType<typeof useEditor> | undefined;

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <ZoomCommandHarness captureEditor={(e) => { captured = e; }} />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    const editor = captured!;
    editor.camera.setViewportSize(1000, 700);
    editor.camera.setState({ x: 0, y: 0, zoom: 1 });

    // Three rapid Ctrl+= dispatches (all before tick — each computes
    // from the SAME camera state since animation hasn't started yet)
    window.dispatchEvent(new KeyboardEvent("keydown", {
      key: "=", ctrlKey: true, bubbles: true,
    }));
    window.dispatchEvent(new KeyboardEvent("keydown", {
      key: "=", ctrlKey: true, bubbles: true,
    }));
    window.dispatchEvent(new KeyboardEvent("keydown", {
      key: "=", ctrlKey: true, bubbles: true,
    }));

    // Each dispatch cancels the previous animation (but since no tick
    // has happened, they all compute the same target from zoom=1)
    expect(editor.camera.isAnimating()).toBe(true);
    tickCamera(editor.camera);
    expect(editor.camera.isAnimating()).toBe(false);
    expect(editor.camera.getState().zoom).toBeCloseTo(1.25, 2);

    dispose();
  });

  it("zoom keyboard shortcuts silently ignored when no document is open", async () => {
    const ws = new WorkspaceManager(); // no documents
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    const container = document.createElement("div");
    document.body.appendChild(container);
    let captured: ReturnType<typeof useEditor> | undefined;

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <ZoomCommandHarness captureEditor={(e) => { captured = e; }} />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    const editor = captured!;
    editor.camera.setViewportSize(1000, 700);
    editor.camera.setState({ x: 0, y: 0, zoom: 1 });

    // Zoom commands require a document — should be silently ignored
    window.dispatchEvent(new KeyboardEvent("keydown", {
      key: "=", ctrlKey: true, bubbles: true,
    }));
    expect(editor.camera.isAnimating()).toBe(false);
    expect(editor.camera.getState().zoom).toBe(1);

    dispose();
  });

  it("Ctrl+NumpadAdd triggers zoom-in (key '+' variant)", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("numpad-add-doc", "Numpad Add", 800, 600);
    ws.addDocument(session);
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    const container = document.createElement("div");
    document.body.appendChild(container);
    let captured: ReturnType<typeof useEditor> | undefined;

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <ZoomCommandHarness captureEditor={(e) => { captured = e; }} />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    const editor = captured!;
    editor.camera.setViewportSize(1000, 700);
    editor.camera.setState({ x: 0, y: 0, zoom: 1 });

    // NumpadAdd sends key "+"
    window.dispatchEvent(new KeyboardEvent("keydown", {
      key: "+", code: "NumpadAdd", ctrlKey: true, bubbles: true,
    }));

    expect(editor.camera.isAnimating()).toBe(true);
    tickCamera(editor.camera);
    expect(editor.camera.getState().zoom).toBeCloseTo(1.25, 2);

    dispose();
  });

  it("Ctrl+NumpadSubtract triggers zoom-out (key '-' variant)", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("numpad-sub-doc", "Numpad Sub", 800, 600);
    ws.addDocument(session);
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    const container = document.createElement("div");
    document.body.appendChild(container);
    let captured: ReturnType<typeof useEditor> | undefined;

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <ZoomCommandHarness captureEditor={(e) => { captured = e; }} />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    const editor = captured!;
    editor.camera.setViewportSize(1000, 700);
    editor.camera.setState({ x: 0, y: 0, zoom: 1 });

    // NumpadSubtract sends key "-"
    window.dispatchEvent(new KeyboardEvent("keydown", {
      key: "-", code: "NumpadSubtract", ctrlKey: true, bubbles: true,
    }));

    expect(editor.camera.isAnimating()).toBe(true);
    tickCamera(editor.camera);
    expect(editor.camera.getState().zoom).toBeCloseTo(0.8, 2);

    dispose();
  });

  it("Ctrl+0 routes to fit-to-screen through useCanvasKeyboard (not useEditorCommands)", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("fit-cmds", "Fit Cmds", 800, 600);
    ws.addDocument(session);
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    const container = document.createElement("div");
    document.body.appendChild(container);
    let captured: ReturnType<typeof useEditor> | undefined;
    const fitToScreenSpy = vi.spyOn(session.engine, "fitToScreen");
    const fitToScreenAndRender = vi.fn();

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <ZoomCommandHarness
            captureEditor={(e) => { captured = e; }}
            fitToScreenAndRender={fitToScreenAndRender}
          />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "0", code: "Digit0", ctrlKey: true }));

    // Ctrl+0 now routes through useCanvasKeyboard → fitToScreenAndRender(false)
    expect(fitToScreenAndRender).toHaveBeenCalledWith(false);
    // useEditorCommands no longer intercepts Ctrl+0
    expect(fitToScreenSpy).not.toHaveBeenCalled();

    dispose();
  });

  it("dispatchEditorCommand('view.fit-canvas') updates camera via menu path (regression: lastVp cache)", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("fit-menu", "Fit Menu", 800, 600);
    ws.addDocument(session);
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    const container = document.createElement("div");
    document.body.appendChild(container);
    let captured: ReturnType<typeof useEditor> | undefined;
    const fitToScreenSpy = vi.spyOn(session.engine, "fitToScreen");

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <ZoomCommandHarness captureEditor={(e) => { captured = e; }} />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    const editor = captured!;
    editor.camera.setViewportSize(1000, 700);

    // Step 1: simulate user panning the viewport
    editor.camera.setState({ x: 200, y: 150, zoom: 3 });

    // Step 2: dispatch menu command (same path as View > Fit Canvas)
    dispatchEditorCommand("view.fit-canvas");

    // Step 3: verify engine.fitToScreen was called (menu path goes through useEditorCommands)
    expect(fitToScreenSpy).toHaveBeenCalledOnce();
    expect(fitToScreenSpy).toHaveBeenCalledWith(expect.any(Number), expect.any(Number));

    // Step 4: verify camera moved from panned position to fit-to-screen values
    const state = editor.camera.getState();
    expect(state.zoom).toBeLessThan(1.5); // zoomed out from 3x
    expect(state.x).not.toBe(200); // panX changed
    expect(state.y).not.toBe(150); // panY changed

    // Step 5: verify render was requested
    expect(scheduler.requestRender).toHaveBeenCalled();

    dispose();
  });

  it("dispatchEditorCommand('view.zoom-to-selection') fits the selection and requests render", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("zts-menu", "ZTS Menu", 800, 600);
    ws.addDocument(session);
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    const container = document.createElement("div");
    document.body.appendChild(container);
    let captured: ReturnType<typeof useEditor> | undefined;
    const zoomToSelectionSpy = vi.spyOn(session.engine, "zoomToSelection");

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <ZoomCommandHarness captureEditor={(e) => { captured = e; }} />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    const editor = captured!;
    editor.camera.setViewportSize(1000, 700);
    session.engine.createSelection(100, 100, 200, 200);
    editor.camera.setState({ x: 200, y: 150, zoom: 3 });

    dispatchEditorCommand("view.zoom-to-selection");

    expect(zoomToSelectionSpy).toHaveBeenCalledOnce();
    const state = editor.camera.getState();
    expect(state.zoom).toBeGreaterThan(1); // zoomed toward the 200x200 selection
    expect(state.x).not.toBe(200);
    expect(state.y).not.toBe(150);
    expect(scheduler.requestRender).toHaveBeenCalled();

    dispose();
  });

  it("routes Ctrl+0 to instant fit-to-screen (useCanvasKeyboard-only path)", async () => {
    const session = WorkspaceManager.createBlankDocument("fit-keyboard", "Fit Keyboard", 800, 600);
    const ws = new WorkspaceManager();
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    const viewport = document.createElement("div");
    const fitToScreenAndRender = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <ZoomKeyboardHarness
            fitToScreenAndRender={fitToScreenAndRender}
            stopMomentum={vi.fn()}
            syncViewport={vi.fn()}
            containerRef={viewport}
            captureEditor={() => {}}
          />
        </EditorProvider>
      ),
      container,
    );
    ws.addDocument(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "0", code: "Digit0", ctrlKey: true }));

    expect(fitToScreenAndRender).toHaveBeenCalledWith(false);

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("merges the active layer down with Ctrl+E", async () => {
    const mergedBitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    installOffscreenCanvasMock(mergedBitmap);

    const session = WorkspaceManager.createBlankDocument("merge-keyboard", "Merge Keyboard", 800, 600);
    const topLayer = session.engine.addLayer("Top");
    session.engine.setActiveLayer(topLayer.id);

    const { ws, renderer, scheduler, dispose } = renderKeyboardHarness(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", ctrlKey: true }));

    const mergedLayer = session.engine.getLayers()[0];
    expect(session.engine.getLayers()).toHaveLength(1);
    expect(mergedLayer.imageBitmap).toBe(mergedBitmap);
    expect(renderer.destroyTexture).toHaveBeenCalledWith(topLayer.id);
    expect(renderer.uploadImage).toHaveBeenCalledWith(mergedLayer.id, mergedBitmap);
    expect(ws.getActiveHistory()?.canUndo()).toBe(true);
    expect(scheduler.requestRender).toHaveBeenCalled();

    dispose();
  });

  it("flattens all layers with Ctrl+Shift+E", async () => {
    const flattenedBitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    installOffscreenCanvasMock(flattenedBitmap);

    const session = WorkspaceManager.createBlankDocument("flatten-keyboard", "Flatten Keyboard", 800, 600);
    session.engine.addLayer("Layer 2");
    session.engine.addLayer("Layer 3");

    const { ws, renderer, scheduler, dispose } = renderKeyboardHarness(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "E", ctrlKey: true, shiftKey: true }));

    const flattenedLayer = session.engine.getLayers()[0];
    expect(session.engine.getLayers()).toHaveLength(1);
    expect(flattenedLayer.imageBitmap).toBe(flattenedBitmap);
    expect(renderer.uploadImage).toHaveBeenCalledWith(flattenedLayer.id, flattenedBitmap);
    expect(ws.getActiveHistory()?.canUndo()).toBe(true);
    expect(scheduler.requestRender).toHaveBeenCalled();

    dispose();
  });

  it("adds a new layer with Ctrl+Shift+N", async () => {
    const session = WorkspaceManager.createBlankDocument("new-layer-keyboard", "New Layer", 800, 600);
    const initialCount = session.engine.getLayers().length;

    const { ws, scheduler, dispose } = renderKeyboardHarness(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "N", ctrlKey: true, shiftKey: true }));

    expect(session.engine.getLayers()).toHaveLength(initialCount + 1);
    expect(ws.getActiveHistory()?.canUndo()).toBe(true);
    expect(scheduler.requestRender).toHaveBeenCalled();

    dispose();
  });

  it("moves the active layer up in the stack with Ctrl+]", async () => {
    const session = WorkspaceManager.createBlankDocument("move-up-keyboard", "Move Up", 800, 600);
    const middle = session.engine.addLayer("Middle");
    session.engine.addLayer("Top");
    session.engine.setActiveLayer(middle.id);

    const { ws, scheduler, dispose } = renderKeyboardHarness(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    const beforeIdx = session.engine.getLayers().findIndex((l) => l.id === middle.id);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "]", ctrlKey: true }));
    const afterIdx = session.engine.getLayers().findIndex((l) => l.id === middle.id);

    expect(afterIdx).toBe(beforeIdx - 1);
    expect(ws.getActiveHistory()?.canUndo()).toBe(true);
    expect(scheduler.requestRender).toHaveBeenCalled();

    dispose();
  });

    it("moves the active layer down in the stack with Ctrl+[ (never below Background)", async () => {
      const session = WorkspaceManager.createBlankDocument("move-down-keyboard", "Move Down", 800, 600);
      const middle = session.engine.addLayer("Middle");
      session.engine.addLayer("Top");
      session.engine.setActiveLayer(middle.id);

      const { ws, scheduler, dispose } = renderKeyboardHarness(session);
      await new Promise((resolve) => setTimeout(resolve, 0));

      const beforeIdx = session.engine.getLayers().findIndex((l) => l.id === middle.id);
      const bgIdxBefore = session.engine.getLayers().findIndex((l) => l.isBackground);
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "[", ctrlKey: true }));
      const afterIdx = session.engine.getLayers().findIndex((l) => l.id === middle.id);
      const bgIdxAfter = session.engine.getLayers().findIndex((l) => l.isBackground);

      // Middle starts just above the Background; "move down" must NOT push
      // it below the (opaque) Background. It stays just above it, and the
      // Background remains the bottommost layer.
      expect(afterIdx).toBe(beforeIdx);
      expect(afterIdx).toBeLessThan(bgIdxAfter);
      expect(bgIdxAfter).toBe(session.engine.getLayers().length - 1);
      expect(ws.getActiveHistory()?.canUndo()).toBe(true);
      expect(scheduler.requestRender).toHaveBeenCalled();

      dispose();
    });

  it("flips the active layer horizontally with Ctrl+G", async () => {
    const session = WorkspaceManager.createBlankDocument("flip-h-keyboard", "Flip H", 800, 600);
    const layer = session.engine.addLayer("Top");
    session.engine.setActiveLayer(layer.id);
    const initialFlipH = layer.transform.flipH;

    const { ws, scheduler, dispose } = renderKeyboardHarness(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "g", ctrlKey: true }));

    expect(session.engine.getLayer(layer.id)?.transform.flipH).toBe(!initialFlipH);
    expect(session.engine.getLayer(layer.id)?.transform.flipV).toBe(false);
    expect(ws.getActiveHistory()?.canUndo()).toBe(true);
    expect(scheduler.requestRender).toHaveBeenCalled();

    dispose();
  });

  it("flips the active layer vertically with Ctrl+Shift+G", async () => {
    const session = WorkspaceManager.createBlankDocument("flip-v-keyboard", "Flip V", 800, 600);
    const layer = session.engine.addLayer("Top");
    session.engine.setActiveLayer(layer.id);
    const initialFlipV = layer.transform.flipV;

    const { ws, scheduler, dispose } = renderKeyboardHarness(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "G", ctrlKey: true, shiftKey: true }));

    expect(session.engine.getLayer(layer.id)?.transform.flipV).toBe(!initialFlipV);
    expect(session.engine.getLayer(layer.id)?.transform.flipH).toBe(false);
    expect(ws.getActiveHistory()?.canUndo()).toBe(true);
    expect(scheduler.requestRender).toHaveBeenCalled();

    dispose();
  });

  it("deletes the active layer with Delete key", async () => {
    const session = WorkspaceManager.createBlankDocument("delete-keyboard", "Delete", 800, 600);
    const top = session.engine.addLayer("Top");
    session.engine.setActiveLayer(top.id);
    const initialCount = session.engine.getLayers().length;

    const { ws, scheduler, dispose } = renderKeyboardHarness(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete" }));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(session.engine.getLayers()).toHaveLength(initialCount - 1);
    expect(session.engine.getLayer(top.id)).toBeUndefined();
    expect(ws.getActiveHistory()?.canUndo()).toBe(true);
    expect(scheduler.requestRender).toHaveBeenCalled();

    dispose();
  });

  it("deletes the active layer with Backspace key", async () => {
    const session = WorkspaceManager.createBlankDocument("backspace-keyboard", "Backspace", 800, 600);
    const top = session.engine.addLayer("Top");
    session.engine.setActiveLayer(top.id);
    const initialCount = session.engine.getLayers().length;

    const { dispose } = renderKeyboardHarness(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace" }));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(session.engine.getLayers()).toHaveLength(initialCount - 1);
    expect(session.engine.getLayer(top.id)).toBeUndefined();

    dispose();
  });

  it("does not delete the only remaining layer", async () => {
    const session = WorkspaceManager.createBlankDocument("delete-last-keyboard", "Delete Last", 800, 600);
    const onlyId = session.engine.getActiveLayerId()!;

    const { dispose } = renderKeyboardHarness(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete" }));

    expect(session.engine.getLayers()).toHaveLength(1);
    expect(session.engine.getLayer(onlyId)).toBeDefined();

    dispose();
  });

  it("sets active layer opacity to 50% with digit 5", async () => {
    const session = WorkspaceManager.createBlankDocument("opacity-keyboard", "Opacity", 800, 600);
    const layer = session.engine.addLayer("Top");
    session.engine.setActiveLayer(layer.id);

    const { ws, scheduler, dispose } = renderKeyboardHarness(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "5" }));

    expect(session.engine.getLayer(layer.id)?.opacity).toBeCloseTo(0.5);
    expect(ws.getActiveHistory()?.canUndo()).toBe(true);
    expect(scheduler.requestRender).toHaveBeenCalled();

    dispose();
  });

  it("sets active layer opacity to 100% with digit 0", async () => {
    const session = WorkspaceManager.createBlankDocument("opacity-0-keyboard", "Opacity 0", 800, 600);
    const layer = session.engine.addLayer("Top");
    session.engine.setActiveLayer(layer.id);
    session.engine.setLayerOpacity(layer.id, 0.3);

    const { dispose } = renderKeyboardHarness(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "0" }));

    expect(session.engine.getLayer(layer.id)?.opacity).toBeCloseTo(1.0);

    dispose();
  });

  it("does not commit history when pressing the same opacity digit twice", async () => {
    const session = WorkspaceManager.createBlankDocument("opacity-noop", "Opacity NoOp", 800, 600);
    const layer = session.engine.addLayer("Top");
    session.engine.setActiveLayer(layer.id);
    session.engine.setLayerOpacity(layer.id, 0.5);

    const { ws, dispose } = renderKeyboardHarness(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "5" }));

    expect(session.engine.getLayer(layer.id)?.opacity).toBeCloseTo(0.5);
    expect(ws.getActiveHistory()?.canUndo()).toBe(false);
    expect(ws.getActiveHistory()?.getUndoCount()).toBe(0);

    dispose();
  });

  it("commits when pressing a different opacity digit but skips on repeat", async () => {
    const session = WorkspaceManager.createBlankDocument("opacity-mixed", "Opacity Mixed", 800, 600);
    const layer = session.engine.addLayer("Top");
    session.engine.setActiveLayer(layer.id);
    session.engine.setLayerOpacity(layer.id, 0.5);

    const { ws, dispose } = renderKeyboardHarness(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "5" }));
    expect(ws.getActiveHistory()?.getUndoCount()).toBe(0);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "7" }));
    expect(session.engine.getLayer(layer.id)?.opacity).toBeCloseTo(0.7);
    expect(ws.getActiveHistory()?.getUndoCount()).toBe(1);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "7" }));
    expect(ws.getActiveHistory()?.getUndoCount()).toBe(1);

    dispose();
  });
});

describe("useCanvasKeyboard keyup/blur wiring", () => {
  let container: HTMLDivElement;
  let dispose: () => void;
  let setIsSpacePressed: (...args: unknown[]) => void;
  let setIsAltPressed: (...args: unknown[]) => void;
  let setIsPanning: (...args: unknown[]) => void;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    setIsSpacePressed = vi.fn();
    setIsAltPressed = vi.fn();
    setIsPanning = vi.fn();
  });

  afterEach(() => {
    dispose?.();
    container.remove();
  });

  it("keyup Space calls setIsSpacePressed(false)", () => {
    const ws = new WorkspaceManager();
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <KeyUpHarness
            setIsSpacePressed={setIsSpacePressed}
            setIsAltPressed={setIsAltPressed}
            setIsPanning={setIsPanning}
          />
        </EditorProvider>
      ),
      container,
    );
    window.dispatchEvent(new KeyboardEvent("keyup", { code: "Space", key: " " }));
    expect(setIsSpacePressed).toHaveBeenCalledWith(false);
  });

  it("keyup Alt calls setIsAltPressed(false)", () => {
    const ws = new WorkspaceManager();
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <KeyUpHarness
            setIsSpacePressed={setIsSpacePressed}
            setIsAltPressed={setIsAltPressed}
            setIsPanning={setIsPanning}
          />
        </EditorProvider>
      ),
      container,
    );
    window.dispatchEvent(new KeyboardEvent("keyup", { key: "Alt" }));
    expect(setIsAltPressed).toHaveBeenCalledWith(false);
  });

  it("window blur resets space, panning, and alt", () => {
    const ws = new WorkspaceManager();
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <KeyUpHarness
            setIsSpacePressed={setIsSpacePressed}
            setIsAltPressed={setIsAltPressed}
            setIsPanning={setIsPanning}
          />
        </EditorProvider>
      ),
      container,
    );
    window.dispatchEvent(new Event("blur"));
    expect(setIsSpacePressed).toHaveBeenCalledWith(false);
    expect(setIsPanning).toHaveBeenCalledWith(false);
    expect(setIsAltPressed).toHaveBeenCalledWith(false);
  });

  it("cleans up listeners on unmount", () => {
    const ws = new WorkspaceManager();
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const scheduler = { requestRender: vi.fn() };
    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <KeyUpHarness
            setIsSpacePressed={setIsSpacePressed}
            setIsAltPressed={setIsAltPressed}
            setIsPanning={setIsPanning}
          />
        </EditorProvider>
      ),
      container,
    );
    dispose(); // unmount
    dispose = () => {};

    window.dispatchEvent(new KeyboardEvent("keyup", { code: "Space", key: " " }));
    expect(setIsSpacePressed).not.toHaveBeenCalled();
  });
});

function KeyUpHarness(props: {
  setIsSpacePressed: (...args: unknown[]) => void;
  setIsAltPressed: (...args: unknown[]) => void;
  setIsPanning: (...args: unknown[]) => void;
}) {
  useCanvasKeyboard({
    isSpacePressed: () => false,
    setIsSpacePressed: props.setIsSpacePressed,
    isAltPressed: () => false,
    setIsAltPressed: props.setIsAltPressed,
    isPanning: () => false,
    setIsPanning: props.setIsPanning,
    stopMomentum: vi.fn(),
    fitToScreenAndRender: vi.fn(),
    syncViewport: vi.fn(),
    getCanvasContainerRef: () => undefined,
  });
  return null;
}

// ─── Fill layer keyboard shortcuts (Alt+Del / Ctrl+Del) ─────────────────────

/** OffscreenCanvas mock that records the fill color passed to `fillStyle`. */
function installFillCaptureMock(fillRef: { color: string | null }) {
  vi.stubGlobal("OffscreenCanvas", class {
    width: number;
    height: number;
    constructor(width: number, height: number) {
      this.width = width;
      this.height = height;
    }
    getContext() {
      return {
        _fs: "" as string,
        set fillStyle(v: string) { fillRef.color = v; },
        get fillStyle() { return fillRef.color ?? ""; },
        fillRect: vi.fn(),
        drawImage: vi.fn(),
        save: vi.fn(),
        restore: vi.fn(),
        translate: vi.fn(),
        rotate: vi.fn(),
        scale: vi.fn(),
        globalAlpha: 1,
        globalCompositeOperation: "source-over",
      };
    }
    transferToImageBitmap() {
      return { width: this.width, height: this.height, close: () => {} } as ImageBitmap;
    }
  });
}

function FillKeyboardHarness(props: {
  captureEditor: (e: ReturnType<typeof useEditor>) => void;
}) {
  const editor = useEditor();
  props.captureEditor(editor);
  useCanvasKeyboard({
    isSpacePressed: () => false,
    setIsSpacePressed: vi.fn(),
    isAltPressed: () => false,
    setIsAltPressed: vi.fn(),
    isPanning: () => false,
    setIsPanning: vi.fn(),
    stopMomentum: vi.fn(),
    fitToScreenAndRender: vi.fn(),
    syncViewport: vi.fn(),
    getCanvasContainerRef: () => undefined,
  });
  return null;
}

function setupFill(renderer: { uploadImage: ReturnType<typeof vi.fn>; destroyTexture: ReturnType<typeof vi.fn> }, fillRef: { color: string | null }) {
  installFillCaptureMock(fillRef);
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument("fill-doc", "Fill", 800, 600);
  ws.addDocument(session);
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);
  let captured: ReturnType<typeof useEditor> | undefined;

  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
        <FillKeyboardHarness captureEditor={(e) => { captured = e; }} />
      </EditorProvider>
    ),
    container,
  );

  return {
    ws,
    scheduler,
    container,
    getEditor: () => captured!,
    dispose: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

describe("fill layer keyboard shortcuts (Alt+Del / Ctrl+Del)", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    clearRegistry();
  });

  it("Alt+Delete fills the active layer with the foreground color", () => {
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const fillRef = { color: null as string | null };
    const { getEditor, scheduler, dispose } = setupFill(renderer, fillRef);

    const editor = getEditor();
    editor.setFgColor("#ff0000");
    const engine = editor.workspace.getActiveEngine()!;
    const activeId = engine.getActiveLayerId();
    const commitSpy = vi.spyOn(editor.workspace.getActiveHistory()!, "commit");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", altKey: true, bubbles: true }));

    // Fill used the foreground color through real production code.
    expect(fillRef.color).toBe("#ff0000");
    expect(renderer.uploadImage).toHaveBeenCalledWith(activeId, expect.anything());
    expect(scheduler.requestRender).toHaveBeenCalled();
    // History committed BEFORE the layer bitmap was mutated/uploaded.
    expect(commitSpy).toHaveBeenCalled();
    expect(commitSpy.mock.invocationCallOrder[0]).toBeLessThan(
      (renderer.uploadImage as ReturnType<typeof vi.fn>).mock.invocationCallOrder[0],
    );
    expect(engine.getLayer(activeId!)?.imageBitmap).toBeTruthy();
    dispose();
  });

  it("Ctrl+Delete fills the active layer with the background color", () => {
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const fillRef = { color: null as string | null };
    const { getEditor, dispose } = setupFill(renderer, fillRef);

    const editor = getEditor();
    editor.setBgColor("#00ff00");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", ctrlKey: true, bubbles: true }));

    expect(fillRef.color).toBe("#00ff00");
    expect(renderer.uploadImage).toHaveBeenCalled();
    dispose();
  });

  it("Alt+Backspace also fills with the foreground color", () => {
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const fillRef = { color: null as string | null };
    const { getEditor, dispose } = setupFill(renderer, fillRef);

    const editor = getEditor();
    editor.setFgColor("#123456");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace", altKey: true, bubbles: true }));

    expect(fillRef.color).toBe("#123456");
    expect(renderer.uploadImage).toHaveBeenCalled();
    dispose();
  });

  it("plain Delete/Backspace does NOT fill the layer (falls through to delete behavior)", () => {
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const fillRef = { color: null as string | null };
    const { dispose } = setupFill(renderer, fillRef);

    // Single-layer (Background) document: plain Delete has nothing to delete
    // and must NOT trigger a fill (which would be a wrong/accidental mutation).
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true }));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace", bubbles: true }));

    expect(fillRef.color).toBeNull();
    expect(renderer.uploadImage).not.toHaveBeenCalled();
    dispose();
  });

  it("Alt+Delete with no active layer shows a warning toast (no silent no-op)", () => {
    const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    const fillRef = { color: null as string | null };
    const { getEditor, dispose } = setupFill(renderer, fillRef);

    getEditor().workspace.getActiveEngine()!.setActiveLayer(null);
    const toastSpy = vi.spyOn(Toast, "showToast");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", altKey: true, bubbles: true }));

    expect(toastSpy).toHaveBeenCalledWith("No editable layer selected", "warn");
    expect(fillRef.color).toBeNull();
    dispose();
  });
});

// ─── Transform session guards: destructive shortcuts must not fire ───────────
// During an active transform session, fill / merge / flatten / duplicate / new
// layer / reorder / delete are all blocked. They would commit to global history
// but Ctrl+Z during a transform only reaches the session's local undo stack,
// leaving the action un-undoable until the session ends.

function setupTransformSession() {
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument("transform-guards", "Transform", 800, 600);
  ws.addDocument(session);
  const engine = session.engine;
  const extra = engine.addLayer("Top");
  engine.setActiveLayer(extra.id);
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);
  let captured: ReturnType<typeof useEditor> | undefined;
  const fillRef = { color: null as string | null };
  installFillCaptureMock(fillRef);

  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
        <FillKeyboardHarness captureEditor={(e) => { captured = e; }} />
      </EditorProvider>
    ),
    container,
  );

  const editor = captured!;
  // Establish a transform session with a changed transform so commit produces a history entry.
  engine.transformLayer(extra.id, { x: 50, y: 25, scaleX: 1.5, scaleY: 0.8, rotation: 10, flipH: false, flipV: false });
  editor.setLayerTransformSession({
    documentId: ws.getActiveDocumentId()!,
    layerId: extra.id,
    originalSnapshot: engine.snapshot(),
    originalTransform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    mode: "resize",
    lockRatio: false,
    startedAt: Date.now(),
  });

  return {
    ws, engine, renderer, scheduler, editor, fillRef, extra,
    dispose: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

describe("transform session blocks destructive shortcuts", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    clearRegistry();
  });

  it("Alt+Backspace does NOT fill while a transform session is active", () => {
    const { fillRef, dispose } = setupTransformSession();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace", altKey: true, bubbles: true }));
    expect(fillRef.color).toBeNull();
    dispose();
  });

  it("Ctrl+E (merge down) is blocked during transform session", () => {
    const { engine, dispose } = setupTransformSession();
    const before = engine.getLayers().length;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", ctrlKey: true, bubbles: true }));
    expect(engine.getLayers().length).toBe(before);
    dispose();
  });

  it("Ctrl+Shift+E (flatten) is blocked during transform session", () => {
    const { engine, dispose } = setupTransformSession();
    const before = engine.getLayers().length;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "E", ctrlKey: true, shiftKey: true, bubbles: true }));
    expect(engine.getLayers().length).toBe(before);
    dispose();
  });

  it("Ctrl+J (duplicate) is blocked during transform session", () => {
    const { engine, dispose } = setupTransformSession();
    const before = engine.getLayers().length;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "j", ctrlKey: true, bubbles: true }));
    expect(engine.getLayers().length).toBe(before);
    dispose();
  });

  it("Ctrl+Shift+N (new layer) is blocked during transform session", () => {
    const { engine, dispose } = setupTransformSession();
    const before = engine.getLayers().length;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "n", ctrlKey: true, shiftKey: true, bubbles: true }));
    expect(engine.getLayers().length).toBe(before);
    dispose();
  });

  it("Ctrl+] (reorder up) is blocked during transform session", () => {
    const { engine, extra, dispose } = setupTransformSession();
    const beforeIdx = engine.getLayers().findIndex((l) => l.id === extra.id);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "]", ctrlKey: true, bubbles: true }));
    expect(engine.getLayers().findIndex((l) => l.id === extra.id)).toBe(beforeIdx);
    dispose();
  });

  it("Backspace (delete layer) is blocked during transform session", () => {
    const { engine, extra, dispose } = setupTransformSession();
    const before = engine.getLayers().length;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace", bubbles: true }));
    expect(engine.getLayers().length).toBe(before);
    expect(engine.getLayer(extra.id)).toBeDefined();
    dispose();
  });

  it("Arrow nudge WORKS during transform session (1px, Shift=10px)", () => {
    const { engine, extra, dispose } = setupTransformSession();
    const beforeX = engine.getLayer(extra.id)!.transform.x;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    // Nudge moves the selected layer even while the transform overlay is live.
    expect(engine.getLayer(extra.id)!.transform.x).toBe(beforeX + 1);

    const beforeY = engine.getLayer(extra.id)!.transform.y;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", shiftKey: true, bubbles: true }));
    expect(engine.getLayer(extra.id)!.transform.y).toBe(beforeY + 10);
    dispose();
  });

  it("Ctrl+G (flip) is still allowed during transform (it mutates the transform)", () => {
    const { engine, extra, dispose } = setupTransformSession();
    const beforeFlipH = engine.getLayer(extra.id)!.transform.flipH;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "g", ctrlKey: true, bubbles: true }));
    expect(engine.getLayer(extra.id)!.transform.flipH).toBe(!beforeFlipH);
    dispose();
  });

  it("tool switch during transform auto-commits the session (history entry created)", () => {
    const { ws, editor, dispose } = setupTransformSession();
    const history = ws.getActiveHistory()!;
    const undoBefore = history.getUndoCount();

    // Switch tool via setActiveTool — mimics keyboard tool shortcut path.
    editor.setActiveTool("brush");

    // Transform should be committed (history entry added) and session cleared.
    expect(history.getUndoCount()).toBe(undoBefore + 1);
    expect(editor.layerTransformSession()).toBeNull();
    dispose();
  });
});

// ─── Crop tool guards: destructive shortcuts must not fire ───────────────────
// While the crop tool is active, fill / delete would commit to global history
// but Ctrl+Z only reaches the crop mini-undo stack, leaving the action
// un-undoable until the crop is applied. Same regression class as the
// transform session guard above.

function setupCropMode() {
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument("crop-guards", "Crop", 800, 600);
  ws.addDocument(session);
  const engine = session.engine;
  const extra = engine.addLayer("Top");
  engine.setActiveLayer(extra.id);
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);
  let captured: ReturnType<typeof useEditor> | undefined;
  const fillRef = { color: null as string | null };
  installFillCaptureMock(fillRef);

  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
        <FillKeyboardHarness captureEditor={(e) => { captured = e; }} />
      </EditorProvider>
    ),
    container,
  );

  const editor = captured!;
  editor.setActiveTool("crop");

  return {
    ws, engine, renderer, scheduler, editor, fillRef, extra,
    dispose: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

describe("crop tool blocks destructive shortcuts", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    clearRegistry();
  });

  it("Alt+Backspace does NOT fill while the crop tool is active", () => {
    const { fillRef, dispose } = setupCropMode();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace", altKey: true, bubbles: true }));
    expect(fillRef.color).toBeNull();
    dispose();
  });

  it("Backspace (delete layer) is blocked while the crop tool is active", () => {
    const { engine, extra, dispose } = setupCropMode();
    const before = engine.getLayers().length;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace", bubbles: true }));
    expect(engine.getLayers().length).toBe(before);
    expect(engine.getLayer(extra.id)).toBeDefined();
    dispose();
  });
});
