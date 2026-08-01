// SPDX-License-Identifier: AGPL-3.0-or-later
// Selection state mutations for DocumentEngine.
// Logic moved verbatim out of document.ts (report #20 phase 3) — behavior
// must stay identical.

import type { DocumentModel } from "./types";

export function createSelection(
  model: DocumentModel,
  x: number,
  y: number,
  w: number,
  h: number,
  angle?: number,
  shape?: "rect" | "ellipse",
): void {
  // Only store `shape` when non-default (ellipse) so the selection object
  // stays backward-compatible (rect selections have no `shape` key).
  model.selection = shape === "ellipse"
    ? { x, y, width: w, height: h, angle: angle ?? 0, shape: "ellipse" }
    : { x, y, width: w, height: h, angle: angle ?? 0 };
}

export function clearSelection(model: DocumentModel): void {
  model.selection = null;
}

export function selectAll(model: DocumentModel): void {
  model.selection = {
    x: 0,
    y: 0,
    width: model.width,
    height: model.height,
    angle: 0,
  };
}

export function invertSelection(model: DocumentModel): void {
  if (model.selection) {
    model.selection = {
      ...model.selection,
      inverted: !model.selection.inverted,
    };
  } else {
    selectAll(model);
    return;
  }
}
