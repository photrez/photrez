import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { invoke } from "@tauri-apps/api/core";
import { EditorProvider } from "../EditorContext";
import { AppTitleBar } from "../AppTitleBar";
import { WorkspaceManager } from "@/engine/workspace";
import * as bridge from "@/lib/protocol/bridge";
import {
  installEmuBridgeSpies,
  installSeedOnlyTransport,
} from "@/lib/protocol/__tests__/emulatorBridgeRoute";
import * as facadeRegistry from "@/lib/protocol/facadeRegistry";
import type { WebGL2Backend } from "@/renderer/webgl2";
import type { RenderScheduler } from "@/renderer/scheduler";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    isMaximized: async () => false,
    onResized: async () => () => {},
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(async () => "0.0.0"),
}));

function tick(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function renderAppTitleBar(onToggleSidePanels = vi.fn()) {
  const ws = new WorkspaceManager();
  const renderer = {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    resize: vi.fn(),
    resizeToViewport: vi.fn(),
  } as unknown as WebGL2Backend;
  const scheduler = { requestRender: vi.fn() } as unknown as RenderScheduler & {
    requestRender: ReturnType<typeof vi.fn>;
  };
  const container = document.createElement("div");
  document.body.appendChild(container);

  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
        <AppTitleBar isRightDockOpen={false} onToggleRightDock={onToggleSidePanels} />
      </EditorProvider>
    ),
    container,
  );

  return {
    ws,
    renderer,
    scheduler: scheduler as unknown as { requestRender: ReturnType<typeof vi.fn> },
    container,
    dispose: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

// Undo/redo setup mirrors production: seed the emulator route, route the layer
// add through the facade (entry lands in the native stream, layers become
// facade-owned), then resync the derived view. Post-collapse canUndo()/canRedo()
// only read the native query, so a TS host-stack commit alone no-ops the press.
async function seedTwoLayers(
  session: ReturnType<typeof WorkspaceManager.createBlankDocument>,
): Promise<void> {
  const facade = facadeRegistry.getFacade(session.engine.getId());
  await facadeRegistry.seedFacadeFromEngine(session.engine as never, facade);
  await facade.addLayer("Layer 2");
  await session.history.resyncFromNativeQuery();
}

describe("AppTitleBar keyboard shortcuts", () => {
  beforeEach(() => {
    localStorage.clear();
    // createBlankDocument's background-flag commit is fire-and-forget and never
    // settles on the seed-only transport; it would render (and warn) mid-test
    // after setup. The tests seed the facade explicitly instead.
    localStorage.setItem("photrez.bgFlagRoute", "0");
    facadeRegistry.__resetFacadeRegistryForTests();
    bridge.__resetEmulatedForTests();
    installEmuBridgeSpies(bridge);
    installSeedOnlyTransport(vi.mocked(invoke));
  });

  afterEach(() => {
    document.body.replaceChildren();
    vi.restoreAllMocks();
    vi.clearAllMocks();
    facadeRegistry.__resetFacadeRegistryForTests();
    bridge.__resetEmulatedForTests();
    localStorage.clear();
  });

  it("Ctrl+Z triggers undo when document is open and history exists", async () => {
    const { ws, scheduler, dispose } = renderAppTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    ws.addDocument(session);
    const facade = facadeRegistry.getFacade("doc-1");
    await facadeRegistry.seedFacadeFromEngine(session.engine as never, facade);
    await facade.addLayer("Layer 2");
    await session.history.resyncFromNativeQuery();
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
    const facade = facadeRegistry.getFacade("doc-1");
    await facadeRegistry.seedFacadeFromEngine(session.engine as never, facade);
    await facade.addLayer("Layer 2");
    await session.history.resyncFromNativeQuery();
    expect(session.engine.getLayers().length).toBe(2);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();
    expect(session.engine.getLayers().length).toBe(1);

    // Redo re-applies the facade snapshot; cursor advances back to the tip.
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "y", ctrlKey: true, bubbles: true }));
    await tick();

    expect(session.engine.getLayers().length).toBe(2);
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

    // No facade layer add has committed a native entry, so canUndo() is false.
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
    const facade = facadeRegistry.getFacade("doc-1");
    await facadeRegistry.seedFacadeFromEngine(session.engine as never, facade);
    await facade.addLayer("Layer 2");
    await session.history.resyncFromNativeQuery();
    expect(session.engine.getLayers().length).toBe(2);

    // Set focus to an input element
    const input = document.createElement("input");
    document.body.appendChild(input);
    input.focus();
    expect(document.activeElement?.tagName).toBe("INPUT");

    // Input guard must swallow the keypress before it reaches the undo handoff;
    // capture the setup render count and assert it did not rise.
    const before = scheduler.requestRender.mock.calls.length;

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();

    expect(session.engine.getLayers().length).toBe(2);
    expect(scheduler.requestRender.mock.calls.length).toBe(before);
    document.body.removeChild(input);
    dispose();
  });

  it("does not trigger undo when focus is in a TEXTAREA element (input guard)", async () => {
    const { ws, scheduler, dispose } = renderAppTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    ws.addDocument(session);
    const facade = facadeRegistry.getFacade("doc-1");
    await facadeRegistry.seedFacadeFromEngine(session.engine as never, facade);
    await facade.addLayer("Layer 2");
    expect(session.engine.getLayers().length).toBe(2);

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
    const facade = facadeRegistry.getFacade("doc-1");
    await facadeRegistry.seedFacadeFromEngine(session.engine as never, facade);
    await facade.addLayer("Layer 2");
    await session.history.resyncFromNativeQuery();

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
    const facade = facadeRegistry.getFacade("doc-1");
    await facadeRegistry.seedFacadeFromEngine(session.engine as never, facade);
    await facade.addLayer("Layer 2");
    await session.history.resyncFromNativeQuery();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();

    // Undo restores the engine state and re-syncs the derived view; the
    // layer set drops back to the baseline and a render is requested.
    expect(session.engine.getLayers().length).toBe(1);
    expect(scheduler.requestRender).toHaveBeenCalled();
    dispose();
  });

  it("undo does not break shortcuts on subsequent calls", async () => {
    const { ws, scheduler, dispose } = renderAppTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    ws.addDocument(session);
    const facade = facadeRegistry.getFacade("doc-1");
    await facadeRegistry.seedFacadeFromEngine(session.engine as never, facade);
    await facade.addLayer("Layer 2");
    await facade.addLayer("Layer 3");
    await session.history.resyncFromNativeQuery();
    expect(session.engine.getLayers().length).toBe(3);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    await tick();

    // Two facade layer adds undone back to the baseline; shortcut system intact.
    expect(session.engine.getLayers().length).toBe(1);
    expect(scheduler.requestRender).toHaveBeenCalled();
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
    // Close before dispose so the Portal is unmounted from a live host; avoids
    // a leftover fragment tearing during cleanup when the component disposes.
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await tick();
    dispose();
  });

  it("Escape closes system menu", async () => {
    const { dispose } = renderAppTitleBar();
    await tick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: " ", altKey: true, bubbles: true }));
    await tick();
    expect(document.querySelector('[role="menu"][aria-label="System menu"]')).not.toBeNull();

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
    const facade = facadeRegistry.getFacade("doc-1");
    await facadeRegistry.seedFacadeFromEngine(session.engine as never, facade);
    await facade.addLayer("Layer 2");
    await session.history.resyncFromNativeQuery();

    const undoBtn = container.querySelector('button[aria-label="Undo"]') as HTMLButtonElement;
    undoBtn.click();
    await tick();

    expect(session.engine.getLayers().length).toBe(1);
    expect(scheduler.requestRender).toHaveBeenCalled();
    dispose();
  });
});
