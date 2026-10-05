import { SelectionState } from "./SelectionTypes";
import { DocumentEngine } from "../../engine/document";
import type { CommandHistory } from "../../engine/history";
import type { WebGL2Backend } from "@/renderer/webgl2";
import {
  commitFacadeClearSelection,
  mirrorSelectionCommand,
  syncFacadeVersionFromPixel,
} from "@/lib/protocol/facadeRegistry";
import {
  applyRustTilesToSurface,
  projectRustPixelsToVisibleSurface,
} from "@/lib/rustShadow";
import { computeChangedRegion, reconstructLayerBuffer } from "@/lib/paint/regionProducer";
import { showToast } from "@/components/editor/Toast";
import { ipcErrorMessage } from "@/tauri/native";
import type { Transform2D } from "../../engine/types";
import type { LayerNode } from "../../engine/types";
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
    const ctx = offscreen.getContext("2d", { willReadFrequently: true });
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

    // Extract the trimmed region into a new buffer. Rows are copied via
    // TypedArray.set (memcpy path) — measured 2.8x faster than per-channel
    // indexing at 4K (77.9 -> 27.9ms, byte-identical; see
    // apps/desktop/scripts/bench-r2-microfix.ts).
    const trimmedData = new Uint8ClampedArray(trimmedW * trimmedH * 4);
    for (let y = 0; y < trimmedH; y++) {
      const src = ((top + y) * width + left) * 4;
      trimmedData.set(pixels.subarray(src, src + trimmedW * 4), y * trimmedW * 4);
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
   *
   * The clear is recorded by Rust as ONE canonical `Pixel` entry (see
   * {@link recordSelectionClearRust}), exactly like Fill Layer: the TS history
   * entry is a cursor token marked `rustOwned`, so one user Cut is one undo step
   * whose pixels come from Rust. Writing the clear only to `layer.imageBitmap`
   * left the store holding the PRE-delete bytes, and the next stroke rehydrated
   * the surface from those bytes - deleted pixels came back.
   */
  static cutSelection(
    engine: DocumentEngine,
    history: CommandHistory,
    renderer?: WebGL2Backend | null,
  ): ImageData | null {
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
      void SelectionOperations.recordSelectionClearRust(engine, history, renderer, "Cut", sel);
    }
    engine.clearSelection();
    mirrorSelectionCommand(engine, () => commitFacadeClearSelection(engine as never));
    return copied;
  }

  /**
   * Delete = clear pixels in selection (set transparent) + clear selection state.
   * Does NOT copy to clipboard. Recorded by Rust, same contract as {@link cutSelection}.
   */
  static deleteSelection(
    engine: DocumentEngine,
    history: CommandHistory,
    renderer?: WebGL2Backend | null,
  ): void {
    const sel = engine.getSelection();
    if (!sel) {
      throw new Error("no selection");
    }
    const activeId = engine.getActiveLayerId();
    if (!activeId) {
      throw new Error("no active layer");
    }
    // PARAMETRIC LAYERS ARE REFUSED, NOT DELETED FROM. A shape or text layer's
    // `shapeParams` / `textData` is its document state and `imageBitmap` is a
    // cache re-derived from it on every edit, so a parametric layer is not a
    // pixel owner and a byte-clear cannot be absorbed into one: the next
    // `updateShapeParams` / `updateTextData` would re-derive the raster from the
    // unchanged params and silently restore the pixels the user just deleted.
    // This is the same desync the canvas pointer guard refuses for brush,
    // eraser, bucket and gradient; the selection tool is simply not in that
    // dispatcher's tool list, so it needs the check here. Refuse before any
    // store seeding or write.
    if (engine.isShapeLayer(activeId) || engine.isTextLayer(activeId)) {
      showToast("Cannot delete pixels from a shape or text layer", "warn");
      return;
    }
    void SelectionOperations.recordSelectionClearRust(engine, history, renderer, "Delete Pixels", sel);
    engine.clearSelection();
    mirrorSelectionCommand(engine, () => commitFacadeClearSelection(engine as never));
  }

  /**
   * Record the selection clear as one canonical Rust pixel write over the
   * byte-diff region, and make it visible + undoable as a single step.
   *
   * `before` is read back from the Rust store, never from `layer.imageBitmap`:
   * the bitmap can lag the store (a paint stroke updates the store first), and
   * diffing against a stale bitmap would ship bytes the store never held. The
   * store-currency invariant in lib/paint/storeCurrency.ts is what keeps the
   * two from diverging in the first place.
   *
   * `sel` is captured by the caller at gesture time: this runs after several
   * awaits, by which point the caller has already cleared the selection the
   * marquee maps from. Reading it here would silently clear nothing.
   */
  private static async recordSelectionClearRust(
    engine: DocumentEngine,
    history: CommandHistory,
    renderer: WebGL2Backend | null | undefined,
    label: string,
    sel: SelectionState,
  ): Promise<void> {
    const activeId = engine.getActiveLayerId();
    const layerId = activeId ?? "";
    const layer = activeId ? engine.getLayer(activeId) : null;
    if (!layer || !layer.imageBitmap || layer.width <= 0 || layer.height <= 0) return;
    const docId = engine.getId();
    // No document id means no canonical pixel namespace to write into; leaving
    // the bitmap untouched keeps the layer self-consistent.
    if (!docId) return;
    const { width, height } = layer;
    const surface = engine.getPaintSurface(layerId);
    if (!surface) {
      // No raster to clear from: nothing to record, and silently writing through
      // a second pixel owner would be worse than doing nothing.
      showToast("Rust pixel surface not ready", "warn");
      return;
    }
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const { pixelInvoke } = await import("@/lib/protocol/pixelInvokeCensus");
      // Ensure-if-absent, mirroring the brush/bucket/fill arms: a delete can be
      // the FIRST raster op on a layer, and there is no store to write into yet.
      let layerReady = true;
      try {
        await invoke("rust_pixels_get_epoch", { docId, layerId });
      } catch {
        layerReady = false;
      }
      if (!layerReady) {
        const seed = surface.context.getImageData(0, 0, width, height).data;
        await invoke("rust_pixels_init", {
          docId,
          layerId,
          width,
          height,
          bytes: new Uint8Array(seed.buffer, seed.byteOffset, seed.byteLength),
        });
      }
      // Canonical pre-image straight from the store.
      const tiles = (await invoke("rust_pixels_snapshot_layer", { docId, layerId })) as
        { x: number; y: number; w: number; h: number; data: number[] }[];
      const before = reconstructLayerBuffer(tiles ?? [], width, height);
      // Copy before clearing: `clearSelectionPixelsFrom` works in place, and
      // diffing a buffer against itself would report "nothing changed" and drop
      // the delete entirely.
      const cleared = SelectionOperations.clearSelectionPixelsFrom(
        layer, sel, before.slice(), width, height,
      );
      if (!cleared) return;
      const changed = computeChangedRegion(before, cleared, width, height);
      if (!changed) return;
      // Capture the pre-delete state BEFORE the write so undo restores it.
      const preSnapshot = engine.snapshot();
      const res = (await pixelInvoke("rust_pixels_write_region", {
        docId,
        layerId,
        x: changed.x,
        y: changed.y,
        w: changed.w,
        h: changed.h,
        rgba: new Uint8Array(changed.rgba.buffer, changed.rgba.byteOffset, changed.rgba.byteLength),
      })) as {
        before: { x: number; y: number; w: number; h: number; data: number[] }[];
        after: { x: number; y: number; w: number; h: number; data: number[] }[];
        epoch: number;
        version: number;
      };
      applyRustTilesToSurface(surface.context, res.after);
      surface.pixelEpoch = res.epoch;
      surface.pixelVersion = res.version;
      syncFacadeVersionFromPixel(docId, res.version);
      renderer?.uploadSurfaceTiles?.(
        layerId,
        width,
        height,
        res.after.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
      );
      // The drawn layer comes from layer.imageBitmap, so rebuild that raster
      // from the surface Rust's tiles were just applied to; this also installs
      // bitmapEpoch, and only once the bitmap genuinely holds the post-delete
      // pixels (claiming it earlier permanently pinned the pre-delete raster).
      await projectRustPixelsToVisibleSurface(engine, renderer, layerId, surface, res.epoch);
      history.commit(
        preSnapshot,
        label,
        {
          layerId,
          surfaceWidth: width,
          surfaceHeight: height,
          before: res.before.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
          after: res.after.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
          rustOwned: true,
        },
        true,
      );
    } catch (err) {
      showToast(`${label} failed: ${ipcErrorMessage(err)}`, "error");
    }
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

  /**
   * Clear the selected pixels out of a full-layer RGBA buffer, in place.
   * Pure over `source` (row-major `layerW*layerH*4`), which the caller supplies
   * as the CANONICAL pre-image read back from the Rust store, never
   * `layer.imageBitmap` - a bitmap that lags the store would otherwise produce
   * a diff the store never held. `sel` and `layer` are captured at gesture time
   * because the caller clears the selection before this async work runs.
   *
   * Clears either the selected rectangle or, for an inverted selection, the four
   * bands outside the excluded rect; an ellipse marquee clears only pixels
   * inside the ellipse, not its bounding box.
   */
  private static clearSelectionPixelsFrom(
    layer: LayerNode,
    sel: SelectionState,
    source: Uint8ClampedArray,
    layerW: number,
    layerH: number,
  ): Uint8ClampedArray | null {
    const aabb = SelectionOperations.selectionToLayerAabb(sel, layer.transform, layerW, layerH);
    const sx = Math.round(aabb.x);
    const sy = Math.round(aabb.y);
    const w = Math.max(0, Math.round(aabb.width));
    const h = Math.max(0, Math.round(aabb.height));
    if (w === 0 || h === 0) return null;

    const clearAt = (x: number, y: number) => {
      if (x < 0 || y < 0 || x >= layerW || y >= layerH) return;
      const idx = (y * layerW + x) * 4 + 3;
      source[idx] = 0;
    };

    if (sel.inverted) {
      const left = Math.max(0, Math.min(layerW, sx));
      const top = Math.max(0, Math.min(layerH, sy));
      const right = Math.max(0, Math.min(layerW, sx + w));
      const bottom = Math.max(0, Math.min(layerH, sy + h));
      for (let y = 0; y < top; y++) for (let x = 0; x < layerW; x++) clearAt(x, y);
      for (let y = bottom; y < layerH; y++) for (let x = 0; x < layerW; x++) clearAt(x, y);
      for (let y = top; y < bottom; y++) {
        for (let x = 0; x < left; x++) clearAt(x, y);
        for (let x = right; x < layerW; x++) clearAt(x, y);
      }
      // Inverted ellipse: also clear the corners inside AABB but outside the ellipse.
      if (sel.shape === "ellipse") {
        const localSel: SelectionState = {
          x: aabb.x, y: aabb.y, width: aabb.width, height: aabb.height,
          angle: 0, shape: "ellipse",
        };
        // px/py are already layer-local absolutes here, so the ellipse test
        // must not add the AABB origin a second time.
        for (let py = sy; py < sy + h; py++) {
          for (let px = sx; px < sx + w; px++) {
            if (!SelectionOperations.isInsideEllipse(px, py, localSel)) clearAt(px, py);
          }
        }
      }
    } else if (sel.shape === "ellipse") {
      const localSel: SelectionState = {
        x: aabb.x, y: aabb.y, width: aabb.width, height: aabb.height,
        angle: 0, shape: "ellipse",
      };
      for (let py = sy; py < sy + h; py++) {
        for (let px = sx; px < sx + w; px++) {
          if (SelectionOperations.isInsideEllipse(px, py, localSel)) clearAt(px, py);
        }
      }
    } else {
      for (let py = sy; py < sy + h; py++) for (let px = sx; px < sx + w; px++) clearAt(px, py);
    }
    return source;
  }

  /**
   * Test-only: clear the in-memory clipboard.
   */
  static __resetClipboard(): void {
    SelectionOperations.clipboard = null;
  }
}
