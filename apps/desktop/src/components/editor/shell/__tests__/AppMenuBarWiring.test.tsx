import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
vi.mock("@tauri-apps/api/app", () => ({ getVersion: () => Promise.resolve("0.1.0") }));
import type { WebGL2Backend } from "@/renderer/webgl2";
import type { RenderScheduler } from "@/renderer/scheduler";
import { WorkspaceManager } from "@/engine/workspace";
import { ViewportCamera } from "@/viewport/viewportCamera";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { EditorProvider, useEditor } from "../EditorContext";
import { DialogProvider } from "../../dialogs/DialogProvider";
import { AppTitleBar } from "../AppTitleBar";

// OffscreenCanvas not available in jsdom; tests that call createBlankDocument need it
if (typeof OffscreenCanvas === "undefined") {
  (globalThis as any).OffscreenCanvas = class MockOffscreenCanvas {
    width = 0; height = 0;
    getContext() { return null; }
    transferToImageBitmap() { return null; }
    convertToBlob() { return Promise.resolve(new Blob()); }
  };
}

function Probe() {
  const editor = useEditor();
  return (
    <output
      data-resize-open={String(editor.showResizeDialog())}
      data-selection-present={String(editor.selection() !== null)}
      data-selection-inverted={String(editor.selection()?.inverted ?? false)}
    />
  );
}

/** Find a button anywhere in the document (popup may be portal-rendered to body) */
function button(_container: HTMLElement, label: string): HTMLButtonElement {
  const match = Array.from(document.querySelectorAll("button"))
    .find((element) => element.getAttribute("aria-label") === label || element.textContent?.trim() === label);
  if (!(match instanceof HTMLButtonElement)) throw new Error(`Button not found: ${label}`);
  return match;
}

function renderTitleBar() {
  const workspace = new WorkspaceManager();
  const renderer = {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    resize: vi.fn(),
    resizeToViewport: vi.fn(),
  } as unknown as WebGL2Backend;
  const scheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;
  const camera = new ViewportCamera();
  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <EditorProvider workspace={workspace} renderer={renderer} scheduler={scheduler} camera={camera}>
        <DialogProvider>
          <AppTitleBar isRightDockOpen={true} onToggleRightDock={vi.fn()} />
          <Probe />
        </DialogProvider>
      </EditorProvider>
    ),
    container,
  );
  return { camera, container, workspace, scheduler, dispose };
}

