import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import type { Event } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type { WebGL2Backend } from "@/renderer/webgl2";
import type { RenderScheduler } from "@/renderer/scheduler";
import { WorkspaceManager } from "@/engine/workspace";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import * as bridge from "@/lib/protocol/bridge";
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
  });

  it("routes a mounted native Undo event through the real editor history", async () => {
    const host = renderNativeMenuHost();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    host.workspace.addDocument(session);
    host.workspace.getActiveHistory()!.commit(session.engine.snapshot());
    session.engine.addLayer("Layer 2");
    await tick();
    // The factory's background-flag commit cannot land on the
    // emulator (the emulator never holds the factory's TS-minted
    // layers), and its failure path leaves the pre-check ownership
    // claim behind. The state the failed commit corresponds to is a
    // document the native engine does not hold, so the undo press
    // under test falls through to the TS store - the only history
    // this document actually has.
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();

    emitNativeMenu("edit.undo");

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
