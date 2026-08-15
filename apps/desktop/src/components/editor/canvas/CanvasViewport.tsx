import { createMemo, createSignal, createEffect, Show, For } from "solid-js";
import { screenToDocument } from "@/viewport/coords";
import { computeSnapAdjustment } from "@/viewport/smartGuides";
import { buildTransformSnapTargets } from "@/viewport/transformSnapTargets";
import { getLayerAabb, getCursorForHandle } from "@/viewport/transformGeometry";
import { useCanvasMarqueeSelect } from "./useCanvasMarqueeSelect";
import { useMultiSelectionGroupTransform } from "./useMultiSelectionGroupTransform";
import { useEditor } from "../shell/EditorContext";
import { useCanvasKeyboard } from "./useCanvasKeyboard";
import { useBrushOverlay } from "../useBrushOverlay";
import { usePanNavigation } from "./usePanNavigation";
import { useViewportRenderer } from "./useViewportRenderer";
import { useCanvasPointerTools } from "./useCanvasPointerTools";
import { usePasteboardGesture } from "./usePasteboardGesture";
import { useCropOverlayStyles } from "./useCropOverlayStyles";
import { useCanvasLayerDrag } from "../layers/useCanvasLayerDrag";
import { useCanvasDerivedState } from "./useCanvasDerivedState";
import { useDragController } from "../DragController";
import { useCanvasDrop } from "./useCanvasDrop";
import { ViewportCamera } from "@/viewport/viewportCamera";
import { SelectionTransformOverlay } from "../SelectionTransformOverlay";
import { HoverHighlight } from "./HoverHighlight";
import { SmartGuides } from "./SmartGuides";
import { BrushCursorOverlay } from "../BrushCursorOverlay";
import { TextEditOverlay } from "../TextEditOverlay";
import { CropOverlay } from "../CropOverlay";
import { ModernCropOverlay } from "../ModernCropOverlay";
import { TransformHud } from "../TransformHud";
import { SelectionRenderer } from "@/features/selection/SelectionRenderer";
import { BrushContextMenu } from "../BrushContextMenu";
import { CanvasContextMenu } from "./CanvasContextMenu";
import { GradientOverlay } from "./GradientOverlay";
import {
  docFrameToScreenFrame,
  getDefaultModernCropFrame,
  getModernCropApplyRotation,
  getModernCropImagePivot,
  getProjectedCanvasSize,
  modernFrameToCropRect,
  screenFrameToDocFrame,
} from "@/viewport/modernCropGeometry";
import { getAdaptiveRotateBandPx } from "@/viewport/rotateBand";
import { fitCropRectToAspect } from "@/viewport/cropAutoFit";
import {
  clearCropPreview,
  applyCropPreview,
} from "../cropToolActions";

