import type {
  DocumentId, LayerId, DocumentModel, LayerNode,
  ViewportState, SelectionState, RenderState, BlendMode,
  Transform2D, TextureHandle, RenderLayer
} from "./types";
import { MAX_LAYERS, MAX_PIXEL_BUDGET, getEffectiveMaxDim } from "./types";

import { createLayerNode, duplicateLayerNode, createMergedLayerNode } from "./layerFactory";
import { drawLayerToContext, compositeTwoLayers, compositeAllLayers } from "./layerComposite";
import { performCropCanvas, performApplyCrop } from "./cropApply";
import { createSnapshot, restoreSnapshot } from "./snapshot";
import { performPixelSampling, sampleSingleLayerAlpha } from "./pixelSample";
import { normalizeBasicAdjustment, bakeAdjustmentToBitmap, type BasicAdjustment } from "./layerAdjustments";
import type { RenderBackend } from "../renderer/types";

export { drawLayerToContext };


export class DocumentEngine {
  private model: DocumentModel;
  private textureHandles: Map<LayerId, TextureHandle>;
  private dirtyLayerIds: Set<LayerId>;
  // Saved baseline for dirty detection. isDirty() must compare against the
  // last *saved* state, not a flag carried inside the model (which undo/restore
  // would revive and falsely report clean).
  private savedModel: DocumentModel | null = null;
  private onChangeCallback: (() => void) | null = null;
  private onVisualChangeCallback: (() => void) | null = null;

  constructor(id: DocumentId, name: string, width: number, height: number) {
    this.model = {
      id,
      name,
      width,
      height,
      layers: [],
      activeLayerId: null,
      selection: null,
      viewport: {
        panX: 0,
        panY: 0,
        zoom: 1.0,
        rotation: 0
      },
      dirty: false
    };
    this.textureHandles = new Map();
    this.dirtyLayerIds = new Set();
  }

  // ─── Accessors ───
  getModel(): Readonly<DocumentModel> {
    return this.model;
  }

  getId(): DocumentId {
    return this.model.id;
  }

  getName(): string {
    return this.model.name;
  }

  getWidth(): number {
    return this.model.width;
  }

  getHeight(): number {
    return this.model.height;
  }

  getLayers(): readonly LayerNode[] {
    return this.model.layers;
  }

  getActiveLayerId(): LayerId | null {
    return this.model.activeLayerId;
  }

  getLayer(id: LayerId): LayerNode | undefined {
    return this.model.layers.find(l => l.id === id);
  }

  getSelection(): SelectionState | null {
    return this.model.selection;
  }

  getViewport(): ViewportState {
    return this.model.viewport;
  }

  isDirty(): boolean {
    return this.model.dirty;
  }

  // ─── Layer Operations ───
  addLayer(name: string, width?: number, height?: number): LayerNode {
    if (this.model.layers.length >= MAX_LAYERS) {
      throw new Error(`Maximum layer limit of ${MAX_LAYERS} reached`);
    }

    const w = width ?? this.model.width;
    const h = height ?? this.model.height;
    if (w > getEffectiveMaxDim() || h > getEffectiveMaxDim()) {
      throw new Error(`Layer dimensions exceed device limit ${getEffectiveMaxDim()}px per side`);
    }

    if (!this.canAddLayer(w, h)) {
      throw new Error("E_RESOURCE_LIMIT: Adding this layer exceeds maximum pixel memory budget.");
    }

    const newLayer = createLayerNode(name, w, h);

    // Insert directly above active layer if selected, else at front (top) of stack
    const activeId = this.model.activeLayerId;
    const activeIndex = activeId ? this.model.layers.findIndex(l => l.id === activeId) : -1;
    if (activeIndex !== -1) {
      this.model.layers = [
        ...this.model.layers.slice(0, activeIndex),
        newLayer,
        ...this.model.layers.slice(activeIndex)
      ];
    } else {
      this.model.layers = [newLayer, ...this.model.layers];
    }
    this.model.activeLayerId = newLayer.id;
    this.model.dirty = true;
    this.markLayerDirty(newLayer.id);
    this.notifyChange();

    return newLayer;
  }

