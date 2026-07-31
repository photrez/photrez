// apps/desktop/src/components/editor/__tests__/crossDocDragDropWiring.test.tsx
//
// Wiring contract tests for cross-document drag-and-drop.
//
// What this catches: the "tests pass but app fails" pattern. Pure-function
// unit tests for addLayerFromCrossDoc / addFilesAsLayers / createNewDocsFromFiles
// pass, but the *wiring* from real user input to those functions is broken
// (dragController state never set, drop handlers reading wrong state, etc).
//
// This file tests the wiring that connects:
//   - In-app layer drag: LayerItem.onDragStart -> dragController.beginLayerDrag
//   - Tab drop handling for layer drags (copy / move / same-doc reorder)
//   - Hover-to-switch: dragover on a tab for 500ms switches the active doc
//   - HTML5 OS file drop: DragGlobalGuard sets dragKind=file, drop handlers
//     read e.dataTransfer.files and call addFilesAsLayersFromFileDrop
//
// If any of these wirings break, the feature silently no-ops in the real app.
// See AI_HISTORY [2026-06-16] BUG FIX - Cross-Doc Drag-Drop Wiring.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { WorkspaceManager } from "@/engine/workspace";
import { LayerItem } from "../layers/LayerItem";
import { DocumentTabsBar } from "../shell/DocumentTabsBar";
import { DragControllerProvider, useDragController } from "../DragController";
import { addFilesAsLayersFromFileDrop } from "../crossDocLayerOps";
import { resetToasts } from "../Toast";
import type { LayerNode } from "@/engine/types";

// ---------------------------------------------------------------------------
//  In-app layer drag wiring: LayerItem.onDragStart must call
//  dragController.beginLayerDrag so drop zones can read state.payload.
//  This was the OTHER half of the "feature doesn't work in real app" bug.
// ---------------------------------------------------------------------------

