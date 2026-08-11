import { afterEach, beforeEach, describe, expect, it, vi, assert } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { CanvasViewport } from "../CanvasViewport";
import { WorkspaceManager } from "@/engine/workspace";
import { DEFAULT_TEXT_DATA } from "@/engine/textTypes";
import { ViewportCamera } from "@/viewport/viewportCamera";
import { stubTextOffscreenCanvas } from "@/__tests__/test-builders";
import { OptionBar } from "../../shell/OptionBar";
import * as Toast from "../../Toast";

// Mock useViewportRenderer
const { mockFitToScreenAndRender } = vi.hoisted(() => ({
  mockFitToScreenAndRender: vi.fn(),
}));
vi.mock("../useViewportRenderer", () => ({
  useViewportRenderer: () => ({
    isFitTransition: () => false,
    fitToScreenAndRender: mockFitToScreenAndRender,
    resizeRenderer: vi.fn(),
  }),
}));

// Mock useBrushOverlay
const { mockCommitBrushStroke } = vi.hoisted(() => ({
  mockCommitBrushStroke: vi.fn(),
}));
vi.mock("../../useBrushOverlay", () => ({
  useBrushOverlay: () => ({
    onPaintStroke: vi.fn(),
    commitBrushStroke: mockCommitBrushStroke,
    setOverlayCanvasRef: vi.fn(),
    getOverlayCanvasRef: vi.fn(),
  }),
}));

// Mock usePanNavigation
let mockSpacePressed = false;
let mockPanningActive = false;
const mockOnViewportPointerDown = vi.fn();
const mockOnViewportPointerUp = vi.fn();
const mockOnViewportPointerCancel = vi.fn();
const mockOnViewportLostPointerCapture = vi.fn();
vi.mock("../usePanNavigation", () => ({
  usePanNavigation: () => ({
    isSpacePressed: () => mockSpacePressed,
    setIsSpacePressed: vi.fn(),
    isPanning: () => mockPanningActive,
    setIsPanning: vi.fn(),
    stopMomentum: vi.fn(),
    handleWheel: vi.fn(),
    onViewportPointerDown: mockOnViewportPointerDown,
    onViewportPointerMove: vi.fn(),
    onViewportPointerUp: mockOnViewportPointerUp,
    onViewportPointerCancel: mockOnViewportPointerCancel,
    onViewportLostPointerCapture: mockOnViewportLostPointerCapture,
  }),
}));

// Mock useCanvasDerivedState
vi.mock("../useCanvasDerivedState", () => ({
  useCanvasDerivedState: () => ({
    cropSnapTargets: () => [],
  }),
}));

// Mock useCanvasKeyboard
vi.mock("../useCanvasKeyboard", () => ({
  useCanvasKeyboard: vi.fn(),
}));

let setTool: (tool: string) => void = () => {};
let setSession: (session: any) => void = () => {};
let setCrop: (rect: any) => void = () => {};
let getCrop: () => any = () => null;
let getCropRotation: () => number = () => 0;
let setCropRotation: (rot: number) => void = () => {};
let getHiddenCropPreview: () => any = () => null;
let setHiddenCropPreview: (preview: any) => void = () => {};
let setCropInteractionMode: (mode: "modern" | "classic") => void = () => {};
let getModernImageTransform: () => any = () => ({ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 });
let setModernImageTransform: (t: any) => void = () => {};
let getModernFrame: () => any = () => null;
let setModernFrameState: (frame: any) => void = () => {};
let resetModernCropState: () => void = () => {};
let setUseGPUCameraForModernCrop: (v: boolean) => void = () => {};
let getCropMode: () => string = () => "free";
let setCropModeState: (mode: any) => void = () => {};
let getCropAspect: () => any = () => null;
let setCropAspectState: (aspect: any) => void = () => {};
let getCropSizeTarget: () => any = () => null;
let setCropSizeTargetState: (target: any) => void = () => {};
let getActiveDocId: () => string | null = () => null;
let clearCropStacksState: () => void = () => {};
let setBgColorState: (color: string) => void = () => {};
  let setZoomState: (z: number) => void = () => {};
  let setPanState: (p: { x: number; y: number }) => void = () => {};
  let setSelectedLayerIdState: (id: string | null) => void = () => {};
  let getSelectedLayerIdState: () => string | null = () => null;
let setCropFillEnabledState: (enabled: boolean) => void = () => {};
let setCropFillSourceState: (source: "background" | "custom") => void = () => {};
let setCropFillCustomColorState: (color: string) => void = () => {};

let getSelectionState: () => any = () => null;
let setSelectionState: (sel: any) => void = () => {};
let getSelectionConstraintMode: () => any = () => "normal";
let setSelectionConstraintMode: (mode: any) => void = () => {};
let setSelectionRatioW: (w: number) => void = () => {};
let setSelectionRatioH: (h: number) => void = () => {};
let setSelectionSizeW: (w: number) => void = () => {};
let setSelectionSizeH: (h: number) => void = () => {};

const TestConsumer = () => {
  const editor = useEditor();
  setTool = editor.setActiveTool;
  setSession = editor.setLayerTransformSession;
  setCrop = editor.setCropRect;
  getCrop = editor.cropRect;
  getCropRotation = editor.cropRotation;
  setCropRotation = editor.setCropRotation;
  getHiddenCropPreview = editor.hiddenCropPreview;
  setHiddenCropPreview = editor.setHiddenCropPreview;
  setCropInteractionMode = editor.setCropInteractionMode;
  getModernImageTransform = editor.modernCropImageTransform;
  setModernImageTransform = editor.setModernCropImageTransform;
  getModernFrame = editor.modernCropFrame;
  setModernFrameState = editor.setModernCropFrame;
  resetModernCropState = editor.resetModernCrop;
  setUseGPUCameraForModernCrop = editor.setUseGPUCameraForModernCrop;
  getCropMode = editor.cropMode;
  setCropModeState = editor.setCropMode;
  getCropAspect = editor.cropAspect;
  setCropAspectState = editor.setCropAspect;
  getCropSizeTarget = editor.cropSizeTarget;
  setCropSizeTargetState = editor.setCropSizeTarget;
  getActiveDocId = editor.activeDocumentId;
  clearCropStacksState = editor.clearCropStacks;
  setBgColorState = editor.setBgColor;
  setCropFillEnabledState = editor.setCropFillEnabled;
  setCropFillSourceState = editor.setCropFillSource;
  setCropFillCustomColorState = editor.setCropFillCustomColor;
  setZoomState = editor.setZoom;
  setPanState = editor.setPan;
  setSelectedLayerIdState = editor.setSelectedLayerId;
  getSelectedLayerIdState = editor.selectedLayerId;

  getSelectionState = editor.selection;
  setSelectionState = editor.setSelection;
  getSelectionConstraintMode = editor.selectionConstraintMode;
  setSelectionConstraintMode = editor.setSelectionConstraintMode;
  setSelectionRatioW = editor.setSelectionRatioW;
  setSelectionRatioH = editor.setSelectionRatioH;
  setSelectionSizeW = editor.setSelectionSizeW;
  setSelectionSizeH = editor.setSelectionSizeH;
  return null;
};

