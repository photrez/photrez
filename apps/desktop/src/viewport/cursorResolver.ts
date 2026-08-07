import type { ToolId } from "@/components/editor/tools/toolTypes";
import { getCursorForHandle } from "./transformGeometry";
import { getRotateCursorByPos, getRotateCursorForHandle } from "./cursorRotate";

export type ToolType = ToolId;

export interface CursorContext {
  isSpacePressed: boolean;
  isPanning: boolean;
  activeTool: ToolType;
  isAltPressed: boolean;
  hoverHandle: string | null;
  isLayerLocked: boolean;
  eyedropperTarget: string | null;
  /** When a non-modal color picker is open, canvas clicks sample color. */
  colorPickerOpen?: boolean;
  /** For rotation-aware cursor: the selected layer's current rotation */
  layerRotation?: number;
  /** For rotation-aware cursor: the selected layer's current scaleX */
  layerScaleX?: number;
  /** For rotation-aware cursor: the selected layer's current scaleY */
  layerScaleY?: number;
  /** Current mouse position (screen-space) for dynamic rotate cursor */
  hoverPos?: { x: number; y: number } | null;
  /** Layer bounding box (document-space) for dynamic rotate cursor */
  layerBoundingBox?: { x: number; y: number; w: number; h: number } | null;
}

const PAINT_BUCKET_CURSOR_SVG = `data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="%23ffffff" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="m19 11-8-8-8.6 8.6a2 2 0 0 0 0 2.8l5.2 5.2a2 2 0 0 0 2.8 0L19 11Z" fill="%23e15a17" fill-opacity="0.9"/><path d="m5 2 5 5"/><line x1="2" y1="13" x2="17" y2="13"/><circle cx="19" cy="18" r="2.5" fill="%23e15a17" stroke="%23ffffff" stroke-width="1"/></svg>`;
const PAINT_BUCKET_CURSOR = `url('${PAINT_BUCKET_CURSOR_SVG}') 3 18, crosshair`;

const GRADIENT_CURSOR_SVG = `data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none"><line x1="12" y1="2" x2="12" y2="22" stroke="%23000000" stroke-width="2.5" stroke-linecap="round"/><line x1="12" y1="2" x2="12" y2="22" stroke="%23ffffff" stroke-width="1.2" stroke-linecap="round"/><line x1="2" y1="12" x2="22" y2="12" stroke="%23000000" stroke-width="2.5" stroke-linecap="round"/><line x1="2" y1="12" x2="22" y2="12" stroke="%23ffffff" stroke-width="1.2" stroke-linecap="round"/><circle cx="12" cy="12" r="3" fill="%23e15a17" stroke="%23ffffff" stroke-width="1"/></svg>`;
const GRADIENT_CURSOR = `url('${GRADIENT_CURSOR_SVG}') 12 12, crosshair`;

export function resolveCursor(ctx: CursorContext): string {
  if (ctx.eyedropperTarget) return "crosshair";
  if (ctx.colorPickerOpen) return "crosshair";
  if (ctx.isSpacePressed) return ctx.isPanning ? "grabbing" : "grab";
  if (ctx.isAltPressed && (ctx.activeTool === "brush" || ctx.activeTool === "eraser")) return "copy";
  if (ctx.activeTool === "move" && ctx.isLayerLocked) return "default";

  if (ctx.activeTool === "move" && ctx.hoverHandle && ctx.hoverHandle !== "move" && !ctx.hoverHandle.startsWith("rotate")) {
    return getCursorForHandle(ctx.hoverHandle, ctx.layerRotation ?? 0, ctx.layerScaleX ?? 1, ctx.layerScaleY ?? 1);
  }

  if (ctx.activeTool === "move" && ctx.hoverHandle && ctx.hoverHandle.startsWith("rotate")) {
    if (ctx.hoverPos && ctx.layerBoundingBox) {
      return getRotateCursorByPos(ctx.hoverPos, ctx.layerBoundingBox);
    }
    return getRotateCursorForHandle(
      "se",
      ctx.layerRotation ?? 0,
      ctx.layerScaleX ?? 1,
      ctx.layerScaleY ?? 1
    );
  }
  if (ctx.activeTool === "move" && ctx.hoverHandle === "move") return "move";

  if (ctx.activeTool === "selection") return "crosshair";

  if (ctx.activeTool === "crop" && ctx.hoverHandle && ctx.hoverHandle !== "move") {
    return getCursorForHandle(ctx.hoverHandle, 0, 1, 1);
  }
  if (ctx.activeTool === "crop" && ctx.hoverHandle === "move") return "move";
  if (ctx.activeTool === "crop") return "crosshair";
  if (ctx.activeTool === "brush" || ctx.activeTool === "eraser") return "none";
  if (ctx.activeTool === "eyedropper") return "crosshair";
  if (ctx.activeTool === "paintBucket") return PAINT_BUCKET_CURSOR;
  if (ctx.activeTool === "gradient") return GRADIENT_CURSOR;
  if (ctx.activeTool === "shape") return "crosshair";
  return "default";
}
