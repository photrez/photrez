// SPDX-License-Identifier: AGPL-3.0-or-later
// DocumentEngine — the document model facade.
// All mutation logic lives in domain modules (layerOps / viewportOps /
// selectionOps / cropApply / layerFactory / layerComposite / snapshot /
// pixelSample / layerAdjustments). This class owns the DocumentModel instance
// plus engine-level side effects: texture handles, dirty tracking, change
// callbacks, memory budget and GPU bake coordination.
// (Report #20 phase 3: split from a single 1006-LOC file into domain modules.)

import type {
  DocumentId, LayerId, DocumentModel, LayerNode,
  ViewportState, SelectionState, RenderState, BlendMode,
  Transform2D, TextureHandle, RenderLayer
} from "./types";
import { MAX_PIXEL_BUDGET, getEffectiveMaxDim } from "./types";

import { drawLayerToContext, compositeTwoLayers, compositeAllLayers } from "./layerComposite";
import { getLoadedWasmModule } from "@/components/editor/wasmExport";
const USE_RUST_SSOT = true; // flip to true to test Rust DocumentEngine vertical slice (add/select/undo) — testing no-regresi
import { performCropCanvas, performApplyCrop } from "./cropApply";
import { createSnapshot, restoreSnapshot } from "./snapshot";
import { performPixelSampling, sampleSingleLayerAlpha } from "./pixelSample";
import { normalizeBasicAdjustment, bakeAdjustmentToBitmap, bakeAdjustmentToBitmapGpu, type BasicAdjustment } from "./layerAdjustments";
import type { RenderBackend } from "../renderer/types";
import { invertRgba } from "../lib/gpu/gpuCompute";

