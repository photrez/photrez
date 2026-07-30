import type { LayerNode } from "./types";
import { getCanvasCompositeOperation } from "./blendModes";

export function drawLayerToContext(ctx: OffscreenCanvasRenderingContext2D, layer: LayerNode): void {
  if (!layer.visible || layer.opacity <= 0 || !layer.imageBitmap) return;

  ctx.save();
  ctx.globalAlpha = layer.opacity;
  ctx.globalCompositeOperation = getCanvasCompositeOperation(layer.blendMode);

  const lw = layer.width;
  const lh = layer.height;
  const sx = layer.transform.scaleX;
  const sy = layer.transform.scaleY;
  const cx = layer.transform.x + (lw * Math.abs(sx)) / 2;
  const cy = layer.transform.y + (lh * Math.abs(sy)) / 2;

  ctx.translate(cx, cy);
  if (layer.transform.rotation) {
    ctx.rotate((layer.transform.rotation * Math.PI) / 180);
  }
  const flipX = layer.transform.flipH ? -1 : 1;
  const flipY = layer.transform.flipV ? -1 : 1;
  ctx.scale(sx * flipX, sy * flipY);
  ctx.drawImage(layer.imageBitmap, -lw / 2, -lh / 2);
  ctx.restore();
}

export function compositeTwoLayers(
  top: LayerNode,
  bottom: LayerNode,
  width: number,
  height: number
): ImageBitmap | null {
  try {
    if (typeof OffscreenCanvas !== "undefined") {
      const offscreen = new OffscreenCanvas(width, height);
      const ctx = offscreen.getContext("2d");
      if (ctx) {
        drawLayerToContext(ctx, bottom);
        drawLayerToContext(ctx, top);
        return offscreen.transferToImageBitmap();
      }
    }
  } catch (err: unknown) {
    if (import.meta.env.DEV) console.error("compositeTwoLayers failed:", err);
    // Return null — callers show a toast when compositing fails.
  }
  return null;
}

export function compositeAllLayers(
  layers: readonly LayerNode[],
  width: number,
  height: number
): ImageBitmap | null {
  try {
    if (typeof OffscreenCanvas !== "undefined") {
      const offscreen = new OffscreenCanvas(width, height);
      const ctx = offscreen.getContext("2d");
      if (ctx) {
        for (let i = layers.length - 1; i >= 0; i--) {
          drawLayerToContext(ctx, layers[i]);
        }
        return offscreen.transferToImageBitmap();
      }
    }
  } catch (err: unknown) {
    if (import.meta.env.DEV) console.error("compositeAllLayers failed:", err);
    // Return null — callers show a toast when compositing fails.
  }
  return null;
}
