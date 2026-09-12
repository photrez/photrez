import type { WorkspaceManager } from "@/engine/workspace";
import type { WebGL2Backend } from "@/renderer/webgl2";
import type { RenderScheduler } from "@/renderer/scheduler";
import type { CropPreview } from "./cropState";
import type { ToolId } from "./tools/toolTypes";
import type { Point } from "@/viewport/transformGeometry";
import type { CropRect } from "@/viewport/cropGeometry";
import { isFacadeEnabled, isNativeAuthority } from "@/lib/protocol/facadeRegistry";
import { routeApplyCrop } from "./canvasRouting";

export interface CropPreviewControls {
  cropRect: () => CropPreview["rect"] | null;
  cropRotation: () => number;
  hiddenCropPreview: () => CropPreview | null;
  setCropRect: (rect: CropPreview["rect"] | null) => void;
  setCropRotation: (rot: number) => void;
  setHiddenCropPreview: (preview: CropPreview | null) => void;
}

export function clearCropPreview(controls: Pick<CropPreviewControls, "setCropRect" | "setCropRotation">) {
  controls.setCropRect(null);
  controls.setCropRotation(0);
}

export function hideCropPreview(controls: CropPreviewControls) {
  const rect = controls.cropRect();
  if (!rect) return;
  controls.setHiddenCropPreview({
    rect: { x: rect.x, y: rect.y, w: rect.w, h: rect.h },
    rotation: controls.cropRotation(),
  });
  controls.setCropRect(null);
  controls.setCropRotation(0);
}

export function restoreHiddenCropPreview(controls: CropPreviewControls): boolean {
  const hidden = controls.hiddenCropPreview();
  if (!hidden) return false;
  controls.setCropRect({ ...hidden.rect });
  controls.setCropRotation(hidden.rotation);
  controls.setHiddenCropPreview(null);
  return true;
}

export function discardCropSession(controls: CropPreviewControls) {
  controls.setCropRect(null);
  controls.setCropRotation(0);
  controls.setHiddenCropPreview(null);
}

export function resetCropPreviewToCanvas(params: {
  engine: { getWidth: () => number; getHeight: () => number } | null;
  setCropRect: (rect: CropPreview["rect"] | null) => void;
  setCropRotation: (rot: number) => void;
  setHiddenCropPreview: (preview: CropPreview | null) => void;
}) {
  if (!params.engine) return;
  params.setCropRect({ x: 0, y: 0, w: params.engine.getWidth(), h: params.engine.getHeight() });
  params.setCropRotation(0);
  params.setHiddenCropPreview(null);
}

export function applyCropPreview(params: {
  workspace: WorkspaceManager;
  renderer: WebGL2Backend;
  viewport: { width: number; height: number };
  cropRect: { x: number; y: number; w: number; h: number } | null;
  cropMode: "free" | "ratio" | "size";
  cropSizeTarget: { w: number; h: number } | null;
  cropDeletePixels: boolean;
  cropFillColor?: string | null;
  cropRotation: number;
  scheduler: RenderScheduler;
  setCropRect: (rect: CropPreview["rect"] | null) => void;
  setCropRotation: (rot: number) => void;
  setHiddenCropPreview: (preview: CropPreview | null) => void;
  setActiveTool: (tool: ToolId) => void;
  setSelectedLayerId: (id: string | null) => void;
  recenterViewport?: () => void;
}) {
  const engine = params.workspace.getActiveEngine();
  const rect = params.cropRect;
  if (!engine || !rect) return;

  const history = params.workspace.getActiveHistory();

  const cropOptions: {
    deleteCroppedPixels: boolean;
    targetSize: { w: number; h: number } | null;
    rotation: number;
    fillBackgroundColor?: string;
  } = {
    deleteCroppedPixels: params.cropDeletePixels,
    targetSize: params.cropMode === "size" && params.cropSizeTarget
      ? { w: Math.round(params.cropSizeTarget.w), h: Math.round(params.cropSizeTarget.h) }
      : null,
    rotation: params.cropRotation,
  };
  if (params.cropFillColor) {
    cropOptions.fillBackgroundColor = params.cropFillColor;
  }
  const cropX = Math.round(rect.x);
  const cropY = Math.round(rect.y);
  const cropW = Math.round(rect.w);
  const cropH = Math.round(rect.h);

  // Post-apply viewport resize + session cleanup. sweepLayers repeats the
  // texture re-upload on the default path; the routed path already re-uploaded
  // layer textures inside the route.
  const finish = (sweepLayers: boolean) => {
    params.recenterViewport?.();

    const dpr = window.devicePixelRatio || 1;
    params.renderer.resizeToViewport(params.viewport.width, params.viewport.height, dpr);

    if (sweepLayers) {
      for (const layer of engine.getLayers()) {
        if (layer.imageBitmap) {
          params.renderer.uploadImage(layer.id, layer.imageBitmap);
        }
      }
      params.scheduler.requestRender();
    }

    discardCropSession({
      cropRect: () => params.cropRect,
      cropRotation: () => params.cropRotation,
      hiddenCropPreview: () => null,
      setCropRect: params.setCropRect,
      setCropRotation: params.setCropRotation,
      setHiddenCropPreview: params.setHiddenCropPreview,
    });
    params.setActiveTool("move");
    params.setSelectedLayerId(null);
    engine.setActiveLayer(null);
  };

  // Routed path: the native applyCrop owns the document size and its undo entry,
  // so there is no TS history commit. On success the route has already re-uploaded
  // layer textures; "legacy" runs the default path below.
  if (isFacadeEnabled() && isNativeAuthority()) {
    void routeApplyCrop(engine, history, params.renderer, params.scheduler, cropX, cropY, cropW, cropH, cropOptions)
      .then((status) => {
        if (status === "applied") {
          finish(false);
          return;
        }
        if (status === "legacy") {
          history?.commit(engine.snapshot(), "Crop Canvas");
          engine.applyCrop(cropX, cropY, cropW, cropH, cropOptions);
          finish(true);
        }
        // "error": leave the crop session in place so the user can adjust it.
      })
      .catch(() => {});
    return;
  }

  // Default path (flag off) — unchanged.
  history?.commit(engine.snapshot(), "Crop Canvas");
  engine.applyCrop(cropX, cropY, cropW, cropH, cropOptions);
  finish(true);
}

export const CROP_REPLACEMENT_DRAG_THRESHOLD_PX = 3;

export function hasCropReplacementDragDistance(
  start: { clientX: number; clientY: number },
  current: { clientX: number; clientY: number },
): boolean {
  const dx = current.clientX - start.clientX;
  const dy = current.clientY - start.clientY;
  return dx * dx + dy * dy >= CROP_REPLACEMENT_DRAG_THRESHOLD_PX * CROP_REPLACEMENT_DRAG_THRESHOLD_PX;
}

export function createCropRectFromDocumentPoints(start: Point, end: Point): CropRect | null {
  const x = Math.min(start.x, end.x);
  const y = Math.min(start.y, end.y);
  const w = Math.abs(end.x - start.x);
  const h = Math.abs(end.y - start.y);

  if (w <= 0 || h <= 0) {
    return null;
  }

  return { x, y, w, h };
}