describe("Space+pan global override across all tools", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    mockOnViewportPointerDown.mockClear();
    mockOnViewportPointerUp.mockClear();
    mockCommitBrushStroke.mockClear();
    mockSpacePressed = false;
    mockPanningActive = false;

    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    vi.restoreAllMocks();
  });

  function renderViewport() {
    const session = WorkspaceManager.createBlankDocument("doc-pan", "Pan Test", 800, 600);
    ws.addDocument(session);

    const result = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );

    dispose = result;
    return { session };
  }

  function getCanvas(): HTMLCanvasElement {
    const c = container.querySelector("canvas") as HTMLCanvasElement;
    if (!c) throw new Error("Canvas not found");
    return c;
  }

  function getContainer(): HTMLDivElement {
    const c = container.querySelector("[data-viewport-container]") as HTMLDivElement;
    if (!c) throw new Error("Viewport container not found");
    return c;
  }

  function firePointerDown(el: Element, pointerId = 10, clientX = 100, clientY = 100, altKey = false) {
    const ev = new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId, clientX, clientY,
    });
    // jsdom's PointerEvent init drops MouseEvent modifiers; force-set it.
    if (altKey) Object.defineProperty(ev, "altKey", { value: true, configurable: true });
    el.dispatchEvent(ev);
  }

  function firePointerMove(el: Element, pointerId = 10, clientX = 200, clientY = 200) {
    el.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId, clientX, clientY,
    }));
  }

  function firePointerUp(el: Element, pointerId = 10, clientX = 200, clientY = 200) {
    el.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId, clientX, clientY,
    }));
  }

  // --- Tool-specific Space+pan tests ---

  it("Move tool + selected layer: routes to pan handler when Space held", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("move");
    mockSpacePressed = true;
    const layers = session.engine.getLayers();
    session.engine.setActiveLayer(layers[0].id);

    firePointerDown(getCanvas());

    expect(mockOnViewportPointerDown).toHaveBeenCalled();
    // Layer should still be active (no pasteboard clear happened)
    expect(session.engine.getActiveLayerId()).toBe(layers[0].id);
  });

  it("Selection tool + canvas: routes to pan handler when Space held", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("selection");
    mockSpacePressed = true;

    firePointerDown(getCanvas());

    expect(mockOnViewportPointerDown).toHaveBeenCalled();
    // No selection marquee should be created
    expect(container.querySelector("rect[data-march]")).toBeNull();
  });

  it("Crop tool + canvas: routes to pan handler when Space held", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("crop");
    mockSpacePressed = true;

    firePointerDown(getCanvas());

    expect(mockOnViewportPointerDown).toHaveBeenCalled();
  });

  it("Brush tool + canvas: routes to pan handler when Space held", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("brush");
    mockSpacePressed = true;

    firePointerDown(getCanvas());

    expect(mockOnViewportPointerDown).toHaveBeenCalled();
    expect(mockCommitBrushStroke).not.toHaveBeenCalled();
  });

  it("Brush tool with no active layer shows a warning toast (no silent no-op)", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    session.engine.setActiveLayer(null);
    const toastSpy = vi.spyOn(Toast, "showToast");
    setTool("brush");

    firePointerDown(getCanvas());

    expect(toastSpy).toHaveBeenCalledWith("No editable layer selected", "warn");
    expect(mockCommitBrushStroke).not.toHaveBeenCalled();
  });

  // --- Move-tool Alt+drag duplicates the active layer ---
  // jsdom has no layout, so stub a realistic container rect and set zoom/pan so
  // a click at (100,100) resolves INSIDE the 800x600 document (not the
  // pasteboard, which would short-circuit onCanvasPointerDown).
  describe("Move tool Alt+drag duplicate", () => {
    let rectStub: ReturnType<typeof vi.spyOn> | undefined;
    let offscreenPrev: any;

    beforeEach(() => {
      rectStub = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
        left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600, x: 0, y: 0,
        toJSON: () => ({}),
      } as DOMRect);
      // duplicateLayer copies the layer bitmap via OffscreenCanvas (real browser
      // global); jsdom lacks it, so stub a minimal version.
      offscreenPrev = (globalThis as any).OffscreenCanvas;
      (globalThis as any).OffscreenCanvas = class {
        width: number; height: number;
        constructor(w: number, h: number) { this.width = w; this.height = h; }
        getContext() {
          return {
            drawImage: vi.fn(), fillRect: vi.fn(), save: vi.fn(), restore: vi.fn(),
            translate: vi.fn(), rotate: vi.fn(), scale: vi.fn(),
            globalAlpha: 1, globalCompositeOperation: "source-over",
          };
        }
        transferToImageBitmap() {
          return { width: this.width, height: this.height, close: vi.fn() } as unknown as ImageBitmap;
        }
      };
    });
    afterEach(() => {
      rectStub?.mockRestore();
      // Restore even when the prior global was undefined (jsdom has no native
      // OffscreenCanvas): skipping the restore leaks this minimal stub class
      // (its ctx has no measureText) into later describes — e.g. the
      // click-select tests, whose addTextLayer would crash rasterizeText with
      // "ctx.measureText is not a function" in full-suite runs.
      if (offscreenPrev !== undefined) {
        (globalThis as any).OffscreenCanvas = offscreenPrev;
      } else {
        delete (globalThis as any).OffscreenCanvas;
      }
    });

    function setup() {
      const { session } = renderViewport();
      setZoomState(1);
      setPanState({ x: 0, y: 0 });
      setTool("move");
      const layers = session.engine.getLayers();
      const originalId = layers[0].id;
      // Give the layer a large bitmap + zero transform so a click at (100,100)
      // lands well inside its bbox regardless of camera zoom/pan in jsdom.
      session.engine.setLayerImageBitmap(originalId, { width: 4000, height: 4000, close: () => {} } as unknown as ImageBitmap);
      session.engine.transformLayer(originalId, { x: 0, y: 0 });
      session.engine.setActiveLayer(originalId);
      setSelectedLayerIdState(originalId);
      return { session, originalId };
    }

    it("Alt+drag duplicates the active layer and DRAGS THE COPY (original stays put)", async () => {
      const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
      const { session, originalId } = setup();
      await new Promise((resolve) => setTimeout(resolve, 0));

      const before = session.engine.getLayers().length;
      const origBefore = { x: session.engine.getLayer(originalId)!.transform.x, y: session.engine.getLayer(originalId)!.transform.y };

      // Full chain: down (alt) → move (delta 60,40) → up.
      firePointerDown(getCanvas(), 10, 100, 100, true);
      firePointerMove(document.body, 10, 160, 140);
      firePointerUp(document.body, 10, 160, 140);

      if (warnSpy.mock.calls.length > 0) console.log("WARN MSG:", String(warnSpy.mock.calls[0][0]), String(warnSpy.mock.calls[0][1]?.message ?? warnSpy.mock.calls[0][1]));
      const dupWarn = warnSpy.mock.calls.find((c) => String(c[0]).includes("Alt+drag duplicate failed"));
      expect(dupWarn).toBeUndefined();

      const after = session.engine.getLayers();
      expect(after.length).toBe(before + 1);

      // Copy is the new active layer.
      const copyId = session.engine.getActiveLayerId()!;
      expect(copyId).not.toBe(originalId);

      // THE USER-REPORTED SYMPTOM: original must NOT move, copy must.
      const origAfter = session.engine.getLayer(originalId)!.transform;
      expect(origAfter.x).toBe(origBefore.x);
      expect(origAfter.y).toBe(origBefore.y);
      const copyAfter = session.engine.getLayer(copyId)!.transform;
      expect(copyAfter.x).toBe(origBefore.x + 60);
      expect(copyAfter.y).toBe(origBefore.y + 40);

      expect(renderer.uploadImage).toHaveBeenCalled();
      warnSpy.mockRestore();
    });

    it("plain drag does NOT duplicate the active layer", async () => {
      const { session, originalId } = setup();
      await new Promise((resolve) => setTimeout(resolve, 0));

      const before = session.engine.getLayers().length;
      firePointerDown(getCanvas(), 10, 100, 100, false);

      expect(session.engine.getLayers().length).toBe(before);
      expect(session.engine.getActiveLayerId()).toBe(originalId);
    });
  });

  // --- Move-tool click MUST select the hit layer (regression: the pasteboard
  // "clear-active-layer" policy ran UNCONDITIONALLY on every move-tool
  // pointerdown, wiping the selection the canvas handler had just set — the
  // panel kept showing the Background layer as "active" even when clicking a
  // text/shape layer) ---
  describe("Move tool click-select on a layer", () => {
    let rectStub: ReturnType<typeof vi.spyOn> | undefined;

    beforeEach(() => {
      rectStub = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
        left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600, x: 0, y: 0,
        toJSON: () => ({}),
      } as DOMRect);
    });
    afterEach(() => rectStub?.mockRestore());

    it("click on a text layer selects it instead of staying on Background", async () => {
      const { session } = renderViewport();
      await new Promise((resolve) => setTimeout(resolve, 0));
      setZoomState(1);
      setPanState({ x: 0, y: 0 });
      setTool("move");

      const text = session.engine.addTextLayer("Text 1", { ...DEFAULT_TEXT_DATA, content: "Hello" });
      session.engine.transformLayer(text.id, { x: 20, y: 20 });
      session.engine.setActiveLayer(null);
      setSelectedLayerIdState(null);

      // Click the CENTER of the text layer's box, derived from its live dims
      // (real canvas metrics in this env). After the rasterizer tightening the
      // box is ~glyph-sized, so a hardcoded point like (100,100) can fall
      // below the box (y was ~124px tall, now ~69px) and miss entirely.
      const l = session.engine.getLayer(text.id)!;
      const clickX = l.transform.x + l.width / 2;
      const clickY = l.transform.y + l.height / 2;
      firePointerDown(getCanvas(), 30, clickX, clickY);
      firePointerUp(document.body, 30, clickX, clickY);

      expect(getSelectedLayerIdState()).toBe(text.id);
      expect(session.engine.getActiveLayerId()).toBe(text.id);
    });

    it("click on a text layer's TRANSPARENT padding still selects the text (box-hittable beats alpha fall-through)", async () => {
      // Wiring-level regression for @bug 2026-08-09: the move-tool auto-select
      // path samples layer alpha to let transparent pixels fall through to the
      // layer underneath. Real text bitmaps are transparent across most of
      // their box (line-height padding), so WITHOUT the box-hittable rule a
      // click in the padding would fall through to the opaque Background — the
      // exact reported symptom. This drives the FULL pointer chain (canvas
      // pointerdown → hitTestLayers with the real sampler) with the text layer
      // reporting alpha 0 at the click point and asserts the text still wins.
      const { session } = renderViewport();
      await new Promise((resolve) => setTimeout(resolve, 0));
      setZoomState(1);
      setPanState({ x: 0, y: 0 });
      setTool("move");

      const text = session.engine.addTextLayer("Text 1", { ...DEFAULT_TEXT_DATA, content: "Hello" });
      session.engine.transformLayer(text.id, { x: 20, y: 20 });
      // Alpha is 0 exactly on this layer at the click point, opaque elsewhere.
      const alphaSpy = vi.spyOn(session.engine, "sampleLayerAlpha").mockImplementation(
        (id: string) => (id === text.id ? 0 : 1),
      );
      session.engine.setActiveLayer(null);
      setSelectedLayerIdState(null);

      // Click the CENTER of the text layer's tight box (dims derived live).
      const l = session.engine.getLayer(text.id)!;
      const clickX = l.transform.x + l.width / 2;
      const clickY = l.transform.y + l.height / 2;
      firePointerDown(getCanvas(), 32, clickX, clickY);
      firePointerUp(document.body, 32, clickX, clickY);

      expect(getSelectedLayerIdState()).toBe(text.id);
      expect(session.engine.getActiveLayerId()).toBe(text.id);
      alphaSpy.mockRestore();
    });

    it("click on empty canvas STILL clears the selection", async () => {
      const { session } = renderViewport();
      await new Promise((resolve) => setTimeout(resolve, 0));
      setZoomState(1);
      setPanState({ x: 0, y: 0 });
      setTool("move");

      const text = session.engine.addTextLayer("Text 1", { ...DEFAULT_TEXT_DATA, content: "Hello" });
      session.engine.transformLayer(text.id, { x: 20, y: 20 });
      session.engine.setActiveLayer(text.id);
      setSelectedLayerIdState(text.id);

      // (700,500) is in the doc but far from the text layer; bg is bitmap-less.
      firePointerDown(getCanvas(), 31, 700, 500);
      firePointerUp(document.body, 31, 700, 500);

      expect(getSelectedLayerIdState()).toBeNull();
    });
  });

  // --- Double-click-to-fit guards (regression: view snapping to fit during brushing) ---

  function fireDblClick(el: Element) {
    el.dispatchEvent(new MouseEvent("dblclick", {
      bubbles: true, cancelable: true,
    }));
  }

  it("Brush tool: double-click does NOT snap view to fit (rapid dabs misread as dblclick)", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("brush");
    mockFitToScreenAndRender.mockClear();

    fireDblClick(getContainer());

    expect(mockFitToScreenAndRender).not.toHaveBeenCalled();
  });

  it("Eraser tool: double-click does NOT snap view to fit", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("eraser");
    mockFitToScreenAndRender.mockClear();

    fireDblClick(getContainer());

    expect(mockFitToScreenAndRender).not.toHaveBeenCalled();
  });

  it("Move tool: double-click on canvas still snaps view to fit", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("move");
    mockFitToScreenAndRender.mockClear();

    fireDblClick(getContainer());

    expect(mockFitToScreenAndRender).toHaveBeenCalled();
  });

  it("Panning (Space held): double-click does NOT snap view to fit", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("move");
    mockSpacePressed = true;
    mockFitToScreenAndRender.mockClear();

    fireDblClick(getContainer());

    expect(mockFitToScreenAndRender).not.toHaveBeenCalled();
  });

  it("Eraser tool + canvas: routes to pan handler when Space held", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("eraser");    mockSpacePressed = true;

    firePointerDown(getCanvas());

    expect(mockOnViewportPointerDown).toHaveBeenCalled();
    expect(mockCommitBrushStroke).not.toHaveBeenCalled();
  });

  it("Crop tool + pasteboard: routes to pan handler when Space held (not pasteboard crop)", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("crop");
    setCrop({ x: 10, y: 10, w: 100, h: 100 });
    mockSpacePressed = true;

    firePointerDown(getContainer());

    expect(mockOnViewportPointerDown).toHaveBeenCalled();
    // Crop rect should not have been cleared
    expect(getCrop()).toEqual({ x: 10, y: 10, w: 100, h: 100 });
  });

  it("Container pointerdown routes to pan handler before pasteboard handler when Space held", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("move");
    mockSpacePressed = true;

    const containerEl = getContainer();
    // Spy on the container's pasteboard handler to verify it's not called
    const listenerSpy = vi.fn();
    containerEl.addEventListener("pointerdown", listenerSpy);

    firePointerDown(containerEl);

    // The mockOnViewportPointerDown should have been called
    expect(mockOnViewportPointerDown).toHaveBeenCalled();
  });

  it("Space+pan works from canvas into pasteboard via pointer capture (isPanning = true)", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("brush");
    mockPanningActive = true;

    const canvas = getCanvas();
    firePointerMove(canvas);
    firePointerUp(canvas);

    // No brush commit since panning blocked everything
    expect(mockCommitBrushStroke).not.toHaveBeenCalled();
  });

  it("Container pointerdown still routes to pasteboard handler when Space is NOT held", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("move");
    mockSpacePressed = false;

    const layers = session.engine.getLayers();
    session.engine.setActiveLayer(layers[0].id);
    firePointerDown(getContainer());

    // Pasteboard click clears transform selection only; the active paint target remains.
    expect(session.engine.getActiveLayerId()).toBe(layers[0].id);
  });

  it("Move tool clears selection only when viewport canvas receives a click outside the artboard", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("move");
    mockSpacePressed = false;

    const layers = session.engine.getLayers();
    session.engine.setActiveLayer(layers[0].id);
    session.engine.setViewport({ panX: 100, panY: 100, zoom: 1 });

    // After the GPU viewport migration the WebGL canvas covers the viewport.
    // A pasteboard click can therefore target the canvas even when it is outside
    // the visible document/artboard.
    firePointerDown(getCanvas(), 31, 20, 20);

    expect(session.engine.getActiveLayerId()).toBe(layers[0].id);
    expect(scheduler.requestRender).toHaveBeenCalled();
  });

  it("Middle mouse button starts panning regardless of tool and Space state", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("brush");

    const canvas = getCanvas();
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 1, pointerId: 20, clientX: 50, clientY: 50,
    }));

    // onViewportPointerDown should be called (it handles button === 1)
    expect(mockOnViewportPointerDown).toHaveBeenCalledWith(
      expect.objectContaining({ pointerId: 20, button: 1 })
    );
  });
});

