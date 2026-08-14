// apps/desktop/src/components/editor/__tests__/textInspectorWiring.test.tsx
import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { PropertiesPanel } from "../PropertiesPanel";
import { WorkspaceManager } from "@/engine/workspace";
import { DEFAULT_TEXT_DATA } from "@/engine/textTypes";

function renderWithSelectedLayer(workspace: WorkspaceManager, layerId: string) {
  const renderer = {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    resize: vi.fn(),
    resizeToViewport: vi.fn(),
  };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);

  const dispose = render(
    () => (
      <EditorProvider workspace={workspace} renderer={renderer as any} scheduler={scheduler as any}>
        <SelectedLayerHarness layerId={layerId} />
      </EditorProvider>
    ),
    container,
  );

  return { container, dispose, renderer, scheduler };
}

function SelectedLayerHarness(props: { layerId: string }) {
  const editor = useEditor();
  editor.setSelectedLayerId(props.layerId);
  return <PropertiesPanel />;
}

describe("PropertiesPanel Typography Section", () => {
  afterEach(() => {
    document.body.replaceChildren();
    vi.restoreAllMocks();
  });

  it("renders Typography section when a text layer is selected", () => {
    const workspace = new WorkspaceManager();
    const doc = WorkspaceManager.createBlankDocument("doc-1", "Doc 1", 100, 100);
    workspace.addDocument(doc);
    const engine = doc.engine;
    const textLayer = engine.addTextLayer("Text", { ...DEFAULT_TEXT_DATA, content: "Hello" });

    const { container, dispose } = renderWithSelectedLayer(workspace, textLayer.id);

    const section = container.querySelector("[data-typography-section]");
    expect(section).not.toBeNull();
    expect(section?.textContent).toContain("Typography");
    dispose();
  });

  it("does NOT render Typography section when a raster layer is selected", () => {
    const workspace = new WorkspaceManager();
    const doc = WorkspaceManager.createBlankDocument("doc-1", "Doc 1", 100, 100);
    workspace.addDocument(doc);
    const rasterLayer = doc.engine.getLayers()[0];

    const { container, dispose } = renderWithSelectedLayer(workspace, rasterLayer.id);

    const section = container.querySelector("[data-typography-section]");
    expect(section).toBeNull();
    dispose();
  });

  it("Italic button toggles fontStyle between normal and italic", () => {
    const workspace = new WorkspaceManager();
    const doc = WorkspaceManager.createBlankDocument("doc-1", "Doc 1", 100, 100);
    workspace.addDocument(doc);
    const textLayer = doc.engine.addTextLayer("Text", { ...DEFAULT_TEXT_DATA, fontStyle: "normal" });

    const { container, dispose } = renderWithSelectedLayer(workspace, textLayer.id);

    const italicBtn = container.querySelector<HTMLButtonElement>("button[aria-label='Italic']");
    expect(italicBtn).not.toBeNull();
    expect(italicBtn?.getAttribute("aria-pressed")).toBe("false");

    italicBtn?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    const updated = doc.engine.getLayer(textLayer.id);
    expect(updated?.textData?.fontStyle).toBe("italic");

    dispose();
  });

  it("Align buttons update text alignment", () => {
    const workspace = new WorkspaceManager();
    const doc = WorkspaceManager.createBlankDocument("doc-1", "Doc 1", 100, 100);
    workspace.addDocument(doc);
    const textLayer = doc.engine.addTextLayer("Text", { ...DEFAULT_TEXT_DATA, align: "left" });

    const { container, dispose } = renderWithSelectedLayer(workspace, textLayer.id);

    const centerBtn = container.querySelector<HTMLButtonElement>("button[aria-label='Align center']");
    expect(centerBtn).not.toBeNull();

    centerBtn?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    const updated = doc.engine.getLayer(textLayer.id);
    expect(updated?.textData?.align).toBe("center");

    dispose();
  });

  it("Box mode dropdown toggles between point and area text", () => {
    const workspace = new WorkspaceManager();
    const doc = WorkspaceManager.createBlankDocument("doc-1", "Doc 1", 100, 100);
    workspace.addDocument(doc);
    const textLayer = doc.engine.addTextLayer("Text", { ...DEFAULT_TEXT_DATA, boxMode: "point", boxWidth: 0 });

    const { container, dispose } = renderWithSelectedLayer(workspace, textLayer.id);

    const trigger = Array.from(container.querySelectorAll("button")).find(
      (b) => b.textContent?.includes("Auto Width (Point)") || b.textContent?.includes("Fixed Box (Area)")
    );
    expect(trigger).not.toBeUndefined();

    trigger!.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    const areaOption = Array.from(container.querySelectorAll("button")).find(
      (b) => b.textContent?.trim() === "Fixed Box (Area)"
    );
    expect(areaOption).not.toBeUndefined();

    areaOption!.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    const updated = doc.engine.getLayer(textLayer.id);
    expect(updated?.textData?.boxMode).toBe("area");
    expect(updated?.textData?.boxWidth).toBeGreaterThan(0);

    dispose();
  });

  it("Text stroke toggle enables and disables outline", () => {
    const workspace = new WorkspaceManager();
    const doc = WorkspaceManager.createBlankDocument("doc-1", "Doc 1", 100, 100);
    workspace.addDocument(doc);
    const textLayer = doc.engine.addTextLayer("Text", { ...DEFAULT_TEXT_DATA, stroke: { width: 0, color: "#000000" } });

    const { container, dispose } = renderWithSelectedLayer(workspace, textLayer.id);

    const strokeBtn = container.querySelector<HTMLButtonElement>("button[aria-label='Toggle stroke']");
    expect(strokeBtn).not.toBeNull();

    strokeBtn?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    const updated = doc.engine.getLayer(textLayer.id);
    expect(updated?.textData?.stroke.width).toBeGreaterThan(0);

    dispose();
  });
});
