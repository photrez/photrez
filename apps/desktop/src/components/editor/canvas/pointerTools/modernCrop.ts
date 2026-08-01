// SPDX-License-Identifier: AGPL-3.0-or-later
import { snapCropRect } from "@/viewport/cropSnap";
import { getDefaultModernCropFrame, clampFrameToProjectedBounds } from "@/viewport/modernCropGeometry";
import { resetCropPreviewToCanvas, restoreHiddenCropPreview } from "../../cropToolActions";
import type { ToolContext, ToolType } from "@/viewport/input-handler";
import type { ModernDragState, PointerToolContext } from "./pointerToolContext";

const DRAG_CREATE_THRESHOLD = 5;
const MIN_CROP_SIZE = 100;

/**
 * Crop pointer-down: track drag start for drag-to-create even when a frame
 * exists. Modern mode sets modernDragStart; classic mode arms the
 * pending-click flag. Returns true only when the event must stop (no
 * container); otherwise the hook continues dispatch (the modern-crop
 * "skip engine" guard further down still applies).
 */
export function startCropDrag(
  ctx: PointerToolContext,
  e: PointerEvent,
  state: ModernDragState,
): boolean {
  const { editor } = ctx;
  const { activeTool, cropInteractionMode, cropRect } = editor;

  if (activeTool() === "crop" && e.button === 0) {
    if (cropInteractionMode() === "modern") {
      // Track drag start for drag-to-create even when frame exists.
      // The ModernCropOverlay SVG on top catches clicks on the frame
      // (move rect, handles, rotate ring) with stopPropagation().
      // Clicks on the mask area (outside the frame) fall through to
      // the canvas →those start a new drag-create.
      const viewport = ctx.getCanvasContainerRef();
      if (!viewport) return true;
      const rect = viewport.getBoundingClientRect();
      state.start = {
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
      };
      state.exceededThreshold = false;
      state.isPendingCropClick = false;
    } else {
      state.isPendingCropClick = !cropRect();
    }
  } else {
    state.isPendingCropClick = false;
  }
  return false;
}

/**
 * Modern crop drag-to-create: show selection preview rect (with snap).
 * Returns true when the event was consumed by the modern drag.
 */
export function trackModernCropDrag(
  ctx: PointerToolContext,
  e: PointerEvent,
  state: ModernDragState,
): boolean {
  const { editor } = ctx;
  const { activeTool, cropInteractionMode, zoom, setModernCropFrame } = editor;
  const { setSnapLines, setCropDragPreview } = ctx;

  if (
    activeTool() !== "crop" ||
    cropInteractionMode() !== "modern" ||
    !state.start
  ) {
    return false;
  }

  const container = ctx.getCanvasContainerRef();
  if (!container) return true;
  const rect = container.getBoundingClientRect();
  const ex = e.clientX - rect.left;
  const ey = e.clientY - rect.top;
  const dx = ex - state.start.x;
  const dy = ey - state.start.y;

  if (!state.exceededThreshold) {
    if (Math.abs(dx) < DRAG_CREATE_THRESHOLD && Math.abs(dy) < DRAG_CREATE_THRESHOLD) {
      setCropDragPreview(null);
      return true;
    }
    state.exceededThreshold = true;
    // Clear existing frame once drag exceeds threshold →visual
    // feedback that a new crop is being created instead of moved.
    setModernCropFrame(null);
  }

  state.end = { x: ex, y: ey };

  // Build screen-space rect
  const sx = Math.min(state.start.x, ex);
  const sy = Math.min(state.start.y, ey);
  const sw = Math.abs(dx);
  const sh = Math.abs(dy);

  // Apply snap if enabled
  const z = zoom();
  const canvasEl = ctx.getCanvasRef();
  const canvasRect = canvasEl?.getBoundingClientRect();
  // Document origin in viewport space. In Modern mode the canvas uses
  // CSS transforms (not left/top from pan), so compute visual offset
  // directly from element bounds.
  const docOriginX = canvasRect ? canvasRect.left - rect.left : 0;
  const docOriginY = canvasRect ? canvasRect.top - rect.top : 0;
  const cst = ctx.cropSnapTargets?.();
  if (
    cst &&
    ctx.moveSnapEnabled?.() !== false &&
    !e.altKey
  ) {
    const snapTargets = cst;
    // Convert screen rect to doc-space for snapping
    const docRect = {
      x: (sx - docOriginX) / z,
      y: (sy - docOriginY) / z,
      w: sw / z,
      h: sh / z,
    };
    const threshold = 12 / z;
    const snapped = snapCropRect(docRect, "new", snapTargets, threshold);
    setSnapLines(snapped.lines);
    // Convert snapped doc rect back to screen-space
    const screenSnapped = {
      x: snapped.rect.x * z + docOriginX,
      y: snapped.rect.y * z + docOriginY,
      w: snapped.rect.w * z,
      h: snapped.rect.h * z,
    };
    setCropDragPreview(screenSnapped);
    state.snappedPreview = screenSnapped;
  } else {
    setSnapLines([]);
    setCropDragPreview({ x: sx, y: sy, w: sw, h: sh });
    state.snappedPreview = null;
  }
  return true; // Don't dispatch to handlePointerMove
}

