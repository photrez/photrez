import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";
import type { WebGL2Backend } from "@/renderer/webgl2";
import { compositeAllLayers } from "@/engine/layerComposite";
import { applyBasicAdjustmentToColor } from "@/engine/layerAdjustments";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import type { SelectionState } from "@/features/selection/SelectionTypes";
import type { LayerNode } from "@/engine/types";
import { applyRustTilesToSurface, rehydratePaintSurfaceFromRust } from "@/lib/rustShadow";
import { computeChangedRegion, reconstructLayerBuffer } from "@/components/editor/canvas/pointerTools/paintBucket";
import { showToast } from "../Toast";

export function mergeActiveLayerDown(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
  activeId: string,
) {
  const beforeLayers = engine.getLayers();
  const activeIndex = beforeLayers.findIndex((layer) => layer.id === activeId);
  const bottomLayer = activeIndex >= 0 ? beforeLayers[activeIndex + 1] : null;

  if (!bottomLayer) {
    return false;
  }

  history.commit(engine.snapshot(), "Merge Down");
  engine.mergeDown(activeId);
  renderer.destroyTexture(activeId);
  renderer.destroyTexture(bottomLayer.id);

  const mergedLayer = engine.getLayer(engine.getActiveLayerId() || "");
  if (!mergedLayer?.imageBitmap) {
    return false;
  }
  renderer.uploadImage(mergedLayer.id, mergedLayer.imageBitmap);

  return true;
}

export function mergeSelectedLayers(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
  layerIds: string[],
): boolean {
  if (!layerIds || layerIds.length < 2) {
    return false;
  }

  history.commit(engine.snapshot(), "Merge Selected Layers");
  engine.mergeSelectedLayers(layerIds);

  for (const id of layerIds) {
    renderer.destroyTexture(id);
  }

  const activeId = engine.getActiveLayerId();
  const mergedLayer = activeId ? engine.getLayer(activeId) : null;
  if (!mergedLayer?.imageBitmap) {
    return false;
  }
  renderer.uploadImage(mergedLayer.id, mergedLayer.imageBitmap);

  return true;
}

export function deleteMultipleLayers(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
  layerIds: string[],
): boolean {
  if (!layerIds || layerIds.length === 0) return false;
  const currentLayers = engine.getLayers();

  // Guard against deleting background or leaving zero layers
  const validIdsToDelete = layerIds.filter((id) => {
    const layer = currentLayers.find((l) => l.id === id);
    return layer && !layer.isBackground;
  });

  if (validIdsToDelete.length === 0) return false;

  // Cannot delete all layers — must preserve at least one
  if (currentLayers.length - validIdsToDelete.length < 1) {
    const canDeleteCount = currentLayers.length - 1;
    if (canDeleteCount <= 0) return false;
    validIdsToDelete.splice(canDeleteCount);
  }

  history.commit(
    engine.snapshot(),
    validIdsToDelete.length > 1 ? "Delete Layers" : "Delete Layer",
  );

  for (const id of validIdsToDelete) {
    engine.deleteLayer(id);
    renderer.destroyTexture(id);
  }

  return true;
}

export function duplicateMultipleLayers(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
  layerIds: string[],
): string[] {
  if (!layerIds || layerIds.length === 0) return [];
  const currentLayers = engine.getLayers();
  const validIds = layerIds.filter((id) => currentLayers.some((l) => l.id === id));
  if (validIds.length === 0) return [];

  history.commit(
    engine.snapshot(),
    validIds.length > 1 ? "Duplicate Layers" : "Duplicate Layer",
  );

  const newIds: string[] = [];
  for (const id of validIds) {
    try {
      const dup = engine.duplicateLayer(id);
      if (dup.imageBitmap) {
        renderer.uploadImage(dup.id, dup.imageBitmap);
      }
      newIds.push(dup.id);
    } catch {
      // Continue if resource limit reached on some
    }
  }

  return newIds;
}

