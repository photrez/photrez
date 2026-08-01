// SPDX-License-Identifier: AGPL-3.0-or-later
import { constrainCropRectToDocument } from "@/viewport/cropGeometry";
import { docFrameToScreenFrame, getModernCropApplyRotation, modernFrameToCropRect } from "@/viewport/modernCropGeometry";
import { applyCropPreview } from "../../cropToolActions";
import type { KeyboardShortcutContext } from "./context";

/**
 * Crop tool keyboard shortcuts: Ctrl+Shift+Z / Ctrl+Z / Ctrl+Y mini undo /
 * redo (modern or classic), Enter = apply crop, Escape = cancel, Arrow keys
 * nudge the crop rect (or modern frame).
 */
export function handleCropToolKey(
  ctx: KeyboardShortcutContext,
  e: KeyboardEvent,
): boolean {
  const { editor, options } = ctx;
  const {
    workspace,
    renderer,
    scheduler,
    activeTool,
    setActiveTool,
    zoom,
    docWidth,
    docHeight,
    fgColor,
    bgColor,
    cropRect,
    setCropRect,
    cropMode,
    cropSizeTarget,
    cropRotation,
    setCropRotation,
    hiddenCropPreview,
    setHiddenCropPreview,
    cropInteractionMode,
    undoLastCrop,
    redoCrop,
    canCropUndo,
    canCropRedo,
    undoModernCrop,
    redoModernCrop,
    commitModernCropState,
    commitCropState,
    cropDeletePixels,
    cropFillEnabled,
    cropFillSource,
    cropFillCustomColor,
    modernCropFrame,
    modernCropImageTransform,
    setModernCropImageTransform,
    resetModernCrop,
    viewportWidth,
    viewportHeight,
    pan,
    setViewportState,
    setSelectedLayerId,
  } = editor;

  const resolvedCropFillColor = () => (
    cropFillSource() === "background"
      ? (typeof bgColor === "function" ? bgColor() : "#ffffff")
      : cropFillCustomColor()
  );
  const isCropFillEnabled = () => (
    typeof cropFillEnabled === "function" ? cropFillEnabled() : false
  );

  if (activeTool() !== "crop") return false;

  const ctrl = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  if (ctrl && e.shiftKey && key === "z") {
    e.preventDefault();
    e.stopPropagation();
    if (cropInteractionMode() === "modern") {
      redoModernCrop();
    } else if (canCropRedo()) {
      const entry = redoCrop();
      if (entry) {
        setCropRect(entry.rect);
        setCropRotation(entry.rotation);
      }
    }
    return true;
  }
  if (ctrl && key === "z") {
    e.preventDefault();
    e.stopPropagation();
    if (cropInteractionMode() === "modern") {
      undoModernCrop();
    } else if (canCropUndo()) {
      const entry = undoLastCrop();
      if (entry) {
        setCropRect(entry.rect);
        setCropRotation(entry.rotation);
      }
    }
    return true;
  }
  if (ctrl && key === "y") {
    e.preventDefault();
    e.stopPropagation();
    if (cropInteractionMode() === "modern") {
      redoModernCrop();
    } else if (canCropRedo()) {
      const entry = redoCrop();
      if (entry) {
        setCropRect(entry.rect);
        setCropRotation(entry.rotation);
      }
    }
    return true;
  }
  if (e.key === "Enter") {
    e.preventDefault();
    if (cropInteractionMode() === "modern" && modernCropFrame()) {
      // Convert doc-coord frame to screen space for modernFrameToCropRect
      const screenFrame = docFrameToScreenFrame(modernCropFrame(), zoom(), pan());
      const rect = modernFrameToCropRect({
        frame: screenFrame!,
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
        workspace, renderer,
        viewport: { width: viewportWidth(), height: viewportHeight() },
        cropRect: rect,
        cropMode: cropMode(),
        cropSizeTarget: cropSizeTarget(),
        cropDeletePixels: cropDeletePixels(),
        cropFillColor: isCropFillEnabled() ? resolvedCropFillColor() : null,
        cropRotation: getModernCropApplyRotation(modernCropImageTransform().rotation),
        scheduler,
        setCropRect, setCropRotation, setHiddenCropPreview, setActiveTool,
        setSelectedLayerId,
        recenterViewport: options.fitToScreenAndRender,
      });
      resetModernCrop();
    } else {
      applyCropPreview({
        workspace,
        renderer,
        viewport: { width: viewportWidth(), height: viewportHeight() },
        cropRect: cropRect(),
        cropMode: cropMode(),
        cropSizeTarget: cropSizeTarget(),
        cropDeletePixels: cropDeletePixels(),
        cropFillColor: isCropFillEnabled() ? resolvedCropFillColor() : null,
        cropRotation: cropRotation(),
        scheduler,
        setCropRect,
        setCropRotation,
        setHiddenCropPreview,
        setActiveTool,
        setSelectedLayerId,
        recenterViewport: options.fitToScreenAndRender,
      });
    }
    return true;
  }
  if (e.key === "Escape") {
    e.preventDefault();
    setCropRect(null);
    setCropRotation(0);
    setHiddenCropPreview(null);
    if (cropInteractionMode() === "modern") {
      resetModernCrop();
    }
    scheduler.requestRender();
    return true;
  }
  if (e.key.startsWith("Arrow") && (cropRect() || modernCropFrame())) {
    e.preventDefault();
    const step = e.shiftKey ? 10 : 1;
    let dx = 0;
    let dy = 0;
    if (e.key === "ArrowUp") dy = -step;
    else if (e.key === "ArrowDown") dy = step;
    else if (e.key === "ArrowLeft") dx = -step;
    else if (e.key === "ArrowRight") dx = step;

    if (cropInteractionMode() === "modern" && modernCropFrame()) {
      if (!e.repeat) commitModernCropState();
      const t = modernCropImageTransform();
      setModernCropImageTransform({
        ...t,
        offsetX: t.offsetX + dx,
        offsetY: t.offsetY + dy,
      });
    } else {
      const rect = cropRect()!;
      if (!e.repeat) commitCropState(rect, cropRotation());
      const newRect = constrainCropRectToDocument(
        { ...rect, x: rect.x + dx, y: rect.y + dy },
        docWidth(),
        docHeight()
      );

      const actualDx = newRect.x - rect.x;
      const actualDy = newRect.y - rect.y;

      setCropRect(newRect);

      const currentPan = pan();
      setViewportState({
        x: currentPan.x - actualDx * zoom(),
        y: currentPan.y - actualDy * zoom(),
        zoom: zoom(),
      });
    }
    scheduler.requestRender();
    return true;
  }

  return false;
}
