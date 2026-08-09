import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { LayersPanel } from "../LayersPanel";
import { RightDock } from "../../shell/RightDock";
import { WorkspaceManager } from "@/engine/workspace";
import { DEFAULT_TEXT_DATA } from "@/engine/textTypes";
import { stubTextOffscreenCanvas } from "@/__tests__/test-builders";

function installCanvasMocks(bitmap: ImageBitmap) {
  vi.spyOn(HTMLCanvasElement.prototype, "getContext")
    .mockImplementation(((type: string) => {
      if (type !== "2d") return null;
      return {
        clearRect: vi.fn(),
        fillRect: vi.fn(),
        drawImage: vi.fn(),
        strokeRect: vi.fn(),
        save: vi.fn(),
        restore: vi.fn(),
        translate: vi.fn(),
        rotate: vi.fn(),
        scale: vi.fn(),
        beginPath: vi.fn(),
        rect: vi.fn(),
        clip: vi.fn(),
        set fillStyle(_value: string) {},
        set strokeStyle(_value: string) {},
        set lineWidth(_value: number) {},
        set globalAlpha(_value: number) {},
        set globalCompositeOperation(_value: string) {},
      } as unknown as CanvasRenderingContext2D;
    }) as any);

  vi.stubGlobal("OffscreenCanvas", class {
    width: number;
    height: number;

    constructor(width: number, height: number) {
      this.width = width;
      this.height = height;
    }

    getContext() {
      return {
        drawImage: vi.fn(),
        save: vi.fn(),
        restore: vi.fn(),
        translate: vi.fn(),
        rotate: vi.fn(),
        scale: vi.fn(),
        set globalAlpha(_value: number) {},
        set globalCompositeOperation(_value: string) {},
      };
    }

    transferToImageBitmap() {
      return bitmap;
    }
  });
}

