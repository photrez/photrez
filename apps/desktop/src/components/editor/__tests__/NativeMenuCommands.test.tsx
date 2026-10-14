import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import type { Event } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type { WebGL2Backend } from "@/renderer/webgl2";
import type { RenderScheduler } from "@/renderer/scheduler";
import { WorkspaceManager } from "@/engine/workspace";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import * as bridge from "@/lib/protocol/bridge";
import * as facadeRegistry from "@/lib/protocol/facadeRegistry";
import {
  installEmuBridgeSpies,
  installSeedOnlyTransport,
} from "@/lib/protocol/__tests__/emulatorBridgeRoute";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    isMaximized: async () => false,
    onResized: async () => () => {},
  }),
}));
import { AppTitleBar } from "../shell/AppTitleBar";

const eventMock = vi.hoisted(() => ({
  listener: undefined as ((event: Event<string>) => void) | undefined,
  unlisten: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_eventName: string, listener: (event: Event<string>) => void) => {
    eventMock.listener = listener;
    return eventMock.unlisten;
  }),
}));

function StateProbe() {
  const editor = useEditor();
  return (
    <output
      data-resize-open={String(editor.showResizeDialog())}
      data-export-open={String(editor.showExportDialog())}
    />
  );
}

function tick(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function renderNativeMenuHost(onToggleSidePanels = vi.fn()) {
  const workspace = new WorkspaceManager();
  const renderer = {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    resize: vi.fn(),
    resizeToViewport: vi.fn(),
  } as unknown as WebGL2Backend;
  const scheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;
  const container = document.createElement("div");
  document.body.appendChild(container);
  window.__TAURI_INTERNALS__ = {};

  const dispose = render(
    () => (
      <EditorProvider workspace={workspace} renderer={renderer} scheduler={scheduler}>
        <AppTitleBar isRightDockOpen={false} onToggleRightDock={onToggleSidePanels} />
        <StateProbe />
      </EditorProvider>
    ),
    container,
  );

  return { container, workspace, scheduler, onToggleSidePanels, dispose };
}

function emitNativeMenu(command: string) {
  expect(eventMock.listener).toBeTypeOf("function");
  eventMock.listener?.({ payload: command } as Event<string>);
}

describe("native menu command wiring", () => {
  beforeEach(() => {
    // The retired photrez.facade=0 / photrez.facadeAuthority=wasm pins
    // used to select the emulator arms through the flag-driven dispatch
    // (the wasm opt-out also no-op'd the native seed); production
    // dispatch is native-only now, so the routed commands the menu
    // wires (the shared layer add) reach the same emulator arms
    // explicitly, and the open-path seed the add awaits resolves
    // through the seed-only transport (the shared route below mirrors
    // the Rust predicate).
    installEmuBridgeSpies(bridge);
    installSeedOnlyTransport(vi.mocked(invoke));
  });
  afterEach(() => {
    eventMock.listener = undefined;
    eventMock.unlisten.mockClear();
    delete window.__TAURI_INTERNALS__;
    vi.clearAllMocks();
    // Wipe per-test facade + emulator state: the suites share the doc-1 id, so
    // a facade seeded by the Undo test must not clobber a later test's engine.
    facadeRegistry.__resetFacadeRegistryForTests();
    bridge.__resetEmulatedForTests();
  });

  it("routes a mounted native Undo event through the real editor history", async () => {
    const host = renderNativeMenuHost();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    host.workspace.addDocument(session);
    // Seed the emulator with the document's layer set, then route the layer add
    // through the facade so it records a native entry and claims the layers -
    // the post-collapse undo gate (activeDocumentOwnsFacadeLayers) only fires
    // for a facade-owned document.
    facadeRegistry.__resetFacadeRegistryForTests();
    bridge.__resetEmulatedForTests();
    const facade = facadeRegistry.getFacade("doc-1");
    await facadeRegistry.seedFacadeFromEngine(session.engine as never, facade);
    await facade.addLayer("Layer 2");
    // Refresh the derived history view from the emulator stream so canUndo()
    // reads a non-empty native query (authority = entries.length > 0).
    await session.history.resyncFromNativeQuery();

    emitNativeMenu("edit.undo");
    // The undo path is async behind the facade external handoff; wait for the
    // restore to settle before asserting the layer set.
    await tick();

    expect(session.engine.getLayers()).toHaveLength(1);
    expect(host.scheduler.requestRender).toHaveBeenCalled();
    host.dispose();
  });

  it("routes native dialog and panel commands from the mounted title-bar host", async () => {
    const host = renderNativeMenuHost();
    host.workspace.addDocument(
      WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600),
    );
    await tick();

    emitNativeMenu("image.resize");
    expect(host.container.querySelector("output")).toHaveAttribute("data-resize-open", "true");

    emitNativeMenu("file.export");
    expect(host.container.querySelector("output")).toHaveAttribute("data-export-open", "true");

    emitNativeMenu("view.toggle-side-panels");
    expect(host.onToggleSidePanels).toHaveBeenCalledOnce();
    host.dispose();
  });

  it("routes a native Layer event through the shared layer command path", async () => {
    const host = renderNativeMenuHost();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    host.workspace.addDocument(session);
    await tick();

    emitNativeMenu("layer.new");
    // The routed add funnel settles on a later microtask.
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(session.engine.getLayers()).toHaveLength(2);
    // The routed add owns its undo in the facade, so it writes no TS history
    // entry: the panel/undo surface reads the facade cursor, not this stack.
    expect(host.workspace.getActiveHistory()?.canUndo()).toBe(false);
    expect(host.scheduler.requestRender).toHaveBeenCalled();
    host.dispose();
  });

  it("ignores unknown native menu IDs and removes the listener on cleanup", async () => {
    const host = renderNativeMenuHost();
    await tick();

    emitNativeMenu("unknown.command");
    expect(host.onToggleSidePanels).not.toHaveBeenCalled();

    host.dispose();
    expect(eventMock.unlisten).toHaveBeenCalledOnce();
  });
});
