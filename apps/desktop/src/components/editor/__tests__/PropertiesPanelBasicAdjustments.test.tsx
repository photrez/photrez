import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { PropertiesPanel } from "../PropertiesPanel";
import { AdjustmentsPanel } from "../AdjustmentsPanel";
import { useEditorCommands } from "../useEditorCommands";
import { clearRegistry } from "../keyboardRegistry";
import { WorkspaceManager } from "@/engine/workspace";

function tick(ms: number = 0): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function renderPropertiesPanel(workspace: WorkspaceManager) {
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
        <PropertiesPanel />
      </EditorProvider>
    ),
    container,
  );

  return { container, dispose, renderer, scheduler };
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
      <EditorProvider workspace={workspace} renderer={renderer as any} scheduler={scheduler as any}>
        <AdjustmentsPanel />
      </EditorProvider>
    ),
    container,
  );

  return { container, dispose, renderer, scheduler };
}

describe("PropertiesPanel basic adjustments", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
  });
  afterEach(() => {
    document.body.replaceChildren();
    localStorage.removeItem("photrez.facade");
    localStorage.removeItem("photrez.facadeAuthority");
    vi.restoreAllMocks();
    clearRegistry();
  });

  it("applies adjustment to the active layer as the slider changes (non-destructive, no debounce)", async () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("adjust-doc", "Adjust Doc", 2, 1);
    workspace.addDocument(session);
    const layer = session.engine.getLayers()[0];
    const fakeBitmap = { width: 2, height: 1 } as ImageBitmap;
    session.engine.setLayerImageBitmap(layer.id, fakeBitmap);

    const applySpy = vi.spyOn(session.engine, "applyBasicAdjustment").mockImplementation(() => undefined);
    const history = workspace.getActiveHistory();
    if (!history) throw new Error("Expected active history");
    const commitSpy = vi.spyOn(history, "commit");

    const { container, dispose, renderer, scheduler } = renderAdjustmentsPanel(workspace);
    await tick();

    const brightness = container.querySelector<HTMLInputElement>("input[aria-label='Bright']");
    if (!brightness) throw new Error("Brightness slider was not rendered");
    brightness.value = "25";
    brightness.dispatchEvent(new InputEvent("input", { bubbles: true }));
    brightness.value = "40";
    brightness.dispatchEvent(new InputEvent("input", { bubbles: true }));

    expect(commitSpy).toHaveBeenCalledWith(expect.any(Object), "Adjust Brightness");
    expect(commitSpy).toHaveBeenCalledTimes(1);
    // No debounce: every input event applies the current value directly to the
    // engine as a render param (the GPU shader re-composites instantly).
    expect(applySpy).toHaveBeenCalledWith(layer.id, { brightness: 25, contrast: 0, saturation: 0 });
    expect(applySpy).toHaveBeenCalledWith(layer.id, { brightness: 40, contrast: 0, saturation: 0 });
    expect(applySpy).toHaveBeenCalledTimes(2);
    // Non-destructive: the panel no longer uploads a mutated bitmap.
    expect(renderer.uploadImage).not.toHaveBeenCalled();
    expect(scheduler.requestRender).toHaveBeenCalled();

    dispose();
  });

  it("submits transform edits from the Properties panel fields", async () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("transform-doc", "Transform Doc", 20, 10);
    workspace.addDocument(session);
    const layer = session.engine.getLayers()[0];
    layer.lockPosition = false;

    const transformSpy = vi.spyOn(session.engine, "transformLayer");
    const history = workspace.getActiveHistory();
    if (!history) throw new Error("Expected active history");
    const commitSpy = vi.spyOn(history, "commit");

    const { container, dispose, scheduler } = renderPropertiesPanel(workspace);
    await tick();

    const xField = container.querySelectorAll<HTMLInputElement>("input[type='text']")[0];
    if (!xField) throw new Error("Transform X field was not rendered");
    xField.focus();
    xField.value = "42";
    xField.dispatchEvent(new InputEvent("input", { bubbles: true }));
    xField.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));

    expect(commitSpy).toHaveBeenCalledWith(expect.any(Object), "Move Layer");
    expect(transformSpy).toHaveBeenCalledWith(layer.id, expect.objectContaining({ x: 42 }));
    expect(scheduler.requestRender).toHaveBeenCalled();

    dispose();
  });

  it("Constrain button toggles the shared constrainRatio signal (unified with canvas drag + option bar)", async () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("constrain-doc", "Constrain Doc", 20, 10);
    workspace.addDocument(session);

    const { container, dispose } = renderPropertiesPanel(workspace);
    await tick();

    const debug = (window as unknown as { __photrezEditor: any }).__photrezEditor;
    // Default is ON (keep aspect by default)
    expect(debug.constrainRatio()).toBe(true);

    const constrainBtn = container.querySelector<HTMLButtonElement>(
      "button[aria-label='Constrain proportions']"
    );
    if (!constrainBtn) throw new Error("Constrain button was not rendered");
    expect(constrainBtn.getAttribute("aria-pressed")).toBe("true");

    constrainBtn.click();
    await tick();

    // The SAME signal the canvas drag and TransformOptionBar read/write
    expect(debug.constrainRatio()).toBe(false);
    expect(constrainBtn.getAttribute("aria-pressed")).toBe("false");

    // Toggle back ON via the same button
    constrainBtn.click();
    await tick();
    expect(debug.constrainRatio()).toBe(true);

    dispose();
  });

  it("commits one undo checkpoint for an opacity slider edit session", async () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("opacity-doc", "Opacity Doc", 20, 10);
    workspace.addDocument(session);
    const layer = session.engine.getLayers()[0];
    const history = workspace.getActiveHistory();
    if (!history) throw new Error("Expected active history");
    const commitSpy = vi.spyOn(history, "commit");

    const { container, dispose, scheduler } = renderPropertiesPanel(workspace);
    await tick();

    const opacity = container.querySelector<HTMLInputElement>("input[aria-label='Opacity']");
    if (!opacity) throw new Error("Opacity slider was not rendered");
    opacity.value = "70";
    opacity.dispatchEvent(new InputEvent("input", { bubbles: true }));
    opacity.value = "45";
    opacity.dispatchEvent(new InputEvent("input", { bubbles: true }));

    expect(commitSpy).toHaveBeenCalledWith(expect.any(Object), "Adjust Opacity");
    expect(commitSpy).toHaveBeenCalledTimes(1);
    expect(session.engine.getLayer(layer.id)?.opacity).toBe(0.45);
    expect(scheduler.requestRender).toHaveBeenCalled();

    dispose();
  });

  it("explains empty and locked layer states inline", async () => {
    const emptyWorkspace = new WorkspaceManager();
    emptyWorkspace.addDocument(WorkspaceManager.createBlankDocument("empty-doc", "Empty Doc", 20, 10));
    const emptyRender = renderAdjustmentsPanel(emptyWorkspace);
    await tick();

    expect(emptyRender.container.textContent).toContain("This layer has no pixels yet");
    emptyRender.dispose();

    const lockedWorkspace = new WorkspaceManager();
    const lockedSession = WorkspaceManager.createBlankDocument("locked-doc", "Locked Doc", 20, 10);
    lockedWorkspace.addDocument(lockedSession);
    const lockedLayer = lockedSession.engine.getLayers()[0];
    lockedSession.engine.setLayerImageBitmap(lockedLayer.id, { width: 20, height: 10 } as ImageBitmap);
    lockedSession.engine.setLayerLocked(lockedLayer.id, true);

    const lockedProperties = renderPropertiesPanel(lockedWorkspace);
    await tick();
    expect(lockedProperties.container.textContent).toContain("Layer is locked. Unlock it in Layers to edit transform values.");
    lockedProperties.dispose();

    const lockedAdjustments = renderAdjustmentsPanel(lockedWorkspace);
    await tick();
    expect(lockedAdjustments.container.textContent).toContain("Layer is locked. Unlock it before applying pixel adjustments.");
    lockedAdjustments.dispose();
  });

  it("renders the selected layer and selected document cards appropriately", async () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("test-doc", "Test Document", 800, 600);
    workspace.addDocument(session);

    // Test case 1: Selected Layer Card is displayed (default selection is Background layer)
    const { container, dispose } = renderPropertiesPanel(workspace);
    await tick();

    expect(container.textContent).toContain("Selected Layer");
    expect(container.textContent).toContain("Background");
    expect(container.textContent).toContain("Image layer · 800 × 600 px");

    // Test case 2: Selected Document Card is displayed when no layer is selected
    session.engine.setActiveLayer(null);
    await tick();

    expect(container.textContent).toContain("Selected Document");
    expect(container.textContent).toContain("Test Document");
    expect(container.textContent).toContain("Canvas · 800 × 600 px");

    dispose();
  });
});

