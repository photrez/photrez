import { describe, it, expect, beforeEach, vi } from "vitest";
import { addLayerFromCrossDoc, computeCascadePosition, CASCADE_OFFSET_PX } from "../crossDocLayerOps";
import type { BlendMode, LayerNode } from "@/engine/types";
import type { LayerDragPayload } from "../dragTypes";
import { resetToasts } from "../Toast";

const { mockShowToast } = vi.hoisted(() => ({
  mockShowToast: vi.fn(),
}));

vi.mock("../Toast", () => ({
  showToast: (msg: string, severity: "info" | "warn" | "error" = "info") =>
    mockShowToast(msg, severity),
  resetToasts: vi.fn(),
  ToastHost: () => null,
}));

const basePayload: LayerDragPayload = {
  version: 1,
  sourceDocId: "doc-A",
  layerId: "layer-1",
  sourceName: "Background",
  isAltPressed: false,
};

function makeEngine(opts: { id: string; layerCount?: number; width?: number; height?: number } = { id: "doc-A" }) {
  const id = opts.id;
  const width = opts.width ?? 800;
  const height = opts.height ?? 600;
  const layerCount = opts.layerCount ?? 3;
  const layerArr: LayerNode[] = Array.from({ length: layerCount }, (_, i) => ({
    id: `layer-${i}`,
    name: `Layer${i}`,
    type: "raster" as const,
    visible: true,
    locked: false,
    blendMode: "normal" as BlendMode,
    width: 200,
    height: 150,
    transform: { x: 0, y: 0, rotation: 0, scaleX: 1, scaleY: 1, flipH: false, flipV: false },
    opacity: 1,
    imageBitmap: null,
  }));
  let newLayerCounter = 0;
  return {
    id,
    width,
    height,
    getWidth: vi.fn(() => width),
    getHeight: vi.fn(() => height),
    getLayer: vi.fn((lid: string) =>
      layerArr.find((l) => l.id === lid)
    ),
    getLayers: vi.fn(() => layerArr),
    getLayerCount: vi.fn(() => layerArr.length),
    addLayer: vi.fn((name: string, layerWidth = 200, layerHeight = 150) => {
      const newLayer = {
        id: `new-${++newLayerCounter}`,
        name: typeof name === "string" ? name : "Imported",
        type: "raster" as const,
        visible: true,
        locked: false,
        blendMode: "normal" as BlendMode,
        width: layerWidth,
        height: layerHeight,
        transform: { x: 0, y: 0, rotation: 0, scaleX: 1, scaleY: 1, flipH: false, flipV: false },
        opacity: 1,
        imageBitmap: null,
      };
      layerArr.push(newLayer);
      return newLayer;
    }),
    addShapeLayer: vi.fn((name: string, params: any) => {
      const newLayer: any = {
        id: `new-${++newLayerCounter}`,
        name: typeof name === "string" ? name : "Imported",
        type: "shape",
        visible: true,
        locked: false,
        blendMode: "normal" as BlendMode,
        width: 200,
        height: 150,
        transform: { x: 0, y: 0, rotation: 0, scaleX: 1, scaleY: 1, flipH: false, flipV: false },
        opacity: 1,
        imageBitmap: null,
        shapeParams: params,
      };
      layerArr.push(newLayer);
      return newLayer;
    }),
    addTextLayer: vi.fn((name: string, textData: any) => {
      const newLayer: any = {
        id: `new-${++newLayerCounter}`,
        name: typeof name === "string" ? name : "Imported",
        type: "text",
        visible: true,
        locked: false,
        blendMode: "normal" as BlendMode,
        width: 200,
        height: 150,
        transform: { x: 0, y: 0, rotation: 0, scaleX: 1, scaleY: 1, flipH: false, flipV: false },
        opacity: 1,
        imageBitmap: null,
        textData,
      };
      layerArr.push(newLayer);
      return newLayer;
    }),
    moveLayer: vi.fn((lid: string, x: number, y: number) => {
      const l = layerArr.find((layer) => layer.id === lid);
      if (l) l.transform = { ...l.transform, x, y };
    }),
    transformLayer: vi.fn((lid: string, transform) => {
      const l = layerArr.find((layer) => layer.id === lid);
      if (l) l.transform = { ...l.transform, ...transform };
    }),
    setLayerOpacity: vi.fn((lid: string, opacity: number) => {
      const l = layerArr.find((layer) => layer.id === lid);
      if (l) l.opacity = opacity;
    }),
    setLayerBlendMode: vi.fn((lid: string, blendMode: BlendMode) => {
      const l = layerArr.find((layer) => layer.id === lid);
      if (l) l.blendMode = blendMode;
    }),
    setLayerVisibility: vi.fn((lid: string, visible: boolean) => {
      const l = layerArr.find((layer) => layer.id === lid);
      if (l) l.visible = visible;
    }),
    setLayerLocked: vi.fn((lid: string, locked: boolean) => {
      const l = layerArr.find((layer) => layer.id === lid);
      if (l) l.locked = locked;
    }),
    setLayerImageBitmap: vi.fn((lid: string, imageBitmap: ImageBitmap) => {
      const l = layerArr.find((layer) => layer.id === lid);
      if (l) l.imageBitmap = imageBitmap;
    }),
    reorderLayer: vi.fn((fromIndex: number, toIndex: number) => {
      const [moved] = layerArr.splice(fromIndex, 1);
      layerArr.splice(toIndex, 0, moved);
    }),
    deleteLayer: vi.fn((lid: string) => {
      const idx = layerArr.findIndex((l) => l.id === lid);
      if (idx >= 0) layerArr.splice(idx, 1);
    }),
    snapshot: vi.fn(() => ({
      id,
      name: "test doc",
      width,
      height,
      layers: [] as LayerNode[],
      activeLayerId: null,
      selection: null,
      /* viewport mock has minimal fields — add real defaults if snapshot is ever diffed */
      viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
      dirty: false,
    })),
  };
}

