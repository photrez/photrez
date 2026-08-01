import { describe, it, expect, vi } from "vitest";
import { useCanvasDrop } from "../useCanvasDrop";
import type { DragController, DragState } from "../../DragController";
import type { ViewportCamera } from "@/viewport/viewportCamera";
import { WorkspaceManager } from "@/engine/workspace";
import type { LayerDragPayload } from "../../dragTypes";

function fakeState(overrides?: Partial<DragState>): DragState {
  return {
    dragKind: null,
    payload: null,
    filePaths: null,
    dragStartPosition: null,
    dropTarget: null,
    hoverTabId: null,
    cascadeIndex: 0,
    ...overrides,
  };
}

function createDragController(initialState: DragState = fakeState()): DragController {
  let state = { ...initialState };
  return {
    state: () => state,
    beginLayerDrag: vi.fn(),
    beginFileDrag: vi.fn(),
    endDrag: vi.fn(() => { state = fakeState(); }),
    setDropTarget: vi.fn((t) => { state.dropTarget = t; }),
    startTabHover: vi.fn(),
    cancelTabHover: vi.fn(),
    isTabHovering: vi.fn(() => false),
  };
}

function createCamera() {
  return { screenToDocument: vi.fn((x: number, y: number) => ({ x, y })) } as unknown as ViewportCamera;
}

// jsdom does not implement DragEvent — use plain Event instead
function dragEvent(
  type: string,
  opts: Partial<EventInit> & { relatedTarget?: EventTarget | null; dataTransfer?: { dropEffect: string } } = {},
): DragEvent {
  const evt = new Event(type, { bubbles: true, cancelable: true, ...opts }) as DragEvent;
  if ("relatedTarget" in opts) {
    Object.defineProperty(evt, "relatedTarget", { value: opts.relatedTarget });
  }
  if (opts.dataTransfer) {
    Object.defineProperty(evt, "dataTransfer", { value: opts.dataTransfer });
  }
  return evt;
}

