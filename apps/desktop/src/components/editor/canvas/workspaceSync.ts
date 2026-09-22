import { batch } from "solid-js";
import type { WorkspaceManager } from "@/engine/workspace";
import type { RenderScheduler } from "@/renderer/scheduler";
import type { DocumentTabSummary, LayerNode, SelectionState } from "@/engine/types";
import { ViewportCamera } from "@/viewport/viewportCamera";
import type { HistoryItem } from "@/engine/history";

interface SyncStateParams {
  workspace: WorkspaceManager;
  camera: ViewportCamera;
  setDocuments: (docs: DocumentTabSummary[]) => void;
  setActiveDocumentId: (id: string | null) => void;
  setLayers: (layers: LayerNode[]) => void;
  setActiveLayerId: (id: string | null) => void;
  selectedLayerIds?: () => string[];
  rawSetSelectedLayerId?: (id: string | null) => void;
  setSelectedLayerId: (id: string | null) => void;
  setSelection: (sel: SelectionState | null) => void;
  setSelectionEditMode: (edit: boolean) => void;
  setDocWidth: (width: number) => void;
  setDocHeight: (height: number) => void;
  setZoom: (zoom: number) => void;
  setPan: (pan: { x: number; y: number }) => void;
  scheduler: RenderScheduler;
  setHistoryItems: (items: HistoryItem[]) => void;
  setActiveHistoryIndex: (index: number) => void;
}

