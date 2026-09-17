import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider } from "../../shell/EditorContext";
import { LayersPanel } from "../../layers/LayersPanel";
import { WorkspaceManager } from "@/engine/workspace";

function installCanvasMocks(bitmap: ImageBitmap) {
  vi.spyOn(HTMLCanvasElement.prototype, "getContext")
    .mockImplementation(((type: string) => {
      if (type !== "2d") return null;
      return {
        clearRect: vi.fn(),
        fillRect: vi.fn(),
        drawImage: vi.fn(),
        strokeRect: vi.fn(),
        save: vi.fn(),
        restore: vi.fn(),
        translate: vi.fn(),
        rotate: vi.fn(),
        scale: vi.fn(),
        beginPath: vi.fn(),
        rect: vi.fn(),
        clip: vi.fn(),
        set fillStyle(_value: string) {},
        set strokeStyle(_value: string) {},
        set lineWidth(_value: number) {},
        set globalAlpha(_value: number) {},
        set globalCompositeOperation(_value: string) {},
      } as unknown as CanvasRenderingContext2D;
    }) as any);

  vi.stubGlobal("OffscreenCanvas", class {
    width: number;
    height: number;
    constructor(width: number, height: number) {
      this.width = width;
      this.height = height;
    }
    getContext() {
      return {
        drawImage: vi.fn(),
        save: vi.fn(),
        restore: vi.fn(),
        translate: vi.fn(),
        rotate: vi.fn(),
        scale: vi.fn(),
        set globalAlpha(_value: number) {},
        set globalCompositeOperation(_value: string) {},
      };
    }
    transferToImageBitmap() {
      return bitmap;
    }
  });
}

function renderLayersPanel(session = WorkspaceManager.createBlankDocument("test", "Test", 800, 600)) {
  const ws = new WorkspaceManager();
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);

  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
        <LayersPanel />
      </EditorProvider>
    ),
    container,
  );

  ws.addDocument(session);

  return {
    ws,
    session,
    renderer,
    scheduler,
    container,
    dispose: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

describe("Delete layer (no confirm dialog)", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
  });
  afterEach(() => {
    localStorage.removeItem("photrez.facade");
    localStorage.removeItem("photrez.facadeAuthority");
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  // After removing the deletion confirmation dialog, delete is immediate.
  // Ctrl+Z undo is the safety net — matches professional editor behavior.
  it("delete button immediately removes the layer without a dialog", () => {
    const session = WorkspaceManager.createBlankDocument("test", "Test", 800, 600);
    const bitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    installCanvasMocks(bitmap);

    const { container, session: s, dispose } = renderLayersPanel(session);

    const l2 = s.engine.addLayer("Layer 2");
    expect(s.engine.getLayers()).toHaveLength(2);

    const deleteBtn = container.querySelector<HTMLButtonElement>("button[aria-label='Delete Layer']");
    expect(deleteBtn).toBeTruthy();

    deleteBtn!.click();

    expect(s.engine.getLayers()).toHaveLength(1);
    expect(s.engine.getLayer(l2.id)).toBeUndefined();
    // No dialog should appear
    expect(document.querySelector('[role="alertdialog"]')).toBeNull();

    dispose();
  });

  it("cannot delete the only remaining layer", () => {
    const session = WorkspaceManager.createBlankDocument("test", "Test", 800, 600);
    const bitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    installCanvasMocks(bitmap);

    const { container, session: s, dispose } = renderLayersPanel(session);
    const initialCount = s.engine.getLayers().length;
    expect(initialCount).toBe(1);

    const deleteBtn = container.querySelector<HTMLButtonElement>("button[aria-label='Delete Layer']");
    expect(deleteBtn).toBeTruthy();
    deleteBtn!.click();

    expect(document.querySelector('[role="alertdialog"]')).toBeNull();
    expect(s.engine.getLayers()).toHaveLength(1);

    dispose();
  });

  it("deletes from the correct document immediately (no dialog race)", () => {
    const first = WorkspaceManager.createBlankDocument("first", "First", 800, 600);
    first.engine.addLayer("Layer 2");
    const { ws, container, dispose } = renderLayersPanel(first);

    const deleteBtn = container.querySelector<HTMLButtonElement>("button[aria-label='Delete Layer']")!;
    deleteBtn.click();

    // Layer removed immediately from doc 1 (no dialog to pause on)
    expect(first.engine.getLayers()).toHaveLength(1);

    // Add doc 2 and verify it has 1 layer (unaffected)
    const second = WorkspaceManager.createBlankDocument("second", "Second", 800, 600);
    ws.addDocument(second);
    expect(second.engine.getLayers()).toHaveLength(1);

    dispose();
  });
});
