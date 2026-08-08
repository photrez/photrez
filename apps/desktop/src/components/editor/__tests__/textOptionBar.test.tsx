import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { render } from "solid-js/web";
import { OptionBar } from "../shell/OptionBar";
import { TextOptionBar } from "../TextOptionBar";
import { resetFontCache } from "@/lib/fontEnumeration";
import type { LayerNode } from "@/engine/types";
import type { TextData } from "@/engine/textTypes";

const baseData: TextData = {
  content: "Hello",
  fontFamily: "Arial",
  fontSize: 48,
  fontWeight: 400,
  fontStyle: "normal",
  color: "#ff0000",
  align: "left",
  lineHeight: 1.4,
  letterSpacing: 0,
  boxMode: "point",
  boxWidth: 0,
};

function makeTextLayer(overrides: Partial<LayerNode> = {}): LayerNode {
  return {
    id: "text-1",
    name: "Text",
    type: "text",
    width: 100,
    height: 50,
    opacity: 1,
    visible: true,
    locked: false,
    blendMode: "normal",
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    imageBitmap: null,
    textData: { ...baseData },
    ...overrides,
  };
}

function makeRasterLayer(overrides: Partial<LayerNode> = {}): LayerNode {
  return {
    id: "r1",
    name: "Raster",
    type: "raster",
    width: 8,
    height: 8,
    opacity: 1,
    visible: true,
    locked: false,
    blendMode: "normal",
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    imageBitmap: null,
    ...overrides,
  };
}

type Setters = {
  setTextFontFamily: ReturnType<typeof vi.fn>;
  setTextFontSize: ReturnType<typeof vi.fn>;
  setTextFontWeight: ReturnType<typeof vi.fn>;
  setTextFontItalic: ReturnType<typeof vi.fn>;
  setTextAlign: ReturnType<typeof vi.fn>;
  setFgColor: ReturnType<typeof vi.fn>;
};

function buildMock(overrides: Record<string, unknown> = {}, layer?: LayerNode) {
  const setters: Setters = {
    setTextFontFamily: vi.fn(),
    setTextFontSize: vi.fn(),
    setTextFontWeight: vi.fn(),
    setTextFontItalic: vi.fn(),
    setTextAlign: vi.fn(),
    setFgColor: vi.fn(),
  };
  const updateTextData = vi.fn();
  const commit = vi.fn();
  const snapshot = vi.fn(() => ({}));
  const engine = { getLayer: () => layer ?? undefined, updateTextData, snapshot };
  const editor = {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => ({ commit }),
    },
    activeTool: () => "text",
    layers: () => (layer ? [layer] : []),
    selectedLayerId: () => (layer ? layer.id : null),
    fgColor: () => "#E15A17",
    setFgColor: setters.setFgColor,
    textFontFamily: () => (layer ? layer.textData!.fontFamily : "Arial"),
    setTextFontFamily: setters.setTextFontFamily,
    textFontSize: () => (layer ? layer.textData!.fontSize : 48),
    setTextFontSize: setters.setTextFontSize,
    textFontWeight: () => (layer ? layer.textData!.fontWeight : 400),
    setTextFontWeight: setters.setTextFontWeight,
    textFontItalic: () => (layer ? layer.textData!.fontStyle === "italic" : false),
    setTextFontItalic: setters.setTextFontItalic,
    textAlign: () => (layer ? layer.textData!.align : "left"),
    setTextAlign: setters.setTextAlign,
    ...overrides,
  };
  mockUseEditor(editor as any);
  return { setters, engine, commit, snapshot, editor };
}

function mountTextBar() {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(() => <TextOptionBar />, container);
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

/** Minimal editor enough for OptionBar's move branch + TextOptionBar. */
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
    fgColor: () => "#E15A17",
    setFgColor: vi.fn(),
    textFontFamily: () => "Arial",
    setTextFontFamily: vi.fn(),
    textFontSize: () => 48,
    setTextFontSize: vi.fn(),
    textFontWeight: () => 400,
    setTextFontWeight: vi.fn(),
    textFontItalic: () => false,
    setTextFontItalic: vi.fn(),
    textAlign: () => "left",
    setTextAlign: vi.fn(),
    ...overrides,
  };
}

