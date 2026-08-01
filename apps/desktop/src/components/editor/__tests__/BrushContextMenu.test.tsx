import { describe, expect, it, vi, afterEach } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { BrushContextMenu } from "../BrushContextMenu";

function createMockEditor(overrides: Record<string, any> = {}) {
  const defaults: Record<string, any> = {
    activeTool: "brush",
    brushSize: 20,
    brushHardness: 0.8,
    brushOpacity: 1,
    brushFlow: 1,
    brushSmoothing: 0,
    eraserSize: 32,
    eraserHardness: 1,
    eraserOpacity: 1,
    eraserFlow: 1,
    eraserSmoothing: 0,
    brushPresetId: null,
    eraserPresetId: null,
  };
  const merged = { ...defaults, ...overrides };
  const signals: Record<string, any> = {};
  for (const [key, val] of Object.entries(merged)) {
    const [s, set] = createSignal(val);
    signals[key] = s;
    const setKey = "set" + key.charAt(0).toUpperCase() + key.slice(1);
    signals[setKey] = set;
  }
  return signals;
}

describe("BrushContextMenu", () => {
  let root: HTMLDivElement;
  let container: HTMLDivElement;

  afterEach(() => {
    document.body.innerHTML = "";
    vi.restoreAllMocks();
  });

  it("renders nothing by default (not open)", () => {
    const mock = createMockEditor({ activeTool: "brush" });
    mockUseEditor(mock);

    root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushContextMenu />, root);

    expect(root.textContent).toBe("");

    dispose();
  });

  it("opens on contextmenu event on the canvas container", () => {
    const mock = createMockEditor({ activeTool: "brush" });
    mockUseEditor(mock);

    container = document.createElement("div");
    container.id = "canvas-container";
    document.body.appendChild(container);

    root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushContextMenu />, root);

    const event = new MouseEvent("contextmenu", {
      bubbles: true,
      clientX: 200,
      clientY: 150,
    });
    container.dispatchEvent(event);

    expect(root.textContent).toContain("Size");
    expect(root.textContent).toContain("Hardness");
    expect(root.textContent).toContain("Strength");

    dispose();
  });

  it("does not open for non-paint tools", () => {
    const mock = createMockEditor({ activeTool: "move" });
    mockUseEditor(mock);

    container = document.createElement("div");
    container.id = "canvas-container";
    document.body.appendChild(container);

    root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushContextMenu />, root);

    const event = new MouseEvent("contextmenu", {
      bubbles: true,
      clientX: 200,
      clientY: 150,
    });
    container.dispatchEvent(event);

    expect(root.textContent).toBe("");

    dispose();
  });

  it("size slider updates brush size signal via non-linear mapping", async () => {
    const mock = createMockEditor({ activeTool: "brush", brushSize: 30 });
    mockUseEditor(mock);

    container = document.createElement("div");
    container.id = "canvas-container";
    document.body.appendChild(container);

    root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushContextMenu />, root);

    container.dispatchEvent(
      new MouseEvent("contextmenu", {
        bubbles: true,
        clientX: 200,
        clientY: 150,
      }),
    );

    const sizeSlider = root.querySelector<HTMLInputElement>(
      "[data-context-size]",
    )!;
    expect(sizeSlider).toBeTruthy();
    // slider is 0-100 non-linear; size=30 maps to slider ~4
    expect(sizeSlider.value).toBe("4");

    // slider=50 → size~334
    sizeSlider.value = "50";
    sizeSlider.dispatchEvent(new Event("input", { bubbles: true }));
    expect(mock.brushSize()).toBe(334);

    dispose();
  });

  it("hardness slider updates brush hardness signal synchronously", () => {
    const mock = createMockEditor({ activeTool: "brush", brushHardness: 0.5 });
    mockUseEditor(mock);

    container = document.createElement("div");
    container.id = "canvas-container";
    document.body.appendChild(container);

    root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushContextMenu />, root);

    container.dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, clientX: 200, clientY: 150 }),
    );

    const hardSlider = root.querySelector<HTMLInputElement>("[data-context-hardness]")!;
    expect(hardSlider).toBeTruthy();
    // hardness=0.5 → slider ~50
    expect(hardSlider.value).toBe("50");

    hardSlider.value = "80";
    hardSlider.dispatchEvent(new Event("input", { bubbles: true }));
    expect(mock.brushHardness()).toBe(0.8);

    dispose();
  });

  it("strength slider updates brush opacity signal synchronously", () => {
    const mock = createMockEditor({ activeTool: "brush", brushOpacity: 0.7 });
    mockUseEditor(mock);

    container = document.createElement("div");
    container.id = "canvas-container";
    document.body.appendChild(container);

    root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushContextMenu />, root);

    container.dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, clientX: 200, clientY: 150 }),
    );

    const strengthSlider = root.querySelector<HTMLInputElement>("[data-context-strength]")!;
    expect(strengthSlider).toBeTruthy();
    // opacity=0.7 → slider 70
    expect(strengthSlider.value).toBe("70");

    strengthSlider.value = "45";
    strengthSlider.dispatchEvent(new Event("input", { bubbles: true }));
    expect(mock.brushOpacity()).toBe(0.45);

    dispose();
  });

  it("closes on Escape key", () => {
    const mock = createMockEditor({ activeTool: "brush" });
    mockUseEditor(mock);

    container = document.createElement("div");
    container.id = "canvas-container";
    document.body.appendChild(container);

    root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushContextMenu />, root);

    container.dispatchEvent(
      new MouseEvent("contextmenu", {
        bubbles: true,
        clientX: 100,
        clientY: 100,
      }),
    );
    expect(root.textContent).toContain("Size");

    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
    expect(root.textContent).toBe("");

    dispose();
  });

  it("targets the active tool's preset store even if the tool changes after mount", () => {
    const mock = createMockEditor({
      activeTool: "brush",
      brushPresetId: "brush-preset",
      eraserPresetId: "eraser-preset",
    });
    mockUseEditor(mock);

    container = document.createElement("div");
    container.id = "canvas-container";
    document.body.appendChild(container);

    root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushContextMenu />, root);

    mock.setActiveTool("eraser");
    container.dispatchEvent(
      new MouseEvent("contextmenu", {
        bubbles: true,
        clientX: 100,
        clientY: 100,
      }),
    );

    const resetButton = Array.from(root.querySelectorAll("button")).find(
      (button) => button.textContent?.trim() === "Reset",
    ) as HTMLButtonElement;
    resetButton.click();

    expect(mock.eraserPresetId()).toBeNull();
    expect(mock.brushPresetId()).toBe("brush-preset");

    dispose();
  });
});
