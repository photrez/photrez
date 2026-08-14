// SPDX-License-Identifier: AGPL-3.0-or-later
// Smart guides and snapping calculations for layer transformations.

export interface SnapRect {
  x: number;
  y: number;
  w: number;
  h: number;
  snapThreshold?: number;
  snapPriority?: number;
  kind?: "layer" | "canvas" | "gap";
}

export interface SnapLine {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  color?: string;
  kind?: "layer" | "canvas" | "gap";
  isDashed?: boolean;
  label?: string;
}

export interface SnapResult {
  dx: number;
  dy: number;
  lines: SnapLine[];
}

const X_KEYS = ["left", "right", "cx"] as const;
const Y_KEYS = ["top", "bottom", "cy"] as const;

function buildAxis(rect: SnapRect) {
  return {
    left: rect.x,
    right: rect.x + rect.w,
    cx: rect.x + rect.w / 2,
    top: rect.y,
    bottom: rect.y + rect.h,
    cy: rect.y + rect.h / 2,
  };
}

export function computeSnapAdjustment(
  moving: SnapRect,
  targets: SnapRect[],
  threshold = 8,
  zoom = 1,
): SnapResult {
  const me = buildAxis(moving);
  // scale thresholds by 1/zoom so screen-space catch zone is constant
  const factor = zoom !== 0 ? 1 / zoom : 1;
  let bestDx = 0;
  let bestDxDist = Infinity;
  let bestDxLineY1 = moving.y;
  let bestDxLineY2 = moving.y + moving.h;
  let bestDxHitX: number | null = null;
  let bestDxPriority = -1;
  let bestDxKind: "layer" | "canvas" | "gap" = "layer";
  let bestDxIsDashed = false;
  let bestDxCustomLines: SnapLine[] | null = null;

  let bestDy = 0;
  let bestDyDist = Infinity;
  let bestDyLineX1 = moving.x;
  let bestDyLineX2 = moving.x + moving.w;
  let bestDyHitY: number | null = null;
  let bestDyPriority = -1;
  let bestDyKind: "layer" | "canvas" | "gap" = "layer";
  let bestDyIsDashed = false;
  let bestDyCustomLines: SnapLine[] | null = null;

  // 1. Direct Edge & Center Snapping
  for (const t of targets) {
    const te = buildAxis(t);
    const isCanvas = t.kind === "canvas" || (t.snapPriority !== undefined && t.snapPriority >= 2 && !Number.isFinite(t.w));
    const targetKind: "layer" | "canvas" | "gap" = t.kind ?? (isCanvas ? "canvas" : "layer");

    for (const mk of X_KEYS) {
      for (const tk of X_KEYS) {
        const tThreshold = (t.snapThreshold ?? threshold) * factor;
        const tPriority = t.snapPriority ?? 1;
        const d = te[tk] - me[mk];
        const dist = Math.abs(d);
        const isCloser = dist < bestDxDist - 0.001;
        const isTieHigherPriority = Math.abs(dist - bestDxDist) <= 0.001 && tPriority > bestDxPriority;

        if (dist < tThreshold && (isCloser || isTieHigherPriority)) {
          bestDxDist = dist;
          bestDxPriority = tPriority;
          bestDx = d;
          bestDxHitX = te[tk];
          bestDxKind = targetKind;
          bestDxIsDashed = mk !== "cx" && tk !== "cx";
          bestDxCustomLines = null;
          const rawY1 = Math.min(moving.y, t.y) - 10;
          const rawY2 = Math.max(moving.y + moving.h, t.y + t.h) + 10;
          bestDxLineY1 = Number.isFinite(rawY1) ? rawY1 : moving.y - 10000;
          bestDxLineY2 = Number.isFinite(rawY2) ? rawY2 : moving.y + moving.h + 10000;
        }
      }
    }

    for (const mk of Y_KEYS) {
      for (const tk of Y_KEYS) {
        const tThreshold = (t.snapThreshold ?? threshold) * factor;
        const tPriority = t.snapPriority ?? 1;
        const d = te[tk] - me[mk];
        const dist = Math.abs(d);
        const isCloser = dist < bestDyDist - 0.001;
        const isTieHigherPriority = Math.abs(dist - bestDyDist) <= 0.001 && tPriority > bestDyPriority;

        if (dist < tThreshold && (isCloser || isTieHigherPriority)) {
          bestDyDist = dist;
          bestDyPriority = tPriority;
          bestDy = d;
          bestDyHitY = te[tk];
          bestDyKind = targetKind;
          bestDyIsDashed = mk !== "cy" && tk !== "cy";
          bestDyCustomLines = null;
          const rawX1 = Math.min(moving.x, t.x) - 10;
          const rawX2 = Math.max(moving.x + moving.w, t.x + t.w) + 10;
          bestDyLineX1 = Number.isFinite(rawX1) ? rawX1 : moving.x - 10000;
          bestDyLineX2 = Number.isFinite(rawX2) ? rawX2 : moving.x + moving.w + 10000;
        }
      }
    }
  }

  // 2. Equal Spacing (Gap Snapping) between Layer Pairs
  const staticLayers = targets.filter(
    (t) => t.kind !== "canvas" && Number.isFinite(t.x) && Number.isFinite(t.y) && Number.isFinite(t.w) && Number.isFinite(t.h) && t.w > 0 && t.h > 0
  );

  if (staticLayers.length >= 2) {
    const gapThreshold = threshold * factor;
    const gapPriority = 1;

    // Horizontal Gap Analysis (Sorted by X to ensure direct neighbor adjacency)
    const sortedByX = [...staticLayers].sort((a, b) => a.x - b.x);
    for (let i = 0; i < sortedByX.length - 1; i++) {
      const a = sortedByX[i];
      // Find the closest neighbor B to the right of A with vertical overlap
      for (let j = i + 1; j < sortedByX.length; j++) {
        const b = sortedByX[j];
        if (a.x + a.w > b.x) continue; // overlapping or left
        const yOverlap = Math.max(a.y, b.y) < Math.min(a.y + a.h, b.y + b.h) + 40;
        if (!yOverlap) continue;

        // Verify there is no intervening layer between A and B
        const hasIntervening = sortedByX.some(
          (k, kIdx) => kIdx !== i && kIdx !== j && k.x >= a.x + a.w && k.x + k.w <= b.x && Math.max(a.y, k.y) < Math.min(a.y + a.h, k.y + k.h) + 40
        );
        if (hasIntervening) continue;

        const gap = b.x - (a.x + a.w);
        if (gap >= 2) {
          const ym1 = a.y + a.h / 2;
          const ym2 = b.y + b.h / 2;
          const ymMid = (ym1 + ym2) / 2;
          const label = `${Math.round(gap)}px`;

          // Placement 1: Moving is right of B (A -> B -> M)
          const idealRightX = b.x + b.w + gap;
          const dRight = idealRightX - moving.x;
          const distRight = Math.abs(dRight);
          const isRightCloser = distRight < bestDxDist - 0.001;
          const isRightTie = Math.abs(distRight - bestDxDist) <= 0.001 && gapPriority > bestDxPriority;

          if (distRight < gapThreshold && (isRightCloser || isRightTie)) {
            bestDxDist = distRight;
            bestDxPriority = gapPriority;
            bestDx = dRight;
            bestDxHitX = null;
            bestDxKind = "gap";
            bestDxCustomLines = [
              { x1: a.x + a.w, y1: ymMid, x2: b.x, y2: ymMid, kind: "gap", color: "var(--guide-gap, #F59E0B)", label },
              { x1: b.x + b.w, y1: ymMid, x2: b.x + b.w + gap, y2: ymMid, kind: "gap", color: "var(--guide-gap, #F59E0B)", label },
            ];
          }

          // Placement 2: Moving is left of A (M -> A -> B)
          const idealLeftX = a.x - moving.w - gap;
          const dLeft = idealLeftX - moving.x;
          const distLeft = Math.abs(dLeft);
          const isLeftCloser = distLeft < bestDxDist - 0.001;
          const isLeftTie = Math.abs(distLeft - bestDxDist) <= 0.001 && gapPriority > bestDxPriority;

          if (distLeft < gapThreshold && (isLeftCloser || isLeftTie)) {
            bestDxDist = distLeft;
            bestDxPriority = gapPriority;
            bestDx = dLeft;
            bestDxHitX = null;
            bestDxKind = "gap";
            bestDxCustomLines = [
              { x1: a.x - gap, y1: ymMid, x2: a.x, y2: ymMid, kind: "gap", color: "var(--guide-gap, #F59E0B)", label },
              { x1: a.x + a.w, y1: ymMid, x2: b.x, y2: ymMid, kind: "gap", color: "var(--guide-gap, #F59E0B)", label },
            ];
          }

          // Placement 3: Moving is centered between A and B (A -> M -> B)
          if (gap > moving.w + 4) {
            const midGap = (gap - moving.w) / 2;
            const idealMidX = a.x + a.w + midGap;
            const dMid = idealMidX - moving.x;
            const distMid = Math.abs(dMid);
            const isMidCloser = distMid < bestDxDist - 0.001;
            const isMidTie = Math.abs(distMid - bestDxDist) <= 0.001 && gapPriority > bestDxPriority;

            if (distMid < gapThreshold && (isMidCloser || isMidTie)) {
              bestDxDist = distMid;
              bestDxPriority = gapPriority;
              bestDx = dMid;
              bestDxHitX = null;
              bestDxKind = "gap";
              const midLabel = `${Math.round(midGap)}px`;
              bestDxCustomLines = [
                { x1: a.x + a.w, y1: ymMid, x2: a.x + a.w + midGap, y2: ymMid, kind: "gap", color: "var(--guide-gap, #F59E0B)", label: midLabel },
                { x1: a.x + a.w + midGap + moving.w, y1: ymMid, x2: b.x, y2: ymMid, kind: "gap", color: "var(--guide-gap, #F59E0B)", label: midLabel },
              ];
            }
          }
        }
        break; // Closest right neighbor evaluated
      }
    }

    // Vertical Gap Analysis (Sorted by Y to ensure direct neighbor adjacency)
    const sortedByY = [...staticLayers].sort((a, b) => a.y - b.y);
    for (let i = 0; i < sortedByY.length - 1; i++) {
      const a = sortedByY[i];
      for (let j = i + 1; j < sortedByY.length; j++) {
        const b = sortedByY[j];
        if (a.y + a.h > b.y) continue;
        const xOverlap = Math.max(a.x, b.x) < Math.min(a.x + a.w, b.x + b.w) + 40;
        if (!xOverlap) continue;

        const hasIntervening = sortedByY.some(
          (k, kIdx) => kIdx !== i && kIdx !== j && k.y >= a.y + a.h && k.y + k.h <= b.y && Math.max(a.x, k.x) < Math.min(a.x + a.w, k.x + k.w) + 40
        );
        if (hasIntervening) continue;

        const gap = b.y - (a.y + a.h);
        if (gap >= 2) {
          const xm1 = a.x + a.w / 2;
          const xm2 = b.x + b.w / 2;
          const xmMid = (xm1 + xm2) / 2;
          const label = `${Math.round(gap)}px`;

          // Placement 1: Moving is below B (A -> B -> M)
          const idealBottomY = b.y + b.h + gap;
          const dBottom = idealBottomY - moving.y;
          const distBottom = Math.abs(dBottom);
          const isBottomCloser = distBottom < bestDyDist - 0.001;
          const isBottomTie = Math.abs(distBottom - bestDyDist) <= 0.001 && gapPriority > bestDyPriority;

          if (distBottom < gapThreshold && (isBottomCloser || isBottomTie)) {
            bestDyDist = distBottom;
            bestDyPriority = gapPriority;
            bestDy = dBottom;
            bestDyHitY = null;
            bestDyKind = "gap";
            bestDyCustomLines = [
              { x1: xmMid, y1: a.y + a.h, x2: xmMid, y2: b.y, kind: "gap", color: "var(--guide-gap, #F59E0B)", label },
              { x1: xmMid, y1: b.y + b.h, x2: xmMid, y2: b.y + b.h + gap, kind: "gap", color: "var(--guide-gap, #F59E0B)", label },
            ];
          }

          // Placement 2: Moving is above A (M -> A -> B)
          const idealTopY = a.y - moving.h - gap;
          const dTop = idealTopY - moving.y;
          const distTop = Math.abs(dTop);
          const isTopCloser = distTop < bestDyDist - 0.001;
          const isTopTie = Math.abs(distTop - bestDyDist) <= 0.001 && gapPriority > bestDyPriority;

          if (distTop < gapThreshold && (isTopCloser || isTopTie)) {
            bestDyDist = distTop;
            bestDyPriority = gapPriority;
            bestDy = dTop;
            bestDyHitY = null;
            bestDyKind = "gap";
            bestDyCustomLines = [
              { x1: xmMid, y1: a.y - gap, x2: xmMid, y2: a.y, kind: "gap", color: "var(--guide-gap, #F59E0B)", label },
              { x1: xmMid, y1: a.y + a.h, x2: xmMid, y2: b.y, kind: "gap", color: "var(--guide-gap, #F59E0B)", label },
            ];
          }

          // Placement 3: Moving is centered between A and B (A -> M -> B)
          if (gap > moving.h + 4) {
            const midGap = (gap - moving.h) / 2;
            const idealMidY = a.y + a.h + midGap;
            const dMid = idealMidY - moving.y;
            const distMid = Math.abs(dMid);
            const isMidCloser = distMid < bestDyDist - 0.001;
            const isMidTie = Math.abs(distMid - bestDyDist) <= 0.001 && gapPriority > bestDyPriority;

            if (distMid < gapThreshold && (isMidCloser || isMidTie)) {
              bestDyDist = distMid;
              bestDyPriority = gapPriority;
              bestDy = dMid;
              bestDyHitY = null;
              bestDyKind = "gap";
              const midLabel = `${Math.round(midGap)}px`;
              bestDyCustomLines = [
                { x1: xmMid, y1: a.y + a.h, x2: xmMid, y2: a.y + a.h + midGap, kind: "gap", color: "var(--guide-gap, #F59E0B)", label: midLabel },
                { x1: xmMid, y1: a.y + a.h + midGap + moving.h, x2: xmMid, y2: b.y, kind: "gap", color: "var(--guide-gap, #F59E0B)", label: midLabel },
              ];
            }
          }
        }
        break;
      }
    }
  }

  const lines: SnapLine[] = [];

  if (bestDxCustomLines) {
    lines.push(...bestDxCustomLines);
  } else if (bestDxHitX !== null) {
    lines.push({
      x1: bestDxHitX,
      y1: bestDxLineY1,
      x2: bestDxHitX,
      y2: bestDxLineY2,
      kind: bestDxKind,
      color: bestDxKind === "layer" ? "var(--guide-layer, #E03183)" : "var(--guide-canvas, #00C3FF)",
      isDashed: bestDxIsDashed,
    });
  }

  if (bestDyCustomLines) {
    lines.push(...bestDyCustomLines);
  } else if (bestDyHitY !== null) {
    lines.push({
      x1: bestDyLineX1,
      y1: bestDyHitY,
      x2: bestDyLineX2,
      y2: bestDyHitY,
      kind: bestDyKind,
      color: bestDyKind === "layer" ? "var(--guide-layer, #E03183)" : "var(--guide-canvas, #00C3FF)",
      isDashed: bestDyIsDashed,
    });
  }

  return { dx: bestDx, dy: bestDy, lines };
}

export function computeSnapLines(
  moving: SnapRect,
  targets: SnapRect[],
  threshold = 8,
  zoom = 1,
): SnapLine[] {
  return computeSnapAdjustment(moving, targets, threshold, zoom).lines;
}