// ─── Production-behavior tests ──────────────────────────────────────────────
// These tests use the REAL engine (applyBasicAdjustment is NOT mocked) with
// only an OffscreenCanvas stub.  The existing test above mocks
// applyBasicAdjustment, which hides timing bugs between setBasicAdjustment
// and previewBasicAdjustment in the sync effect.  These tests would have
// caught bugs #1 (sliders stuck after undo), #2 (restore closing baseImageBitmap),
// and #3 (multi-checkpoint per drag session).

function setupOffscreenCanvasMock() {
  const MockConstructor = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    this.getContext = vi.fn((type: string) => {
      if (type === "2d") {
        return {
          drawImage: vi.fn(),
          clearRect: vi.fn(),
          save: vi.fn(),
          restore: vi.fn(),
          translate: vi.fn(),
          rotate: vi.fn(),
          scale: vi.fn(),
          putImageData: vi.fn(),
          getImageData: vi.fn(() => ({
            data: new Uint8ClampedArray(w * h * 4),
          })),
        };
      }
      return null;
    });
    this.transferToImageBitmap = vi.fn(function (this: any) {
      return {
        width: this.width,
        height: this.height,
        close: vi.fn(),
      } as unknown as ImageBitmap;
    });
  };

  vi.stubGlobal("OffscreenCanvas", MockConstructor as unknown as typeof OffscreenCanvas);
}