  duplicateLayer(id: LayerId): LayerNode {
    if (this.model.layers.length >= MAX_LAYERS) {
      throw new Error(`Maximum layer limit of ${MAX_LAYERS} reached`);
    }

    const layer = this.getLayer(id);
    if (!layer) {
      throw new Error(`Layer with ID ${id} not found`);
    }

    if (!this.canAddLayer(layer.width, layer.height)) {
      throw new Error("E_RESOURCE_LIMIT: Duplicating this layer exceeds maximum pixel memory budget.");
    }

    const duplicated = duplicateLayerNode(layer);

    // Rename with numeric suffix instead of "copy"
    // "Background" → "Background 2", "Background 2" → "Background 3", etc.
    duplicated.name = this.nextDuplicateName(layer.name);

    const index = this.model.layers.findIndex(l => l.id === id);
    if (index !== -1) {
      const updated = [...this.model.layers];
      updated.splice(index, 0, duplicated);
      this.model.layers = updated;
    } else {
      this.model.layers = [duplicated, ...this.model.layers];
    }

    this.model.activeLayerId = duplicated.id;
    this.model.dirty = true;
    this.markLayerDirty(duplicated.id);
    this.notifyChange();

    return duplicated;
  }

  mergeDown(id: LayerId): void {
    const index = this.model.layers.findIndex(l => l.id === id);
    if (index === -1 || index >= this.model.layers.length - 1) {
      return;
    }

    const top = this.model.layers[index];
    const bottom = this.model.layers[index + 1];

    const mergedW = this.model.width;
    const mergedH = this.model.height;

    const mergedBitmap = compositeTwoLayers(top, bottom, mergedW, mergedH);

    const mergedLayer = createMergedLayerNode(
      `${top.name} + ${bottom.name}`,
      mergedW,
      mergedH,
      mergedBitmap,
      bottom.locked || top.locked,
      bottom.blendMode
    );

    // Clean up WebGL textures for merged layers
    this.dirtyLayerIds.delete(top.id);
    this.textureHandles.delete(top.id);
    this.dirtyLayerIds.delete(bottom.id);
    this.textureHandles.delete(bottom.id);

    const updated = [...this.model.layers];
    updated.splice(index, 2, mergedLayer);
    this.model.layers = updated;

    this.model.activeLayerId = mergedLayer.id;
    this.model.dirty = true;
    this.markLayerDirty(mergedLayer.id);
    this.notifyChange();
  }

  flattenLayers(): void {
    if (this.model.layers.length <= 1) return;

    const mergedW = this.model.width;
    const mergedH = this.model.height;

    const mergedBitmap = compositeAllLayers(this.model.layers, mergedW, mergedH);

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

    for (const layer of this.model.layers) {
      this.dirtyLayerIds.delete(layer.id);
      this.textureHandles.delete(layer.id);
    }

    this.model.layers = [flattenedLayer];
    this.model.activeLayerId = flattenedLayer.id;
    this.model.dirty = true;
    this.markLayerDirty(flattenedLayer.id);
    this.notifyChange();
  }

  deleteLayer(id: LayerId): void {
    const layer = this.getLayer(id);
    if (layer?.isBackground) return;

    if (this.model.layers.length <= 1) {
      return; // prevent deleting the last layer
    }

    const index = this.model.layers.findIndex(l => l.id === id);
    if (index !== -1) {
      const removed = this.model.layers[index];
      this.model.layers = this.model.layers.filter(l => l.id !== id);
      this.dirtyLayerIds.delete(id);
      this.textureHandles.delete(id);
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
      if (this.model.activeLayerId === id) {
        const nextActiveIndex = Math.min(index, this.model.layers.length - 1);
        this.model.activeLayerId = this.model.layers[nextActiveIndex].id;
      }

      this.model.dirty = true;
      this.notifyChange();
    }
  }