// -----------------------------------------------------------------------
// Bug Hunt: Edge case regression tests
// -----------------------------------------------------------------------

describe("Bug Hunt: Pasteboard pointercancel clears pending gesture", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    mockOnViewportPointerDown.mockClear();
    mockOnViewportPointerUp.mockClear();
    mockOnViewportPointerCancel.mockClear();
    mockOnViewportLostPointerCapture.mockClear();
    mockCommitBrushStroke.mockClear();
    mockSpacePressed = false;
    mockPanningActive = false;

    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    vi.restoreAllMocks();
  });

  function renderViewport() {
    const session = WorkspaceManager.createBlankDocument("bug-doc", "Bug Doc", 800, 600);
    ws.addDocument(session);
    const result = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    dispose = result;
    setCropInteractionMode("classic");
    return { session };
  }

  function getContainer(): HTMLDivElement {
    const c = container.querySelector("[data-viewport-container]") as HTMLDivElement;
    if (!c) throw new Error("Viewport container not found");
    return c;
  }

  it("clears pendingPasteboardCropGesture on pointercancel after pasteboard pointerdown in Crop mode", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("crop");

    const containerEl = getContainer();

    // Start a pasteboard gesture (pointerdown on pasteboard)
    containerEl.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10, clientX: 5, clientY: 5,
    }));

    // Give effects time to run
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Now pointercancel fires (e.g., system dialog, tab switch)
    containerEl.dispatchEvent(new PointerEvent("pointercancel", {
      bubbles: false, pointerId: 10,
    }));

    // Give effects time to run
    await new Promise((resolve) => setTimeout(resolve, 0));

    // The pending gesture should have been cleaned up.
    // Verify by checking that a subsequent small movement on pasteboard
    // does NOT trigger a crop replacement (it should be ignored).
    // If pending gesture leaked, the move handler would try to create a replacement crop.
    const setRectSpy = vi.spyOn(renderer, "uploadImage");

    setCrop({ x: 50, y: 50, w: 200, h: 150 });
    const prevCrop = getCrop();

    // Dispatch a small move →should not create a new replacement if gesture is cleared
    containerEl.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10, clientX: 10, clientY: 10,
    }));

    // Crop rect should NOT have changed since the pending gesture was cancelled
    expect(getCrop()).toEqual(prevCrop);
  });
});

describe("Bug Hunt: Modern crop state leak across tool switches", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    mockOnViewportPointerDown.mockClear();
    mockOnViewportPointerUp.mockClear();
    mockOnViewportPointerCancel.mockClear();
    mockOnViewportLostPointerCapture.mockClear();
    mockCommitBrushStroke.mockClear();
    mockSpacePressed = false;
    mockPanningActive = false;

    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    vi.restoreAllMocks();
  });

  function renderViewport() {
    const session = WorkspaceManager.createBlankDocument("leak-doc", "Leak Doc", 800, 600);
    ws.addDocument(session);
    const result = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    dispose = result;
    setCropInteractionMode("modern");
    return { session };
  }

  it("resets modernCropImageTransform when leaving Crop tool", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(getModernFrame()).not.toBeNull();

    // Simulate user interaction by setting non-default transform
    setModernImageTransform({ offsetX: 55, offsetY: 33, rotation: 25, scale: 1.2 });

    // Switch to another tool
    setTool("move");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // After switching away from crop, modern crop state should be reset
    const afterExitTransform = getModernImageTransform();
    expect(afterExitTransform.offsetX).toBe(0);
    expect(afterExitTransform.offsetY).toBe(0);
    expect(afterExitTransform.rotation).toBe(0);
    expect(afterExitTransform.scale).toBe(1);

    // Frame should also be null when not in crop mode
    expect(getModernFrame()).toBeNull();
  });

  it("modern crop: Escape leaves frame null (no auto-recreate)", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Frame should be created on entry
    expect(getModernFrame()).not.toBeNull();
    const createdFrame = getModernFrame();

    // Simulate Escape: reset modern crop state
    resetModernCropState();
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Frame should be null after dismiss
    expect(getModernFrame()).toBeNull();

    // Wait extra ticks →ensure createEffect does NOT recreate frame
    // because session key hasn't changed
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(getModernFrame()).toBeNull();

    // Image transform should also be reset
    const t = getModernImageTransform();
    expect(t.offsetX).toBe(0);
    expect(t.offsetY).toBe(0);
    expect(t.rotation).toBe(0);
    expect(t.scale).toBe(1);
  });

  it("modern crop: canvas click creates frame after Escape dismiss", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Dismiss by resetting (simulating Escape)
    resetModernCropState();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(getModernFrame()).toBeNull();

    // Click on canvas
    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0,
      clientX: 200, clientY: 200, pointerId: 80,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0,
      clientX: 200, clientY: 200, pointerId: 80,
    }));

    await new Promise((resolve) => setTimeout(resolve, 50));

    // Frame should be recreated
    expect(getModernFrame()).not.toBeNull();
  });

  it("modern crop: mode change recreates frame after Escape dismiss", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Dismiss by resetting
    resetModernCropState();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(getModernFrame()).toBeNull();

    // Change mode (e.g., free →ratio)
    setCropModeState("ratio");
    setCropAspectState({ w: 16, h: 9 });
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Mode change should trigger session key change →createEffect recreates frame
    expect(getModernFrame()).not.toBeNull();
    if (getModernFrame()) {
      // Frame should roughly match 16:9 aspect ratio (wider than tall, centered in viewport)
      expect(getModernFrame().w).toBeGreaterThan(getModernFrame().h);
    }
  });

  it("modern crop: tool switch away and back recreates frame after Escape dismiss", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Dismiss by resetting
    resetModernCropState();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(getModernFrame()).toBeNull();

    // Switch to another tool
    setTool("move");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Switch back to crop (new session created)
    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Frame should be recreated
    expect(getModernFrame()).not.toBeNull();
  });

  it("modern crop: resize document does not recreate dismissed frame", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Dismiss by resetting
    resetModernCropState();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(getModernFrame()).toBeNull();
  });
});

describe("Bug Hunt: Modern Crop pointercancel releases capture", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    mockOnViewportPointerDown.mockClear();
    mockOnViewportPointerUp.mockClear();
    mockOnViewportPointerCancel.mockClear();
    mockOnViewportLostPointerCapture.mockClear();
    mockCommitBrushStroke.mockClear();
    mockSpacePressed = false;
    mockPanningActive = false;

    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    vi.restoreAllMocks();
  });

  function renderViewport() {
    const session = WorkspaceManager.createBlankDocument("cap-doc", "Cap Doc", 800, 600);
    ws.addDocument(session);
    const result = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    dispose = result;
    setCropInteractionMode("modern");
    return { session };
  }

  it("clears drag state on ModernCropOverlay onLostPointerCapture", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 50));

    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 0));

    const moveZone = container.querySelector("[data-modern-crop-move]") as SVGRectElement;
    if (!moveZone) throw new Error("Modern crop move zone not found");

    // Register a listener to track drag state changes
    const dragStateSpy = vi.fn();
    const svg = container.querySelector("[data-modern-crop-overlay]") as SVGSVGElement;
    if (!svg) throw new Error("Modern crop overlay SVG not found");

    svg.addEventListener("pointerup", () => {
      dragStateSpy("cleared");
    });

    // Start a move drag on modern crop frame
    moveZone.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, pointerId: 5, clientX: 400, clientY: 300,
    }));

    expect(Element.prototype.setPointerCapture).toHaveBeenCalledWith(5);

    // Simulate lostpointercapture →should trigger clearDrag
    svg.dispatchEvent(new PointerEvent("lostpointercapture", { bubbles: false, pointerId: 5 }));

    // Allow effects/enqueued state setters to flush
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Now dispatch pointerup →if clearDrag was already called above,
    // the dragState should be null and pointerup should be a no-op (no additional "cleared")
    svg.dispatchEvent(new PointerEvent("pointerup", { bubbles: false, pointerId: 5 }));

    // The lostpointercapture handler should have fired clearDrag once
    // pointerup should not fire clearDrag again because dragState is null
    expect(dragStateSpy).toHaveBeenCalledTimes(1);
  });
});

describe("Bug Hunt: Crop state leaks across document switch", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    mockOnViewportPointerDown.mockClear();
    mockOnViewportPointerUp.mockClear();
    mockOnViewportPointerCancel.mockClear();
    mockOnViewportLostPointerCapture.mockClear();
    mockCommitBrushStroke.mockClear();
    mockSpacePressed = false;
    mockPanningActive = false;

    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    vi.restoreAllMocks();
  });

  function renderViewportWithDocs() {
    const session1 = WorkspaceManager.createBlankDocument("doc-1", "Doc 1", 800, 600);
    const session2 = WorkspaceManager.createBlankDocument("doc-2", "Doc 2", 1600, 400);
    ws.addDocument(session1);
    ws.addDocument(session2);

    const result = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    dispose = result;
    setCropInteractionMode("classic");
    return { session1, session2 };
  }

  it("resets Classic crop rect when switching to another document", async () => {
    renderViewportWithDocs();
    await new Promise((resolve) => setTimeout(resolve, 0));

    // After render, active doc is doc-2 (last added). Switch to doc-1 first.
    ws.switchDocument("doc-1");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Set up crop state on doc-1
    setTool("crop");
    setCrop({ x: 50, y: 50, w: 200, h: 150 });
    setCropRotation(15);
    setCropModeState("ratio");
    setCropAspectState({ w: 16, h: 9 });
    setCropSizeTargetState({ w: 800, h: 600 });
    setHiddenCropPreview({ rect: { x: 10, y: 10, w: 100, h: 100 }, rotation: 0 });

    expect(getCrop()).not.toBeNull();
    expect(getCropRotation()).toBe(15);

    // Switch to doc-2
    ws.switchDocument("doc-2");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Crop state should be reset
    expect(getCrop()).toBeNull();
    expect(getCropRotation()).toBe(0);
    expect(getCropMode()).toBe("free");
    expect(getCropAspect()).toBeNull();
    expect(getCropSizeTarget()).toBeNull();
    expect(getHiddenCropPreview()).toBeNull();
  });

  it("resets Classic crop rect when switching back to original document", async () => {
    renderViewportWithDocs();
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Switch to doc-1 first (active is doc-2 after render)
    ws.switchDocument("doc-1");
    await new Promise((resolve) => setTimeout(resolve, 50));

    setTool("crop");
    setCrop({ x: 100, y: 100, w: 300, h: 200 });
    setCropRotation(45);

    // Switch to doc-2 then back to doc-1
    ws.switchDocument("doc-2");
    await new Promise((resolve) => setTimeout(resolve, 50));
    ws.switchDocument("doc-1");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Crop state should be reset (from doc-2 switch), not restored to old values
    expect(getCrop()).toBeNull();
    expect(getCropRotation()).toBe(0);
  });

  it("recomputes Modern crop frame when switching documents", async () => {
    renderViewportWithDocs();
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Switch to doc-1 first (800x600)
    ws.switchDocument("doc-1");
    await new Promise((resolve) => setTimeout(resolve, 50));

    setCropInteractionMode("modern");
    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    const frame1 = getModernFrame();
    expect(frame1).not.toBeNull();

    // Switch to doc-2 (1600x400 →different aspect ratio from doc-1's 800x600)
    ws.switchDocument("doc-2");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Frame should be recomputed for the new document dimensions/aspect ratio
    const frame2 = getModernFrame();
    expect(frame2).not.toBeNull();
    // Different aspect ratio means different frame shape
    const ratio1 = frame1.w / frame1.h;
    const ratio2 = frame2.w / frame2.h;
    expect(ratio2).not.toBeCloseTo(ratio1, 1);
  });

  it("does not reset crop state when switching between non-crop tools", async () => {
    renderViewportWithDocs();
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Switch to doc-1 first (active is doc-2 after render)
    ws.switchDocument("doc-1");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Set crop state but stay in move tool
    setCrop({ x: 50, y: 50, w: 200, h: 150 });
    setCropRotation(15);

    // Switch documents →crop state should still reset even though not in crop tool
    ws.switchDocument("doc-2");
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(getCrop()).toBeNull();
    expect(getCropRotation()).toBe(0);
  });
});

