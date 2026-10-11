import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
vi.mock("@tauri-apps/api/app", () => ({ getVersion: () => Promise.resolve("0.1.0") }));
import type { WebGL2Backend } from "@/renderer/webgl2";
import type { RenderScheduler } from "@/renderer/scheduler";
import { WorkspaceManager } from "@/engine/workspace";
import { facadeOwnedIdsOf } from "@/engine/document";
import { ViewportCamera } from "@/viewport/viewportCamera";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { EditorProvider, useEditor } from "../EditorContext";
import { DialogProvider } from "../../dialogs/DialogProvider";
import { AppTitleBar } from "../AppTitleBar";
import { invoke } from "@tauri-apps/api/core";
import { answerInvoke, resetStreams } from "@/engine/__tests__/rustStreamEmulator";

// The native transport, as the accept/refuse switch the open gate reads. Every
// sibling open route (the empty workspace, the tab bar, the image-open path)
// installs a document only after this commit settles, and a refused commit opens
// nothing; the transport is the emulator every other ownership suite drives, the
// gate is the real one.
const nativeSeeds = { accepted: true };
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

// The seeded layer set, as the native engine would hold it. The commit's own
// gates read it back twice - once as membership (`protocol_layer_ids_native`
// answers a bare id array) and once as the flagged layer's state - so the
// transport has to answer both or the open refuses for the wrong reason.
const { seededLayers } = vi.hoisted(() => ({ seededLayers: new Map<string, unknown[]>() }));
vi.mock("@/lib/protocol/nativeClient", () => ({
  nativeProtocol: {
    protocol_seed_native: vi.fn(async (payloadJson: string, docId: string) => {
      if (!nativeSeeds.accepted) throw new Error("E_SEED: the canonical document seed was refused");
      const payload = JSON.parse(payloadJson || "{}") as { layers?: unknown[] };
      seededLayers.set(docId, (payload.layers ?? []).map((layer) => ({ ...(layer as object) })));
      return JSON.stringify({ version: 0 });
    }),
    protocol_seed_canonical_native: vi.fn(async () => {
      if (!nativeSeeds.accepted) throw new Error("E_SEED: the canonical document seed was refused");
      return JSON.stringify({ ok: true });
    }),
    protocol_canonical_native: vi.fn(async () => JSON.stringify({ ok: true })),
    protocol_snapshot_native: vi.fn(async (docId: string) =>
      JSON.stringify({
        version: 0,
        layers: (seededLayers.get(docId) ?? []).map((layer) => ({
          ...(layer as object),
          isBackground: true,
          lockPosition: true,
          lockRotation: true,
        })),
      }),
    ),
    protocol_layer_ids_native: vi.fn(async (docId: string) =>
      JSON.stringify((seededLayers.get(docId) ?? []).map((layer) => (layer as { id: string }).id)),
    ),
    protocol_version_native: vi.fn(async () => JSON.stringify({ version: 0 })),
    protocol_register_adapter_native: vi.fn(async () => JSON.stringify({ ok: true })),
    protocol_history_query_native: vi.fn(async () => JSON.stringify({ cursor: 0, entries: [] })),
    protocol_history_cursor_commit_native: vi.fn(async () => JSON.stringify({ ok: true })),
    protocol_apply_command_native: vi.fn(async () =>
      JSON.stringify({ documentVersion: 0, delta: { changes: [], baseVersion: 0, version: 0 } }),
    ),
  },
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
  beforeEach(() => {
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    nativeSeeds.accepted = true;
    seededLayers.clear();
    resetStreams();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) =>
      answerInvoke(cmd, args) as Promise<unknown>,
    );
  });
  afterEach(() => {
    localStorage.removeItem("photrez.facadeAuthority");
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
    vi.restoreAllMocks();
  });

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
    // The open is SEEDED, not merely opened: the background-flag commit settled and
    // claimed the new document's layers before it was installed. An un-claimed
    // document has no native executor, so this is the difference between a live
    // document and an undoable one.
    expect(
      facadeOwnedIdsOf(host.workspace.getActiveDocumentId()!)?.size ?? 0,
      "the document the route installed owns its layers",
    ).toBeGreaterThan(0);
    host.dispose();
  });

  it("a refused native seed installs nothing from File > New Document", async () => {
    nativeSeeds.accepted = false;
    const host = renderTitleBar();

    button(host.container, "File").click();
    button(host.container, "New Document").click();
    await Promise.resolve();

    const createBtn = Array.from(document.querySelectorAll("button")).find((btn) => btn.textContent === "Create");
    if (!createBtn) throw new Error("Create button not found in dialog");
    createBtn.click();
    for (let i = 0; i < 8; i++) await new Promise((resolve) => setTimeout(resolve, 0));

    // DEFEAT: the ungated open installed the session regardless, leaving a live
    // document the native engine does not hold and the ledger does not claim.
    expect(host.workspace.getDocumentCount(), "a refused seed opens nothing").toBe(0);
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
    // Cut also carries the history store and renderer: it is recorded by Rust
    // as one canonical pixel write, so it owns its history commit.
    expect(cut).toHaveBeenCalledWith(session.engine, expect.anything(), expect.anything());
    expect(paste).toHaveBeenCalledWith(session.engine);
    host.dispose();
  });

  it("routes Layer > Duplicate Layer through history-backed layer actions", async () => {
    const host = renderTitleBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Test", 800, 600);
    host.workspace.addDocument(session);
    expect(session.engine.getLayers()).toHaveLength(1);

    button(host.container, "Layer").click();
    button(host.container, "Duplicate Layer").click();
    // The routed duplicate settles a microtask after the menu click.
    await new Promise((resolve) => setTimeout(resolve, 0));

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
