import { getLayerCorners } from "./transformGeometry";
import type { LayerNode, Transform2D } from "../engine/types";

export interface LayerHit {
  id: string;
}

function pointInPolygon(px: number, py: number, corners: { x: number; y: number }[]): boolean {
  let inside = false;
  const n = corners.length;
  for (let i = 0, j = n - 1; i < n; j = i++) {
    const xi = corners[i].x, yi = corners[i].y;
    const xj = corners[j].x, yj = corners[j].y;
    if ((yi > py) !== (yj > py) && px < ((xj - xi) * (py - yi)) / (yj - yi) + xi) {
      inside = !inside;
    }
  }
  return inside;
}

export interface LayerInfo {
  id: string;
  transform: Transform2D;
  width: number;
  height: number;
  visible: boolean;
  locked: boolean;
  /**
   * LayerNode.type — REQUIRED so a call site can never silently drop the
   * box-hittable wiring: omitting it would quietly fall back to raster-style
   * alpha fall-through for text/shape layers (isBoxHittable reads this field
   * to treat parametric layers as whole-box targets).
   */
  type: LayerNode["type"];
}

/**
 * Parametric layers (text, shape) are OBJECTS, not photographs: their raster
 * boxes contain large transparent areas (text line-height padding, hollow
 * shape interiors) that users still expect to be "the layer". Alpha-aware
 * fall-through stays for raster layers (transparent corners of photos select
 * what's underneath) but must NOT fall through for text/shape boxes —
 * otherwise clicking the padding of a text layer selects the Background layer
 * beneath it (@bug 2026-08-09: "clicking text/shape selects Background").
 */
export function isBoxHittable(layer: Pick<LayerInfo, "type">): boolean {
  return layer.type === "text" || layer.type === "shape";
}

/**
 * Alpha sampler used for alpha-aware hit-testing. Given a layer id and a
 * document-space point, returns the layer's alpha there (0..1), or `null`
 * when not provided / not applicable. When it returns 0 the point is treated
 * as a miss even if it falls inside the layer's bounding box, so clicks on
 * transparent corners fall through to the layer underneath.
 */
export type AlphaSampler = (layerId: string, x: number, y: number) => number | null;

export const ALPHA_HIT_THRESHOLD = 0.1;

export function hitTestLayer(
  point: { x: number; y: number },
  layer: LayerInfo,
  alphaAt?: AlphaSampler
): boolean {
  if (!layer.visible) return false;
  const corners = getLayerCorners(layer.transform, layer.width, layer.height);
  if (!pointInPolygon(point.x, point.y, corners)) return false;
  // Parametric layers are selected by their whole box — no alpha fall-through
  // (see isBoxHittable). Raster layers keep the alpha-aware behavior so
  // transparent corners/clipped photos still reveal the layer underneath.
  if (alphaAt && !isBoxHittable(layer)) {
    const a = alphaAt(layer.id, point.x, point.y);
    if (a !== null && a < ALPHA_HIT_THRESHOLD) return false;
  }
  return true;
}

export function hitTestLayers(
  point: { x: number; y: number },
  layers: LayerInfo[],
  alphaAt?: AlphaSampler
): LayerHit | null {
  for (const layer of layers) {
    if (!layer.visible) continue;
    if (hitTestLayer(point, layer, alphaAt)) {
      return { id: layer.id };
    }
  }
  return null;
}