export function CanvasViewport() {
  const editor = useEditor();
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
    snapToLayersEnabled,
    snapToCanvasEnabled,
    layerTransformSession,
    showTransformControls,
    selection,
    selectionEditMode,
    setSelectionEditMode,
    scheduler,
    useGPUCameraForModernCrop,
    selectedLayerIds,
    toggleLayerSelection,
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

  const { resolvedCropFillColor, classicCropFillPreviewStyle, modernCropScreenFrame, modernCropFillPreviewStyle, canvasScreenRect } =
    useCropOverlayStyles({
      cropRect,
      cropRotation,
      zoom,
      pan,
      cropFillSource,
      bgColor,
      cropFillCustomColor,
      modernCropFrame,
      modernCropImageTransform,
      docWidth,
      docHeight,
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

  const canvasMarquee = useCanvasMarqueeSelect({
    isSpacePressed,
    isPanning,
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
    onStartMarquee: (e) => canvasMarquee.handlePointerDown(e, canvasContainerRef),
  });

  const {
    handlePasteboardPointerDown,
    handlePasteboardPointerMove,
    handlePasteboardPointerUp,
    handlePasteboardPointerCancel,
    handleMoveAutoSelect,
  } = usePasteboardGesture({
    getCanvasContainerRef: () => canvasContainerRef,
    getCanvasRef: () => canvasRef,
    isSpacePressed,
    isPanning,
    activeTool,
    cropRect,
    cropRotation,
    hiddenCropPreview,
    cropInteractionMode,
    docWidth,
    docHeight,
    moveAutoSelect,
    layerTransformSession,
    selectionBox,
    selectedLayerId,
    selectedLayerIds,
    getEngine: () => workspace.getActiveEngine(),
    screenToDocumentPoint,
    onCanvasPointerDown,
    setSelectedLayerId,
    toggleLayerSelection,
    setSelectionBoxSignal,
    setHoverHandle,
    setSnapLines,
    setHudInfo,
    setCropRect,
    setCropRotation,
    setHiddenCropPreview,
    scheduler,
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
    onHudUpdate: setHudInfo,
    isSpacePressed,
    isPanning,
  });

  const multiGroupTransform = useMultiSelectionGroupTransform({
    isNavigationMode: isSpacePressed() || isPanning(),
    onHudUpdate: setHudInfo,
    onScreenToDoc: (cx, cy) => ({ x: (cx - pan().x) / zoom(), y: (cy - pan().y) / zoom() }),
  });

  const HANDLE_TYPES = ["nw", "n", "ne", "e", "se", "s", "sw", "w"] as const;

  const { cropSnapTargets } = useCanvasDerivedState({
    getCanvasContainerRef: () => canvasContainerRef,
    getCanvasRef: () => canvasRef,
    isSpacePressed,
    isPanning,
    isAltPressed,
  });

  const multiSelectionGroupAabb = createMemo(() => {
    const ids = typeof selectedLayerIds === "function" ? selectedLayerIds() : [];
    if (ids.length <= 1) return null;
    const all = typeof layers === "function" ? layers() : [];
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    let count = 0;
    for (const id of ids) {
      const l = all.find((item) => item.id === id);
      if (!l || !l.visible) continue;
      const aabb = getLayerAabb(l.transform, l.width, l.height);
      if (aabb.x < minX) minX = aabb.x;
      if (aabb.y < minY) minY = aabb.y;
      if (aabb.x + aabb.width > maxX) maxX = aabb.x + aabb.width;
      if (aabb.y + aabb.height > maxY) maxY = aabb.y + aabb.height;
      count++;
    }
    if (count <= 1 || minX === Infinity) return null;
    return {
      x: minX,
      y: minY,
      width: maxX - minX,
      height: maxY - minY,
    };
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
            if (!canvasLayerDrag.isDragging() && activeTool() === "move") {
              canvasMarquee.handlePointerDown(e, canvasContainerRef);
            }
            onViewportPointerDown(e);
          }
        }
      }}
      onPointerMove={(e) => {
        if (isPanning()) {
          onViewportPointerMove(e);
          return;
        }
        if (multiGroupTransform.isTransforming()) {
          multiGroupTransform.handlePointerMove(e);
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
        if (multiGroupTransform.isTransforming()) {
          multiGroupTransform.handlePointerUp(e);
          return;
        }
        handlePasteboardPointerUp(e);
        if (!e.defaultPrevented) onViewportPointerUp(e);
      }}
      onPointerCancel={(e) => {
        if (multiGroupTransform.isTransforming()) {
          multiGroupTransform.handlePointerCancel(e);
        }
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

            {/* Canvas Rubberband Marquee Box */}
            <Show when={canvasMarquee.marqueeRect()}>
              {(rect) => {
                const z = zoom();
                const p = pan();
                return (
                  <rect
                    data-canvas-marquee
                    x={rect().x * z + p.x}
                    y={rect().y * z + p.y}
                    width={rect().width * z}
                    height={rect().height * z}
                    fill="var(--color-editor-accent)"
                    fill-opacity="0.15"
                    stroke="var(--color-editor-accent)"
                    stroke-width="1.25"
                    stroke-dasharray="3 3"
                    vector-effect="non-scaling-stroke"
                    style={{ "pointer-events": "none" }}
                  />
                );
              }}
            </Show>

            {/* Multi-Selection Group Bounding Box & Interactive Handles */}
            <Show when={activeTool() === "move" && !canvasMarquee.isMarqueeActive() ? multiSelectionGroupAabb() : null}>
              {(group) => {
                const gx = () => group().x * zoom() + pan().x;
                const gy = () => group().y * zoom() + pan().y;
                const gw = () => group().width * zoom();
                const gh = () => group().height * zoom();
                const hs = 8;
                const ht = 28;
                const ringWidth = () => getAdaptiveRotateBandPx(gw(), gh());

                return (
                  <g data-multi-selection-group>
                    {/* Unified Group Boundary Box */}
                    <rect
                      data-multi-group-boundary
                      x={gx()}
                      y={gy()}
                      width={gw()}
                      height={gh()}
                      fill="var(--color-editor-accent)"
                      fill-opacity={showTransformControls() ? "0.02" : "0.04"}
                      stroke="var(--color-editor-accent)"
                      stroke-width={showTransformControls() ? "1.5" : "1.25"}
                      stroke-dasharray={showTransformControls() ? undefined : "6 4"}
                      vector-effect="non-scaling-stroke"
                      style={{
                        "pointer-events": "none",
                        filter: "drop-shadow(0px 0px 2px rgba(0, 0, 0, 0.8))",
                      }}
                    />

                    {/* Mode A: Clean corner brackets when showTransformControls is OFF */}
                    <Show when={!showTransformControls()}>
                      <g data-multi-corner-brackets style={{ "pointer-events": "none" }}>
                        <rect x={gx() - 3} y={gy() - 3} width={6} height={6} fill="var(--color-editor-accent)" stroke="#111" stroke-width="1" />
                        <rect x={gx() + gw() - 3} y={gy() - 3} width={6} height={6} fill="var(--color-editor-accent)" stroke="#111" stroke-width="1" />
                        <rect x={gx() - 3} y={gy() + gh() - 3} width={6} height={6} fill="var(--color-editor-accent)" stroke="#111" stroke-width="1" />
                        <rect x={gx() + gw() - 3} y={gy() + gh() - 3} width={6} height={6} fill="var(--color-editor-accent)" stroke="#111" stroke-width="1" />
                      </g>
                    </Show>

                    {/* Mode B: Interactive Handles, Rotate Donut & Top Rotate Pin when showTransformControls is ON */}
                    <Show when={showTransformControls()}>
                      {/* Perimeter Adaptive Donut Rotate Ring */}
                      {(() => {
                        const rw = ringWidth();
                        const g = Math.max(0, Math.min(3, Math.min(gw(), gh()) / 2 - 1));
                        return (
                          <path
                            d={`
                              M ${gx() - rw} ${gy() - rw}
                              L ${gx() + gw() + rw} ${gy() - rw}
                              L ${gx() + gw() + rw} ${gy() + gh() + rw}
                              L ${gx() - rw} ${gy() + gh() + rw}
                              Z
                              M ${gx() + g} ${gy() + g}
                              L ${gx() + g} ${gy() + gh() - g}
                              L ${gx() + gw() - g} ${gy() + gh() - g}
                              L ${gx() + gw() - g} ${gy() + g}
                              Z
                            `}
                            fill="transparent"
                            fill-rule="evenodd"
                            style={{
                              "pointer-events": isSpacePressed() || isPanning() ? "none" : "all",
                              cursor: "crosshair",
                            }}
                            onPointerDown={(e) => multiGroupTransform.handlePointerDown(e, "rotate")}
                          />
                        );
                      })()}

                      {/* Top Rotate Antenna / Pin Handle */}
                      {(() => {
                        const topMidX = () => gx() + gw() / 2;
                        const topMidY = () => gy();
                        const pinY = () => gy() - 20;
                        const pinR = 4.5;
                        return (
                          <g data-multi-rotate-pin>
                            {/* Stalk connecting line */}
                            <line
                              x1={topMidX()}
                              y1={topMidY()}
                              x2={topMidX()}
                              y2={pinY()}
                              stroke="var(--color-editor-accent)"
                              stroke-width={1.25}
                              vector-effect="non-scaling-stroke"
                              style={{ "pointer-events": "none" }}
                            />
                            {/* Transparent hit area */}
                            <circle
                              cx={topMidX()}
                              cy={pinY()}
                              r={14}
                              fill="transparent"
                              style={{
                                "pointer-events": isSpacePressed() || isPanning() ? "none" : "all",
                                cursor: "crosshair",
                              }}
                              onPointerDown={(e) => multiGroupTransform.handlePointerDown(e, "rotate")}
                            />
                            {/* Visible pin handle */}
                            <circle
                              cx={topMidX()}
                              cy={pinY()}
                              r={pinR}
                              fill="#FFFFFF"
                              stroke="var(--color-editor-accent)"
                              stroke-width={1.5}
                              vector-effect="non-scaling-stroke"
                              style={{
                                "pointer-events": "none",
                                filter: "drop-shadow(0px 1px 2px rgba(0, 0, 0, 0.6))",
                              }}
                            />
                          </g>
                        );
                      })()}

                      {/* 8 Interactive Transform Handles */}
                      <For each={HANDLE_TYPES}>
                        {(type) => {
                          const hx = () =>
                            type === "nw" || type === "sw" || type === "w"
                              ? gx()
                              : type === "ne" || type === "se" || type === "e"
                              ? gx() + gw()
                              : gx() + gw() / 2;
                          const hy = () =>
                            type === "nw" || type === "n" || type === "ne"
                              ? gy()
                              : type === "sw" || type === "s" || type === "se"
                              ? gy() + gh()
                              : gy() + gh() / 2;
                          const cursor = () => getCursorForHandle(type, 0, 1, 1);
                          return (
                            <g data-multi-handle={type}>
                              {/* Hit area for drag resize */}
                              <rect
                                x={hx() - ht / 2}
                                y={hy() - ht / 2}
                                width={ht}
                                height={ht}
                                fill="transparent"
                                style={{
                                  "pointer-events": isSpacePressed() || isPanning() ? "none" : "all",
                                  cursor: cursor(),
                                }}
                                onPointerDown={(e) => multiGroupTransform.handlePointerDown(e, type)}
                              />
                              {/* Visible crisp white square handle with accent stroke */}
                              <rect
                                x={hx() - hs / 2}
                                y={hy() - hs / 2}
                                width={hs}
                                height={hs}
                                fill="#FFFFFF"
                                stroke="var(--color-editor-accent)"
                                stroke-width={1.3}
                                vector-effect="non-scaling-stroke"
                                style={{
                                  "pointer-events": "none",
                                  filter: "drop-shadow(0px 1px 2px rgba(0, 0, 0, 0.5))",
                                }}
                              />
                            </g>
                          );
                        }}
                      </For>
                    </Show>
                  </g>
                );
              }}
            </Show>

            {/* Multi-Selected Layer Outlines */}
            <Show when={typeof selectedLayerIds === "function" && selectedLayerIds().length > 1 && activeTool() === "move" && !canvasMarquee.isMarqueeActive()}>
              <For each={typeof selectedLayerIds === "function" ? selectedLayerIds() : []}>
                {(id) => {
                  const currentLayer = createMemo(() => {
                    const all = typeof layers === "function" ? layers() : [];
                    return all.find((item) => item.id === id);
                  });
                  const currentAabb = createMemo(() => {
                    const l = currentLayer();
                    return l && l.visible ? getLayerAabb(l.transform, l.width, l.height) : null;
                  });
                  const ox = () => (currentAabb() ? currentAabb()!.x * zoom() + pan().x : 0);
                  const oy = () => (currentAabb() ? currentAabb()!.y * zoom() + pan().y : 0);
                  const ow = () => (currentAabb() ? currentAabb()!.width * zoom() : 0);
                  const oh = () => (currentAabb() ? currentAabb()!.height * zoom() : 0);

                  return (
                    <Show when={currentAabb()}>
                      <rect
                        data-multi-select-outline={id}
                        x={ox()}
                        y={oy()}
                        width={ow()}
                        height={oh()}
                        fill="none"
                        stroke="var(--color-editor-accent)"
                        stroke-opacity="0.95"
                        stroke-width="1.5"
                        vector-effect="non-scaling-stroke"
                        style={{
                          "pointer-events": "none",
                          filter: "drop-shadow(0px 0px 2px rgba(0, 0, 0, 0.9))",
                        }}
                      />
                    </Show>
                  );
                }}
              </For>
            </Show>

            <BrushCursorOverlay
              isAltPressed={isAltPressed()}
              isPanning={isSpacePressed() || isPanning()}
            />
          </svg>

          {/* Live Cursor HUD Tooltip (HTML Overlay) */}
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

          {/* Gradient drag vector line and distance/angle overlay */}
          <GradientOverlay />

          {/* SelectionTransformOverlay →screen-space coordinates */}
          <Show when={(activeTool() === "move" || activeTool() === "shape") && showTransformControls()}>
            <SelectionTransformOverlay
              isNavigationMode={isSpacePressed() || isPanning()}
              onHudUpdate={setHudInfo}
              onComputeSnap={(rect) => {
                const engine = workspace.getActiveEngine();
                if (!engine) return { dx: 0, dy: 0, lines: [] };
                const snapToLayers = typeof snapToLayersEnabled === "function" ? snapToLayersEnabled() : true;
                const snapToCanvas = typeof snapToCanvasEnabled === "function" ? snapToCanvasEnabled() : true;
                const result = computeSnapAdjustment(
                  rect,
                  buildTransformSnapTargets(engine, engine.getWidth(), engine.getHeight(), {
                    snapToLayers,
                    snapToCanvas,
                  }),
                  8,
                  zoom(),
                );
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
                outline: "1.5px dashed var(--color-editor-accent, #E15A17)",
                "pointer-events": "none",
                "z-index": 45,
              }}
            />
          )}
        </Show>
        </Show>

        <BrushContextMenu />
        <CanvasContextMenu />

        {/* Text edit session overlay — mounts while textEditSession is open */}
        <TextEditOverlay />
      </div>
  );
}