beforeEach(() => {
  resetFontCache();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("TextOptionBar", () => {
  it("draw mode: renders font/size/style/align/color controls when no text layer is selected", () => {
    buildMock();
    const { container, cleanup } = mountTextBar();
    expect(qs(container, "[data-font-picker-trigger]")).not.toBeNull();
    expect(qs<HTMLInputElement>(container, 'input[aria-label="Font size"]')).not.toBeNull();
    expect(qs<HTMLButtonElement>(container, 'button[aria-label="Bold"]')).not.toBeNull();
    expect(qs<HTMLButtonElement>(container, 'button[aria-label="Italic"]')).not.toBeNull();
    expect(qs<HTMLButtonElement>(container, 'button[aria-label="Align left"]')).not.toBeNull();
    expect(qs<HTMLButtonElement>(container, 'button[aria-label="Align center"]')).not.toBeNull();
    expect(qs<HTMLButtonElement>(container, 'button[aria-label="Align right"]')).not.toBeNull();
    expect(qs<HTMLInputElement>(container, 'input[aria-label="Text color"]')).not.toBeNull();
    cleanup();
  });

  it("draw mode: Bold click updates the textFontWeight signal", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();
    qs<HTMLButtonElement>(container, 'button[aria-label="Bold"]')!.click();
    expect(setters.setTextFontWeight).toHaveBeenCalledWith(700);
    cleanup();
  });

  it("draw mode: Italic click updates textFontItalic", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();
    qs<HTMLButtonElement>(container, 'button[aria-label="Italic"]')!.click();
    expect(setters.setTextFontItalic).toHaveBeenCalledWith(true);
    cleanup();
  });

  it("draw mode: size input updates textFontSize (clamped 1..2000)", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();
    const size = qs<HTMLInputElement>(container, 'input[aria-label="Font size"]')!;
    size.value = "64";
    size.dispatchEvent(new Event("input", { bubbles: true }));
    expect(setters.setTextFontSize).toHaveBeenCalledWith(64);
    // clamp
    size.value = "0";
    size.dispatchEvent(new Event("input", { bubbles: true }));
    expect(setters.setTextFontSize).toHaveBeenLastCalledWith(1);
    cleanup();
  });

  it("draw mode: align buttons update textAlign", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();
    qs<HTMLButtonElement>(container, 'button[aria-label="Align center"]')!.click();
    expect(setters.setTextAlign).toHaveBeenCalledWith("center");
    qs<HTMLButtonElement>(container, 'button[aria-label="Align right"]')!.click();
    expect(setters.setTextAlign).toHaveBeenLastCalledWith("right");
    cleanup();
  });

  it("draw mode: color swatch writes the shared editor foreground color", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();
    const color = qs<HTMLInputElement>(container, 'input[aria-label="Text color"]')!;
    color.value = "#123456";
    color.dispatchEvent(new Event("input", { bubbles: true }));
    expect(setters.setFgColor).toHaveBeenCalledWith("#123456");
    cleanup();
  });

  it("font picker: opens with searchable WYSIWYG list and selecting writes the family signal", async () => {
    buildMock();
    const { container, cleanup } = mountTextBar();
    // Font enumeration resolves async (WEB_SAFE fallback in jsdom).
    await new Promise((r) => setTimeout(r, 0));
    qs<HTMLButtonElement>(container, "[data-font-picker-trigger]")!.click();
    const picker = qs(container, "[data-font-picker]")!;
    expect(picker).not.toBeNull();
    const arial = Array.from(picker.querySelectorAll("button")).find(
      (b) => b.textContent?.includes("Arial"),
    );
    expect(arial).toBeDefined();
    // WYSIWYG preview: the swatch is rendered in the family's own face.
    expect(arial!.querySelector("span")!.getAttribute("style")).toContain('"Arial"');
    arial!.click();
    cleanup();
  });

  it("edit mode: controls reflect the selected text layer and commit BEFORE mutation", () => {
    const layer = makeTextLayer({
      id: "t1",
      textData: { ...baseData, fontStyle: "italic", align: "center" },
    });
    const { engine, commit } = buildMock({}, layer);
    const { container, cleanup } = mountTextBar();

    const italic = qs<HTMLButtonElement>(container, 'button[aria-label="Italic"]')!;
    expect(italic.getAttribute("aria-pressed")).toBe("true");
    const center = qs<HTMLButtonElement>(container, 'button[aria-label="Align center"]')!;
    expect(center.getAttribute("aria-pressed")).toBe("true");

    // Clicking Align right is a real edit → commit happened BEFORE updateTextData.
    qs<HTMLButtonElement>(container, 'button[aria-label="Align right"]')!.click();
    expect(commit).toHaveBeenCalled();
    expect(engine.updateTextData).toHaveBeenCalledWith(
      "t1",
      expect.objectContaining({ align: "right" }),
    );
    expect(commit.mock.invocationCallOrder[0]).toBeLessThan(engine.updateTextData.mock.invocationCallOrder[0]);
    cleanup();
  });

  it("edit mode: clicking the already-active control does not push a ghost commit", () => {
    const layer = makeTextLayer({ id: "t2", textData: { ...baseData, align: "left" } });
    const { engine, commit } = buildMock({}, layer);
    const { container, cleanup } = mountTextBar();

    const left = qs<HTMLButtonElement>(container, 'button[aria-label="Align left"]')!;
    expect(left.getAttribute("aria-pressed")).toBe("true");
    left.click();
    expect(commit).not.toHaveBeenCalled();
    expect(engine.updateTextData).not.toHaveBeenCalled();
    cleanup();
  });

  it("edit mode: font/size/color edits route through applyEdit (updateTextData)", () => {
    const layer = makeTextLayer({ id: "t3" });
    const { engine } = buildMock({}, layer);
    const { container, cleanup } = mountTextBar();

    const size = qs<HTMLInputElement>(container, 'input[aria-label="Font size"]')!;
    size.value = "96";
    size.dispatchEvent(new Event("input", { bubbles: true }));
    expect(engine.updateTextData).toHaveBeenCalledWith(
      "t3",
      expect.objectContaining({ fontSize: 96 }),
    );

    const color = qs<HTMLInputElement>(container, 'input[aria-label="Text color"]')!;
    color.value = "#0000ff";
    color.dispatchEvent(new Event("input", { bubbles: true }));
    expect(engine.updateTextData).toHaveBeenLastCalledWith(
      "t3",
      expect.objectContaining({ color: "#0000ff" }),
    );
    cleanup();
  });
});

