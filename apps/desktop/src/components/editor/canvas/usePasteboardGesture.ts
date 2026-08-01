// SPDX-License-Identifier: AGPL-3.0-or-later
// Pasteboard (empty canvas area) pointer gesture handling for CanvasViewport.
// Extracted from CanvasViewport.tsx (report #20 phase 3) — behavior must stay
// identical to the inline handlers it replaces.

import { createSignal } from "solid-js";
import type { CropPreview } from "../cropState";
import type { ToolId } from "../tools/toolTypes";
import { getPasteboardClickAction } from "../tools/pasteboardClickPolicy";
import { hitTestLayers, type LayerInfo } from "@/viewport/layerHitTest";
import {
  hasCropReplacementDragDistance,
  createCropRectFromDocumentPoints,
  hideCropPreview,
} from "../cropToolActions";
import type { DocumentEngine } from "@/engine/document";
import type { SnapLine } from "@/viewport/smartGuides";
import type { RenderScheduler } from "@/renderer/scheduler";

interface PasteboardGestureState {
  pointerId: number;
  startClient: { clientX: number; clientY: number };
  startDocument: { x: number; y: number };
  replacementStarted: boolean;
}

interface SelectionPreviewBox {
  x: number;
  y: number;
  w: number;
  h: number;
  angle: number;
  shape?: "rect" | "ellipse";
  inverted?: boolean;
}

export interface UsePasteboardGestureParams {
  getCanvasContainerRef: () => HTMLElement | null;
  getCanvasRef: () => HTMLElement | null;
  isSpacePressed: () => boolean;
  isPanning: () => boolean;
  activeTool: () => ToolId;
  cropRect: () => { x: number; y: number; w: number; h: number } | null;
  cropRotation: () => number;
  hiddenCropPreview: () => CropPreview | null;
  cropInteractionMode: () => string;
  docWidth: () => number;
  docHeight: () => number;
  moveAutoSelect: () => boolean;
  layerTransformSession: () => unknown;
  selectionBox: () => SelectionPreviewBox | null;
  selectedLayerId: () => string | null;
  getEngine: () => DocumentEngine | null;
  screenToDocumentPoint: (e: PointerEvent) => { x: number; y: number };
  onCanvasPointerDown: (e: PointerEvent) => void;
  setSelectedLayerId: (id: string | null) => void;
  setSelectionBoxSignal: (box: SelectionPreviewBox | null) => void;
  setHoverHandle: (h: string | null) => void;
  setSnapLines: (lines: SnapLine[]) => void;
  setHudInfo: (hud: null) => void;
  setCropRect: (rect: { x: number; y: number; w: number; h: number } | null) => void;
  setCropRotation: (rot: number) => void;
  setHiddenCropPreview: (preview: CropPreview | null) => void;
  scheduler: RenderScheduler;
}

