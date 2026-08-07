import { describe, it, expect, vi, afterEach } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { render } from "solid-js/web";
import { OptionBar } from "../shell/OptionBar";
import { ShapeOptionBar } from "../ShapeOptionBar";
import type { LayerNode, ShapeParams } from "@/engine/types";

const baseParams: ShapeParams = {
  kind: "rect",
  width: 100,
  height: 50,
  radius: 8,
  fill: { kind: "solid", color: "#ff0000" },
  stroke: { enabled: true, color: "#00ff00", width: 6 },
  arrowHead: false,
};

function makeShapeLayer(overrides: Partial<LayerNode> = {}): LayerNode {
  return {
    id: "shape-1",
    name: "Shape",
    type: "shape",
    width: 100,
    height: 50,
    opacity: 1,
    visible: true,
    locked: false,
    blendMode: "normal",
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    imageBitmap: null,
    shapeParams: { ...baseParams },
    ...overrides,
  };
}

type Setters = {
  setShapeKind: ReturnType<typeof vi.fn>;
  setShapeFillEnabled: ReturnType<typeof vi.fn>;
  setShapeStrokeEnabled: ReturnType<typeof vi.fn>;
  setShapeStrokeColor: ReturnType<typeof vi.fn>;
  setShapeStrokeWidth: ReturnType<typeof vi.fn>;
  setShapeRadius: ReturnType<typeof vi.fn>;
  setShapeArrowHead: ReturnType<typeof vi.fn>;
};

function buildMock(overrides: Record<string, unknown> = {}, layer?: LayerNode) {
  const setters: Setters = {
    setShapeKind: vi.fn(),
    setShapeFillEnabled: vi.fn(),
    setShapeStrokeEnabled: vi.fn(),
    setShapeStrokeColor: vi.fn(),
    setShapeStrokeWidth: vi.fn(),
    setShapeRadius: vi.fn(),
    setShapeArrowHead: vi.fn(),
  };
  const updateShapeParams = vi.fn();
  const commit = vi.fn();
  const snapshot = vi.fn(() => ({}));
  const engine = { getLayer: () => layer ?? undefined, updateShapeParams, snapshot };
  const editor = {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => ({ commit }),
    },
    activeTool: () => "shape",
    selectedLayerId: () => (layer ? layer.id : null),
    fgColor: () => "#E15A17",
    shapeKind: () => (layer ? layer.shapeParams!.kind : "rect"),
    setShapeKind: setters.setShapeKind,
    shapeFillEnabled: () => (layer ? layer.shapeParams!.fill.kind === "solid" : true),
    setShapeFillEnabled: setters.setShapeFillEnabled,
    shapeStrokeEnabled: () => (layer ? layer.shapeParams!.stroke.enabled : false),
    setShapeStrokeEnabled: setters.setShapeStrokeEnabled,
    shapeStrokeColor: () => (layer ? layer.shapeParams!.stroke.color : "#000000"),
    setShapeStrokeColor: setters.setShapeStrokeColor,
    shapeStrokeWidth: () => (layer ? layer.shapeParams!.stroke.width : 4),
    setShapeStrokeWidth: setters.setShapeStrokeWidth,
    shapeRadius: () => (layer ? layer.shapeParams!.radius : 0),
    setShapeRadius: setters.setShapeRadius,
    shapeArrowHead: () => (layer ? layer.shapeParams!.arrowHead : false),
    setShapeArrowHead: setters.setShapeArrowHead,
    ...overrides,
  };
  mockUseEditor(editor as any);
  return { setters, engine, commit, snapshot, editor };
}

function mountShapeBar() {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(() => <ShapeOptionBar />, container);
  const cleanup = () => {
    dispose();
    container.parentNode?.removeChild(container);
  };
  return { container, cleanup };
}

function mountOptionBar() {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(() => <OptionBar />, container);
  const cleanup = () => {
    dispose();
    container.parentNode?.removeChild(container);
  };
  return { container, cleanup };
}

function qs<T extends HTMLElement>(root: HTMLElement, sel: string): T | null {
  return root.querySelector(sel) as T | null;
}

