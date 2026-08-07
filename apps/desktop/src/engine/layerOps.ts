// SPDX-License-Identifier: AGPL-3.0-or-later
// Layer-stack mutation operations for DocumentEngine.
// Each function mutates the DocumentModel in place and returns only what the
// caller (DocumentEngine) needs for side effects (texture cleanup, notify).
// Logic was moved verbatim out of document.ts (report #20 phase 3) — behavior
// must stay identical; regression comments are preserved with the code.

import type { DocumentModel, LayerNode, LayerId, BlendMode, Transform2D, ShapeParams } from "./types";
import { MAX_LAYERS, MAX_PIXEL_BUDGET, getEffectiveMaxDim } from "./types";
import { createLayerNode, duplicateLayerNode, createMergedLayerNode } from "./layerFactory";
import { compositeTwoLayers, compositeAllLayers } from "./layerComposite";
import { renderShapeToBitmap } from "./shapeRaster";

/** Insert a new layer directly above the active layer (or at top when none). */
export function addLayer(model: DocumentModel, name: string, width?: number, height?: number): LayerNode {
  if (model.layers.length >= MAX_LAYERS) {
    throw new Error(`Maximum layer limit of ${MAX_LAYERS} reached`);
  }

  const w = width ?? model.width;
  const h = height ?? model.height;
  if (w > getEffectiveMaxDim() || h > getEffectiveMaxDim()) {
    throw new Error(`Layer dimensions exceed device limit ${getEffectiveMaxDim()}px per side`);
  }

  if (!canAddLayer(model, w, h)) {
    throw new Error("E_RESOURCE_LIMIT: Adding this layer exceeds maximum pixel memory budget.");
  }

  const newLayer = createLayerNode(name, w, h);

  // Insert directly above active layer if selected, else at front (top) of stack
  const activeId = model.activeLayerId;
  const activeIndex = activeId ? model.layers.findIndex(l => l.id === activeId) : -1;
  if (activeIndex !== -1) {
    model.layers = [
      ...model.layers.slice(0, activeIndex),
      newLayer,
      ...model.layers.slice(activeIndex)
    ];
  } else {
    model.layers = [newLayer, ...model.layers];
  }
  model.activeLayerId = newLayer.id;
  model.dirty = true;

  return newLayer;
}

/** Add a parametric shape layer above the active layer (or at top when none). */
export function addShapeLayer(
  model: DocumentModel,
  name: string,
  params: ShapeParams
): LayerNode {
  if (model.layers.length >= MAX_LAYERS) {
    throw new Error(`Maximum layer limit of ${MAX_LAYERS} reached`);
  }
  // Budget check (canAddLayer) deliberately skipped per plan scope — Task 6
  // temp shape layers are tiny; add it if real layers grow.
  const newLayer = createShapeLayerNode(name, params);

  // Insert directly above active layer if selected, else at front (top) of stack
  const activeId = model.activeLayerId;
  const activeIndex = activeId ? model.layers.findIndex(l => l.id === activeId) : -1;
  if (activeIndex !== -1) {
    model.layers = [
      ...model.layers.slice(0, activeIndex),
      newLayer,
      ...model.layers.slice(activeIndex),
    ];
  } else {
    model.layers = [newLayer, ...model.layers];
  }
  model.activeLayerId = newLayer.id;
  model.dirty = true;

  return newLayer;
}