function makeWorkspace(
  engines: Record<string, ReturnType<typeof makeEngine> | undefined>,
) {
  return {
    getEngine: vi.fn((id: string) => engines[id] ?? null),
    getHistory: vi.fn(() => ({ commit: vi.fn() })),
    getActiveDocumentId: vi.fn(() => "doc-B"),
    isFull: vi.fn(() => false),
    addDocument: vi.fn(),
  };
}

describe("addLayerFromCrossDoc â€” copy (default)", () => {
  beforeEach(() => {
    mockShowToast.mockClear();
    resetToasts();
  });

  it("adds a cloned layer to target doc", () => {
    const sourceEngine = makeEngine({ id: "doc-A" });
    const targetEngine = makeEngine({ id: "doc-B" });
    const ws = makeWorkspace({ "doc-A": sourceEngine, "doc-B": targetEngine });

    addLayerFromCrossDoc(basePayload, { type: "canvas" }, { x: 200, y: 200 }, ws);

    expect(targetEngine.addLayer).toHaveBeenCalledOnce();
    expect(sourceEngine.deleteLayer).not.toHaveBeenCalled();
  });
});

describe("addLayerFromCrossDoc â€” move (Alt+drag)", () => {
  beforeEach(() => {
    mockShowToast.mockClear();
    resetToasts();
  });

  it("adds to target AND deletes from source", () => {
    const sourceEngine = makeEngine({ id: "doc-A" });
    const targetEngine = makeEngine({ id: "doc-B" });
    const ws = makeWorkspace({ "doc-A": sourceEngine, "doc-B": targetEngine });

    addLayerFromCrossDoc({ ...basePayload, isAltPressed: true }, { type: "canvas" }, { x: 100, y: 100 }, ws);

    expect(targetEngine.addLayer).toHaveBeenCalledOnce();
    expect(sourceEngine.deleteLayer).toHaveBeenCalledWith("layer-1");
  });

  it("aborts Alt-move when source only has one layer", () => {
    const sourceEngine = makeEngine({ id: "doc-A", layerCount: 1 });
    const targetEngine = makeEngine({ id: "doc-B" });
    const ws = makeWorkspace({ "doc-A": sourceEngine, "doc-B": targetEngine });

    const result = addLayerFromCrossDoc(
      { ...basePayload, layerId: "layer-0", isAltPressed: true },
      { type: "canvas" },
      { x: 100, y: 100 },
      ws
    );

    expect(result.newLayerId).toBeNull();
    expect(targetEngine.addLayer).not.toHaveBeenCalled();
    expect(sourceEngine.deleteLayer).not.toHaveBeenCalled();
    expect(mockShowToast).toHaveBeenCalledWith(expect.stringContaining("last layer"), "error");
  });
});

