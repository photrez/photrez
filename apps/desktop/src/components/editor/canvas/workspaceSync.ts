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

  const syncState = () => {
    batch(() => {
      params.setDocuments(params.workspace.getTabSummaries());
      const activeId = params.workspace.getActiveDocumentId();
      const docChanged = activeId !== lastDocId;
      lastDocId = activeId;
      params.setActiveDocumentId(activeId);

      const engine = params.workspace.getActiveEngine();
      if (engine) {
        params.setLayers(engine.getLayers().map(l => ({ ...l, transform: { ...l.transform } })));
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
