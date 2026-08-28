// SPDX-License-Identifier: AGPL-3.0-or-later
import { documentToLayerLocal } from "@/viewport/transformGeometry";
import { floodFill, type FillMask } from "@/features/fill/fillOperations";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { showToast } from "../../Toast";
import { trySetPointerCapture } from "../../tools/pointerCapture";
import type { PointerToolContext } from "./pointerToolContext";
import { applyRustTilesToSurface, rehydratePaintSurfaceFromRust } from "@/lib/rustShadow";

/**
 * Paint Bucket: click-to-fill. Runs flood fill on the active layer at the
 * clicked document point, honoring the selection mask and fill tolerance.
 * Returns true when the bucket tool handled the event.
 *
 * C5.4 (pilot): when the canonical Rust pixel owner is enabled
 * (localStorage "photrez.rustPixels" === "1"), the fill writes the changed
 * region through `rust_pixels_write_region` (one canonical `Pixel` history
 * entry) and drives the derived TS PaintTileSurface + history memento from the
 * Rust result — mirroring the brush. A single user Fill yields exactly ONE
 * user-visible undo step (the Rust history entry is subordinate to the TS
 * `history.commit` that drives undo/redo; there is no second step).
 */
export function applyPaintBucketFill(
  ctx: PointerToolContext,
  e: PointerEvent,
): boolean {
  const { editor } = ctx;
  const { workspace, renderer, scheduler, fgColor, fillTolerance, fillContiguous } = editor;

  if (editor.activeTool() !== "paintBucket") return false;

  const engine = workspace.getActiveEngine();
  const history = workspace.getActiveHistory();
  if (!engine || !history) return true;

  const layerId = engine.getActiveLayerId();
  if (!layerId) { showToast("No editable layer selected", "warn"); trySetPointerCapture(ctx.getCanvasRef(), e.pointerId); return true; }
  const layer = engine.getLayer(layerId);
  if (!layer || layer.locked) { showToast("Layer is locked", "warn"); trySetPointerCapture(ctx.getCanvasRef(), e.pointerId); return true; }
  if (!layer.visible) { showToast("Layer is hidden", "warn"); trySetPointerCapture(ctx.getCanvasRef(), e.pointerId); return true; }
  if (layer.lockTransparency) { showToast("Transparent pixels protected", "warn"); trySetPointerCapture(ctx.getCanvasRef(), e.pointerId); return true; }

  const coords = ctx.getDocCoords(e);

  // Layer-local click coords (accounts for scale/rotation/flip, not just translation)
  const localPt = documentToLayerLocal(coords.x, coords.y, layer.transform, layer.width, layer.height);
  const lx = Math.floor(localPt.x);
  const ly = Math.floor(localPt.y);

  // Fill colour from foreground
  const hex = fgColor().replace("#", "");
  const fillR = parseInt(hex.slice(0, 2), 16);
  const fillG = parseInt(hex.slice(2, 4), 16);
  const fillB = parseInt(hex.slice(4, 6), 16);

  // Build selection mask
  const sel = engine.getSelection();
  let fillMask: FillMask | undefined;
  if (sel) {
    const aabb = SelectionOperations.selectionToLayerAabb(sel, layer.transform, layer.width, layer.height);
    fillMask = {
      x: Math.round(aabb.x), y: Math.round(aabb.y),
      w: Math.max(0, Math.round(aabb.width)), h: Math.max(0, Math.round(aabb.height)),
      shape: sel.shape, inverted: sel.inverted,
    };
  }

  // C5.4 canonical-pixel path (flag matches the brush + undo gating).
  const rustPixelsFlag = (() => {
    try { return localStorage.getItem("photrez.rustPixels") === "1"; } catch { return false; }
  })();
  if (rustPixelsFlag) {
    const surface = engine.getPaintSurface(layerId);
    if (!surface) { showToast("Rust pixel surface not ready", "warn"); trySetPointerCapture(ctx.getCanvasRef(), e.pointerId); return true; }
    const docId = workspace.getActiveDocumentId() ?? "";
    // Kick off the async canonical fill; the pointer event is already handled
    // here. Fire-and-forget keeps this function synchronous so the dispatcher's
    // downstream tool routing (handlePointerDown) is unaffected.
    void (async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        // C5.4 ensure-if-absent: the canonical store only holds an entry for this
        // layer once a prior Rust pixel op seeded it (the brush seeds inside its
        // commit). A Paint Bucket Fill can be the FIRST raster op on a layer, so
        // seed from the current derived pixels when Rust has no entry yet.
        let layerReady = true;
        try {
          await invoke("rust_pixels_get_epoch", { docId, layerId });
        } catch {
          layerReady = false;
        }
        if (!layerReady) {
          const seedData = surface.context.getImageData(0, 0, layer.width, layer.height).data;
          await invoke("rust_pixels_init", {
            docId,
            layerId,
            width: layer.width,
            height: layer.height,
            bytes: Array.from(seedData),
          });
        }
        // Ensure the derived surface reflects the CURRENT canonical state before
        // we overlay the fill (mirrors the brush's pre-commit rehydration).
        await rehydratePaintSurfaceFromRust(docId, layerId, surface);
        // Source current pixels from Rust so OVERLAPPING fills read the post-prior-fill
        // canonical buffer (not a stale TS bitmap).
        const tiles = (await invoke("rust_pixels_snapshot_layer", { docId, layerId })) as
          { x: number; y: number; w: number; h: number; data: number[] }[];
        const buf = reconstructLayerBuffer(tiles, layer.width, layer.height);
        const before = new Uint8ClampedArray(buf); // snapshot pre-fill for diffing
        const imgData = new ImageData(buf as Uint8ClampedArray<ArrayBuffer>, layer.width, layer.height);
        floodFill(imgData, lx, ly, fillR, fillG, fillB, 255, fillTolerance(), fillMask ?? null, fillContiguous());
        const changed = computeChangedRegion(before, imgData.data, layer.width, layer.height);
        if (!changed) return;
        const preSnapshot = engine.snapshot();
        const res = (await invoke("rust_pixels_write_region", {
          docId,
          layerId,
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
        renderer?.uploadSurfaceTiles?.(layerId, layer.width, layer.height, res.after.map(t => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })));
        // C5.4 bitmap sync: bitmap was set before Rust write (setLayerImageBitmap).
        // Now that write_region succeeded, bitmap and Rust are proven identical.
        const bucketLayer = engine.getLayer(layerId);
        if (bucketLayer) bucketLayer.bitmapEpoch = res.epoch;
        // Imperative is entry-owned (tile-memento model): history stores it and
        // replays its before/after tiles on undo/redo; the Rust entry is synced
        // via `rust_pixels_undo` (single step, no second TS-visible entry).
        const imperative = {
          layerId,
          surfaceWidth: layer.width,
          surfaceHeight: layer.height,
          before: res.before.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
          after: res.after.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
        };
        history.commit(preSnapshot, "Paint Bucket Fill", imperative);
        scheduler.requestRender();
      } catch (err) {
        showToast(`Fill failed: ${err instanceof Error ? err.message : 'Unknown error'}`, "error");
      } finally {
        trySetPointerCapture(ctx.getCanvasRef(), e.pointerId);
      }
    })();
    return true;
  }

  // ── Legacy path (canonical TS bitmap) ──
  const bitmap = engine.getLayerImageBitmap(layerId);
  if (!bitmap) { showToast("Layer has no image data", "warn"); return true; }

  const offscreen = typeof OffscreenCanvas !== "undefined"
    ? new OffscreenCanvas(layer.width, layer.height)
    : (() => {
        const el = document.createElement("canvas");
        el.width = layer.width;
        el.height = layer.height;
        return el;
      })();
  const ctx2d = offscreen.getContext("2d") as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null;
  if (!ctx2d) return true;
  ctx2d.drawImage(bitmap, 0, 0);
  const imgData = ctx2d.getImageData(0, 0, layer.width, layer.height);

  floodFill(imgData, lx, ly, fillR, fillG, fillB, 255, fillTolerance(), fillMask ?? null, fillContiguous());

  const preSnapshot = engine.snapshot();
  ctx2d.putImageData(imgData, 0, 0);
  // OffscreenCanvas → true ImageBitmap. HTMLCanvasElement fallback (only
  // reachable when OffscreenCanvas is unavailable) is structurally compatible
  // (width/height + drawImage source) but not typed as ImageBitmap; add a
  // no-op close() so history/export resource-release calls don't throw.
  const newBitmap = typeof OffscreenCanvas !== "undefined" && offscreen instanceof OffscreenCanvas
    ? offscreen.transferToImageBitmap()
    : (() => {
        const html = offscreen as HTMLCanvasElement & { close?: () => void };
        html.close = () => {};
        return html as unknown as ImageBitmap;
      })();
  try {
    engine.setLayerImageBitmap(layerId, newBitmap);
    renderer?.uploadImage(layerId, newBitmap);
  } catch (err) {
    showToast(`Fill failed: ${err instanceof Error ? err.message : 'Unknown error'}`, "error");
    trySetPointerCapture(ctx.getCanvasRef(), e.pointerId);
    return true;
  }
  history.commit(preSnapshot, "Paint Bucket Fill");
  scheduler.requestRender();

  trySetPointerCapture(ctx.getCanvasRef(), e.pointerId);
  return true;
}