export function setupWorkspaceSync(params: SyncStateParams) {
  let lastDocId: string | null = null;
  // Identity cache for the published layer rows. Reorder drop math reads
  // `data-layer-idx` from live DOM rows and the panel harness assumes a
  // reorder recreates rows, so ANY id-sequence change (add/remove/reorder)
  // emits fresh row objects for every row and re-anchors the cache. Toggles
  // never reorder, so the same-sequence path reuses the previous row object
  // on field equality and only the edited row is recreated. Every sync still
  // publishes a new array so the signal fires.
  let prevRows: LayerNode[] | null = null;

  const copyLayer = (l: LayerNode): LayerNode => ({
    ...l,
    transform: { ...l.transform },
    // Nested params are small plain objects: copy by value so a later
    // in-place engine mutation can never alias a cached row.
    ...(l.basicAdjustment ? { basicAdjustment: { ...l.basicAdjustment } } : null),
    ...(l.shapeParams
      ? {
          shapeParams: {
            ...l.shapeParams,
            fill: { ...l.shapeParams.fill },
            stroke: { ...l.shapeParams.stroke },
          },
        }
      : null),
    ...(l.textData ? { textData: { ...l.textData, stroke: { ...l.textData.stroke } } } : null),
  });

  const rowsEqual = (a: LayerNode, b: LayerNode): boolean =>
    a.id === b.id &&
    a.name === b.name &&
    a.type === b.type &&
    a.visible === b.visible &&
    a.opacity === b.opacity &&
    a.locked === b.locked &&
    a.isBackground === b.isBackground &&
    a.lockTransparency === b.lockTransparency &&
    a.lockPosition === b.lockPosition &&
    a.lockRotation === b.lockRotation &&
    a.hasAdjustments === b.hasAdjustments &&
    a.blendMode === b.blendMode &&
    a.width === b.width &&
    a.height === b.height &&
    a.imageBitmap === b.imageBitmap &&
    a.baseImageBitmap === b.baseImageBitmap &&
    a.bitmapEpoch === b.bitmapEpoch &&
    a.transform.x === b.transform.x &&
    a.transform.y === b.transform.y &&
    a.transform.scaleX === b.transform.scaleX &&
    a.transform.scaleY === b.transform.scaleY &&
    a.transform.rotation === b.transform.rotation &&
    a.transform.flipH === b.transform.flipH &&
    a.transform.flipV === b.transform.flipV &&
    (a.basicAdjustment?.brightness ?? 0) === (b.basicAdjustment?.brightness ?? 0) &&
    (a.basicAdjustment?.contrast ?? 0) === (b.basicAdjustment?.contrast ?? 0) &&
    (a.basicAdjustment?.saturation ?? 0) === (b.basicAdjustment?.saturation ?? 0) &&
    JSON.stringify(a.shapeParams ?? null) === JSON.stringify(b.shapeParams ?? null) &&
    JSON.stringify(a.textData ?? null) === JSON.stringify(b.textData ?? null);

  const syncState = () => {
    batch(() => {
      params.setDocuments(params.workspace.getTabSummaries());
      const activeId = params.workspace.getActiveDocumentId();
      const docChanged = activeId !== lastDocId;
      lastDocId = activeId;
      if (docChanged) prevRows = null;
      params.setActiveDocumentId(activeId);

      const engine = params.workspace.getActiveEngine();
      if (engine) {
        const live = engine.getLayers();
        let next: LayerNode[];
        if (
          prevRows !== null &&
          prevRows.length === live.length &&
          prevRows.every((r, i) => r.id === live[i].id)
        ) {
          next = live.map((l, i) => (rowsEqual(prevRows![i], l) ? prevRows![i] : copyLayer(l)));
        } else {
          next = live.map(copyLayer);
        }
        prevRows = next;
        params.setLayers(next);
        const activeLayerId = engine.getActiveLayerId();
        params.setActiveLayerId(activeLayerId);
        const currentMulti = params.selectedLayerIds ? params.selectedLayerIds() : [];
        if (!docChanged && activeLayerId && currentMulti.includes(activeLayerId)) {
          params.rawSetSelectedLayerId?.(activeLayerId);
        } else {
          params.setSelectedLayerId(activeLayerId);
        }
        const newSel = engine.getSelection() ? { ...engine.getSelection()! } : null;
        params.setSelection(newSel);
        // Auto-disable edit mode when selection is cleared
        if (!newSel) {
          params.setSelectionEditMode(false);
        }
        params.setDocWidth(engine.getWidth());
        params.setDocHeight(engine.getHeight());
      } else {
        prevRows = null;
        params.setLayers([]);
        params.setActiveLayerId(null);
        params.setSelectedLayerId(null);
        params.setSelection(null);
        params.setSelectionEditMode(false);
      }

      const history = params.workspace.getActiveHistory();
      if (history) {
        params.setHistoryItems(history.getHistoryStack());
        params.setActiveHistoryIndex(history.getUndoCount());
      } else {
        params.setHistoryItems([]);
        params.setActiveHistoryIndex(0);
      }
    });
  };

  // Track the last-known engine viewport state so syncViewport only
  // overwrites the camera when the engine viewport was *intentionally*
  // changed (new doc, fit-to-screen, zoom shortcut, etc.) — NOT when
  // a stale onChange fires mid-drag after panning skipped syncFromCamera.
  // See commit 4680973 + b53417e (direct signal updates during panning).
  let lastVp = { panX: 0, panY: 0, zoom: 1 };

  const syncViewport = () => {
    const engine = params.workspace.getActiveEngine();
    if (!engine) return;
    const vp = engine.getViewport();
    // Bail if the engine viewport hasn't changed — avoids overwriting
    // the camera with stale values during rotation/resize/move drags.
    if (vp.panX === lastVp.panX && vp.panY === lastVp.panY && vp.zoom === lastVp.zoom) return;
    lastVp = { panX: vp.panX, panY: vp.panY, zoom: vp.zoom };
    params.camera.setState({ x: vp.panX, y: vp.panY, zoom: vp.zoom });
    params.setZoom(vp.zoom);
    params.setPan({ x: vp.panX, y: vp.panY });
  };

  params.workspace.onChange(() => {
    syncState();
    syncViewport();
  });

  params.workspace.onVisualChange(() => {
    syncState();
    params.scheduler.requestRender();
  });

  return {
    syncState,
    syncViewport,
  };
}