describe("LayerItem wiring (in-app layer drag)", () => {
  let container: HTMLDivElement;
  let dispose: () => void;
  let probeRef: { current: ReturnType<typeof useDragController> | null };

  const mockLayer: LayerNode = {
    id: "layer-1",
    name: "Layer 1",
    type: "raster",
    visible: true,
    opacity: 1,
    locked: false,
    blendMode: "normal",
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    width: 100,
    height: 100,
    imageBitmap: null,
  };

  function renderLayer() {
    probeRef = { current: null };
    container = document.createElement("div");
    document.body.appendChild(container);
    const Probe = () => {
      probeRef!.current = useDragController();
      return null;
    };
    dispose = render(
      () => (
        <DragControllerProvider workspaceOverride={{ switchDocument: vi.fn() }}>
          <LayerItem
            layer={mockLayer}
            idx={0}
            isActive={false}
            isEditing={false}
            editName=""
            setEditingLayerId={vi.fn()}
            setEditName={vi.fn()}
            onSelect={vi.fn()}
            onToggleVisibility={vi.fn()}
            onToggleLock={vi.fn()}
            onMoveUp={vi.fn()}
            onMoveDown={vi.fn()}
            layersLength={1}
            workspace={{} as any}
            scheduler={{} as any}
            activeDocumentId="doc-source"
          />
          <Probe />
        </DragControllerProvider>
      ),
      container,
    );
  }

  function fireDragStart(el: Element, altKey = false) {
    const dt = {
      setData: vi.fn(),
      effectAllowed: "",
    } as any;
    const evt = new Event("dragstart", { bubbles: true, cancelable: true }) as any;
    evt.dataTransfer = dt;
    evt.altKey = altKey;
    el.dispatchEvent(evt);
    return dt;
  }

  it("onDragStart calls dragController.beginLayerDrag with the layer payload", { timeout: 15000 }, () => {
    renderLayer();
    const layerEl = container.querySelector("[data-layer-idx='0']") as HTMLElement;
    expect(layerEl).not.toBeNull();
    const dt = fireDragStart(layerEl);
    const state = probeRef.current!.state();
    expect(state.dragKind).toBe("layer");
    expect(dt.effectAllowed).toBe("copyMove");
    expect(state.payload).toEqual({
      version: 1,
      sourceDocId: "doc-source",
      layerId: "layer-1",
      sourceName: "Layer 1",
      isAltPressed: false,
    });
  });

  it("onDragStart with Alt pressed sets isAltPressed=true (for Move vs Copy)", () => {
    renderLayer();
    const layerEl = container.querySelector("[data-layer-idx='0']") as HTMLElement;
    const dt = fireDragStart(layerEl, true);
    expect(probeRef.current!.state().payload?.isAltPressed).toBe(true);
    expect(dt.effectAllowed).toBe("copyMove");
  });

  it("onDragEnd clears dragController state (prevent orphan state)", () => {
    renderLayer();
    const layerEl = container.querySelector("[data-layer-idx='0']") as HTMLElement;
    fireDragStart(layerEl);
    expect(probeRef.current!.state().dragKind).toBe("layer");
    layerEl.dispatchEvent(new Event("dragend", { bubbles: true }));
    expect(probeRef.current!.state().dragKind).toBeNull();
  });

  it("onDragStart on locked layer does NOT begin a drag (early return)", () => {
    const lockedLayer: LayerNode = { ...mockLayer, locked: true };
    probeRef = { current: null };
    container = document.createElement("div");
    document.body.appendChild(container);
    const Probe = () => {
      probeRef!.current = useDragController();
      return null;
    };
    dispose = render(
      () => (
        <DragControllerProvider workspaceOverride={{ switchDocument: vi.fn() }}>
          <LayerItem
            layer={lockedLayer}
            idx={0}
            isActive={false}
            isEditing={false}
            editName=""
            setEditingLayerId={vi.fn()}
            setEditName={vi.fn()}
            onSelect={vi.fn()}
            onToggleVisibility={vi.fn()}
            onToggleLock={vi.fn()}
            onMoveUp={vi.fn()}
            onMoveDown={vi.fn()}
            layersLength={1}
            workspace={{} as any}
            scheduler={{} as any}
            activeDocumentId="doc-source"
          />
          <Probe />
        </DragControllerProvider>
      ),
      container,
    );
    const layerEl = container.querySelector("[data-layer-idx='0']") as HTMLElement;
    // Locked layer is not draggable (LayerItem sets draggable={!locked}),
    // so the onDragStart should be a no-op even if dispatched manually.
    fireDragStart(layerEl);
    expect(probeRef.current!.state().dragKind).toBeNull();
  });
});

// ---------------------------------------------------------------------------
//  DocumentTabsBar wiring: handleTabDrop must handle BOTH file drag AND
//  layer drag. The previous code only handled file drag -> layer drop on
//  tab was a silent no-op even though state.dragKind === "layer".
// ---------------------------------------------------------------------------

