// apps/desktop/src/components/editor/__tests__/modernCropDrag.test.tsx
//
// Wiring contract for the modern-crop drag-to-create pointer chain
// (pointerTools/modernCrop.ts). The ModernCropOverlay SVG stops events that
// land on the frame itself; mask-area clicks fall through to the canvas and
// must start a drag-create. No existing test covered this chain:
//
//   pointerdown → startCropDrag (state.start set; classic arms pending-click)
//   pointermove → trackModernCropDrag (threshold → preview + clear frame)
//   pointerup   → handleCropPointerUp (commitDragCreateFrame / click fallback)
//
// Pattern: set crop + modern → fire pointer chain on the hook → assert the
// editor signals (setModernCropFrame / setModernCropImageTransform /
// setViewportState) are mutated.

import { describe, it, expect, vi, afterEach } from "vitest";
import { createSignal } from "solid-js";
import { useCanvasPointerTools } from "../canvas/useCanvasPointerTools";
import * as InputHandlerModule from "@/viewport/input-handler";
import * as EditorContextModule from "../shell/EditorContext";
import * as CropToolActions from "../cropToolActions";
import { createMockEditorParams, createPointerTools, makePointerEvent } from "../../../__tests__/pointerRoutingHarness";

// Mock the shared input-handler so the chain stops at our handlers.
vi.mock("@/viewport/input-handler", () => ({
  handlePointerDown: vi.fn(),
  handlePointerMove: vi.fn(),
  handlePointerUp: vi.fn(),
  ToolType: null,
  ToolContext: null,
}));

// Deterministic geometry: frame math is tested by modernCropGeometry tests.
vi.mock("@/viewport/modernCropGeometry", () => ({
  getDefaultModernCropFrame: vi.fn(() => ({ x: 0, y: 0, w: 400, h: 300 })),
  clampFrameToProjectedBounds: vi.fn((frame: any) => frame),
}));

// Classic pending-click fallback path calls cropToolActions; mock it so the
// test asserts the wiring (restore called) without real layer mutations.
vi.mock("../cropToolActions", () => ({
  resetCropPreviewToCanvas: vi.fn(),
  restoreHiddenCropPreview: vi.fn(() => true),
}));

const baseParams = {
  getCanvasContainerRef: () => document.createElement("div"),
  getCanvasRef: () => document.createElement("canvas"),
  isSpacePressed: () => false,
  isPanning: () => false,
  isAltPressed: () => false,
  stopMomentum: vi.fn(),
  fitToScreenAndRender: vi.fn(),
  commitBrushStroke: vi.fn(),
  cropSnapTargets: () => [],
  moveSnapEnabled: () => false,
};

// Wrap a signal setter in a spy that still updates the signal (the harness
// creates plain setters, not vi.fn, so assertions need a wrapper).
function spyOnSetter<T>(signals: any, setterName: string, fallback?: (v: T) => void) {
  const setter = signals[setterName];
  const spy = vi.fn((v: T) => {
    if (typeof setter === "function") setter(v);
    else fallback?.(v);
  });
  signals[setterName] = spy;
  return spy;
}

function makeModernCropSignals(initialFrame: { x: number; y: number; w: number; h: number } | null = null) {
  const { signals, dispose } = createMockEditorParams("crop");
  const [mode] = createSignal<"modern" | "classic">("modern");
  signals.cropInteractionMode = mode;
  const [cropMode] = createSignal<"free" | "ratio" | "size">("free");
  signals.cropMode = cropMode;
  const [cropAspect] = createSignal<{ w: number; h: number } | null>(null);
  signals.cropAspect = cropAspect;
  const [cropSizeTarget] = createSignal<{ w: number; h: number } | null>(null);
  signals.cropSizeTarget = cropSizeTarget;
  const [modernCropFrame, setModernCropFrameInner] = createSignal(initialFrame);
  signals.modernCropFrame = modernCropFrame;
  signals.setModernCropFrame = vi.fn((f: any) => setModernCropFrameInner(f));
  const [modernCropImageTransform, setModernCropImageTransformInner] = createSignal({
    offsetX: 0,
    offsetY: 0,
    rotation: 0,
    scale: 1,
  });
  signals.modernCropImageTransform = modernCropImageTransform;
  signals.setModernCropImageTransform = vi.fn((t: any) => setModernCropImageTransformInner(t));
  return { signals, dispose };
}

