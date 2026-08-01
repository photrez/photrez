import { describe, it, expect, vi, afterEach } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { MoveOptionBar } from "../MoveOptionBar";
import type { LayerNode } from "@/engine/types";
import { mockEditorContext } from "../../../__tests__/test-builders";

function tick(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function mockLayer(overrides: Partial<LayerNode> = {}): LayerNode {
  return {
    id: "layer-1",
    name: "Layer",
    type: "raster",
    width: 100,
    height: 100,
    opacity: 1,
    visible: true,
    locked: false,
    blendMode: "normal",
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    imageBitmap: null,
    ...overrides,
  };
}

function byLabel(label: string): string {
  return `button[aria-label="${label}"]`;
}

function qs<T extends HTMLElement>(root: HTMLElement, sel: string): T | null {
  return root.querySelector(sel) as T | null;
}

describe("MoveOptionBar", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders tool name badge", () => {
    const [activeTool] = createSignal("move");
    const mockValue = {
      workspace: { getActiveEngine: () => null, getActiveHistory: () => null },
      activeTool,
      layers: () => [] as LayerNode[],
      activeLayerId: () => null,
      selectedLayerId: () => null,
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    expect(container.textContent).toContain("Move");
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("renders Auto-select toggle", () => {
    const [moveAutoSelect, setMoveAutoSelect] = createSignal(true);
    const mockValue = {
      workspace: { getActiveEngine: () => null, getActiveHistory: () => null },
      activeTool: () => "move",
      layers: () => [] as LayerNode[],
      activeLayerId: () => null,
      selectedLayerId: () => null,
      scheduler: { requestRender: vi.fn() },
      moveAutoSelect,
      setMoveAutoSelect,
      moveSnapEnabled: () => true,
      setMoveSnapEnabled: vi.fn(),
      showTransformControls: () => true,
      setShowTransformControls: vi.fn(),
      hoveredLayerId: () => null,
      docWidth: () => 800,
      docHeight: () => 600,
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    expect(container.textContent).toContain("Auto");
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("renders Snap toggle", () => {
    const [moveSnapEnabled, setMoveSnapEnabled] = createSignal(false);
    const mockValue = {
      workspace: { getActiveEngine: () => null, getActiveHistory: () => null },
      activeTool: () => "move",
      layers: () => [] as LayerNode[],
      activeLayerId: () => null,
      selectedLayerId: () => null,
      scheduler: { requestRender: vi.fn() },
      moveAutoSelect: () => true,
      setMoveAutoSelect: vi.fn(),
      moveSnapEnabled,
      setMoveSnapEnabled,
      showTransformControls: () => true,
      setShowTransformControls: vi.fn(),
      hoveredLayerId: () => null,
      docWidth: () => 800,
      docHeight: () => 600,
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    expect(container.textContent).toContain("Snap");
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("renders hovered layer target badge when auto-select enabled and hovered", () => {
    const [hoveredLayerId] = createSignal("layer-1");
    const layers: LayerNode[] = [mockLayer({ name: "Test Layer" })];
    const mockValue = {
      workspace: { getActiveEngine: () => null, getActiveHistory: () => null },
      activeTool: () => "move",
      layers: () => layers,
      activeLayerId: () => null,
      selectedLayerId: () => null,
      scheduler: { requestRender: vi.fn() },
      moveAutoSelect: () => true,
      setMoveAutoSelect: vi.fn(),
      moveSnapEnabled: () => true,
      setMoveSnapEnabled: vi.fn(),
      showTransformControls: () => true,
      setShowTransformControls: vi.fn(),
      hoveredLayerId,
      docWidth: () => 800,
      docHeight: () => 600,
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    expect(container.textContent).toContain("Target:");
    expect(container.textContent).toContain("Test Layer");
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("does not render hovered target badge when auto-select disabled", () => {
    const [hoveredLayerId] = createSignal("layer-1");
    const layers: LayerNode[] = [mockLayer({ name: "Test Layer" })];
    const mockValue = {
      workspace: { getActiveEngine: () => null, getActiveHistory: () => null },
      activeTool: () => "move",
      layers: () => layers,
      activeLayerId: () => null,
      selectedLayerId: () => null,
      scheduler: { requestRender: vi.fn() },
      moveAutoSelect: () => false,
      setMoveAutoSelect: vi.fn(),
      moveSnapEnabled: () => true,
      setMoveSnapEnabled: vi.fn(),
      showTransformControls: () => true,
      setShowTransformControls: vi.fn(),
      hoveredLayerId,
      docWidth: () => 800,
      docHeight: () => 600,
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    expect(container.textContent).not.toContain("Target:");
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("shows locked indicator when active layer is locked", () => {
    const engine = {
      getLayer: () => ({ locked: true }),
      snapshot: vi.fn(),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => ({ commit: vi.fn() }) },
      activeTool: () => "move",
      layers: () => [mockLayer({ name: "Locked Layer", locked: true })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    expect(container.textContent).toContain("Locked");
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("calls flipLayer when flip H button clicked", () => {
    const engine = {
      getLayer: () => ({ locked: false }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({})],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    const flipHBtn = qs<HTMLButtonElement>(container, 'button[aria-label="Flip horizontal"]')!;
    flipHBtn.click();

    expect(history.commit).toHaveBeenCalled();
    expect(engine.flipLayer).toHaveBeenCalledWith("layer-1", "h");
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("calls flipLayer when flip V button clicked", () => {
    const engine = {
      getLayer: () => ({ locked: false }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({})],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    const flipVBtn = qs<HTMLButtonElement>(container, 'button[aria-label="Flip vertical"]')!;
    flipVBtn.click();

    expect(history.commit).toHaveBeenCalled();
    expect(engine.flipLayer).toHaveBeenCalledWith("layer-1", "v");
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("calls handleResetTransform on Reset button click", () => {
    const engine = {
      getLayer: () => ({ locked: false, transform: {} }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({ name: "Layer", transform: { x: 10, y: 20, scaleX: 1, scaleY: 1, rotation: 30, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    const resetBtn = Array.from(container.querySelectorAll("button")).find(b => b.textContent === "Reset")!;
    resetBtn.click();

    expect(history.commit).toHaveBeenCalled();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1", {
      x: 0, y: 0, scaleX: 1.0, scaleY: 1.0, rotation: 0, flipH: false, flipV: false,
    });
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("locked layer disables Reset button", () => {
    const engine = {
      getLayer: () => ({ locked: true }),
      snapshot: vi.fn(),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => ({ commit: vi.fn() }) },
      activeTool: () => "move",
      layers: () => [mockLayer({ name: "Locked", locked: true })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    const resetBtn = Array.from(container.querySelectorAll("button")).find(b => b.textContent === "Reset")!;
    expect((resetBtn as HTMLButtonElement).disabled).toBe(true);
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("calls transformLayer with correct position on X field submit", async () => {
    const engine = {
      getLayer: () => ({ locked: false, transform: { x: 10, y: 20, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({ name: "Layer", transform: { x: 10, y: 20, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    await tick();

    const xInput = qs<HTMLInputElement>(container, 'input[type="text"]')!;
    xInput.focus();
    xInput.dispatchEvent(new Event("focus", { bubbles: true }));
    (xInput as HTMLInputElement).value = "150";
    xInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
    xInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    expect(history.commit).toHaveBeenCalled();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1", { x: 150, y: 20, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false });
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("renders align buttons", () => {
    const engine = {
      getLayer: () => ({ locked: false }),
      snapshot: vi.fn(),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => ({ commit: vi.fn() }) },
      activeTool: () => "move",
      layers: () => [mockLayer({})],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    const alignLabels = ["Align left", "Align center horizontal", "Align right", "Align top", "Align center vertical", "Align bottom"];
    for (const label of alignLabels) {
      expect(container.querySelector(`button[aria-label="${label}"]`)).not.toBeNull();
    }
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("calls align left correctly", () => {
    const engine = {
      getLayer: () => ({ locked: false, width: 100, height: 100, transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({ name: "Layer", transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    qs<HTMLButtonElement>(container, 'button[aria-label="Align left"]')!.click();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1", { x: 0, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false });
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("calls align center horizontal correctly", () => {
    const engine = {
      getLayer: () => ({ locked: false, width: 100, height: 100, transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({ name: "Layer", transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    qs<HTMLButtonElement>(container, 'button[aria-label="Align center horizontal"]')!.click();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1", { x: 350, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false });
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("calls align right correctly", () => {
    const engine = {
      getLayer: () => ({ locked: false, width: 100, height: 100, transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({ name: "Layer", transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    qs<HTMLButtonElement>(container, 'button[aria-label="Align right"]')!.click();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1", { x: 700, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false });
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("calls align top correctly", () => {
    const engine = {
      getLayer: () => ({ locked: false, width: 100, height: 100, transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({ name: "Layer", transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    qs<HTMLButtonElement>(container, 'button[aria-label="Align top"]')!.click();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1", { x: 50, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false });
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("calls align center vertical correctly", () => {
    const engine = {
      getLayer: () => ({ locked: false, width: 100, height: 100, transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({ name: "Layer", transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    qs<HTMLButtonElement>(container, 'button[aria-label="Align center vertical"]')!.click();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1", { x: 50, y: 250, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false });
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("calls align bottom correctly", () => {
    const engine = {
      getLayer: () => ({ locked: false, width: 100, height: 100, transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({ name: "Layer", transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    qs<HTMLButtonElement>(container, 'button[aria-label="Align bottom"]')!.click();
    expect(engine.transformLayer).toHaveBeenCalledWith("layer-1", { x: 50, y: 500, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false });
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("locked layer prevents align actions", () => {
    const engine = {
      getLayer: () => ({ locked: true, width: 100, height: 100, transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } }),
      snapshot: vi.fn(),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => ({ commit: vi.fn() }) },
      activeTool: () => "move",
      layers: () => [mockLayer({ name: "Locked", locked: true, transform: { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    qs<HTMLButtonElement>(container, 'button[aria-label="Align left"]')!.click();
    expect(engine.transformLayer).not.toHaveBeenCalled();
    qs<HTMLButtonElement>(container, 'button[aria-label="Flip horizontal"]')!.click();
    expect(engine.flipLayer).not.toHaveBeenCalled();
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("renders W and H display fields when active layer exists", () => {
    const engine = {
      getLayer: () => ({ locked: false, width: 100, height: 200, transform: { x: 0, y: 0, scaleX: 2, scaleY: 1.5, rotation: 0, flipH: false, flipV: false } }),
      snapshot: vi.fn(),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => ({ commit: vi.fn() }) },
      activeTool: () => "move",
      layers: () => [mockLayer({ name: "Layer", width: 100, height: 200, transform: { x: 0, y: 0, scaleX: 2, scaleY: 1.5, rotation: 0, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    expect(container.textContent).toContain("W");
    expect(container.textContent).toContain("H");
    expect(container.textContent).toContain("200px"); // 100 * 2 = 200
    expect(container.textContent).toContain("300px"); // 200 * 1.5 = 300
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("renders nothing when no active layer and no hovered layer", () => {
    const mockValue = {
      workspace: { getActiveEngine: () => null, getActiveHistory: () => null },
      activeTool: () => "move",
      layers: () => [] as LayerNode[],
      activeLayerId: () => null,
      selectedLayerId: () => null,
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    expect(container.textContent).toContain("Move");
    expect(container.textContent).toContain("Auto");
    expect(container.textContent).toContain("Snap");
    expect(container.textContent).not.toContain("X:");
    dispose();
    container.parentNode?.removeChild(container);
  });

  // ─── Regression 2026-06-18 follow-up: skip ghost commits on no-op clicks ───

  it("clicking Align Left when already at x=0 produces NO history entry (regression: ghost commit)", () => {
    const engine = {
      getLayer: () => ({
        id: "layer-1",
        locked: false,
        width: 100,
        height: 100,
        transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({ width: 100, height: 100, transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    qs<HTMLButtonElement>(container, 'button[aria-label="Align left"]')!.click();
    expect(history.commit).not.toHaveBeenCalled();
    expect(engine.transformLayer).not.toHaveBeenCalled();

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("clicking Align Left when NOT at x=0 commits exactly once", () => {
    const engine = {
      getLayer: () => ({
        id: "layer-1",
        locked: false,
        width: 100,
        height: 100,
        transform: { x: 50, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({ width: 100, height: 100, transform: { x: 50, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    qs<HTMLButtonElement>(container, 'button[aria-label="Align left"]')!.click();
    expect(history.commit).toHaveBeenCalledTimes(1);
    expect(engine.transformLayer).toHaveBeenCalledTimes(1);

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("clicking Reset Transform when already at default produces NO history entry (regression: ghost commit)", () => {
    const engine = {
      getLayer: () => ({
        id: "layer-1",
        locked: false,
        width: 100,
        height: 100,
        transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({ transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    const resetBtn = Array.from(container.querySelectorAll("button")).find(
      (b) => b.textContent?.trim() === "Reset",
    ) as HTMLButtonElement | undefined;
    expect(resetBtn).toBeDefined();
    resetBtn!.click();

    expect(history.commit).not.toHaveBeenCalled();
    expect(engine.transformLayer).not.toHaveBeenCalled();

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("clicking Reset Transform when transform IS modified commits exactly once", () => {
    const engine = {
      getLayer: () => ({
        id: "layer-1",
        locked: false,
        width: 100,
        height: 100,
        transform: { x: 50, y: 20, scaleX: 1.5, scaleY: 1, rotation: 45, flipH: false, flipV: false },
      }),
      snapshot: vi.fn(() => ({})),
      flipLayer: vi.fn(),
      transformLayer: vi.fn(),
    };
    const history = { commit: vi.fn() };
    const mockValue = {
      workspace: { getActiveEngine: () => engine, getActiveHistory: () => history },
      activeTool: () => "move",
      layers: () => [mockLayer({ transform: { x: 50, y: 20, scaleX: 1.5, scaleY: 1, rotation: 45, flipH: false, flipV: false } })],
      activeLayerId: () => "layer-1",
      selectedLayerId: () => "layer-1",
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
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    const resetBtn = Array.from(container.querySelectorAll("button")).find(
      (b) => b.textContent?.trim() === "Reset",
    ) as HTMLButtonElement | undefined;
    expect(resetBtn).toBeDefined();
    resetBtn!.click();

    expect(history.commit).toHaveBeenCalledTimes(1);
    expect(engine.transformLayer).toHaveBeenCalledTimes(1);

    dispose();
    container.parentNode?.removeChild(container);
  });

  // ─── Transform Controls wiring test ──────────────────────────────────────

  it("Transform Controls checkbox renders and calls setShowTransformControls on click", () => {
    const setter = vi.fn();
    const mockValue = {
      workspace: { getActiveEngine: () => null, getActiveHistory: () => null },
      activeTool: () => "move",
      layers: () => [] as LayerNode[],
      activeLayerId: () => null,
      selectedLayerId: () => null,
      scheduler: { requestRender: vi.fn() },
      moveAutoSelect: () => true,
      setMoveAutoSelect: vi.fn(),
      moveSnapEnabled: () => true,
      setMoveSnapEnabled: vi.fn(),
      showTransformControls: () => true,
      setShowTransformControls: setter,
      hoveredLayerId: () => null,
      docWidth: () => 800,
      docHeight: () => 600,
    };
    mockUseEditor(mockValue);

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(() => <MoveOptionBar />, container);

    // The label text should be visible
    expect(container.textContent).toContain("Transform Controls");

    // Find the checkbox input and click it
    const checkbox = container.querySelector<HTMLInputElement>('input[type="checkbox"]');
    expect(checkbox).not.toBeNull();

    // Now find the correct checkbox — the third checkbox (Auto-Select, Snap, Transform Controls)
    const allCheckboxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
    // The last checkbox should be Transform Controls
    const tcCheckbox = allCheckboxes[allCheckboxes.length - 1];
    expect(tcCheckbox).not.toBeNull();

    // Click it — onChange should call the setter
    tcCheckbox.click();
    expect(setter).toHaveBeenCalledTimes(1);

    dispose();
    container.parentNode?.removeChild(container);
  });
});