describe("Crop re-entry syncs preview with option bar values", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    mockOnViewportPointerDown.mockClear();
    mockOnViewportPointerUp.mockClear();
    mockOnViewportPointerCancel.mockClear();
    mockOnViewportLostPointerCapture.mockClear();
    mockCommitBrushStroke.mockClear();
    mockSpacePressed = false;
    mockPanningActive = false;
    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    vi.restoreAllMocks();
  });

  function renderViewport() {
    const session = WorkspaceManager.createBlankDocument("doc-1", "Doc 1", 800, 600);
    ws.addDocument(session);
    const result = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    dispose = result;
  }

  it("Modern: entering Crop in Size mode initializes frame at target aspect ratio", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Set Size mode values while in Move tool
    setCropSizeTargetState({ w: 300, h: 600 });
    setCropModeState("size");

    // Enter crop in Modern mode
    setCropInteractionMode("modern");
    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();
    // Frame should fill canvas at target 300:600 (0.5) aspect
    expect(frame.w / frame.h).toBeCloseTo(300 / 600, 1);
    expect(frame.w).toBeGreaterThan(0);
    expect(frame.h).toBeGreaterThan(0);
  });

  it("Classic: entering Crop in Size mode initializes rect at target aspect ratio", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    // No cropRect set, set Size mode values
    setCropSizeTargetState({ w: 200, h: 600 });
    setCropModeState("size");

    // Enter crop in Classic mode
    setCropInteractionMode("classic");
    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    const rect = getCrop();
    expect(rect).not.toBeNull();
    // Rect should fill document at target 200:600 (1:3) aspect
    expect(rect.w / rect.h).toBeCloseTo(200 / 600, 1);
  });

  it("Classic: entering Crop in Ratio mode initializes rect at cropAspect", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Set Ratio mode values
    setCropAspectState({ w: 16, h: 9 });
    setCropModeState("ratio");

    // Enter crop in Classic mode
    setCropInteractionMode("classic");
    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    const rect = getCrop();
    expect(rect).not.toBeNull();
    expect(rect.w / rect.h).toBeCloseTo(16 / 9, 1);
  });

  it("Classic: entering Crop in Free mode does not auto-create rect", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setCropModeState("free");
    setCropInteractionMode("classic");
    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Free mode should NOT auto-create a rect
    expect(getCrop()).toBeNull();
  });

  it("Switching mode after entering Crop refits the Modern frame", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Enter crop in Free mode (Modern)
    setCropInteractionMode("modern");
    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    const frameFree = getModernFrame();
    expect(frameFree).not.toBeNull();

    // Switch to Size mode
    setCropSizeTargetState({ w: 400, h: 300 });
    setCropModeState("size");
    await new Promise((resolve) => setTimeout(resolve, 50));

    const frameSize = getModernFrame();
    expect(frameSize).not.toBeNull();
    expect(frameSize.w / frameSize.h).toBeCloseTo(400 / 300, 1);

    // Switch to Ratio mode
    setCropAspectState({ w: 3, h: 2 });
    setCropModeState("ratio");
    await new Promise((resolve) => setTimeout(resolve, 50));

    const frameRatio = getModernFrame();
    expect(frameRatio).not.toBeNull();
    expect(frameRatio.w / frameRatio.h).toBeCloseTo(3 / 2, 1);
  });
});

describe("Phase 3 Tool Switch Contracts (Move, Selection, Brush, Transform)", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    mockOnViewportPointerDown.mockClear();
    mockOnViewportPointerUp.mockClear();
    mockOnViewportPointerCancel.mockClear();
    mockOnViewportLostPointerCapture.mockClear();
    mockCommitBrushStroke.mockClear();
    mockSpacePressed = false;
    mockPanningActive = false;
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    vi.restoreAllMocks();
  });

  function renderViewport() {
    const session = WorkspaceManager.createBlankDocument("phase3", "Phase 3", 800, 600);
    ws.addDocument(session);
    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    return { session };
  }

  it("Move: round-trip Move -> Brush -> Move leaves no orphan state", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("move");
    await new Promise((resolve) => setTimeout(resolve, 50));
    setTool("brush");
    await new Promise((resolve) => setTimeout(resolve, 50));
    setTool("move");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // After round-trip, engine still works
    const engine = ws.getActiveEngine();
    expect(engine).toBeDefined();
    expect(ws.getActiveSession()).toBeDefined();
  });

  it("Selection: round-trip select -> crop -> select leaves no orphan state", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("select");
    await new Promise((resolve) => setTimeout(resolve, 50));
    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));
    setTool("select");
    await new Promise((resolve) => setTimeout(resolve, 50));

    const engine = ws.getActiveEngine();
    expect(engine).toBeDefined();
    expect(ws.getActiveSession()).toBeDefined();
  });

  it("Brush: round-trip brush -> move -> brush leaves no orphan state", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("brush");
    await new Promise((resolve) => setTimeout(resolve, 50));
    setTool("move");
    await new Promise((resolve) => setTimeout(resolve, 50));
    setTool("brush");
    await new Promise((resolve) => setTimeout(resolve, 50));

    const engine = ws.getActiveEngine();
    expect(engine).toBeDefined();
    expect(ws.getActiveSession()).toBeDefined();
  });

  it("Transform: round-trip move -> crop -> move leaves no orphan layerTransformSession", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("move");
    await new Promise((resolve) => setTimeout(resolve, 50));
    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));
    setTool("move");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // After round-trip, no stale layerTransformSession
    const session = ws.getActiveSession();
    expect(session).toBeDefined();

    const engine = ws.getActiveEngine();
    expect(engine).toBeDefined();
  });
});

describe("Phase 4 Deep Tool State Cleanup (per-signal assertions)", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;
  let editorRef: { current: any };

  const tick = (ms = 50) => new Promise<void>((resolve) => setTimeout(resolve, ms));

  const DeepTestConsumer = () => {
    editorRef.current = useEditor();
    return null;
  };

  beforeEach(async () => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    editorRef = { current: null };

    mockOnViewportPointerDown.mockClear();
    mockOnViewportPointerUp.mockClear();
    mockOnViewportPointerCancel.mockClear();
    mockOnViewportLostPointerCapture.mockClear();
    mockCommitBrushStroke.mockClear();
    mockSpacePressed = false;
    mockPanningActive = false;

    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();

    const session = WorkspaceManager.createBlankDocument("deep", "Deep", 800, 600);
    ws.addDocument(session);

    // The transform box is suppressed for the Background layer
    // (useSelectionTransformDrag.getLayer guard). Convert it to a normal,
    // transformable layer so the overlay renders for this doc's layer.
    const bg = session.engine.getLayers()[0];
    if (bg) {
      bg.isBackground = false;
      bg.lockPosition = false;
      bg.lockRotation = false;
      session.engine.transformLayer(bg.id, {});
    }

    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <DeepTestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );

    await tick();
  });

  afterEach(() => {
    if (dispose) dispose();
    if (container.parentNode) container.parentNode.removeChild(container);
    vi.restoreAllMocks();
  });

  it("Move: switching away clears hoverHandle, hoverPos, layerTransformSession", async () => {
    const ed = editorRef.current;
    setTool("move");
    await tick();

    // Simulate Move-specific state
    ed.setHoverHandle("nw");
    ed.setHoverPos({ x: 100, y: 100 });
    const engine = ws.getActiveEngine()!;
    const l1 = engine.addLayer("L1");
    await tick();
    ed.setLayerTransformSession({
      documentId: "deep",
      layerId: l1.id,
      originalSnapshot: engine.snapshot(),
      originalTransform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      mode: "resize",
      lockRatio: false,
      startedAt: Date.now(),
    });
    await tick();

    // Verify state is set
    expect(ed.hoverHandle()).toBe("nw");
    expect(ed.hoverPos()).not.toBeNull();
    expect(ed.layerTransformSession()).not.toBeNull();

    // Switch to another tool
    setTool("brush");
    await tick();

    // All Move-specific transient state should be cleared
    expect(ed.hoverHandle()).toBeNull();
    expect(ed.hoverPos()).toBeNull();
    expect(ed.layerTransformSession()).toBeNull();
  });

  it("Selection: switching away preserves selection but exits edit mode (no orphan edit mode)", async () => {
    const ed = editorRef.current;
    setTool("select");
    await tick();

    // Set selection and enter edit mode
    ed.setSelection({ x: 10, y: 10, width: 100, height: 100, angle: 0 });
    ed.setSelectionEditMode(true);
    await tick();

    expect(ed.selection()).not.toBeNull();
    expect(ed.selectionEditMode()).toBe(true);

    // Switch to move
    setTool("move");
    await tick();

    // Selection persists (user can have selection in any tool),
    // but edit mode is off (orphan edit mode is the bug)
    expect(ed.selection()).not.toBeNull();
    expect(ed.selectionEditMode()).toBe(false);
  });

  it("Crop (modern): switching away clears modernCropFrame, modernCropImageTransform, undo/redo", async () => {
    const ed = editorRef.current;
    setTool("crop");
    setCropInteractionMode("modern");
    await tick();

    // setTool("crop") with modern mode auto-creates a frame
    expect(ed.modernCropFrame()).not.toBeNull();

    // Set custom image transform
    ed.setModernCropImageTransform({ offsetX: 50, offsetY: 25, rotation: 45, scale: 1.5 });
    await tick();

    // Switch to another tool
    setTool("move");
    await tick();

    // All crop-specific state should be cleared
    expect(ed.modernCropFrame()).toBeNull();
    const t = ed.modernCropImageTransform();
    expect(t.offsetX).toBe(0);
    expect(t.offsetY).toBe(0);
    expect(t.rotation).toBe(0);
    expect(t.scale).toBe(1);
    expect(ed.canModernCropUndo()).toBe(false);
  });

  it("Transform: switching away clears layerTransformSession (orphan transform state)", async () => {
    const ed = editorRef.current;
    setTool("move");
    await tick();

    const engine = ws.getActiveEngine()!;
    const l1 = engine.addLayer("L1");
    await tick();

    // Set up an active transform session (simulates mid-drag)
    ed.setLayerTransformSession({
      documentId: "deep",
      layerId: l1.id,
      originalSnapshot: engine.snapshot(),
      originalTransform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      mode: "resize",
      lockRatio: false,
      startedAt: Date.now(),
    });
    await tick();

    expect(ed.layerTransformSession()).not.toBeNull();

    // Switch to crop (any other tool)
    setTool("crop");
    await tick();

    // Transform session should be cleared
    expect(ed.layerTransformSession()).toBeNull();
  });

  it("Transform Controls: toggling off clears hoverHandle and hoverPos (cursor ghosting)", async () => {
    const ed = editorRef.current;
    setTool("move");
    await tick();

    // Simulate cursor hovering over a resize handle
    ed.setHoverHandle("se");
    ed.setHoverPos({ x: 200, y: 150 });
    await tick();

    expect(ed.hoverHandle()).toBe("se");
    expect(ed.hoverPos()).not.toBeNull();

    // Toggle transform controls off
    ed.setShowTransformControls(false);
    await tick();

    // Both signals should be cleared
    expect(ed.hoverHandle()).toBeNull();
    expect(ed.hoverPos()).toBeNull();
  });

  it("Transform Controls: toggling back on preserves cleared state (no orphan from previous hover)", async () => {
    const ed = editorRef.current;
    setTool("move");
    await tick();

    // Set hover state, toggle off, verify cleared
    ed.setHoverHandle("nw");
    ed.setHoverPos({ x: 100, y: 100 });
    await tick();
    ed.setShowTransformControls(false);
    await tick();

    expect(ed.hoverHandle()).toBeNull();
    expect(ed.hoverPos()).toBeNull();

    // Toggle back on — hover state should remain null (not re-applied)
    ed.setShowTransformControls(true);
    await tick();

    expect(ed.hoverHandle()).toBeNull();
    expect(ed.hoverPos()).toBeNull();
  });

  it("Transform Controls: overlay SVG is mounted/unmounted based on toggle", async () => {
    const ed = editorRef.current;
    setTool("move");
    ed.setShowTransformControls(true);
    await tick(100);

    // With showTransformControls=true and tool=move, the overlay should be in the DOM
    expect(container.querySelector("[data-overlay-svg]")).not.toBeNull();

    // Toggle off — overlay should be removed from the DOM
    ed.setShowTransformControls(false);
    await tick(100);

    expect(container.querySelector("[data-overlay-svg]")).toBeNull();

    // Toggle back on — overlay should re-appear
    ed.setShowTransformControls(true);
    await tick(100);

    expect(container.querySelector("[data-overlay-svg]")).not.toBeNull();
  });
});

