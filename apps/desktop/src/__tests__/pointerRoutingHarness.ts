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
    getLayer: (id: string) => ({ id, locked: false, visible: true, width: 100, height: 100 }),
    samplePixel: vi.fn(() => [128, 255, 64, 255]),
    getViewport: () => ({ panX: 0, panY: 0, zoom: 1 }),
    snapshot: vi.fn(() => ({})),
    getWidth: () => 100,
    getHeight: () => 100,
    getLayers: () => [],
    setActiveLayer: vi.fn(),
    getSelection: () => null,
    getLayerImageBitmap: () => undefined,
  } as unknown as DocumentEngine;

  let currentLastPaintCoords: any = null;

  const defaults: Record<string, any> = {
    workspace: {
      getActiveEngine: () => mockEngine,
      getActiveHistory: () => ({
        commit: vi.fn(),
        getLastPaintCoords: () => currentLastPaintCoords,
        setLastPaintCoords: (c: any) => { currentLastPaintCoords = c; },
      }),
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
    scheduler: { requestRender: vi.fn() },
  };

  const merged = { ...defaults };
  let dispose = () => {};
  const signals = createRoot((rootDispose) => {
    dispose = rootDispose;
    const ownedSignals: Record<string, any> = {
      workspace: merged.workspace,
      scheduler: merged.scheduler,
    };

    for (const [key, val] of Object.entries(merged)) {
      if (key === "workspace" || key === "scheduler") continue;
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
