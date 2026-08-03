import { getLayerCorners } from "./transformGeometry";
import type { Transform2D } from "../engine/types";

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
  if (alphaAt) {
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
