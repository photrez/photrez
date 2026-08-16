import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { AppMenuBar } from "../AppMenuBar";
import type { EditorCommand } from "../../useEditorCommands";
import { EditorProvider } from "../EditorContext";
import { WorkspaceManager } from "@/engine/workspace";
import { WebGL2Backend } from "@/renderer/webgl2";
import { RenderScheduler } from "@/renderer/scheduler";
import { ViewportCamera } from "@/viewport/viewportCamera";

function tick(): Promise<void> {
  return new Promise((resolve) => queueMicrotask(resolve));
}

function renderMenu(options?: {
  isEnabled?: (command: EditorCommand) => boolean;
  isRightDockOpen?: boolean;
}) {
  const execute = vi.fn<(command: EditorCommand) => void>();
  const container = document.createElement("div");
  document.body.appendChild(container);
  // right-dock-layout label conditionally. In production it is
  // always mounted inside EditorProvider (EditorShell.tsx renders
  // it under EditorProvider), so the test now wraps with the same
  // provider to mirror the production tree.
  const workspace = new WorkspaceManager();
  const camera = new ViewportCamera();
  const renderer = new WebGL2Backend();
  const scheduler = new RenderScheduler(() => {});
  const dispose = render(
    () => (
      <EditorProvider
        workspace={workspace}
        renderer={renderer}
        scheduler={scheduler}
        camera={camera}
      >
        <AppMenuBar
          execute={execute}
          isEnabled={options?.isEnabled ?? (() => true)}
          isRightDockOpen={options?.isRightDockOpen ?? true}
        />
      </EditorProvider>
    ),
    container,
  );
  return { container, execute, dispose };
}

/** Find a button anywhere in the document (popup may be portal-rendered to body) */
function button(_container: HTMLElement, label: string): HTMLButtonElement {
  const match = Array.from(document.querySelectorAll("button"))
    .find((element) => element.getAttribute("aria-label") === label || element.textContent?.trim() === label);
  if (!(match instanceof HTMLButtonElement)) throw new Error(`Button not found: ${label}`);
  return match;
}

describe("AppMenuBar", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("opens a dropdown without executing an action, then routes the selected item", () => {
    const host = renderMenu();

    button(host.container, "File").click();
    expect(host.execute).not.toHaveBeenCalled();
    expect(document.querySelector('[role="menu"][aria-label="File menu"]')).not.toBeNull();

    button(host.container, "Open Image…").click();
    expect(host.execute).toHaveBeenCalledWith("file.open");
    expect(document.querySelector('[role="menu"][aria-label="File menu"]')).toBeNull();
    host.dispose();
  });

  it("renders every application menu with real entries", () => {
    const host = renderMenu();
    const expectedItems: Record<string, string> = {
      File: "New Document",
      Edit: "Undo",
      Image: "Resize Canvas…",
      Layer: "New Layer",
      View: "Zoom In",
      Window: "Minimize",
      Help: "About Photrez",
    };

    for (const [menu, item] of Object.entries(expectedItems)) {
      button(host.container, menu).click();
      expect(button(host.container, item)).toBeEnabled();
      button(host.container, menu).click();
    }

    button(host.container, "Edit").click();
    expect(button(host.container, "Cut")).toBeEnabled();
    expect(button(host.container, "Select All")).toBeEnabled();
    button(host.container, "Edit").click();

    button(host.container, "Layer").click();
    expect(button(host.container, "Flatten Image")).toBeEnabled();
    host.dispose();
  });

  it("disables document commands when the shared command router rejects them", () => {
    const host = renderMenu({
      isEnabled: (command) => command !== "file.export" && command !== "edit.undo",
    });

    button(host.container, "File").click();
    expect(button(host.container, "Export…")).toBeDisabled();
    button(host.container, "Export…").click();
    expect(host.execute).not.toHaveBeenCalled();

    button(host.container, "Edit").dispatchEvent(new PointerEvent("pointerenter", { bubbles: true }));
    expect(button(host.container, "Undo")).toBeDisabled();
    host.dispose();
  });

  it("supports Arrow navigation between items and adjacent menus", async () => {
    const host = renderMenu();
    const fileTrigger = button(host.container, "File");

    fileTrigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    await tick();
    expect(document.activeElement).toHaveAttribute("aria-label", "New Document");

    document.activeElement?.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }),
    );
    expect(document.activeElement).toHaveAttribute("aria-label", "Open Image…");

    document.activeElement?.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }),
    );
    await tick();
    expect(document.querySelector('[role="menu"][aria-label="Edit menu"]')).not.toBeNull();
    expect(document.activeElement).toHaveAttribute("aria-label", "Undo");
    host.dispose();
  });

  it("closes on Escape and outside pointerdown, restoring trigger focus for Escape", async () => {
    const host = renderMenu();
    const fileTrigger = button(host.container, "File");
    fileTrigger.click();
    button(host.container, "New Document").focus();

    document.activeElement?.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
    await tick();
    expect(document.querySelector('[role="menu"]')).toBeNull();
    expect(document.activeElement).toBe(fileTrigger);

    fileTrigger.click();
    document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    expect(document.querySelector('[role="menu"]')).toBeNull();
    host.dispose();
  });

  it("reflects the current side-panel state in the View menu label", () => {
    const openHost = renderMenu({ isRightDockOpen: true });
    button(openHost.container, "View").click();
    expect(button(openHost.container, "Hide Side Panels")).toBeEnabled();
    openHost.dispose();

    const closedHost = renderMenu({ isRightDockOpen: false });
    button(closedHost.container, "View").click();
    expect(button(closedHost.container, "Show Side Panels")).toBeEnabled();
    closedHost.dispose();
  });

  it("opens Settings menu and routes Preferences to app.settings", () => {
    const host = renderMenu();
    button(host.container, "Settings").click();
    expect(document.querySelector('[role="menu"][aria-label="Settings menu"]')).not.toBeNull();
    button(host.container, "Preferences…").click();
    expect(host.execute).toHaveBeenCalledWith("app.settings");
    host.dispose();
  });
});
