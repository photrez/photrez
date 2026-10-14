/**
 * Regression pins: an adjustment slider's VALUE must survive undo/redo.
 *
 * These two cases lived in PropertiesPanelBasicAdjustments.test.tsx until the
 * flag-OFF sweep retired that file, and both pinned real defects (the file's
 * "bug #1" comments): an adjustment that never wrote a second history entry
 * left the sliders showing the adjusted value after an undo, and a redo did
 * not restore them. The sweep killed the file with its flag pins, so these two
 * LIVE cases were re-homed here rather than lost with it.
 *
 * The simulation below drives the surviving restore path: restore a captured
 * memento snapshot, plus the workspace notify the real restoreHistorySnapshot
 * performs, which is what runs the panel's sync effect.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider } from "../shell/EditorContext";
import { AdjustmentsPanel } from "../AdjustmentsPanel";
import { clearRegistry } from "../keyboardRegistry";
import { WorkspaceManager } from "@/engine/workspace";
import type { DocumentModel } from "@/engine/types";

function tick(ms: number = 0): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function renderAdjustmentsPanel(workspace: WorkspaceManager) {
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
      <EditorProvider workspace={workspace} renderer={renderer as never} scheduler={scheduler as never}>
        <AdjustmentsPanel />
      </EditorProvider>
    ),
    container,
  );

  return { container, dispose, renderer, scheduler };
}

type BlankSession = ReturnType<typeof WorkspaceManager.createBlankDocument>;

// Undo/redo press for an un-owned document: restore the memento snapshot the
// native history hands back, then notify so the panel sync effect runs.
function simulateRestore(
  ws: WorkspaceManager,
  session: BlankSession,
  memento: DocumentModel,
) {
  session.engine.restore(memento);
  ws.notifyVisualChange();
}

describe("adjustment slider values across undo/redo", () => {
  afterEach(() => {
    document.body.replaceChildren();
    vi.restoreAllMocks();
    clearRegistry();
  });

  it("undo after adjustment resets sliders and clears adjustmentBase", async () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("undo-doc", "Undo Doc", 4, 2);
    workspace.addDocument(session);
    const layer = session.engine.getLayers()[0];
    session.engine.setLayerImageBitmap(layer.id, {
      width: 4,
      height: 2,
      close: vi.fn(),
    } as unknown as ImageBitmap);

    const history = workspace.getActiveHistory()!;
    const commitSpy = vi.spyOn(history, "commit");

    // Memento of the pre-adjustment state, exactly as the native
    // history would hand it back on undo.
    const preAdjustment = session.engine.snapshot();

    const { container, dispose } = renderAdjustmentsPanel(workspace);
    await tick();

    const brightness = container.querySelector<HTMLInputElement>("input[aria-label='Bright']");
    if (!brightness) throw new Error("Brightness slider not found");

    // Drag brightness to 50
    brightness.value = "50";
    brightness.dispatchEvent(new InputEvent("input", { bubbles: true }));
    await tick(100);
    // Gesture boundary: the slider commits on change (pointerup/blur), exactly
    // as a real drag-releases does.
    brightness.dispatchEvent(new Event("change", { bubbles: true }));
    await tick(100);

    // Confirm engine applied the adjustment
    expect(session.engine.getLayer(layer.id)!.basicAdjustment?.brightness).toBe(50);

    // Undo: restore the pre-adjustment memento
    simulateRestore(workspace, session, preAdjustment);
    await tick();

    // Sliders must reset to zero
    expect(brightness.value).toBe("0");

    // Engine restored to pre-adjustment state
    const restored = session.engine.getLayer(layer.id)!;
    expect(restored.basicAdjustment).toBeUndefined();

    // Exactly 1 commit total (the pre-adjustment snapshot)
    expect(commitSpy).toHaveBeenCalledTimes(1);

    dispose();
  });

  it("redo restores slider values after undo", async () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("redo-doc", "Redo Doc", 4, 2);
    workspace.addDocument(session);
    const layer = session.engine.getLayers()[0];
    session.engine.setLayerImageBitmap(layer.id, {
      width: 4,
      height: 2,
      close: vi.fn(),
    } as unknown as ImageBitmap);

    // Mementos of the pre- and post-adjustment states.
    const preAdjustment = session.engine.snapshot();

    const { container, dispose } = renderAdjustmentsPanel(workspace);
    await tick();

    const brightness = container.querySelector<HTMLInputElement>("input[aria-label='Bright']");
    if (!brightness) throw new Error("Brightness slider not found");

    // Adjust to 60
    brightness.value = "60";
    brightness.dispatchEvent(new InputEvent("input", { bubbles: true }));
    await tick(100);
    brightness.dispatchEvent(new Event("change", { bubbles: true }));
    await tick(100);
    const postAdjustment = session.engine.snapshot();

    // Undo
    simulateRestore(workspace, session, preAdjustment);
    await tick();
    expect(brightness.value).toBe("0");

    // Redo
    simulateRestore(workspace, session, postAdjustment);
    await tick();

    // Sliders must show the redone value
    expect(brightness.value).toBe("60");
    expect(session.engine.getLayer(layer.id)!.basicAdjustment?.brightness).toBe(60);

    dispose();
  });
});