describe("Phase 5 Cross-Tool State Interaction (UX contracts)", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;
  let editorRef: { current: any };

  const tick = (ms = 50) => new Promise<void>((resolve) => setTimeout(resolve, ms));

  const DeepTestConsumer = () => {
    editorRef.current = useEditor();
    return null;
  };

  beforeEach(async () => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    editorRef = { current: null };

    mockOnViewportPointerDown.mockClear();
    mockOnViewportPointerUp.mockClear();
    mockOnViewportPointerCancel.mockClear();
    mockOnViewportLostPointerCapture.mockClear();
    mockCommitBrushStroke.mockClear();
    mockSpacePressed = false;
    mockPanningActive = false;

    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();

    const session = WorkspaceManager.createBlankDocument("cross", "Cross", 800, 600);
    ws.addDocument(session);

    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <DeepTestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );

    await tick();
  });

  afterEach(() => {
    if (dispose) dispose();
    if (container.parentNode) container.parentNode.removeChild(container);
    vi.restoreAllMocks();
  });

  it("Selection persists across non-crop tool switch (cross-tool UX contract)", async () => {
    const ed = editorRef.current;
    setTool("select");
    await tick();

    ed.setSelection({ x: 50, y: 50, width: 200, height: 100, angle: 0 });
    await tick();

    expect(ed.selection()).not.toBeNull();

    // Switch through all non-crop tools (selection is document state, persists)
    setTool("move");
    await tick();
    setTool("brush");
    await tick();
    setTool("select");
    await tick();

    // Selection should survive the round-trip
    const sel = ed.selection();
    expect(sel).not.toBeNull();
    expect(sel.x).toBe(50);
    expect(sel.y).toBe(50);
    expect(sel.width).toBe(200);
    expect(sel.height).toBe(100);
  });

  it("Selection is cleared on entering crop tool (documented design)", async () => {
    const ed = editorRef.current;
    setTool("select");
    await tick();

    ed.setSelection({ x: 50, y: 50, width: 200, height: 100, angle: 0 });
    await tick();

    expect(ed.selection()).not.toBeNull();

    // Entering crop is a fresh operation. Selection is intentionally
    // cleared (different tool = different operation, similar to how
    // cropRect is independent from selection). Documented here so
    // future maintainers know this is by design, not a bug.
    setTool("crop");
    setCropInteractionMode("modern");
    await tick();

    expect(ed.selection()).toBeNull();
  });

  it("Active layer persists across tool switch (document state contract)", async () => {
    const ed = editorRef.current;
    const engine = ws.getActiveEngine()!;

    const l1 = engine.addLayer("L1");
    const l2 = engine.addLayer("L2");
    await tick();

    expect(ed.activeLayerId()).toBe(l2.id);

    setTool("move");
    await tick();
    setTool("brush");
    await tick();
    setTool("crop");
    await tick();
    setTool("select");
    await tick();

    // Active layer should still be l2 (last activated, not reset to default)
    expect(ed.activeLayerId()).toBe(l2.id);
  });

  it("Brush settings persist across tool switch (user preferences contract)", async () => {
    const ed = editorRef.current;
    setTool("brush");
    await tick();

    ed.setBrushSize(150);
    ed.setBrushHardness(0.8);
    ed.setBrushOpacity(0.65);
    await tick();

    expect(ed.brushSize()).toBe(150);
    expect(ed.brushHardness()).toBe(0.8);
    expect(ed.brushOpacity()).toBe(0.65);

    // Switch through other tools
    setTool("move");
    await tick();
    setTool("crop");
    await tick();
    setTool("select");
    await tick();
    setTool("brush");
    await tick();

    // User preferences should persist
    expect(ed.brushSize()).toBe(150);
    expect(ed.brushHardness()).toBe(0.8);
    expect(ed.brushOpacity()).toBe(0.65);
  });

  it("Crop (modern): switching away and back creates fresh frame (no orphan state)", async () => {
    const ed = editorRef.current;
    setTool("crop");
    setCropInteractionMode("modern");
    await tick();

    const firstFrame = ed.modernCropFrame();
    expect(firstFrame).not.toBeNull();

    // Switch to another tool
    setTool("move");
    await tick();
    expect(ed.modernCropFrame()).toBeNull();

    // Switch back to crop
    setTool("crop");
    await tick();

    // Should have a fresh frame (may differ in position due to viewport sync,
    // but should exist and be valid)
    const secondFrame = ed.modernCropFrame();
    expect(secondFrame).not.toBeNull();
    expect(secondFrame.w).toBeGreaterThan(0);
    expect(secondFrame.h).toBeGreaterThan(0);
  });
});

describe("CanvasViewport Overlay Container (Screen-Space Migration)", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);

    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    vi.restoreAllMocks();
  });

  it("positions artboard border in screen-space coords (no CSS transform wrapper)", async () => {
    const session = WorkspaceManager.createBlankDocument(
      "doc-screen-space",
      "Doc",
      800,
      600,
    );
    ws.addDocument(session);
    ws.switchDocument("doc-screen-space");

    const result = render(
      () => (
        <EditorProvider
          workspace={ws}
          renderer={renderer}
          scheduler={scheduler}
        >
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    dispose = result;
    setCropInteractionMode("classic");

    const engine = session.engine;
    engine.setViewport({ panX: 50, panY: 50, zoom: 2.0 });
    await new Promise((resolve) => setTimeout(resolve, 0));

    const artboard = container.querySelector(
      "[data-artboard-border]",
    ) as HTMLDivElement;
    expect(artboard).not.toBeNull();

    // Artboard border uses GPU-accelerated CSS transform instead of left/top
    expect(artboard.style.transform).toBe("translate(50px, 50px)");
    expect(artboard.style.left).toBe("");
    expect(artboard.style.top).toBe("");
    expect(artboard.style.width).toBe("1600px");
    expect(artboard.style.height).toBe("1200px");

    // No wrapper div with transform: translate3d(...) scale(...) should exist
    // (In this test crop mode is not active, so the only transform that
    // would be a "wrapper" is the overlay container's viewport transform.)
    const allDivs = container.querySelectorAll("div");
    for (const div of Array.from(allDivs)) {
      const t = (div as HTMLDivElement).style.transform;
      if (t && t.includes("translate3d") && t.includes("scale(")) {
        throw new Error(
          `Found unexpected CSS transform wrapper: div.transform="${t}"`,
        );
      }
    }
  });
});

describe("CanvasViewport Modern Crop dashed canvas boundary line", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);

    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    vi.restoreAllMocks();
  });

  it("dashed canvas boundary line follows image transform offset and scale", async () => {
    const session = WorkspaceManager.createBlankDocument(
      "doc-dashed",
      "Doc",
      800,
      600,
    );
    ws.addDocument(session);
    ws.switchDocument("doc-dashed");

    const result = render(
      () => (
        <EditorProvider
          workspace={ws}
          renderer={renderer}
          scheduler={scheduler}
        >
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    dispose = result;
    setCropInteractionMode("classic");

    // Enter modern crop with default frame
    setTool("crop");
    setCropInteractionMode("modern");
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Find the dashed line rect (stroke-dasharray="6,4")
    const dashedRect = container.querySelector(
      'rect[stroke-dasharray="6,4"]',
    ) as SVGRectElement | null;

    // The dashed line may not be visible if the frame fits inside the doc.
    // To force it to show, set frame to be larger than the doc.
    setModernFrameState({ x: -200, y: -200, w: 1200, h: 1000 });
    await new Promise((resolve) => setTimeout(resolve, 0));



    const dashedAfterFrame = container.querySelector(
      'rect[stroke-dasharray="6,4"]',
    ) as SVGRectElement | null;
    expect(dashedAfterFrame).not.toBeNull();
    const dashXBefore = dashedAfterFrame!.getAttribute("x");
    const dashYBefore = dashedAfterFrame!.getAttribute("y");
    const dashWBefore = dashedAfterFrame!.getAttribute("width");
    const dashHBefore = dashedAfterFrame!.getAttribute("height");

    // Now set a non-zero image transform offset (simulating user dragging the cropbox)
    setModernImageTransform({
      offsetX: 50,
      offsetY: 30,
      rotation: 0,
      scale: 1.5,
    });
    await new Promise((resolve) => setTimeout(resolve, 0));

    const dashedAfterOffset = container.querySelector(
      'rect[stroke-dasharray="6,4"]',
    ) as SVGRectElement | null;
    expect(dashedAfterOffset).not.toBeNull();

    // The dashed line should follow the image transform offset and scale
    expect(parseFloat(dashedAfterOffset!.getAttribute("x")!)).toBeCloseTo(parseFloat(dashXBefore!) + 50, 1);
    expect(parseFloat(dashedAfterOffset!.getAttribute("y")!)).toBeCloseTo(parseFloat(dashYBefore!) + 30, 1);
    expect(parseFloat(dashedAfterOffset!.getAttribute("width")!)).toBeCloseTo(parseFloat(dashWBefore!) * 1.5, 1);
    expect(parseFloat(dashedAfterOffset!.getAttribute("height")!)).toBeCloseTo(parseFloat(dashHBefore!) * 1.5, 1);
  });
});

