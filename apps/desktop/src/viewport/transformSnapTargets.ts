// SPDX-License-Identifier: AGPL-3.0-or-later
// Snap target construction for move-tool transforms.
// Extracted from CanvasViewport.tsx (report #20 phase 3) — behavior must stay
// identical to the inline builder it replaces.

import type { SnapRect } from "./smartGuides";
import { getLayerAabb } from "./transformGeometry";
import type { DocumentEngine } from "@/engine/document";

/**
 * Builds the snap target list for a move-tool drag:
 * document bounds (strong priority), doc center axes, and every visible
 * layer's AABB except the layer being moved.
 */
export function buildTransformSnapTargets(
  engine: DocumentEngine,
  docW: number,
  docH: number,
): SnapRect[] {
  const movingId = engine.getActiveLayerId();
  const layerTargets: SnapRect[] = engine
    .getLayers()
    .filter((l) => l.visible && l.id !== movingId)
    .map((l) => {
      const aabb = getLayerAabb(l.transform, l.width, l.height);
      return {
        x: aabb.x,
        y: aabb.y,
        w: aabb.width,
        h: aabb.height,
      };
    });
  return [
    {
      x: 0,
      y: 0,
      w: docW,
      h: docH,
      snapThreshold: 12,
      snapPriority: 3,
    },
    {
      x: docW / 2,
      y: -Infinity,
      w: 0,
      h: Infinity,
      snapThreshold: 6,
      snapPriority: 2,
    },
    {
      x: -Infinity,
      y: docH / 2,
      w: Infinity,
      h: 0,
      snapThreshold: 6,
      snapPriority: 2,
    },
    ...layerTargets,
  ];
}
