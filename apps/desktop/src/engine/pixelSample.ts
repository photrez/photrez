import type { LayerNode } from "./types";

// Reusable 1x1 scratch canvas — avoids allocating a new OffscreenCanvas
// per layer per sample during Alt+drag eyedropper (was ~50-200µs each).
// Lazily created on first use so module import does not fail in envs
// without OffscreenCanvas (e.g. jsdom test runner).
let sampleCanvas: OffscreenCanvas | null = null;
let sampleCtx: OffscreenCanvasRenderingContext2D | null = null;

function getSampleCtx(): OffscreenCanvasRenderingContext2D | null {
  if (sampleCtx) return sampleCtx;
  sampleCanvas = new OffscreenCanvas(1, 1);
  sampleCtx = sampleCanvas.getContext("2d");
  return sampleCtx;
}

export function performPixelSampling(
  layers: readonly LayerNode[],
  docWidth: number,
  docHeight: number,
  x: number,
  y: number
): [number, number, number, number] {
  // If coordinates are out of bounds, return fully transparent
  if (x < 0 || x >= docWidth || y < 0 || y >= docHeight) {
    return [0, 0, 0, 0];
  }

  // Dynamic color sampling from layers bottom-to-top (we compose them simple Normal blending for eyedropper)
  let composed: [number, number, number, number] = [0, 0, 0, 0];

  // Iterating backwards from bottom (index length-1) to top (index 0)
  for (let i = layers.length - 1; i >= 0; i--) {
    const layer = layers[i];
    if (!layer.visible || !layer.imageBitmap) continue;

    // Map document coordinates to layer-relative coordinates, then to BITMAP
    // pixel space. Parametric layers (text) rasterize at RASTER_SCALE (2x), so
    // the bitmap is larger than layer.width/height — sampling at raw doc
    // coords read the wrong pixels (eyedropper picked wrong colors on text;
    // @bug WYSIWYG 2026-08-09).
    const bmp = layer.imageBitmap;
    // Fake/decoy bitmaps in tests may lack real dims — fall back to 1x.
    const bmpW = typeof bmp.width === "number" ? bmp.width : layer.width;
    const bmpH = typeof bmp.height === "number" ? bmp.height : layer.height;
    const bmpScaleX = layer.width > 0 && bmpW > 0 ? bmpW / layer.width : 1;
    const bmpScaleY = layer.height > 0 && bmpH > 0 ? bmpH / layer.height : 1;
    // layer.width is the UNSCALED doc box; the visible extent is width*|scaleX|,
    // so divide by the layer scale (axis-aligned only — rotation still assumes
    // an unrotated frame, matching the pre-existing behavior).
    const absSx = Math.abs(layer.transform.scaleX ?? 1) || 1;
    const absSy = Math.abs(layer.transform.scaleY ?? 1) || 1;
    const rx = Math.floor(((x - layer.transform.x) / absSx) * bmpScaleX);
    const ry = Math.floor(((y - layer.transform.y) / absSy) * bmpScaleY);

     if (rx >= 0 && rx < bmpW && ry >= 0 && ry < bmpH) {
       try {
        const ctx = getSampleCtx();
        if (ctx) {
          ctx.clearRect(0, 0, 1, 1);
          // Downsample the bitmap-space source rect (2x2 for text, 1x1 for
          // raster) into the 1x1 scratch so the sample averages the
          // supersampled pixels.
          const srcW = Math.max(1, Math.ceil(bmpScaleX));
          const srcH = Math.max(1, Math.ceil(bmpScaleY));
          const srcX = Math.max(0, Math.min(rx, bmpW - srcW));
          const srcY = Math.max(0, Math.min(ry, bmpH - srcH));
          ctx.drawImage(bmp, srcX, srcY, srcW, srcH, 0, 0, 1, 1);
          const imgData = ctx.getImageData(0, 0, 1, 1);
          const r = imgData.data[0];
          const g = imgData.data[1];
          const b = imgData.data[2];
          const a = (imgData.data[3] / 255) * layer.opacity;

          // Simple alpha blend composed and current layer
          const [cr, cg, cb, ca] = composed;
          const outA = a + ca * (1.0 - a);
          if (outA > 0) {
            const outR = Math.round((r * a + cr * ca * (1.0 - a)) / outA);
            const outG = Math.round((g * a + cg * ca * (1.0 - a)) / outA);
            const outB = Math.round((b * a + cb * ca * (1.0 - a)) / outA);
            composed = [outR, outG, outB, outA];
          }
        }
      } catch {
        // Fallback if canvas read fails
        composed = [225, 90, 23, 1.0]; // Photon Amber fallback
      }
    }
  }

  return composed;
}

/**
 * Alpha of a single layer at a document-space point, accounting for the
 * layer's transform and opacity. Returns 0 when the point is outside the
 * layer bitmap or the layer is invisible / has no bitmap.
 *
 * Used for alpha-aware layer hit-testing so clicks on a layer's transparent
 * corners fall through to the layer underneath instead of selecting the
 * topmost bounding box.
 */
export function sampleSingleLayerAlpha(
  layers: readonly LayerNode[],
  x: number,
  y: number,
  layerId: string,
): number {
  const layer = layers.find((l) => l.id === layerId);
  if (!layer || !layer.visible || !layer.imageBitmap) return 0;

  // Scale-aware mapping (same rationale as performPixelSampling): text
  // bitmaps are 2x the doc box, so sample in bitmap pixel space. Divided by
  // the layer scale because layer.width is the UNSCALED doc box (rotation is
  // still approximated as an axis-aligned frame, pre-existing limitation).
  const bmp = layer.imageBitmap;
  const bmpW = typeof bmp.width === "number" ? bmp.width : layer.width;
  const bmpH = typeof bmp.height === "number" ? bmp.height : layer.height;
  const bmpScaleX = layer.width > 0 && bmpW > 0 ? bmpW / layer.width : 1;
  const bmpScaleY = layer.height > 0 && bmpH > 0 ? bmpH / layer.height : 1;
  const absSx = Math.abs(layer.transform.scaleX ?? 1) || 1;
  const absSy = Math.abs(layer.transform.scaleY ?? 1) || 1;
  const rx = Math.floor(((x - layer.transform.x) / absSx) * bmpScaleX);
  const ry = Math.floor(((y - layer.transform.y) / absSy) * bmpScaleY);
  if (rx < 0 || rx >= bmpW || ry < 0 || ry >= bmpH) return 0;

  try {
    const ctx = getSampleCtx();
    if (!ctx) return 1; // jsdom: assume opaque so hit-test still selects
    ctx.clearRect(0, 0, 1, 1);
    const srcW = Math.max(1, Math.ceil(bmpScaleX));
    const srcH = Math.max(1, Math.ceil(bmpScaleY));
    const srcX = Math.max(0, Math.min(rx, bmpW - srcW));
    const srcY = Math.max(0, Math.min(ry, bmpH - srcH));
    ctx.drawImage(bmp, srcX, srcY, srcW, srcH, 0, 0, 1, 1);
    const imgData = ctx.getImageData(0, 0, 1, 1);
    return (imgData.data[3] / 255) * layer.opacity;
  } catch {
    return 1;
  }
}
