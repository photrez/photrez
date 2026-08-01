// SPDX-License-Identifier: AGPL-3.0-or-later
// Viewport state mutations for DocumentEngine.
// Logic moved verbatim out of document.ts (report #20 phase 3) — behavior
// must stay identical.

import type { DocumentModel, ViewportState } from "./types";

export function setViewport(model: DocumentModel, viewport: Partial<ViewportState>): void {
  model.viewport = {
    ...model.viewport,
    ...viewport
  };
}

export function pan(model: DocumentModel, dx: number, dy: number): void {
  model.viewport.panX += dx;
  model.viewport.panY += dy;
}

export function zoom(model: DocumentModel, factor: number, anchorX?: number, anchorY?: number): void {
  const currentZoom = model.viewport.zoom;
  const nextZoom = Math.max(0.01, Math.min(100.0, currentZoom * factor));

  if (anchorX !== undefined && anchorY !== undefined) {
    // Zoom centered at anchor point
    model.viewport.panX = anchorX - (anchorX - model.viewport.panX) * (nextZoom / currentZoom);
    model.viewport.panY = anchorY - (anchorY - model.viewport.panY) * (nextZoom / currentZoom);
  }

  model.viewport.zoom = nextZoom;
}

export function fitToScreen(model: DocumentModel, containerWidth: number, containerHeight: number): void {
  const padding = 80;
  const fitZoom = Math.min(
    (containerWidth - padding) / model.width,
    (containerHeight - padding) / model.height,
    10.0
  );

  model.viewport.zoom = Math.max(0.05, fitZoom);
  model.viewport.panX = (containerWidth - model.width * model.viewport.zoom) / 2;
  model.viewport.panY = (containerHeight - model.height * model.viewport.zoom) / 2;
}

/**
 * Fit the active selection bounds into the viewport (centered), like a
 * zoom-to-selection command in similar editors. Clamps the rect to the
 * document and falls back to the whole-document fit when no selection
 * exists (so it is always safe to call).
 */
export function zoomToSelection(model: DocumentModel, containerWidth: number, containerHeight: number): void {
  const sel = model.selection;
  if (sel) {
    const left = Math.max(0, Math.min(model.width, sel.x));
    const top = Math.max(0, Math.min(model.height, sel.y));
    const right = Math.max(0, Math.min(model.width, sel.x + sel.width));
    const bottom = Math.max(0, Math.min(model.height, sel.y + sel.height));
    const w = Math.max(1, right - left);
    const h = Math.max(1, bottom - top);
    const padding = 80;
    const zoom = Math.min(
      (containerWidth - padding) / w,
      (containerHeight - padding) / h,
      10.0
    );
    model.viewport.zoom = Math.max(0.05, zoom);
    model.viewport.panX = (containerWidth - w * model.viewport.zoom) / 2 - left * model.viewport.zoom;
    model.viewport.panY = (containerHeight - h * model.viewport.zoom) / 2 - top * model.viewport.zoom;
    return;
  }
  fitToScreen(model, containerWidth, containerHeight);
}
