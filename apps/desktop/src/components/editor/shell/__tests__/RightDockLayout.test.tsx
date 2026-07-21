import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../EditorContext";
import { RightDock } from "../RightDock";
import { WorkspaceManager } from "@/engine/workspace";
import { useEditorCommands, dispatchEditorCommand } from "../../useEditorCommands";
import { clearRegistry } from "../../keyboardRegistry";

function renderWithEditor(renderChildren: () => any, workspace = new WorkspaceManager()) {
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
        {renderChildren()}
      </EditorProvider>
    ),
    container,
  );

  return {
    container,
    workspace,
    scheduler,
    dispose: () => {
      dispose();
      container.remove();
    },
  };
}

describe("RightDock layout and tab navigation", () => {
  afterEach(() => {
    document.body.replaceChildren();
    vi.restoreAllMocks();
    localStorage.clear();
    clearRegistry();
  });

  it("renders with default values and allows switching top-level and sub-tabs", { timeout: 15000 }, () => {
    const workspace = new WorkspaceManager();
    workspace.addDocument(WorkspaceManager.createBlankDocument("dock-test", "Dock Test", 800, 600));

    const TestComponent = () => {
      const editor = useEditor();
      return (
        <div>
          <button data-testid="set-presets" onClick={() => editor.setInspectorTab("presets")}>Go Presets</button>
          <button data-testid="set-properties" onClick={() => { editor.setInspectorTab("adjust"); editor.setAdjustSubTab("properties"); }}>Go Prop</button>
          <button data-testid="set-adjustments" onClick={() => { editor.setInspectorTab("adjust"); editor.setAdjustSubTab("adjustments"); }}>Go Adjust</button>
          <button data-testid="set-history" onClick={() => editor.setRightDockPanel("history")}>Go History</button>
          <button data-testid="set-layers" onClick={() => editor.setRightDockPanel("layers")}>Go Layers</button>
          <RightDock open={true} onClose={vi.fn()} />
        </div>
      );
    };

    const { container, dispose } = renderWithEditor(() => <TestComponent />, workspace);

    // Verify all flat tabs are rendered
    expect(container.textContent).toContain("Properties");
    expect(container.textContent).toContain("Adjustments");
    expect(container.textContent).toContain("Presets");
    expect(container.textContent).toContain("Layers");
    expect(container.textContent).toContain("History");

    // Click Presets tab
    container.querySelector<HTMLButtonElement>("[data-testid='set-presets']")?.click();
    expect(container.textContent).toContain("Coming soon: save and apply custom filter");

    // Click History tab
    container.querySelector<HTMLButtonElement>("[data-testid='set-history']")?.click();
    expect(container.textContent).toContain("Edits appear here");

    dispose();
  });

  it("supports stacked vs side-by-side dock layouts", () => {
    const workspace = new WorkspaceManager();
    workspace.addDocument(WorkspaceManager.createBlankDocument("dock-layout-test", "Dock Layout Test", 800, 600));

    const LayoutController = () => {
      const editor = useEditor();
      useEditorCommands(() => undefined);
      return (
        <div>
          <button data-testid="toggle-layout" onClick={() => dispatchEditorCommand("view.toggle-right-dock-layout")}>Toggle Layout</button>
          <RightDock open={true} onClose={vi.fn()} />
        </div>
      );
    };

    const { container, dispose } = renderWithEditor(() => <LayoutController />, workspace);

    const aside = container.querySelector("aside");
    expect(aside).not.toBeNull();
    // Default is side-by-side
    expect(aside!.className).toContain("lg:flex-row");
    expect(aside!.className).not.toContain("h-full");

    // Toggle layout to stacked
    container.querySelector<HTMLButtonElement>("[data-testid='toggle-layout']")?.click();
    expect(aside!.className).toContain("flex-col");
    expect(aside!.className).toContain("h-full");
    expect(localStorage.getItem("photrez.rightDockLayout")).toBe("stacked");

    // Toggle layout back to side-by-side
    container.querySelector<HTMLButtonElement>("[data-testid='toggle-layout']")?.click();
    expect(aside!.className).toContain("lg:flex-row");
    expect(localStorage.getItem("photrez.rightDockLayout")).toBe("side-by-side");

    dispose();
  });
});
