import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { OptionBar } from "../shell/OptionBar";
import { TextOptionBar } from "../TextOptionBar";
import { resetFontCache } from "@/lib/fontEnumeration";
import type { LayerNode } from "@/engine/types";
import type { TextData } from "@/engine/textTypes";

const mockColorPicker = vi.fn().mockResolvedValue("#ff0000");
vi.mock("../dialogs/DialogProvider", () => ({
  useDialog: () => ({
    colorPicker: (opts: any) => {
      opts?.onChange?.("#ff0000");
      return mockColorPicker(opts);
    },
  }),
}));

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
  boxHeight: 0,
  stroke: { width: 0, color: "#000000" },
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
  setTextStrokeWidth: ReturnType<typeof vi.fn>;
  setTextStrokeColor: ReturnType<typeof vi.fn>;
  setFgColor: ReturnType<typeof vi.fn>;
};

function buildMock(overrides: Record<string, unknown> = {}, layer?: LayerNode) {
  const setters: Setters = {
    setTextFontFamily: vi.fn(),
    setTextFontSize: vi.fn(),
    setTextFontWeight: vi.fn(),
    setTextFontItalic: vi.fn(),
    setTextAlign: vi.fn(),
    setTextStrokeWidth: vi.fn(),
    setTextStrokeColor: vi.fn(),
    setFgColor: vi.fn(),
  };
  // Live draw-mode stroke state (Solid signals so `Show when={strokeWidth()>0}`
  // re-renders after the toggle — a static getter would keep the Stroke
  // controls hidden after enabling).
  const [strokeWidth, setStrokeWidthLive] = createSignal(0);
  const [strokeColor, setStrokeColorLive] = createSignal("#000000");
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
    textEditSession: () => null,
    textStrokeWidth: () => (layer ? layer.textData!.stroke?.width ?? 0 : strokeWidth()),
    setTextStrokeWidth: (v: number) => {
      setStrokeWidthLive(v);
      (setters.setTextStrokeWidth as (v: number) => void)(v);
    },
    textStrokeColor: () => (layer ? layer.textData!.stroke?.color ?? "#000000" : strokeColor()),
    setTextStrokeColor: (v: string) => {
      setStrokeColorLive(v);
      (setters.setTextStrokeColor as (v: string) => void)(v);
    },
    colorPickerOpen: () => false,
    setColorPickerOpen: vi.fn(),
    colorPickerTarget: () => "foreground",
    setColorPickerTarget: vi.fn(),
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
    textEditSession: () => null,
    textStrokeWidth: () => 0,
    setTextStrokeWidth: vi.fn(),
    textStrokeColor: () => "#000000",
    setTextStrokeColor: vi.fn(),
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
  it("font dropdown lists fonts immediately (WEB_SAFE placeholder, never empty while loading)", async () => {
    buildMock();
    const { container, cleanup } = mountTextBar();

    qs<HTMLButtonElement>(container, "[data-font-picker-trigger]")!.click();

    // The dropdown must never be empty while the async enumeration is in
    // flight — the instant WEB_SAFE placeholder is rendered synchronously.
    const items = container.querySelectorAll("[data-font-picker] button");
    expect(items.length).toBeGreaterThan(0);
    expect(items[0].textContent).toContain("Arial");

    cleanup();
  });

  it("font picker: ArrowDown + Enter selects the next family (keyboard nav)", async () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();
    await new Promise((r) => setTimeout(r, 0));

    const trigger = qs<HTMLButtonElement>(container, "[data-font-picker-trigger]")!;
    trigger.click();
    const picker = qs(container, "[data-font-picker]")!;
    picker.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    picker.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    expect(setters.setTextFontFamily).toHaveBeenCalledWith("Arial Black");
    cleanup();
  });

  it("font picker: Escape closes the dropdown", async () => {
    buildMock();
    const { container, cleanup } = mountTextBar();
    const trigger = qs<HTMLButtonElement>(container, "[data-font-picker-trigger]")!;
    trigger.click();
    expect(qs(container, "[data-font-picker]")).not.toBeNull();

    qs(container, "[data-font-picker]")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(qs(container, "[data-font-picker]")).toBeNull();
    cleanup();
  });

  it("font picker: search input is auto-focused on open", async () => {
    buildMock();
    const { container, cleanup } = mountTextBar();
    qs<HTMLButtonElement>(container, "[data-font-picker-trigger]")!.click();
    await new Promise((r) => setTimeout(r, 0));

    const search = qs<HTMLInputElement>(container, 'input[aria-label="Search fonts"]')!;
    expect(document.activeElement).toBe(search);
    cleanup();
  });

  it("font picker: search filters the list and highlights the matching substring", async () => {
    buildMock();
    const { container, cleanup } = mountTextBar();
    await new Promise((r) => setTimeout(r, 0));
    qs<HTMLButtonElement>(container, "[data-font-picker-trigger]")!.click();

    const search = qs<HTMLInputElement>(container, 'input[aria-label="Search fonts"]')!;
    search.value = "ar";
    search.dispatchEvent(new Event("input", { bubbles: true }));

    const picker = qs(container, "[data-font-picker]")!;
    const items = picker.querySelectorAll("[role=option]");
    // Only Arial and Arial Black contain "ar".
    expect(items.length).toBe(2);
    const mark = picker.querySelector("mark")!;
    // Highlight shows the original-case substring from the family name.
    expect(mark.textContent!.toLowerCase()).toBe("ar");
    cleanup();
  });

  it("font picker: no match shows a 'No fonts match' empty state", async () => {
    buildMock();
    const { container, cleanup } = mountTextBar();
    qs<HTMLButtonElement>(container, "[data-font-picker-trigger]")!.click();

    const search = qs<HTMLInputElement>(container, 'input[aria-label="Search fonts"]')!;
    search.value = "zzzz";
    search.dispatchEvent(new Event("input", { bubbles: true }));

    expect(qs(container, "[data-font-picker]")!.textContent).toContain("No fonts match");
    expect(container.querySelectorAll("[data-font-picker] [role=option]").length).toBe(0);
    cleanup();
  });

  it("font picker: the selected font is marked with aria-selected and a checkmark", async () => {
    buildMock();
    const { container, cleanup } = mountTextBar();
    await new Promise((r) => setTimeout(r, 0));
    qs<HTMLButtonElement>(container, "[data-font-picker-trigger]")!.click();

    const picker = qs(container, "[data-font-picker]")!;
    const selected = Array.from(picker.querySelectorAll<HTMLElement>("[role=option]")).find(
      (o) => o.getAttribute("aria-selected") === "true",
    );
    expect(selected).toBeDefined();
    expect(selected!.textContent).toContain("Arial"); // current family
    expect(selected!.querySelector("svg")).not.toBeNull(); // check icon
    cleanup();
  });

  it("draw mode: renders font/size/style/align/color controls when no text layer is selected", () => {
    buildMock();
    const { container, cleanup } = mountTextBar();
    expect(qs(container, "[data-font-picker-trigger]")).not.toBeNull();
    expect(qs<HTMLInputElement>(container, 'input[aria-label="Font size"]')).not.toBeNull();
    expect(qs<HTMLButtonElement>(container, 'button[aria-label="Font weight"]')).not.toBeNull();
    expect(qs<HTMLButtonElement>(container, 'button[aria-label="Italic"]')).not.toBeNull();
    expect(qs<HTMLButtonElement>(container, 'button[aria-label="Align left"]')).not.toBeNull();
    expect(qs<HTMLButtonElement>(container, 'button[aria-label="Align center"]')).not.toBeNull();
    expect(qs<HTMLButtonElement>(container, 'button[aria-label="Align right"]')).not.toBeNull();
    expect(qs<HTMLButtonElement>(container, 'button[aria-label="Text color"]')).not.toBeNull();
    cleanup();
  });

  it("draw mode: Font weight popover updates the textFontWeight signal", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();
    const btn = qs<HTMLButtonElement>(container, 'button[aria-label="Font weight"]')!;
    btn.click();
    
    // Find Bold option (700)
    const options = Array.from(container.querySelectorAll("button"));
    const boldBtn = options.find((b) => b.textContent?.trim().startsWith("Bold"));
    expect(boldBtn).toBeDefined();
    boldBtn!.click();
    expect(setters.setTextFontWeight).toHaveBeenCalledWith(700);
    cleanup();
  });

  it("font weight: options show named labels (Regular/Bold/Black) mapped to numeric values", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();
    const btn = qs<HTMLButtonElement>(container, 'button[aria-label="Font weight"]')!;
    btn.click();

    const options = Array.from(container.querySelectorAll("button")).map((b) => b.textContent?.trim());
    expect(options).toContain("Regular");
    expect(options).toContain("Semibold");
    expect(options).toContain("Bold");
    expect(options).toContain("Black");

    const blackBtn = Array.from(container.querySelectorAll("button")).find((b) => b.textContent?.trim().startsWith("Black"));
    blackBtn!.click();
    expect(setters.setTextFontWeight).toHaveBeenCalledWith(900);
    cleanup();
  });

  it("font weight: a custom (non-preset) weight displays its formatted label on the trigger button", () => {
    buildMock({ textFontWeight: () => 650 });
    const { container, cleanup } = mountTextBar();
    const btn = qs<HTMLButtonElement>(container, 'button[aria-label="Font weight"]')!;
    expect(btn.textContent).toContain("650");
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
    const color = qs<HTMLButtonElement>(container, 'button[aria-label="Text color"]')!;
    color.click();
    expect(setters.setFgColor).toHaveBeenCalledWith("#ff0000");
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

    const color = qs<HTMLButtonElement>(container, 'button[aria-label="Text color"]')!;
    color.click();
    expect(engine.updateTextData).toHaveBeenLastCalledWith(
      "t3",
      expect.objectContaining({ color: "#ff0000" }),
    );
    cleanup();
  });

  it("draw mode: stroke toggle enables at 4px then disables at 0", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();

    // Open stroke options popover
    qs<HTMLButtonElement>(container, 'button[aria-label="Toggle stroke options"]')!.click();
    const toggle = qs<HTMLButtonElement>(container, 'button[aria-label="Toggle stroke"]')!;

    toggle.click();
    expect(setters.setTextStrokeWidth).toHaveBeenLastCalledWith(4);
    cleanup();
  });

  it("stroke width: clearing the input does NOT disable the stroke (empty draft reverts on blur)", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();

    qs<HTMLButtonElement>(container, 'button[aria-label="Toggle stroke options"]')!.click();
    qs<HTMLButtonElement>(container, 'button[aria-label="Toggle stroke"]')!.click(); // enable stroke -> 4
    const width = qs<HTMLInputElement>(container, 'input[aria-label="Stroke width"]')!;
    width.value = "";
    width.dispatchEvent(new Event("input", { bubbles: true }));
    expect(setters.setTextStrokeWidth).toHaveBeenLastCalledWith(4);

    width.dispatchEvent(new Event("blur"));
    expect(setters.setTextStrokeWidth).toHaveBeenLastCalledWith(4);
    cleanup();
  });

  it("stroke width: stepper clamps at 1 and never disables via minus", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();

    qs<HTMLButtonElement>(container, 'button[aria-label="Toggle stroke options"]')!.click();
    qs<HTMLButtonElement>(container, 'button[aria-label="Toggle stroke"]')!.click(); // enable stroke -> 4
    const minus = qs<HTMLButtonElement>(container, 'button[aria-label="Decrease stroke width"]')!;
    minus.click(); // 3
    minus.click(); // 2
    minus.click(); // 1
    minus.click(); // clamped at 1

    expect(setters.setTextStrokeWidth).toHaveBeenLastCalledWith(1);
    expect(setters.setTextStrokeWidth.mock.calls.filter((c) => c[0] === 0)).toHaveLength(0);

    qs<HTMLButtonElement>(container, 'button[aria-label="Increase stroke width"]')!.click();
    expect(setters.setTextStrokeWidth).toHaveBeenLastCalledWith(2);
    cleanup();
  });

  it("font size: clearing the input reverts instead of snapping to the clamp", async () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();
    const size = qs<HTMLInputElement>(container, 'input[aria-label="Font size"]')!;

    size.value = "";
    size.dispatchEvent(new Event("input", { bubbles: true }));
    expect(setters.setTextFontSize).not.toHaveBeenCalled();

    size.dispatchEvent(new Event("blur"));
    await new Promise((r) => setTimeout(r, 0));
    expect(size.value).toBe("48"); // reverts to the current value
    cleanup();
  });

  it("stroke width: clear-then-retype commits the new value (draft → 8, no ghost 0)", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();

    qs<HTMLButtonElement>(container, 'button[aria-label="Toggle stroke options"]')!.click();
    qs<HTMLButtonElement>(container, 'button[aria-label="Toggle stroke"]')!.click(); // enable stroke -> 4
    const width = qs<HTMLInputElement>(container, 'input[aria-label="Stroke width"]')!;
    width.value = "";
    width.dispatchEvent(new Event("input", { bubbles: true }));
    width.value = "8";
    width.dispatchEvent(new Event("input", { bubbles: true }));

    // The exact retype flow from the complaint: empty draft transitions into
    // a committed 8 — never a clamp snap or a ghost 0 (stroke stays on).
    expect(setters.setTextStrokeWidth).toHaveBeenLastCalledWith(8);
    expect(setters.setTextStrokeWidth.mock.calls.filter((c) => c[0] === 0)).toHaveLength(0);
    cleanup();
  });

  it("draw mode: stroke width input and stroke color route to session signals", () => {
    const { setters } = buildMock();
    const { container, cleanup } = mountTextBar();

    // Open stroke flyout & enable stroke.
    qs<HTMLButtonElement>(container, 'button[aria-label="Toggle stroke options"]')!.click();
    qs<HTMLButtonElement>(container, 'button[aria-label="Toggle stroke"]')!.click();

    const width = qs<HTMLInputElement>(container, 'input[aria-label="Stroke width"]')!;
    width.value = "8";
    width.dispatchEvent(new Event("input", { bubbles: true }));
    expect(setters.setTextStrokeWidth).toHaveBeenLastCalledWith(8);

    const strokeColor = qs<HTMLButtonElement>(container, 'button[aria-label="Stroke color"]')!;
    strokeColor.click();
    expect(setters.setTextStrokeColor).toHaveBeenLastCalledWith("#ff0000");
    cleanup();
  });

  it("edit mode: stroke patch routes through updateTextData on the layer", () => {
    const layer = makeTextLayer({ id: "t4", textData: { ...baseData, stroke: { width: 4, color: "#000000" } } });
    const { engine } = buildMock({}, layer);
    const { container, cleanup } = mountTextBar();

    qs<HTMLButtonElement>(container, 'button[aria-label="Toggle stroke options"]')!.click();
    const width = qs<HTMLInputElement>(container, 'input[aria-label="Stroke width"]')!;
    width.value = "8";
    width.dispatchEvent(new Event("input", { bubbles: true }));

    // applyEdit merges stroke with the existing stroke (width 8, color kept).
    expect(engine.updateTextData).toHaveBeenLastCalledWith(
      "t4",
      expect.objectContaining({ stroke: expect.objectContaining({ width: 8, color: "#000000" }) }),
    );
    cleanup();
  });

  it("edit mode: no-op stroke press on an already-stroked layer commits nothing", () => {
    const layer = makeTextLayer({ id: "t5", textData: { ...baseData, stroke: { width: 4, color: "#ff0000" } } });
    const { engine } = buildMock({}, layer);
    const { container, cleanup } = mountTextBar();

    qs<HTMLButtonElement>(container, 'button[aria-label="Toggle stroke options"]')!.click();
    const toggle = qs<HTMLButtonElement>(container, 'button[aria-label="Toggle stroke"]')!;
    toggle.click();
    expect(engine.updateTextData).toHaveBeenCalledWith(
      "t5",
      expect.objectContaining({ stroke: expect.objectContaining({ width: 0, color: "#ff0000" }) }),
    );
    cleanup();
  });

  it("edit mode with an OPEN session on the same layer skips the history commit (B4)", () => {
    const layer = makeTextLayer({ id: "t6" });
    const { engine, commit } = buildMock(
      {
        textEditSession: () => ({
          layerId: "t6", docX: 0, docY: 0, boxMode: "point", boxWidth: 0,
          isNewLayer: false, preSnapshot: {},
        }),
      },
      layer,
    );
    const { container, cleanup } = mountTextBar();

    qs<HTMLButtonElement>(container, 'button[aria-label="Align right"]')!.click();
    // Live-mutated with NO history entry — the session's own commit at close
    // produces the single "Edit Text" undo step (one per session contract).
    expect(engine.updateTextData).toHaveBeenCalledWith(
      "t6",
      expect.objectContaining({ align: "right" }),
    );
    expect(commit).not.toHaveBeenCalled();
    cleanup();
  });

  it("edit mode with a session open on a DIFFERENT layer still commits (per-layer guard)", () => {
    const layer = makeTextLayer({ id: "t7" });
    const { engine, commit } = buildMock(
      {
        textEditSession: () => ({
          layerId: "other-layer", docX: 0, docY: 0, boxMode: "point", boxWidth: 0,
          isNewLayer: false, preSnapshot: {},
        }),
      },
      layer,
    );
    const { container, cleanup } = mountTextBar();

    qs<HTMLButtonElement>(container, 'button[aria-label="Align right"]')!.click();
    expect(commit).toHaveBeenCalledTimes(1);
    expect(engine.updateTextData).toHaveBeenCalledWith(
      "t7",
      expect.objectContaining({ align: "right" }),
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

  it("hides TextOptionBar when move tool is active even if a text layer is selected", () => {
    const layer = makeTextLayer();
    mockUseEditor(optionBarEditor({ activeTool: () => "move" }, layer) as any);
    const { container, cleanup } = mountOptionBar();
    expect(container.querySelector("[data-text-option-bar]")).toBeNull();
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
