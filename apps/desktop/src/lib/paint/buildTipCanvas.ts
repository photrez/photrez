/**
 * buildTipCanvas — renders a brush-tip alpha profile onto an OffscreenCanvas.
 *
 * Extracted from the deleted benchPixelParity.ts buildTip helper.
 * Used by rustShadow.ts shadow/parity autorun to create a CanvasImageSource
 * that can be drawn via ctx.drawImage() for deterministic brush-dab stamping.
 *
 * This is a thin wrapper around rasterizeBrushTip from brushTipMask.ts.
 */

import { rasterizeBrushTip } from "@/components/editor/brushTipMask";

/**
 * Build an OffscreenCanvas containing the brush tip alpha profile.
 *
 * @param brushDiameter - brush size in pixels
 * @param hardness - hardness 0..1
 * @returns OffscreenCanvas suitable for ctx.drawImage()
 */
export function buildTipCanvas(
  brushDiameter: number,
  hardness: number,
): OffscreenCanvas {
  const tip = rasterizeBrushTip(brushDiameter, hardness);
  const size = tip.width;
  const canvas = new OffscreenCanvas(size, size);
  const ctx = canvas.getContext("2d") as OffscreenCanvasRenderingContext2D;
  const imageData = ctx.createImageData(size, size);
  const data = imageData.data;
  for (let i = 0; i < tip.data.length; i++) {
    const alpha = Math.round(tip.data[i] * 255);
    data[i * 4 + 0] = 255; // R
    data[i * 4 + 1] = 255; // G
    data[i * 4 + 2] = 255; // B
    data[i * 4 + 3] = alpha; // A
  }
  ctx.putImageData(imageData, 0, 0);
  return canvas;
}