  reorderLayer(fromIndex: number, toIndex: number): void {
    if (fromIndex < 0 || fromIndex >= this.model.layers.length ||
        toIndex < 0 || toIndex >= this.model.layers.length) {
      return;
    }

    const fromLayer = this.model.layers[fromIndex];
    // The Background layer is locked to the bottom of the stack — it can
    // never be reordered, and no other layer may be placed below it (a
    // layer beneath the opaque Background would be unreachable / hidden).
    if (fromLayer?.isBackground) return;

    const updated = [...this.model.layers];
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

    this.model.layers = updated;
    this.model.dirty = true;
    this.notifyChange();
  }

  setActiveLayer(id: LayerId | null): void {
    if (id === null || this.model.layers.some(l => l.id === id)) {
      this.model.activeLayerId = id;
      this.notifyChange();
    }
  }

  // ─── Layer Properties ───
  // NOTE: caller MUST call history.commit() BEFORE this method
  setLayerOpacity(id: LayerId, opacity: number): void {
    const layer = this.getLayer(id);
    if (layer && !layer.locked) {
      layer.opacity = Math.max(0.0, Math.min(1.0, opacity));
      this.model.dirty = true;
      this.notifyChange();
    }
  }

  setLayerVisibility(id: LayerId, visible: boolean): void {
    const layer = this.getLayer(id);
    if (layer) {
      layer.visible = visible;
      this.model.dirty = true;
      this.notifyChange();
    }
  }

  setLayerLocked(id: LayerId, locked: boolean): void {
    const layer = this.getLayer(id);
    if (layer) {
      layer.locked = locked;
      this.model.dirty = true;
      this.notifyChange();
    }
  }

  // NOTE: caller MUST call history.commit() BEFORE this method
  setLayerLockTransparency(id: LayerId, locked: boolean): void {
    const layer = this.getLayer(id);
    if (layer) {
      layer.lockTransparency = locked;
      this.model.dirty = true;
      this.notifyChange();
    }
  }

  // NOTE: caller MUST call history.commit() BEFORE this method
  setLayerLockPosition(id: LayerId, locked: boolean): void {
    const layer = this.getLayer(id);
    if (layer) {
      layer.lockPosition = locked;
      this.model.dirty = true;
      this.notifyChange();
    }
  }

  // NOTE: caller MUST call history.commit() BEFORE this method
  setLayerLockRotation(id: LayerId, locked: boolean): void {
    const layer = this.getLayer(id);
    if (layer) {
      layer.lockRotation = locked;
      this.model.dirty = true;
      this.notifyChange();
    }
  }

  /** Strip trailing number from a layer name to get the base name */
  private baseName(name: string): string {
    const match = name.match(/^(.*?)\s*(\d+)$/);
    return match ? match[1].trimEnd() : name.trimEnd();
  }

  /** Generate the next numeric-suffixed name for a duplicating layer.
   *  "Background" → "Background 2", "Background 2" → "Background 3", etc. */
  private nextDuplicateName(layerName: string): string {
    const base = this.baseName(layerName);
    const prefix = `${base} `;
    let maxNum = 1;
    for (const l of this.model.layers) {
      if (l.name.startsWith(prefix)) {
        const num = parseInt(l.name.slice(prefix.length), 10);
        if (!isNaN(num) && num > maxNum) maxNum = num;
      }
    }
    return `${base} ${maxNum + 1}`;
  }

  // NOTE: caller MUST call history.commit() BEFORE this method
  setLayerName(id: LayerId, name: string): void {
    const layer = this.getLayer(id);
    if (layer) {
      // Renaming Background → normal layer
      if (layer.isBackground) {
        layer.isBackground = undefined;
        layer.lockPosition = false;
        layer.lockRotation = false;
      }
      layer.name = name;
      this.model.dirty = true;
      this.notifyChange();
    }
  }

  // NOTE: caller MUST call history.commit() BEFORE this method
  setLayerBlendMode(id: LayerId, mode: BlendMode): void {
    const layer = this.getLayer(id);
    if (layer && !layer.locked) {
      layer.blendMode = mode;
      this.model.dirty = true;
      this.notifyChange();
    }
  }

