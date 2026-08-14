import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { useCanvasKeyboard } from "../canvas/useCanvasKeyboard";
import { clearRegistry } from "../keyboardRegistry";
import { applyCropPreview } from "../cropToolActions";

const editorMock = vi.hoisted(() => ({ current: null as any }));

vi.mock("../shell/EditorContext", () => ({
  useEditor: () => editorMock.current,
}));

vi.mock("../cropToolActions", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../cropToolActions")>();
  return {
    ...actual,
    applyCropPreview: vi.fn(),
    discardCropSession: vi.fn(),
  };
});

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
  return <div />;
}

function makeEditor(overrides: Partial<Record<string, any>> = {}) {
  const requestRender = vi.fn();
  return {
    workspace: {
      getActiveEngine: () => ({
        getViewport: () => ({ panX: 0, panY: 0 }),
        setViewport: vi.fn(),
      }),
      getActiveHistory: () => ({ canUndo: () => false }),
    },
    renderer: {},
    activeTool: () => "crop",
    setActiveTool: vi.fn(),
    textEditSession: vi.fn(() => null),
    zoom: () => 1,
    docWidth: () => 800,
    docHeight: () => 600,
    activeLayerId: () => null,
    cropRect: () => null,
    setCropRect: vi.fn(),
    cropMode: () => "free",
    cropSizeTarget: () => null,
    cropRotation: () => 0,
    setCropRotation: vi.fn(),
    hiddenCropPreview: () => null,
    setHiddenCropPreview: vi.fn(),
    cropInteractionMode: () => "modern",
    undoLastCrop: vi.fn(),
    redoCrop: vi.fn(),
    canCropUndo: () => false,
    canCropRedo: () => false,
    undoModernCrop: vi.fn(),
    redoModernCrop: vi.fn(),
    commitModernCropState: vi.fn(),
    commitCropState: vi.fn(),
    cropDeletePixels: () => false,
    modernCropFrame: () => ({ w: 400, h: 300 }),
    modernCropImageTransform: () => ({ offsetX: 3, offsetY: 4, rotation: 15, scale: 1 }),
    setModernCropImageTransform: vi.fn(),
    resetModernCrop: vi.fn(),
    viewportWidth: () => 1000,
    viewportHeight: () => 800,
    pan: () => ({ x: 100, y: 80 }),
    layerTransformSession: () => null,
    setLayerTransformSession: vi.fn(),
    brushSize: () => 20,
    setBrushSize: vi.fn(),
    eraserSize: () => 20,
    setEraserSize: vi.fn(),
    brushHardness: () => 50,
    setBrushHardness: vi.fn(),
    eraserHardness: () => 50,
    setEraserHardness: vi.fn(),
    brushOpacity: () => 1,
    brushFlow: () => 1,
    brushSmoothing: () => 0,
    eraserOpacity: () => 1,
    eraserFlow: () => 1,
    eraserSmoothing: () => 0,
    ...overrides,
    scheduler: overrides.scheduler ?? { requestRender },
  };
}

function renderHarness(editor = makeEditor()) {
  editorMock.current = editor;
  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(() => <KeyboardHarness />, container);
  return {
    editor,
    dispose: () => {
      dispose();
      container.parentNode?.removeChild(container);
      editorMock.current = null;
    },
  };
}

describe("Modern Crop keyboard parity", () => {
  afterEach(() => {
    vi.clearAllMocks();
    clearRegistry();
  });

  it("applies Modern Crop on Enter using the crop engine rotation convention", () => {
    const editor = makeEditor();
    const { dispose } = renderHarness(editor);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    expect(applyCropPreview).toHaveBeenCalledWith(expect.objectContaining({
      cropRotation: -15,
      cropMode: "free",
    }));
    expect(editor.resetModernCrop).toHaveBeenCalled();
    dispose();
  });

  it("cancels Modern Crop on Escape", () => {
    const editor = makeEditor();
    const { dispose } = renderHarness(editor);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));

    expect(editor.resetModernCrop).toHaveBeenCalled();
    dispose();
  });

  it("nudges the Modern Crop image by arrow keys with Shift using the editor step convention", () => {
    const editor = makeEditor();
    const { dispose } = renderHarness(editor);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", shiftKey: true, bubbles: true }));

    expect(editor.setModernCropImageTransform).toHaveBeenCalledWith({
      offsetX: 13,
      offsetY: 4,
      rotation: 15,
      scale: 1,
    });
    expect(editor.scheduler.requestRender).toHaveBeenCalled();
    dispose();
  });

  it("keeps Modern Crop undo and redo shortcuts aligned with editor conventions", () => {
    const editor = makeEditor();
    const { dispose } = renderHarness(editor);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    expect(editor.undoModernCrop).toHaveBeenCalledTimes(1);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "y", ctrlKey: true, bubbles: true }));
    expect(editor.redoModernCrop).toHaveBeenCalledTimes(1);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, shiftKey: true, bubbles: true }));
    expect(editor.redoModernCrop).toHaveBeenCalledTimes(2);
    expect(editor.undoModernCrop).toHaveBeenCalledTimes(1);
    dispose();
  });
});
