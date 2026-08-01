// SPDX-License-Identifier: AGPL-3.0-or-later
import { documentToLayerLocal } from "@/viewport/transformGeometry";
import { floodFill, type FillMask } from "@/features/fill/fillOperations";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { showToast } from "../../Toast";
import { trySetPointerCapture } from "../../tools/pointerCapture";
import type { PointerToolContext } from "./pointerToolContext";

/**
 * Paint Bucket: click-to-fill. Runs flood fill on the active layer at the
 * clicked document point, honoring the selection mask and fill tolerance.
 * Returns true when the bucket tool handled the event.
 */
export function applyPaintBucketFill(
  ctx: PointerToolContext,
  e: PointerEvent,
): boolean {
  const { editor } = ctx;
  const { workspace, renderer, scheduler, fgColor, fillTolerance, fillContiguous } = editor;

  if (editor.activeTool() !== "paintBucket") return false;

  const engine = workspace.getActiveEngine();
  const history = workspace.getActiveHistory();
  if (!engine || !history) return true;

  const layerId = engine.getActiveLayerId();
  if (!layerId) { showToast("No editable layer selected", "warn"); trySetPointerCapture(ctx.getCanvasRef(), e.pointerId); return true; }
  const layer = engine.getLayer(layerId);
  if (!layer || layer.locked) { showToast("Layer is locked", "warn"); trySetPointerCapture(ctx.getCanvasRef(), e.pointerId); return true; }
  if (!layer.visible) { showToast("Layer is hidden", "warn"); trySetPointerCapture(ctx.getCanvasRef(), e.pointerId); return true; }
  if (layer.lockTransparency) { showToast("Transparent pixels protected", "warn"); trySetPointerCapture(ctx.getCanvasRef(), e.pointerId); return true; }

  const coords = ctx.getDocCoords(e);
  const bitmap = engine.getLayerImageBitmap(layerId);
  if (!bitmap) { showToast("Layer has no image data", "warn"); return true; }

  const offscreen = typeof OffscreenCanvas !== "undefined"
    ? new OffscreenCanvas(layer.width, layer.height)
    : (() => {
        const el = document.createElement("canvas");
        el.width = layer.width;
        el.height = layer.height;
        return el;
      })();
  const ctx2d = offscreen.getContext("2d") as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null;
  if (!ctx2d) return true;
  ctx2d.drawImage(bitmap, 0, 0);
  const imgData = ctx2d.getImageData(0, 0, layer.width, layer.height);

  // Layer-local click coords (accounts for scale/rotation/flip, not just translation)
  const localPt = documentToLayerLocal(coords.x, coords.y, layer.transform, layer.width, layer.height);
  const lx = Math.floor(localPt.x);
  const ly = Math.floor(localPt.y);

  // Fill colour from foreground
  const hex = fgColor().replace("#", "");
  const fillR = parseInt(hex.slice(0, 2), 16);
  const fillG = parseInt(hex.slice(2, 4), 16);
  const fillB = parseInt(hex.slice(4, 6), 16);

  // Build selection mask
  const sel = engine.getSelection();
  let fillMask: FillMask | undefined;
  if (sel) {
    const aabb = SelectionOperations.selectionToLayerAabb(sel, layer.transform, layer.width, layer.height);
    fillMask = {
      x: Math.round(aabb.x), y: Math.round(aabb.y),
      w: Math.max(0, Math.round(aabb.width)), h: Math.max(0, Math.round(aabb.height)),
      shape: sel.shape, inverted: sel.inverted,
    };
  }

  floodFill(imgData, lx, ly, fillR, fillG, fillB, 255, fillTolerance(), fillMask ?? null, fillContiguous());

  const preSnapshot = engine.snapshot();
  ctx2d.putImageData(imgData, 0, 0);
  // OffscreenCanvas → true ImageBitmap. HTMLCanvasElement fallback (only
  // reachable when OffscreenCanvas is unavailable) is structurally compatible
  // (width/height + drawImage source) but not typed as ImageBitmap.
  const newBitmap = typeof OffscreenCanvas !== "undefined" && offscreen instanceof OffscreenCanvas
    ? offscreen.transferToImageBitmap()
    : offscreen as unknown as ImageBitmap;
  try {
    engine.setLayerImageBitmap(layerId, newBitmap);
    renderer?.uploadImage(layerId, newBitmap);
  } catch (err) {
    showToast(`Fill failed: ${err instanceof Error ? err.message : 'Unknown error'}`, "error");
    trySetPointerCapture(ctx.getCanvasRef(), e.pointerId);
    return true;
  }
  history.commit(preSnapshot, "Paint Bucket Fill");
  scheduler.requestRender();

  trySetPointerCapture(ctx.getCanvasRef(), e.pointerId);
  return true;
}