describe("custom application menu wiring", () => {
  afterEach(() => vi.restoreAllMocks());

  it("creates a real blank document from File > New Document", async () => {
    const host = renderTitleBar();

    button(host.container, "File").click();
    expect(host.workspace.getDocumentCount()).toBe(0);
    button(host.container, "New Document").click();

    await Promise.resolve(); // wait for the dialog to mount

    const createBtn = Array.from(document.querySelectorAll("button")).find((btn) => btn.textContent === "Create");
    if (!createBtn) throw new Error("Create button not found in dialog");
    createBtn.click();

    // The 'file.new' command triggers an async IIFE that awaits the dialog result.
    // A single microtask flush might not be enough, use a real macro-task delay.
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(host.workspace.getDocumentCount()).toBe(1);
    expect(host.scheduler.requestRender).toHaveBeenCalled();
    host.dispose();
  });

  it("opens the real resize state from Image > Resize Canvas", () => {
    const host = renderTitleBar();
    host.workspace.addDocument(
      WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600),
    );

    button(host.container, "Image").click();
    button(host.container, "Resize Canvas…").click();

    expect(host.container.querySelector("output")).toHaveAttribute("data-resize-open", "true");
    host.dispose();
  });

  it("routes View > Actual Size through the real viewport camera", () => {
    const host = renderTitleBar();
    host.workspace.addDocument(
      WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600),
    );
    host.camera.setViewportSize(1000, 700);
    host.camera.setState({ x: 100, y: 80, zoom: 2 });

    button(host.container, "View").click();
    button(host.container, "Actual Size").click();

    // Keyboard zoom shortcuts now use animation, tick to completion
    expect(host.camera.isAnimating()).toBe(true);
    const startTime = performance.now();
    while (host.camera.isAnimating()) {
      host.camera.tick(startTime + 300); // Fast-forward past 200ms duration
    }

    expect(host.camera.getState().zoom).toBeCloseTo(1);
    // Note: scheduler.requestRender is called by camera animation callbacks, not directly
    host.dispose();
  });

  it("routes Edit selection commands through the active document engine", () => {
    const host = renderTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    host.workspace.addDocument(session);

    button(host.container, "Edit").click();
    button(host.container, "Select All").click();
    expect(session.engine.getSelection()).toMatchObject({ x: 0, y: 0, width: 800, height: 600 });
    expect(host.container.querySelector("output")).toHaveAttribute("data-selection-present", "true");

    button(host.container, "Edit").click();
    button(host.container, "Invert Selection").click();
    expect(session.engine.getSelection()).toMatchObject({ inverted: true });
    expect(host.container.querySelector("output")).toHaveAttribute("data-selection-inverted", "true");

    button(host.container, "Edit").click();
    button(host.container, "Deselect").click();
    expect(session.engine.getSelection()).toBeNull();
    expect(host.container.querySelector("output")).toHaveAttribute("data-selection-present", "false");
    expect(host.scheduler.requestRender).toHaveBeenCalled();
    host.dispose();
  });

  it("routes Edit Cut, Copy, and Paste to the production selection operations", () => {
    const copy = vi.spyOn(SelectionOperations, "copySelection").mockReturnValue({} as ImageData);
    const cut = vi.spyOn(SelectionOperations, "cutSelection").mockReturnValue({} as ImageData);
    const paste = vi.spyOn(SelectionOperations, "pasteSelection").mockImplementation(() => undefined);
    vi.spyOn(SelectionOperations, "hasClipboard").mockReturnValue(true);
    const host = renderTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    host.workspace.addDocument(session);
    const activeId = session.engine.getActiveLayerId()!;
    session.engine.setLayerImageBitmap(activeId, { width: 800, height: 600 } as ImageBitmap);
    session.engine.selectAll();

    button(host.container, "Edit").click();
    button(host.container, "Copy").click();
    button(host.container, "Edit").click();
    button(host.container, "Cut").click();
    button(host.container, "Edit").click();
    button(host.container, "Paste").click();

    expect(copy).toHaveBeenCalledWith(session.engine);
    expect(cut).toHaveBeenCalledWith(session.engine);
    expect(paste).toHaveBeenCalledWith(session.engine);
    host.dispose();
  });

  it("routes Layer > Duplicate Layer through history-backed layer actions", () => {
    const host = renderTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    host.workspace.addDocument(session);
    expect(session.engine.getLayers()).toHaveLength(1);

    button(host.container, "Layer").click();
    button(host.container, "Duplicate Layer").click();

    expect(session.engine.getLayers()).toHaveLength(2);
    expect(host.workspace.getActiveHistory()?.canUndo()).toBe(true);
    expect(host.scheduler.requestRender).toHaveBeenCalled();
    host.dispose();
  });

  it("opens About Photrez in the shared informational dialog", async () => {
    const host = renderTitleBar();
    button(host.container, "Help").click();
    button(host.container, "About Photrez").click();
    await new Promise<void>((resolve) => queueMicrotask(resolve));

    const dialog = document.querySelector<HTMLElement>('[role="dialog"]');
    expect(dialog).toHaveTextContent("Photrez");
    expect(dialog).toHaveTextContent("v0.1.0");
    expect(dialog).toHaveTextContent("Native desktop image studio");
    // The About dialog (informational) closes via its primary "Close" button;
    // focus returns to the menu trigger that opened it.
    const closeBtn = button(host.container, "Close");
    expect(closeBtn).not.toBeNull();
    closeBtn!.click();
    await new Promise<void>((resolve) => queueMicrotask(resolve));
    expect(document.activeElement).toBe(button(host.container, "Help"));
    host.dispose();
  });

  it("closes dropdown when pointerdown outside nav (document listener wiring)", () => {
    const host = renderTitleBar();

    // Open the File menu
    button(host.container, "File").click();
    expect(document.querySelector<HTMLElement>('[role="menu"]')).not.toBeNull();

    // Click outside the nav — dispatch on the container (parent of nav)
    // This exercises the document.addEventListener("pointerdown") from onMount
    host.container.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));

    // Menu should now be closed — no popup
    expect(document.querySelector<HTMLElement>('[role="menu"]')).toBeNull();
    host.dispose();
  });
});