describe("AdjustmentsPanel — production engine behavior (no applyBasicAdjustment mock)", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    setupOffscreenCanvasMock();
  });

  afterEach(() => {
    document.body.replaceChildren();
    localStorage.removeItem("photrez.facade");
    localStorage.removeItem("photrez.facadeAuthority");
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    clearRegistry();
  });

  /** Simulate what useEditorCommands.restoreHistorySnapshot does:
   *  history.undo/redo → engine.restore → upload → notifyVisualChange.
   *  Keyboard shortcuts aren't mounted in the test tree, so we call
   *  the same internal path directly. */
  function simulateUndo(ws: WorkspaceManager, session: ReturnType<typeof WorkspaceManager.createBlankDocument>) {
    const engine = session.engine;
    const history = session.history;
    const snapshot = history.undo(engine.snapshot());
    if (snapshot) engine.restore(snapshot);
    // The real restoreHistorySnapshot also notifies workspace visual change
    // which triggers the sync effect that updates layers() signal.
    ws.notifyVisualChange();
  }
  function simulateRedo(ws: WorkspaceManager, session: ReturnType<typeof WorkspaceManager.createBlankDocument>) {
    const engine = session.engine;
    const history = session.history;
    const snapshot = history.redo(engine.snapshot());
    if (snapshot) engine.restore(snapshot);
    ws.notifyVisualChange();
  }

  it("slider drag creates exactly ONE history checkpoint per session (bug #3)", async () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("adj-doc", "Adj Doc", 4, 2);
    workspace.addDocument(session);
    const layer = session.engine.getLayers()[0];
    session.engine.setLayerImageBitmap(layer.id, {
      width: 4, height: 2, close: vi.fn(),
    } as unknown as ImageBitmap);

    const history = workspace.getActiveHistory()!;
    const commitSpy = vi.spyOn(history, "commit");

    const { container, dispose } = renderAdjustmentsPanel(workspace);
    await tick();

    const brightness = container.querySelector<HTMLInputElement>("input[aria-label='Bright']");
    if (!brightness) throw new Error("Brightness slider not found");

    // Simulate 10 incremental drag ticks (like a real slider drag from 0 → 100)
    for (let v = 10; v <= 100; v += 10) {
      brightness.value = String(v);
      brightness.dispatchEvent(new InputEvent("input", { bubbles: true }));
    }

    // CRITICAL: only 1 checkpoint for the entire drag session, not 10
    expect(commitSpy).toHaveBeenCalledTimes(1);
    expect(commitSpy).toHaveBeenCalledWith(expect.any(Object), "Adjust Brightness");

    // Let any pending microtasks settle
    await tick(100);

    // Engine received the FINAL adjustment value
    const adjusted = session.engine.getLayer(layer.id)!;
    expect(adjusted.basicAdjustment).toEqual({ brightness: 100, contrast: 0, saturation: 0 });

    dispose();
  });

  it("releasing the slider keeps the adjustment as a live param (baked on destructive edit)", async () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("bake-doc", "Bake Doc", 4, 2);
    workspace.addDocument(session);
    const layer = session.engine.getLayers()[0];
    const initialBitmap = { width: 4, height: 2, close: vi.fn() } as unknown as ImageBitmap;
    session.engine.setLayerImageBitmap(layer.id, initialBitmap);

    const { container, dispose } = renderAdjustmentsPanel(workspace);
    await tick();

    const brightness = container.querySelector<HTMLInputElement>("input[aria-label='Bright']");
    if (!brightness) throw new Error("Brightness slider not found");

    // Drag to 50 — during the drag the adjustment is a live GPU preview param.
    brightness.value = "50";
    brightness.dispatchEvent(new InputEvent("input", { bubbles: true }));
    await tick();
    expect(session.engine.getLayer(layer.id)!.basicAdjustment).toEqual({ brightness: 50, contrast: 0, saturation: 0 });

    // Release (input `change`) — the panel no longer bakes on release; the
    // adjustment stays a live, non-destructive param and the slider stays put.
    brightness.dispatchEvent(new Event("change", { bubbles: true }));
    await tick();

    const released = session.engine.getLayer(layer.id)!;
    expect(released.basicAdjustment).toEqual({ brightness: 50, contrast: 0, saturation: 0 }); // param kept
    expect(released.imageBitmap).toBe(initialBitmap); // NOT baked on release
    expect(brightness.value).toBe("50"); // slider stays

    dispose();
  });

  it("undo after adjustment resets sliders and clears adjustmentBase (bug #1)", async () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("undo-doc", "Undo Doc", 4, 2);
    workspace.addDocument(session);
    const layer = session.engine.getLayers()[0];
    session.engine.setLayerImageBitmap(layer.id, {
      width: 4, height: 2, close: vi.fn(),
    } as unknown as ImageBitmap);

    const history = workspace.getActiveHistory()!;
    const commitSpy = vi.spyOn(history, "commit");

    const { container, dispose } = renderAdjustmentsPanel(workspace);
    await tick();

    const brightness = container.querySelector<HTMLInputElement>("input[aria-label='Bright']");
    if (!brightness) throw new Error("Brightness slider not found");

    // Drag brightness to 50
    brightness.value = "50";
    brightness.dispatchEvent(new InputEvent("input", { bubbles: true }));
    await tick(100);

    // Confirm engine applied the adjustment
    expect(session.engine.getLayer(layer.id)!.basicAdjustment?.brightness).toBe(50);

    // Simulate undo (same path as Ctrl+Z handler)
    simulateUndo(workspace, session);
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

  it("redo restores slider values after undo (bug #1 continued)", async () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("redo-doc", "Redo Doc", 4, 2);
    workspace.addDocument(session);
    const layer = session.engine.getLayers()[0];
    session.engine.setLayerImageBitmap(layer.id, {
      width: 4, height: 2, close: vi.fn(),
    } as unknown as ImageBitmap);

    const { container, dispose } = renderAdjustmentsPanel(workspace);
    await tick();

    const brightness = container.querySelector<HTMLInputElement>("input[aria-label='Bright']");
    if (!brightness) throw new Error("Brightness slider not found");

    // Adjust to 60
    brightness.value = "60";
    brightness.dispatchEvent(new InputEvent("input", { bubbles: true }));
    await tick(100);

    // Undo
    simulateUndo(workspace, session);
    await tick();
    expect(brightness.value).toBe("0");

    // Redo
    simulateRedo(workspace, session);
    await tick();

    // Sliders must show the redone value
    expect(brightness.value).toBe("60");
    expect(session.engine.getLayer(layer.id)!.basicAdjustment?.brightness).toBe(60);

    dispose();
  });

  it("engine round-trip: adjust → undo → re-adjust creates separate checkpoints", () => {
    // Engine-level test (no SolidJS UI timing).
    // Catches the scenario where undo→re-adjust reuses the same
    // adjustmentBase instead of creating a fresh history checkpoint.
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("round-doc", "Round Doc", 4, 2);
    workspace.addDocument(session);
    const layer = session.engine.getLayers()[0];
    session.engine.setLayerImageBitmap(layer.id, {
      width: 4, height: 2, close: vi.fn(),
    } as unknown as ImageBitmap);

    const engine = workspace.getActiveEngine()!;
    const history = workspace.getActiveHistory()!;

    // ── Session 1: adjust brightness to 40 ──
    const snap0 = engine.snapshot();
    history.commit(snap0, "Adjust Brightness");
    engine.applyBasicAdjustment(layer.id, { brightness: 40, contrast: 0, saturation: 0 });
    expect(engine.getLayer(layer.id)!.basicAdjustment?.brightness).toBe(40);

    // ── Undo: back to pre-adjustment ──
    const restored = history.undo(engine.snapshot());
    expect(restored).toBeTruthy();
    engine.restore(restored!);
    expect(engine.getLayer(layer.id)!.basicAdjustment).toBeUndefined();

    // ── Session 2: re-adjust brightness to 70 ──
    // Must create a NEW checkpoint (not reuse session 1's base)
    const snap2 = engine.snapshot();
    history.commit(snap2, "Adjust Brightness");
    engine.applyBasicAdjustment(layer.id, { brightness: 70, contrast: 0, saturation: 0 });
    expect(engine.getLayer(layer.id)!.basicAdjustment?.brightness).toBe(70);

    // ── Undo session 2: back to pre-adjustment again ──
    const restored2 = history.undo(engine.snapshot());
    expect(restored2).toBeTruthy();
    engine.restore(restored2!);
    expect(engine.getLayer(layer.id)!.basicAdjustment).toBeUndefined();

    // ── Redo session 2: back to 70 ──
    const redone = history.redo(engine.snapshot());
    expect(redone).toBeTruthy();
    engine.restore(redone!);
    expect(engine.getLayer(layer.id)!.basicAdjustment?.brightness).toBe(70);

    // ── Redo session 1: back to 40 (if still on stack) ──
    // Note: the exact redo behavior depends on how many undos were done.
    // With 2 commits and 2 undos, redo should restore session 2 first.
  });

  it("switching sliders creates separate undo entries for each property", async () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("multi-doc", "Multi Doc", 4, 2);
    workspace.addDocument(session);
    const layer = session.engine.getLayers()[0];
    session.engine.setLayerImageBitmap(layer.id, {
      width: 4, height: 2, close: vi.fn(),
    } as unknown as ImageBitmap);

    const history = workspace.getActiveHistory()!;
    const commitSpy = vi.spyOn(history, "commit");

    const { container, dispose } = renderAdjustmentsPanel(workspace);
    await tick();

    const brightness = container.querySelector<HTMLInputElement>("input[aria-label='Bright']");
    const contrast = container.querySelector<HTMLInputElement>("input[aria-label='Contrast']");
    const saturation = container.querySelector<HTMLInputElement>("input[aria-label='Saturate']");
    if (!brightness || !contrast || !saturation) throw new Error("Sliders not found");

    // Adjust brightness
    brightness.value = "50";
    brightness.dispatchEvent(new InputEvent("input", { bubbles: true }));
    await tick(100);
    expect(commitSpy).toHaveBeenCalledTimes(1);

    // Switch to contrast — should create a NEW checkpoint
    contrast.value = "30";
    contrast.dispatchEvent(new InputEvent("input", { bubbles: true }));
    await tick(100);
    expect(commitSpy).toHaveBeenCalledTimes(2);

    // Switch to saturation — should create a NEW checkpoint
    saturation.value = "20";
    saturation.dispatchEvent(new InputEvent("input", { bubbles: true }));
    await tick(100);
    expect(commitSpy).toHaveBeenCalledTimes(3);

    // Engine has all three applied
    const adjusted = session.engine.getLayer(layer.id)!;
    expect(adjusted.basicAdjustment).toEqual({ brightness: 50, contrast: 30, saturation: 20 });

    // Each commit has the correct label
    expect(commitSpy.mock.calls[0][1]).toBe("Adjust Brightness");
    expect(commitSpy.mock.calls[1][1]).toBe("Adjust Contrast");
    expect(commitSpy.mock.calls[2][1]).toBe("Adjust Saturation");

    // Undo saturation → contrast + brightness remain
    simulateUndo(workspace, session);
    await tick();
    expect(session.engine.getLayer(layer.id)!.basicAdjustment?.brightness).toBe(50);
    expect(session.engine.getLayer(layer.id)!.basicAdjustment?.contrast).toBe(30);
    expect(session.engine.getLayer(layer.id)!.basicAdjustment?.saturation ?? 0).toBe(0);

    // Undo contrast → only brightness remains
    simulateUndo(workspace, session);
    await tick();
    expect(session.engine.getLayer(layer.id)!.basicAdjustment?.brightness).toBe(50);
    expect(session.engine.getLayer(layer.id)!.basicAdjustment?.contrast ?? 0).toBe(0);

    // Undo brightness → no adjustments
    simulateUndo(workspace, session);
    await tick();
    // Sliders must reset to zero
    expect(session.engine.getLayer(layer.id)!.basicAdjustment?.brightness ?? 0).toBe(0);
    expect(session.engine.getLayer(layer.id)!.basicAdjustment?.contrast ?? 0).toBe(0);
    expect(session.engine.getLayer(layer.id)!.basicAdjustment?.saturation ?? 0).toBe(0);
    expect(brightness.value).toBe("0");
    expect(contrast.value).toBe("0");
    expect(saturation.value).toBe("0");

    dispose();
  });

  it("Ctrl+Z works while slider is focused (isEditableTarget excludes range inputs)", async () => {
    // Reproduces the real-app scenario: user drags brightness slider →
    // the <input type="range"> retains focus → user presses Ctrl+Z.
    // Bug #4: isEditableTarget blocked the shortcut because <input> matched.
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("focus-doc", "Focus Doc", 4, 2);
    workspace.addDocument(session);
    const layer = session.engine.getLayers()[0];
    session.engine.setLayerImageBitmap(layer.id, {
      width: 4, height: 2, close: vi.fn(),
    } as unknown as ImageBitmap);

    const renderer = {
      uploadImage: vi.fn(),
      destroyTexture: vi.fn(),
      resize: vi.fn(),
      resizeToViewport: vi.fn(),
    };
    const scheduler = { requestRender: vi.fn() };
    const container = document.createElement("div");
    document.body.appendChild(container);

    // Mount BOTH AdjustmentsPanel and useEditorCommands (the keyboard handler)
    function Harness() {
      useEditorCommands(() => undefined);
      return <AdjustmentsPanel />;
    }

    const dispose = render(
      () => (
        <EditorProvider workspace={workspace} renderer={renderer as any} scheduler={scheduler as any}>
          <Harness />
        </EditorProvider>
      ),
      container,
    );

    await tick();

    const brightness = container.querySelector<HTMLInputElement>("input[aria-label='Bright']");
    if (!brightness) throw new Error("Brightness slider not found");

    // Drag brightness to 50
    brightness.value = "50";
    brightness.dispatchEvent(new InputEvent("input", { bubbles: true }));
    await tick(100);

    expect(session.engine.getLayer(layer.id)!.basicAdjustment?.brightness).toBe(50);

    // Simulate the real scenario: slider retains focus after drag,
    // then user presses Ctrl+Z.  The keyboard handler checks
    // isEditableTarget(document.activeElement) — this must NOT block
    // for <input type="range">.
    brightness.focus();

    // Dispatch Ctrl+Z while slider is focused (same path as useEditorCommands)
    window.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "z",
        ctrlKey: true,
        bubbles: true,
      }),
    );
    await tick();

    // After undo, sliders must reset and engine must revert
    expect(session.engine.getLayer(layer.id)!.basicAdjustment).toBeUndefined();
    expect(brightness.value).toBe("0");

    dispose();
  });
});

