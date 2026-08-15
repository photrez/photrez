/**
 * Shared rotate-band geometry for Classic and Modern crop overlays.
 *
 * Band geometry:
 *   Side reach:   20px screen-space
 *   Corner reach: ~30px (20√2 + 2 ≈ 30.3px via cornerRadius=2)
 *
 * The band is a ring between an outer rounded rect and the crop box inner rect,
 * rendered with SVG evenodd fill rule.
 */

export const ROTATE_BAND_PX = 100;
export const ROTATE_CORNER_EXTRA = 2;

/**
 * Calculates adaptive rotate band width (clamped between 32px and 60px)
 * based on layer screen dimensions.
 */
export function getAdaptiveRotateBandPx(width: number, height: number): number {
  const minDim = Math.min(Math.abs(width), Math.abs(height));
  return Math.max(32, Math.min(60, Math.round(minDim * 0.15 + 20)));
}

/** Shared resize handle sizes — used by CropOverlay, ModernCropOverlay, and SelectionTransformOverlay */
export const HANDLE_SIZE = 8;
export const HANDLE_HIT = 28;

function roundedRectPath(x: number, y: number, w: number, h: number, r: number): string {
  const rx = Math.min(r, w / 2);
  const ry = Math.min(r, h / 2);
  return [
    `M ${x + rx} ${y}`,
    `H ${x + w - rx}`,
    `A ${rx} ${ry} 0 0 1 ${x + w} ${y + ry}`,
    `V ${y + h - ry}`,
    `A ${rx} ${ry} 0 0 1 ${x + w - rx} ${y + h}`,
    `H ${x + rx}`,
    `A ${rx} ${ry} 0 0 1 ${x} ${y + h - ry}`,
    `V ${y + ry}`,
    `A ${rx} ${ry} 0 0 1 ${x + rx} ${y}`,
    `Z`,
  ].join(" ");
}

/**
 * Returns SVG path data for the rotate band ring around a crop rect.
 *
 * @param x - crop rect x
 * @param y - crop rect y
 * @param w - crop rect width
 * @param h - crop rect height
 * @param bandWidth - band width on sides (default 20px)
 * @param cornerRadius - outer rect corner radius (default 2px)
 * @returns SVG path data string (use with fill-rule="evenodd")
 */
export function getRotateBandPath(
  x: number,
  y: number,
  w: number,
  h: number,
  bandWidth: number = ROTATE_BAND_PX,
  cornerRadius: number = ROTATE_CORNER_EXTRA,
): string {
  const pad = bandWidth + cornerRadius;
  const ox = x - pad;
  const oy = y - pad;
  const ow = w + pad * 2;
  const oh = h + pad * 2;
  const outer = roundedRectPath(ox, oy, ow, oh, cornerRadius);
  const inner = `M ${x} ${y} H ${x + w} V ${y + h} H ${x} Z`;
  return `${outer} ${inner}`;
}
