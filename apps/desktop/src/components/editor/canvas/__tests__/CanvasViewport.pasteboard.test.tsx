import { afterEach, beforeEach, describe, expect, it, vi, assert } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { CanvasViewport } from "../CanvasViewport";
import { WorkspaceManager } from "@/engine/workspace";
import { ViewportCamera } from "@/viewport/viewportCamera";

// Mock useViewportRenderer
vi.mock("../useViewportRenderer", () => ({
  useViewportRenderer: () => ({
    isFitTransition: () => false,
    fitToScreenAndRender: vi.fn(),
    resizeRenderer: vi.fn(),
  }),
}));

// Mock useBrushOverlay
const { mockCommitBrushStroke, mockCancelBrushStroke } = vi.hoisted(() => ({
  mockCommitBrushStroke: vi.fn(),
  mockCancelBrushStroke: vi.fn(),
}));
vi.mock("../../useBrushOverlay", () => ({
  useBrushOverlay: () => ({
    onPaintStroke: vi.fn(),
    commitBrushStroke: mockCommitBrushStroke,
    cancelActiveStroke: mockCancelBrushStroke,
    isStrokeActive: () => true,
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
let setCropFillEnabledState: (enabled: boolean) => void = () => {};
let setCropFillSourceState: (source: "background" | "custom") => void = () => {};
let setCropFillCustomColorState: (color: string) => void = () => {};

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
  return null;
};

describe("CanvasViewport Pasteboard Clicks", () => {
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
    setCropInteractionMode("classic");
    return { session };
  }

  function dispatchPasteboardClick() {
    const viewportContainer = container.querySelector("[data-viewport-container]") as HTMLDivElement;
    if (!viewportContainer) throw new Error("Viewport container not found");

    viewportContainer.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true,
      cancelable: true,
      button: 0,
      pointerId: 1,
      clientX: 10,
      clientY: 10,
    }));
    viewportContainer.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true,
      cancelable: true,
      button: 0,
      pointerId: 1,
      clientX: 10,
      clientY: 10,
    }));
  }

  it("renders Classic crop fill preview from the current background color", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("classic");
    setCrop({ x: -20, y: -10, w: 120, h: 90 });
    setBgColorState("#224466");
    setCropFillEnabledState(true);
    setCropFillSourceState("background");

    const preview = container.querySelector("[data-crop-fill-preview='classic']") as HTMLElement | null;
    expect(preview).not.toBeNull();
    expect(preview!.style.backgroundColor).toBe("rgb(34, 68, 102)");
  });

  it("renders Modern crop fill preview with custom fill color", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    setCropFillEnabledState(true);
    setCropFillSourceState("custom");
    setCropFillCustomColorState("#778899");
    await new Promise((resolve) => setTimeout(resolve, 0));

    const preview = container.querySelector("[data-crop-fill-preview='modern']") as HTMLElement | null;
    expect(preview).not.toBeNull();
    expect(preview!.style.backgroundColor).toBe("rgb(119, 136, 153)");
  });

  it("positions Modern crop fill preview according to frame.x and frame.y", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    setCropFillEnabledState(true);
    setModernFrameState({ x: 120, y: 80, w: 200, h: 150 });
    await new Promise((resolve) => setTimeout(resolve, 0));

    const preview = container.querySelector("[data-crop-fill-preview='modern']") as HTMLElement | null;
    expect(preview).not.toBeNull();
    expect(preview!.style.left).toBe("120px");
    expect(preview!.style.top).toBe("80px");
    expect(preview!.style.width).toBe("200px");
    expect(preview!.style.height).toBe("150px");
  });

  it("clears Move tool selection without clearing the active paint layer when pasteboard is clicked", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("move");

    const layers = session.engine.getLayers();
    session.engine.setActiveLayer(layers[0].id);
    expect(session.engine.getActiveLayerId()).toBe(layers[0].id);

    dispatchPasteboardClick();

    expect(session.engine.getActiveLayerId()).toBe(layers[0].id);
    expect(scheduler.requestRender).toHaveBeenCalled();
  });

  it("clears selection preview when pasteboard is clicked in Selection tool mode", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("selection");

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    // Drag to create a selection box (keep pointer down to keep preview visible)
    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 10, clientY: 10, button: 0 }));
    canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 110, clientY: 110, button: 0 }));

    // Selection marquee should be visible
    expect(container.querySelector("rect[data-march]")).not.toBeNull();

    dispatchPasteboardClick();

    // Selection marquee should be cleared
    expect(container.querySelector("rect[data-march]")).toBeNull();
    expect(scheduler.requestRender).toHaveBeenCalled();
  });

  it("selection marquee stays visible after pointer up (no spurious clear)", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("selection");

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    // Full draw flow
    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 50, clientY: 50, button: 0, pointerId: 1 }));
    canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 150, clientY: 150, button: 0, pointerId: 1 }));
    canvas.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: 150, clientY: 150, button: 0, pointerId: 1 }));

    // Engine should now have a selection
    expect(session.engine.getSelection()).not.toBeNull();

    // Marquee should STILL be visible after pointer up
    expect(container.querySelector("rect[data-march]")).not.toBeNull();
  });

  it("clears the visible marquee when the engine is deselected outside the pointer chain", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("selection");

    session.engine.createSelection(10, 20, 100, 80);
    expect(container.querySelector("[data-selection-marquee]")).not.toBeNull();

    session.engine.clearSelection();
    expect(container.querySelector("[data-selection-marquee]")).toBeNull();
  });

  it("renders an inverted engine selection as canvas boundary plus excluded bounds", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("selection");

    session.engine.createSelection(10, 20, 100, 80);
    session.engine.invertSelection();

    expect(container.querySelector("[data-selection-inverted-boundary]")).not.toBeNull();
    expect(container.querySelector("[data-selection-group]")?.getAttribute("data-selection-inverted")).toBe("true");
  });

  it("selection marquee updates in real-time during drag (live preview)", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("selection");

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 10, clientY: 10, button: 0, pointerId: 1 }));

    canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 50, clientY: 50, button: 0, pointerId: 1 }));
    const rect1 = container.querySelector("rect[data-march]") as SVGRectElement | null;
    expect(rect1).not.toBeNull();
    const w1 = parseFloat(rect1!.getAttribute("width") || "0");

    canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 200, clientY: 200, button: 0, pointerId: 1 }));
    const rect2 = container.querySelector("rect[data-march]") as SVGRectElement | null;
    expect(rect2).not.toBeNull();
    const w2 = parseFloat(rect2!.getAttribute("width") || "0");

    // Width should have grown
    expect(w2).toBeGreaterThan(w1);

    canvas.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: 200, clientY: 200, button: 0, pointerId: 1 }));
  });

  it("SelectionOptionBar appears when selection is committed (engine state)", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("selection");

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 50, clientY: 50, button: 0, pointerId: 1 }));
    canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 150, clientY: 150, button: 0, pointerId: 1 }));
    canvas.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: 150, clientY: 150, button: 0, pointerId: 1 }));

    // Engine has selection
    expect(session.engine.getSelection()).not.toBeNull();
  });

  it("clicking inside an existing selection moves it (drag-in-selection)", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("selection");

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    // First, create a selection
    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 100, clientY: 100, button: 0, pointerId: 1 }));
    canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 200, clientY: 200, button: 0, pointerId: 1 }));
    canvas.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: 200, clientY: 200, button: 0, pointerId: 1 }));

    const initialSel = session.engine.getSelection();
    expect(initialSel).not.toBeNull();
    expect(initialSel!.x).toBe(100);
    expect(initialSel!.width).toBe(100);

    // Now click inside the selection and drag
    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 150, clientY: 150, button: 0, pointerId: 2 }));
    canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 200, clientY: 200, button: 0, pointerId: 2 }));
    canvas.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: 200, clientY: 200, button: 0, pointerId: 2 }));

    const newSel = session.engine.getSelection();
    expect(newSel).not.toBeNull();
    // Selection should have moved (x increased by 50)
    expect(newSel!.x).toBe(150);
    expect(newSel!.width).toBe(100); // width preserved
  });

  it("does not clear active layer when transform session is active", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("move");
    const layers = session.engine.getLayers();
    session.engine.setActiveLayer(layers[0].id);

    setSession({
      documentId: "doc-1",
      layerId: layers[0].id,
      originalSnapshot: {} as any,
      originalTransform: {} as any,
      mode: "resize",
      lockRatio: false,
      startedAt: Date.now(),
    });

    dispatchPasteboardClick();

    expect(session.engine.getActiveLayerId()).toBe(layers[0].id);
  });

  it("clears crop preview but keeps tool active when pasteboard is clicked in Crop mode", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCrop({ x: 10, y: 10, w: 200, h: 200 });

    dispatchPasteboardClick();

    expect(getCrop()).toBeNull();
    expect(getCropRotation()).toBe(0);
  });

  it("canvas click in Crop mode with no crop box restores default crop box and centers viewport", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCrop(null);
    session.engine.setViewport({ panX: 100, panY: 150, zoom: 1 });

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    const downEvent = new PointerEvent("pointerdown", { bubbles: true, cancelable: true, button: 0, clientX: 50, clientY: 50 });
    canvas.dispatchEvent(downEvent);

    const upEvent = new PointerEvent("pointerup", { bubbles: true, cancelable: true, button: 0, clientX: 50, clientY: 50 });
    canvas.dispatchEvent(upEvent);

    expect(getCrop()).toEqual({ x: 0, y: 0, w: 800, h: 600 });
    expect(session.engine.getViewport().panX).toBe(0);
    expect(session.engine.getViewport().panY).toBe(0);
  });

  it("does not clear active layer when paint tools are active", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("brush");
    const layers = session.engine.getLayers();
    session.engine.setActiveLayer(layers[0].id);

    dispatchPasteboardClick();

    expect(session.engine.getActiveLayerId()).toBe(layers[0].id);
  });

  it("hides crop preview on pasteboard click and preserves it for restore", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("crop");
    setCrop({ x: 12, y: 18, w: 200, h: 140 });
    setCropRotation(15);

    dispatchPasteboardClick();

    expect(getCrop()).toBeNull();
    expect(getHiddenCropPreview()).toEqual({
      rect: { x: 12, y: 18, w: 200, h: 140 },
      rotation: 15,
    });
  });

  it("restores hidden crop preview on canvas click without drag", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("crop");
    setCrop(null);
    setHiddenCropPreview({
      rect: { x: 25, y: 35, w: 160, h: 120 },
      rotation: -10,
    });

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, button: 0, clientX: 100, clientY: 100, pointerId: 1 }));
    canvas.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, button: 0, clientX: 100, clientY: 100, pointerId: 1 }));

    expect(getCrop()).toEqual({ x: 25, y: 35, w: 160, h: 120 });
    expect(getCropRotation()).toBe(-10);
    expect(getHiddenCropPreview()).toBeNull();
  });

  it("creates full-canvas crop preview on canvas click only when no hidden preview exists", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("crop");
    setCrop(null);
    setHiddenCropPreview(null);

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, button: 0, clientX: 100, clientY: 100, pointerId: 1 }));
    canvas.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, button: 0, clientX: 100, clientY: 100, pointerId: 1 }));

    expect(getCrop()).toEqual({ x: 0, y: 0, w: 800, h: 600 });
    expect(getCropRotation()).toBe(0);
  });

  it("creates replacement crop preview from pasteboard drag instead of hiding preview", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("crop");
    setCrop({ x: 120, y: 120, w: 180, h: 120 });
    setHiddenCropPreview({
      rect: { x: 24, y: 32, w: 140, h: 100 },
      rotation: -12,
    });

    const viewportContainer = container.querySelector("[data-viewport-container]") as HTMLDivElement;
    if (!viewportContainer) throw new Error("Viewport container not found");

    viewportContainer.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 10, clientY: 10, button: 0, pointerId: 1 }));
    viewportContainer.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 160, clientY: 140, button: 0, pointerId: 1 }));
    viewportContainer.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: 160, clientY: 140, button: 0, pointerId: 1 }));

    expect(getHiddenCropPreview()).toBeNull();
    expect(getCropRotation()).toBe(0);
    expect(getCrop()).not.toBeNull();
    expect(getCrop()).not.toEqual({ x: 120, y: 120, w: 180, h: 120 });
  });

  it("treats small pasteboard movement as click hide, not replacement drag", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("crop");
    setCrop({ x: 120, y: 120, w: 180, h: 120 });

    const viewportContainer = container.querySelector("[data-viewport-container]") as HTMLDivElement;
    if (!viewportContainer) throw new Error("Viewport container not found");

    viewportContainer.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 10, clientY: 10, button: 0, pointerId: 1 }));
    viewportContainer.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 11, clientY: 12, button: 0, pointerId: 1 }));
    viewportContainer.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: 11, clientY: 12, button: 0, pointerId: 1 }));

    expect(getCrop()).toBeNull();
    expect(getHiddenCropPreview()).toEqual({
      rect: { x: 120, y: 120, w: 180, h: 120 },
      rotation: 0,
    });
  });

  it("allows pasteboard panning in Crop mode while Space is held", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("crop");
    setCrop({ x: 120, y: 120, w: 180, h: 120 });
    mockSpacePressed = true;

    const viewportContainer = container.querySelector("[data-viewport-container]") as HTMLDivElement;
    if (!viewportContainer) throw new Error("Viewport container not found");

    const down = new PointerEvent("pointerdown", {
      bubbles: true,
      cancelable: true,
      clientX: 10,
      clientY: 10,
      button: 0,
      pointerId: 1,
    });
    viewportContainer.dispatchEvent(down);

    expect(mockOnViewportPointerDown).toHaveBeenCalledWith(expect.objectContaining({ pointerId: 1 }));
    expect(getCrop()).toEqual({ x: 120, y: 120, w: 180, h: 120 });
  });

  it("discards brush stroke on pointercancel during active brush drag (no commit)", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("brush");

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 50, clientY: 50, button: 0, pointerId: 99 }));
    canvas.dispatchEvent(new PointerEvent("pointercancel", { bubbles: true, pointerId: 99 }));

    // Design contract (2026-08-24): pointercancel = DISCARD — the stroke must
    // never permanently paint. cancelBrushStroke runs; commit must not.
    expect(mockCancelBrushStroke).toHaveBeenCalled();
    expect(mockCommitBrushStroke).not.toHaveBeenCalled();
  });

  it("Escape during an active brush stroke discards it (no commit)", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("brush");

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 50, clientY: 50, button: 0, pointerId: 99 }));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));

    expect(mockCancelBrushStroke).toHaveBeenCalled();
    expect(mockCommitBrushStroke).not.toHaveBeenCalled();
  });

  it("commits brush stroke on lostpointercapture during active brush drag", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("brush");

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 50, clientY: 50, button: 0, pointerId: 99 }));
    canvas.dispatchEvent(new PointerEvent("lostpointercapture", { bubbles: true, pointerId: 99 }));

    expect(mockCommitBrushStroke).toHaveBeenCalled();
  });

  it("does not start brush drag when isPanning is true", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("brush");
    mockPanningActive = true;

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 50, clientY: 50, button: 0, pointerId: 42 }));
    canvas.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 100, clientY: 100, button: 0, pointerId: 42 }));
    canvas.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: 100, clientY: 100, button: 0, pointerId: 42 }));

    // No brush stroke should be committed since drag was blocked
    expect(mockCommitBrushStroke).not.toHaveBeenCalled();
  });

  it("routes pointerdown event to container panning handler when isPanning blocks canvas", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("selection");
    mockPanningActive = true;

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, clientX: 100, clientY: 100, button: 0, pointerId: 7 }));

    // Event should bubble to container →onViewportPointerDown should be called
    expect(mockOnViewportPointerDown).toHaveBeenCalledWith(
      expect.objectContaining({ pointerId: 7, button: 0 })
    );
  });

  it("calls onViewportPointerCancel when container receives pointercancel", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    const viewportContainer = container.querySelector("[data-viewport-container]") as HTMLDivElement;
    if (!viewportContainer) throw new Error("Viewport container not found");

    viewportContainer.dispatchEvent(new PointerEvent("pointercancel", { bubbles: false, pointerId: 5 }));

    expect(mockOnViewportPointerCancel).toHaveBeenCalledWith(
      expect.objectContaining({ pointerId: 5 })
    );
  });

  it("calls onViewportLostPointerCapture when container receives lostpointercapture", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    const viewportContainer = container.querySelector("[data-viewport-container]") as HTMLDivElement;
    if (!viewportContainer) throw new Error("Viewport container not found");

    viewportContainer.dispatchEvent(new PointerEvent("lostpointercapture", { bubbles: false, pointerId: 3 }));

    expect(mockOnViewportLostPointerCapture).toHaveBeenCalledWith(
      expect.objectContaining({ pointerId: 3 })
    );
  });

  it("sets pointer capture on canvas pointerdown when isPanning is false", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("brush");

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 50, clientY: 50, button: 0, pointerId: 8 }));

    expect(Element.prototype.setPointerCapture).toHaveBeenCalledWith(8);
  });

  it("does not set pointer capture on canvas pointerdown when isPanning is true", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));
    setTool("brush");
    mockPanningActive = true;

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    canvas.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 50, clientY: 50, button: 0, pointerId: 9 }));

    // The isPanning guard should prevent setPointerCapture
    // It may have been called by other code, so check the last call args
    const calls = (Element.prototype.setPointerCapture as ReturnType<typeof vi.fn>).mock.calls;
    const lastCall = calls[calls.length - 1];
    // If there are calls, the last one should NOT be for pointerId 9 (our test event)
    if (lastCall) {
      expect(lastCall[0]).not.toBe(9);
    }
  });

  function dispatchModernCanvasDrag(
    fromX: number, fromY: number, toX: number, toY: number,
    options?: { shiftKey?: boolean }
  ) {
    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    if (!canvas) throw new Error("Canvas not found");

    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: fromX, clientY: fromY,
      ...(options?.shiftKey ? { shiftKey: true } : {}),
    }));

    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: toX, clientY: toY,
      ...(options?.shiftKey ? { shiftKey: true } : {}),
    }));

    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: toX, clientY: toY,
      ...(options?.shiftKey ? { shiftKey: true } : {}),
    }));
  }

  it("creates default frame on canvas click in Modern mode with no frame and centers viewport", async () => {
    const { session } = renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    resetModernCropState();
    expect(getModernFrame()).toBeNull();
    session.engine.setViewport({ panX: 120, panY: 170, zoom: 1 });

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    expect(canvas).not.toBeNull();

    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 50, clientY: 50,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 50, clientY: 50,
    }));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();
    expect(frame!.w).toBeGreaterThan(0);
    expect(frame!.h).toBeGreaterThan(0);
    expect(session.engine.getViewport().panX).toBe(0);
    expect(session.engine.getViewport().panY).toBe(0);
  });

  it("drag below 5px threshold creates default frame on pointerup", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    resetModernCropState();
    expect(getModernFrame()).toBeNull();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    // Drag 3px horizontally →below 5px threshold
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 100, clientY: 100,
    }));
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 103, clientY: 100,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 103, clientY: 100,
    }));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();
    expect(frame!.w).toBeGreaterThan(0);
    expect(frame!.h).toBeGreaterThan(0);
  });

  it("drag above 5px threshold creates frame with expected aspect", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    setCropModeState("free");
    resetModernCropState();
    expect(getModernFrame()).toBeNull();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    // Drag horizontally (200px →wider than tall)
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 100, clientY: 100,
    }));
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 300, clientY: 150,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 300, clientY: 150,
    }));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();
    // Wider-than-tall drag →frame should be wider than tall
    expect(frame!.w).toBeGreaterThan(frame!.h);
    expect(frame!.x).toBe((800 - frame!.w) / 2);
    expect(frame!.y).toBe((600 - frame!.h) / 2);
  });

  it("Shift+drag in Free mode creates roughly square frame", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    setCropModeState("free");
    resetModernCropState();
    expect(getModernFrame()).toBeNull();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    // Drag horizontally with shiftKey →should be square
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 100, clientY: 100, shiftKey: true,
    }));
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 300, clientY: 150, shiftKey: true,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 300, clientY: 150, shiftKey: true,
    }));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();
    // Should be roughly square: ratio < 1.1
    const ratio = Math.max(frame!.w, frame!.h) / Math.min(frame!.w, frame!.h);
    expect(ratio).toBeLessThan(1.1);
  });

  it("Ratio mode drag uses option bar aspect, not drag direction", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    setCropModeState("ratio");
    setCropAspectState({ w: 16, h: 9 });
    resetModernCropState();
    expect(getModernFrame()).toBeNull();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    // Drag tall (narrow horizontally) →but ratio mode should use 16:9
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 100, clientY: 100,
    }));
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 105, clientY: 300,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 105, clientY: 300,
    }));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();
    // Should be 16:9 (wider than tall), not tall
    expect(frame!.w / frame!.h).toBeGreaterThan(1);
  });

  it("pointercancel mid-drag cleans up state and allows subsequent click", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    resetModernCropState();
    expect(getModernFrame()).toBeNull();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 100, clientY: 100,
    }));

    // Cancel mid-drag
    canvas.dispatchEvent(new PointerEvent("pointercancel", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
    }));

    // No frame should have been created
    expect(getModernFrame()).toBeNull();

    // Subsequent click should still create a frame
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 2,
      clientX: 100, clientY: 100,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 2,
      clientX: 100, clientY: 100,
    }));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();
    expect(frame!.w).toBeGreaterThan(0);
  });

  it("lostpointercapture mid-drag cleans up state", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    resetModernCropState();
    expect(getModernFrame()).toBeNull();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 100, clientY: 100,
    }));

    canvas.dispatchEvent(new PointerEvent("lostpointercapture", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
    }));

    // No frame should have been created (threshold not exceeded)
    expect(getModernFrame()).toBeNull();
  });

  it("drag above threshold creates frame on pointerup with proper sizing (WYSIWYG)", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    setCropModeState("free");
    resetModernCropState();
    expect(getModernFrame()).toBeNull();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;

    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 100, clientY: 100,
    }));

    // Move beyond threshold →frame should NOT exist yet (only preview rect shown during drag)
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 300, clientY: 200,
    }));

    expect(getModernFrame()).toBeNull();

    // Drag further →still no frame
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 400, clientY: 150,
    }));

    expect(getModernFrame()).toBeNull();

    // Pointerup →frame should be created
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 1,
      clientX: 400, clientY: 150,
    }));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();
    expect(frame!.w).toBeGreaterThan(0);
    expect(frame!.h).toBeGreaterThan(0);
  });

  it("WYSIWYG: frame size matches selection exactly in Free mode", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    setCropModeState("free");
    resetModernCropState();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    const fromX = 100, fromY = 100, toX = 400, toY = 300;
    const expectedW = Math.abs(toX - fromX); // 300
    const expectedH = Math.abs(toY - fromY); // 200

    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: fromX, clientY: fromY,
    }));
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: toX, clientY: toY,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: toX, clientY: toY,
    }));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();
    expect(frame!.w).toBe(expectedW);
    expect(frame!.h).toBe(expectedH);
  });

  it("reverse drag direction produces same frame size", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    setCropModeState("free");
    resetModernCropState();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    // Drag right-to-left then bottom-to-top →same |dx|,|dy|
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 400, clientY: 200,
    }));
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 150, clientY: 100,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 150, clientY: 100,
    }));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();
    expect(frame!.w).toBe(250);
    expect(frame!.h).toBe(100);
  });

  it("image transform offset shifts selection center to viewport center", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    setCropModeState("free");
    resetModernCropState();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    // Selection from (100,100) to (400,350)
    // selCenter = (250, 225), vpCenter = (400, 300)
    // offset = (400-250, 300-225) = (150, 75)
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 100, clientY: 100,
    }));
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 400, clientY: 350,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 400, clientY: 350,
    }));

    const t = getModernImageTransform();
    expect(t.offsetX).toBe(150);
    expect(t.offsetY).toBe(75);
  });

  it("Size mode drag creates frame at target size regardless of selection", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    setCropModeState("size");
    setCropSizeTargetState({ w: 200, h: 150 });
    resetModernCropState();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    // Drag a 400x300 selection →frame should still be 200x150
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 100, clientY: 100,
    }));
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 500, clientY: 400,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 500, clientY: 400,
    }));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();
    expect(frame!.w).toBe(200);
    expect(frame!.h).toBe(150);
  });

  it("preview rect shown during drag, hidden after pointerup", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    setCropModeState("free");
    resetModernCropState();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;

    // Pointerdown →no preview yet
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 100, clientY: 100,
    }));

    // Move beyond threshold →preview should appear
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 300, clientY: 200,
    }));

    const previewDuringDrag = container.querySelector("[data-crop-drag-preview]");
    expect(previewDuringDrag).not.toBeNull();

    // Pointerup →preview should be removed
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 300, clientY: 200,
    }));

    // Let SolidJS batch settle
    await new Promise((resolve) => setTimeout(resolve, 50));
    const previewAfter = container.querySelector("[data-crop-drag-preview]");
    expect(previewAfter).toBeNull();
  });

  it("modern crop: drag-dismiss-redrag cycle works", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    setCropModeState("free");
    resetModernCropState();
    expect(getModernFrame()).toBeNull();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;

    // First drag →create frame
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 100, clientY: 100,
    }));
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 300, clientY: 200,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 300, clientY: 200,
    }));

    expect(getModernFrame()).not.toBeNull();
    const firstW = getModernFrame()!.w;

    // Dismiss via Escape
    resetModernCropState();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(getModernFrame()).toBeNull();

    // Second drag with different direction
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 20,
      clientX: 50, clientY: 50,
    }));
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 20,
      clientX: 350, clientY: 300,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 20,
      clientX: 350, clientY: 300,
    }));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();
    expect(frame!.w).toBe(300);
    expect(frame!.h).toBe(250);
    // Different from first frame's size (which was 200x100)
    expect(frame!.w).not.toBe(firstW);
  });

  it("vertical drag creates taller-than-wide frame", async () => {
    renderViewport();
    await new Promise((resolve) => setTimeout(resolve, 0));

    setTool("crop");
    setCropInteractionMode("modern");
    setCropModeState("free");
    resetModernCropState();

    const canvas = container.querySelector("canvas") as HTMLCanvasElement;

    // Drag vertically →100px wide, 300px tall
    canvas.dispatchEvent(new PointerEvent("pointerdown", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 200, clientY: 100,
    }));
    canvas.dispatchEvent(new PointerEvent("pointermove", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 300, clientY: 400,
    }));
    canvas.dispatchEvent(new PointerEvent("pointerup", {
      bubbles: true, cancelable: true, button: 0, pointerId: 10,
      clientX: 300, clientY: 400,
    }));

    const frame = getModernFrame();
    expect(frame).not.toBeNull();
    expect(frame!.h).toBeGreaterThan(frame!.w);
    expect(frame!.w).toBe(100);
    expect(frame!.h).toBe(300);
  });
});