describe("addLayerFromCrossDoc â€” validation", () => {
  beforeEach(() => {
    mockShowToast.mockClear();
    resetToasts();
  });

  it("reorders layer to end of stack on same-doc drop without insertAt (Pass 12 fallback, Pass 14 polish)", () => {
    const engine = makeEngine({ id: "doc-A" });
    /* layers just need .id for reorder — as LayerNode[] silences structural check */
    const layers = [
      { id: "layer-1" } as LayerNode,
      { id: "layer-2" } as LayerNode,
      { id: "layer-3" } as LayerNode,
    ];
    engine.getLayers.mockReturnValue(layers);
    engine.getLayer.mockImplementation((id: string) => layers.find((l) => l.id === id));
    const ws = makeWorkspace({ "doc-A": engine });
    ws.getActiveDocumentId.mockReturnValue("doc-A");
    addLayerFromCrossDoc(
      { ...basePayload, sourceDocId: "doc-A", layerId: "layer-1" },
      { type: "layers-panel" },
      { x: 0, y: 0 },
      ws,
    );
    // Should NOT clone (same-doc means reorder, not copy).
    expect(engine.addLayer).not.toHaveBeenCalled();
    // Should reorder the source layer to end of its own stack.
    expect(engine.reorderLayer).toHaveBeenCalledWith(0, 2);
  });

  it("reorders layer to insertAt on same-doc drop with insertPosition='above' (Pass 14)", () => {
    const engine = makeEngine({ id: "doc-A" });
    const layers = [
      { id: "layer-1" } as LayerNode,
      { id: "layer-2" } as LayerNode,
      { id: "layer-3" } as LayerNode,
    ];
    engine.getLayers.mockReturnValue(layers);
    engine.getLayer.mockImplementation((id: string) => layers.find((l) => l.id === id));
    const ws = makeWorkspace({ "doc-A": engine });
    ws.getActiveDocumentId.mockReturnValue("doc-A");
    addLayerFromCrossDoc(
      { ...basePayload, sourceDocId: "doc-A", layerId: "layer-1" },
      { type: "layers-panel", insertAt: 2, insertPosition: "above" },
      { x: 0, y: 0 },
      ws,
    );
    // Source is at index 0, target insertAt=2 above. After moving
    // source out, indexes 1 and 2 shift down to 0 and 1. The target
    // row (was index 2) is now at index 1; we want to land above it.
    expect(engine.reorderLayer).toHaveBeenCalledWith(0, 1);
  });

  it("reorders layer to insertAt+1 on same-doc drop with insertPosition='below' (Pass 14)", () => {
    const engine = makeEngine({ id: "doc-A" });
    const layers = [
      { id: "layer-1" } as LayerNode,
      { id: "layer-2" } as LayerNode,
      { id: "layer-3" } as LayerNode,
    ];
    engine.getLayers.mockReturnValue(layers);
    engine.getLayer.mockImplementation((id: string) => layers.find((l) => l.id === id));
    const ws = makeWorkspace({ "doc-A": engine });
    ws.getActiveDocumentId.mockReturnValue("doc-A");
    addLayerFromCrossDoc(
      { ...basePayload, sourceDocId: "doc-A", layerId: "layer-1" },
      { type: "layers-panel", insertAt: 0, insertPosition: "below" },
      { x: 0, y: 0 },
      ws,
    );
    // Source at index 0 → moves to index 1 (below row 0).
    expect(engine.reorderLayer).toHaveBeenCalledWith(0, 1);
  });

  it("error toast + abort when source doc missing", () => {
    const ws = makeWorkspace({});
    addLayerFromCrossDoc(basePayload, { type: "canvas" }, { x: 0, y: 0 }, ws);
    expect(mockShowToast).toHaveBeenCalledWith(expect.stringContaining("closed"), "error");
  });

  it("error toast + abort when source layer missing", () => {
    const sourceEngine = makeEngine({ id: "doc-A" });
    sourceEngine.getLayer.mockReturnValue(undefined);
    const targetEngine = makeEngine({ id: "doc-B" });
    const ws = makeWorkspace({ "doc-A": sourceEngine, "doc-B": targetEngine });
    addLayerFromCrossDoc(basePayload, { type: "canvas" }, { x: 0, y: 0 }, ws);
    expect(mockShowToast).toHaveBeenCalledWith(expect.stringContaining("deleted"), "error");
    expect(targetEngine.addLayer).not.toHaveBeenCalled();
  });

  it("error toast + abort when target has MAX_LAYERS", () => {
    const sourceEngine = makeEngine({ id: "doc-A" });
    const targetEngine = makeEngine({ id: "doc-B", layerCount: 200 });
    const ws = makeWorkspace({ "doc-A": sourceEngine, "doc-B": targetEngine });
    addLayerFromCrossDoc(basePayload, { type: "canvas" }, { x: 0, y: 0 }, ws);
    expect(mockShowToast).toHaveBeenCalledWith(expect.stringContaining("200"), "error");
  });
});

