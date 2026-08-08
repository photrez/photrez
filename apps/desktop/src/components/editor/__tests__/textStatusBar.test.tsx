// apps/desktop/src/components/editor/__tests__/textStatusBar.test.tsx
//
// E2E-style wiring (plan Task 10): the status bar must advertise the text
// tool when it is active — "Text Tool" label plus the click-to-type hint
// (plan §8.8: "Text Tool — click to type, drag for area box").
//
// Rendered through the REAL EditorProvider (no context mocks) so the tool
// switch uses the same signals the app does.

import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { BottomStatusBar } from "../shell/BottomStatusBar";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { WorkspaceManager } from "@/engine/workspace";

// OffscreenCanvas not available in jsdom; createBlankDocument needs it.
if (typeof OffscreenCanvas === "undefined") {
  (globalThis as any).OffscreenCanvas = class MockOffscreenCanvas {
    width = 0;
    height = 0;
    getContext() {
      return null;
    }
    transferToImageBitmap() {
      return null;
    }
    convertToBlob() {
      return Promise.resolve(new Blob());
    }
  };
}

function setup() {
  const workspace = new WorkspaceManager();
  workspace.addDocument(WorkspaceManager.createBlankDocument("doc-sb", "Status Bar", 800, 600));
  const renderer = {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    resize: vi.fn(),
    resizeToViewport: vi.fn(),
  };
  const scheduler = { requestRender: vi.fn(), getFrameMetrics: () => null, resetFrameMetrics: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);

  // Capture the real editor context so tests can switch tools through the
  // same setter the toolbar uses.
  let editorRef: any = null;
  const EditorConsumer = () => {
    editorRef = useEditor();
    return null;
  };

  const dispose = render(
    () => (
      <EditorProvider workspace={workspace} renderer={renderer as any} scheduler={scheduler as any}>
        <EditorConsumer />
        <BottomStatusBar />
      </EditorProvider>
    ),
    container,
  );
  return {
    container,
    getEditor: () => editorRef,
    dispose: () => {
      dispose();
      container.remove();
    },
  };
}

describe("status bar text tool wiring", () => {
  afterEach(() => {
    document.body.replaceChildren();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("shows 'Text Tool' as the active tool label", () => {
    const { container, getEditor, dispose } = setup();
    getEditor().setActiveTool("text");
    expect(container.textContent).toContain("Text Tool");
    dispose();
  });

  it("advertises click-to-type / drag-for-area-box hint while text tool is active", () => {
    const { container, getEditor, dispose } = setup();
    getEditor().setActiveTool("text");
    expect(container.textContent).toContain("Click to type, drag for a text box. Ctrl+Enter to commit.");
    dispose();
  });

  it("hides the text hint after switching away to the move tool", () => {
    const { container, getEditor, dispose } = setup();
    getEditor().setActiveTool("text");
    expect(container.textContent).toContain("Text Tool");
    getEditor().setActiveTool("move");
    expect(container.textContent).not.toContain("Click to type, drag for a text box");
    dispose();
  });
});
