import { createMemo, createSignal, createEffect, Show } from "solid-js";
import { screenToDocument } from "@/viewport/coords";
import { computeSnapAdjustment } from "@/viewport/smartGuides";
import type { SnapRect } from "@/viewport/smartGuides";
import { getLayerAabb } from "@/viewport/transformGeometry";
import { hitTestLayers, type LayerInfo } from "@/viewport/layerHitTest";
import { useEditor } from "../shell/EditorContext";
import { useCanvasKeyboard } from "./useCanvasKeyboard";
import { useBrushOverlay } from "../useBrushOverlay";
import { usePanNavigation } from "./usePanNavigation";
import { useViewportRenderer } from "./useViewportRenderer";
import { useCanvasPointerTools } from "./useCanvasPointerTools";
import { useCanvasLayerDrag } from "../layers/useCanvasLayerDrag";
import { useCanvasDerivedState } from "./useCanvasDerivedState";
import { useDragController } from "../DragController";
import { useCanvasDrop } from "./useCanvasDrop";
import { ViewportCamera } from "@/viewport/viewportCamera";
import { SelectionTransformOverlay } from "../SelectionTransformOverlay";
import { HoverHighlight } from "./HoverHighlight";
import { SmartGuides } from "./SmartGuides";
import { BrushCursorOverlay } from "../BrushCursorOverlay";
import { CropOverlay } from "../CropOverlay";
import { ModernCropOverlay } from "../ModernCropOverlay";
import { TransformHud } from "../TransformHud";
import { SelectionRenderer } from "@/features/selection/SelectionRenderer";
import { BrushContextMenu } from "../BrushContextMenu";
import { CanvasContextMenu } from "./CanvasContextMenu";
import { getPasteboardClickAction } from "../tools/pasteboardClickPolicy";
import {
  docFrameToScreenFrame,
  getDefaultModernCropFrame,
  getModernCropApplyRotation,
  getModernCropImagePivot,
  getProjectedCanvasSize,
  modernFrameToCropRect,
  screenFrameToDocFrame,
} from "@/viewport/modernCropGeometry";
import { fitCropRectToAspect } from "@/viewport/cropAutoFit";
import {
  clearCropPreview,
  applyCropPreview,
  hideCropPreview,
  hasCropReplacementDragDistance,
  createCropRectFromDocumentPoints,
} from "../cropToolActions";