export function flattenAllLayers(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
) {
  const oldLayerIds = engine.getLayers().map((layer) => layer.id);
  if (oldLayerIds.length <= 1) {
    return false;
  }

  history.commit(engine.snapshot(), "Flatten Image");
  engine.flattenLayers();

  for (const id of oldLayerIds) {
    renderer.destroyTexture(id);
  }

  const flattenedLayer = engine.getLayer(engine.getActiveLayerId() || "");
  if (!flattenedLayer?.imageBitmap) {
    return false;
  }
  renderer.uploadImage(flattenedLayer.id, flattenedLayer.imageBitmap);

  return true;
}

export function stampVisibleLayers(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
) {
  const layers = engine.getLayers();
  const visibleLayers = layers.filter((l) => l.visible);
  if (visibleLayers.length === 0) return false;

  history.commit(engine.snapshot(), "Stamp Visible");

  const w = engine.getWidth();
  const h = engine.getHeight();
  const composite = compositeAllLayers(visibleLayers, w, h);
  if (!composite) {
    return false;
  }

  const newLayer = engine.addLayer("Stamp Visible", w, h);
  engine.setLayerImageBitmap(newLayer.id, composite);
  renderer.uploadImage(newLayer.id, composite);

  return true;
}

/**
 * Fill the active layer with a solid color (Alt+Del / Ctrl+Del).
 * Replaces the entire layer content with an opaque `color` bitmap. Skips
 * locked layers and layers with no active id. Commits history BEFORE mutation
 * so the fill is undoable/redoable, then uploads the new bitmap to the renderer.
 *
 * C5.4 (Fill Layer): when the canonical Rust pixel owner is enabled
 * (localStorage "photrez.rustPixels" === "1") and the layer has a PaintTileSurface,
 * the fill writes through `rust_pixels_write_region` (one canonical `Pixel` history
 * entry) and drives the derived TS PaintTileSurface + history memento from the Rust
 * result — mirroring the brush/bucket. A single user Fill yields exactly ONE
 * user-visible undo step (the Rust history entry is subordinate to the TS
 * `history.commit` that drives undo/redo; there is no second step). Legacy path
 * (flag off, or no surface e.g. shape/text layers) is preserved unchanged.
 */