/** Duplicate a layer (with numeric-suffix rename) directly above the source. */
export function duplicateLayer(model: DocumentModel, id: LayerId): LayerNode {
  if (model.layers.length >= MAX_LAYERS) {
    throw new Error(`Maximum layer limit of ${MAX_LAYERS} reached`);
  }

  const layer = model.layers.find(l => l.id === id);
  if (!layer) {
    throw new Error(`Layer with ID ${id} not found`);
  }

  if (!canAddLayer(model, layer.width, layer.height)) {
    throw new Error("E_RESOURCE_LIMIT: Duplicating this layer exceeds maximum pixel memory budget.");
  }

  const duplicated = duplicateLayerNode(layer);

  // Rename with numeric suffix instead of "copy"
  // "Background" → "Background 2", "Background 2" → "Background 3", etc.
  duplicated.name = nextDuplicateName(model, layer.name);

  const index = model.layers.findIndex(l => l.id === id);
  if (index !== -1) {
    const updated = [...model.layers];
    updated.splice(index, 0, duplicated);
    model.layers = updated;
  } else {
    model.layers = [duplicated, ...model.layers];
  }

  model.activeLayerId = duplicated.id;
  model.dirty = true;

  return duplicated;
}

export interface MergeDownResult {
  merged: LayerNode;
  /** Ids of the two source layers — caller must clean up their textures. */
  removedIds: [LayerId, LayerId];
}

/** Merge a layer down into the one below it. Returns null when not mergeable. */
export function mergeDown(model: DocumentModel, id: LayerId): MergeDownResult | null {
  const index = model.layers.findIndex(l => l.id === id);
  if (index === -1 || index >= model.layers.length - 1) {
    return null;
  }

  const top = model.layers[index];
  const bottom = model.layers[index + 1];

  const mergedW = model.width;
  const mergedH = model.height;

  const mergedBitmap = compositeTwoLayers(top, bottom, mergedW, mergedH);

  const mergedLayer = createMergedLayerNode(
    `${top.name} + ${bottom.name}`,
    mergedW,
    mergedH,
    mergedBitmap,
    bottom.locked || top.locked,
    bottom.blendMode
  );

  const updated = [...model.layers];
  updated.splice(index, 2, mergedLayer);
  model.layers = updated;

  model.activeLayerId = mergedLayer.id;
  model.dirty = true;

  return { merged: mergedLayer, removedIds: [top.id, bottom.id] };
}

/** Flatten all layers into a single Background layer. Returns removed ids. */
export function flattenLayers(model: DocumentModel): LayerId[] {
  if (model.layers.length <= 1) return [];

  const mergedW = model.width;
  const mergedH = model.height;

  const mergedBitmap = compositeAllLayers(model.layers, mergedW, mergedH);

  const flattenedLayer = createMergedLayerNode(
    "Background",
    mergedW,
    mergedH,
    mergedBitmap,
    false,
    "normal"
  );
  // The flattened result is the new bottom layer — flag it as the
  // Background (with the position/rotation locks the app's real
  // Background layers carry) so it matches the bg invariant.
  flattenedLayer.isBackground = true;
  flattenedLayer.lockPosition = true;
  flattenedLayer.lockRotation = true;

  const removedIds = model.layers.map(l => l.id);

  model.layers = [flattenedLayer];
  model.activeLayerId = flattenedLayer.id;
  model.dirty = true;

  return removedIds;
}

/**
 * Remove a layer, re-selecting a neighbor when the active layer was removed.
 * Returns the removed layer id (for texture cleanup) or null when nothing was
 * removed. Background is never deletable; the last layer is never deletable.
 */
export function deleteLayer(model: DocumentModel, id: LayerId): LayerId | null {
  const layer = model.layers.find(l => l.id === id);
  if (layer?.isBackground) return null;

  if (model.layers.length <= 1) {
    return null; // prevent deleting the last layer
  }

  const index = model.layers.findIndex(l => l.id === id);
  if (index === -1) return null;

  model.layers = model.layers.filter(l => l.id !== id);
  // NOTE: we intentionally do NOT close any bitmaps from the
  // removed layer here.  Snapshots in the undo/redo stack may
  // hold a reference to them; closing them here would make
  // those snapshots point to closed/detached bitmaps, causing
  // "image source is detached" errors on restore (undo/redo).
  // Memory is reclaimed by GC once no snapshot or layer
  // references remain.
  // deleteLayer is called AFTER history.commit() in
  // every production path, so the undo-stack snapshot already
  // holds a reference to these bitmaps.

  // Select another layer
  if (model.activeLayerId === id) {
    const nextActiveIndex = Math.min(index, model.layers.length - 1);
    model.activeLayerId = model.layers[nextActiveIndex].id;
  }

  model.dirty = true;
  return id;
}

