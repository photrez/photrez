// apps/desktop/src/components/editor/__tests__/textEditOverlay.test.tsx
//
// Task 7 — TextEditOverlay DOM contract:
//   - mounts while textEditSession() is non-null, unmounts when null
//   - textarea value syncs to textData.content (new session adopts layer text)
//   - typing pushes a debounced live re-raster (updateTextData → upload)
//   - IME-safe: composition text is not pushed/committed until compositionend
//   - Ctrl+Enter commits (one undo step); Escape cancels (deletes temp layer)
//   - no placeholder (research R1)

import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { TextEditOverlay, OVERLAY_MIN_WIDTH } from "../TextEditOverlay";
import { commitTextSession, flushPendingText } from "../canvas/pointerTools/textTool";
import type { LayerNode } from "@/engine/types";
import type { TextData } from "@/engine/textTypes";
import type { TextEditSession } from "../tools/editorState";

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
  stroke: { width: 0, color: "#000000" },
};

function makeTextLayer(content: string, over: Partial<LayerNode> = {}): LayerNode {
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
    textData: { ...baseData, content, ...(over.textData as Partial<TextData>) },
    ...over,
  };
}

interface MockState {
  setSession: (s: TextEditSession | null) => void;
  engine: Record<string, any>;
  commit: ReturnType<typeof vi.fn>;
  uploadImage: ReturnType<typeof vi.fn>;
  requestRender: ReturnType<typeof vi.fn>;
  layer: LayerNode;
}

function buildMock(initialContent = "Hello") {
  const [session, setSession] = createSignal<TextEditSession | null>(null);
  const layer = makeTextLayer(initialContent);
  const updateTextData = vi.fn((id: string, data: TextData) => {
    // Mutate the shared layer in place so getLayer() reflects the new content.
    layer.textData = { ...layer.textData, ...data };
  });
  const commit = vi.fn();
  const snapshot = vi.fn(() => ({ layers: [] }));
  const deleteLayer = vi.fn();
  const uploadImage = vi.fn();
  const requestRender = vi.fn();
  const getLayerImageBitmap = vi.fn(() => document.createElement("canvas"));

  const engine = {
    getLayer: (id: string) => (id === layer.id ? layer : undefined),
    updateTextData,
    snapshot,
    deleteLayer,
    getLayerImageBitmap,
    setRenderHiddenLayerId: vi.fn(),
  };

  const state: MockState = {
    setSession,
    engine,
    commit,
    uploadImage,
    requestRender,
    layer,
  };

  const editor = {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => ({ commit }),
    },
    renderer: { uploadImage },
    scheduler: { requestRender },
    zoom: () => 1,
    pan: () => ({ x: 10, y: 20 }),
    layers: () => [layer],
    textEditSession: session,
    setTextEditSession: setSession,
  };
  mockUseEditor(editor as any);
  return { state, setSession, engine, commit, layer, uploadImage, requestRender, editor };
}

function mountOverlay() {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(() => <TextEditOverlay />, container);
  const cleanup = () => {
    dispose();
    container.parentNode?.removeChild(container);
  };
  return { container, cleanup };
}