export function fillActiveLayerWithColor(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
  color: string,
): boolean {
  const activeId = engine.getActiveLayerId();
  if (!activeId) return false;

  const layer = engine.getLayer(activeId);
  if (!layer || layer.locked) return false;

  // A solid fill is a uniform color — apply the layer adjustment to the color
  // directly (O(1)) instead of baking every pixel on the CPU. This matches the
  // shader's applyAdjustment on the fill and then drops the adjustment param.
  const fillColor = layer.basicAdjustment
    ? applyBasicAdjustmentToColor(color, layer.basicAdjustment)
    : color;

  const sel = engine.getSelection();

  // C5.4 canonical-pixel path (flag matches the brush/bucket/undo gating).
  const rustPixelsFlag = (() => {
    try { return localStorage.getItem("photrez.rustPixels") === "1"; } catch { return false; }
  })();
  const surface = engine.getPaintSurface(activeId);
  if (rustPixelsFlag && surface) {
    const docId = engine.getId();
    // Fire-and-forget keeps the Alt+Del handler synchronous so it can
    // requestRender immediately; the canonical write + cache sync complete async.
    void (async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        // C5.4 ensure-if-absent: a Fill Layer can be the FIRST raster op on a
        // layer, so seed the canonical store from the current derived pixels when
        // Rust has no entry yet (mirrors the brush/bucket; never overwrites).
        let layerReady = true;
        try {
          await invoke("rust_pixels_get_epoch", { docId, layerId: activeId });
        } catch {
          layerReady = false;
        }
        if (!layerReady) {
          const seedData = surface.context.getImageData(0, 0, layer.width, layer.height).data;
          await invoke("rust_pixels_init", {
            docId,
            layerId: activeId,
            width: layer.width,
            height: layer.height,
            bytes: Array.from(seedData),
          });
        }
        // Ensure the derived surface reflects the CURRENT canonical state before
        // we overlay the fill (mirrors the brush/bucket pre-commit rehydration).
        await rehydratePaintSurfaceFromRust(docId, activeId, surface);
        // Source current pixels from Rust so OVERLAPPING fills read the post-prior-fill
        // canonical buffer (not a stale TS bitmap).
        const tiles = (await invoke("rust_pixels_snapshot_layer", { docId, layerId: activeId })) as
          { x: number; y: number; w: number; h: number; data: number[] }[];
        const before = reconstructLayerBuffer(tiles, layer.width, layer.height);
        // Build the filled result, preserving outside-selection pixels from `before`.
        const existing = await createImageBitmap(new ImageData(before as Uint8ClampedArray<ArrayBuffer>, layer.width, layer.height));
        const offscreen = buildFilledCanvas(layer, fillColor, sel, existing);
        if (!offscreen) return;
        const after = (offscreen.getContext("2d") as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D)
          .getImageData(0, 0, layer.width, layer.height).data as Uint8ClampedArray;
        const changed = computeChangedRegion(before, after, layer.width, layer.height);
        if (!changed) return;
        // Capture pre-fill state BEFORE clearing the adjustment so undo restores it.
        const preSnapshot = engine.snapshot();
        if (layer.basicAdjustment) engine.clearBasicAdjustments(activeId);
        const res = (await invoke("rust_pixels_write_region", {
          docId,
          layerId: activeId,
          x: changed.x,
          y: changed.y,
          w: changed.w,
          h: changed.h,
          rgba: Array.from(changed.rgba),
        })) as {
          before: { x: number; y: number; w: number; h: number; data: number[] }[];
          after: { x: number; y: number; w: number; h: number; data: number[] }[];
          epoch: number;
          version: number;
        };
        // TS derived cache updated from Rust's authoritative returned `after` tiles + epoch.
        applyRustTilesToSurface(surface.context, res.after);
        surface.pixelEpoch = res.epoch;
        surface.pixelVersion = res.version;
        renderer?.uploadSurfaceTiles?.(activeId, layer.width, layer.height, res.after.map(t => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })));
        // C5.4 bitmap sync: bitmap was set before Rust write (setLayerImageBitmap).
        // Now that write_region succeeded, bitmap and Rust are proven identical.
        const fillLayer = engine.getLayer(activeId);
        if (fillLayer) fillLayer.bitmapEpoch = res.epoch;
        // Imperative is entry-owned (tile-memento model): history stores it and
        // replays its before/after tiles on undo/redo; the Rust entry is synced
        // via `rust_pixels_undo` (single step, no second TS-visible entry).
        const imperative = {
          layerId: activeId,
          surfaceWidth: layer.width,
          surfaceHeight: layer.height,
          before: res.before.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
          after: res.after.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
        };
        history.commit(preSnapshot, "Fill Layer", imperative);
      } catch (err) {
        showToast(`Fill Layer failed: ${err instanceof Error ? err.message : "Unknown error"}`, "error");
      }
    })();
    return true;
  }

  // ── Legacy path (TS-authoritative bitmap) ──
  let bitmap: ImageBitmap | null = null;
  try {
    const offscreen = buildFilledCanvas(layer, fillColor, sel, engine.getLayerImageBitmap(activeId));
    if (!offscreen) return false;
    bitmap = (offscreen as OffscreenCanvas).transferToImageBitmap();
  } catch (err: unknown) {
    if (import.meta.env.DEV) console.error("Failed to fill layer with color:", err);
    return false;
  }
  if (!bitmap) return false;

  // Capture the pre-bake state so the fill's undo checkpoint restores to the
  // adjustment-still-applied state (not pre-adjustment) — keeping the
  // adjustment independently undoable from the fill.
  const preFillSnapshot = engine.snapshot();

  if (layer.basicAdjustment) engine.clearBasicAdjustments(activeId);

  // Commit pre-action snapshot BEFORE mutating so the fill is undoable/redoable.
  history.commit(preFillSnapshot, "Fill Layer");
  engine.setLayerImageBitmap(activeId, bitmap);
  renderer.uploadImage(activeId, bitmap);
  return true;
}

/** Build an OffscreenCanvas (or HTMLCanvas fallback) with the solid fill applied,
 *  preserving existing pixels outside the (optional) selection. Shared by the
 *  legacy and C5.4 Rust-canonical paths so fill semantics stay identical. */
