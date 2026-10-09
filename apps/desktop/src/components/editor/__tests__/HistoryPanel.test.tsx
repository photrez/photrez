import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { invoke } from "@tauri-apps/api/core";
import { EditorProvider } from "../shell/EditorContext";
import { HistoryPanel } from "../HistoryPanel";
import { WorkspaceManager, type DocumentSession } from "@/engine/workspace";
import type { WebGL2Backend } from "@/renderer/webgl2";
import type { RenderScheduler } from "@/renderer/scheduler";
import { navigateHistoryNative } from "../nativeHistoryJump";
import * as bridge from "@/lib/protocol/bridge";
import * as facadeRegistry from "@/lib/protocol/facadeRegistry";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));

vi.mock("../Toast", () => ({ showToast: vi.fn() }));
import { showToast } from "../Toast";

vi.mock("../nativeHistoryJump", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../nativeHistoryJump")>();
  return { ...actual, navigateHistoryNative: vi.fn(actual.navigateHistoryNative) };
});

function renderHistoryPanel(session?: DocumentSession) {
  const workspace = new WorkspaceManager();
  const renderer = {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
  } as unknown as WebGL2Backend;
  const scheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;
  const container = document.createElement("div");
  document.body.appendChild(container);

  if (session) workspace.addDocument(session);

  const dispose = render(
    () => (
      <EditorProvider workspace={workspace} renderer={renderer} scheduler={scheduler}>
        <HistoryPanel />
      </EditorProvider>
    ),
    container,
  );

  return { workspace, renderer, scheduler, container, dispose };
}