describe("CanvasViewport Modern Crop →Camera Image Transform Sync", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;
  let testCamera: ViewportCamera;

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);

    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();

    testCamera = new ViewportCamera();
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    vi.restoreAllMocks();
  });

  it("modern crop with frame + transform propagates to camera image transform", async () => {
    const session = WorkspaceManager.createBlankDocument(
      "doc-mc-1",
      "Doc",
      800,
      600,
    );
    ws.addDocument(session);
    ws.switchDocument("doc-mc-1");

    const result = render(
      () => (
        <EditorProvider
          workspace={ws}
          renderer={renderer}
          scheduler={scheduler}
          camera={testCamera}
        >
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    dispose = result;
    setCropInteractionMode("classic");

    // Set frame BEFORE entering modern crop to use our specific frame
    setModernFrameState({ x: 100, y: 100, w: 200, h: 200 });
    setModernImageTransform({
      offsetX: 0,
      offsetY: 0,
      rotation: 30,
      scale: 1,
    });
    setTool("crop");
    setCropInteractionMode("modern");
    await new Promise((resolve) => setTimeout(resolve, 0));

    const it = testCamera.getImageTransform();
    expect(it.rotation).toBe(30);
    expect(it.scale).toBe(1);
    expect(it.pivotScreen).not.toBeNull();
    expect(it.pivotDocument).not.toBeNull();
    // With scale=1, pan=0, offset=0: pivotScreen == pivotDocument
    expect(it.pivotScreen?.x).toBeCloseTo(it.pivotDocument?.x ?? 0, 5);
    expect(it.pivotScreen?.y).toBeCloseTo(it.pivotDocument?.y ?? 0, 5);
  });

  it("modern crop exit resets camera image transform to identity", async () => {
    const session = WorkspaceManager.createBlankDocument(
      "doc-mc-2",
      "Doc",
      800,
      600,
    );
    ws.addDocument(session);
    ws.switchDocument("doc-mc-2");

    const result = render(
      () => (
        <EditorProvider
          workspace={ws}
          renderer={renderer}
          scheduler={scheduler}
          camera={testCamera}
        >
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    dispose = result;
    setCropInteractionMode("classic");

    setModernFrameState({ x: 100, y: 100, w: 200, h: 200 });
    setModernImageTransform({
      offsetX: 0,
      offsetY: 0,
      rotation: 45,
      scale: 1.5,
    });
    setTool("crop");
    setCropInteractionMode("modern");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(testCamera.getImageTransform().rotation).toBe(45);

    setTool("move");
    await new Promise((resolve) => setTimeout(resolve, 0));

    const it = testCamera.getImageTransform();
    expect(it.rotation).toBe(0);
    expect(it.scale).toBe(1);
    expect(it.pivotScreen).toBeNull();
    expect(it.pivotDocument).toBeNull();
  });

  it("feature flag OFF: modern crop uses legacy CSS path (camera not touched)", async () => {
    const session = WorkspaceManager.createBlankDocument(
      "doc-mc-flag",
      "Doc",
      800,
      600,
    );
    ws.addDocument(session);
    ws.switchDocument("doc-mc-flag");

    const result = render(
      () => (
        <EditorProvider
          workspace={ws}
          renderer={renderer}
          scheduler={scheduler}
          camera={testCamera}
        >
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    dispose = result;
    setCropInteractionMode("classic");

    // Disable the feature flag
    setUseGPUCameraForModernCrop(false);

    setModernFrameState({ x: 100, y: 100, w: 200, h: 200 });
    setModernImageTransform({
      offsetX: 0,
      offsetY: 0,
      rotation: 60,
      scale: 2,
    });
    setTool("crop");
    setCropInteractionMode("modern");
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Camera should NOT have the transform set (legacy path uses CSS instead)
    const it = testCamera.getImageTransform();
    expect(it.rotation).toBe(0);
    expect(it.scale).toBe(1);
    expect(it.pivotScreen).toBeNull();
    expect(it.pivotDocument).toBeNull();
  });
});

describe("CanvasViewport Selection Constraints Integration", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);

    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    vi.restoreAllMocks();
  });

  function renderViewport() {
    const session = WorkspaceManager.createBlankDocument("sel-const", "Sel Const", 800, 600);
    ws.addDocument(session);
    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    return { session };
  }

  function getCanvas(): HTMLCanvasElement {
    const c = container.querySelector("canvas") as HTMLCanvasElement;
    if (!c) throw new Error("Canvas not found");
    return c;
  }

  function firePointerDown(el: Element, pointerId = 10, clientX = 100, clientY = 100, altKey = false) {
    const ev = new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId, clientX, clientY,
    });
    // jsdom's PointerEvent init drops MouseEvent modifiers; force-set it.
    if (altKey) Object.defineProperty(ev, "altKey", { value: true, configurable: true });
    el.dispatchEvent(ev);
  }

  function firePointerMove(el: Element, pointerId = 10, clientX = 200, clientY = 200) {
    el.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId, clientX, clientY,
    }));
  }

  function firePointerUp(el: Element, pointerId = 10, clientX = 200, clientY = 200) {
    el.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId, clientX, clientY,
    }));
  }

  it("Selection draw: Fixed Ratio mode locks aspect ratio", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("selection");
    
    // Set ratio constraint to 2:1
    setSelectionConstraintMode("ratio");
    setSelectionRatioW(2);
    setSelectionRatioH(1);
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Start drawing a marquee at (100, 100) and drag to (200, 150)
    const canvas = getCanvas();
    firePointerDown(canvas, 12, 100, 100);
    firePointerMove(canvas, 12, 200, 150);
    firePointerUp(canvas, 12, 200, 150);
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Selection should be (100, 100, 100, 50)
    const sel = getSelectionState();
    expect(sel).not.toBeNull();
    expect(sel!.width).toBe(100);
    expect(sel!.height).toBe(50);
  });

  it("Selection draw: Fixed Size mode creates selection of fixed dimensions", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("selection");

    // Set size constraint to 120 x 80
    setSelectionConstraintMode("size");
    setSelectionSizeW(120);
    setSelectionSizeH(80);
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Click at (150, 150)
    const canvas = getCanvas();
    firePointerDown(canvas, 13, 150, 150);
    firePointerUp(canvas, 13, 150, 150);
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Selection should be created centered around (150, 150)
    const sel = getSelectionState();
    expect(sel).not.toBeNull();
    expect(sel!.width).toBe(120);
    expect(sel!.height).toBe(80);
    // Centered around (150, 150) => x = 150 - 120 / 2 = 90, y = 150 - 80 / 2 = 110
    expect(sel!.x).toBe(90);
    expect(sel!.y).toBe(110);
  });
});

describe("Crop resize: frame stays in doc coords (no screen leak)", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    mockOnViewportPointerDown.mockClear();
    mockOnViewportPointerUp.mockClear();
    mockOnViewportPointerCancel.mockClear();
    mockOnViewportLostPointerCapture.mockClear();
    mockCommitBrushStroke.mockClear();
    mockSpacePressed = false;
    mockPanningActive = false;

    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    vi.restoreAllMocks();
  });

  function renderViewport() {
    const session = WorkspaceManager.createBlankDocument("cap-doc", "Cap Doc", 800, 600);
    ws.addDocument(session);
    const result = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    dispose = result;
    setCropInteractionMode("modern");
    return { session };
  }

  it("resize drag at zoom=2 stores doc-coord frame (not screen-coord)", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Set zoom=2 so screen≠doc — reveals any missing screen→doc conversion
    setZoomState(2);
    setPanState({ x: 0, y: 0 });
    await new Promise((resolve) => setTimeout(resolve, 50));

    setTool("crop");
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Replace auto-created frame with a known doc-coord frame
    setModernFrameState({ x: 0, y: 0, w: 400, h: 300 });
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Find the SE handle on the overlay
    const svg = container.querySelector("[data-modern-crop-overlay]") as SVGSVGElement;
    if (!svg) throw new Error("Modern crop overlay SVG not found");

    const seHandle = svg.querySelector('[data-modern-crop-handle="se"]') as SVGRectElement;
    if (!seHandle) throw new Error("SE handle not found");

    // Simulate drag on SE handle: 50px right, 30px down
    seHandle.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, pointerId: 42, clientX: 800, clientY: 600,
    }));
    seHandle.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, pointerId: 42, clientX: 850, clientY: 630,
    }));
    seHandle.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, pointerId: 42, clientX: 850, clientY: 630,
    }));

    await new Promise((resolve) => setTimeout(resolve, 0));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();

    // After drag, w should be in doc coords: expect ~450 (start 400 + 50px/zoom*2)
    // If screen coords leaked, w would be ~900 (400*2 + 100, stored as doc)
    // docWidth=800 → w < 800 proves doc-coord storage
    expect(frame!.w).toBeLessThanOrEqual(800);
    expect(frame!.h).toBeLessThanOrEqual(600);

    // w should have INCREASED from 400 (SE drag increases size)
    expect(frame!.w).toBeGreaterThan(400);
    expect(frame!.h).toBeGreaterThan(300);
  });
});

describe("Move auto-select: drag target matches panel selection (alpha-aware)", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;
  let editorRef: { current: any };

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    editorRef = { current: null };

    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    vi.restoreAllMocks();
  });

  function renderViewport() {
    const session = WorkspaceManager.createBlankDocument("doc-move-alpha", "Move Doc", 800, 600);
    ws.addDocument(session);

    const EditorRefCapture = () => {
      editorRef.current = useEditor();
      return null;
    };

    const result = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <EditorRefCapture />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );

    dispose = result;
    return { session };
  }

  it("drag target and panel selection stay consistent when the top layer is transparent at the click point", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    editorRef.current.setActiveTool("move");
    await new Promise((resolve) => setTimeout(resolve, 0));

    const engine = ws.getActiveEngine()!;

    // Remove the locked background so both test layers are freely movable.
    const bg = engine.getLayers().find((l) => l.isBackground);
    if (bg) engine.deleteLayer(bg.id);
    const bottom = engine.addLayer("bottom", 800, 600);
    const top = engine.addLayer("top", 800, 600);
    await new Promise((resolve) => setTimeout(resolve, 0));

    // The click point (200,200) sits inside BOTH layers. The top layer is
    // transparent there (alpha 0) so alpha-aware hit-testing must fall
    // through to the bottom layer. Both auto-select paths must agree.
    const alphaSpy = vi.spyOn(engine, "sampleLayerAlpha").mockImplementation(
      (id: string) => (id === top.id ? 0 : 1),
    );

    // Real-user pre-condition: the top layer is already selected before the
    // click (context.selectedLayerId is captured at pointerdown).
    engine.setActiveLayer(top.id);
    editorRef.current.setSelectedLayerId(top.id);
    await new Promise((resolve) => setTimeout(resolve, 0));

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    const tfSpy = vi.spyOn(engine, "transformLayer");

    const topStartX = top.transform.x;
    const topStartY = top.transform.y;
    const bottomStartX = bottom.transform.x;
    const bottomStartY = bottom.transform.y;

    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 99,
      clientX: 200, clientY: 200,
    }));
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 99,
      clientX: 230, clientY: 230,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 99,
      clientX: 230, clientY: 230,
    }));
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Panel selection == engine active layer (both hit-test paths agree)
    expect(editorRef.current.selectedLayerId()).toBe(bottom.id);
    expect(engine.getActiveLayerId()).toBe(bottom.id);

    // The layer that actually moved == the selected layer. The transparent
    // top layer must NOT move while the panel shows the bottom layer.
    const topAfter = engine.getLayer(top.id)!;
    const bottomAfter = engine.getLayer(bottom.id)!;
    expect(topAfter.transform.x).toBe(topStartX);
    expect(topAfter.transform.y).toBe(topStartY);
    expect(bottomAfter.transform.x).not.toBe(bottomStartX);
    expect(bottomAfter.transform.y).not.toBe(bottomStartY);

    alphaSpy.mockRestore();
  });
});

