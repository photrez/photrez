// apps/desktop/src/__tests__/pointerRoutingHarness.ts
//
// Shared harness for pointer-routing tests. Deliberately free of any
// `vi.mock(...)` so each test file can opt IN or OUT of mocking
// `@/viewport/input-handler` independently:
//   - pointerToolRouting.test.tsx mocks it to spy on routing
//   - eyedropper-regression.test.tsx uses the REAL handler

import { vi } from "vitest";
import { createRoot, createSignal } from "solid-js";
import { useCanvasPointerTools } from "../components/editor/canvas/useCanvasPointerTools";
import type { DocumentEngine } from "@/engine/document";

export function createMockEditorParams(toolId: string) {
  const mockEngine = {
    getActiveLayerId: () => "layer-1",
    getLayer: (id: string) => ({ id, locked: false, visible: true, width: 100, height: 100, transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 } }),
    samplePixel: vi.fn(() => [128, 255, 64, 255]),
    getViewport: () => ({ panX: 0, panY: 0, zoom: 1 }),
    snapshot: vi.fn(() => ({})),
    getWidth: () => 100,
    getHeight: () => 100,
    getLayers: () => [],
    setActiveLayer: vi.fn(),
    getSelection: () => null,
    getLayerImageBitmap: () => (typeof document !== "undefined" ? document.createElement("canvas") : ({} as any)),
    setLayerImageBitmap: vi.fn(),
    transformLayer: vi.fn(),
    isShapeLayer: vi.fn(() => false),
    addShapeLayer: vi.fn(() => ({ id: "shape-1", type: "shape", width: 1, height: 1 })),
    updateShapeParams: vi.fn(),
    isTextLayer: vi.fn(() => false),
    addTextLayer: vi.fn(() => ({ id: "text-1", type: "text", width: 1, height: 1 })),
    updateTextData: vi.fn(),
    sampleLayerAlpha: vi.fn(() => 1),
    deleteLayer: vi.fn(),
  } as unknown as DocumentEngine;

  let currentLastPaintCoords: any = null;
  const historyMock = {
    commit: vi.fn(),
    getLastPaintCoords: () => currentLastPaintCoords,
    setLastPaintCoords: (c: any) => { currentLastPaintCoords = c; },
  };
  (mockEngine as any).__historyMock = historyMock;

  const defaults: Record<string, any> = {
    workspace: {
      getActiveEngine: () => mockEngine,
      getActiveHistory: () => historyMock,
    },
    activeTool: toolId,
    fgColor: "#000000",
    bgColor: "#ffffff",
    colorPickerOpen: false,
    colorPickerTarget: "foreground",
    zoom: 1,
    pan: { x: 0, y: 0 },
    docWidth: 100,
    docHeight: 100,
    brushSize: 20,
    brushHardness: 0.8,
    brushOpacity: 1.0,
    eraserSize: 20,
    eraserHardness: 0.8,
    eraserOpacity: 1.0,
    brushFlow: 1,
    brushSmoothing: 0,
    eraserFlow: 1,
    eraserSmoothing: 0,
    moveAutoSelect: false,
    moveSnapEnabled: false,
    cropInteractionMode: "classic",
    cropRect: null,
    setCropRect: vi.fn(),
    cropRotation: 0,
    setCropRotation: vi.fn(),
    hiddenCropPreview: null,
    setHiddenCropPreview: vi.fn(),
    viewportWidth: 1024,
    viewportHeight: 768,
    setHoverPos: vi.fn(),
    selectedLayerId: null,
    setSelectedLayerId: vi.fn(),
    fillTolerance: 32,
    fillContiguous: true,
    gradientType: "linear",
    gradientPreset: "fg-bg",
    gradientDragLine: null,
    setGradientDragLine: vi.fn(),
    setHoverHandle: vi.fn(),
    setViewportState: vi.fn(),
    renderer: { uploadImage: vi.fn() },
    scheduler: { requestRender: vi.fn() },
    shapeKind: "rect",
    shapeFillEnabled: true,
    shapeStrokeEnabled: false,
    shapeStrokeColor: "#000000",
    shapeStrokeWidth: 4,
    shapeRadius: 0,
    shapeArrowHead: false,
    textFontFamily: "Arial",
    textFontSize: 48,
    textFontWeight: 400,
    textFontItalic: false,
    textAlign: "left",
    textEditSession: null,
  };

  const merged = { ...defaults };
  let dispose = () => {};
  const signals = createRoot((rootDispose) => {
    dispose = rootDispose;
    const ownedSignals: Record<string, any> = {
      workspace: merged.workspace,
      scheduler: merged.scheduler,
      renderer: merged.renderer,
    };

    for (const [key, val] of Object.entries(merged)) {
      if (key === "workspace" || key === "scheduler" || key === "renderer") continue;
      const [s, set] = createSignal(val);
      ownedSignals[key] = s;
      const setKey = "set" + key.charAt(0).toUpperCase() + key.slice(1);
      ownedSignals[setKey] = set;
    }
    return ownedSignals;
  });

  return { signals, mockEngine, dispose };
}

export function createPointerTools(params: any) {
  return createRoot((dispose) => ({ tools: useCanvasPointerTools(params), dispose }));
}

export function makePointerEvent(overrides: Partial<PointerEvent> = {}): PointerEvent {
  return {
    button: 0,
    clientX: 50,
    clientY: 50,
    pointerId: 1,
    shiftKey: false,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    target: document.createElement("div"),
    currentTarget: document.createElement("div"),
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
    ...overrides,
  } as unknown as PointerEvent;
}