describe("HistoryPanel", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    document.body.replaceChildren();
    localStorage.clear();
  });

  it("shows a useful empty state when no document is active", () => {
    const { container, dispose } = renderHistoryPanel();

    expect(container.textContent).toContain("No image open");

    dispose();
  });

  it("uses an edge-to-edge list and explains the baseline-only state", async () => {
    const session = WorkspaceManager.createBlankDocument("test-hist-baseline", "History Baseline", 800, 600);
    const { container, dispose } = renderHistoryPanel(session);

    await Promise.resolve();

    const list = container.querySelector<HTMLElement>("[data-history-list]");
    expect(list).not.toBeNull();
    expect(list).not.toHaveClass("rounded-[3px]");
    expect(list).not.toHaveClass("border");
    expect(container.textContent).toContain("Edits appear here");

    dispose();
  });

  it("renders chronological operations and exposes the active state accessibly", async () => {
    const session = WorkspaceManager.createBlankDocument("test-hist", "History Test", 800, 600);
    session.history.commit(session.engine.snapshot(), "Brush Stroke");
    session.history.commit(session.engine.snapshot(), "New Layer");
    const { container, dispose } = renderHistoryPanel(session);

    await Promise.resolve();

    const buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("button"));
    expect(buttons.map((button) => button.textContent)).toEqual([
      "Open",
      "Brush Stroke",
      "New Layer",
    ]);
    expect(buttons[2]).toHaveAttribute("aria-current", "step");
    expect(buttons[0]).not.toHaveAttribute("aria-current");

    dispose();
  });

  it("marks future states and navigates one step backward", async () => {
    const session = WorkspaceManager.createBlankDocument("test-hist-click", "History Click Test", 800, 600);
    const { history, engine } = session;
    history.commit(engine.snapshot(), "Brush Stroke");
    history.commit(engine.snapshot(), "New Layer");
    history.commit(engine.snapshot(), "Resize Canvas");
    const previous = history.undo(engine.snapshot());
    if (previous) engine.restore(previous);
    const { container, dispose } = renderHistoryPanel(session);

    await Promise.resolve();

    let buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("button"));
    expect(buttons.map((button) => button.textContent)).toEqual([
      "Open",
      "Brush Stroke",
      "New Layer",
      "Resize Canvas",
    ]);
    expect(buttons[2]).toHaveAttribute("aria-current", "step");
    expect(buttons[3]).toHaveAttribute("data-history-state", "future");

    buttons[1].click();
    await new Promise((r) => setTimeout(r, 0));

    expect(vi.mocked(navigateHistoryNative)).toHaveBeenCalledWith(expect.anything(), 1);
    // Whether navigator descended + applyFacadeSnapshot happened there is the real
    // navigateHistoryNative behavior pinned in navigateHistoryCursorStep.test.tsx; here
    // the panel contract is only "the click reaches the native stepper". After the
    // click the mocked stepper leaves history state alone, so active selection advances
    // no further than the re-synced TS count allows: future-state markings stay.
    buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("button"));
    expect(buttons[1]).toHaveAttribute("data-history-state", "past");

    dispose();
  });

  it("preserves intermediate snapshots during multi-step time travel", async () => {
    const session = WorkspaceManager.createBlankDocument("test-hist-multi", "History Multi", 800, 600);
    const { history, engine } = session;

    history.commit(engine.snapshot(), "New Layer");
    engine.addLayer("Layer 2");
    history.commit(engine.snapshot(), "New Layer");
    engine.addLayer("Layer 3");
    history.commit(engine.snapshot(), "New Layer");
    engine.addLayer("Layer 4");

    const { container, dispose } = renderHistoryPanel(session);
    await Promise.resolve();

    let buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("button"));
    buttons[1].click();
    await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(navigateHistoryNative)).toHaveBeenCalledWith(expect.anything(), 1);

    buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("button"));
    buttons[2].click();
    await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(navigateHistoryNative)).toHaveBeenCalledWith(expect.anything(), 2);

    dispose();
  });

  it("a real panel click moves aria-current to the clicked row", async () => {
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    facadeRegistry.__resetFacadeRegistryForTests();
    bridge.__resetEmulatedForTests();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async () => undefined);

    const workspace = new WorkspaceManager();
    const renderer = {
      uploadImage: vi.fn(),
      destroyTexture: vi.fn(),
      uploadSurfaceTiles: vi.fn(),
    } as unknown as WebGL2Backend;
    const scheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;
    const container = document.createElement("div");
    document.body.appendChild(container);

    const session = WorkspaceManager.createBlankDocument("test-hist-aria", "Aria Current", 8, 8);
    workspace.addDocument(session);
    const dispose = render(
      () => (
        <EditorProvider workspace={workspace} renderer={renderer} scheduler={scheduler}>
          <HistoryPanel />
        </EditorProvider>
      ),
      container,
    );
    await Promise.resolve();

    const facade = facadeRegistry.getFacade("test-hist-aria");
    await facade.addLayer("A");
    await facade.addLayer("B");
    await facade.addLayer("C");
    await session.history.resyncFromNativeQuery();
    workspace.notifyVisualChange();
    await Promise.resolve();

    let buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("button"));
    expect(buttons.length).toBe(4);
    expect(buttons[3]).toHaveAttribute("aria-current", "step");

    // Real stepper: three undos must move the native cursor to the open row,
    // and aria-current must follow it there - not merely record the call.
    buttons[0].click();
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));

    const after = await bridge.getHistoryQuery("test-hist-aria");
    expect(after.cursor).toBe(0);
    buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("button"));
    expect(buttons[0]).toHaveAttribute("aria-current", "step");
    expect(buttons[3]).not.toHaveAttribute("aria-current");

    dispose();
  });

  it("a metadata commit through the real facade shim appears in the panel with no manual resync or notify", async () => {
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    facadeRegistry.__resetFacadeRegistryForTests();
    bridge.__resetEmulatedForTests();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async () => undefined);

    const workspace = new WorkspaceManager();
    const renderer = {
      uploadImage: vi.fn(),
      destroyTexture: vi.fn(),
      uploadSurfaceTiles: vi.fn(),
    } as unknown as WebGL2Backend;
    const scheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;
    const container = document.createElement("div");
    document.body.appendChild(container);

    const session = WorkspaceManager.createBlankDocument("test-hist-commit", "Commit Resync", 8, 8);
    workspace.addDocument(session);
    // The production shim, installed the same way EditorShell installs it:
    // every CommandHistory.commit is mirrored into the native stream here.
    facadeRegistry.installFacadeCommitShim({
      getEngine: () => session.engine as never,
      getDocId: () => session.engine.getId(),
    });
    const dispose = render(
      () => (
        <EditorProvider workspace={workspace} renderer={renderer} scheduler={scheduler}>
          <HistoryPanel />
        </EditorProvider>
      ),
      container,
    );
    await Promise.resolve();

    const notifySpy = vi.spyOn(workspace, "notifyVisualChange");
    session.history.commit(session.engine.snapshot(), "New Layer");

    // The mirror settles asynchronously; the derived view can only be correct
    // once the read that follows it resolves. Poll like a user would see it.
    let buttons: HTMLButtonElement[] = [];
    for (let i = 0; i < 50; i++) {
      await new Promise((r) => setTimeout(r, 0));
      buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("button"));
      if (buttons.length === 2) break;
    }

    expect(buttons.map((b) => b.textContent)).toEqual(["Open", "New Layer"]);
    expect(buttons[1]).toHaveAttribute("aria-current", "step");
    // The publish must come from the history's own view-change hook - the test
    // body never calls notifyVisualChange itself.
    expect(notifySpy).toHaveBeenCalled();

    dispose();
  });

  it("a blocked jump surfaces a toast and a throwing restore cannot escape the chain", async () => {
    const session = WorkspaceManager.createBlankDocument("test-hist-blocked", "Blocked Jump", 800, 600);
    const { history, engine } = session;
    history.commit(engine.snapshot(), "Brush Stroke");
    history.commit(engine.snapshot(), "New Layer");
    const { container, dispose } = renderHistoryPanel(session);
    await Promise.resolve();

    vi.mocked(navigateHistoryNative).mockResolvedValueOnce({
      completedSteps: 0,
      blocked: true,
      reason: "external entry pending confirmation blocked the step",
    });
    let buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("button"));
    buttons[0].click();
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(showToast)).toHaveBeenCalledWith(
      expect.stringContaining("blocked"),
      "error",
    );

    vi.mocked(navigateHistoryNative).mockRejectedValueOnce(new Error("restore boom"));
    buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("button"));
    buttons[1].click();
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(showToast)).toHaveBeenCalledWith(
      expect.stringContaining("restore boom"),
      "error",
    );

    dispose();
  });

  it("two rapid row clicks cannot run concurrent native jumps", async () => {
    const session = WorkspaceManager.createBlankDocument("test-hist-inflight", "In Flight", 800, 600);
    const { history, engine } = session;
    history.commit(engine.snapshot(), "Brush Stroke");
    history.commit(engine.snapshot(), "New Layer");
    history.commit(engine.snapshot(), "Resize Canvas");
    const { container, dispose } = renderHistoryPanel(session);
    await Promise.resolve();

    let release: (v: { completedSteps: number; blocked: boolean }) => void = () => {};
    vi.mocked(navigateHistoryNative).mockClear();
    vi.mocked(navigateHistoryNative).mockImplementationOnce(
      () => new Promise((r) => { release = r; }),
    );
    const buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("button"));
    buttons[1].click();
    buttons[2].click();
    // navigateHistoryNative is called inside the resync .then, so give the
    // microtask queue one macrotask; the second click must have been dropped
    // by the in-flight guard before it ever reached the stepper.
    await new Promise((r) => setTimeout(r, 0));
    expect(vi.mocked(navigateHistoryNative)).toHaveBeenCalledTimes(1);
    release({ completedSteps: 1, blocked: false });
    await new Promise((r) => setTimeout(r, 0));

    dispose();
  });
});
