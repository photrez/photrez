// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, it, expect, vi, beforeEach } from "vitest";
import { useMultiSelectionGroupTransform } from "../useMultiSelectionGroupTransform";
import { createRoot } from "solid-js";
import type { LayerNode, DocumentModel, Transform2D } from "@/engine/types";

let mockSelectedLayerIds = ["layer-1", "layer-2"];
let mockLayers: LayerNode[] = [];
let mockHistoryCommit = vi.fn();
let mockTransformLayer = vi.fn();
let mockTransformSilent = vi.fn();
let mockFlush = vi.fn();

const engineLayersMap = new Map<string, LayerNode>();

const mockEngine = {
  getLayer: vi.fn((id: string) => engineLayersMap.get(id) || null),
  transformLayer: vi.fn((id: string, t: Transform2D) => {
    mockTransformLayer(id, t);
    const existing = engineLayersMap.get(id);
    if (existing) {
      existing.transform = { ...t };
    }
  }),
  // Silent twin: same model write, no notification. Mirrors the production
  // engine method so legacy-drag tests run green.
  transformLayerSilent: vi.fn((id: string, t: Transform2D) => {
    mockTransformSilent(id, t);
    const existing = engineLayersMap.get(id);
    if (existing) {
      existing.transform = { ...t };
    }
  }),
  flushChangeNotification: mockFlush,
  snapshot: vi.fn(() => ({ id: "snap-1" } as unknown as DocumentModel)),
  restoreSnapshot: vi.fn(),
};

const mockHistory = {
  commit: mockHistoryCommit,
};

const mockWorkspace = {
  getActiveEngine: () => mockEngine,
  getActiveHistory: () => mockHistory,
  notifyVisualChange: vi.fn(),
};

vi.mock("../../shell/EditorContext", () => ({
  useEditor: () => ({
    workspace: mockWorkspace,
    layers: () => mockLayers,
    selectedLayerIds: () => mockSelectedLayerIds,
    zoom: () => 1,
    pan: () => ({ x: 0, y: 0 }),
    scheduler: { requestRender: vi.fn() },
    activeTool: () => "move",
  }),
}));