import {
  addLayer as applyAddLayer,
  addShapeLayer as applyAddShapeLayer,
  duplicateLayer as applyDuplicateLayer,
  mergeDown as applyMergeDown,
  mergeSelectedLayers as applyMergeSelectedLayers,
  flattenLayers as applyFlattenLayers,
  deleteLayer as applyDeleteLayer,
  reorderLayer as applyReorderLayer,
  setActiveLayer as applySetActiveLayer,
  setLayerOpacity as applySetLayerOpacity,
  setLayerVisibility as applySetLayerVisibility,
  setLayerLocked as applySetLayerLocked,
  setLayerLockTransparency as applySetLayerLockTransparency,
  setLayerLockPosition as applySetLayerLockPosition,
  setLayerLockRotation as applySetLayerLockRotation,
  setLayerName as applySetLayerName,
  setLayerBlendMode as applySetLayerBlendMode,
  moveLayer as applyMoveLayer,
  transformLayer as applyTransformLayer,
  flipLayer as applyFlipLayer,
  calculateMemoryUsage as calcLayerMemory,
  canAddLayer as canFitLayer,
  isShapeLayer as applyIsShapeLayer,
  shapeLayerToRaster as applyShapeLayerToRaster,
  addTextLayer as applyAddTextLayer,
  isTextLayer as applyIsTextLayer,
  textLayerToRaster as applyTextLayerToRaster,
} from "./layerOps";
import { renderShapeToBitmap } from "./shapeRaster";
import { rasterizeText } from "./textRasterizer";
import { normalizeTextData, type TextData } from "./textTypes";
import type { ShapeParams } from "./types";
import {
  setViewport as applySetViewport,
  pan as applyPan,
  zoom as applyZoom,
  fitToScreen as applyFitToScreen,
  zoomToSelection as applyZoomToSelection,
} from "./viewportOps";
import {
  createSelection as applyCreateSelection,
  clearSelection as applyClearSelection,
  selectAll as applySelectAll,
  invertSelection as applyInvertSelection,
} from "./selectionOps";

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
  // Layer that must not be composited while a text edit session is open over
  // it (the edit overlay renders the live text — seeing BOTH the canvas
  // raster and the textarea produced the double/stacked text).
  private renderHiddenLayerId: LayerId | null = null;
  // Bitmaps captured by snapshot() are referenced by the undo/redo stacks (and
  // saved baselines) — they must survive bitmap replacement. WeakSet so stale
  // entries never pin an ImageBitmap object after its snapshot is evicted.
  private snapshotRetainedBitmaps = new WeakSet<ImageBitmap>();
  private rustEngine: any = null;

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
    if (USE_RUST_SSOT) {
      const m = getLoadedWasmModule();
      if (m?.DocumentEngine) {
        try { this.rustEngine = new m.DocumentEngine(id, name, width, height); } catch {}
      }
    }
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
    if (USE_RUST_SSOT && this.rustEngine) {
      const id = `layer-${Math.random().toString(36).slice(2, 10)}`;
      const w = width ?? this.model.width;
      const h = height ?? this.model.height;
      this.rustEngine.add_layer(id, name, w, h);
      // Sync back to TS model for thin-renderer (so layers() signal updates)
      try {
        const layersJson = this.rustEngine.get_layers_json();
        const rustLayers = JSON.parse(layersJson);
        // Map Rust Layer (camelCase) to TS LayerNode (keep minimal fields for the slice)
        this.model.layers = rustLayers.map((l: any) => ({
          id: l.id, name: l.name, type: "raster" as const, visible: l.visible, opacity: l.opacity,
          locked: l.locked, blendMode: l.blendMode, transform: l.transform, width: l.width, height: l.height,
          imageBitmap: null, hasAdjustments: l.hasAdjustments ?? false,
        }));
        this.model.activeLayerId = this.rustEngine.get_active_layer_id() ?? null;
        this.model.dirty = true;
        const newLayer = this.model.layers.find(l => l.id === id)!;
        this.markLayerDirty(newLayer.id);
        this.notifyChange();
        return newLayer;
      } catch {}
    }
    const newLayer = applyAddLayer(this.model, name, width, height);
    this.markLayerDirty(newLayer.id);
    this.notifyChange();
    return newLayer;
  }

  duplicateLayer(id: LayerId): LayerNode {
    if (USE_RUST_SSOT && this.rustEngine) {
      try {
        const newId: string | null = this.rustEngine.duplicate_layer(id);
        if (newId) {
          const layersJson = this.rustEngine.get_layers_json();
          const rustLayers = JSON.parse(layersJson);
          this.model.layers = rustLayers.map((l: any) => ({
            id: l.id, name: l.name, type: "raster" as const, visible: l.visible, opacity: l.opacity,
            locked: l.locked, blendMode: l.blendMode, transform: l.transform, width: l.width, height: l.height,
            imageBitmap: null, hasAdjustments: l.hasAdjustments ?? false,
          }));
          this.model.activeLayerId = this.rustEngine.get_active_layer_id() ?? null;
          this.model.dirty = true;
          const dup = this.model.layers.find(l => l.id === newId)!;
          this.markLayerDirty(dup.id);
          this.notifyChange();
          return dup;
        }
      } catch {}
    }
    const duplicated = applyDuplicateLayer(this.model, id);
    this.markLayerDirty(duplicated.id);
    this.notifyChange();
    return duplicated;
  }

  mergeDown(id: LayerId): void {
    const result = applyMergeDown(this.model, id);
    if (!result) return;

    // Clean up WebGL textures for merged layers
    for (const removedId of result.removedIds) {
      this.dirtyLayerIds.delete(removedId);
      this.textureHandles.delete(removedId);
    }
    this.markLayerDirty(result.merged.id);
    this.notifyChange();
  }

  mergeSelectedLayers(ids: LayerId[]): void {
    const result = applyMergeSelectedLayers(this.model, ids);
    if (!result) return;

    // Clean up WebGL textures for merged layers
    for (const removedId of result.removedIds) {
      this.dirtyLayerIds.delete(removedId);
      this.textureHandles.delete(removedId);
    }
    this.markLayerDirty(result.merged.id);
    this.notifyChange();
  }

  flattenLayers(): void {
    const removedIds = applyFlattenLayers(this.model);
    if (removedIds.length === 0) return;

    for (const removedId of removedIds) {
      this.dirtyLayerIds.delete(removedId);
      this.textureHandles.delete(removedId);
    }
    this.markLayerDirty(this.model.activeLayerId!);
    this.notifyChange();
  }

  deleteLayer(id: LayerId): void {
    if (USE_RUST_SSOT && this.rustEngine) {
      try {
        const ok: boolean = this.rustEngine.delete_layer(id);
        if (ok) {
          const layersJson = this.rustEngine.get_layers_json();
          const rustLayers = JSON.parse(layersJson);
          this.model.layers = rustLayers.map((l: any) => ({
            id: l.id, name: l.name, type: "raster" as const, visible: l.visible, opacity: l.opacity,
            locked: l.locked, blendMode: l.blendMode, transform: l.transform, width: l.width, height: l.height,
            imageBitmap: null, hasAdjustments: l.hasAdjustments ?? false,
          }));
          this.model.activeLayerId = this.rustEngine.get_active_layer_id() ?? null;
          this.model.dirty = true;
          this.dirtyLayerIds.delete(id);
          this.textureHandles.delete(id);
          this.notifyChange();
          return;
        }
      } catch {}
    }
    const removedId = applyDeleteLayer(this.model, id);
    if (removedId === null) return;

    this.dirtyLayerIds.delete(removedId);
    this.textureHandles.delete(removedId);
    this.notifyChange();
  }

  reorderLayer(fromIndex: number, toIndex: number): void {
    if (USE_RUST_SSOT && this.rustEngine) {
      try {
        const ok: boolean = this.rustEngine.reorder_layer(fromIndex, toIndex);
        if (ok) {
          const layersJson = this.rustEngine.get_layers_json();
          const rustLayers = JSON.parse(layersJson);
          this.model.layers = rustLayers.map((l: any) => ({
            id: l.id, name: l.name, type: "raster" as const, visible: l.visible, opacity: l.opacity,
            locked: l.locked, blendMode: l.blendMode, transform: l.transform, width: l.width, height: l.height,
            imageBitmap: this.model.layers.find(x => x.id === l.id)?.imageBitmap ?? null, hasAdjustments: l.hasAdjustments ?? false,
          }));
          this.model.dirty = true;
          this.notifyChange();
          return;
        }
      } catch {}
    }
    applyReorderLayer(this.model, fromIndex, toIndex);
    this.notifyChange();
  }

  setActiveLayer(id: LayerId | null): void {
    if (USE_RUST_SSOT && this.rustEngine && id !== null) {
      try {
        const ok: boolean = this.rustEngine.set_active_layer(id);
        if (ok) {
          this.model.activeLayerId = id;
          this.notifyChange();
          return;
        }
      } catch {}
    }
    applySetActiveLayer(this.model, id);
    this.notifyChange();
  }

  // ─── Shape Layers ───
  addShapeLayer(name: string, params: ShapeParams): LayerNode {
    const layer = applyAddShapeLayer(this.model, name, params);
    this.markLayerDirty(layer.id);
    this.notifyChange();
    return layer;
  }

  updateShapeParams(id: LayerId, params: ShapeParams): void {
    const layer = this.getLayer(id);
    if (!layer || layer.type !== "shape") return; // no-op on non-shape
    const bitmap = renderShapeToBitmap(params);
    layer.width = bitmap.width;
    layer.height = bitmap.height;
    layer.shapeParams = params;
    this.replaceLayerBitmap(layer, bitmap);
    this.markLayerDirty(id);
    this.notifyChange();
  }

  shapeLayerToRaster(id: LayerId): void {
    const layer = this.getLayer(id);
    if (!layer || layer.type !== "shape") return;
    applyShapeLayerToRaster(layer);
    this.markLayerDirty(id);
    this.notifyChange();
  }

  isShapeLayer(id: LayerId): boolean {
    const layer = this.getLayer(id);
    return !!layer && applyIsShapeLayer(layer);
  }

  // ─── Text Layers ───
  addTextLayer(name: string, data: TextData): LayerNode {
    const layer = applyAddTextLayer(this.model, name, data);
    this.markLayerDirty(layer.id);
    this.notifyChange();
    return layer;
  }

  updateTextData(id: LayerId, data: TextData): void {
    const layer = this.getLayer(id);
    if (!layer || layer.type !== "text") return; // no-op on non-text
    const normalized = normalizeTextData(data);
    const { imageBitmap, width, height } = rasterizeText(normalized);
    layer.width = width;
    layer.height = height;
    layer.textData = normalized;
    this.replaceLayerBitmap(layer, imageBitmap);
    this.markLayerDirty(id);
    this.notifyChange();
  }

  textLayerToRaster(id: LayerId): void {
    const layer = this.getLayer(id);
    if (!layer || layer.type !== "text") return;
    applyTextLayerToRaster(layer);
    this.markLayerDirty(id);
    this.notifyChange();
  }

  isTextLayer(id: LayerId): boolean {
    const layer = this.getLayer(id);
    return !!layer && applyIsTextLayer(layer);
  }

  // ─── Layer Properties ───
  // NOTE: caller MUST call history.commit() BEFORE this method
  setLayerOpacity(id: LayerId, opacity: number): void {
    applySetLayerOpacity(this.model, id, opacity);
    this.notifyChange();
  }

  setLayerVisibility(id: LayerId, visible: boolean): void {
    applySetLayerVisibility(this.model, id, visible);
    this.notifyChange();
  }

  setLayerLocked(id: LayerId, locked: boolean): void {
    applySetLayerLocked(this.model, id, locked);
    this.notifyChange();
  }

  // NOTE: caller MUST call history.commit() BEFORE this method
  setLayerLockTransparency(id: LayerId, locked: boolean): void {
    applySetLayerLockTransparency(this.model, id, locked);
    this.notifyChange();
  }

  // NOTE: caller MUST call history.commit() BEFORE this method
  setLayerLockPosition(id: LayerId, locked: boolean): void {
    applySetLayerLockPosition(this.model, id, locked);
    this.notifyChange();
  }

  // NOTE: caller MUST call history.commit() BEFORE this method
  setLayerLockRotation(id: LayerId, locked: boolean): void {
    applySetLayerLockRotation(this.model, id, locked);
    this.notifyChange();
  }

  // NOTE: caller MUST call history.commit() BEFORE this method
  setLayerName(id: LayerId, name: string): void {
    applySetLayerName(this.model, id, name);
    this.notifyChange();
  }

  // NOTE: caller MUST call history.commit() BEFORE this method
  setLayerBlendMode(id: LayerId, mode: BlendMode): void {
    applySetLayerBlendMode(this.model, id, mode);
    this.notifyChange();
  }

  // ─── Layer Transform ───
  moveLayer(id: LayerId, x: number, y: number): void {
    applyMoveLayer(this.model, id, x, y);
    this.notifyChange();
  }

  /**
   * Move layer WITHOUT firing onChange — for live drag updates that fire
   * notifyChange on EVERY pointermove (50+ fps). The caller MUST call
   * flushChangeNotification() once when the interaction ends so workspace
   * sync (tab dirty state, title) still runs. Keyboard nudges and other
   * single-shot callers keep using moveLayer().
   */
  moveLayerSilent(id: LayerId, x: number, y: number): void {
    applyMoveLayer(this.model, id, x, y);
  }

  /** Fire the deferred onChange after a moveLayerSilent interaction. */
  flushChangeNotification(): void {
    this.notifyChange();
  }

  transformLayer(id: LayerId, transform: Partial<Transform2D>): void {
    applyTransformLayer(this.model, id, transform);
    this.notifyChange();
  }

  flipLayer(id: LayerId, axis: "h" | "v"): void {
    applyFlipLayer(this.model, id, axis);
    this.notifyChange();
  }

  // ─── Selection ───
  createSelection(x: number, y: number, w: number, h: number, angle?: number, shape?: "rect" | "ellipse"): void {
    if (USE_RUST_SSOT && this.rustEngine) {
      try {
        this.rustEngine.set_selection(x, y, w, h, angle ?? 0, shape ? shape : null, null);
        const json = this.rustEngine.get_selection_json();
        this.model.selection = JSON.parse(json);
        this.model.dirty = true;
        this.notifyChange();
        return;
      } catch {}
    }
    applyCreateSelection(this.model, x, y, w, h, angle, shape);
    this.notifyChange();
  }

  clearSelection(): void {
    if (USE_RUST_SSOT && this.rustEngine) {
      try {
        this.rustEngine.clear_selection();
        this.model.selection = null;
        this.model.dirty = true;
        this.notifyChange();
        return;
      } catch {}
    }
    applyClearSelection(this.model);
    this.notifyChange();
  }

  selectAll(): void {
    applySelectAll(this.model);
    this.notifyChange();
  }

  invertSelection(): void {
    applyInvertSelection(this.model);
    this.notifyChange();
  }

  // ─── Viewport ───
  setViewport(viewport: Partial<ViewportState>): void {
    applySetViewport(this.model, viewport);
    this.notifyChange();
  }

  pan(dx: number, dy: number): void {
    applyPan(this.model, dx, dy);
    this.notifyChange();
  }

  zoom(factor: number, anchorX?: number, anchorY?: number): void {
    applyZoom(this.model, factor, anchorX, anchorY);
    this.notifyChange();
  }

  fitToScreen(containerWidth: number, containerHeight: number): void {
    applyFitToScreen(this.model, containerWidth, containerHeight);
    this.notifyChange();
  }

  zoomToSelection(containerWidth: number, containerHeight: number): void {
    applyZoomToSelection(this.model, containerWidth, containerHeight);
    this.notifyChange();
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

  /**
   * Replace a layer's bitmap, closing the superseded one when NO committed
   * snapshot still references it. Snapshots share bitmap references (see
   * deleteLayer's note), so a bitmap captured by snapshot() must survive —
   * closing it would detach the undo/redo image. Bitmaps never snapshotted
   * (e.g. the intermediate rasters produced by rapid live typing during a
   * text session) are safe to close immediately, reclaiming the GPU memory
   * that previously accumulated until GC (@bug 2026-08-09 B5).
   */
  private replaceLayerBitmap(layer: LayerNode, bitmap: ImageBitmap): void {
    const prev = layer.imageBitmap;
    layer.imageBitmap = bitmap;
    // Close the superseded raster only when NO committed snapshot references
    // it (snapshot() registers live bitmaps). Snapshotted bitmaps survive for
    // undo/redo; unregistered ones (live-typing intermediates) close now,
    // reclaiming the VRAM that previously accumulated until GC (@bug B5).
    if (prev && prev !== bitmap && !this.snapshotRetainedBitmaps.has(prev)) {
      try { prev.close(); } catch { /* already closed */ }
    }
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
   * Inverts the pixels of a layer in place (per-layer color invert). Uses the
   * GPU compute path when available, falling back to the CPU pixel pass. The
   * result is baked into a fresh ImageBitmap via replaceLayerBitmap so any
   * undo/redo snapshot keeps the original raster.
   *
   * Callers MUST commit a history snapshot BEFORE calling this (mirrors the
   * other mutating engine methods that rely on the command layer for undo).
   */
  async invertLayerPixels(id: LayerId): Promise<"gpu" | "cpu" | "noop"> {
    const layer = this.getLayer(id);
    if (!layer || !layer.imageBitmap) return "noop";

    const { width, height } = layer;
    const canvas = new OffscreenCanvas(width, height);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("Failed to acquire 2D context for invert");
    ctx.drawImage(layer.imageBitmap, 0, 0);
    const imageData = ctx.getImageData(0, 0, width, height);
    const res = await invertRgba(imageData.data);
    imageData.data.set(res.data);
    ctx.putImageData(imageData, 0, 0);
    const newBitmap = canvas.transferToImageBitmap();
    this.replaceLayerBitmap(layer, newBitmap);
    this.markLayerDirty(id);
    this.notifyVisualChange();
    return res.usedGpu ? "gpu" : "cpu";
  }

  /**
   * Commits the live (GPU-previewed) adjustment into the layer's pixels. Called
   * when the user releases the adjustment slider. The adjustment is baked via a
   * CPU pixel pass and the param is dropped, so the stored bitmap now reflects
   * the adjustment and any later paint shows the raw picked colors. This keeps
   * the slider drag lag-free (GPU preview) while matching the expected
   * "layer adjustment is applied to the layer's pixels" behavior.
   */
  async commitBasicAdjustment(id: LayerId, renderer?: RenderBackend): Promise<"gpu" | "cpu" | "noop"> {
    const layer = this.getLayer(id);
    if (!layer || !layer.imageBitmap || !layer.basicAdjustment) return "noop";

    const adj = layer.basicAdjustment;
    // No-op adjustment: just drop the param, skip the pixel pass.
    if (adj.brightness === 0 && adj.contrast === 0 && adj.saturation === 0) {
      this.clearBasicAdjustments(id);
      return "noop";
    }

    // Poa (Rust wasm WebGPU) is fastest per 2026-08-18 bench (5.9× vs C at 12 Mpx, 2.9× at 2 Mpx) — try first even when WebGL is present.
    let baked: ImageBitmap | null = null;
    let usedGpu = false;
    try {
      baked = await bakeAdjustmentToBitmapGpu(layer.imageBitmap, layer.width, layer.height, adj);
      if (baked) usedGpu = true;
    } catch {
      baked = null;
    }
    // Fallback to WebGL renderer bakes (PBO async, then sync) if Poa/WGSL failed.
    if (!baked) {
      const gpuAsync = renderer?.bakeLayerToBitmapAsync?.(id, layer.width, layer.height, adj);
      if (gpuAsync) {
        const gpu = await gpuAsync;
        if (gpu) {
          baked = gpu;
          usedGpu = true;
        }
      }
    }
    if (!baked) {
      const gpu = renderer?.bakeLayerToBitmap?.(id, layer.width, layer.height, adj) ?? null;
      if (gpu) {
        baked = gpu;
        usedGpu = true;
      }
    }
    if (!baked) {
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
    this.notifyChange();
    return usedGpu ? "gpu" : "cpu";
  }

  // ─── Texture Handles ───
  setTextureHandle(layerId: LayerId, handle: TextureHandle): void {
    this.textureHandles.set(layerId, handle);
  }

  getTextureHandle(layerId: LayerId): TextureHandle | undefined {
    return this.textureHandles.get(layerId);
  }

  // ─── Render State ───
  /**
   * While a text edit overlay owns the visual for a layer, hide it from the
   * compositor so the raster and the overlay textarea never double-draw.
   * Caller clears with null when the edit session closes.
   */
  setRenderHiddenLayerId(layerId: LayerId | null): void {
    this.renderHiddenLayerId = layerId;
  }

  getRenderState(): RenderState {
    const renderLayers: RenderLayer[] = this.model.layers.map(l => {
      const handle = this.textureHandles.get(l.id) || { id: `tex-${l.id}` };
      return {
        id: l.id,
        textureHandle: handle,
        visible: l.visible && l.id !== this.renderHiddenLayerId,
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

  /**
   * Mark the document as clean (saved).
   *
   * @param baseline - Optional snapshot taken BEFORE the save operation.
   *   When provided, it is used as the saved baseline instead of the current
   *   model so that any edits made during an async save window are correctly
   *   detected as dirty rather than silently accepted as the saved state.
   */
  clearDirty(baseline?: DocumentModel): void {
    this.dirtyLayerIds.clear();
    // Use the caller-supplied pre-save snapshot as the saved baseline when
    // available; otherwise use the current model (for new / reopened docs).
    this.savedModel = baseline ? createSnapshot(baseline) : createSnapshot(this.model);
    // Register the baseline bitmaps too: a later replaceLayerBitmap would
    // otherwise close a bitmap the saved baseline still references. Dirty
    // detection only reference-compares today, but the "snapshotted bitmaps
    // survive replacement" invariant must hold for any future baseline use.
    this.retainBitmaps(this.savedModel);
    // If the current model differs from the baseline, edits happened during
    // the async save — keep the dirty flag so they aren't silently dropped.
    this.model.dirty = !DocumentEngine.modelsEqual(this.savedModel, this.model as DocumentModel);
  }

  // ─── Change Notification ───
  onChange(callback: () => void): void {
    this.onChangeCallback = callback;
  }

  onVisualChange(callback: () => void): void {
    this.onVisualChangeCallback = callback;
  }

  /** Detach both change callbacks — used when a document session is removed
   *  so a removed engine never fires into workspace state (review #40). */
  clearCallbacks(): void {
    this.onChangeCallback = null;
    this.onVisualChangeCallback = null;
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
  /**
   * Register every bitmap in a model/snapshot so replaceLayerBitmap never
   * closes one that a committed history entry or saved baseline still
   * references (@bug 2026-08-09 B5). WeakSet: stale entries are weak, so they
   * never pin an ImageBitmap after its entry is evicted.
   */
  private retainBitmaps(snap: DocumentModel): void {
    for (const layer of snap.layers) {
      if (layer.imageBitmap) this.snapshotRetainedBitmaps.add(layer.imageBitmap);
      if (layer.baseImageBitmap) this.snapshotRetainedBitmaps.add(layer.baseImageBitmap);
    }
  }

  snapshot(): DocumentModel {
    if (USE_RUST_SSOT && this.rustEngine) {
      try {
        const json = this.rustEngine.snapshot_json();
        return JSON.parse(json);
      } catch {}
    }
    // Register every live bitmap so replaceLayerBitmap never closes one a
    // committed snapshot still references.
    this.retainBitmaps(this.model);
    return createSnapshot(this.model);
  }

  restore(snapshot: DocumentModel, options?: { restoreViewport?: boolean }): void {
    if (USE_RUST_SSOT && this.rustEngine) {
      try {
        const json = JSON.stringify(snapshot);
        if (this.rustEngine.restore_snapshot(json)) {
          const layersJson = this.rustEngine.get_layers_json();
          const rustLayers = JSON.parse(layersJson);
          this.model.layers = rustLayers.map((l: any) => ({
            id: l.id, name: l.name, type: "raster" as const, visible: l.visible, opacity: l.opacity,
            locked: l.locked, blendMode: l.blendMode, transform: l.transform, width: l.width, height: l.height,
            imageBitmap: this.model.layers.find(x => x.id === l.id)?.imageBitmap ?? null, hasAdjustments: l.hasAdjustments ?? false,
          }));
          this.model.activeLayerId = this.rustEngine.get_active_layer_id() ?? null;
          this.model.dirty = true;
          this.notifyChange();
          return;
        }
      } catch {}
    }
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
    // changed (@regression 2026-07-05: "layer turns black on undo" because
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
        // Parametric layer rasterization (shapeLayerToRaster/textLayerToRaster)
        // flips `type` WITHOUT replacing the bitmap; without this check an undo
        // of a rasterize-after-save would falsely report the doc clean.
        x.type !== y.type ||
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
    return calcLayerMemory(this.model);
  }

  canAddLayer(width: number, height: number): boolean {
    return canFitLayer(this.model, width, height);
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