/**
 * Crop pointer-up: modern drag end (commit frame or click-to-create default
 * frame) and classic pending-click fallback (reset canvas position + restore
 * hidden preview). Void: the hook continues to the selection-marquee sync
 * afterwards, exactly like the original inline flow.
 */
export function handleCropPointerUp(
  ctx: PointerToolContext,
  e: PointerEvent,
  state: ModernDragState,
  coords: { x: number; y: number },
  interactiveState: ToolContext,
  tool: ToolType,
): void {
  const { editor } = ctx;
  const {
    activeTool,
    cropInteractionMode,
    zoom,
    docWidth,
    docHeight,
    viewportWidth,
    viewportHeight,
    cropMode,
    cropAspect,
    cropSizeTarget,
    setViewportState,
    setModernCropImageTransform,
    setModernCropFrame,
    scheduler,
    cropRect,
    cropRotation,
    hiddenCropPreview,
    setCropRect,
    setCropRotation,
    setHiddenCropPreview,
  } = editor;

  // Modern crop: handle drag end or click fallback
  if (
    activeTool() === "crop" &&
    cropInteractionMode() === "modern" &&
    state.start
  ) {
    if (state.exceededThreshold && state.end) {
      commitDragCreateFrame(
        ctx,
        state,
        state.start.x, state.start.y,
        state.end.x, state.end.y,
        e.shiftKey,
      );
    } else if (!editor.modernCropFrame()) {
      // Click behavior →create default frame and reset canvas position to center
      const mode = cropMode();
      const ratioAspect = cropAspect();
      const sizeTarget = cropSizeTarget();
      const aspect = mode === "ratio" && ratioAspect
        ? ratioAspect
        : mode === "size" && sizeTarget && sizeTarget.w > 0 && sizeTarget.h > 0
          ? { w: sizeTarget.w, h: sizeTarget.h }
          : null;

      const scale = 1;
      const centerPanX = (viewportWidth() - docWidth() * zoom() * scale) / 2;
      const centerPanY = (viewportHeight() - docHeight() * zoom() * scale) / 2;
      setViewportState({ x: centerPanX, y: centerPanY, zoom: zoom() });
      setModernCropImageTransform({ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 });

      setModernCropFrame(getDefaultModernCropFrame({
        viewportWidth: viewportWidth(),
        viewportHeight: viewportHeight(),
        docWidth: docWidth(),
        docHeight: docHeight(),
        zoom: zoom(),
        aspect,
        panX: centerPanX,
        panY: centerPanY,
      }));
      scheduler.requestRender();
    }
    ctx.setCropDragPreview(null);
    state.reset();
    return;
  }

  // `tool` is captured by the hook BEFORE dragTool is reset — matching the
  // original `(interactiveState.dragTool ?? activeTool())` semantics.
  if (tool === "crop" && state.isPendingCropClick) {
    const dx = Math.abs(coords.x - interactiveState.dragStart.x);
    const dy = Math.abs(coords.y - interactiveState.dragStart.y);
    if (dx <= 2 && dy <= 2) {
      // Reset canvas position to center
      const centerPanX = (viewportWidth() - docWidth() * zoom()) / 2;
      const centerPanY = (viewportHeight() - docHeight() * zoom()) / 2;
      setViewportState({ x: centerPanX, y: centerPanY, zoom: zoom() });

      const restored = restoreHiddenCropPreview({
        cropRect,
        cropRotation,
        hiddenCropPreview,
        setCropRect,
        setCropRotation,
        setHiddenCropPreview,
      });
      if (!restored) {
        resetCropPreviewToCanvas({
          engine: editor.workspace.getActiveEngine()!,
          setCropRect,
          setCropRotation,
          setHiddenCropPreview,
        });
      }
      scheduler.requestRender();
    }
    state.isPendingCropClick = false;
  }
}