function qs<T extends HTMLElement>(root: HTMLElement, sel: string): T | null {
  return root.querySelector(sel) as T | null;
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("TextEditOverlay", () => {
  it("does not render when no session is open; mounts when a session opens", () => {
    const { setSession, layer } = buildMock();
    const { container, cleanup } = mountOverlay();
    expect(qs(container, "[data-text-edit-overlay]")).toBeNull();

    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });
    expect(qs(container, "[data-text-edit-overlay]")).not.toBeNull();
    cleanup();
  });

  it("unmounts when the session closes (commit/cancel clears the signal)", () => {
    const { setSession, layer } = buildMock();
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });
    expect(qs(container, "[data-text-edit-overlay]")).not.toBeNull();

    setSession(null);
    expect(qs(container, "[data-text-edit-overlay]")).toBeNull();
    cleanup();
  });

  it("hides the edited layer from the compositor while the session is open (no doubled text)", () => {
    const { setSession, layer, engine } = buildMock();
    const { container, cleanup } = mountOverlay();
    // Initial mount effect clears any stale hidden id (session closed).
    expect(engine.setRenderHiddenLayerId).toHaveBeenCalledWith(null);

    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });
    expect(engine.setRenderHiddenLayerId).toHaveBeenLastCalledWith(layer.id);

    setSession(null);
    expect(engine.setRenderHiddenLayerId).toHaveBeenLastCalledWith(null);
    cleanup();
  });

  it("applies -webkit-text-stroke from textData.stroke, scaled by zoom (×2 raster parity)", () => {
    const { setSession, layer } = buildMock();
    layer.textData = { ...layer.textData!, stroke: { width: 3, color: "#00ff00" } };
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "area", boxWidth: 100, isNewLayer: false, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    // The raster draws the stroke at lineWidth = strokePad*2 (fill covers the
    // inner half), so the visible outline is stroke.width doc px → the CSS
    // stroke, also centered on the glyph, must be 2× that to look identical.
    expect(ta.style.webkitTextStroke).toBe("6px #00ff00");
    // Glyph ink starts at raster PADDING + strokePad → overlay padding and box
    // width grow by the same stroke extent (zoom 1: +3px pad, +6px box). Base
    // pad is 1px (border-box: 1px pad + 1px border keeps the box tight).
    expect(ta.style.padding).toBe("4px");
    expect(ta.style.width).toBe("106px");
    cleanup();
  });

  it("stroke width 0 disables the CSS stroke", () => {
    const { setSession, layer } = buildMock();
    // layer already has stroke width 0 (baseData default)
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    expect(ta.style.webkitTextStroke).toBe("0px");
    cleanup();
  });

  it("mirrors the layer's rotation, scale and flip so the box matches the rendered frame", () => {
    const { setSession, layer } = buildMock();
    layer.transform = { x: 0, y: 0, scaleX: 1.5, scaleY: 1, rotation: 45, flipH: true, flipV: false };
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    expect(ta.style.opacity).toBe("1"); // untransformed opacity still mirrored
    expect(ta.style.transform).toBe("rotate(45deg) scale(-1.5, 1)");
    expect(ta.style.transformOrigin).toBe("center");
    cleanup();
  });

  it("mirrors layer opacity and stays untransformed for the identity transform", () => {
    const { setSession, layer } = buildMock();
    layer.opacity = 0.5;
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    expect(ta.style.opacity).toBe("0.5");
    expect(ta.style.transform).toBe("");
    cleanup();
  });

  it("keeps a small min-width so short text boxes are still easy to click", () => {
    const { setSession, layer } = buildMock();
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    // jsdom's cssstyle drops relative units for min-width, so the constant is
    // the contract; the cssText check catches a regression back to 160px.
    expect(OVERLAY_MIN_WIDTH).toBe("2ch");
    expect(ta.style.cssText).not.toContain("min-width: 160px");
    cleanup();
  });

  it("value syncs to the layer content when the session opens", () => {
    const { setSession, layer } = buildMock();
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    expect(ta.value).toBe("Hello");
    cleanup();
  });

  it("typing schedules a debounced live re-raster (updateTextData + upload)", () => {
    vi.useFakeTimers();
    const { setSession, engine, uploadImage } = buildMock("");
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: "text-1", docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    ta.value = "Hello world";
    ta.dispatchEvent(new InputEvent("input", { bubbles: true }));

    // Debounce window: no push yet.
    expect(engine.updateTextData).not.toHaveBeenCalled();

    vi.advanceTimersByTime(50);
    expect(engine.updateTextData).toHaveBeenCalledWith(
      "text-1",
      expect.objectContaining({ content: "Hello world" }),
    );
    expect(uploadImage).toHaveBeenCalledWith("text-1", expect.anything());
    cleanup();
  });

  it("positioning follows doc→screen math (pan + doc*zoom)", () => {
    const { setSession, layer } = buildMock();
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 100, docY: 50, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    expect(ta.style.left).toBe("110px"); // pan.x(10) + 100*1
    expect(ta.style.top).toBe("70px");   // pan.y(20) + 50*1
    cleanup();
  });

  it("IME-safe: composition input does not push until compositionend", () => {
    vi.useFakeTimers();
    const { setSession, engine } = buildMock();
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: "text-1", docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    ta.dispatchEvent(new CompositionEvent("compositionstart"));
    ta.value = "の";
    ta.dispatchEvent(new InputEvent("input", { bubbles: true }));
    ta.value = "のみ";
    ta.dispatchEvent(new InputEvent("input", { bubbles: true }));

    vi.advanceTimersByTime(200);
    expect(engine.updateTextData).not.toHaveBeenCalled();

    ta.dispatchEvent(new CompositionEvent("compositionend", { data: "のみ" }));
    // The composition-settled value is pushed through the same debounce.
    vi.advanceTimersByTime(50);
    expect(engine.updateTextData).toHaveBeenCalledWith(
      "text-1",
      expect.objectContaining({ content: "のみ" }),
    );
    cleanup();
  });

  it("Ctrl+Enter commits the session; Escape cancels and deletes the temp layer", () => {
    const { setSession, engine, commit, layer } = buildMock();
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    ta.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", ctrlKey: true, bubbles: true }));
    // Flushed pending content before commit so the session sees the content.
    expect(engine.updateTextData).toHaveBeenCalledWith("text-1", expect.objectContaining({ content: "Hello" }));
    expect(commit).toHaveBeenCalled();
    // Commit clears the session → overlay unmounts.
    expect(qs(container, "[data-text-edit-overlay]")).toBeNull();

    // Reopen and Escape → cancel path deletes the temp layer.
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });
    const ta2 = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    ta2.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(engine.deleteLayer).toHaveBeenCalledWith(layer.id);
    cleanup();
  });

  it("Escape during IME composition cancels the composition, not the whole session (B7)", () => {
    const { setSession, engine, layer } = buildMock();
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    ta.dispatchEvent(new CompositionEvent("compositionstart"));
    ta.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));

    // The session survives — Escape during composition only cancels the IME
    // candidate, it must not kill the whole edit (no temp-layer delete).
    expect(engine.deleteLayer).not.toHaveBeenCalled();
    expect(qs(container, "[data-text-edit-overlay]")).not.toBeNull();

    ta.dispatchEvent(new CompositionEvent("compositionend", { data: "" }));
    ta.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
    expect(engine.deleteLayer).toHaveBeenCalledWith(layer.id);
    cleanup();
  });

  it("point mode: measured glyph width sets an explicit overlay width so long text does not wrap (B8)", () => {
    // The canvas probe drives the width; without it the textarea would use the
    // browser-default width and wrap a line the committed raster keeps whole.
    const measureText = vi.fn((t: string) => ({ width: t.length * 10 }));
    const getContextSpy = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
      { measureText, font: "" } as unknown as CanvasRenderingContext2D,
    );
    try {
      const { setSession, layer } = buildMock("Hello World");
      const { container, cleanup } = mountOverlay();
      setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: { layers: [] } as any });

      const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
      // "Hello World" = 11 chars × 10 + 8px box = 118px.
      expect(ta.style.width).toBe("118px");
      expect(measureText).toHaveBeenCalledWith("Hello World");
      cleanup();
    } finally {
      getContextSpy.mockRestore();
    }
  });

  it("area mode keeps the box width — no point measurement applied (B8)", () => {
    const getContextSpy = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
      { measureText: vi.fn(() => ({ width: 500 })), font: "" } as unknown as CanvasRenderingContext2D,
    );
    try {
      const { setSession, layer } = buildMock("Hello World");
      const { container, cleanup } = mountOverlay();
      setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "area", boxWidth: 200, isNewLayer: false, preSnapshot: { layers: [] } as any });

      const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
      expect(ta.style.width).toBe("200px");
      cleanup();
    } finally {
      getContextSpy.mockRestore();
    }
  });

  it("has subtle placeholder text 'Type text...' for initial creation affordance", () => {
    const { setSession, layer } = buildMock();
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    expect(ta.getAttribute("placeholder")).toBe("Type text...");
    cleanup();
  });

  it("click-away commit flushes pending debounced content (no data loss, no empty-cleanup delete)", () => {
    vi.useFakeTimers();
    const { setSession, engine, commit, editor } = buildMock("");
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: "text-1", docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    ta.value = "Hello world";
    ta.dispatchEvent(new InputEvent("input", { bubbles: true }));
    expect(engine.updateTextData).not.toHaveBeenCalled(); // still inside debounce

    // Simulate the wiring path: external commit (click-away) BEFORE the
    // debounce fires. The flush registry must push the pending text first.
    // Uses the SAME mock editor (shared session signal + engine) as the overlay.
    commitTextSession(editor as any);

    expect(engine.updateTextData).toHaveBeenCalledWith(
      "text-1",
      expect.objectContaining({ content: "Hello world" }),
    );
    expect(commit).toHaveBeenCalledWith(expect.anything(), "Add Text");
    expect(engine.deleteLayer).not.toHaveBeenCalled();
    cleanup();
  });

  it("flushPendingText(engine) pushes pending content into the PROVIDED engine (doc-switch B2)", () => {
    vi.useFakeTimers();
    const { setSession, engine, layer } = buildMock("");
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    ta.value = "Hello world";
    ta.dispatchEvent(new InputEvent("input", { bubbles: true }));
    expect(engine.updateTextData).not.toHaveBeenCalled(); // still inside debounce

    // The SOURCE engine (doc A) the session belongs to — different from the
    // overlay's active engine. The flush must push there, not to the active
    // engine, or the last debounce-window keystrokes are silently dropped.
    const sourceEngine = {
      getLayer: (id: string) => (id === layer.id ? layer : undefined),
      updateTextData: vi.fn(),
      getLayerImageBitmap: vi.fn(() => document.createElement("canvas")),
    };
    flushPendingText(sourceEngine as any);

    expect(sourceEngine.updateTextData).toHaveBeenCalledWith(
      "text-1",
      expect.objectContaining({ content: "Hello world" }),
    );
    // The active engine never sees the push.
    expect(engine.updateTextData).not.toHaveBeenCalled();
    setSession(null); // clears the flush registry for the next test
    cleanup();
  });

  it("re-editing the same layer in a second session still gets focus+select-all", () => {
    const { setSession, layer } = buildMock();
    const { container, cleanup } = mountOverlay();
    const open = (isNewLayer: boolean) => setSession({
      layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0,
      isNewLayer, preSnapshot: { layers: [] } as any,
    });

    open(true);
    let ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    expect(ta.value).toBe("Hello");
    // Close the session (commit path clears the signal).
    setSession(null);
    expect(qs(container, "[data-text-edit-overlay]")).toBeNull();

    // Second session on the SAME layer: must re-select-all (R3 double-click).
    open(false);
    ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    expect(ta.value).toBe("Hello");
    expect(ta.selectionStart).toBe(0);
    expect(ta.selectionEnd).toBe(5); // "Hello".length
    cleanup();
  });

  it("auto-grows rows so lines past the second stay visible while typing", () => {
    // The layer is hidden from the compositor during a session, so the
    // textarea is the ONLY visible rendering — a fixed default of 2 rows
    // would clip every line beyond the second (invisible typing bug).
    const { setSession, layer } = buildMock("L1\nL2\nL3\nL4");
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    expect(ta.rows).toBeGreaterThanOrEqual(4);
    cleanup();
  });

  it("single-line content stays at the minimum 2 rows", () => {
    const { setSession, layer } = buildMock("Hello");
    const { container, cleanup } = mountOverlay();
    setSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: { layers: [] } as any });

    const ta = qs<HTMLTextAreaElement>(container, "[data-text-edit-overlay]")!;
    expect(ta.rows).toBe(2);
    cleanup();
  });
});