function renderLayersPanel(session = WorkspaceManager.createBlankDocument("layers-test", "Layers Test", 800, 600)) {
  const ws = new WorkspaceManager();
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);

  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
        <LayersPanel />
      </EditorProvider>
    ),
    container,
  );

  ws.addDocument(session);

  return {
    ws,
    session,
    renderer,
    scheduler,
    container,
    dispose: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

describe("LayersPanel interactions", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("uploads the newly merged bitmap after Merge Down", async () => {
    const mergedBitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    installCanvasMocks(mergedBitmap);

    const session = WorkspaceManager.createBlankDocument("merge-test", "Merge Test", 800, 600);
    session.engine.addLayer("Top");

    const { renderer, container, dispose } = renderLayersPanel(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    const mergeButton = container.querySelector('button[aria-label="Merge Down"]') as HTMLButtonElement | null;
    if (!mergeButton) throw new Error("Merge Down button was not rendered");

    mergeButton.click();

    const mergedLayer = session.engine.getLayers()[0];
    expect(mergedLayer.imageBitmap).toBe(mergedBitmap);
    expect(renderer.uploadImage).toHaveBeenCalledWith(mergedLayer.id, mergedBitmap);

    dispose();
  });

  it("right-click selects the target layer and duplicates it through the production action", async () => {
    const session = WorkspaceManager.createBlankDocument("context-test", "Context Test", 800, 600);
    session.engine.addLayer("Top Layer");
    const target = session.engine.getLayers()[1];
    const { ws, container, dispose } = renderLayersPanel(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    container.querySelector<HTMLElement>('[data-layer-idx="1"]')!.dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, clientX: 40, clientY: 50 }),
    );
    expect(session.engine.getActiveLayerId()).toBe(target.id);
    const duplicate = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'))
      .find((button) => button.textContent?.includes("Duplicate Layer"))!;
    duplicate.click();

    expect(session.engine.getLayers()).toHaveLength(3);
    expect(ws.getActiveHistory()?.canUndo()).toBe(true);
    dispose();
  });

  it("starts inline rename from the target layer context menu", async () => {
    const { container, dispose } = renderLayersPanel();
    await new Promise((resolve) => setTimeout(resolve, 0));
    container.querySelector<HTMLElement>('[data-layer-idx="0"]')!.dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, clientX: 40, clientY: 50 }),
    );
    const rename = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'))
      .find((button) => button.textContent?.includes("Rename Layer"))!;
    rename.click();

    expect(container.querySelector('input[type="text"]')).not.toBeNull();
    dispose();
  });

  it("bakes the layer adjustment to pixels via the 'Apply Adjustment' context-menu action", async () => {
    const bakedBitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    installCanvasMocks(bakedBitmap);
    const renderer = {
      uploadImage: vi.fn(),
      destroyTexture: vi.fn(),
      bakeLayerToBitmap: vi.fn(() => bakedBitmap),
    };
    const scheduler = { requestRender: vi.fn() };
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("apply-test", "Apply Test", 800, 600);
    ws.addDocument(session);
    session.engine.addLayer("Adjusted");
    // Resolve the adjusted layer by name (addLayer prepends when there is no
    // active layer, and the panel may reverse display order — so array indexing
    // is unreliable). A layer needs rasterized pixels to bake into; give it a
    // placeholder bitmap and simulate the post-adjustment state a real session
    // would hold.
    const adjusted = session.engine.getLayers().find((l) => l.name === "Adjusted")!;
    adjusted.imageBitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    adjusted.basicAdjustment = { brightness: 20, contrast: 0, saturation: 0 };
    adjusted.hasAdjustments = true;

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          <LayersPanel />
        </EditorProvider>
      ),
      container,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Right-click the Adjusted layer's row (found by name, since display order
    // is not guaranteed to match the model array order).
    const adjustedRow = Array.from(container.querySelectorAll<HTMLElement>("[data-layer-idx]"))
      .find((row) => row.textContent?.includes("Adjusted"))!;
    adjustedRow.dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, clientX: 40, clientY: 50 }),
    );
    const applyItem = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'))
      .find((button) => button.textContent?.includes("Apply Adjustment"))!;
    expect(applyItem).toBeTruthy();
    expect(applyItem.disabled).toBe(false);

    applyItem.click();
    // The apply handler is async (PBO readback resolves on a microtask).
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(renderer.bakeLayerToBitmap).toHaveBeenCalledWith(adjusted.id, 800, 600, {
      brightness: 20,
      contrast: 0,
      saturation: 0,
    });
    expect(renderer.uploadImage).toHaveBeenCalledWith(adjusted.id, bakedBitmap);
    expect(ws.getActiveHistory()?.canUndo()).toBe(true);
    // The live adjustment param is cleared after baking.
    expect(session.engine.getLayer(adjusted.id)?.basicAdjustment).toBeUndefined();

    // Undo restores the pre-bake adjustment. `undo` returns the prior snapshot;
    // the app applies it via `engine.restore` (mirrored here).
    const restored = ws.getActiveHistory()?.undo(session.engine.snapshot());
    const restoredLayer = restored?.layers.find((l) => l.name === "Adjusted");
    expect(restoredLayer?.basicAdjustment).toEqual({ brightness: 20, contrast: 0, saturation: 0 });
    session.engine.restore(restored!);
    expect(
      session.engine.getLayers().find((l) => l.name === "Adjusted")?.basicAdjustment,
    ).toEqual({ brightness: 20, contrast: 0, saturation: 0 });

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("disables 'Apply Adjustment' for a layer with no live adjustment", async () => {
    const session = WorkspaceManager.createBlankDocument("apply-disabled", "Apply Disabled", 800, 600);
    session.engine.addLayer("Plain"); // fresh layer: no bitmap, no adjustment
    const plain = session.engine.getLayers()[1];
    expect(plain.hasAdjustments).toBeFalsy();

    const { container, dispose } = renderLayersPanel(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    container.querySelector<HTMLElement>('[data-layer-idx="1"]')!.dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, clientX: 40, clientY: 50 }),
    );
    const applyItem = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'))
      .find((button) => button.textContent?.includes("Apply Adjustment"))!;
    expect(applyItem).toBeTruthy();
    expect(applyItem.disabled).toBe(true);

    dispose();
  });

  it("uploads the newly flattened bitmap after Flatten All Layers", async () => {
    const flattenedBitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    installCanvasMocks(flattenedBitmap);

    const session = WorkspaceManager.createBlankDocument("flatten-test", "Flatten Test", 800, 600);
    session.engine.addLayer("Layer 2");
    session.engine.addLayer("Layer 3");

    const { renderer, container, dispose } = renderLayersPanel(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    const flattenButton = container.querySelector('button[aria-label="Flatten All Layers"]') as HTMLButtonElement | null;
    if (!flattenButton) throw new Error("Flatten All Layers button was not rendered");

    flattenButton.click();

    const flattenedLayer = session.engine.getLayers()[0];
    expect(flattenedLayer.imageBitmap).toBe(flattenedBitmap);
    expect(renderer.uploadImage).toHaveBeenCalledWith(flattenedLayer.id, flattenedBitmap);

    dispose();
  });

  it("commits history before renaming a layer", async () => {
    const bitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    installCanvasMocks(bitmap);

    const { ws, session, container, dispose } = renderLayersPanel();
    await new Promise((resolve) => setTimeout(resolve, 0));

    const layerName = container.querySelector("span.flex-1") as HTMLSpanElement | null;
    if (!layerName) throw new Error("Layer name was not rendered");

    layerName.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 20));

    const input = container.querySelector("input[type='text']") as HTMLInputElement | null;
    if (!input) throw new Error("Layer rename input was not rendered");

    input.value = "Renamed Layer";
    input.dispatchEvent(new InputEvent("input", { bubbles: true }));
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    const history = ws.getActiveHistory();
    expect(history?.canUndo()).toBe(true);

    const previous = history?.undo(session.engine.snapshot());
    expect(previous?.layers[0].name).toBe("Background");

    dispose();
  });

  it("commits history before toggling layer visibility", async () => {
    const bitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    installCanvasMocks(bitmap);

    const { ws, session, container, dispose } = renderLayersPanel();
    await new Promise((resolve) => setTimeout(resolve, 0));

    const visibilityButton = container.querySelector("[data-layer-visibility]") as HTMLButtonElement | null;
    if (!visibilityButton) throw new Error("Layer visibility button was not rendered");

    visibilityButton.click();

    const history = ws.getActiveHistory();
    expect(history?.canUndo()).toBe(true);

    const previous = history?.undo(session.engine.snapshot());
    expect(previous?.layers[0].visible).toBe(true);

    dispose();
  });

  it("commits the pre-drag opacity snapshot once for opacity changes", async () => {
    const bitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    installCanvasMocks(bitmap);

    const { ws, session, container, dispose } = renderLayersPanel();
    await new Promise((resolve) => setTimeout(resolve, 0));

    const opacityToggle = container.querySelector("[data-layer-opacity-toggle]") as HTMLButtonElement | null;
    if (!opacityToggle) throw new Error("Opacity toggle was not rendered");
    opacityToggle.click();

    const opacityInput = container.querySelector("[data-layer-opacity]") as HTMLInputElement | null;
    if (!opacityInput) throw new Error("Opacity slider was not rendered");

    opacityInput.value = "45";
    opacityInput.dispatchEvent(new InputEvent("input", { bubbles: true }));
    opacityInput.dispatchEvent(new Event("change", { bubbles: true }));

    expect(session.engine.getLayers()[0].opacity).toBe(0.45);

    const history = ws.getActiveHistory();
    expect(history?.canUndo()).toBe(true);

    const previous = history?.undo(session.engine.snapshot());
    expect(previous?.layers[0].opacity).toBe(1);

    dispose();
  });