function buildFilledCanvas(
  layer: LayerNode,
  fillColor: string,
  sel: SelectionState | null,
  existing: ImageBitmap | null,
): OffscreenCanvas | HTMLCanvasElement | null {
  const w = layer.width, h = layer.height;
  const offscreen = typeof OffscreenCanvas !== "undefined"
    ? new OffscreenCanvas(w, h)
    : (() => {
        const el = document.createElement("canvas");
        el.width = w;
        el.height = h;
        return el;
      })();
  const ctx = offscreen.getContext("2d") as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null;
  if (!ctx) return null;
  if (existing) ctx.drawImage(existing, 0, 0);
  applyFillToContext(ctx, layer, fillColor, sel);
  return offscreen;
}

/** Apply the solid fill (whole-layer or selection-scoped) onto an already-drawn
 *  2D context. Extracted from `fillActiveLayerWithColor` so the legacy and
 *  Rust-canonical paths share identical fill semantics. */
function applyFillToContext(
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  layer: LayerNode,
  fillColor: string,
  sel: SelectionState | null,
): void {
  const w = layer.width, h = layer.height;
  ctx.fillStyle = fillColor;
  if (sel) {
    // Selection is in document space; map it into layer-local pixel space so the
    // fill lands under the marquee even after the layer is resized/translated/rotated.
    const aabb = SelectionOperations.selectionToLayerAabb(sel, layer.transform, w, h);
    const sx = Math.round(aabb.x);
    const sy = Math.round(aabb.y);
    const sw = Math.max(0, Math.round(aabb.width));
    const sh = Math.max(0, Math.round(aabb.height));
    if (sel.inverted) {
      if (sel.shape === "ellipse") {
        // Inverted ellipse: fill everything EXCEPT the ellipse interior.
        const img = ctx.getImageData(0, 0, w, h);
        const hex = fillColor.replace("#", "");
        const r = parseInt(hex.slice(0, 2), 16);
        const g = parseInt(hex.slice(2, 4), 16);
        const b = parseInt(hex.slice(4, 6), 16);
        const localSel: SelectionState = { x: aabb.x, y: aabb.y, width: aabb.width, height: aabb.height, angle: 0, shape: "ellipse" };
        for (let py = 0; py < h; py++) {
          for (let px = 0; px < w; px++) {
            if (!SelectionOperations.isInsideEllipse(px, py, localSel)) {
              const idx = (py * w + px) * 4;
              img.data[idx] = r;
              img.data[idx + 1] = g;
              img.data[idx + 2] = b;
              img.data[idx + 3] = 255;
            }
          }
        }
        ctx.putImageData(img, 0, 0);
      } else {
        // Fill everything EXCEPT the (clamped) selected rect.
        const left = Math.max(0, Math.min(w, sx));
        const top = Math.max(0, Math.min(h, sy));
        const right = Math.max(0, Math.min(w, sx + sw));
        const bottom = Math.max(0, Math.min(h, sy + sh));
        ctx.fillRect(0, 0, w, top);
        ctx.fillRect(0, bottom, w, h - bottom);
        ctx.fillRect(0, top, left, bottom - top);
        ctx.fillRect(right, top, w - right, bottom - top);
      }
    } else {
      if (sel.shape === "ellipse") {
        // Non-inverted ellipse: fill only pixels INSIDE the ellipse.
        const img = ctx.getImageData(0, 0, w, h);
        const hex = fillColor.replace("#", "");
        const r = parseInt(hex.slice(0, 2), 16);
        const g = parseInt(hex.slice(2, 4), 16);
        const b = parseInt(hex.slice(4, 6), 16);
        const localSel: SelectionState = { x: aabb.x, y: aabb.y, width: aabb.width, height: aabb.height, angle: 0, shape: "ellipse" };
        for (let py = sy; py < sy + sh; py++) {
          for (let px = sx; px < sx + sw; px++) {
            if (SelectionOperations.isInsideEllipse(px, py, localSel)) {
              const idx = (py * w + px) * 4;
              img.data[idx] = r;
              img.data[idx + 1] = g;
              img.data[idx + 2] = b;
              img.data[idx + 3] = 255;
            }
          }
        }
        ctx.putImageData(img, 0, 0);
      } else {
        ctx.fillRect(sx, sy, sw, sh);
      }
    }
  } else {
    ctx.fillRect(0, 0, w, h);
  }
}
