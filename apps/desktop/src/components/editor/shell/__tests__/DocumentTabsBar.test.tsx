import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider } from "../EditorContext";
import { DocumentTabsBar } from "../DocumentTabsBar";
import { WorkspaceManager } from "@/engine/workspace";

vi.mock("../../dialogs/DialogProvider", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../dialogs/DialogProvider")>();
  return {
    ...actual,
    useDialog: () => ({
      newDocument: vi.fn().mockResolvedValue({
        name: "Untitled-1",
        width: 800,
        height: 600,
        backgroundColor: "transparent",
      }),
    }),
  };
});

function qs<T extends HTMLElement>(root: HTMLElement, sel: string): T | null {
  return root.querySelector(sel) as T | null;
}

function renderTabsBar() {
  const ws = new WorkspaceManager();
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);

  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
        <DocumentTabsBar />
      </EditorProvider>
    ),
    container,
  );

  return {
    ws,
    scheduler,
    container,
    dispose: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

// SolidJS batches reactive updates — need a microtask tick to flush
function tick(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

describe("DocumentTabsBar", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("renders new document button", () => {
    const { container, dispose } = renderTabsBar();
    expect(container.querySelector('button[aria-label="New document"]')).not.toBeNull();
    dispose();
  });

  it("renders document name in tab", async () => {
    const { ws, container, dispose } = renderTabsBar();
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-1", "Hello World", 800, 600));
    await tick();
    expect(container.textContent).toContain("Hello World");
    dispose();
  });

  it("renders multiple tabs for multiple documents", async () => {
    const { ws, container, dispose } = renderTabsBar();
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-1", "Tab A", 800, 600));
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-2", "Tab B", 800, 600));
    await tick();
    expect(container.textContent).toContain("Tab A");
    expect(container.textContent).toContain("Tab B");
    dispose();
  });

  it("has close button per tab with correct aria-label", async () => {
    const { ws, container, dispose } = renderTabsBar();
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-1", "My Tab", 800, 600));
    await tick();
    expect(container.querySelector('button[aria-label="Close My Tab"]')).not.toBeNull();
    dispose();
  });

  it("close button removes document", async () => {
    const { ws, container, scheduler, dispose } = renderTabsBar();
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-1", "Delete Me", 800, 600));
    await tick();
    qs<HTMLButtonElement>(container, 'button[aria-label="Close Delete Me"]')?.click();
    expect(ws.getSession("doc-1")).toBeNull();
    expect(scheduler.requestRender).toHaveBeenCalled();
    dispose();
  });

  it("new document button creates a new document", async () => {
    const { ws, container, scheduler, dispose } = renderTabsBar();
    const before = ws.getDocumentCount();
    qs<HTMLButtonElement>(container, 'button[aria-label="New document"]')?.click();
    await tick();
    expect(ws.getDocumentCount()).toBe(before + 1);
    expect(scheduler.requestRender).toHaveBeenCalled();
    dispose();
  });

  it("dirty tab shows dirty indicator dot", async () => {
    const { ws, container, dispose } = renderTabsBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Dirty", 800, 600);
    session.dirty = true;
    ws.addDocument(session);
    await tick();
    expect(container.textContent).toContain("•");
    dispose();
  });

  it("clean tab does not show dirty indicator dot", async () => {
    const { ws, container, dispose } = renderTabsBar();
    const session = WorkspaceManager.createBlankDocument("doc-1", "Clean", 800, 600);
    session.dirty = false;
    ws.addDocument(session);
    await tick();
    expect(container.textContent).not.toContain("•");
    dispose();
  });

  it("renders close button with svg icon inside tab", async () => {
    const { ws, container, dispose } = renderTabsBar();
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-1", "Tab", 800, 600));
    await tick();
    const closeBtn = container.querySelector('button[aria-label="Close Tab"]');
    expect(closeBtn).not.toBeNull();
    expect(closeBtn!.querySelector("svg")).not.toBeNull();
    dispose();
  });

  it("switches active tab on pointer click", async () => {
    const { ws, container, dispose } = renderTabsBar();
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-1", "Tab 1", 800, 600));
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-2", "Tab 2", 800, 600));
    await tick();

    expect(ws.getActiveDocumentId()).toBe("doc-2");
    const tab1 = container.querySelector('[data-document-tab="doc-1"]') as HTMLElement;
    tab1.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 50, clientY: 10, button: 0 }));
    tab1.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: 50, clientY: 10, button: 0 }));
    await tick();

    expect(ws.getActiveDocumentId()).toBe("doc-1");
    dispose();
  });

  it("reorders tabs when pointer drag moves past threshold", async () => {
    const { ws, container, dispose } = renderTabsBar();
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-1", "Tab 1", 800, 600));
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-2", "Tab 2", 800, 600));
    await tick();

    const tab1 = container.querySelector('[data-document-tab="doc-1"]') as HTMLElement;
    const tab2 = container.querySelector('[data-document-tab="doc-2"]') as HTMLElement;

    // Mock bounding client rects
    vi.spyOn(tab1, "getBoundingClientRect").mockReturnValue({
      left: 0,
      right: 100,
      top: 0,
      bottom: 38,
      width: 100,
      height: 38,
      x: 0,
      y: 0,
      toJSON: () => {},
    });
    vi.spyOn(tab2, "getBoundingClientRect").mockReturnValue({
      left: 100,
      right: 200,
      top: 0,
      bottom: 38,
      width: 100,
      height: 38,
      x: 100,
      y: 0,
      toJSON: () => {},
    });

    tab1.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 20, clientY: 10, button: 0 }));
    tab1.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 160, clientY: 10, button: 0 }));
    tab1.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: 160, clientY: 10, button: 0 }));
    await tick();

    expect(ws.getTabSummaries().map((t) => t.id)).toEqual(["doc-2", "doc-1"]);
    dispose();
  });
});
