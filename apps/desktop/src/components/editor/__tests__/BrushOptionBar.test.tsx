import { describe, expect, it, vi } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { BrushOptionBar } from "../BrushOptionBar";

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

describe("BrushOptionBar", () => {
  it("shows brush settings for Brush tool", () => {
    const mock = createMockEditor({ activeTool: "brush", brushSize: 20, brushHardness: 0.8, brushOpacity: 1 });
    mockUseEditor(mock);

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushOptionBar />, root);

    expect(root.textContent).toContain("Brush");
    expect(root.querySelector<HTMLInputElement>("[data-paint-size]")?.value).toBe("20");
    expect(root.querySelector<HTMLInputElement>("[data-paint-hardness]")?.value).toBe("80");
    expect(root.querySelector<HTMLInputElement>("[data-paint-opacity]")?.value).toBe("100");

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });

  it("shows eraser settings for Eraser tool", () => {
    const mock = createMockEditor({ activeTool: "eraser", eraserSize: 32, eraserHardness: 1, eraserOpacity: 1 });
    mockUseEditor(mock);

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushOptionBar />, root);

    expect(root.textContent).toContain("Eraser");
    expect(root.querySelector<HTMLInputElement>("[data-paint-size]")?.value).toBe("32");
    expect(root.querySelector<HTMLInputElement>("[data-paint-hardness]")?.value).toBe("100");

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });

  it("updates active tool size without changing inactive size", async () => {
    const mock = createMockEditor({ activeTool: "eraser", eraserSize: 32, brushSize: 20 });
    mockUseEditor(mock);

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushOptionBar />, root);

    root.querySelector<HTMLInputElement>("[data-paint-size]")!.value = "44";
    root.querySelector<HTMLInputElement>("[data-paint-size]")!.dispatchEvent(new Event("input", { bubbles: true }));

    expect(mock.eraserSize()).toBe(44);
    expect(mock.brushSize()).toBe(20);

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });

  it("renders flow and smoothing inputs", () => {
    const mock = createMockEditor({ activeTool: "brush", brushFlow: 0.8, brushSmoothing: 25 });
    mockUseEditor(mock);

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushOptionBar />, root);

    expect(root.querySelector<HTMLInputElement>("[data-paint-flow]")).toBeTruthy();
    expect(root.querySelector<HTMLInputElement>("[data-paint-flow]")!.value).toBe("80");
    expect(root.querySelector<HTMLInputElement>("[data-paint-smoothing]")).toBeTruthy();
    expect(root.querySelector<HTMLInputElement>("[data-paint-smoothing]")!.value).toBe("25");

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });

  it("renders preset dropdown with current preset name or Custom", () => {
    const mock = createMockEditor({ activeTool: "brush", brushPresetId: null });
    mockUseEditor(mock);

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushOptionBar />, root);

    const presetBtn = root.querySelector<HTMLButtonElement>("[data-paint-preset]");
    expect(presetBtn).toBeTruthy();
    expect(presetBtn!.textContent).toContain("Custom");

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });

  it("range slider [data-paint-size-slider] fires synchronous signal write (no RAF)", () => {
    const mock = createMockEditor({ activeTool: "brush", brushSize: 50 });
    mockUseEditor(mock);

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushOptionBar />, root);

    const rangeSlider = root.querySelector<HTMLInputElement>("[data-paint-size-slider]")!;
    expect(rangeSlider).toBeTruthy();

    // slider=75 → MAX_LINEAR_SIZE=500 (non-linear mapping, 0-75 is linear 1-500)
    rangeSlider.value = "75";
    rangeSlider.dispatchEvent(new Event("input", { bubbles: true }));
    // Assert synchronous — signal updated immediately, no RAF deferral
    expect(mock.brushSize()).toBe(500);

    // slider=100 → MAX_PAINT_SIZE=2000
    rangeSlider.value = "100";
    rangeSlider.dispatchEvent(new Event("input", { bubbles: true }));
    expect(mock.brushSize()).toBe(2000);

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });

  it("all slider types fire synchronous writes (hardness, opacity, flow, smoothing)", () => {
    const mock = createMockEditor({
      activeTool: "brush",
      brushHardness: 0.5,
      brushOpacity: 0.8,
      brushFlow: 0.6,
      brushSmoothing: 30,
    });
    mockUseEditor(mock);

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <BrushOptionBar />, root);

    // Hardness number input
    const hardInput = root.querySelector<HTMLInputElement>("[data-paint-hardness]")!;
    hardInput.value = "75";
    hardInput.dispatchEvent(new Event("input", { bubbles: true }));
    expect(mock.brushHardness()).toBe(0.75);

    // Opacity number input
    const opInput = root.querySelector<HTMLInputElement>("[data-paint-opacity]")!;
    opInput.value = "60";
    opInput.dispatchEvent(new Event("input", { bubbles: true }));
    expect(mock.brushOpacity()).toBe(0.6);

    // Flow number input
    const flowInput = root.querySelector<HTMLInputElement>("[data-paint-flow]")!;
    flowInput.value = "40";
    flowInput.dispatchEvent(new Event("input", { bubbles: true }));
    expect(mock.brushFlow()).toBe(0.4);

    // Smoothing number input
    const smoothInput = root.querySelector<HTMLInputElement>("[data-paint-smoothing]")!;
    smoothInput.value = "55";
    smoothInput.dispatchEvent(new Event("input", { bubbles: true }));
    expect(mock.brushSmoothing()).toBe(55);

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });
});