describe("useMultiSelectionGroupTransform", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockLayers = [
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
        width: 100,
        height: 100,
        transform: { x: 250, y: 50, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      },
    ];

    engineLayersMap.clear();
    for (const l of mockLayers) {
      engineLayersMap.set(l.id, { ...l, transform: { ...l.transform } });
    }
    mockSelectedLayerIds = ["layer-1", "layer-2"];
  });

  it("calculates groupAabb covering all selected layers", () => {
    createRoot((dispose) => {
      const transform = useMultiSelectionGroupTransform();
      const aabb = transform.groupAabb();
      expect(aabb).not.toBeNull();
      expect(aabb?.x).toBe(50);
      expect(aabb?.y).toBe(50);
      expect(aabb?.width).toBe(300); // from x=50 to x=350 (250+100)
      expect(aabb?.height).toBe(100);
      dispose();
    });
  });

  it("returns null groupAabb if fewer than 2 layers selected", () => {
    mockSelectedLayerIds = ["layer-1"];
    createRoot((dispose) => {
      const transform = useMultiSelectionGroupTransform();
      expect(transform.groupAabb()).toBeNull();
      dispose();
    });
  });

  it("scales multiple layers proportionally when dragging SE handle", () => {
    createRoot((dispose) => {
      const transform = useMultiSelectionGroupTransform();
      const mockEl = { setPointerCapture: vi.fn() } as unknown as HTMLElement;

      // Pointer down on SE corner handle at (350, 150)
      transform.handlePointerDown(
        {
          button: 0,
          clientX: 350,
          clientY: 150,
          pointerId: 1,
          preventDefault: vi.fn(),
          stopPropagation: vi.fn(),
          currentTarget: mockEl,
        } as unknown as PointerEvent,
        "se",
      );

      expect(transform.isTransforming()).toBe(true);
      expect(transform.activeHandle()).toBe("se");

      // Pointer move to drag SE handle to 2x width (300px + 300px = 600px width)
      // dx = 300, dy = 100
      transform.handlePointerMove({
        pointerId: 1,
        clientX: 650,
        clientY: 250,
        shiftKey: true, // unconstrained / direct
        altKey: false,
        preventDefault: vi.fn(),
        stopPropagation: vi.fn(),
      } as unknown as PointerEvent);

      expect(mockTransformLayer).not.toHaveBeenCalled();
      expect(mockTransformSilent).toHaveBeenCalled();

      // Pointer up commits history
      transform.handlePointerUp({
        pointerId: 1,
        preventDefault: vi.fn(),
        stopPropagation: vi.fn(),
      } as unknown as PointerEvent);

      expect(transform.isTransforming()).toBe(false);
      expect(mockHistoryCommit).toHaveBeenCalledWith(expect.anything(), "Transform Layers");
      // Single end-of-gesture flush for the silent per-frame writes.
      expect(mockFlush).toHaveBeenCalledTimes(1);

      dispose();
    });
  });

  it("legacy frames write silent per move and flush once at pointerup", () => {
    createRoot((dispose) => {
      const transform = useMultiSelectionGroupTransform();
      const mockEl = { setPointerCapture: vi.fn() } as unknown as HTMLElement;
      transform.handlePointerDown(
        {
          button: 0,
          clientX: 350,
          clientY: 150,
          pointerId: 1,
          preventDefault: vi.fn(),
          stopPropagation: vi.fn(),
          currentTarget: mockEl,
        } as unknown as PointerEvent,
        "se",
      );
      expect(transform.isTransforming()).toBe(true);

      const N = 4;
      for (let i = 1; i <= N; i++) {
        transform.handlePointerMove({
          pointerId: 1,
          clientX: 350 + 10 * i,
          clientY: 150 + 5 * i,
          shiftKey: true,
          altKey: false,
          preventDefault: vi.fn(),
          stopPropagation: vi.fn(),
        } as unknown as PointerEvent);
      }
      // Zero noisy writes mid-gesture; one silent write per member per move.
      expect(mockTransformLayer).not.toHaveBeenCalled();
      expect(mockTransformSilent).toHaveBeenCalledTimes(N * 2);

      transform.handlePointerUp({
        pointerId: 1,
        preventDefault: vi.fn(),
        stopPropagation: vi.fn(),
      } as unknown as PointerEvent);
      expect(mockHistoryCommit).toHaveBeenCalledWith(expect.anything(), "Transform Layers");
      expect(mockFlush).toHaveBeenCalledTimes(1);
      dispose();
    });
  });

  it("legacy cancel writes back silent with a single flush", () => {
    createRoot((dispose) => {
      const transform = useMultiSelectionGroupTransform();
      const mockEl = { setPointerCapture: vi.fn() } as unknown as HTMLElement;
      transform.handlePointerDown(
        {
          button: 0,
          clientX: 350,
          clientY: 150,
          pointerId: 1,
          preventDefault: vi.fn(),
          stopPropagation: vi.fn(),
          currentTarget: mockEl,
        } as unknown as PointerEvent,
        "se",
      );
      transform.handlePointerMove({
        pointerId: 1,
        clientX: 400,
        clientY: 200,
        shiftKey: true,
        altKey: false,
        preventDefault: vi.fn(),
        stopPropagation: vi.fn(),
      } as unknown as PointerEvent);
      mockTransformLayer.mockClear();
      mockTransformSilent.mockClear();
      mockFlush.mockClear();

      transform.handlePointerCancel({
        pointerId: 1,
        preventDefault: vi.fn(),
        stopPropagation: vi.fn(),
      } as unknown as PointerEvent);
      expect(mockTransformLayer).not.toHaveBeenCalled();
      expect(mockTransformSilent).toHaveBeenCalledTimes(2);
      expect(mockFlush).toHaveBeenCalledTimes(1);
      // Both members are back at their gesture-start transforms.
      expect(engineLayersMap.get("layer-1")!.transform.x).toBe(50);
      expect(engineLayersMap.get("layer-1")!.transform.y).toBe(50);
      expect(engineLayersMap.get("layer-2")!.transform.x).toBe(250);
      expect(engineLayersMap.get("layer-2")!.transform.y).toBe(50);
      dispose();
    });
  });
});
