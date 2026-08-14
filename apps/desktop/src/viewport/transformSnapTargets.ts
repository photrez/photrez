// SPDX-License-Identifier: AGPL-3.0-or-later
// Snap target construction for move-tool and transform interactions.

import type { SnapRect } from "./smartGuides";
import { getLayerAabb } from "./transformGeometry";
import type { DocumentEngine } from "@/engine/document";

export interface TransformSnapOptions {
  snapToLayers?: boolean;
  snapToCanvas?: boolean;
  excludeLayerId?: string;
}

/**
 * Builds the snap target list for layer transformations:
 * document bounds & center axes (tagged as canvas), and every visible
 * layer's AABB (tagged as layer) except the layer being transformed.
 */
export function buildTransformSnapTargets(
  engine: DocumentEngine,
  docW: number,
  docH: number,
  opts: TransformSnapOptions = {},
): SnapRect[] {
  const movingId = opts.excludeLayerId ?? engine.getActiveLayerId();
  const snapToLayers = opts.snapToLayers ?? true;
  const snapToCanvas = opts.snapToCanvas ?? true;

  const targets: SnapRect[] = [];

  if (snapToCanvas) {
    targets.push(
      {
        x: 0,
        y: 0,
        w: docW,
        h: docH,
        snapThreshold: 10,
        snapPriority: 3,
        kind: "canvas",
      },
      {
        x: docW / 2,
        y: -Infinity,
        w: 0,
        h: Infinity,
        snapThreshold: 6,
        snapPriority: 2,
        kind: "canvas",
      },
      {
        x: -Infinity,
        y: docH / 2,
        w: Infinity,
        h: 0,
        snapThreshold: 6,
        snapPriority: 2,
        kind: "canvas",
      },
    );
  }

  if (snapToLayers) {
    const layerTargets: SnapRect[] = engine
      .getLayers()
      .filter((l) => l.visible && l.id !== movingId && l.name !== "Background")
      .map((l) => {
        const aabb = getLayerAabb(l.transform, l.width, l.height);
        return {
          x: aabb.x,
          y: aabb.y,
          w: aabb.width,
          h: aabb.height,
          snapThreshold: 8,
          snapPriority: 1,
          kind: "layer",
        };
      });
    targets.push(...layerTargets);
  }

  return targets;
}
