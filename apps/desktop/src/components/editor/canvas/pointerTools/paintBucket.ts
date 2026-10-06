// SPDX-License-Identifier: AGPL-3.0-or-later
import { documentToLayerLocal } from "@/viewport/transformGeometry";
import { floodFill, type FillMask } from "@/features/fill/fillOperations";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { showToast } from "../../Toast";
import { ipcErrorMessage } from "@/tauri/native";
import { trySetPointerCapture } from "../../tools/pointerCapture";
import type { PointerToolContext } from "./pointerToolContext";
import { applyRustTilesToSurface, projectRustPixelsToVisibleSurface, rehydratePaintSurfaceFromRust } from "@/lib/rustShadow";
import { syncFacadeVersionFromPixel } from "@/lib/protocol/facadeRegistry";
import { pixelSeedDispatch, encodePixelBytes, decodeRustBytes } from "@/lib/protocol/pixelSeedCall";
import { selectionUploadRect } from "../keyboardShortcuts/selectionTool";
import { computeChangedRegion, computeDirtyRegion, reconstructLayerBuffer } from "@/lib/paint/regionProducer";
import { resolveRustPixelOperationArm } from "@/lib/paint/rustPixelOperationArm";

/**
 * Paint Bucket: click-to-fill. Runs flood fill on the active layer at the
 * clicked document point, honoring the selection mask and fill tolerance.
 * Returns true when the bucket tool handled the event.
 *
 * Rust records the fill: it writes the changed region through
 * `rust_pixels_write_region` (one canonical `Pixel` history entry) and drives
 * the derived TS PaintTileSurface + history memento from the Rust result.
 * A single user Fill yields exactly ONE user-visible undo step: the TS entry is
 * a cursor token marked `rustOwned`, and the undo/redo dispatch takes the pixels
 * from Rust while draining that token in lockstep.
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

  // UNCONDITIONAL: a Paint Bucket Fill is always recorded by Rust, so the
  // default state of photrez.rustPixels (key absent or "0") takes the same path
  // as "1". Gating on it sent the default state down the TS-only commit, which
  // recorded no Rust Pixel entry and therefore had no twin for the undo drain
  // to move in lockstep. The surface is resolved unconditionally now; a missing
  // one is a visible error, never a silent second pixel owner.
  const surface = engine.getPaintSurface(layerId);
  const arm = resolveRustPixelOperationArm("bucket", true, surface !== null);
  if (arm !== "legacy") {
    if (!surface) { showToast("Rust pixel surface not ready", "warn"); trySetPointerCapture(ctx.getCanvasRef(), e.pointerId); return true; }
    const docId = workspace.getActiveDocumentId() ?? "";
    // Kick off the async canonical fill; the pointer event is already handled
    // here. Fire-and-forget keeps this function synchronous so the dispatcher's
    // downstream tool routing (handlePointerDown) is unaffected.
    void (async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const { pixelInvoke } = await import("@/lib/protocol/pixelInvokeCensus");
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
          await invoke("rust_pixels_init", pixelSeedDispatch(docId, layerId, layer.width, layer.height, seedData));
        }
        // Ensure the derived surface reflects the CURRENT canonical state before
        // we overlay the fill (mirrors the brush's pre-commit rehydration).
        await rehydratePaintSurfaceFromRust(docId, layerId, surface);
        // Source current pixels from Rust so OVERLAPPING fills read the post-prior-fill
        // canonical buffer (not a stale TS bitmap).
        const tiles = decodeRustBytes<{ x: number; y: number; w: number; h: number; data: number[] }[]>(
          await invoke("rust_pixels_snapshot_layer", { docId, layerId }),
        );
        const buf = reconstructLayerBuffer(tiles, layer.width, layer.height);
        const before = new Uint8ClampedArray(buf); // snapshot pre-fill for diffing
        const imgData = new ImageData(buf as Uint8ClampedArray<ArrayBuffer>, layer.width, layer.height);
        floodFill(imgData, lx, ly, fillR, fillG, fillB, 255, fillTolerance(), fillMask ?? null, fillContiguous());
        const changed = computeChangedRegion(before, imgData.data, layer.width, layer.height);
        if (!changed) return;
        // The byte-diff oracle above stays the authority for WHAT changed; the
        // producer owns the region arithmetic. A non-inverted fill only touches
        // pixels inside the fill mask, so the mask bounds contain the changed
        // box and the intersection below is that same box - the shipped rgba
        // therefore always matches the region (rust_pixels_write_region
        // rejects any length mismatch).
        const region = computeDirtyRegion(changed, fillMask && !fillMask.inverted
          ? { x: fillMask.x, y: fillMask.y, w: fillMask.w, h: fillMask.h }
          : null);
        if (!region) return;
        const preSnapshot = engine.snapshot();
        const res = decodeRustBytes<{
          after: { x: number; y: number; w: number; h: number; data: ArrayLike<number> }[];
          epoch: number;
          version: number;
        }>(await pixelInvoke("rust_pixels_write_region", {
          docId,
          layerId,
          x: region.x,
          y: region.y,
          w: region.w,
          h: region.h,
          rgbaBase64: encodePixelBytes(changed.rgba),
        }));
        // TS derived cache updated from Rust's authoritative returned `after` tiles + epoch.
        applyRustTilesToSurface(surface.context, res.after);
        surface.pixelEpoch = res.epoch;
        surface.pixelVersion = res.version;
        syncFacadeVersionFromPixel(docId, res.version);
        renderer?.uploadSurfaceTiles?.(layerId, layer.width, layer.height, res.after.map(t => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })));
        // Rust is authoritative but it is not what the user looks at: the drawn
        // layer comes from layer.imageBitmap. Rebuild that raster from the surface
        // Rust's tiles were just applied to, so the fill is visible NOW instead of
        // only after an undo/redo. This also installs bitmapEpoch, and only once
        // the bitmap genuinely holds the post-fill pixels - setting the epoch
        // against an unchanged bitmap made ensureBitmapCurrent skip the rebuild
        // and left the pre-fill raster permanently on screen.
        await projectRustPixelsToVisibleSurface(engine, renderer, layerId, surface, res.epoch);
        // Imperative is entry-owned (tile-memento model): history stores it and
        // replays its before/after tiles on undo/redo; the Rust entry is synced
        // via `rust_pixels_undo` (single step, no second TS-visible entry).
        // `rustOwned` marks it a cursor token for a step Rust already holds, so
        // the undo/redo dispatch takes the pixels from Rust and drains this twin
        // in lockstep instead of replaying tiles no store holds.
        const imperative = {
          layerId,
          surfaceWidth: layer.width,
          surfaceHeight: layer.height,
          // The reply carries the POST-image only, so it has no pre-image to store.
          // This entry is a cursor token for a step Rust already holds: the undo/redo
          // dispatch takes its pixels from Rust and refuses to replay these tiles, so
          // the empty array below is what it already did with them.
          before: [],
          after: res.after.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
          rustOwned: true,
        };
        history.commit(preSnapshot, "Paint Bucket Fill", imperative, true);
        scheduler.requestRender();
      } catch (err) {
        showToast(`Fill failed: ${ipcErrorMessage(err)}`, "error");
      } finally {
        trySetPointerCapture(ctx.getCanvasRef(), e.pointerId);
      }
    })();
    return true;
  }

  // ── Legacy arm (canonical TS bitmap) ──
  // UNREACHABLE now that the Rust arm above is unconditional: `arm` is "rust"
  // whenever a paint surface exists and "blocked" when it does not, so
  // resolveRustPixelOperationArm never returns "legacy" for this call site.
  // Kept verbatim pending the retirement phase, which deletes it together with
  // the "legacy" member of RustPixelOperationArm.
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
    // Non-inverted selections bound the fill, so upload only that rect.
    // Inverted or absent selections can touch the whole layer: full upload.
    const dirty = selectionUploadRect(engine);
    if (dirty) renderer?.uploadImage(layerId, newBitmap, dirty);
    else renderer?.uploadImage(layerId, newBitmap);
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