function mountPointerTools(signals: any) {
  vi.spyOn(EditorContextModule, "useEditor").mockReturnValue(signals as any);
  return createPointerTools({ ...baseParams });
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("modern crop drag-to-create (pointer chain)", () => {
  it("commits a frame sized from the drag rect and re-centers the image", () => {
    const { signals, dispose } = makeModernCropSignals();
    const { tools, dispose: disposeTools } = mountPointerTools(signals);

    tools.onCanvasPointerDown(makePointerEvent({ clientX: 200, clientY: 150 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 400, clientY: 350 }));
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 400, clientY: 350 }));

    // Threshold exceeded → existing frame cleared (none here, still called).
    expect(signals.setModernCropFrame).toHaveBeenCalledWith(null);
    // Drag rect 200x200 in doc space (pan 0, zoom 1, container rect at 0,0)
    // → frame w/h 200, centered on viewport center (512, 384).
    expect(signals.setModernCropFrame).toHaveBeenLastCalledWith(
      expect.objectContaining({ x: 412, y: 284, w: 200, h: 200 }),
    );
    // Image shifted so drag-selection center (300, 250) maps to viewport center.
    expect(signals.setModernCropImageTransform).toHaveBeenLastCalledWith(
      expect.objectContaining({ offsetX: 212, offsetY: 134 }),
    );
    expect(signals.scheduler.requestRender).toHaveBeenCalled();

    disposeTools();
    dispose();
  });

  it("clears an existing frame once the drag exceeds the threshold", () => {
    const { signals, dispose } = makeModernCropSignals({ x: 100, y: 100, w: 300, h: 200 });
    const { tools, dispose: disposeTools } = mountPointerTools(signals);

    tools.onCanvasPointerDown(makePointerEvent({ clientX: 200, clientY: 150 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 400, clientY: 350 }));

    // Frame cleared during drag → visual feedback that a new crop is created.
    expect(signals.setModernCropFrame.mock.calls[0]).toEqual([null]);

    tools.onCanvasPointerUp(makePointerEvent({ clientX: 400, clientY: 350 }));
    expect(signals.setModernCropFrame).toHaveBeenLastCalledWith(
      expect.objectContaining({ x: 412, y: 284, w: 200, h: 200 }),
    );

    disposeTools();
    dispose();
  });

  it("click without drag creates the default frame and centers the viewport", () => {
    const { signals, dispose } = makeModernCropSignals();
    const setViewportState = spyOnSetter(signals, "setViewportState");
    const { tools, dispose: disposeTools } = mountPointerTools(signals);

    tools.onCanvasPointerDown(makePointerEvent({ clientX: 300, clientY: 200 }));
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 300, clientY: 200 }));

    // Default frame from modernCropGeometry mock.
    expect(signals.setModernCropFrame).toHaveBeenCalledWith({ x: 0, y: 0, w: 400, h: 300 });
    // Viewport recentered: pan = (1024-100)/2, (768-100)/2.
    expect(setViewportState).toHaveBeenCalledWith({ x: 462, y: 334, zoom: 1 });
    expect(signals.setModernCropImageTransform).toHaveBeenLastCalledWith(
      expect.objectContaining({ offsetX: 0, offsetY: 0 }),
    );

    disposeTools();
    dispose();
  });
});

describe("classic crop pending-click fallback", () => {
  it("click ≤2px recenters viewport and restores the hidden preview", () => {
    // Default harness mode is "classic"; cropRect starts null → pending-click armed.
    const { signals, dispose } = createMockEditorParams("crop");
    const setViewportState = spyOnSetter(signals, "setViewportState");
    const { tools, dispose: disposeTools } = mountPointerTools(signals);

    tools.onCanvasPointerDown(makePointerEvent({ clientX: 0, clientY: 0 }));
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 1, clientY: 1 }));

    expect(setViewportState).toHaveBeenCalledWith({ x: 462, y: 334, zoom: 1 });
    expect(CropToolActions.restoreHiddenCropPreview).toHaveBeenCalled();

    disposeTools();
    dispose();
  });

  it("drag >2px does NOT trigger the pending-click fallback", () => {
    const { signals, dispose } = createMockEditorParams("crop");
    const setViewportState = spyOnSetter(signals, "setViewportState");
    const { tools, dispose: disposeTools } = mountPointerTools(signals);

    tools.onCanvasPointerDown(makePointerEvent({ clientX: 0, clientY: 0 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 10, clientY: 5 }));
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 10, clientY: 5 }));

    expect(setViewportState).not.toHaveBeenCalled();

    disposeTools();
    dispose();
  });
});
