import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { PropertiesPanel } from "../PropertiesPanel";
import { WorkspaceManager } from "@/engine/workspace";

function renderWithSelectedLayer(workspace: WorkspaceManager, layerId: string) {
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
        <SelectedLayerHarness layerId={layerId} />
      </EditorProvider>
    ),
    container,
  );

  return { container, dispose, renderer, scheduler };
}

function SelectedLayerHarness(props: { layerId: string }) {
  const editor = useEditor();
  editor.setSelectedLayerId(props.layerId);
  return <PropertiesPanel />;
}

function clickButton(container: HTMLElement, aria: string) {
  const btn = container.querySelector<HTMLButtonElement>(`button[aria-label='${aria}']`);
  btn?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  return btn;
}

describe("PropertiesPanel transform actions (Flip / Reset)", () => {
  afterEach(() => {
    document.body.replaceChildren();
    vi.restoreAllMocks();
  });

  it("Flip H toggles the layer's horizontal flip", () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("act-doc", "Act Doc", 100, 100);
    workspace.addDocument(session);
    const engine = session.engine;
    const layer = engine.getLayers()[0];
    engine.transformLayer(layer.id, { flipH: true });

    const { container, dispose } = renderWithSelectedLayer(workspace, layer.id);

    const before = engine.getLayer(layer.id)!.transform.flipH;
    expect(before).toBe(true);

    const btn = clickButton(container, "Flip horizontal");
    expect(btn).toBeTruthy();

    expect(engine.getLayer(layer.id)!.transform.flipH).toBe(false);
    dispose();
  });

  it("Reset Transform restores default position/scale/rotation/flip", () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("reset-doc", "Reset Doc", 100, 100);
    workspace.addDocument(session);
    const engine = session.engine;
    const layer = engine.getLayers()[0];
    engine.transformLayer(layer.id, {
      x: 50, y: 30, scaleX: 2, scaleY: 1.5, rotation: 45, flipH: true, flipV: true,
    });

    const { container, dispose } = renderWithSelectedLayer(workspace, layer.id);

    const btn = clickButton(container, "Reset transform");
    expect(btn).toBeTruthy();

    const t = engine.getLayer(layer.id)!.transform;
    expect(t.x).toBe(0);
    expect(t.y).toBe(0);
    expect(t.scaleX).toBeCloseTo(1, 5);
    expect(t.scaleY).toBeCloseTo(1, 5);
    expect(t.rotation).toBe(0);
    expect(t.flipH).toBe(false);
    expect(t.flipV).toBe(false);
    dispose();
  });

  it("Flip / Reset buttons are disabled for a locked layer", () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("lock-doc", "Lock Doc", 100, 100);
    workspace.addDocument(session);
    const engine = session.engine;
    const layer = engine.getLayers()[0];
    engine.setLayerLocked(layer.id, true);

    const { container, dispose } = renderWithSelectedLayer(workspace, layer.id);

    const flip = container.querySelector<HTMLButtonElement>("button[aria-label='Flip horizontal']");
    const reset = container.querySelector<HTMLButtonElement>("button[aria-label='Reset transform']");
    expect(flip?.disabled).toBe(true);
    expect(reset?.disabled).toBe(true);
    dispose();
  });

  it("Center Horizontal moves the layer to the horizontal canvas center", () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("ch-doc", "CH Doc", 400, 300);
    workspace.addDocument(session);
    const engine = session.engine;
    const layer = engine.addLayer("Test", 100, 50); // normal layer, no position lock
    engine.transformLayer(layer.id, { x: 10, y: 20 });

    const { container, dispose } = renderWithSelectedLayer(workspace, layer.id);
    const btn = container.querySelector<HTMLButtonElement>("button[aria-label='Center horizontally on canvas']");
    btn?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    const t = engine.getLayer(layer.id)!.transform;
    expect(t.x).toBeCloseTo((400 - 100) / 2, 5); // = 150
    expect(t.y).toBe(20); // untouched
    dispose();
  });

  it("Fit to Canvas scales the layer to fit inside the canvas and centers it", () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("fit-doc", "Fit Doc", 800, 600);
    workspace.addDocument(session);
    const engine = session.engine;
    const layer = engine.addLayer("Fit", 200, 150); // 4x smaller than canvas
    engine.transformLayer(layer.id, { scaleX: 1, scaleY: 1, x: 0, y: 0 });

    const { container, dispose } = renderWithSelectedLayer(workspace, layer.id);
    const btn = container.querySelector<HTMLButtonElement>("button[aria-label='Fit to canvas (scale + center)']");
    btn?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    const t = engine.getLayer(layer.id)!.transform;
    expect(t.scaleX).toBeCloseTo(4, 5);
    expect(t.scaleY).toBeCloseTo(4, 5);
    const effW = layer.width * t.scaleX;
    expect(effW).toBeCloseTo(800, 1);
    expect(t.x).toBeCloseTo((800 - effW) / 2, 5);
    dispose();
  });

  it("Rotate 90 CW adds 90 degrees (normalized)", () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("rot-doc", "Rot Doc", 100, 100);
    workspace.addDocument(session);
    const engine = session.engine;
    const layer = engine.addLayer("Rot", 100, 100); // normal layer, no rotation lock
    engine.transformLayer(layer.id, { rotation: 170 });

    const { container, dispose } = renderWithSelectedLayer(workspace, layer.id);
    const btn = container.querySelector<HTMLButtonElement>("button[aria-label='Rotate 90° clockwise']");
    btn?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    // 170 + 90 = 260 -> normalized to -100
    expect(engine.getLayer(layer.id)!.transform.rotation).toBe(-100);
    dispose();
  });

  it("Quick buttons are disabled on the Background layer (position+rotation locked)", () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("bg-doc", "BG Doc", 800, 600);
    workspace.addDocument(session);
    const engine = session.engine;
    const bg = engine.getLayers()[0]; // Background: isBackground, lockPosition, lockRotation
    expect(bg.isBackground).toBe(true);
    expect(bg.lockPosition).toBe(true);
    expect(bg.lockRotation).toBe(true);

    const { container, dispose } = renderWithSelectedLayer(workspace, bg.id);

    const centerH = container.querySelector<HTMLButtonElement>("button[aria-label='Center horizontally on canvas']");
    const centerV = container.querySelector<HTMLButtonElement>("button[aria-label='Center vertically on canvas']");
    const fit = container.querySelector<HTMLButtonElement>("button[aria-label='Fit to canvas (scale + center)']");
    const rotCcw = container.querySelector<HTMLButtonElement>("button[aria-label='Rotate 90° counterclockwise']");
    const rotCw = container.querySelector<HTMLButtonElement>("button[aria-label='Rotate 90° clockwise']");

    expect(centerH?.disabled).toBe(true);
    expect(centerV?.disabled).toBe(true);
    expect(fit?.disabled).toBe(true);
    expect(rotCcw?.disabled).toBe(true);
    expect(rotCw?.disabled).toBe(true);

    dispose();
  });
});