/** Reorder a layer, keeping the Background pinned to the bottom. */
export function reorderLayer(model: DocumentModel, fromIndex: number, toIndex: number): void {
  if (fromIndex < 0 || fromIndex >= model.layers.length ||
      toIndex < 0 || toIndex >= model.layers.length) {
    return;
  }

  const fromLayer = model.layers[fromIndex];
  // The Background layer is locked to the bottom of the stack — it can
  // never be reordered, and no other layer may be placed below it (a
  // layer beneath the opaque Background would be unreachable / hidden).
  if (fromLayer?.isBackground) return;

  const updated = [...model.layers];
  const [moved] = updated.splice(fromIndex, 1);
  updated.splice(toIndex, 0, moved);

  // Invariant: the Background is always the bottommost layer. If the move
  // pushed it off the bottom, re-seat it there so a normal layer can
  // never end up hidden behind it.
  const bgIdx = updated.findIndex((l) => l.isBackground);
  if (bgIdx >= 0 && bgIdx !== updated.length - 1) {
    const [bg] = updated.splice(bgIdx, 1);
    updated.push(bg);
  }

  model.layers = updated;
  model.dirty = true;
}

export function setActiveLayer(model: DocumentModel, id: LayerId | null): void {
  if (id === null || model.layers.some(l => l.id === id)) {
    model.activeLayerId = id;
  }
}

export function setLayerOpacity(model: DocumentModel, id: LayerId, opacity: number): void {
  const layer = model.layers.find(l => l.id === id);
  if (layer && !layer.locked) {
    layer.opacity = Math.max(0.0, Math.min(1.0, opacity));
    model.dirty = true;
  }
}

export function setLayerVisibility(model: DocumentModel, id: LayerId, visible: boolean): void {
  const layer = model.layers.find(l => l.id === id);
  if (layer) {
    layer.visible = visible;
    model.dirty = true;
  }
}

export function setLayerLocked(model: DocumentModel, id: LayerId, locked: boolean): void {
  const layer = model.layers.find(l => l.id === id);
  if (layer) {
    layer.locked = locked;
    model.dirty = true;
  }
}

export function setLayerLockTransparency(model: DocumentModel, id: LayerId, locked: boolean): void {
  const layer = model.layers.find(l => l.id === id);
  if (layer) {
    layer.lockTransparency = locked;
    model.dirty = true;
  }
}

export function setLayerLockPosition(model: DocumentModel, id: LayerId, locked: boolean): void {
  const layer = model.layers.find(l => l.id === id);
  if (layer) {
    layer.lockPosition = locked;
    model.dirty = true;
  }
}

export function setLayerLockRotation(model: DocumentModel, id: LayerId, locked: boolean): void {
  const layer = model.layers.find(l => l.id === id);
  if (layer) {
    layer.lockRotation = locked;
    model.dirty = true;
  }
}

/** Strip trailing number from a layer name to get the base name */
export function baseName(name: string): string {
  const match = name.match(/^(.*?)\s*(\d+)$/);
  return match ? match[1].trimEnd() : name.trimEnd();
}

/** Generate the next numeric-suffixed name for a duplicating layer.
 *  "Background" → "Background 2", "Background 2" → "Background 3", etc. */
export function nextDuplicateName(model: DocumentModel, layerName: string): string {
  const base = baseName(layerName);
  const prefix = `${base} `;
  let maxNum = 1;
  for (const l of model.layers) {
    if (l.name.startsWith(prefix)) {
      const num = parseInt(l.name.slice(prefix.length), 10);
      if (!isNaN(num) && num > maxNum) maxNum = num;
    }
  }
  return `${base} ${maxNum + 1}`;
}