  // ─── Layer Transform ───
  moveLayer(id: LayerId, x: number, y: number): void {
    const layer = this.getLayer(id);
    if (layer && !layer.locked && !layer.lockPosition) {
      layer.transform.x = x;
      layer.transform.y = y;
      this.model.dirty = true;
      this.notifyChange();
    }
  }

  transformLayer(id: LayerId, transform: Partial<Transform2D>): void {
    const layer = this.getLayer(id);
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
      this.model.dirty = true;
      this.notifyChange();
    }
  }

  flipLayer(id: LayerId, axis: "h" | "v"): void {
    const layer = this.getLayer(id);
    if (layer && !layer.locked) {
      if (axis === "h") {
        layer.transform.flipH = !layer.transform.flipH;
      } else {
        layer.transform.flipV = !layer.transform.flipV;
      }
      this.model.dirty = true;
      this.notifyChange();
    }
  }

  // ─── Selection ───
  createSelection(x: number, y: number, w: number, h: number, angle?: number, shape?: "rect" | "ellipse"): void {
    // Only store `shape` when non-default (ellipse) so the selection object
    // stays backward-compatible (rect selections have no `shape` key).
    this.model.selection = shape === "ellipse"
      ? { x, y, width: w, height: h, angle: angle ?? 0, shape: "ellipse" }
      : { x, y, width: w, height: h, angle: angle ?? 0 };
    this.notifyChange();
  }

  clearSelection(): void {
    this.model.selection = null;
    this.notifyChange();
  }

  selectAll(): void {
    this.model.selection = {
      x: 0,
      y: 0,
      width: this.model.width,
      height: this.model.height,
      angle: 0,
    };
    this.notifyChange();
  }

  invertSelection(): void {
    if (this.model.selection) {
      this.model.selection = {
        ...this.model.selection,
        inverted: !this.model.selection.inverted,
      };
    } else {
      this.selectAll();
      return;
    }
    this.notifyChange();
  }

  // ─── Viewport ───
  setViewport(viewport: Partial<ViewportState>): void {
    this.model.viewport = {
      ...this.model.viewport,
      ...viewport
    };
    this.notifyChange();
  }

  pan(dx: number, dy: number): void {
    this.model.viewport.panX += dx;
    this.model.viewport.panY += dy;
    this.notifyChange();
  }

  zoom(factor: number, anchorX?: number, anchorY?: number): void {
    const currentZoom = this.model.viewport.zoom;
    const nextZoom = Math.max(0.01, Math.min(100.0, currentZoom * factor));

    if (anchorX !== undefined && anchorY !== undefined) {
      // Zoom centered at anchor point
      this.model.viewport.panX = anchorX - (anchorX - this.model.viewport.panX) * (nextZoom / currentZoom);
      this.model.viewport.panY = anchorY - (anchorY - this.model.viewport.panY) * (nextZoom / currentZoom);
    }

    this.model.viewport.zoom = nextZoom;
    this.notifyChange();
  }

  fitToScreen(containerWidth: number, containerHeight: number): void {
    const padding = 80;
    const fitZoom = Math.min(
      (containerWidth - padding) / this.model.width,
      (containerHeight - padding) / this.model.height,
      10.0
    );

    this.model.viewport.zoom = Math.max(0.05, fitZoom);
    this.model.viewport.panX = (containerWidth - this.model.width * this.model.viewport.zoom) / 2;
    this.model.viewport.panY = (containerHeight - this.model.height * this.model.viewport.zoom) / 2;
    this.notifyChange();
  }

  /**
   * Fit the active selection bounds into the viewport (centered), like a
   * zoom-to-selection command in similar editors. Clamps the rect to the
   * document and falls back to the whole-document fit when no selection
   * exists (so it is always safe to call).
   */
  zoomToSelection(containerWidth: number, containerHeight: number): void {
    const sel = this.model.selection;
    if (sel) {
      const left = Math.max(0, Math.min(this.model.width, sel.x));
      const top = Math.max(0, Math.min(this.model.height, sel.y));
      const right = Math.max(0, Math.min(this.model.width, sel.x + sel.width));
      const bottom = Math.max(0, Math.min(this.model.height, sel.y + sel.height));
      const w = Math.max(1, right - left);
      const h = Math.max(1, bottom - top);
      const padding = 80;
      const zoom = Math.min(
        (containerWidth - padding) / w,
        (containerHeight - padding) / h,
        10.0
      );
      this.model.viewport.zoom = Math.max(0.05, zoom);
      this.model.viewport.panX = (containerWidth - w * this.model.viewport.zoom) / 2 - left * this.model.viewport.zoom;
      this.model.viewport.panY = (containerHeight - h * this.model.viewport.zoom) / 2 - top * this.model.viewport.zoom;
      this.notifyChange();
      return;
    }
    this.fitToScreen(containerWidth, containerHeight);
  }

  // ─── Canvas Operations ───
  cropCanvas(x: number, y: number, width: number, height: number): void {
    if (width <= 0 || height <= 0) return;
    if (width > getEffectiveMaxDim() || height > getEffectiveMaxDim()) return;

    this.model.width = width;
    this.model.height = height;

    performCropCanvas(this.model.layers, x, y);

    this.model.selection = null; // Reset selection on crop
    this.model.dirty = true;
    this.notifyChange();
  }

  applyCrop(
    x: number,
    y: number,
    width: number,
    height: number,
    options?: {
      deleteCroppedPixels?: boolean;
      targetSize?: { w: number; h: number } | null;
      rotation?: number;
      fillBackgroundColor?: string | null;
    },
  ): void {
    if (width <= 0 || height <= 0) return;

    const targetSize = options?.targetSize ?? null;
    const finalW = targetSize ? targetSize.w : width;
    const finalH = targetSize ? targetSize.h : height;
    if (finalW > getEffectiveMaxDim() || finalH > getEffectiveMaxDim()) return;

    performApplyCrop(this.model.layers, x, y, width, height, options);

    this.model.width = finalW;
    this.model.height = finalH;

    this.model.selection = null;
    this.model.dirty = true;
    this.notifyChange();
  }

  resizeCanvas(width: number, height: number): void {
    if (width <= 0 || height <= 0) return;
    const maxDim = getEffectiveMaxDim();
    if (width > maxDim || height > maxDim) return;

    // Memory budget check: resizing to larger dimensions could cause OOM
    // if layers are later re-allocated at the new size.
    const newBytes = width * height * 4;
    // Estimate: each existing layer could be resized to the new canvas size.
    // This is conservative — layers may keep their own dimensions, but
    // paint operations or crop/resize to canvas size could trigger re-alloc.
    const layerCount = this.model.layers.length;
    if (layerCount > 0) {
      const estimatedGrowth = (newBytes - (this.model.width * this.model.height * 4)) * layerCount;
      const projected = this.calculateMemoryUsage() + Math.max(0, estimatedGrowth);
      if (projected > MAX_PIXEL_BUDGET) {
        throw new Error("E_RESOURCE_LIMIT: Resizing canvas exceeds maximum pixel memory budget.");
      }
    }

    this.model.width = width;
    this.model.height = height;
    this.model.dirty = true;
    this.notifyChange();
  }

  // ─── Image Data ───
  getLayerImageBitmap(id: LayerId): ImageBitmap | null {
    const layer = this.getLayer(id);
    return layer ? layer.imageBitmap : null;
  }

  setLayerImageBitmap(id: LayerId, bitmap: ImageBitmap): void {
    const layer = this.getLayer(id);
    if (layer) {
      if (!bitmap) {
        throw new TypeError("Bitmap cannot be null");
      }

      // Memory budget check: reject bitmap that would exceed the pixel
      // memory budget.  Subtract the current layer's bytes since the new
      // bitmap replaces the old one.
      const bitmapBytes = bitmap.width * bitmap.height * 4;
      const oldBytes = layer.width * layer.height * 4;
      const currentBytes = this.calculateMemoryUsage();
      const totalBytes = currentBytes - oldBytes + bitmapBytes;
      if (totalBytes > MAX_PIXEL_BUDGET) {
        throw new Error("E_RESOURCE_LIMIT: Setting this bitmap exceeds maximum pixel memory budget.");
      }

      // NOTE: we intentionally do NOT close the old imageBitmap here.
      // Snapshots in the undo/redo stack may hold a reference to it;
      // closing it here would make those snapshots point to a closed/
      // detached bitmap, causing "image source is detached" errors on
      // restore (undo/redo).  Memory is reclaimed by GC once no
      // snapshot or layer references remain.
      layer.imageBitmap = bitmap;
      layer.baseImageBitmap = null;
      // NOTE: intentionally do NOT clear basicAdjustment here. Adjustments are
      // a non-destructive layer-level effect applied in the renderer shader, so
      // replacing the layer bitmap (paint commit, fill, etc.) must keep the
      // adjustment param — otherwise it silently resets to zero after a brush/
      // eraser stroke. The shader re-applies it on top of the new bitmap.
      if (bitmap) {
        layer.width = bitmap.width;
        layer.height = bitmap.height;
      }
      this.model.dirty = true;
      this.markLayerDirty(id);
      this.notifyVisualChange();
    }
  }

  applyBasicAdjustment(id: LayerId, adjustment: BasicAdjustment): void {
    const layer = this.getLayer(id);
    if (!layer || !layer.imageBitmap) return;

    // Non-destructive: store the adjustment as a render param. The renderer
    // applies it in the layer fragment shader (u_adjustment), so the live
    // preview is instant regardless of image size. The layer bitmap stays the
    // original (base) pixels — no CPU pixel loop, no texture re-upload during
    // editing. Export bakes the adjustment via applyBasicAdjustmentToPixels.
    const normalized = normalizeBasicAdjustment(adjustment);
    layer.basicAdjustment = normalized;
    layer.hasAdjustments =
      normalized.brightness !== 0 ||
      normalized.contrast !== 0 ||
      normalized.saturation !== 0;
    this.model.dirty = true;
    this.markLayerDirty(id);
    this.notifyVisualChange();
  }

  clearBasicAdjustments(id: LayerId): void {
    const layer = this.getLayer(id);
    if (layer) {
      // With non-destructive adjustments the layer bitmap is already the
      // original (base) pixels, so nothing to restore — just drop the param.
      layer.basicAdjustment = undefined;
      layer.hasAdjustments = false;
      this.model.dirty = true;
      this.markLayerDirty(id);
      this.notifyVisualChange();
    }
  }

  /**
   * Commits the live (GPU-previewed) adjustment into the layer's pixels. Called
   * when the user releases the adjustment slider. The adjustment is baked via a
   * CPU pixel pass and the param is dropped, so the stored bitmap now reflects
   * the adjustment and any later paint shows the raw picked colors. This keeps
   * the slider drag lag-free (GPU preview) while matching the expected
   * "layer adjustment is applied to the layer's pixels" behavior.
   */
  commitBasicAdjustment(id: LayerId, renderer?: RenderBackend): "gpu" | "cpu" | "noop" {
    const layer = this.getLayer(id);
    if (!layer || !layer.imageBitmap || !layer.basicAdjustment) return "noop";

    const adj = layer.basicAdjustment;
    // No-op adjustment: just drop the param, skip the pixel pass.
    if (adj.brightness === 0 && adj.contrast === 0 && adj.saturation === 0) {
      this.clearBasicAdjustments(id);
      return "noop";
    }

    // Prefer the GPU bake (fast, off the paint path) when the renderer offers
    // it; fall back to the CPU pixel pass otherwise (export, fill, tests).
    let baked: ImageBitmap;
    let usedGpu = false;
    const gpu = renderer?.bakeLayerToBitmap?.(id, layer.width, layer.height, adj);
    if (gpu) {
      baked = gpu;
      usedGpu = true;
    } else {
      baked = bakeAdjustmentToBitmap(layer.imageBitmap, layer.width, layer.height, adj);
    }
    // NOTE: do NOT close the old imageBitmap — an undo/redo snapshot may still
    // reference it. GC reclaims it once no snapshot/layer references remain.
    layer.imageBitmap = baked;
    layer.baseImageBitmap = null;
    layer.basicAdjustment = undefined;
    layer.hasAdjustments = false;
    this.model.dirty = true;
    this.markLayerDirty(id);
    return usedGpu ? "gpu" : "cpu";
    this.notifyVisualChange();
  }

  // ─── Texture Handles ───
  setTextureHandle(layerId: LayerId, handle: TextureHandle): void {
    this.textureHandles.set(layerId, handle);
  }

  getTextureHandle(layerId: LayerId): TextureHandle | undefined {
    return this.textureHandles.get(layerId);
  }

  // ─── Render State ───
  getRenderState(): RenderState {
    const renderLayers: RenderLayer[] = this.model.layers.map(l => {
      const handle = this.textureHandles.get(l.id) || { id: `tex-${l.id}` };
      return {
        id: l.id,
        textureHandle: handle,
        visible: l.visible,
        opacity: l.opacity,
        blendMode: l.blendMode,
        transform: l.transform,
        width: l.width,
        height: l.height,
        basicAdjustment: l.basicAdjustment
      };
    });

    return {
      documentId: this.model.id,
      viewport: this.model.viewport,
      documentSize: { width: this.model.width, height: this.model.height },
      layers: renderLayers,
      selection: this.model.selection,
      checkerboard: true,
      backgroundColor: [0.05, 0.06, 0.07, 1.0] // Midnight dark background
    };
  }

  // ─── Dirty Tracking ───
  markLayerDirty(id: LayerId): void {
    this.dirtyLayerIds.add(id);
  }

  getDirtyLayerIds(): LayerId[] {
    return Array.from(this.dirtyLayerIds);
  }

  clearDirty(): void {
    this.dirtyLayerIds.clear();
    this.model.dirty = false;
    // Snapshot the saved baseline so undo back to a *different* state than
    // what was saved is still reported dirty (regression: undo to pre-edit
    // state after save showed "clean" because the flag lived in the model).
    this.savedModel = createSnapshot(this.model);
  }

  // ─── Change Notification ───
  onChange(callback: () => void): void {
    this.onChangeCallback = callback;
  }

  onVisualChange(callback: () => void): void {
    this.onVisualChangeCallback = callback;
  }

  private notifyChange(): void {
    if (this.onChangeCallback) {
      this.onChangeCallback();
    }
  }

  private notifyVisualChange(): void {
    if (this.onVisualChangeCallback) {
      this.onVisualChangeCallback();
    }
  }

  // ─── Snapshot & Restore (Undo/Redo Support) ───
  snapshot(): DocumentModel {
    return createSnapshot(this.model);
  }

  restore(snapshot: DocumentModel, options?: { restoreViewport?: boolean }): void {
    const currentViewport = { ...this.model.viewport };

    // NOTE: we intentionally do NOT close any bitmaps from the current model
    // here.  Snapshots in the undo/redo history stack may hold references to
    // those bitmaps; closing them would make future restore() calls point to
    // closed/detached bitmaps ("image source is detached" errors).  Bitmap
    // memory is reclaimed by GC once no snapshot or layer references remain.

    this.model = restoreSnapshot(snapshot);

    // Invariant: the Background layer is always the bottommost layer.
    // A restored snapshot (e.g. a legacy / hand-edited saved file)
    // could carry the Background at a non-bottom index; re-seat it so
    // no layer is left hidden behind it.
    const restoredLayers = [...this.model.layers];
    const bgIdx = restoredLayers.findIndex((l) => l.isBackground);
    if (bgIdx >= 0 && bgIdx !== restoredLayers.length - 1) {
      const [bg] = restoredLayers.splice(bgIdx, 1);
      restoredLayers.push(bg);
      this.model.layers = restoredLayers;
    }

    if (!options?.restoreViewport) {
      this.model.viewport = currentViewport;
    }

    // Clean up stale texture handles for layers that no longer exist
    const currentIds = new Set(this.model.layers.map(l => l.id));
    for (const existingId of this.textureHandles.keys()) {
      if (!currentIds.has(existingId)) {
        this.textureHandles.delete(existingId);
      }
    }
    // Mark all restored layers as dirty so any consumer (renderer, UI)
    // knows textures need re-upload.  Previous code called dirtyLayerIds.clear()
    // here, which left consumers with no signal that the layer bitmaps had
    // changed (regression 2026-07-05: "layer turns black on undo" because
    // the renderer's WebGL texture was re-uploaded only by the direct caller
    // (restoreHistorySnapshot), but code paths such as cancelLayerTransformSession
    // called engine.restore() without the re-upload step).
    this.dirtyLayerIds.clear();
    for (const layer of this.model.layers) {
      this.dirtyLayerIds.add(layer.id);
    }
    // Dirty = restored state differs from the last *saved* baseline. Without
    // this, undo to a pre-save state (whose snapshot carries dirty=false)
    // would falsely report clean after a save.
    this.model.dirty = this.savedModel
      ? !DocumentEngine.modelsEqual(this.savedModel, this.model)
      : this.model.dirty;
      this.notifyVisualChange();
    }

  // Cheap structural equality (no pixel compare) used for dirty detection
  // against the saved baseline. Compares refs for immutable ImageBitmaps;
  // enough to catch any real edit.
  private static modelsEqual(a: DocumentModel, b: DocumentModel): boolean {
    if (a.id !== b.id || a.width !== b.width || a.height !== b.height) return false;
    if (a.layers.length !== b.layers.length) return false;
    for (let i = 0; i < a.layers.length; i++) {
      const x = a.layers[i];
      const y = b.layers[i];
      if (
        x.id !== y.id ||
        x.name !== y.name ||
        x.visible !== y.visible ||
        x.opacity !== y.opacity ||
        x.locked !== y.locked ||
        x.isBackground !== y.isBackground ||
        x.hasAdjustments !== y.hasAdjustments ||
        x.blendMode !== y.blendMode ||
        x.width !== y.width ||
        x.height !== y.height ||
        x.imageBitmap !== y.imageBitmap ||
        x.baseImageBitmap !== y.baseImageBitmap ||
        x.lockPosition !== y.lockPosition ||
        x.lockRotation !== y.lockRotation ||
        x.lockTransparency !== y.lockTransparency ||
        // basicAdjustment is a small plain object; shallow-compare fields
        (x.basicAdjustment?.brightness ?? 0) !== (y.basicAdjustment?.brightness ?? 0) ||
        (x.basicAdjustment?.contrast ?? 0) !== (y.basicAdjustment?.contrast ?? 0) ||
        (x.basicAdjustment?.saturation ?? 0) !== (y.basicAdjustment?.saturation ?? 0) ||
        x.transform.x !== y.transform.x ||
        x.transform.y !== y.transform.y ||
        x.transform.scaleX !== y.transform.scaleX ||
        x.transform.scaleY !== y.transform.scaleY ||
        x.transform.rotation !== y.transform.rotation
      ) {
        return false;
      }
    }
    return true;
  }

  // ─── Memory Budget ───
  calculateMemoryUsage(): number {
    let bytes = 0;
    for (const layer of this.model.layers) {
      bytes += layer.width * layer.height * 4; // RGBA8
    }
    return bytes;
  }

  canAddLayer(width: number, height: number): boolean {
    const projected = this.calculateMemoryUsage() + (width * height * 4);
    return projected <= MAX_PIXEL_BUDGET;
  }

  // ─── Pixel Sampling (Eyedropper support) ───
  samplePixel(x: number, y: number): [number, number, number, number] {
    return performPixelSampling(this.model.layers, this.model.width, this.model.height, x, y);
  }

  /** Alpha (0..1) of a single layer at a document-space point, transform-aware. */
  sampleLayerAlpha(layerId: string, x: number, y: number): number {
    return sampleSingleLayerAlpha(this.model.layers, x, y, layerId);
  }
}