describe("OptionBar text mount gating", () => {
  it("shows TextOptionBar when the text tool is active", () => {
    mockUseEditor(optionBarEditor({ activeTool: () => "text", selectedLayerId: () => null }) as any);
    const { container, cleanup } = mountOptionBar();
    expect(container.querySelector("[data-text-option-bar]")).not.toBeNull();
    cleanup();
  });

  it("shows TextOptionBar when a text layer is selected (tool may be move)", () => {
    const layer = makeTextLayer();
    mockUseEditor(optionBarEditor({ activeTool: () => "move" }, layer) as any);
    const { container, cleanup } = mountOptionBar();
    expect(container.querySelector("[data-text-option-bar]")).not.toBeNull();
    cleanup();
  });

  it("hides TextOptionBar when move tool is active and no text is selected", () => {
    mockUseEditor(optionBarEditor({ activeTool: () => "move" }) as any);
    const { container, cleanup } = mountOptionBar();
    expect(container.querySelector("[data-text-option-bar]")).toBeNull();
    cleanup();
  });

  it("hides TextOptionBar when a raster layer is selected (not a text layer)", () => {
    const raster = makeRasterLayer();
    mockUseEditor(optionBarEditor({ activeTool: () => "move" }, raster) as any);
    const { container, cleanup } = mountOptionBar();
    expect(container.querySelector("[data-text-option-bar]")).toBeNull();
    cleanup();
  });

  it("does not crash and hides the bar when the engine returns null", () => {
    mockUseEditor(optionBarEditor({ activeTool: () => "move", selectedLayerId: () => "ghost" }) as any);
    const { container, cleanup } = mountOptionBar();
    expect(container.querySelector("[data-text-option-bar]")).toBeNull();
    cleanup();
  });
});
