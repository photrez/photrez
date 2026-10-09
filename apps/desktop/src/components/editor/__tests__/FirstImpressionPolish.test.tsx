import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { BottomStatusBar } from "../shell/BottomStatusBar";
import { EditorProvider } from "../shell/EditorContext";
import { EmptyWorkspace } from "../shell/EmptyWorkspace";
import { RightDock } from "../shell/RightDock";
import { WorkspaceManager } from "@/engine/workspace";

import { DialogProvider } from "../dialogs/DialogProvider";

// A blank document opened from a preset seeds the native engine through the
// factory's background-flag commit, and the open refuses a document whose seed
// was refused - so the boundary has to answer for this file to exercise the
// preset buttons rather than a refused open.
const { nativeProtocolEmulator } = await vi.hoisted(async () => {
  const { createNativeProtocolEmulator } = await import(
    "@/engine/__tests__/nativeProtocolEmulator"
  );
  return { nativeProtocolEmulator: createNativeProtocolEmulator() };
});
vi.mock("@/lib/protocol/nativeClient", () => ({
  nativeProtocol: nativeProtocolEmulator.nativeProtocol,
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "rust_pixels_open_document") return undefined;
    throw `emulator: unhandled command ${cmd}`;
  }),
}));
vi.mock("@/lib/protocol/bridge_emu", () => ({
  __resetEmulatedForTests: vi.fn(),
  resetWasmDoc: vi.fn(),
  getHistoryQuery: vi.fn(async () => ({ entries: [], cursor: 0 })),
}));

// OffscreenCanvas not available in jsdom; tests that call createBlankDocument need it
if (typeof OffscreenCanvas === "undefined") {
  (globalThis as any).OffscreenCanvas = class MockOffscreenCanvas {
    width = 0; height = 0;
    getContext() { return null; }
    transferToImageBitmap() { return null; }
    convertToBlob() { return Promise.resolve(new Blob()); }
  };
}

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
        <DialogProvider>
          {renderChildren()}
        </DialogProvider>
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

describe("first impression polish", () => {
  afterEach(() => {
    document.body.replaceChildren();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("creates a real blank document from the empty workspace presets", async () => {
    vi.stubGlobal("crypto", {
      ...globalThis.crypto,
      randomUUID: vi.fn(() => "empty-workspace-test"),
    });
    const { container, workspace, scheduler, dispose } = renderWithEditor(() => <EmptyWorkspace />);

    expect(container.textContent).toContain("Start a Photrez document");
    expect(container.textContent).not.toContain("portrait-retouch");
    expect(container.textContent).not.toContain("brand-poster");

    const newDocBtn = Array.from(container.querySelectorAll("button")).find((button) =>
      button.textContent?.includes("New Document"),
    );
    if (!newDocBtn) throw new Error("New Document button was not rendered");

    newDocBtn.click();
    await Promise.resolve(); // flush microtasks to mount the dialog

    const createBtn = Array.from(document.querySelectorAll("button")).find((btn) => btn.textContent === "Create");
    if (!createBtn) throw new Error("Create button not found in dialog");
    createBtn.click();

    // One macrotask, not one microtask: the open now awaits the new document's
    // native seed before installing it, so the document lands a tick later.
    await new Promise((r) => setTimeout(r, 0));

    const engine = workspace.getActiveEngine();
    expect(workspace.getDocumentCount()).toBe(1);
    expect(engine?.getWidth()).toBe(1080);
    expect(engine?.getHeight()).toBe(1080);
    expect(scheduler.requestRender).toHaveBeenCalled();
    dispose();
  });

  it("keeps the status bar limited to wired actions", () => {
    const workspace = new WorkspaceManager();
    workspace.addDocument(WorkspaceManager.createBlankDocument("status-polish", "Status Polish", 800, 600));
    const { container, dispose } = renderWithEditor(() => <BottomStatusBar />, workspace);

    expect(container.querySelector("[data-status-history-trigger]")).not.toBeNull();
    expect(container.textContent).toContain("History");
    expect(container.textContent).not.toContain("Snapshots");
    expect(container.textContent).not.toContain("Assets");
    dispose();
  });

  it("labels the inspector with flat tabs: Properties, Adjustments, Presets, Layers, History", () => {
    const workspace = new WorkspaceManager();
    workspace.addDocument(WorkspaceManager.createBlankDocument("dock-polish", "Dock Polish", 800, 600));
    const { container, dispose } = renderWithEditor(() => <RightDock open={true} onClose={vi.fn()} />, workspace);

    expect(container.textContent).toContain("Properties");
    expect(container.textContent).toContain("Adjustments");
    expect(container.textContent).toContain("Presets");
    expect(container.textContent).toContain("Layers");
    expect(container.textContent).toContain("History");
    dispose();
  });
});
