// SPDX-License-Identifier: AGPL-3.0-or-later
// Crop overlay style memos for CanvasViewport (classic + modern fill previews).
// Extracted from CanvasViewport.tsx (report #20 phase 3) — behavior must stay
// identical to the inline memos it replaces.

import { createMemo } from "solid-js";
import type { JSX } from "solid-js";
import { docFrameToScreenFrame } from "@/viewport/modernCropGeometry";

export interface ModernCropImageTransform {
  offsetX: number;
  offsetY: number;
  scale: number;
  rotation: number;
}

export interface UseCropOverlayStylesParams {
  cropRect: () => { x: number; y: number; w: number; h: number } | null;
  cropRotation: () => number;
  zoom: () => number;
  pan: () => { x: number; y: number };
  cropFillSource: () => string;
  bgColor: () => string;
  cropFillCustomColor: () => string;
  modernCropFrame: () => { x: number; y: number; w: number; h: number } | null;
  modernCropImageTransform: () => ModernCropImageTransform;
  docWidth: () => number;
  docHeight: () => number;
}

export function useCropOverlayStyles(params: UseCropOverlayStylesParams) {
  const resolvedCropFillColor = createMemo(() =>
    params.cropFillSource() === "background"
      ? params.bgColor()
      : params.cropFillCustomColor(),
  );

  const classicCropFillPreviewStyle = createMemo(() => {
    const rect = params.cropRect();
    if (!rect) return {};
    return {
      position: "absolute" as const,
      width: `${rect.w * params.zoom()}px`,
      height: `${rect.h * params.zoom()}px`,
      "background-color": resolvedCropFillColor(),
      transform: `translate(${params.pan().x + rect.x * params.zoom()}px, ${params.pan().y + rect.y * params.zoom()}px) rotate(${params.cropRotation()}deg)`,
      "transform-origin": "center",
      "pointer-events": "none" as const,
      "will-change": "transform",
    } satisfies JSX.CSSProperties;
  });

  // Derived screen-space frame from doc-space frame + zoom + pan.
  // Used for rendering, pivot computation, and overlay positioning.
  const modernCropScreenFrame = createMemo(() => {
    return docFrameToScreenFrame(params.modernCropFrame(), params.zoom(), params.pan());
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
    } satisfies JSX.CSSProperties;
  });

  // Derived canvas screen rect for expansion fill indicator →memo outside Show to guarantee reactivity
  // The dashed canvas boundary line represents the TRANSFORMED image boundary,
  // including offset/scale/rotation from modernCropImageTransform. This ensures
  // the expansion fill and dashed outline track the image, not just the
  // viewport-space doc position.
  const canvasScreenRect = createMemo(() => {
    const p = params.pan();
    const z = params.zoom();
    const docW = params.docWidth();
    const docH = params.docHeight();
    const transform = params.modernCropImageTransform();
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

  return {
    resolvedCropFillColor,
    classicCropFillPreviewStyle,
    modernCropScreenFrame,
    modernCropFillPreviewStyle,
    canvasScreenRect,
  };
}