/** Minimal editor enough for OptionBar's move branch + ShapeOptionBar. */
function optionBarEditor(overrides: Record<string, unknown> = {}, layer?: LayerNode) {
  const layers = layer ? [layer] : ([] as LayerNode[]);
  return {
    workspace: {
      getActiveEngine: () => (layer ? { getLayer: () => layer } : { getLayer: () => undefined }),
      getActiveHistory: () => ({ commit: vi.fn() }),
    },
    activeTool: () => "move",
    layerTransformSession: () => null,
    layers: () => layers,
    activeLayerId: () => (layer ? layer.id : null),
    selectedLayerId: () => (layer ? layer.id : null),
    scheduler: { requestRender: vi.fn() },
    moveAutoSelect: () => true,
    setMoveAutoSelect: vi.fn(),
    moveSnapEnabled: () => true,
    setMoveSnapEnabled: vi.fn(),
    showTransformControls: () => true,
    setShowTransformControls: vi.fn(),
    hoveredLayerId: () => null,
    docWidth: () => 800,
    docHeight: () => 600,
    shapeKind: () => "rect",
    setShapeKind: vi.fn(),
    shapeFillEnabled: () => true,
    setShapeFillEnabled: vi.fn(),
    shapeStrokeEnabled: () => false,
    setShapeStrokeEnabled: vi.fn(),
    shapeStrokeColor: () => "#000000",
    setShapeStrokeColor: vi.fn(),
    shapeStrokeWidth: () => 4,
    setShapeStrokeWidth: vi.fn(),
    shapeRadius: () => 0,
    setShapeRadius: vi.fn(),
    shapeArrowHead: () => false,
    setShapeArrowHead: vi.fn(),
    fgColor: () => "#E15A17",
    ...overrides,
  };
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("ShapeOptionBar", () => {
  it("draw mode: renders kind buttons when no shape layer is selected", () => {
    buildMock();
    const { container, cleanup } = mountShapeBar();
    for (const label of ["Rectangle", "Ellipse", "Line", "Arrow"]) {
      expect(container.querySelector(`button[aria-label="${label}"]`)).not.toBeNull();
    }
    cleanup();
  });

  it("clicking a kind button updates the shapeKind signal", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountShapeBar();
    qs<HTMLButtonElement>(container, 'button[aria-label="Ellipse"]')!.click();
    expect(setters.setShapeKind).toHaveBeenCalledWith("ellipse");
    cleanup();
  });

  it("Arrow kind button sets kind=line and enables arrow head", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountShapeBar();
    qs<HTMLButtonElement>(container, 'button[aria-label="Arrow"]')!.click();
    expect(setters.setShapeKind).toHaveBeenCalledWith("line");
    expect(setters.setShapeArrowHead).toHaveBeenCalledWith(true);
    cleanup();
  });

  it("clicking the Stroke/fill toggles flips their signals", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountShapeBar();
    const stroke = qs<HTMLInputElement>(container, 'input[aria-label="Stroke"]')!;
    stroke.click();
    expect(setters.setShapeStrokeEnabled).toHaveBeenCalledWith(true);
    const fill = qs<HTMLInputElement>(container, 'input[aria-label="Fill"]')!;
    fill.click();
    expect(setters.setShapeFillEnabled).toHaveBeenCalledWith(false);
    cleanup();
  });

  it("changing stroke width range updates shapeStrokeWidth", () => {
    const { setters } = buildMock({ shapeStrokeEnabled: () => true });
    const { container, cleanup } = mountShapeBar();
    const width = qs<HTMLInputElement>(container, 'input[aria-label="Stroke width"]')!;
    expect(width).not.toBeNull();
    width.value = "12";
    width.dispatchEvent(new Event("input", { bubbles: true }));
    expect(setters.setShapeStrokeWidth).toHaveBeenCalledWith(12);
    cleanup();
  });

  it("stroke color swatch is hidden when stroke is disabled", () => {
    buildMock();
    const { container, cleanup } = mountShapeBar();
    expect(qs<HTMLInputElement>(container, 'input[aria-label="Stroke color"]')).toBeNull();
    cleanup();
  });

  it("stroke color swatch appears and updates shapeStrokeColor when stroke enabled", () => {
    const { setters } = buildMock({ shapeStrokeEnabled: () => true });
    const { container, cleanup } = mountShapeBar();
    const color = qs<HTMLInputElement>(container, 'input[aria-label="Stroke color"]')!;
    expect(color).not.toBeNull();
    color.value = "#123456";
    color.dispatchEvent(new Event("input", { bubbles: true }));
    expect(setters.setShapeStrokeColor).toHaveBeenCalledWith("#123456");
    cleanup();
  });

  it("radius range visible for rect, hidden for line", () => {
    buildMock();
    const { container, cleanup } = mountShapeBar();
    expect(qs<HTMLInputElement>(container, 'input[aria-label="Corner radius"]')).not.toBeNull();
    cleanup();
  });

  it("radius range hidden for line kind", () => {
    buildMock({ shapeKind: () => "line" });
    const { container, cleanup } = mountShapeBar();
    expect(qs<HTMLInputElement>(container, 'input[aria-label="Corner radius"]')).toBeNull();
    cleanup();
  });

  it("changing radius input calls setShapeRadius", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountShapeBar();
    const radius = qs<HTMLInputElement>(container, 'input[aria-label="Corner radius"]')!;
    radius.value = "30";
    radius.dispatchEvent(new Event("input", { bubbles: true }));
    expect(setters.setShapeRadius).toHaveBeenCalledWith(30);
    cleanup();
  });

  it("arrow-head toggle visible for line kind, hidden for rect", () => {
    buildMock({ shapeKind: () => "line" });
    const { container, cleanup } = mountShapeBar();
    expect(qs<HTMLInputElement>(container, 'input[aria-label="Arrow head"]')).not.toBeNull();
    cleanup();
  });

  it("arrow-head toggle hidden for rect kind", () => {
    buildMock();
    const { container, cleanup } = mountShapeBar();
    expect(qs<HTMLInputElement>(container, 'input[aria-label="Arrow head"]')).toBeNull();
    cleanup();
  });

  it("edit mode: controls reflect selected shape layer and commit before mutation", () => {
    const layer = makeShapeLayer({
      id: "s1",
      shapeParams: { ...baseParams, kind: "ellipse", radius: 5, stroke: { enabled: true, color: "#00ff00", width: 10 } },
    });
    const { engine, commit } = buildMock({}, layer);
    const { container, cleanup } = mountShapeBar();

    // radius hidden because the selected shape is an ellipse
    expect(qs<HTMLInputElement>(container, 'input[aria-label="Corner radius"]')).toBeNull();
    const ellipse = qs<HTMLButtonElement>(container, 'button[aria-label="Ellipse"]')!;
    expect(ellipse.getAttribute("aria-pressed")).toBe("true");

    // commit happened BEFORE updateShapeParams (wiring rule)
    const width = qs<HTMLInputElement>(container, 'input[aria-label="Stroke width"]')!;
    width.value = "14";
    width.dispatchEvent(new Event("input", { bubbles: true }));
    expect(commit).toHaveBeenCalled();
    expect(engine.updateShapeParams).toHaveBeenCalledWith(
      "s1",
      expect.objectContaining({ stroke: expect.objectContaining({ width: 14 }) })
    );
    expect(commit.mock.invocationCallOrder[0]).toBeLessThan(engine.updateShapeParams.mock.invocationCallOrder[0]);
    cleanup();
  });
});

describe("OptionBar shape mount gating", () => {
  it("shows ShapeOptionBar when the shape tool is active", () => {
    mockUseEditor(optionBarEditor({ activeTool: () => "shape", selectedLayerId: () => null }) as any);
    const { container, cleanup } = mountOptionBar();
    expect(container.querySelector("[data-shape-option-bar]")).not.toBeNull();
    cleanup();
  });

  it("shows ShapeOptionBar when a shape layer is selected (tool may be move)", () => {
    const layer = makeShapeLayer();
    mockUseEditor(optionBarEditor({ activeTool: () => "move" }, layer) as any);
    const { container, cleanup } = mountOptionBar();
    expect(container.querySelector("[data-shape-option-bar]")).not.toBeNull();
    cleanup();
  });

  it("hides ShapeOptionBar when move tool is active and no shape is selected", () => {
    mockUseEditor(optionBarEditor({ activeTool: () => "move" }) as any);
    const { container, cleanup } = mountOptionBar();
    expect(container.querySelector("[data-shape-option-bar]")).toBeNull();
    cleanup();
  });
});