export function usePasteboardGesture(params: UsePasteboardGestureParams) {
  const [pendingPasteboardCropGesture, setPendingPasteboardCropGesture] =
    createSignal<PasteboardGestureState | null>(null);

  const isPasteboardPointerDown = (e: PointerEvent) => {
    if (e.target === params.getCanvasContainerRef()) return true;
    // The SVG overlay (z-index 40, full viewport) sits on top of the canvas.
    // Clicks outside the document bounds that land on the SVG are pasteboard clicks.
    const target = e.target as Element | null;
    if (target?.closest?.("[data-overlay-svg]")) {
      if (params.activeTool() === "crop" && !params.cropRect()) return false;
      const point = params.screenToDocumentPoint(e);
      return (
        point.x < 0 ||
        point.y < 0 ||
        point.x > params.docWidth() ||
        point.y > params.docHeight()
      );
    }
    if (e.target === params.getCanvasRef()) {
      if (params.activeTool() === "crop" && !params.cropRect()) return false;
      const point = params.screenToDocumentPoint(e);
      return (
        point.x < 0 ||
        point.y < 0 ||
        point.x > params.docWidth() ||
        point.y > params.docHeight()
      );
    }
    // In Modern crop mode, the SVG overlay (z-index 40, full viewport)
    // captures pasteboard clicks. Route them to the canvas handler.
    if (params.activeTool() === "crop" && params.cropInteractionMode() === "modern") {
      if (!target?.closest) return false;
      if (!target.closest("[data-modern-crop-overlay]")) return false;
      // If the click hit an interactive child (handle, move rect, rotate ring),
      // it's a frame interaction, not a pasteboard click.
      return !target.closest(
        "[data-modern-crop-handle], [data-modern-crop-move], [data-modern-crop-rotate]",
      );
    }
    return false;
  };

  const handleMoveAutoSelect = (e: PointerEvent) => {
    if (params.activeTool() !== "move") return;
    if (!params.moveAutoSelect()) return;
    if (params.isSpacePressed() || params.isPanning()) return;
    const target = e.target as Element | null;

    const isSvgOverlayClick = target?.closest?.("[data-overlay-svg]");
    if (isSvgOverlayClick) {
      // Only intercept clicks on the SVG root (not on child elements like
      // move rect/handles which have their own onPointerDown).
      if (target !== target?.closest?.("[data-overlay-svg]")) return;
    } else {
      // If there is no SVG overlay, allow clicks directly on the canvas or container
      if (target !== params.getCanvasRef() && target !== params.getCanvasContainerRef()) return;
    }

    const engine = params.getEngine();
    if (!engine) return;

    const coords = params.screenToDocumentPoint(e);
    const docW = params.docWidth();
    const docH = params.docHeight();
    if (coords.x < 0 || coords.y < 0 || coords.x > docW || coords.y > docH)
      return;

    const allLayers = [...engine.getLayers()];
    const hit = hitTestLayers(coords, allLayers as LayerInfo[], (id, x, y) => engine.sampleLayerAlpha(id, x, y));
    if (hit && hit.id !== params.selectedLayerId()) {
      engine.setActiveLayer(hit.id);
      params.setSelectedLayerId(hit.id);
      params.scheduler.requestRender();
    } else if (!hit) {
      params.setSelectedLayerId(null);
    }
  };

  const handlePasteboardPointerDown = (e: PointerEvent) => {
    if (!isPasteboardPointerDown(e)) return;
    if (e.button !== 0) return;

    if (params.activeTool() === "crop") {
      if (params.isSpacePressed() || params.isPanning()) return;
      if (params.cropInteractionMode() === "modern") {
        // Route to canvas handler →it calls canvas.setPointerCapture()
        // and tracks modernDragStart for drag-to-create. After canvas
        // captures the pointer, subsequent events go to canvas handlers.
        params.onCanvasPointerDown(e);
        e.preventDefault();
        e.stopPropagation();
        return;
      }

      const startDocument = params.screenToDocumentPoint(e);
      setPendingPasteboardCropGesture({
        pointerId: e.pointerId,
        startClient: { clientX: e.clientX, clientY: e.clientY },
        startDocument,
        replacementStarted: false,
      });
      (e.currentTarget as HTMLElement)?.setPointerCapture(e.pointerId);
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    const engine = params.getEngine();
    const action = getPasteboardClickAction({
      hasDocument: Boolean(engine),
      activeTool: params.activeTool(),
      isNavigationMode: params.isSpacePressed() || params.isPanning(),
      hasLayerTransformSession: Boolean(params.layerTransformSession()),
      hasCropRect: Boolean(params.cropRect()),
      hasSelectionPreview: Boolean(params.selectionBox()),
    });

    if (action === "noop") return;

    e.preventDefault();
    e.stopPropagation();

    if (action === "clear-active-layer" && engine) {
      params.setSelectedLayerId(null);
      params.setHoverHandle(null);
      params.setSnapLines([]);
      params.setHudInfo(null);
      params.scheduler.requestRender();
      return;
    }

    if (action === "clear-selection-preview") {
      params.setSelectionBoxSignal(null);
      engine?.clearSelection();
      params.setSnapLines([]);
      params.setHudInfo(null);
      params.scheduler.requestRender();
      return;
    }
  };

  const handlePasteboardPointerMove = (e: PointerEvent) => {
    const pending = pendingPasteboardCropGesture();
    if (pending && e.pointerId === pending.pointerId) {
      if (!hasCropReplacementDragDistance(pending.startClient, e)) {
        return;
      }

      const nextRect = createCropRectFromDocumentPoints(
        pending.startDocument,
        params.screenToDocumentPoint(e),
      );
      if (!nextRect) {
        return;
      }

      params.setHiddenCropPreview(null);
      params.setCropRotation(0);
      params.setCropRect(nextRect);
      setPendingPasteboardCropGesture({ ...pending, replacementStarted: true });
      e.preventDefault();
    }
  };

  const handlePasteboardPointerUp = (e: PointerEvent) => {
    const pending = pendingPasteboardCropGesture();
    if (pending && e.pointerId === pending.pointerId) {
      setPendingPasteboardCropGesture(null);
      try {
        (e.currentTarget as HTMLElement)?.releasePointerCapture(e.pointerId);
      } catch {}

      if (
        !pending.replacementStarted &&
        !hasCropReplacementDragDistance(pending.startClient, e)
      ) {
        hideCropPreview({
          cropRect: params.cropRect,
          cropRotation: params.cropRotation,
          hiddenCropPreview: params.hiddenCropPreview,
          setCropRect: params.setCropRect,
          setCropRotation: params.setCropRotation,
          setHiddenCropPreview: params.setHiddenCropPreview,
        });
        params.setHoverHandle(null);
        params.setSnapLines([]);
        params.setHudInfo(null);
        params.scheduler.requestRender();
      }

      e.preventDefault();
    }
  };

  const handlePasteboardPointerCancel = (e: PointerEvent) => {
    const pending = pendingPasteboardCropGesture();
    if (pending && e.pointerId === pending.pointerId) {
      setPendingPasteboardCropGesture(null);
    }
  };

  return {
    handlePasteboardPointerDown,
    handlePasteboardPointerMove,
    handlePasteboardPointerUp,
    handlePasteboardPointerCancel,
    handleMoveAutoSelect,
  };
}
