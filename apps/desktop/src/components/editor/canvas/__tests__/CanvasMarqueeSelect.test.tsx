// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, it, expect, vi, beforeEach } from "vitest";
import { useCanvasMarqueeSelect } from "../useCanvasMarqueeSelect";
import { createRoot, createSignal } from "solid-js";
import type { LayerNode, DocumentModel } from "@/engine/types";

let mockSelectedLayerIds = ["layer-1"];
let mockSelectedLayerId: string | null = "layer-1";
const mockSetSelectedLayerIds = vi.fn((ids: string[]) => {
  mockSelectedLayerIds = ids;
});
const mockRawSetSelectedLayerId = vi.fn((id: string | null) => {
  mockSelectedLayerId = id;
});
const mockSetSelectedLayerId = vi.fn((id: string | null) => {
  mockSelectedLayerId = id;
  mockSelectedLayerIds = id ? [id] : [];
});

const mockLayers: LayerNode[] = [
  {
    id: "bg-layer",
    name: "Background",
    type: "raster",
    imageBitmap: null,
    visible: true,
    locked: true,
    isBackground: true,
    opacity: 1,
    blendMode: "normal",
    width: 800,
    height: 600,
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
  },
  {
    id: "layer-1",
    name: "Layer 1",
    type: "raster",
    imageBitmap: null,
    visible: true,
    locked: false,
    isBackground: false,
    opacity: 1,
    blendMode: "normal",
    width: 100,
    height: 100,
    transform: { x: 50, y: 50, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
  },
  {
    id: "layer-2",
    name: "Layer 2",
    type: "raster",
    imageBitmap: null,
    visible: true,
    locked: false,
    isBackground: false,
    opacity: 1,
    blendMode: "normal",
    width: 150,
    height: 150,
    transform: { x: 200, y: 200, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
  },
  {
    id: "layer-hidden",
    name: "Layer Hidden",
    type: "raster",
    imageBitmap: null,
    visible: false,
    locked: false,
    isBackground: false,
    opacity: 1,
    blendMode: "normal",
    width: 100,
    height: 100,
    transform: { x: 50, y: 50, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
  },
];

const mockEngine = {
  getLayers: () => mockLayers,
  getActiveLayerId: () => mockSelectedLayerId,
};

const mockCamera = {
  screenToDocument: (x: number, y: number) => ({ x, y }),
};

const [activeTool, setActiveTool] = createSignal<string>("move");

vi.mock("../../shell/EditorContext", () => ({
  useEditor: () => ({
    workspace: {
      getActiveEngine: () => mockEngine,
    },
    camera: mockCamera,
    pan: () => ({ x: 0, y: 0 }),
    zoom: () => 1,
    activeTool,
    selectedLayerIds: () => mockSelectedLayerIds,
    setSelectedLayerIds: mockSetSelectedLayerIds,
    setSelectedLayerId: mockSetSelectedLayerId,
    selectedLayerId: () => mockSelectedLayerId,
    scheduler: {
      requestRender: vi.fn(),
    },
  }),
}));

describe("useCanvasMarqueeSelect", () => {
  beforeEach(() => {
    mockSelectedLayerIds = ["layer-1"];
    mockSelectedLayerId = "layer-1";
    setActiveTool("move");
    vi.clearAllMocks();
  });

  it("initializes with inactive marquee state", () => {
    createRoot((dispose) => {
      const { marqueeRect, isMarqueeActive } = useCanvasMarqueeSelect();
      expect(isMarqueeActive()).toBe(false);
      expect(marqueeRect()).toBeNull();
      dispose();
    });
  });

  it("does not start marquee if activeTool is not move", () => {
    setActiveTool("brush");
    createRoot((dispose) => {
      const { handlePointerDown, isMarqueeActive } = useCanvasMarqueeSelect();
      const mockContainer = {
        getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 }),
      } as HTMLElement;

      const e = {
        button: 0,
        clientX: 10,
        clientY: 10,
        shiftKey: false,
        ctrlKey: false,
        metaKey: false,
        target: mockContainer,
        currentTarget: mockContainer,
      } as unknown as PointerEvent;

      const started = handlePointerDown(e, mockContainer);
      expect(started).toBe(false);
      expect(isMarqueeActive()).toBe(false);
      dispose();
    });
  });

  it("starts marquee on empty space and selects intersecting layers on drag", () => {
    createRoot((dispose) => {
      const { handlePointerDown, marqueeRect, isMarqueeActive } = useCanvasMarqueeSelect();
      const mockContainer = {
        getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 }),
      } as HTMLElement;

      const e = {
        button: 0,
        clientX: 0,
        clientY: 0,
        shiftKey: false,
        ctrlKey: false,
        metaKey: false,
        target: mockContainer,
        currentTarget: mockContainer,
      } as unknown as PointerEvent;

      const started = handlePointerDown(e, mockContainer);
      expect(started).toBe(true);

      // Simulate dragging across layer-1 (at 50,50 to 150,150)
      const moveEvent = new PointerEvent("pointermove", {
        clientX: 120,
        clientY: 120,
      });
      window.dispatchEvent(moveEvent);

      expect(isMarqueeActive()).toBe(true);
      expect(marqueeRect()).toEqual({
        x: 0,
        y: 0,
        width: 120,
        height: 120,
      });

      // Assert layer-1 was selected and background/hidden were skipped
      expect(mockSetSelectedLayerIds).toHaveBeenCalledWith(["layer-1"]);

      // Pointer up ends the marquee
      const upEvent = new PointerEvent("pointerup", {
        clientX: 120,
        clientY: 120,
      });
      window.dispatchEvent(upEvent);

      expect(isMarqueeActive()).toBe(false);
      expect(marqueeRect()).toBeNull();

      dispose();
    });
  });

  it("supports additive multi-selection when Shift is held", () => {
    createRoot((dispose) => {
      mockSelectedLayerIds = ["layer-1"];
      const { handlePointerDown, isMarqueeActive } = useCanvasMarqueeSelect();
      const mockContainer = {
        getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 }),
      } as HTMLElement;

      const e = {
        button: 0,
        clientX: 180,
        clientY: 180,
        shiftKey: true,
        ctrlKey: false,
        metaKey: false,
        target: mockContainer,
        currentTarget: mockContainer,
      } as unknown as PointerEvent;

      handlePointerDown(e, mockContainer);

      // Drag across layer-2 (at 200,200 to 350,350)
      const moveEvent = new PointerEvent("pointermove", {
        clientX: 300,
        clientY: 300,
      });
      window.dispatchEvent(moveEvent);

      expect(isMarqueeActive()).toBe(true);
      // Both layer-1 (initial) and layer-2 (newly intersected) should be selected
      expect(mockSetSelectedLayerIds).toHaveBeenCalledWith(
        expect.arrayContaining(["layer-1", "layer-2"]),
      );

      const upEvent = new PointerEvent("pointerup", {
        clientX: 300,
        clientY: 300,
      });
      window.dispatchEvent(upEvent);
      dispose();
    });
  });

  it("deselects layers on single click without drag or modifiers", () => {
    createRoot((dispose) => {
      const { handlePointerDown } = useCanvasMarqueeSelect();
      const mockContainer = {
        getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 }),
      } as HTMLElement;

      const e = {
        button: 0,
        clientX: 10,
        clientY: 10,
        shiftKey: false,
        ctrlKey: false,
        metaKey: false,
        target: mockContainer,
        currentTarget: mockContainer,
      } as unknown as PointerEvent;

      handlePointerDown(e, mockContainer);

      // Pointer up without significant move (dist <= 3px)
      const upEvent = new PointerEvent("pointerup", {
        clientX: 11,
        clientY: 11,
      });
      window.dispatchEvent(upEvent);

      expect(mockSetSelectedLayerId).toHaveBeenCalledWith(null);
      expect(mockSetSelectedLayerIds).toHaveBeenCalledWith([]);
      dispose();
    });
  });
});