export function setLayerName(model: DocumentModel, id: LayerId, name: string): void {
  const layer = model.layers.find(l => l.id === id);
  if (layer) {
    // Renaming Background → normal layer
    if (layer.isBackground) {
      layer.isBackground = undefined;
      layer.lockPosition = false;
      layer.lockRotation = false;
    }
    layer.name = name;
    model.dirty = true;
  }
}

export function setLayerBlendMode(model: DocumentModel, id: LayerId, mode: BlendMode): void {
  const layer = model.layers.find(l => l.id === id);
  if (layer && !layer.locked) {
    layer.blendMode = mode;
    model.dirty = true;
  }
}

export function moveLayer(model: DocumentModel, id: LayerId, x: number, y: number): void {
  const layer = model.layers.find(l => l.id === id);
  if (layer && !layer.locked && !layer.lockPosition) {
    layer.transform.x = x;
    layer.transform.y = y;
    model.dirty = true;
  }
}

export function transformLayer(model: DocumentModel, id: LayerId, transform: Partial<Transform2D>): void {
  const layer = model.layers.find(l => l.id === id);
  if (layer && !layer.locked) {
    const updatedTransform = { ...layer.transform };

    // Apply positional changes only if position lock is false
    if (!layer.lockPosition) {
      if (transform.x !== undefined) updatedTransform.x = transform.x;
      if (transform.y !== undefined) updatedTransform.y = transform.y;
    }

    // Apply rotational changes only if rotation lock is false
    if (!layer.lockRotation) {
      if (transform.rotation !== undefined) updatedTransform.rotation = transform.rotation;
    }

    // Scale, flips, etc. are always applied (or add more locks if needed in the future)
    if (transform.scaleX !== undefined) updatedTransform.scaleX = transform.scaleX;
    if (transform.scaleY !== undefined) updatedTransform.scaleY = transform.scaleY;
    if (transform.flipH !== undefined) updatedTransform.flipH = transform.flipH;
    if (transform.flipV !== undefined) updatedTransform.flipV = transform.flipV;

    layer.transform = updatedTransform;
    model.dirty = true;
  }
}

export function flipLayer(model: DocumentModel, id: LayerId, axis: "h" | "v"): void {
  const layer = model.layers.find(l => l.id === id);
  if (layer && !layer.locked) {
    if (axis === "h") {
      layer.transform.flipH = !layer.transform.flipH;
    } else {
      layer.transform.flipV = !layer.transform.flipV;
    }
    model.dirty = true;
  }
}

// ─── Memory Budget ───
export function calculateMemoryUsage(model: DocumentModel): number {
  let bytes = 0;
  for (const layer of model.layers) {
    bytes += layer.width * layer.height * 4; // RGBA8
  }
  return bytes;
}

export function canAddLayer(model: DocumentModel, width: number, height: number): boolean {
  const projected = calculateMemoryUsage(model) + (width * height * 4);
  return projected <= MAX_PIXEL_BUDGET;
}

// ─── Shape Layers ───
/** True when the layer is a parametric shape layer. */
export function isShapeLayer(layer: LayerNode): boolean {
  return layer.type === "shape";
}

/** Build a shape layer node; bitmap already rasterized from params.
 *  Layer width/height come from the ACTUAL bitmap dims so they can never
 *  diverge from the rasterized pixels (stroke margin is baked by the
 *  rasterizer). */
export function createShapeLayerNode(
  name: string,
  params: ShapeParams
): LayerNode {
  const bitmap = renderShapeToBitmap(params);
  const layer = createLayerNode(name, 1, 1);
  layer.type = "shape";
  layer.shapeParams = params;
  layer.width = bitmap.width;
  layer.height = bitmap.height;
  layer.imageBitmap = bitmap;
  return layer;
}

/** Convert a shape layer to a plain raster layer (drops params, keeps bitmap). */
export function shapeLayerToRaster(layer: LayerNode): void {
  if (layer.type !== "shape") return;
  layer.type = "raster";
  delete layer.shapeParams;
}
