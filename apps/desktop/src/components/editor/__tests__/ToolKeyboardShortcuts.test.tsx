import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider } from "../shell/EditorContext";
import { useEditor } from "../shell/EditorContext";
import { useCanvasKeyboard } from "../canvas/useCanvasKeyboard";
import { clearRegistry } from "../keyboardRegistry";
import { WorkspaceManager } from "@/engine/workspace";

function KeyboardHarness(props: { captureEditor: (editor: ReturnType<typeof useEditor>) => void }) {
  const editor = useEditor();
  props.captureEditor(editor);
  useCanvasKeyboard({
    isSpacePressed: () => false,
    setIsSpacePressed: vi.fn(),
    isAltPressed: () => false,
    setIsAltPressed: vi.fn(),
    isPanning: () => false,
    setIsPanning: vi.fn(),
    stopMomentum: vi.fn(),
    fitToScreenAndRender: vi.fn(),
    syncViewport: vi.fn(),
    getCanvasContainerRef: () => undefined,
  });
  return null;
}

describe("Tool Keyboard Shortcuts", () => {
  afterEach(() => {
    clearRegistry();
  });

  it("should switch to move, selection, crop, and eyedropper on keypress", () => {
    let capturedEditor!: ReturnType<typeof useEditor>;
    const ws = new WorkspaceManager();
    const doc = WorkspaceManager.createBlankDocument("test-doc", "Test Doc", 800, 600);
    ws.addDocument(doc);

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={{ uploadImage: vi.fn(), destroyTexture: vi.fn() } as any} scheduler={{ requestRender: vi.fn() } as any}>
          <KeyboardHarness captureEditor={(e) => { capturedEditor = e; }} />
        </EditorProvider>
      ),
      container
    );

    // Initial tool is move
    expect(capturedEditor.activeTool()).toBe("move");

    // Press b to switch to brush
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "b" }));
    expect(capturedEditor.activeTool()).toBe("brush");

    // Press v to switch to move
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "v" }));
    expect(capturedEditor.activeTool()).toBe("move");

    // Press m to switch to selection
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "m" }));
    expect(capturedEditor.activeTool()).toBe("selection");

    // Press c to switch to crop
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "c" }));
    expect(capturedEditor.activeTool()).toBe("crop");

    // Press i to switch to eyedropper
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "i" }));
    expect(capturedEditor.activeTool()).toBe("eyedropper");

    dispose();
    container.remove();
  });

  it("should switch to shape on u and text on t", () => {
    let capturedEditor!: ReturnType<typeof useEditor>;
    const ws = new WorkspaceManager();
    const doc = WorkspaceManager.createBlankDocument("test-doc2", "Test Doc", 800, 600);
    ws.addDocument(doc);

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={{ uploadImage: vi.fn(), destroyTexture: vi.fn() } as any} scheduler={{ requestRender: vi.fn() } as any}>
          <KeyboardHarness captureEditor={(e) => { capturedEditor = e; }} />
        </EditorProvider>
      ),
      container
    );

    // Press u to switch to shape
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "u" }));
    expect(capturedEditor.activeTool()).toBe("shape");

    // Press t to switch to text
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "t" }));
    expect(capturedEditor.activeTool()).toBe("text");

    dispose();
    container.remove();
  });
});
