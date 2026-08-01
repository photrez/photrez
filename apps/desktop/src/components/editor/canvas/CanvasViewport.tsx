import { createMemo, createSignal, createEffect, Show } from "solid-js";
import { screenToDocument } from "@/viewport/coords";
import { computeSnapAdjustment } from "@/viewport/smartGuides";
import { buildTransformSnapTargets } from "@/viewport/transformSnapTargets";
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
import { fitCropRectToAspect } from "@/viewport/cropAutoFit";
import {
  clearCropPreview,
  applyCropPreview,
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
    getEngine: () => workspace.getActiveEngine(),
    screenToDocumentPoint,
    onCanvasPointerDown,
    setSelectedLayerId,
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
          <Show when={activeTool() === "move" && showTransformControls()}>
            <SelectionTransformOverlay
              isNavigationMode={isSpacePressed() || isPanning()}
              onHudUpdate={setHudInfo}
              onComputeSnap={(rect) => {
                const engine = workspace.getActiveEngine();
                if (!engine) return { dx: 0, dy: 0, lines: [] };
                const result = computeSnapAdjustment(
                  rect,
                  buildTransformSnapTargets(engine, engine.getWidth(), engine.getHeight()),
                  5,
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