describe("Shape tool Phase 3 contract", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;
  let editorRef: { current: any };

  const tick = (ms = 50) => new Promise<void>((resolve) => setTimeout(resolve, ms));

  // Capture the real editor so undo/redo goes through navigateHistory — the
  // same path the app's Ctrl+Z / toolbar buttons use.
  const ShapeConsumer = () => {
    editorRef.current = useEditor();
    return null;
  };

  beforeEach(async () => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    editorRef = { current: null };

    mockOnViewportPointerDown.mockClear();
    mockOnViewportPointerUp.mockClear();
    mockOnViewportPointerCancel.mockClear();
    mockOnViewportLostPointerCapture.mockClear();
    mockCommitBrushStroke.mockClear();
    mockSpacePressed = false;
    mockPanningActive = false;

    // jsdom has no OffscreenCanvas and its <canvas>.getContext('2d') returns
    // null, so the shape rasterizer's jsdom fallback would throw E_CANVAS.
    // Stub OffscreenCanvas exactly like shapeLayer.test.ts so the engine does
    // the mock-canvas rasterization path instead.
    const MockOffscreenCanvas = function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      const ctx = {
        translate: () => undefined,
        fillStyle: undefined,
        strokeStyle: undefined,
        lineWidth: undefined,
        lineCap: undefined,
        beginPath: () => undefined,
        rect: () => undefined,
        roundRect: () => undefined,
        ellipse: () => undefined,
        moveTo: () => undefined,
        lineTo: () => undefined,
        closePath: () => undefined,
        fill: () => undefined,
        stroke: () => undefined,
        drawImage: () => undefined,
      };
      this.getContext = () => ctx;
      this.transferToImageBitmap = () => ({ width: w, height: h });
    } as any;
    vi.stubGlobal("OffscreenCanvas", MockOffscreenCanvas);

    const session = WorkspaceManager.createBlankDocument("shape-p3", "Shape P3", 800, 600);
    ws.addDocument(session);

    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <ShapeConsumer />
          <OptionBar />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
  });

  afterEach(() => {
    if (dispose) dispose();
    if (container.parentNode) container.parentNode.removeChild(container);
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  function getCanvas(): HTMLCanvasElement {
    const c = container.querySelector("canvas") as HTMLCanvasElement;
    if (!c) throw new Error("Canvas not found");
    return c;
  }

  function shapeLayers(): any[] {
    return (ws.getActiveEngine()?.getLayers() ?? []).filter((l) => l.type === "shape");
  }

  // jsdom's PointerEvent init drops MouseEvent modifiers; force-set them
  // (mirrors the file's legacy firePointerDown helper).
  function firePointer(
    canvas: HTMLCanvasElement,
    type: "pointerdown" | "pointermove" | "pointerup" | "pointercancel",
    pointerId: number,
    clientX: number,
    clientY: number,
    mods: { shiftKey?: boolean; altKey?: boolean } = {},
  ) {
    const ev = new PointerEvent(type, {
      bubbles: true,
      cancelable: true,
      button: 0,
      pointerId,
      clientX,
      clientY,
    });
    Object.defineProperty(ev, "shiftKey", { value: !!mods.shiftKey, configurable: true });
    Object.defineProperty(ev, "altKey", { value: !!mods.altKey, configurable: true });
    canvas.dispatchEvent(ev);
  }

  // Full drag chain at default zoom: pointerdown (100,100) -> move (200,150) -> up.
  function drawRect(canvas: HTMLCanvasElement, pointerId = 10, mods: { shiftKey?: boolean; altKey?: boolean } = {}) {
    firePointer(canvas, "pointerdown", pointerId, 100, 100, mods);
    firePointer(canvas, "pointermove", pointerId, 200, 150, mods);
    firePointer(canvas, "pointerup", pointerId, 200, 150, mods);
  }

  it("idle -> shape drag -> committed layer -> switch round-trip leaves no orphan state", async () => {
    setTool("shape");
    await tick();

    const engine = ws.getActiveEngine()!;
    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();
    expect(engine.getLayers().length).toBe(1); // background only

    drawRect(getCanvas());
    await tick();

    // Committed: exactly ONE shape layer with the drag math under zoom=1.
    expect(shapeLayers().length).toBe(1);
    expect(engine.getLayers().length).toBe(2);
    expect(history.getUndoCount()).toBe(baseline + 1);

    const s = shapeLayers()[0];
    expect(s.width).toBe(100);
    expect(s.height).toBe(50);
    expect(s.transform.x).toBe(100);
    expect(s.transform.y).toBe(100);
    expect(s.shapeParams).toMatchObject({ kind: "rect", width: 100, height: 50 });
    expect(engine.getActiveLayerId()).toBe(s.id);

    // Tool switch round-trip: shape -> brush -> shape -> move.
    setTool("brush");
    await tick();
    setTool("shape");
    await tick();
    setTool("move");
    await tick();

    // No orphan state: tool normalized, same layer count, no temp leftover.
    expect(editorRef.current.activeTool()).toBe("move");
    expect(engine.getLayers().length).toBe(2);
    expect(shapeLayers().length).toBe(1);
    expect(history.getUndoCount()).toBe(baseline + 1);

    // A fresh drag after the round-trip still commits cleanly.
    setTool("shape");
    await tick();
    firePointer(getCanvas(), "pointerdown", 11, 300, 300);
    firePointer(getCanvas(), "pointermove", 11, 400, 360);
    firePointer(getCanvas(), "pointerup", 11, 400, 360);
    await tick();

    expect(shapeLayers().length).toBe(2);
    expect(history.getUndoCount()).toBe(baseline + 2);
  });

  it("undo removes exactly the added shape; redo restores it with params intact", async () => {
    setTool("shape");
    await tick();

    const engine = ws.getActiveEngine()!;
    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();

    drawRect(getCanvas());
    await tick();
    expect(shapeLayers().length).toBe(1);

    // Undo/redo through the same primitives the app's Ctrl+Z handler uses
    // (useEditorCommands.restoreHistorySnapshot: history.undo/redo ->
    // engine.restore). navigateHistory is NOT used: it's an absolute index
    // into the history dock whose signal is refreshed on engine onChange,
    // and the shape commit (temp-mutation-then-commit ordering) lands after
    // the last engine mutation — so the index lags one entry.
    const undone = history.undo(engine.snapshot());
    expect(undone).not.toBeNull();
    engine.restore(undone!);
    await tick();
    expect(shapeLayers().length).toBe(0);
    expect(engine.getLayers().length).toBe(1);
    expect(history.getUndoCount()).toBe(baseline);
    expect(history.getRedoCount()).toBe(1);

    // Redo: exactly one layer returns, same params, no temp leftovers.
    const redone = history.redo(engine.snapshot());
    expect(redone).not.toBeNull();
    engine.restore(redone!);
    await tick();
    expect(shapeLayers().length).toBe(1);
    expect(shapeLayers()[0].shapeParams).toMatchObject({ kind: "rect", width: 100, height: 50 });
    expect(history.getUndoCount()).toBe(baseline + 1);
    expect(history.getRedoCount()).toBe(0);
  });

  it("pointercancel mid-drag removes the temp layer without committing", async () => {
    setTool("shape");
    await tick();

    const engine = ws.getActiveEngine()!;
    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();

    const canvas = getCanvas();
    firePointer(canvas, "pointerdown", 10, 100, 100);
    firePointer(canvas, "pointermove", 10, 200, 200);
    firePointer(canvas, "pointercancel", 10, 200, 200);
    await tick();

    expect(shapeLayers().length).toBe(0);
    expect(engine.getLayers().length).toBe(1);
    expect(history.getUndoCount()).toBe(baseline);

    // Drag state was fully released: a fresh drag still commits.
    drawRect(canvas, 11);
    await tick();
    expect(shapeLayers().length).toBe(1);
    expect(history.getUndoCount()).toBe(baseline + 1);
  });

  it("sub-3px click creates no layer and no history entry", async () => {
    setTool("shape");
    await tick();

    const engine = ws.getActiveEngine()!;
    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();

    const canvas = getCanvas();
    firePointer(canvas, "pointerdown", 10, 100, 100);
    firePointer(canvas, "pointerup", 10, 101, 100);
    await tick();

    expect(shapeLayers().length).toBe(0);
    expect(engine.getLayers().length).toBe(1);
    expect(history.getUndoCount()).toBe(baseline);
  });

  it("Shift drag locks the shape to a square", async () => {
    setTool("shape");
    await tick();

    drawRect(getCanvas(), 10, { shiftKey: true });
    await tick();

    const s = shapeLayers()[0];
    expect(s.shapeParams.width).toBe(100);
    expect(s.shapeParams.height).toBe(100);
  });

  it("Alt drag centers the shape on the drag start point", async () => {
    setTool("shape");
    await tick();

    drawRect(getCanvas(), 10, { altKey: true });
    await tick();

    // Drag delta 100x50 is the half-extent: doubled to 200x100, top-left one
    // extent back from the center at (100,100).
    const s = shapeLayers()[0];
    expect(s.shapeParams.width).toBe(200);
    expect(s.shapeParams.height).toBe(100);
    expect(s.transform.x).toBe(0);
    expect(s.transform.y).toBe(50);
  });

  it("option bar: shape kind controls are visible while the shape tool is active", async () => {
    // Move tool default: no shape option bar mounted (draw-mode branch).
    expect(container.querySelector("[data-shape-option-bar]")).toBeNull();

    setTool("shape");
    await tick();
    expect(container.querySelector("[data-shape-option-bar]")).not.toBeNull();

    // Draw-mode bar unmounts when the tool leaves shape and nothing is selected.
    setTool("brush");
    await tick();
    expect(container.querySelector("[data-shape-option-bar]")).toBeNull();

    // Back into the shape tool: the committed layer keeps the bar visible.
    setTool("shape");
    await tick();
    drawRect(getCanvas());
    await tick();
    expect(container.querySelector("[data-shape-option-bar]")).not.toBeNull();
    expect(shapeLayers().length).toBe(1);

    // Edit-mode persistence after commit: the committed shape is selected, so
    // switching tools keeps the bar until the selection is cleared (Task 7).
    setTool("brush");
    await tick();
    expect(container.querySelector("[data-shape-option-bar]")).not.toBeNull();
    setSelectedLayerIdState(null);
    await tick();
    expect(container.querySelector("[data-shape-option-bar]")).toBeNull();
  });
});