describe("addLayerFromCrossDoc â€” position", () => {
  beforeEach(() => {
    mockShowToast.mockClear();
    resetToasts();
  });

  it("centers the dropped layer on the cursor for a canvas drop", () => {
    const sourceEngine = makeEngine({ id: "doc-A" });
    const targetEngine = makeEngine({ id: "doc-B" });
    const ws = makeWorkspace({ "doc-A": sourceEngine, "doc-B": targetEngine });
    // makeEngine default layer is 200x150; canvas drop centers on cursor.
    addLayerFromCrossDoc(basePayload, { type: "canvas" }, { x: 333, y: 444 }, ws);
    const transform = targetEngine.transformLayer.mock.calls[0][1];
    expect(transform.x).toBe(333 - 100); // 200 / 2
    expect(transform.y).toBe(444 - 75); // 150 / 2
  });

  it("uses doc center when target is tab (plan: tab/layers-panel → center)", () => {
    const sourceEngine = makeEngine({ id: "doc-A" });
    const targetEngine = makeEngine({ id: "doc-B", width: 800, height: 600 });
    const ws = makeWorkspace({ "doc-A": sourceEngine, "doc-B": targetEngine });
    addLayerFromCrossDoc(basePayload, { type: "tab", docId: "doc-B" }, { x: 123, y: 456 }, ws);
    const transform = targetEngine.transformLayer.mock.calls[0][1];
    // (targetW - layerW)/2, (targetH - layerH)/2 = (300, 225)
    expect(transform.x).toBe(300);
    expect(transform.y).toBe(225);
  });

  it("uses doc center when target is layers-panel", () => {
    const sourceEngine = makeEngine({ id: "doc-A" });
    const targetEngine = makeEngine({ id: "doc-B", width: 1000, height: 800 });
    const ws = makeWorkspace({ "doc-A": sourceEngine, "doc-B": targetEngine });
    addLayerFromCrossDoc(basePayload, { type: "layers-panel" }, { x: 0, y: 0 }, ws);
    const transform = targetEngine.transformLayer.mock.calls[0][1];
    expect(transform.x).toBe(400);
    expect(transform.y).toBe(325);
  });
});

describe("addLayerFromCrossDoc â€” return value", () => {
  beforeEach(() => {
    mockShowToast.mockClear();
    resetToasts();
  });

  it("returns the new layer id on success (hook needs it to upload bitmap)", () => {
    const sourceEngine = makeEngine({ id: "doc-A" });
    const targetEngine = makeEngine({ id: "doc-B" });
    const ws = makeWorkspace({ "doc-A": sourceEngine, "doc-B": targetEngine });

    const result = addLayerFromCrossDoc(
      basePayload,
      { type: "tab", docId: "doc-B" },
      { x: 100, y: 100 },
      ws
    );

    expect(result.newLayerId).toBeTruthy();
    expect(result.newLayerId).toBe("new-1");
  });

  it("returns null newLayerId when source doc missing", () => {
    const ws = makeWorkspace({});
    const result = addLayerFromCrossDoc(
      basePayload,
      { type: "canvas" },
      { x: 0, y: 0 },
      ws
    );
    expect(result.newLayerId).toBeNull();
  });

  it("returns the source layer id on same-doc drop (caller uses it to signal reorder completed)", () => {
    const engine = makeEngine({ id: "doc-A" });
    const layers = [{ id: "layer-source" } as LayerNode];
    engine.getLayers.mockReturnValue(layers);
    engine.getLayer.mockReturnValue(layers[0]!);
    const ws = makeWorkspace({ "doc-A": engine });
    ws.getActiveDocumentId.mockReturnValue("doc-A");
    const result = addLayerFromCrossDoc(
      { ...basePayload, sourceDocId: "doc-A", layerId: "layer-source" },
      { type: "layers-panel" },
      { x: 0, y: 0 },
      ws
    );
    expect(result.newLayerId).toBe("layer-source");
  });
});