describe("DocumentTabsBar wiring (tab drop with layer drag)", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;
  let probeRef: { current: ReturnType<typeof useDragController> | null };

  function renderTabs() {
    ws = new WorkspaceManager();
    const a = WorkspaceManager.createBlankDocument("doc-a", "A", 800, 600);
    const b = WorkspaceManager.createBlankDocument("doc-b", "B", 800, 600);
    ws.addDocument(a);
    ws.addDocument(b);
    ws.switchDocument("doc-a");
    // Add a layer to doc-a so we have something to drag
    const dragMeLayer = a.engine.addLayer("Drag Me");
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    probeRef = { current: null };
    container = document.createElement("div");
    document.body.appendChild(container);
    const Probe = () => {
      probeRef!.current = useDragController();
      return null;
    };
    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <DocumentTabsBar />
          <Probe />
        </EditorProvider>
      ),
      container,
    );
    return { dragMeLayerId: dragMeLayer.id };
  }

  const tick = (ms = 50) => new Promise<void>((r) => setTimeout(r, ms));

  function fireDrop(tabEl: Element) {
    const dt = { getData: vi.fn(), setData: vi.fn() } as any;
    const evt = new Event("drop", { bubbles: true, cancelable: true }) as any;
    evt.dataTransfer = dt;
    tabEl.dispatchEvent(evt);
  }

  it("drop on tab with layer state copies layer to target doc (the missing wiring)", async () => {
    const { dragMeLayerId } = renderTabs();
    await tick();

    probeRef.current!.beginLayerDrag(
      {
        version: 1,
        sourceDocId: "doc-a",
        layerId: dragMeLayerId,
        sourceName: "Drag Me",
        isAltPressed: false,
      },
      null,
    );

    const tabEl = container.querySelector('[data-document-tab="doc-b"]') as HTMLElement;
    expect(tabEl).not.toBeNull();
    fireDrop(tabEl);
    await tick();

    // The target doc should now have a new layer (bg + 1 copied layer = 2)
    const targetEngine = ws.getEngine("doc-b")!;
    expect(targetEngine.getLayers().length).toBe(2);
    // Source unchanged (default = copy, not move)
    expect(ws.getEngine("doc-a")!.getLayers().length).toBe(2);
    // The new layer is inserted above the bg (active layer); find by name to be
    // position-independent.
    const copied = targetEngine.getLayers().find(l => l.name === "Drag Me");
    expect(copied).toBeDefined();
  });

  it("drop on tab with Alt+drag MOVES layer from source to target", async () => {
    const { dragMeLayerId } = renderTabs();
    await tick();

    probeRef.current!.beginLayerDrag(
      {
        version: 1,
        sourceDocId: "doc-a",
        layerId: dragMeLayerId,
        sourceName: "Drag Me",
        isAltPressed: true,
      },
      null,
    );

    const tabEl = container.querySelector('[data-document-tab="doc-b"]') as HTMLElement;
    fireDrop(tabEl);
    await tick();

    // Source loses the layer (move)
    expect(ws.getEngine("doc-a")!.getLayers().length).toBe(1);
    // Target gains it (find by name to be position-independent)
    expect(ws.getEngine("doc-b")!.getLayers().length).toBe(2);
    expect(ws.getEngine("doc-b")!.getLayers().some(l => l.name === "Drag Me")).toBe(true);
  });

  it("same-doc tab drop is a reorder and does NOT call uploadImage (guard)", async () => {
    const { dragMeLayerId } = renderTabs();
    await tick();

    // Set a bitmap so the guard (not bitmap absence) skips uploadImage
    const engineA = ws.getEngine("doc-a")!;
    const fakeBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engineA.setLayerImageBitmap(dragMeLayerId, fakeBitmap);
    vi.clearAllMocks();

    probeRef.current!.beginLayerDrag(
      {
        version: 1,
        sourceDocId: "doc-a",
        layerId: dragMeLayerId,
        sourceName: "Drag Me",
        isAltPressed: false,
      },
      null,
    );

    const sameTabEl = container.querySelector('[data-document-tab="doc-a"]') as HTMLElement;
    fireDrop(sameTabEl);
    await tick();

    // Both should be unchanged (same-doc reorder preserves layer count)
    expect(ws.getEngine("doc-a")!.getLayers().length).toBe(2);
    expect(ws.getEngine("doc-b")!.getLayers().length).toBe(1);
    // Same-doc -> same layer id -> guard skips upload
    expect(renderer.uploadImage).not.toHaveBeenCalled();
  });

  it("cross-doc tab drop with bitmap calls uploadImage and requestRender", async () => {
    const { dragMeLayerId } = renderTabs();
    await tick();

    // Give the source layer a real bitmap so uploadImage is exercised
    const engineA = ws.getEngine("doc-a")!;
    const fakeBitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engineA.setLayerImageBitmap(dragMeLayerId, fakeBitmap);
    vi.clearAllMocks();

    probeRef.current!.beginLayerDrag(
      {
        version: 1,
        sourceDocId: "doc-a",
        layerId: dragMeLayerId,
        sourceName: "Drag Me",
        isAltPressed: false,
      },
      null,
    );

    const tabB = container.querySelector('[data-document-tab="doc-b"]') as HTMLElement;
    fireDrop(tabB);
    await tick();

    // Target doc gained a new layer
    const targetEngine = ws.getEngine("doc-b")!;
    expect(targetEngine.getLayers().length).toBe(2);
    const copied = targetEngine.getLayers().find(l => l.name === "Drag Me")!;
    expect(copied).toBeDefined();
    // uploadImage called for the new (different id) layer's bitmap
    expect(renderer.uploadImage).toHaveBeenCalledWith(copied.id, copied.imageBitmap);
    expect(scheduler.requestRender).toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
//  Hover-to-switch: dragover on a tab for 500ms must switch the active
//  document. This is the feature: "pas di drag ke tab maka akan terbuka
//  document yang satunya".
// ---------------------------------------------------------------------------

describe("DocumentTabsBar wiring (hover-to-switch)", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;
  let probeRef: { current: any };

  function renderTabs() {
    ws = new WorkspaceManager();
    const a = WorkspaceManager.createBlankDocument("doc-a", "A", 800, 600);
    const b = WorkspaceManager.createBlankDocument("doc-b", "B", 800, 600);
    ws.addDocument(a);
    ws.addDocument(b);
    ws.switchDocument("doc-a");
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    probeRef = { current: null };
    container = document.createElement("div");
    document.body.appendChild(container);
    const Probe = () => {
      probeRef.current = { drag: useDragController(), editor: useEditor() };
      return null;
    };
    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <DocumentTabsBar />
          <Probe />
        </EditorProvider>
      ),
      container,
    );
  }

  function fireDragOver(tabEl: Element) {
    const dt = { types: ["application/x-photrez-layer"], setData: vi.fn() } as any;
    const evt = new Event("dragover", { bubbles: true, cancelable: true }) as any;
    evt.dataTransfer = dt;
    tabEl.dispatchEvent(evt);
  }

  it("dragover on a different tab sets dropTarget to that tab", async () => {
    renderTabs();
    const tabEl = container.querySelector('[data-document-tab="doc-b"]') as HTMLElement;
    fireDragOver(tabEl);
    const dropTarget = probeRef.current.drag.state().dropTarget;
    expect(dropTarget).toEqual({ type: "tab", docId: "doc-b" });
  });

  it("hovering over a different tab for 500ms switches through the real EditorProvider workspace", async () => {
    vi.useFakeTimers();
    try {
      renderTabs();
      expect(ws.getActiveDocumentId()).toBe("doc-a");

      const tabEl = container.querySelector('[data-document-tab="doc-b"]') as HTMLElement;
      fireDragOver(tabEl);
      expect(probeRef.current.drag.state().hoverTabId).toBe("doc-b");

      vi.advanceTimersByTime(500);
      expect(ws.getActiveDocumentId()).toBe("doc-b");
      expect(probeRef.current.drag.state().hoverTabId).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("dragleave on tab cancels the hover-to-switch timer", async () => {
    vi.useFakeTimers();
    try {
      renderTabs();
      const tabEl = container.querySelector('[data-document-tab="doc-b"]') as HTMLElement;
      fireDragOver(tabEl);
      expect(probeRef.current.drag.state().hoverTabId).toBe("doc-b");

      // Simulate leaving the tab - no relatedTarget means we go off the tab
      const leaveEvt = new Event("dragleave", { bubbles: true, cancelable: true }) as any;
      leaveEvt.relatedTarget = null;
      Object.defineProperty(leaveEvt, "currentTarget", { value: tabEl });
      tabEl.dispatchEvent(leaveEvt);

      expect(probeRef.current.drag.state().hoverTabId).toBeNull();

      vi.advanceTimersByTime(500);
      expect(ws.getActiveDocumentId()).toBe("doc-a");
    } finally {
      vi.useRealTimers();
    }
  });
});

// ---------------------------------------------------------------------------
//  HTML5 file drop wiring - when DragController has dragKind=file with
//  empty filePaths (set by DragGlobalGuard), the CanvasViewport / LayersPanel
//  drop handler must read `e.dataTransfer.files` directly and call
//  addFilesAsLayersFromFileDrop.
// ---------------------------------------------------------------------------

describe("HTML5 file drop wiring (OS file drop on drop zone)", () => {
  let ws: WorkspaceManager;
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;
  let probeRef: { current: ReturnType<typeof useDragController> | null };

  function FileDropHost(props: { onFileDrop: typeof addFilesAsLayersFromFileDrop }) {
    const dragController = useDragController();
    return (
      <div
        data-test-drop-zone=""
        onDrop={async (e) => {
          e.preventDefault();
          const state = dragController.state();
          if (state.dragKind === "file") {
            if (e.dataTransfer?.files && e.dataTransfer.files.length > 0) {
              const files = Array.from(e.dataTransfer.files);
              // Mirror production handler: CanvasViewport + LayersPanel
              await props.onFileDrop(files, { type: "canvas" }, { x: 0, y: 0 }, ws);
            }
          }
          dragController.endDrag();
        }}
      />
    );
  }

  beforeEach(() => {
    ws = new WorkspaceManager();
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-a", "A", 800, 600));
    renderer = { uploadImage: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    probeRef = { current: null };
    container = document.createElement("div");
    document.body.appendChild(container);
  });

  afterEach(() => {
    dispose?.();
    document.body.replaceChildren();
  });

  function Probe() {
    probeRef.current = useDragController();
    return null;
  }

  function fireDrop(el: Element, files: File[]) {
    const evt = new Event("drop", { bubbles: true, cancelable: true }) as any;
    evt.dataTransfer = { files };
    el.dispatchEvent(evt);
    return evt;
  }

  it("calls addFilesAsLayersFromFileDrop when dragKind=file and OS files are dropped", async () => {
    const onFileDrop = vi.fn(addFilesAsLayersFromFileDrop);
    const created = vi.fn().mockResolvedValue([]);
    onFileDrop.mockImplementation(created);

    dispose = render(
      () => (
        <DragControllerProvider workspaceOverride={{ switchDocument: vi.fn() }}>
          <FileDropHost onFileDrop={onFileDrop} />
          <Probe />
        </DragControllerProvider>
      ),
      container,
    );

    // Simulate DragGlobalGuard having set this on dragover
    probeRef.current!.beginFileDrag([], { x: 100, y: 200 });

    const el = container.querySelector("[data-test-drop-zone]")!;
    const files = [new File(["fake"], "photo.png", { type: "image/png" })];
    fireDrop(el, files);

    // Wait for async handler to settle
    await new Promise((r) => setTimeout(r, 50));

    expect(onFileDrop).toHaveBeenCalledWith(
      files,
      { type: "canvas" },
      { x: 0, y: 0 },
      ws,
    );
    // endDrag was called - dragKind is cleared
    expect(probeRef.current!.state().dragKind).toBeNull();
  });

  it("does NOT call addFilesAsLayersFromFileDrop when dragKind is 'layer' (type guard)", async () => {
    const onFileDrop = vi.fn(addFilesAsLayersFromFileDrop);
    const created = vi.fn().mockResolvedValue([]);
    onFileDrop.mockImplementation(created);

    dispose = render(
      () => (
        <DragControllerProvider workspaceOverride={{ switchDocument: vi.fn() }}>
          <FileDropHost onFileDrop={onFileDrop} />
          <Probe />
        </DragControllerProvider>
      ),
      container,
    );

    // Set layer drag state (not file)
    probeRef.current!.beginLayerDrag(
      { version: 1, sourceDocId: "doc-a", layerId: "l1", sourceName: "L1", isAltPressed: false },
      null,
    );

    const el = container.querySelector("[data-test-drop-zone]")!;
    fireDrop(el, [new File(["fake"], "photo.png")]);

    await new Promise((r) => setTimeout(r, 50));

    expect(onFileDrop).not.toHaveBeenCalled();
    // endDrag was still called
    expect(probeRef.current!.state().dragKind).toBeNull();
  });
});