export function CanvasViewport() {
  const {
    workspace,
    renderer,
    camera,
    activeTool,
    activeDocumentId,
    zoom,
    pan,
    setViewportState,
    viewportWidth,
    viewportHeight,
    docWidth,
    docHeight,
    bgColor,
    setHoverHandle,
    syncViewport,
    moveSnapEnabled,
    layers,
    activeLayerId,
    cropRect,
    setCropRect,
    cropInteractionMode,
    setCropInteractionMode,
    cropMode,
    cropGuideMode,
    cropAspect,
    cropRotation,
    setCropRotation,
    modernCropFrame,
    setModernCropFrame,
    modernCropImageTransform,
    setModernCropImageTransform,
    resetModernCrop,
    commitModernCropState,
    hiddenCropPreview,
    setHiddenCropPreview,
    cropDeletePixels,
    cropFillEnabled,
    cropFillSource,
    cropFillCustomColor,
    cropSizeTarget,
    setCropAspect,
    setCropMode,
    setCropSizeTarget,
    clearCropStacks,
    setActiveTool,
    setSelectedLayerId,
    moveAutoSelect,
    selectedLayerId,
    layerTransformSession,
    showTransformControls,
    selection,
    selectionEditMode,
    setSelectionEditMode,
    scheduler,
    useGPUCameraForModernCrop,
  } = useEditor();

  const {
    onPaintStroke,
    commitBrushStroke,
    setOverlayCanvasRef,
    getOverlayCanvasRef,
  } = useBrushOverlay();

  let canvasContainerRef!: HTMLDivElement;
  let canvasRef!: HTMLCanvasElement;
  let lastModernCropSessionKey: string | null = null;

  const {
    isSpacePressed,
    setIsSpacePressed,
    isPanning,
    setIsPanning,
    stopMomentum,
    handleWheel,
    onViewportPointerDown,
    onViewportPointerMove,
    onViewportPointerUp,
    onViewportPointerCancel,
    onViewportLostPointerCapture,
  } = usePanNavigation({
    getCanvasContainerRef: () => canvasContainerRef,
    fitToScreenAndRender: () => fitToScreenAndRender(),
  });

  // Alt key state for eyedropper shortcut (Alt+Brush/Eraser)
  const [isAltPressed, setIsAltPressed] = createSignal(false);

  // Active crop handle drag state
  const [isCropDragging, setIsCropDragging] = createSignal(false);

  const [pendingPasteboardCropGesture, setPendingPasteboardCropGesture] =
    createSignal<{
      pointerId: number;
      startClient: { clientX: number; clientY: number };
      startDocument: { x: number; y: number };
      replacementStarted: boolean;
    } | null>(null);

  // Sync modern crop state to camera image transform.
  // The camera's VP matrix will include this transform, eliminating
  // the need for CSS transform on the canvas.
  createEffect(() => {
    if (!useGPUCameraForModernCrop()) {
      // Feature flag disabled: don't touch camera, let CSS handle it
      return;
    }

    const tool = activeTool();
    const mode = cropInteractionMode();

    if (tool !== "crop" || mode !== "modern") {
      camera.resetImageTransform();
      scheduler.requestRender();
      return;
    }

    const frame = modernCropFrame();
    const transform = modernCropImageTransform();

    if (!frame) {
      // No frame: apply offset + scale only (no rotation pivot)
      camera.setImageTransform({
        offsetX: transform.offsetX,
        offsetY: transform.offsetY,
        rotation: 0,
        scale: transform.scale,
        pivotScreen: null,
        pivotDocument: null,
      });
      scheduler.requestRender();
      return;
    }

    // With frame: compute pivot, apply full transform
    // getModernCropImagePivot expects screen-space frame
    const screenFrame = modernCropScreenFrame();
    const pivot = getModernCropImagePivot({
      frame: screenFrame!,
      viewport: {
        width: viewportWidth(),
        height: viewportHeight(),
        panX: pan().x,
        panY: pan().y,
        zoom: zoom(),
      },
      transform,
    });

    camera.setImageTransform({
      offsetX: transform.offsetX,
      offsetY: transform.offsetY,
      rotation: transform.rotation,
      scale: transform.scale,
      pivotScreen: pivot.screen,
      pivotDocument: pivot.document,
    });
    scheduler.requestRender();
  });

  // Modern crop CSS transform string (used only when feature flag is OFF)
  const modernImageTransformStyle = createMemo(() => {
    const frame = modernCropFrame();
    const transform = modernCropImageTransform();
    if (!frame) {
      return `translate3d(${pan().x + transform.offsetX}px, ${pan().y + transform.offsetY}px, 0) scale(${zoom() * transform.scale})`;
    }

    // getModernCropImagePivot expects screen-space frame
    const screenFrame = modernCropScreenFrame();
    const pivot = getModernCropImagePivot({
      frame: screenFrame!,
      viewport: {
        width: viewportWidth(),
        height: viewportHeight(),
        panX: pan().x,
        panY: pan().y,
        zoom: zoom(),
      },
      transform,
    });

    return [
      `translate3d(${pivot.screen.x}px, ${pivot.screen.y}px, 0)`,
      `rotate(${transform.rotation}deg)`,
      `scale(${zoom() * transform.scale})`,
      `translate3d(${-pivot.document.x}px, ${-pivot.document.y}px, 0)`,
    ].join(" ");
  });

  const resolvedCropFillColor = createMemo(() =>
    cropFillSource() === "background" ? bgColor() : cropFillCustomColor(),
  );

  const classicCropFillPreviewStyle = createMemo(() => {
    const rect = cropRect();
    if (!rect) return {};
    return {
      position: "absolute" as const,
      width: `${rect.w * zoom()}px`,
      height: `${rect.h * zoom()}px`,
      "background-color": resolvedCropFillColor(),
      transform: `translate(${pan().x + rect.x * zoom()}px, ${pan().y + rect.y * zoom()}px) rotate(${cropRotation()}deg)`,
      "transform-origin": "center",
      "pointer-events": "none" as const,
      "will-change": "transform",
    };
  });

  // Derived screen-space frame from doc-space frame + zoom + pan.
  // Used for rendering, pivot computation, and overlay positioning.
  const modernCropScreenFrame = createMemo(() => {
    return docFrameToScreenFrame(modernCropFrame(), zoom(), pan());
  });

  const modernCropFillPreviewStyle = createMemo(() => {
    const frame = modernCropScreenFrame();
    if (!frame) return {};
    return {
      position: "absolute" as const,
      left: `${frame.x}px`,
      top: `${frame.y}px`,
      width: `${frame.w}px`,
      height: `${frame.h}px`,
      "background-color": resolvedCropFillColor(),
      "pointer-events": "none" as const,
    };
  });

  const activeLayer = createMemo(() => {
    layers();
    const activeId = activeLayerId();
    if (!activeId) return null;
    const activeEngine = workspace.getActiveEngine();
    if (!activeEngine) return null;
    return activeEngine.getLayer(activeId);
  });

  const overlayCanvasStyleScreenSpace = createMemo(() => {
    const layer = activeLayer();
    const tool = activeTool();
    const isBrushOrEraser = tool === "brush" || tool === "eraser";

    if (!layer || !isBrushOrEraser) {
      return {
        display: "none",
      };
    }

    const transform = layer.transform;
    const rot = transform.rotation || 0;
    const scaleX = transform.scaleX ?? 1;
    const scaleY = transform.scaleY ?? 1;
    const flipX = transform.flipH ? -1 : 1;
    const flipY = transform.flipV ? -1 : 1;

    return {
      position: "absolute" as const,
      width: `${layer.width * zoom()}px`,
      height: `${layer.height * zoom()}px`,
      transform: `translate(${pan().x + (transform.x ?? 0) * zoom()}px, ${pan().y + (transform.y ?? 0) * zoom()}px) rotate(${rot}deg) scale(${scaleX * flipX}, ${scaleY * flipY})`,
      "transform-origin": "0 0",
      opacity: layer.opacity ?? 1,
      "pointer-events": "none" as const,
      "will-change": "transform",
    };
  });

  const screenToDocumentPoint = (e: PointerEvent) => {
    const rect = canvasContainerRef?.getBoundingClientRect();
    if (!rect) return { x: e.clientX, y: e.clientY };
    // Use fresh pan/zoom signals instead of engine.getViewport() because
    // the engine viewport goes stale during panning (usePanNavigation skips
    // engine.setViewport to avoid triggering layer re-selection).
    // Reference: useCanvasPointerTools.ts getDocCoords() comment (bug 2026-07-05).
    return screenToDocument(e.clientX, e.clientY, rect, {
      panX: pan().x,
      panY: pan().y,
      zoom: zoom(),
      rotation: 0,
    });
  };

  const { isFitTransition, fitToScreenAndRender } = useViewportRenderer({
    getCanvasContainerRef: () => canvasContainerRef,
    getCanvasRef: () => canvasRef,
    getOverlayCanvasRef: () => getOverlayCanvasRef() || undefined,
  });

  const {
    cropDragPreview,
    snapLines,
    setSnapLines,
    selectionBox,
    setSelectionBoxSignal,
    startSelectionRotation,
    hudInfo,
    setHudInfo,
    handleDoubleClick,
    onCanvasPointerDown,
    onCanvasPointerMove,
    onCanvasPointerUp,
    onCanvasPointerCancel,
    onCanvasLostPointerCapture,
  } = useCanvasPointerTools({
    getCanvasContainerRef: () => canvasContainerRef,
    getCanvasRef: () => canvasRef,
    isSpacePressed,
    isPanning,
    isAltPressed,
    stopMomentum,
    fitToScreenAndRender,
    commitBrushStroke,
    onPaintStroke,
    cropSnapTargets: () => cropSnapTargets(),
    moveSnapEnabled: () => moveSnapEnabled(),
  });

  // The engine/context selection is authoritative. Pointer tools keep a local
  // box for live drag previews, but menu and option-bar commands mutate the
  // engine directly and must update (or clear) that same visible marquee.
  createEffect(() => {
    if (activeTool() !== "selection") {
      setSelectionBoxSignal(null);
      return;
    }
    const current = selection();
    setSelectionBoxSignal(current ? {
      x: current.x,
      y: current.y,
      w: current.width,
      h: current.height,
      angle: current.angle,
      shape: current.shape,
      inverted: current.inverted,
    } : null);
  });

  const canvasLayerDrag = useCanvasLayerDrag({
    onSnapLinesChange: setSnapLines,
    isSpacePressed,
    isPanning,
  });

  const { cropSnapTargets } = useCanvasDerivedState({
    getCanvasContainerRef: () => canvasContainerRef,
    getCanvasRef: () => canvasRef,
    isSpacePressed,
    isPanning,
    isAltPressed,
  });

  // Reset Classic crop state when switching documents to prevent stale
  // cropRect/cropRotation from leaking across documents with different dimensions.
  let prevDocIdForCropReset: string | null = null;
  createEffect(() => {
    const docId = activeDocumentId();
    if (prevDocIdForCropReset !== null && prevDocIdForCropReset !== docId) {
      setCropRect(null);
      setCropRotation(0);
      setCropMode("free");
      setCropAspect(null);
      setCropSizeTarget(null);
      setHiddenCropPreview(null);
      clearCropStacks();
    }
    prevDocIdForCropReset = docId;
  });

  // Modern crop keeps the frame in viewport coordinates, independent of cropRect.
  createEffect(() => {
    if (activeTool() !== "crop" || cropInteractionMode() !== "modern") {
      camera.isModernCropActive = false;
      if (lastModernCropSessionKey !== null) {
        resetModernCrop();
      }
      lastModernCropSessionKey = null;
      return;
    }

    camera.isModernCropActive = true;

    // Build aspect from current mode so frame stays in sync with option bar
    const mode = cropMode();
    const ratioAspect = cropAspect();
    const sizeTarget = cropSizeTarget();
    const aspect =
      mode === "ratio" && ratioAspect
        ? ratioAspect
        : mode === "size" && sizeTarget && sizeTarget.w > 0 && sizeTarget.h > 0
          ? { w: sizeTarget.w, h: sizeTarget.h }
          : null;

    const aspectKey = aspect ? `${aspect.w}x${aspect.h}` : "";
    // Intentionally NOT including zoom() in session key — zoom changes
    // should NOT recreate the frame or recenter the viewport. The frame
    // is in viewport coordinates and must stay where the user placed it.
    const sessionKey = `${activeDocumentId() ?? "none"}:${viewportWidth()}x${viewportHeight()}:${mode}:${aspectKey}`;
    if (lastModernCropSessionKey !== sessionKey) {
      lastModernCropSessionKey = sessionKey;
      // Center document in viewport so frame + document align on entry
      const scale = modernCropImageTransform().scale;
      const centerPanX = (viewportWidth() - docWidth() * zoom() * scale) / 2;
      const centerPanY = (viewportHeight() - docHeight() * zoom() * scale) / 2;
      setViewportState({ x: centerPanX, y: centerPanY, zoom: zoom() });
      setModernCropFrame(
        getDefaultModernCropFrame({
          viewportWidth: viewportWidth(),
          viewportHeight: viewportHeight(),
          docWidth: docWidth(),
          docHeight: docHeight(),
          zoom: zoom(),
          scale: modernCropImageTransform().scale,
          aspect,
          panX: centerPanX,
          panY: centerPanY,
        }),
      );
    }
  });

  // Classic crop: initialize preview on entry when mode is constrained
  createEffect(() => {
    if (activeTool() !== "crop" || cropInteractionMode() !== "classic") return;

    const mode = cropMode();
    if (mode === "free") return;
    if (cropRect() !== null || hiddenCropPreview() !== null) return;

    const docW = docWidth();
    const docH = docHeight();
    if (docW <= 0 || docH <= 0) return;

    if (mode === "ratio") {
      const a = cropAspect();
      if (a) {
        setCropRect(fitCropRectToAspect(a, docW, docH, cropRotation()));
      }
    } else if (mode === "size") {
      const t = cropSizeTarget();
      if (t && t.w > 0 && t.h > 0) {
        setCropRect(fitCropRectToAspect(t, docW, docH, cropRotation()));
      }
    }
  });

  // Alt key and shortcuts Setup
  useCanvasKeyboard({
    isSpacePressed,
    setIsSpacePressed,
    isAltPressed,
    setIsAltPressed,
    isPanning,
    setIsPanning,
    stopMomentum,
    fitToScreenAndRender,
    syncViewport,
    getCanvasContainerRef: () => canvasContainerRef,
    onSelectionChange: () => {
      const engine = workspace.getActiveEngine();
      const sel = engine?.getSelection();
      if (sel) {
        setSelectionBoxSignal({
          x: sel.x,
          y: sel.y,
          w: sel.width,
          h: sel.height,
          angle: sel.angle,
          shape: sel.shape,
          inverted: sel.inverted,
        });
      } else {
        setSelectionBoxSignal(null);
      }
    },
  });

  // Derived canvas screen rect for expansion fill indicator →memo outside Show to guarantee reactivity
  // The dashed canvas boundary line represents the TRANSFORMED image boundary,
  // including offset/scale/rotation from modernCropImageTransform. This ensures
  // the expansion fill and dashed outline track the image, not just the
  // viewport-space doc position.
  const canvasScreenRect = createMemo(() => {
    const p = pan();
    const z = zoom();
    const docW = docWidth();
    const docH = docHeight();
    const transform = modernCropImageTransform();
    // Apply image offset and scale so the dashed line tracks the image
    const sx = p.x + transform.offsetX;
    const sy = p.y + transform.offsetY;
    const sz = z * (transform.scale ?? 1);
    const rot = transform.rotation;
    if (rot !== 0) {
      // Under rotation the document is skewed; compute an axis-aligned
      // bounding box that covers the rotated canvas so the expansion
      // fill and dashed boundary outline remain visible as reference.
      const cx = sx + (docW * sz) / 2;
      const cy = sy + (docH * sz) / 2;
      const rad = (Math.abs(rot) * Math.PI) / 180;
      const hw = (docW * sz) / 2;
      const hh = (docH * sz) / 2;
      const cos = Math.cos(rad);
      const sin = Math.sin(rad);
      // Rotate the four corners and find the bounding box
      const corners = [
        { x: -hw, y: -hh }, { x: hw, y: -hh },
        { x: hw, y: hh }, { x: -hw, y: hh },
      ].map(c => ({
        x: c.x * cos - c.y * sin + cx,
        y: c.x * sin + c.y * cos + cy,
      }));
      const minX = Math.min(...corners.map(c => c.x));
      const minY = Math.min(...corners.map(c => c.y));
      const maxX = Math.max(...corners.map(c => c.x));
      const maxY = Math.max(...corners.map(c => c.y));
      return { x: minX, y: minY, w: maxX - minX, h: maxY - minY };
    }
    return {
      x: sx,
      y: sy,
      w: docW * sz,
      h: docH * sz,
    };
  });

  const isPasteboardPointerDown = (e: PointerEvent) => {
    if (e.target === canvasContainerRef) return true;
    // The SVG overlay (z-index 40, full viewport) sits on top of the canvas.
    // Clicks outside the document bounds that land on the SVG are pasteboard clicks.
    const target = e.target as Element | null;
    if (target?.closest?.("[data-overlay-svg]")) {
      if (activeTool() === "crop" && !cropRect()) return false;
      const point = screenToDocumentPoint(e);
      return (
        point.x < 0 ||
        point.y < 0 ||
        point.x > docWidth() ||
        point.y > docHeight()
      );
    }
    if (e.target === canvasRef) {
      if (activeTool() === "crop" && !cropRect()) return false;
      const point = screenToDocumentPoint(e);
      return (
        point.x < 0 ||
        point.y < 0 ||
        point.x > docWidth() ||
        point.y > docHeight()
      );
    }
    // In Modern crop mode, the SVG overlay (z-index 40, full viewport)
    // captures pasteboard clicks. Route them to the canvas handler.
    if (activeTool() === "crop" && cropInteractionMode() === "modern") {
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
    if (activeTool() !== "move") return;
    if (!moveAutoSelect()) return;
    if (isSpacePressed() || isPanning()) return;
    const target = e.target as Element | null;

    const isSvgOverlayClick = target?.closest?.("[data-overlay-svg]");
    if (isSvgOverlayClick) {
      // Only intercept clicks on the SVG root (not on child elements like
      // move rect/handles which have their own onPointerDown).
      if (target !== target?.closest?.("[data-overlay-svg]")) return;
    } else {
      // If there is no SVG overlay, allow clicks directly on the canvas or container
      if (target !== canvasRef && target !== canvasContainerRef) return;
    }

    const engine = workspace.getActiveEngine();
    if (!engine) return;

    const coords = screenToDocumentPoint(e);
    const docW = docWidth();
    const docH = docHeight();
    if (coords.x < 0 || coords.y < 0 || coords.x > docW || coords.y > docH)
      return;

    const allLayers = [...engine.getLayers()];
    const hit = hitTestLayers(coords, allLayers as LayerInfo[], (id, x, y) => engine.sampleLayerAlpha(id, x, y));
    if (hit && hit.id !== selectedLayerId()) {
      engine.setActiveLayer(hit.id);
      setSelectedLayerId(hit.id);
      scheduler.requestRender();
    } else if (!hit) {
      setSelectedLayerId(null);
    }
  };

  const handlePasteboardPointerDown = (e: PointerEvent) => {
    if (!isPasteboardPointerDown(e)) return;
    if (e.button !== 0) return;

    if (activeTool() === "crop") {
      if (isSpacePressed() || isPanning()) return;
      if (cropInteractionMode() === "modern") {
        // Route to canvas handler →it calls canvas.setPointerCapture()
        // and tracks modernDragStart for drag-to-create. After canvas
        // captures the pointer, subsequent events go to canvas handlers.
        onCanvasPointerDown(e);
        e.preventDefault();
        e.stopPropagation();
        return;
      }

      const startDocument = screenToDocumentPoint(e);
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

    const engine = workspace.getActiveEngine();
    const action = getPasteboardClickAction({
      hasDocument: Boolean(engine),
      activeTool: activeTool(),
      isNavigationMode: isSpacePressed() || isPanning(),
      hasLayerTransformSession: Boolean(layerTransformSession()),
      hasCropRect: Boolean(cropRect()),
      hasSelectionPreview: Boolean(selectionBox()),
    });

    if (action === "noop") return;

    e.preventDefault();
    e.stopPropagation();

    if (action === "clear-active-layer" && engine) {
      setSelectedLayerId(null);
      setHoverHandle(null);
      setSnapLines([]);
      setHudInfo(null);
      scheduler.requestRender();
      return;
    }

    if (action === "clear-selection-preview") {
      setSelectionBoxSignal(null);
      engine?.clearSelection();
      setSnapLines([]);
      setHudInfo(null);
      scheduler.requestRender();
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
        screenToDocumentPoint(e),
      );
      if (!nextRect) {
        return;
      }

      setHiddenCropPreview(null);
      setCropRotation(0);
      setCropRect(nextRect);
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
          cropRect,
          cropRotation,
          hiddenCropPreview,
          setCropRect,
          setCropRotation,
          setHiddenCropPreview,
        });
        setHoverHandle(null);
        setSnapLines([]);
        setHudInfo(null);
        scheduler.requestRender();
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

  const dragController = useDragController();

  const { onDragOver, onDragLeave, onDrop } = useCanvasDrop({
    dragController,
    camera,
    workspace,
    renderer,
    scheduler,
  });

  return (
    <div
      ref={canvasContainerRef}
      id="canvas-container"
      data-viewport-container
      data-canvas-drop-zone
      data-drag-over={dragController.state().dropTarget?.type === "canvas" ? "canvas" : null}
      class="flex-1 relative overflow-hidden bg-editor-canvas"
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}

      onWheel={handleWheel}
      onDblClick={handleDoubleClick}
      onPointerDown={(e) => {
        stopMomentum();
        if (isSpacePressed() || isPanning()) {
          onViewportPointerDown(e);
          return;
        }
        canvasLayerDrag.handlePointerDown(e);
        if (!e.defaultPrevented) {
          handlePasteboardPointerDown(e);
          if (!e.defaultPrevented) {
            handleMoveAutoSelect(e);
            onViewportPointerDown(e);
          }
        }
      }}
      onPointerMove={(e) => {
        if (isPanning()) {
          onViewportPointerMove(e);
          return;
        }
        handlePasteboardPointerMove(e);
        if (!e.defaultPrevented) onViewportPointerMove(e);
      }}
      onPointerUp={(e) => {
        if (isPanning()) {
          onViewportPointerUp(e);
          return;
        }
        handlePasteboardPointerUp(e);
        if (!e.defaultPrevented) onViewportPointerUp(e);
      }}
      onPointerCancel={(e) => {
        handlePasteboardPointerCancel(e);
        onViewportPointerCancel(e);
      }}
      onContextMenu={(e) => {
        // Suppress browser context menu during Alt+RightButton brush adjustment
        if (e.altKey && (activeTool() === "brush" || activeTool() === "eraser")) {
          e.preventDefault();
          e.stopPropagation();
        }
      }}
      onLostPointerCapture={onViewportLostPointerCapture}
    >
      {/* CSS Transform container →GPU-accelerated pan/zoom */}
      {/* the engine is a non-reactive class method, so wrapping
         it directly in `<Show when={...}>` would compile to a constant
         getter that never re-evaluates when activeDocumentId changes.
         The activeDocumentId signal is referenced first to register the
         dependency in the JSX reactive scope, then getActiveEngine()
         reads through the freshly-updated workspace state. This keeps
         the canvas mounted across document switches. */}
      <Show when={activeDocumentId() && !!workspace.getActiveEngine()}>
        {/* WebGL Canvas →outside transform div for 1:1 pixel mapping.
            The canvas pixel buffer is Math.round(docWidth * zoom * dpr) but the
            CSS box inside a scale(zoom) parent would be docWidth before transform.
            This creates a downscale then upscale cycle →bilinear filtering bleeds
            transparent pixels from outside the canvas onto edge pixels, causing the
            "visible thin gap" at high zoom. By placing the canvas outside, its CSS
            dimensions exactly match its pixel buffer (at dpr=1), eliminating the
            filtering artifact. */}
        <Show
          when={
            activeTool() === "crop" &&
            cropFillEnabled() &&
            cropInteractionMode() === "classic" &&
            cropRect()
          }
        >
          <div
            data-crop-fill-preview="classic"
            style={classicCropFillPreviewStyle()}
          />
        </Show>
        <Show
          when={
            activeTool() === "crop" &&
            cropFillEnabled() &&
            cropInteractionMode() === "modern" &&
            modernCropFrame()
          }
        >
          <div
            data-crop-fill-preview="modern"
            style={modernCropFillPreviewStyle()}
          />
        </Show>
        <canvas
          ref={canvasRef}
          onPointerDown={onCanvasPointerDown}
          onPointerMove={(e) => {
            // During a useCanvasLayerDrag-managed drag (move tool), the
            // document-level listener already handles the transform. Skip
            // the canvas-level handler to prevent a double mutation —
            // both engine.moveLayer() AND engine.transformLayer() would
            // fire per pointermove, doubling all work (syncState, layer
            // array clone, Solid reactivity, notifyChange chain).
            if (canvasLayerDrag.isDragging()) return;
            onCanvasPointerMove(e);
          }}
          onPointerUp={onCanvasPointerUp}
          onPointerCancel={onCanvasPointerCancel}
          onLostPointerCapture={onCanvasLostPointerCapture}
          style={
            activeTool() === "crop" &&
            cropInteractionMode() === "modern" &&
            !useGPUCameraForModernCrop()
              ? {
                  // Legacy CSS path: doc-sized canvas with CSS transform
                  position: "absolute",
                  left: "0px",
                  top: "0px",
                  width: `${docWidth()}px`,
                  height: `${docHeight()}px`,
                  transform: modernImageTransformStyle(),
                  "transform-origin": "0 0",
                  "image-rendering": "auto",
                  transition: "none",
                }
              : {
                  // GPU camera path (modern crop + flag on) OR non-modern-crop:
                  // viewport-sized canvas, transform handled in VP matrix
                  position: "absolute",
                  inset: "0px",
                  width: "100%",
                  height: "100%",
                  "image-rendering": "auto",
                  transition: "none",
                }
          }
        />
        <Show
          when={activeTool() !== "crop" || cropInteractionMode() !== "modern"}
        >
          {/* 2D brush preview canvas →screen-space coords, layer transform preserved */}
          <canvas
            ref={setOverlayCanvasRef}
            data-overlay-canvas
            style={overlayCanvasStyleScreenSpace()}
          />

          {/* Artboard border & shadow →screen-space coords, GPU-accelerated via transform */}
          <div
            data-artboard-border
            class="absolute pointer-events-none border border-white/10"
            style={{
              transform: `translate(${pan().x}px, ${pan().y}px)`,
              width: `${docWidth() * zoom()}px`,
              height: `${docHeight() * zoom()}px`,
              "box-shadow":
                "0 0 0 1px rgba(0, 0, 0, 0.6), 0 8px 32px rgba(0, 0, 0, 0.7)",
              "will-change": "transform",
            }}
          />

          {/* Screen-space SVG Overlay Layer */}
          <svg
            style={{
              position: "absolute",
              inset: 0,
              width: "100%",
              height: "100%",
              overflow: "visible",
              "pointer-events": "none",
            }}
          >
            {/* Selection marquee →screen-space coordinates */}
            <Show when={selectionBox()}>
              {(box) => (
                <SelectionRenderer
                  selection={{
                    x: box().x,
                    y: box().y,
                    width: box().w,
                    height: box().h,
                    angle: box().angle ?? 0,
                    shape: box().shape,
                    inverted: box().inverted,
                  }}
                  zoom={zoom()}
                  pan={pan()}
                  canvasWidth={docWidth()}
                  canvasHeight={docHeight()}
                  editMode={selectionEditMode()}
                  onRotatePointerDown={() => startSelectionRotation()}
                />
              )}
            </Show>
            <HoverHighlight />
            <SmartGuides lines={snapLines()} />
            <BrushCursorOverlay
              isAltPressed={isAltPressed()}
              isPanning={isSpacePressed() || isPanning()}
            />
            <Show when={hudInfo()}>
              {(h) => (
                <TransformHud
                  mode={h().mode}
                  clientX={h().clientX}
                  clientY={h().clientY}
                  zoom={zoom()}
                  deltaX={h().deltaX}
                  deltaY={h().deltaY}
                  width={h().width}
                  height={h().height}
                  scalePercent={h().scalePercent}
                  angle={h().angle}
                  snapActive={h().snapActive}
                />
              )}
            </Show>
          </svg>

          {/* SelectionTransformOverlay →screen-space coordinates */}
          <Show when={activeTool() === "move" && showTransformControls()}>
            <SelectionTransformOverlay
              isNavigationMode={isSpacePressed() || isPanning()}
              onHudUpdate={setHudInfo}
              onComputeSnap={(rect) => {
                const engine = workspace.getActiveEngine();
                if (!engine) return { dx: 0, dy: 0, lines: [] };
                const movingId = engine.getActiveLayerId();
                const docW = engine.getWidth();
                const docH = engine.getHeight();
                const layerTargets: SnapRect[] = engine
                  .getLayers()
                  .filter((l) => l.visible && l.id !== movingId)
                  .map((l) => {
                    const aabb = getLayerAabb(l.transform, l.width, l.height);
                    return {
                      x: aabb.x,
                      y: aabb.y,
                      w: aabb.width,
                      h: aabb.height,
                    };
                  });
                const snapTargets: SnapRect[] = [
                  {
                    x: 0,
                    y: 0,
                    w: docW,
                    h: docH,
                    snapThreshold: 12,
                    snapPriority: 3,
                  },
                  {
                    x: docW / 2,
                    y: -Infinity,
                    w: 0,
                    h: Infinity,
                    snapThreshold: 6,
                    snapPriority: 2,
                  },
                  {
                    x: -Infinity,
                    y: docH / 2,
                    w: Infinity,
                    h: 0,
                    snapThreshold: 6,
                    snapPriority: 2,
                  },
                  ...layerTargets,
                ];
                const result = computeSnapAdjustment(rect, snapTargets, 5, zoom());
                setSnapLines(result.lines);
                return result;
              }}
              onSnapClear={() => setSnapLines([])}
              onScreenToDoc={(cx, cy) => {
                const rect = canvasContainerRef?.getBoundingClientRect();
                const engine = workspace.getActiveEngine();
                if (!rect || !engine)
                  return {
                    x: (cx - pan().x) / zoom(),
                    y: (cy - pan().y) / zoom(),
                  };
                return camera.screenToDocument(cx - rect.left, cy - rect.top);
              }}
              snapActive={snapLines().length > 0}
              onStopMomentum={stopMomentum}
            />
          </Show>

          {/* Classic Crop Overlay →screen-space coordinates */}
          <Show
            when={
              activeTool() === "crop" &&
              cropInteractionMode() === "classic" &&
              cropRect()
            }
          >
            <CropOverlay
              isNavigationMode={isSpacePressed() || isPanning()}
              cropRect={cropRect()}
              guideMode={cropGuideMode()}
              canvasWidth={docWidth()}
              canvasHeight={docHeight()}
              zoom={zoom()}
              cropMode={cropMode()}
              cropAspect={cropAspect()}
              cropRotation={cropRotation()}
              deleteCropped={cropDeletePixels()}
              onCropRectChange={(rect) => setCropRect(rect)}
              onCropRotationChange={setCropRotation}
              onHoverHandleChange={setHoverHandle}
              snapTargets={cropSnapTargets()}
              snapEnabled={moveSnapEnabled()}
              onSnapLines={setSnapLines}
              onDragStateChange={setIsCropDragging}
              hiddenCropPreview={hiddenCropPreview()}
              onHiddenCropPreviewChange={setHiddenCropPreview}
              isAltPressed={isAltPressed}
              onApplyCrop={() => {
                applyCropPreview({
                  workspace,
                  renderer,
                  viewport: { width: viewportWidth(), height: viewportHeight() },
                  cropRect: cropRect(),
                  cropMode: cropMode(),
                  cropSizeTarget: cropSizeTarget(),
                  cropDeletePixels: cropDeletePixels(),
                  cropFillColor: cropFillEnabled()
                    ? resolvedCropFillColor()
                    : null,
                  cropRotation: cropRotation(),
                  scheduler,
                  setCropRect,
                  setCropRotation,
                  setHiddenCropPreview,
                  setActiveTool,
                  setSelectedLayerId,
                  recenterViewport: () => fitToScreenAndRender(),
                });
              }}
            />
          </Show>
        </Show>
        <Show
          when={
            activeTool() === "crop" &&
            cropInteractionMode() === "modern" &&
            modernCropFrame()
          }
        >
          {(frame) => {
            const sa = () =>
              cropMode() === "size" && cropSizeTarget()
                ? cropSizeTarget()
                : null;
            const ea = () => (cropMode() === "ratio" ? cropAspect() : sa());
            return (
              <ModernCropOverlay
                isNavigationMode={isSpacePressed() || isPanning()}
                // frame() is a reactive accessor for modernCropFrame()
                // Convert doc-coords to screen-coords for overlay positioning
                frame={docFrameToScreenFrame(frame(), zoom(), pan())!}
                imageTransform={modernCropImageTransform()}
                viewportWidth={viewportWidth()}
                viewportHeight={viewportHeight()}
                projectedWidth={
                  docWidth() * zoom() * (modernCropImageTransform().scale ?? 1)
                }
                projectedHeight={
                  docHeight() * zoom() * (modernCropImageTransform().scale ?? 1)
                }
                canvasScreenRect={canvasScreenRect()}
                cropMode={cropMode()}
                cropAspect={ea()}
                guideMode={cropGuideMode()}
                onFrameChange={(screenFrame) => setModernCropFrame(
                  screenFrameToDocFrame(screenFrame, zoom(), pan())!,
                )}
                onImageTransformChange={setModernCropImageTransform}
                onHoverHandleChange={setHoverHandle}
                onDragStateChange={setIsCropDragging}
                onModernCropCommit={() => commitModernCropState()}
                isAltPressed={isAltPressed}
                onApplyCrop={() => {
                  const f = modernCropScreenFrame();
                  if (!f) return;
                  const rect = modernFrameToCropRect({
                    frame: f,
                    viewport: {
                      width: viewportWidth(),
                      height: viewportHeight(),
                      panX: pan().x,
                      panY: pan().y,
                      zoom: zoom(),
                    },
                    transform: modernCropImageTransform(),
                  });
                  applyCropPreview({
                    workspace,
                    renderer,
                    viewport: { width: viewportWidth(), height: viewportHeight() },
                    cropRect: rect,
                    cropMode: cropMode(),
                    cropSizeTarget: cropSizeTarget(),
                    cropDeletePixels: cropDeletePixels(),
                    cropFillColor: cropFillEnabled()
                      ? resolvedCropFillColor()
                      : null,
                    cropRotation: getModernCropApplyRotation(
                      modernCropImageTransform().rotation,
                    ),
                    scheduler,
                    setCropRect,
                    setCropRotation,
                    setHiddenCropPreview,
                    setActiveTool,
                    setSelectedLayerId,
                    recenterViewport: () => fitToScreenAndRender(),
                  });
                  resetModernCrop();
                }}
              />
            );
          }}
        </Show>

        {/* Crop drag preview →screen-space selection rectangle */}
        <Show when={cropDragPreview()}>
          {(box) => (
            <div
              data-crop-drag-preview=""
              style={{
                position: "absolute",
                left: `${box().x}px`,
                top: `${box().y}px`,
                width: `${box().w}px`,
                height: `${box().h}px`,
                outline: "1.5px dashed #E15A17",
                "pointer-events": "none",
                "z-index": 45,
              }}
            />
          )}
        </Show>
      </Show>
      <BrushContextMenu />
      <CanvasContextMenu />
    </div>
  );
}
