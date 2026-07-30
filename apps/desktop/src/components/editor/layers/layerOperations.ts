import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory } from "@/engine/history";
import type { WebGL2Backend } from "@/renderer/webgl2";
import { compositeAllLayers } from "@/engine/layerComposite";
import { applyBasicAdjustmentToColor } from "@/engine/layerAdjustments";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import type { SelectionState } from "@/features/selection/SelectionTypes";

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

  const w = layer.width;
  const h = layer.height;

  // When a selection is active, the fill is scoped to the selection bounds
  // (matching how similar editors fill only the selected region). Without a
  // selection the entire layer is filled, identical to the prior behavior.
  const sel = engine.getSelection();

  let bitmap: ImageBitmap | null = null;
  try {
    if (typeof OffscreenCanvas !== "undefined") {
      const offscreen = new OffscreenCanvas(w, h);
      const ctx = offscreen.getContext("2d");
      if (ctx) {
        // Preserve existing layer content outside the filled region.
        const existing = engine.getLayerImageBitmap(activeId);
        if (existing) ctx.drawImage(existing, 0, 0);

        ctx.fillStyle = fillColor;
        if (sel) {
          // Selection is in document space; map it into layer-local pixel
          // space so the fill lands under the marquee even after the layer
          // is resized/translated/rotated. Identity transform ⇒ unchanged.
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
              const localSel: SelectionState = {
                x: aabb.x, y: aabb.y,
                width: aabb.width, height: aabb.height,
                angle: 0, shape: "ellipse",
              };
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
              const localSel: SelectionState = {
                x: aabb.x, y: aabb.y,
                width: aabb.width, height: aabb.height,
                angle: 0, shape: "ellipse",
              };
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
        bitmap = offscreen.transferToImageBitmap();
      }
    }
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