/** Rebuild a full layer RGBA buffer (width*height*4) from 256-grid Rust tiles.
 *  Tile `data` arrives as a plain number[] (JSON), so copy by index. */
export function reconstructLayerBuffer(
  tiles: { x: number; y: number; w: number; h: number; data: number[] }[],
  width: number,
  height: number,
): Uint8ClampedArray {
  const buf = new Uint8ClampedArray(width * height * 4);
  for (const t of tiles) {
    const rowBytes = t.w * 4;
    for (let row = 0; row < t.h; row++) {
      const srcOff = rowBytes * row;
      const dstOff = ((t.y + row) * width + t.x) * 4;
      for (let i = 0; i < rowBytes; i++) {
        buf[dstOff + i] = t.data[srcOff + i];
      }
    }
  }
  return buf;
}

/**
 * Pure helper: find the bounding box of pixels that differ between `before`
 * and `after` (both width*height*4 RGBA) and extract the `after` sub-region.
 * Returns null when nothing changed. The filled region is what gets pushed to
 * Rust as one canonical write (localized, not the whole layer).
 */
export function computeChangedRegion(
  before: Uint8ClampedArray,
  after: Uint8ClampedArray,
  width: number,
  height: number,
): { x: number; y: number; w: number; h: number; rgba: Uint8ClampedArray } | null {
  let minX = width, minY = height, maxX = -1, maxY = -1;
  const n = width * height * 4;
  for (let i = 0; i < n; i += 4) {
    if (
      before[i] !== after[i] ||
      before[i + 1] !== after[i + 1] ||
      before[i + 2] !== after[i + 2] ||
      before[i + 3] !== after[i + 3]
    ) {
      const p = i / 4;
      const px = p % width;
      const py = (p / width) | 0;
      if (px < minX) minX = px;
      if (py < minY) minY = py;
      if (px > maxX) maxX = px;
      if (py > maxY) maxY = py;
    }
  }
  if (maxX < 0) return null;
  const x = minX, y = minY, w = maxX - minX + 1, h = maxY - minY + 1;
  const rgba = new Uint8ClampedArray(w * h * 4);
  for (let row = 0; row < h; row++) {
    const src = ((y + row) * width + x) * 4;
    const dst = row * w * 4;
    rgba.set(after.subarray(src, src + w * 4), dst);
  }
  return { x, y, w, h, rgba };
}
