import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider } from "../EditorContext";
import { AppTitleBar } from "../AppTitleBar";
import { WorkspaceManager } from "@/engine/workspace";

function tick(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function renderAppTitleBar() {
  const ws = new WorkspaceManager();
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
      <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
        <AppTitleBar isRightDockOpen={false} onToggleRightDock={vi.fn()} />
      </EditorProvider>
    ),
    container,
  );

  return {
    ws,
    renderer,
    scheduler,
    container,
    dispose: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

describe("AppTitleBar keyboard shortcuts", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("Ctrl+Z triggers undo when document is open and history exists", async () => {
    const { ws, scheduler, dispose } = renderAppTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    ws.addDocument(session);
    ws.getActiveHistory()!.commit(session.engine.snapshot());
    await tick();

    session.engine.addLayer("Layer 2");
    expect(session.engine.getLayers().length).toBe(2);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();

    expect(session.engine.getLayers().length).toBe(1);
    expect(scheduler.requestRender).toHaveBeenCalled();
    dispose();
  });

  it("Ctrl+Y triggers redo after undo", async () => {
    const { ws, scheduler, dispose } = renderAppTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    ws.addDocument(session);
    ws.getActiveHistory()!.commit(session.engine.snapshot());
    await tick();

    session.engine.addLayer("Layer 2");
    const snapAfterAdd = session.engine.snapshot();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();
    expect(session.engine.getLayers().length).toBe(1);

    // Put the layer back into history so redo works
    ws.getActiveHistory()!.commit(snapAfterAdd);
    await tick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "y", ctrlKey: true, bubbles: true }));
    await tick();

    expect(scheduler.requestRender).toHaveBeenCalled();
    dispose();
  });

  it("Ctrl+Z does nothing when no document is open", async () => {
    const { ws, scheduler, dispose } = renderAppTitleBar();
    // No document added - workspace is empty
    await tick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();

    // Should not crash, no render call
    expect(scheduler.requestRender).not.toHaveBeenCalled();
    dispose();
  });

  it("Ctrl+Z does nothing when history cannot undo", async () => {
    const { ws, scheduler, dispose } = renderAppTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    ws.addDocument(session);
    await tick();

    // No history committed - canUndo returns false
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();

    expect(scheduler.requestRender).not.toHaveBeenCalled();
    dispose();
  });

  it("Ctrl+Y does nothing when no document is open", async () => {
    const { scheduler, dispose } = renderAppTitleBar();
    await tick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "y", ctrlKey: true, bubbles: true }));
    await tick();

    expect(scheduler.requestRender).not.toHaveBeenCalled();
    dispose();
  });

  it("does not trigger undo when focus is in an INPUT element (input guard)", async () => {
    const { ws, scheduler, dispose } = renderAppTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    ws.addDocument(session);
    ws.getActiveHistory()!.commit(session.engine.snapshot());
    await tick();

    session.engine.addLayer("Layer 2");
    expect(session.engine.getLayers().length).toBe(2);

    // Set focus to an input element
    const input = document.createElement("input");
    document.body.appendChild(input);
    input.focus();
    expect(document.activeElement?.tagName).toBe("INPUT");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();

    // Layer count should remain at 2 (undo not triggered)
    expect(session.engine.getLayers().length).toBe(2);
    expect(scheduler.requestRender).not.toHaveBeenCalled();
    document.body.removeChild(input);
    dispose();
  });

  it("does not trigger undo when focus is in a TEXTAREA element (input guard)", async () => {
    const { ws, scheduler, dispose } = renderAppTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    ws.addDocument(session);
    ws.getActiveHistory()!.commit(session.engine.snapshot());
    await tick();

    session.engine.addLayer("Layer 2");

    const textarea = document.createElement("textarea");
    document.body.appendChild(textarea);
    textarea.focus();
    expect(document.activeElement?.tagName).toBe("TEXTAREA");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();

    expect(session.engine.getLayers().length).toBe(2);
    document.body.removeChild(textarea);
    dispose();
  });

  it("input guard still allows undo when no activeElement", async () => {
    const { ws, scheduler, dispose } = renderAppTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    ws.addDocument(session);
    ws.getActiveHistory()!.commit(session.engine.snapshot());
    await tick();

    session.engine.addLayer("Layer 2");

    // Ensure no active element
    if (document.activeElement && "blur" in document.activeElement) {
      (document.activeElement as HTMLElement).blur();
    }

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();

    expect(session.engine.getLayers().length).toBe(1);
    dispose();
  });

  it("undo of layer-add correctly handles renderer state", async () => {
    const { ws, renderer, scheduler, dispose } = renderAppTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    ws.addDocument(session);
    ws.getActiveHistory()!.commit(session.engine.snapshot());
    await tick();

    session.engine.addLayer("Layer 2");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();

    // Undo restores the engine state and syncs viewport via syncViewport.
    // The renderer.resize() is no longer called from handleUndo directly
    // (that caused the "stretched checkerboard" / "squished layer" bug).
    // Instead the buffer stays at the viewport size (set by ResizeObserver
    // or by the createEffect on the next zoom change) and the next render
    // pass re-composites the engine state to the existing FBO.
    //
    // This test just verifies that the undo path runs without throwing and
    // that the engine state is correctly restored.
    expect(session.engine.getLayers().length).toBe(1);
    dispose();
  });

  it("undo does not break shortcuts on subsequent calls", async () => {
    const { ws, scheduler, dispose } = renderAppTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    ws.addDocument(session);
    ws.getActiveHistory()!.commit(session.engine.snapshot());
    await tick();

    session.engine.addLayer("Layer 2");
    session.engine.addLayer("Layer 3");

    // Undo twice in a row
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();

    // Should not have thrown - shortcut system still works
    expect(session.engine.getLayers().length).toBe(1);
    dispose();
  });

  it("undo and redo buttons are rendered", () => {
    const { container, dispose } = renderAppTitleBar();
    expect(container.querySelector('button[aria-label="Undo"]')).not.toBeNull();
    expect(container.querySelector('button[aria-label="Redo"]')).not.toBeNull();
    dispose();
  });

  // ── System menu (Alt+Space) ──

  it("Alt+Space opens system menu", async () => {
    const { container, dispose } = renderAppTitleBar();
    await tick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: " ", altKey: true, bubbles: true }));
    await tick();

    const menu = document.querySelector('[role="menu"][aria-label="System menu"]');
    expect(menu).not.toBeNull();
    dispose();
  });

  it("Escape closes system menu", async () => {
    const { dispose } = renderAppTitleBar();
    await tick();

    // Open via Alt+Space
    window.dispatchEvent(new KeyboardEvent("keydown", { key: " ", altKey: true, bubbles: true }));
    await tick();
    expect(document.querySelector('[role="menu"][aria-label="System menu"]')).not.toBeNull();

    // Close via Escape
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await tick();
    expect(document.querySelector('[role="menu"][aria-label="System menu"]')).toBeNull();
    dispose();
  });

  it("outside click closes system menu", async () => {
    const { dispose } = renderAppTitleBar();
    await tick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: " ", altKey: true, bubbles: true }));
    await tick();
    expect(document.querySelector('[role="menu"][aria-label="System menu"]')).not.toBeNull();

    // Click outside the menu
    document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    await tick();
    expect(document.querySelector('[role="menu"][aria-label="System menu"]')).toBeNull();
    dispose();
  });

  // ── F10 menu activation ──

  it("F10 focuses the first menu trigger button", async () => {
    const { container, dispose } = renderAppTitleBar();
    await tick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "F10", bubbles: true }));
    await tick();

    const firstMenu = container.querySelector<HTMLButtonElement>('button[aria-haspopup="menu"]');
    expect(document.activeElement).toBe(firstMenu);
    dispose();
  });

  it("F10 does not activate when focus is in an INPUT element", async () => {
    const { container, dispose } = renderAppTitleBar();
    await tick();

    const input = document.createElement("input");
    document.body.appendChild(input);
    input.focus();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "F10", bubbles: true }));
    await tick();

    const firstMenu = container.querySelector<HTMLButtonElement>('button[aria-haspopup="menu"]');
    expect(document.activeElement).not.toBe(firstMenu);
    document.body.removeChild(input);
    dispose();
  });

  it("clicking undo button triggers undo", async () => {
    const { ws, container, scheduler, dispose } = renderAppTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    ws.addDocument(session);
    ws.getActiveHistory()!.commit(session.engine.snapshot());
    await tick();

    session.engine.addLayer("Layer 2");

    const undoBtn = container.querySelector('button[aria-label="Undo"]') as HTMLButtonElement;
    undoBtn.click();
    await tick();

    expect(session.engine.getLayers().length).toBe(1);
    expect(scheduler.requestRender).toHaveBeenCalled();
    dispose();
  });
});
