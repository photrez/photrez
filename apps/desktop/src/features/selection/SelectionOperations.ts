import { SelectionState } from "./SelectionTypes";
import { DocumentEngine } from "../../engine/document";
import type { Transform2D } from "../../engine/types";
import { documentToLayerLocal } from "../../viewport/transformGeometry";

/**
 * Selection pixel operations (cut/copy/paste/delete) for the active layer.
 *
 * Selection state is read from `engine.getSelection()`. The clipboard is
 * a module-level in-memory buffer shared across the app — for MVP we do not
 * integrate with the system clipboard.
 */
export class SelectionOperations {
  private static clipboard: ImageData | null = null;

  static hasClipboard(): boolean {
    return SelectionOperations.clipboard !== null;
  }

  static getSelectionBounds(engine: DocumentEngine): SelectionState | null {
    const sel = engine.getSelection();
    if (!sel) return null;
    return { ...sel, angle: 0 };
  }

  /**
   * Map a document-space selection rect to its axis-aligned bounding box in
   * layer-local pixel space, accounting for the layer transform (position,
   * scale, rotation, flip). A selection is stored in document space, but pixel
   * operations write into the layer bitmap (layer-local space); without this
   * mapping, delete/fill/copy land in the wrong place once the layer is
   * transformed (resized/translated/rotated). An identity transform yields
   * the raw selection rect unchanged, so untransformed layers are unaffected.
   */
  static selectionToLayerAabb(
    sel: SelectionState,
    transform: Transform2D,
    layerW: number,
    layerH: number,
  ): { x: number; y: number; width: number; height: number } {
    const cx = sel.x + sel.width / 2;
    const cy = sel.y + sel.height / 2;
    const rad = (sel.angle * Math.PI) / 180;
    const cos = Math.cos(rad);
    const sin = Math.sin(rad);
    const corners = [
      { x: sel.x, y: sel.y },
      { x: sel.x + sel.width, y: sel.y },
      { x: sel.x + sel.width, y: sel.y + sel.height },
      { x: sel.x, y: sel.y + sel.height },
    ].map((c) => ({
      x: cx + (c.x - cx) * cos - (c.y - cy) * sin,
      y: cy + (c.x - cx) * sin + (c.y - cy) * cos,
    }));

    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const c of corners) {
      const lp = documentToLayerLocal(c.x, c.y, transform, layerW, layerH);
      if (lp.x < minX) minX = lp.x;
      if (lp.y < minY) minY = lp.y;
      if (lp.x > maxX) maxX = lp.x;
      if (lp.y > maxY) maxY = lp.y;
    }
    return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
  }

  /**
   * Copy pixels from the active layer within the selection bounds.
   *
   * Clamps source coordinates to the layer bounds so selections extending
   * beyond the canvas do not crash.  Also auto-trims fully-transparent
   * rows/columns from the edges so that empty areas of the selection are
   * not included in the clipboard.
   *
   * Returns the ImageData (also stored in module-level clipboard) or null
   * if no selection or no active layer.
   */
  static copySelection(engine: DocumentEngine): ImageData | null {
    const sel = engine.getSelection();
    if (!sel) return null;
    const activeId = engine.getActiveLayerId();
    if (!activeId) return null;

    const bitmap = engine.getLayerImageBitmap(activeId);
    if (!bitmap) return null;

    const layer = engine.getLayer(activeId);
    if (!layer) return null;

    const layerW = layer.width;
    const layerH = layer.height;

    // Selection is in document space; map it into layer-local pixel space
    // so the copy reads the pixels that are actually under the marquee.
    const aabb = SelectionOperations.selectionToLayerAabb(sel, layer.transform, layerW, layerH);

    let w: number, h: number, sx: number, sy: number;

    if (sel.inverted) {
      // Inverted selection: copy the full layer, then clear the excluded rect
      sx = 0;
      sy = 0;
      w = layerW;
      h = layerH;
    } else {
      // Clamp source rect to layer bounds to avoid Canvas drawImage crash
      // when the selection extends beyond the canvas.
      sx = Math.max(0, Math.round(aabb.x));
      sy = Math.max(0, Math.round(aabb.y));
      const se = Math.min(layerW, Math.round(aabb.x + aabb.width));
      const sb = Math.min(layerH, Math.round(aabb.y + aabb.height));
      w = Math.max(0, se - sx);
      h = Math.max(0, sb - sy);
    }

    if (w === 0 || h === 0) return null;

    const offscreen = new OffscreenCanvas(w, h);
    const ctx = offscreen.getContext("2d");
    if (!ctx) return null;

    try {
      if (sel.inverted) {
        ctx.drawImage(bitmap, 0, 0);
        if (sel.shape === "ellipse") {
          // Inverted ellipse: mask out the ellipse interior instead of a rectangle
          const img = ctx.getImageData(0, 0, w, h);
          const localSel: SelectionState = {
            x: aabb.x, y: aabb.y,
            width: aabb.width, height: aabb.height,
            angle: 0, shape: "ellipse",
          };
          const ax = Math.max(0, Math.round(aabb.x));
          const ay = Math.max(0, Math.round(aabb.y));
          const aw = Math.min(layerW - ax, Math.round(aabb.width));
          const ah = Math.min(layerH - ay, Math.round(aabb.height));
          for (let py = ay; py < ay + ah; py++) {
            for (let px = ax; px < ax + aw; px++) {
              if (SelectionOperations.isInsideEllipse(px, py, localSel)) {
                img.data[(py * w + px) * 4 + 3] = 0;
              }
            }
          }
          ctx.putImageData(img, 0, 0);
        } else {
          ctx.clearRect(Math.round(aabb.x), Math.round(aabb.y), Math.round(aabb.width), Math.round(aabb.height));
        }
      } else {
        ctx.drawImage(
          bitmap,
          sx, sy, w, h,
          0, 0, w, h,
        );
      }
      const data = ctx.getImageData(0, 0, w, h);
      // Auto-trim transparent pixels so empty areas are not included.
      // Skip for inverted selections: the excluded region may touch the
      // layer edge, causing trimTransparent to cut off visible content.
      // For ellipse marquees, trim would cut to the bounding box — instead
      // mask out pixels outside the ellipse so the copied region follows it.
      let trimmed = sel.inverted ? data : SelectionOperations.trimTransparent(data);
      if (sel.shape === "ellipse" && !sel.inverted) {
        // data is a fresh ImageData from getImageData, safe to mutate in place.
        SelectionOperations.maskOutsideEllipse(data, aabb, sel);
        trimmed = data;
      }
      SelectionOperations.clipboard = trimmed;
      return trimmed;
    } catch (err) {
      console.error("copySelection failed:", err);
      return null;
    }
  }

  /**
   * Remove fully-transparent rows/columns from the edges of ImageData.
   * If every pixel is transparent, returns the original data unchanged.
   */
  private static trimTransparent(imageData: ImageData): ImageData {
    const { width, height, data: pixels } = imageData;

    // Find bounding box of non-transparent pixels
    let top = height;
    let bottom = 0;
    let left = width;
    let right = 0;

    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        if (pixels[(y * width + x) * 4 + 3] > 0) {
          if (y < top) top = y;
          if (y > bottom) bottom = y;
          if (x < left) left = x;
          if (x > right) right = x;
        }
      }
    }

    // Fully transparent — nothing to trim
    if (top > bottom) {
      return imageData;
    }

    const trimmedW = right - left + 1;
    const trimmedH = bottom - top + 1;

    // Already minimal — no trimming needed
    if (trimmedW === width && trimmedH === height) {
      return imageData;
    }

    // Extract the trimmed region into a new buffer
    const trimmedData = new Uint8ClampedArray(trimmedW * trimmedH * 4);
    for (let y = 0; y < trimmedH; y++) {
      for (let x = 0; x < trimmedW; x++) {
        const srcIdx = ((top + y) * width + (left + x)) * 4;
        const dstIdx = (y * trimmedW + x) * 4;
        trimmedData[dstIdx] = pixels[srcIdx];
        trimmedData[dstIdx + 1] = pixels[srcIdx + 1];
        trimmedData[dstIdx + 2] = pixels[srcIdx + 2];
        trimmedData[dstIdx + 3] = pixels[srcIdx + 3];
      }
    }

    return {
      data: trimmedData,
      width: trimmedW,
      height: trimmedH,
      colorSpace: "srgb",
    } as ImageData;
  }

  /**
   * Cut = copy + clear selection pixels + clear selection state.
   */
  static cutSelection(engine: DocumentEngine): ImageData | null {
    const sel = engine.getSelection();
    if (!sel) {
      throw new Error("no selection");
    }
    const activeId = engine.getActiveLayerId();
    if (!activeId) {
      throw new Error("no active layer");
    }

    const copied = SelectionOperations.copySelection(engine);
    if (copied) {
      SelectionOperations.fillSelectionWithTransparent(engine);
    }
    engine.clearSelection();
    return copied;
  }

  /**
   * Delete = clear pixels in selection (set transparent) + clear selection state.
   * Does NOT copy to clipboard.
   */
  static deleteSelection(engine: DocumentEngine): void {
    const sel = engine.getSelection();
    if (!sel) {
      throw new Error("no selection");
    }
    const activeId = engine.getActiveLayerId();
    if (!activeId) {
      throw new Error("no active layer");
    }
    SelectionOperations.fillSelectionWithTransparent(engine);
    engine.clearSelection();
  }

  /**
   * Paste the in-memory clipboard as a new layer.
   * If no clipboard, no-op.
   */
  static pasteSelection(engine: DocumentEngine, data: ImageData | null = null): void {
    const payload = data ?? SelectionOperations.clipboard;
    if (!payload) return;
    const layer = engine.addLayer("Pasted Layer", payload.width, payload.height);

    const w = payload.width;
    const h = payload.height;
    const offscreen = new OffscreenCanvas(w, h);
    const ctx = offscreen.getContext("2d");
    if (!ctx) return;
    ctx.putImageData(payload, 0, 0);
    const bitmap = offscreen.transferToImageBitmap();
    engine.setLayerImageBitmap(layer.id, bitmap);
  }

  /**
   * Helper: fill selection bounds with transparent pixels on the active layer.
   *
   * The selection is stored in document space, but the layer bitmap lives in
   * layer-local pixel space. The selection rect is inverse-transformed into
   * layer-local space (via `selectionToLayerAabb`) so the cleared region
   * matches the marquee the user sees, even after the layer is resized,
   * translated, or rotated. With an identity transform the result equals the
   * raw selection rect.
   */
  /**
   * True if document-space point (x, y) is inside an ellipse marquee
   * defined by the selection bounds. Used when `sel.shape === "ellipse"`.
   * Pure function — tested separately.
   */
  static isInsideEllipse(x: number, y: number, sel: SelectionState): boolean {
    const cx = sel.x + sel.width / 2;
    const cy = sel.y + sel.height / 2;
    const rx = sel.width / 2;
    const ry = sel.height / 2;
    if (rx <= 0 || ry <= 0) return false;
    const nx = (x - cx) / rx;
    const ny = (y - cy) / ry;
    return nx * nx + ny * ny <= 1;
  }

  /**
   * Zero out alpha for every pixel OUTSIDE the ellipse marquee, within the
   * given layer-local AABB. Used for ellipse copy/delete so the result
   * follows the ellipse, not its bounding box.
   */
  private static maskOutsideEllipse(
    data: ImageData,
    aabb: { x: number; y: number; width: number; height: number },
    sel: SelectionState,
  ): void {
    const left = Math.round(aabb.x);
    const top = Math.round(aabb.y);
    const w = data.width;
    for (let py = 0; py < data.height; py++) {
      for (let px = 0; px < w; px++) {
        const docX = left + px;
        const docY = top + py;
        if (!SelectionOperations.isInsideEllipse(docX, docY, sel)) {
          data.data[(py * w + px) * 4 + 3] = 0;
        }
      }
    }
  }

  private static fillSelectionWithTransparent(engine: DocumentEngine): void {
    const sel = engine.getSelection();
    if (!sel) return;
    const activeId = engine.getActiveLayerId();
    if (!activeId) return;

    const layer = engine.getLayer(activeId);
    if (!layer) return;
    const bitmap = layer.imageBitmap;
    if (!bitmap) return;

    // Use the layer's own bitmap size — the layer is the unit of truth.
    const layerW = layer.width;
    const layerH = layer.height;

    const aabb = SelectionOperations.selectionToLayerAabb(sel, layer.transform, layerW, layerH);
    const sx = Math.round(aabb.x);
    const sy = Math.round(aabb.y);
    const w = Math.max(0, Math.round(aabb.width));
    const h = Math.max(0, Math.round(aabb.height));
    if (w === 0 || h === 0) return;

    const offscreen = new OffscreenCanvas(layerW, layerH);
    const ctx = offscreen.getContext("2d");
    if (!ctx) return;

    // Copy the entire layer bitmap, then clear either the selected rectangle
    // or, for an inverted selection, the four bands outside the excluded rect.
    ctx.drawImage(bitmap, 0, 0);
    if (sel.inverted) {
      const left = Math.max(0, Math.min(layerW, sx));
      const top = Math.max(0, Math.min(layerH, sy));
      const right = Math.max(0, Math.min(layerW, sx + w));
      const bottom = Math.max(0, Math.min(layerH, sy + h));
      ctx.clearRect(0, 0, layerW, top);
      ctx.clearRect(0, bottom, layerW, layerH - bottom);
      ctx.clearRect(0, top, left, bottom - top);
      ctx.clearRect(right, top, layerW - right, bottom - top);
      // Inverted ellipse: also clear the corners inside AABB but outside the ellipse
      if (sel.shape === "ellipse") {
        const img = ctx.getImageData(sx, sy, w, h);
        const localSel: SelectionState = {
          x: aabb.x, y: aabb.y,
          width: aabb.width, height: aabb.height,
          angle: 0, shape: "ellipse",
        };
        for (let py = 0; py < h; py++) {
          for (let px = 0; px < w; px++) {
            const docX = sx + px;
            const docY = sy + py;
            if (!SelectionOperations.isInsideEllipse(docX, docY, localSel)) {
              img.data[(py * w + px) * 4 + 3] = 0;
            }
          }
        }
        ctx.putImageData(img, sx, sy);
      }
    } else {
      if (sel.shape === "ellipse") {
        // Clear only pixels inside the ellipse marquee (layer-local AABB space),
        // not the whole bounding box.
        const img = ctx.getImageData(0, 0, layerW, layerH);
        const localSel: SelectionState = {
          x: aabb.x,
          y: aabb.y,
          width: aabb.width,
          height: aabb.height,
          angle: 0,
          shape: "ellipse",
        };
        for (let py = sy; py < sy + h; py++) {
          for (let px = sx; px < sx + w; px++) {
            if (SelectionOperations.isInsideEllipse(px, py, localSel)) {
              img.data[(py * layerW + px) * 4 + 3] = 0;
            }
          }
        }
        ctx.putImageData(img, 0, 0);
      } else {
        ctx.clearRect(sx, sy, w, h);
      }
    }

    const newBitmap = offscreen.transferToImageBitmap();
    engine.setLayerImageBitmap(layer.id, newBitmap);
  }

  /**
   * Test-only: clear the in-memory clipboard.
   */
  static __resetClipboard(): void {
    SelectionOperations.clipboard = null;
  }
}