describe("useCanvasDrop", () => {
  it("calls preventDefault on dragover and sets canvas drop target", () => {
    const dc = createDragController(fakeState({ dragKind: "layer" }));
    const { onDragOver } = useCanvasDrop({
      dragController: dc,
      camera: createCamera(),
      workspace: {} as any,
      renderer: {} as any,
      scheduler: { requestRender: vi.fn() },
    });

    const evt = dragEvent("dragover") as DragEvent;
    const preventDefault = vi.spyOn(evt, "preventDefault");
    onDragOver(evt);

    expect(preventDefault).toHaveBeenCalled();
    expect(dc.setDropTarget).toHaveBeenCalledWith({ type: "canvas" });
  });

  it("sets copy cursor for cross-doc layer dragover and move for same-doc", () => {
    const make = (sourceDocId: string, activeId: string) => {
      const dc = createDragController(fakeState({
        dragKind: "layer",
        payload: { version: 1, sourceDocId, layerId: "l1", sourceName: "x", isAltPressed: false },
      }));
      const { onDragOver } = useCanvasDrop({
        dragController: dc,
        camera: createCamera(),
        workspace: { getActiveDocumentId: () => activeId } as any,
        renderer: {} as any,
        scheduler: { requestRender: vi.fn() },
      });
      const evt = dragEvent("dragover", { dataTransfer: { dropEffect: "" } });
      onDragOver(evt);
      return evt.dataTransfer!.dropEffect;
    };
    expect(make("doc-A", "doc-B")).toBe("copy"); // cross-doc
    expect(make("doc-A", "doc-A")).toBe("move"); // same-doc reorder
  });

  it("does not set drop target when no drag is active", () => {
    const dc = createDragController(fakeState({ dragKind: null }));
    const { onDragOver } = useCanvasDrop({
      dragController: dc,
      camera: createCamera(),
      workspace: {} as any,
      renderer: {} as any,
      scheduler: { requestRender: vi.fn() },
    });

    onDragOver(dragEvent("dragover") as DragEvent);
    expect(dc.setDropTarget).not.toHaveBeenCalled();
  });

  it("calls endDrag on drop even when no drag kind matches", () => {
    const dc = createDragController(fakeState({ dragKind: null }));
    const { onDrop } = useCanvasDrop({
      dragController: dc,
      camera: createCamera(),
      workspace: {} as any,
      renderer: {} as any,
      scheduler: { requestRender: vi.fn() },
    });

    onDrop(dragEvent("drop") as DragEvent);
    expect(dc.endDrag).toHaveBeenCalled();
  });

  it("clears canvas drop target on dragLeave when mouse leaves the element", () => {
    const dc = createDragController(fakeState({ dragKind: "layer", dropTarget: { type: "canvas" } }));
    const { onDragLeave } = useCanvasDrop({
      dragController: dc,
      camera: createCamera(),
      workspace: {} as any,
      renderer: {} as any,
      scheduler: { requestRender: vi.fn() },
    });

    const el = document.createElement("div");
    const evt = dragEvent("dragleave", { relatedTarget: document.createElement("span") });
    Object.defineProperty(evt, "currentTarget", { value: el });

    onDragLeave(evt as DragEvent);
    expect(dc.setDropTarget).toHaveBeenCalledWith(null);
  });

  it("converts window-absolute drop coords to canvas-container-relative before screenToDocument", async () => {
    const dc = createDragController(fakeState({ dragKind: "file", filePaths: ["/tmp/x.png"] }));
    const camera = createCamera();
    const { onDrop } = useCanvasDrop({
      dragController: dc,
      camera,
      workspace: { getActiveDocumentId: () => "doc-x", getEngine: () => null } as any, // resolveTargetDocId → doc-x; getEngine null → addFilesAsLayers short-circuits to []
      renderer: {} as any,
      scheduler: { requestRender: vi.fn() },
    });

    const el = document.createElement("div");
    el.getBoundingClientRect = () =>
      ({ left: 100, top: 80, right: 1100, bottom: 680, width: 1000, height: 600, x: 100, y: 80, toJSON: () => ({}) }) as DOMRect;
    const evt = dragEvent("drop");
    Object.defineProperty(evt, "clientX", { value: 500 });
    Object.defineProperty(evt, "clientY", { value: 400 });
    Object.defineProperty(evt, "currentTarget", { value: el });

    await onDrop(evt as DragEvent);

    // 500-100=400, 400-80=320 — the canvas-container offset must be removed
    expect(camera.screenToDocument).toHaveBeenCalledWith(400, 320);
  });

  it("drops a cross-doc layer onto the canvas → real engine gains a centered layer", async () => {
    // Real WorkspaceManager + real engines: proves the drop handler actually
    // creates the layer in the target document (catches the "passes unit,
    // fails in app" class) and centers it on the cursor.
    const ws = new WorkspaceManager();
    const sA = WorkspaceManager.createBlankDocument("docA", "A", 800, 600);
    ws.addDocument(sA);
    const sB = WorkspaceManager.createBlankDocument("docB", "B", 1000, 800);
    ws.addDocument(sB);
    ws.switchDocument("docB"); // a canvas drop targets the active doc

    const engineA = ws.getEngine("docA")!;
    const src = engineA.addLayer("Logo");
    const fakeBitmap = { width: 800, height: 600, close: vi.fn() } as unknown as ImageBitmap;
    engineA.setLayerImageBitmap(src.id, fakeBitmap);
    const payload: LayerDragPayload = {
      version: 1,
      sourceDocId: "docA",
      layerId: src.id,
      sourceName: "Logo",
      isAltPressed: false,
    };

    const uploadImage = vi.fn();
    const requestRender = vi.fn();
    const dc = createDragController(
      fakeState({ dragKind: "layer", payload, dropTarget: { type: "canvas" } }),
    );

    const camera = createCamera();
    const { onDrop } = useCanvasDrop({
      dragController: dc,
      camera,
      workspace: ws as any,
      renderer: { uploadImage },
      scheduler: { requestRender },
    });

    const targetEngine = ws.getEngine("docB")!;
    const before = targetEngine.getLayers().length;

    const el = document.createElement("div");
    el.getBoundingClientRect = () =>
      ({ left: 0, top: 0, right: 1000, bottom: 800, width: 1000, height: 800, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect;
    const evt = dragEvent("drop") as DragEvent;
    Object.defineProperty(evt, "clientX", { value: 500 });
    Object.defineProperty(evt, "clientY", { value: 400 });
    Object.defineProperty(evt, "currentTarget", { value: el });

    await onDrop(evt);

    const layers = ws.getEngine("docB")!.getLayers();
    expect(layers).toHaveLength(before + 1);
    // cursor (500,400) is the layer CENTER; source "Logo" is doc-sized 800x600
    // → top-left = (500-400, 400-300) = (100,100). Find the newly added copy
    // (a different id than the source layer).
    const added = layers.find((l) => l.name === "Logo" && l.id !== src.id)!;
    expect(added).toBeDefined();
    expect(added.transform.x).toBe(100);
    expect(added.transform.y).toBe(100);
    expect(dc.endDrag).toHaveBeenCalled();
    // uploadImage was called for the NEW layer (cross-doc → new id ≠ payload's)
    expect(uploadImage).toHaveBeenCalledWith(added.id, added.imageBitmap);
    expect(requestRender).toHaveBeenCalled();
  });

  it("does not clear canvas drop target when dragLeave moves inside the element", () => {
    const dc = createDragController(fakeState({ dragKind: "layer", dropTarget: { type: "canvas" } }));
    const { onDragLeave } = useCanvasDrop({
      dragController: dc,
      camera: createCamera(),
      workspace: {} as any,
      renderer: {} as any,
      scheduler: { requestRender: vi.fn() },
    });

    const el = document.createElement("div");
    const child = document.createElement("span");
    el.appendChild(child);
    const evt = dragEvent("dragleave");
    Object.defineProperty(evt, "currentTarget", { value: el });
    Object.defineProperty(evt, "relatedTarget", { value: child });

    onDragLeave(evt as DragEvent);
    // relatedTarget is a child of currentTarget → still inside the element
    expect(dc.setDropTarget).not.toHaveBeenCalledWith(null);
  });
});
