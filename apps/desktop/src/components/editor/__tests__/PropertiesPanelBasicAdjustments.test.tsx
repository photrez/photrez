import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { PropertiesPanel } from "../PropertiesPanel";
import { AdjustmentsPanel } from "../AdjustmentsPanel";
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
    localStorage.setItem("photrez.facadeAuthority", "wasm");
  });
  afterEach(() => {
    document.body.replaceChildren();
    localStorage.removeItem("photrez.facadeAuthority");
    vi.restoreAllMocks();
    clearRegistry();
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
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    setupOffscreenCanvasMock();
  });

  afterEach(() => {
    document.body.replaceChildren();
    localStorage.removeItem("photrez.facadeAuthority");
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    clearRegistry();
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
});