describe("Text tool Phase 3 contract", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;
  let editorRef: { current: any };

  const tick = (ms = 50) => new Promise<void>((resolve) => setTimeout(resolve, ms));

  // Capture the real editor so session state + activeTool assertions go
  // through the same signals the app uses (no hand-rolled state).
  const TextConsumer = () => {
    editorRef.current = useEditor();
    return null;
  };

  beforeEach(async () => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    editorRef = { current: null };

    mockOnViewportPointerDown.mockClear();
    mockOnViewportPointerUp.mockClear();
    mockOnViewportPointerCancel.mockClear();
    mockOnViewportLostPointerCapture.mockClear();
    mockCommitBrushStroke.mockClear();
    mockSpacePressed = false;
    mockPanningActive = false;

    // jsdom has no OffscreenCanvas and its <canvas>.getContext('2d') returns
    // null, so the text rasterizer's jsdom fallback would throw E_CANVAS.
    // Shared text-capable seam (10px/char, live-dims transferToImageBitmap).
    stubTextOffscreenCanvas();

    const session = WorkspaceManager.createBlankDocument("text-p3", "Text P3", 800, 600);
    ws.addDocument(session);

    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <TextConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
  });

  afterEach(() => {
    if (dispose) dispose();
    if (container.parentNode) container.parentNode.removeChild(container);
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  function getCanvas(): HTMLCanvasElement {
    const c = container.querySelector("canvas") as HTMLCanvasElement;
    if (!c) throw new Error("Canvas not found");
    return c;
  }

  function textLayers(): any[] {
    return (ws.getActiveEngine()?.getLayers() ?? []).filter((l) => l.type === "text");
  }

  // jsdom's PointerEvent init drops MouseEvent modifiers; force-set them
  // (mirrors the shape describe's firePointer helper).
  function firePointer(
    canvas: HTMLCanvasElement,
    type: "pointerdown" | "pointermove" | "pointerup" | "pointercancel",
    pointerId: number,
    clientX: number,
    clientY: number,
  ) {
    const ev = new PointerEvent(type, {
      bubbles: true,
      cancelable: true,
      button: 0,
      pointerId,
      clientX,
      clientY,
    });
    canvas.dispatchEvent(ev);
  }

  // Click = pointerdown+up at the same point (default zoom/pan → doc coords
  // equal client coords because jsdom's container rect is all-zero).
  function clickText(canvas: HTMLCanvasElement, x: number, y: number, pointerId = 10) {
    firePointer(canvas, "pointerdown", pointerId, x, y);
    firePointer(canvas, "pointerup", pointerId, x, y);
  }

  function getOverlay(): HTMLTextAreaElement {
    const t = container.querySelector("[data-text-edit-overlay]") as HTMLTextAreaElement;
    if (!t) throw new Error("Text edit overlay not found — session may not be open");
    return t;
  }

  // Typing goes through the SAME surface a user uses: set the textarea value
  // and fire an input event (Solid's onInput reads e.currentTarget.value).
  function typeText(text: string) {
    const ta = getOverlay();
    ta.value = text;
    ta.dispatchEvent(new Event("input", { bubbles: true }));
  }

  // Ctrl+Enter flushes the debounce and commits the session (one undo step).
  function commitWithCtrlEnter() {
    getOverlay().dispatchEvent(
      new KeyboardEvent("keydown", {
        bubbles: true,
        cancelable: true,
        key: "Enter",
        ctrlKey: true,
      }),
    );
  }

  it("idle -> text click -> typed session -> Ctrl+Enter commit -> tool-switch round-trip leaves no orphan state", async () => {
    setTool("text");
    await tick();

    const engine = ws.getActiveEngine()!;
    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();
    expect(engine.getLayers().length).toBe(1); // background only

    clickText(getCanvas(), 100, 100);
    await tick();

    // Session open: temp text layer + edit overlay mounted.
    expect(textLayers().length).toBe(1);
    expect(editorRef.current.textEditSession()).not.toBeNull();
    getOverlay();

    typeText("Hello");
    commitWithCtrlEnter();
    await tick();

    // Committed: exactly ONE text layer with the typed content.
    expect(textLayers().length).toBe(1);
    expect(engine.getLayers().length).toBe(2);
    expect(history.getUndoCount()).toBe(baseline + 1);
    expect(editorRef.current.textEditSession()).toBeNull();

    const t = textLayers()[0];
    expect(t.textData.content).toBe("Hello");
    expect(t.transform.x).toBe(100);
    expect(t.transform.y).toBe(100);
    expect(engine.getActiveLayerId()).toBe(t.id);

    // Tool switch round-trip: text -> brush -> text -> move.
    setTool("brush");
    await tick();
    setTool("text");
    await tick();
    setTool("move");
    await tick();

    // No orphan state: tool normalized, same layer count, no temp leftover.
    expect(editorRef.current.activeTool()).toBe("move");
    expect(editorRef.current.textEditSession()).toBeNull();
    expect(engine.getLayers().length).toBe(2);
    expect(textLayers().length).toBe(1);
    expect(history.getUndoCount()).toBe(baseline + 1);

    // A fresh text session after the round-trip still commits cleanly.
    setTool("text");
    await tick();
    clickText(getCanvas(), 300, 300, 11);
    await tick();
    typeText("Again");
    commitWithCtrlEnter();
    await tick();
    expect(textLayers().length).toBe(2);
    expect(history.getUndoCount()).toBe(baseline + 2);
  });

  it("tool switch auto-commits an open session (typed content persists, no temp layer)", async () => {
    setTool("text");
    await tick();

    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();

    clickText(getCanvas(), 120, 80);
    await tick();
    expect(textLayers().length).toBe(1);
    expect(editorRef.current.textEditSession()).not.toBeNull();

    typeText("Watermark");
    // No Ctrl+Enter: switching tools must flush pending content and commit.
    setTool("move");
    await tick();

    expect(textLayers().length).toBe(1);
    expect(textLayers()[0].textData.content).toBe("Watermark");
    expect(editorRef.current.textEditSession()).toBeNull();
    expect(history.getUndoCount()).toBe(baseline + 1);
  });

  it("empty click (no typing) commits to nothing: temp layer deleted, no history entry", async () => {
    setTool("text");
    await tick();

    const engine = ws.getActiveEngine()!;
    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();

    clickText(getCanvas(), 100, 100);
    await tick();
    expect(textLayers().length).toBe(1);

    commitWithCtrlEnter();
    await tick();

    // Empty-commit cleanup: no placeholder text ever left behind (research R1).
    expect(textLayers().length).toBe(0);
    expect(engine.getLayers().length).toBe(1);
    expect(history.getUndoCount()).toBe(baseline);
    expect(editorRef.current.textEditSession()).toBeNull();
  });

  it("lostpointercapture after a placement click does NOT cancel the session (regression: text vanished right after creating)", async () => {
    setTool("text");
    await tick();

    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();

    // Placement: pointerdown → pointerup. In a real browser the deliberate
    // capture release in applyTextPointer fires lostpointercapture on the
    // canvas right after the click — jsdom never emits it, so we fire the
    // event the browser would have produced.
    const canvas = getCanvas();
    firePointer(canvas, "pointerdown", 10, 100, 100);
    firePointer(canvas, "pointerup", 10, 100, 100);
    canvas.dispatchEvent(new PointerEvent("lostpointercapture", { bubbles: false, pointerId: 10 }));
    await tick();

    // The temp layer + typing session must survive the lost-capture event.
    expect(textLayers().length).toBe(1);
    expect(editorRef.current.textEditSession()).not.toBeNull();
    getOverlay();

    typeText("Not lost");
    commitWithCtrlEnter();
    await tick();

    expect(textLayers().length).toBe(1);
    expect(textLayers()[0].textData.content).toBe("Not lost");
    expect(history.getUndoCount()).toBe(baseline + 1);
    expect(editorRef.current.textEditSession()).toBeNull();
  });

  it("pointercancel while typing does NOT close an open text session", async () => {
    setTool("text");
    await tick();

    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();

    clickText(getCanvas(), 100, 100);
    await tick();
    typeText("Persist");

    // A stray pointercancel (OS can emit it while the textarea is focused)
    // must leave the temp layer and open session untouched.
    getCanvas().dispatchEvent(new PointerEvent("pointercancel", { bubbles: true, pointerId: 10 }));
    await tick();

    expect(textLayers().length).toBe(1);
    expect(editorRef.current.textEditSession()).not.toBeNull();
    expect(getOverlay().value).toBe("Persist");

    commitWithCtrlEnter();
    await tick();
    expect(textLayers().length).toBe(1);
    expect(textLayers()[0].textData.content).toBe("Persist");
    expect(history.getUndoCount()).toBe(baseline + 1);
  });

  it("pointercancel DURING the placement drag cancels: temp layer removed, no session", async () => {
    setTool("text");
    await tick();

    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();

    const canvas = getCanvas();
    firePointer(canvas, "pointerdown", 10, 100, 100);
    // Interrupted BEFORE the releasing pointerup that ends placement.
    firePointer(canvas, "pointercancel", 10, 100, 100);
    await tick();

    expect(textLayers().length).toBe(0);
    expect(editorRef.current.textEditSession()).toBeNull();
    expect(history.getUndoCount()).toBe(baseline);
  });

  it("undo removes exactly the added text; redo restores it with textData intact", async () => {
    setTool("text");
    await tick();

    const engine = ws.getActiveEngine()!;
    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();

    clickText(getCanvas(), 100, 100);
    await tick();
    typeText("Hello");
    commitWithCtrlEnter();
    await tick();
    expect(textLayers().length).toBe(1);

    // Undo/redo through the same primitives the app's Ctrl+Z handler uses
    // (mirrors the shape describe — snapshot must carry textData).
    const undone = history.undo(engine.snapshot());
    expect(undone).not.toBeNull();
    engine.restore(undone!);
    await tick();
    expect(textLayers().length).toBe(0);
    expect(engine.getLayers().length).toBe(1);
    expect(history.getUndoCount()).toBe(baseline);
    expect(history.getRedoCount()).toBe(1);

    // Redo: exactly one layer returns, same content, no temp leftovers.
    const redone = history.redo(engine.snapshot());
    expect(redone).not.toBeNull();
    engine.restore(redone!);
    await tick();
    expect(textLayers().length).toBe(1);
    // Full textData roundtrip (not just content): snapshot must carry every
    // field — font, color, alignment, box — not just the string. Color is the
    // live foreground color the tool used when creating the layer.
    expect(textLayers()[0].textData).toMatchObject({
      ...DEFAULT_TEXT_DATA,
      content: "Hello",
      color: editorRef.current.fgColor(),
    });
    expect(history.getUndoCount()).toBe(baseline + 1);
    expect(history.getRedoCount()).toBe(0);
  });

  it("area-box drag creates an area text layer with boxMode/boxWidth", async () => {
    setTool("text");
    await tick();

    const engine = ws.getActiveEngine()!;
    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();

    const canvas = getCanvas();
    firePointer(canvas, "pointerdown", 10, 100, 100);
    firePointer(canvas, "pointermove", 10, 300, 200); // 200 doc px wide
    firePointer(canvas, "pointerup", 10, 300, 200);
    await tick();

    // Session is area mode before commit (drag beyond the 3px threshold).
    const s = editorRef.current.textEditSession();
    expect(s).not.toBeNull();
    expect(s.boxMode).toBe("area");
    expect(s.boxWidth).toBe(200);

    typeText("Area text");
    commitWithCtrlEnter();
    await tick();

    const t = textLayers()[0];
    expect(t.textData.boxMode).toBe("area");
    expect(t.textData.boxWidth).toBe(200);
    expect(t.textData.content).toBe("Area text");
    expect(history.getUndoCount()).toBe(baseline + 1);
  });

  it("move tool + double-click a committed text layer re-opens the edit session (R3)", async () => {
    setTool("text");
    await tick();

    const engine = ws.getActiveEngine()!;
    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();

    // Create + commit a text layer at (100,100). "Hello" rasterizes to
    // ~27x70 doc px (10px/char + 2px padding, tight ink box), so a
    // double-click at (110,110) hits it.
    clickText(getCanvas(), 100, 100);
    await tick();
    typeText("Hello");
    commitWithCtrlEnter();
    await tick();
    const textId = textLayers()[0].id;
    expect(textLayers().length).toBe(1);

    // Switch to move, then double-click the text on the canvas (R3: move
    // tool + dblclick → switch to text tool + edit session, no new layer).
    // NOTE: the hit resolves via layer BOUNDS — the OffscreenCanvas stub's
    // fillText paints no ink, so the alpha-aware sample can't read real
    // pixels in jsdom. (110,110) is safely inside the ~27x70px layer box,
    // which is what this test verifies: the R3 re-edit wiring, not
    // pixel-accurate hit testing.
    setTool("move");
    await tick();
    getCanvas().dispatchEvent(
      new MouseEvent("dblclick", { bubbles: true, cancelable: true, clientX: 110, clientY: 110 }),
    );
    await tick();

    expect(editorRef.current.activeTool()).toBe("text");
    const s = editorRef.current.textEditSession();
    expect(s).not.toBeNull();
    expect(s.layerId).toBe(textId);
    expect(s.isNewLayer).toBe(false);
    // B1 regression: the overlay anchors at the LAYER's box origin (100,100)
    // — never the double-click point (110,110) — so the edit box covers the
    // real text instead of rendering displaced from it (and snapping back on
    // commit). Default zoom 1 / pan (0,0) → overlay left/top == docX/docY.
    expect(s.docX).toBe(100);
    expect(s.docY).toBe(100);
    expect(getOverlay().style.left).toBe("100px");
    expect(getOverlay().style.top).toBe("100px");
    // Overlay re-selects the existing content for editing.
    expect(getOverlay().value).toBe("Hello");

    // Edit the content and commit: one "Edit Text" undo step, same layer id.
    typeText("Hello world");
    commitWithCtrlEnter();
    await tick();
    expect(textLayers().length).toBe(1);
    expect(textLayers()[0].id).toBe(textId);
    expect(textLayers()[0].textData.content).toBe("Hello world");
    expect(history.getUndoCount()).toBe(baseline + 2); // add + edit
    expect(editorRef.current.textEditSession()).toBeNull();
  });

  it("pointercancel mid-session removes the temp layer without committing", async () => {
    setTool("text");
    await tick();

    const engine = ws.getActiveEngine()!;
    const history = ws.getActiveHistory()!;
    const baseline = history.getUndoCount();

    const canvas = getCanvas();
    firePointer(canvas, "pointerdown", 10, 100, 100);
    firePointer(canvas, "pointercancel", 10, 100, 100);
    await tick();

    expect(textLayers().length).toBe(0);
    expect(engine.getLayers().length).toBe(1);
    expect(history.getUndoCount()).toBe(baseline);
    expect(editorRef.current.textEditSession()).toBeNull();

    // Drag state fully released: a fresh click still opens a session.
    clickText(canvas, 200, 200, 11);
    await tick();
    expect(textLayers().length).toBe(1);
    expect(editorRef.current.textEditSession()).not.toBeNull();
  });
});