function renderRightDock(session = WorkspaceManager.createBlankDocument("layers-test", "Layers Test", 800, 600)) {
  const ws = new WorkspaceManager();
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);

  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
        <RightDock open={true} onClose={vi.fn()} />
      </EditorProvider>
    ),
    container,
  );

  ws.addDocument(session);

  return {
    ws,
    session,
    renderer,
    scheduler,
    container,
    dispose: () => {
      dispose();
      container.remove();
    },
  };
}

  it("preserves the locked Layers and History tabs and renders history in its tab", async () => {
    const bitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    installCanvasMocks(bitmap);

    const { container, dispose } = renderRightDock();
    await new Promise((resolve) => setTimeout(resolve, 0));

    const tabs = container.querySelectorAll<HTMLButtonElement>("[aria-label='Layer dock tabs'] [role='tab']");
    expect(tabs).toHaveLength(2);
    expect(tabs[0]).toHaveTextContent("Layers");
    expect(tabs[1]).toHaveTextContent("History");
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
    expect(tabs[1]).toHaveAttribute("aria-selected", "false");
    expect(tabs[0]).toHaveClass("after:bg-editor-accent");
    expect(container.querySelector("[data-layers-panel-content]")).not.toBeNull();
    expect(container.querySelector("[data-navigator-panel]")).not.toBeNull();

    tabs[1].click();

    expect(tabs[0]).toHaveAttribute("aria-selected", "false");
    expect(tabs[1]).toHaveAttribute("aria-selected", "true");
    expect(tabs[1]).toHaveClass("after:bg-editor-accent");
    expect(container.querySelector("[data-history-panel]")).not.toBeNull();
    expect(container.querySelector("[data-layers-panel-content]")).toBeNull();
    expect(container.querySelector("[data-navigator-panel]")).not.toBeNull();
    expect(container.textContent).toContain("Open");

    dispose();
  });

  it("shows a T glyph on text layers and double-click opens the edit session (plan §7.3)", async () => {
    const bitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    installCanvasMocks(bitmap);
    stubTextOffscreenCanvas(); // text-capable OffscreenCanvas for addTextLayer rasterization

    const session = WorkspaceManager.createBlankDocument("text-panel", "Text Panel", 800, 600);
    session.engine.addLayer("Raster");
    const textLayerNode = session.engine.addTextLayer("Text", { ...DEFAULT_TEXT_DATA, content: "Hello" });

    const { container, dispose } = renderLayersPanel(session);
    await new Promise((resolve) => setTimeout(resolve, 0));

    const editor = (window as unknown as { __photrezEditor?: ReturnType<typeof useEditor> }).__photrezEditor;
    expect(editor).toBeTruthy();

    // Only the text layer row shows the T glyph.
    const glyphs = container.querySelectorAll("[data-text-layer-glyph]");
    expect(glyphs.length).toBe(1);
    const glyph = glyphs[0] as HTMLElement;

    // Double-click the text layer name → edit session opens (no rename input).
    const names = Array.from(container.querySelectorAll<HTMLElement>("span.flex-1"));
    const textName = names.find((n) => n.textContent === "Text");
    if (!textName) throw new Error("Text layer name was not rendered");
    textName.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(editor!.activeTool()).toBe("text");
    const sessionState = editor!.textEditSession();
    expect(sessionState).not.toBeNull();
    expect(sessionState!.layerId).toBe(textLayerNode.id);
    expect(sessionState!.isNewLayer).toBe(false);
    // No rename input appeared for the text layer.
    expect(container.querySelector("input[type='text']")).toBeNull();

    // Double-click the T glyph (thumbnail wrapper) opens the edit session too.
    // The unchanged re-edit commit is a no-op, so this reopens cleanly on the
    // same layer with no ghost history entry.
    glyph.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 0));
    const sessionState2 = editor!.textEditSession();
    expect(sessionState2).not.toBeNull();
    expect(sessionState2!.layerId).toBe(textLayerNode.id);

    // Double-click a raster layer still opens rename (not the text session).
    const rasterName = names.find((n) => n.textContent === "Raster");
    if (!rasterName) throw new Error("Raster layer name was not rendered");
    rasterName.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(container.querySelector("input[type='text']")).not.toBeNull();

    dispose();
  });
});