function commitDragCreateFrame(
  ctx: PointerToolContext,
  state: ModernDragState,
  startX: number, startY: number, endX: number, endY: number, shiftKey: boolean,
) {
  const { editor } = ctx;
  const {
    viewportWidth,
    viewportHeight,
    zoom,
    pan,
    cropMode,
    cropAspect,
    cropSizeTarget,
    docWidth,
    docHeight,
    setModernCropFrame,
    setModernCropImageTransform,
    scheduler,
  } = editor;
  const vw = viewportWidth();
  const vh = viewportHeight();
  const z = zoom();
  const p = pan();
  const snappedPreview = state.snappedPreview;

  // Compute selection bounds in DOCUMENT coordinates
  let docSelW: number;
  let docSelH: number;
  let docSelCenterX: number;
  let docSelCenterY: number;

  if (snappedPreview) {
    // snappedPreview is in screen space → convert to doc coords
    docSelW = snappedPreview.w / z;
    docSelH = snappedPreview.h / z;
    docSelCenterX = (snappedPreview.x + snappedPreview.w / 2 - p.x) / z;
    docSelCenterY = (snappedPreview.y + snappedPreview.h / 2 - p.y) / z;
  } else {
    const docStartX = (startX - p.x) / z;
    const docStartY = (startY - p.y) / z;
    const docEndX = (endX - p.x) / z;
    const docEndY = (endY - p.y) / z;
    docSelW = Math.abs(docEndX - docStartX);
    docSelH = Math.abs(docEndY - docStartY);
    docSelCenterX = Math.min(docStartX, docEndX) + docSelW / 2;
    docSelCenterY = Math.min(docStartY, docEndY) + docSelH / 2;
  }

  const mode = cropMode();
  const ratioAspect = cropAspect();
  const sizeTarget = cropSizeTarget();

  let frameW: number;
  let frameH: number;

  if (mode === "free" && shiftKey) {
    const size = Math.max(docSelW, docSelH);
    frameW = size;
    frameH = size;
  } else if (mode === "free") {
    frameW = docSelW;
    frameH = docSelH;
  } else if (mode === "ratio" && ratioAspect && ratioAspect.w > 0 && ratioAspect.h > 0) {
    const ar = ratioAspect.w / ratioAspect.h;
    const area = Math.max(docSelW * docSelH, MIN_CROP_SIZE * MIN_CROP_SIZE);
    frameW = Math.sqrt(area * ar);
    frameH = frameW / ar;
  } else if (mode === "size" && sizeTarget && sizeTarget.w > 0 && sizeTarget.h > 0) {
    frameW = sizeTarget.w;
    frameH = sizeTarget.h;
  } else {
    frameW = docSelW;
    frameH = docSelH;
  }

  frameW = Math.max(MIN_CROP_SIZE, frameW);
  frameH = Math.max(MIN_CROP_SIZE, frameH);

  const clamped = clampFrameToProjectedBounds(
    { x: 0, y: 0, w: frameW, h: frameH },
    { w: docWidth(), h: docHeight() },
    MIN_CROP_SIZE,
  );

  // Center frame at viewport center in document coordinates
  const docCenterX = (vw / 2 - p.x) / z;
  const docCenterY = (vh / 2 - p.y) / z;
  const frame = {
    ...clamped,
    x: Math.round(docCenterX - clamped.w / 2),
    y: Math.round(docCenterY - clamped.h / 2),
  };

  setModernCropFrame(frame);

  // Shift image so selection center maps to viewport center (screen pixels)
  const vpCenterX = vw / 2;
  const vpCenterY = vh / 2;
  const selCenterScreenX = docSelCenterX * z + p.x;
  const selCenterScreenY = docSelCenterY * z + p.y;
  setModernCropImageTransform({
    ...editor.modernCropImageTransform(),
    offsetX: vpCenterX - selCenterScreenX,
    offsetY: vpCenterY - selCenterScreenY,
  });
  scheduler.requestRender();
}
