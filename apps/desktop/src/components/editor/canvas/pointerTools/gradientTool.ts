// SPDX-License-Identifier: AGPL-3.0-or-later
import { documentToLayerLocal } from "@/viewport/transformGeometry";
import { gradientFill, type FillMask, type ColorStop } from "@/features/fill/fillOperations";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { showToast } from "../../Toast";
import { trySetPointerCapture } from "../../tools/pointerCapture";
import type { GradientDragState, PointerToolContext } from "./pointerToolContext";
import { applyRustTilesToSurface, getRustEpoch, projectRustPixelsToVisibleSurface, rehydratePaintSurfaceFromRust } from "@/lib/rustShadow";
import { syncFacadeVersionFromPixel } from "@/lib/protocol/facadeRegistry";
import { pixelSeedDispatch, encodePixelBytes, decodeRustBytes } from "@/lib/protocol/pixelSeedCall";
import { assertWriteRegionBytes, assertWriteRegionTarget, computeDirtyRegion } from "@/lib/paint/regionProducer";
import { ipcErrorMessage } from "@/tauri/native";
import { selectionUploadRect } from "../keyboardShortcuts/selectionTool";
import { computeChangedRegion } from "@/lib/paint/regionProducer";

/**
 * Gradient tool: start drag (pointer down), track end point during drag
 * (with Shift 45° angle lock), and apply the gradient fill on pointer up.
 */
export function startGradientDrag(
  ctx: PointerToolContext,
  e: PointerEvent,
  state: GradientDragState,
): boolean {
  const { editor } = ctx;
  const { workspace, scheduler, gradientType, setGradientDragLine } = editor;

  if (editor.activeTool() !== "gradient") return false;

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
  state.start = { x: coords.x, y: coords.y };
  state.end = { x: coords.x, y: coords.y };
  state.isDragging = true;
  const gType = typeof gradientType === "function" ? gradientType() : "linear";
  if (typeof setGradientDragLine === "function") {
    setGradientDragLine({ start: coords, end: coords, type: gType, angle: 0, distance: 0 });
  }
  trySetPointerCapture(ctx.getCanvasRef(), e.pointerId);
  void scheduler;
  void history;
  return true;
}

export function trackGradientDrag(
  ctx: PointerToolContext,
  e: PointerEvent,
  state: GradientDragState,
): boolean {
  const { editor } = ctx;
  const { scheduler, gradientType, setGradientDragLine } = editor;

  if (editor.activeTool() !== "gradient" || !state.isDragging || !state.start) return false;

  const coords = ctx.getDocCoords(e);
  let endX = coords.x;
  let endY = coords.y;

  if (e.shiftKey) {
    const dx = endX - state.start.x;
    const dy = endY - state.start.y;
    const dist = Math.sqrt(dx * dx + dy * dy);
    let angle = Math.atan2(dy, dx);
    const step = Math.PI / 4; // 45°
    angle = Math.round(angle / step) * step;
    endX = state.start.x + dist * Math.cos(angle);
    endY = state.start.y + dist * Math.sin(angle);
  }

  state.end = { x: endX, y: endY };
  const dx = endX - state.start.x;
  const dy = endY - state.start.y;
  const dist = Math.sqrt(dx * dx + dy * dy);
  let deg = (Math.atan2(dy, dx) * 180 / Math.PI + 360) % 360;

  const gType = typeof gradientType === "function" ? gradientType() : "linear";
  if (typeof setGradientDragLine === "function") {
    setGradientDragLine({
      start: state.start,
      end: { x: endX, y: endY },
      type: gType,
      angle: Math.round(deg * 10) / 10,
      distance: Math.round(dist),
    });
  }
  scheduler.requestRender();
  return true;
}

/**
 * Apply gradient on pointer up. Reads the drag start/end from `state`,
 * builds color stops from the active preset, applies a selection mask, and
 * records the changed pixels in the Rust canonical store. Returns true when
 * handled.
 *
 * Rust records the gradient: it takes the changed region through
 * `rust_pixels_write_region` (one canonical `Pixel` history entry) and drives
 * the derived surface + visible raster from the Rust result, so one gradient is
 * exactly one byte-exact undo step. The raster is computed on the CPU from the
 * canonical read; the WebGPU gradient kernel stays a preview-tier rasteriser
 * because GPU-produced bytes that reach the model are bytes no canonical entry
 * can undo.
 */
export async function applyGradientFill(
  ctx: PointerToolContext,
  state: GradientDragState,
): Promise<boolean> {
  const { editor } = ctx;
  const {
    workspace,
    renderer,
    scheduler,
    fgColor,
    bgColor,
    gradientPreset,
    gradientType,
    setGradientDragLine,
  } = editor;

  if (editor.activeTool() !== "gradient" || !state.isDragging) return false;
  state.isDragging = false;
  // Capture drag points BEFORE clearing state — the guards below need them.
  const dragStart = state.start;
  const dragEnd = state.end;
  // Reset the drag line SYNCHRONOUSLY — the line must vanish the instant the
  // pointer releases, even though the canonical pixel write below is async.
  state.start = null;
  state.end = null;
  if (typeof setGradientDragLine === "function") {
    setGradientDragLine(null);
  }
  const resetGradientState = () => {
    state.start = null;
    state.end = null;
    if (typeof setGradientDragLine === "function") {
      setGradientDragLine(null);
    }
  };

  const engine = workspace.getActiveEngine();
  const history = workspace.getActiveHistory();
  if (!engine || !history || !dragStart || !dragEnd) {
    resetGradientState();
    return true;
  }

  const layerId = engine.getActiveLayerId();
  if (!layerId) { resetGradientState(); return true; }
  const layer = engine.getLayer(layerId);
  if (!layer) { resetGradientState(); return true; }

  // UNCONDITIONAL: a gradient is always recorded by the Rust canonical pixel
  // store, so the default state of photrez.rustPixels (key absent or "0") takes
  // the same path as "1". Gating on it sent the default state down a TypeScript
  // bitmap commit that recorded no Rust Pixel entry, so one undo stepped past the
  // gradient onto the previous entry and left its pixels on the canvas. A layer
  // with no derived surface has no pixels to seed the store from either, so that
  // case is a visible refusal, never a silent second pixel owner.
  const surface = engine.getPaintSurface(layerId);
  if (!surface) { showToast("Rust pixel surface not ready", "warn"); resetGradientState(); return true; }

  // Build color stops from preset
  const hex = fgColor().replace("#", "");
  const fgR = parseInt(hex.slice(0, 2), 16);
  const fgG = parseInt(hex.slice(2, 4), 16);
  const fillB_local = parseInt(hex.slice(4, 6), 16);
  const bgHex = bgColor().replace("#", "");
  const bgR = parseInt(bgHex.slice(0, 2), 16);
  const bgG = parseInt(bgHex.slice(2, 4), 16);
  const bgB = parseInt(bgHex.slice(4, 6), 16);

  let stops: ColorStop[];
  if (gradientPreset() === "fg-transparent") {
    stops = [
      { offset: 0, r: fgR, g: fgG, b: fillB_local, a: 255 },
      { offset: 1, r: fgR, g: fgG, b: fillB_local, a: 0 },
    ];
  } else {
    stops = [
      { offset: 0, r: fgR, g: fgG, b: fillB_local, a: 255 },
      { offset: 1, r: bgR, g: bgG, b: bgB, a: 255 },
    ];
  }

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

  // Convert gradient start/end to layer-local space (accounts for transform)
  const startLocal = documentToLayerLocal(dragStart.x, dragStart.y, layer.transform, layer.width, layer.height);
  const endLocal = documentToLayerLocal(dragEnd.x, dragEnd.y, layer.transform, layer.width, layer.height);

  // Calm inline loading in status bar (200ms delay per Material/Carbon — avoids flicker on 31ms, shows on 81ms+).
  const loadingTimer: number | null = window.setTimeout(() => ctx.editor.setStatusLoadingMessage?.("Applying gradient..."), 200);
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const { pixelInvoke } = await import("@/lib/protocol/pixelInvokeCensus");
    const docId = workspace.getActiveDocumentId() ?? "";
    const w = layer.width;
    const h = layer.height;
    // One store epoch read serves both steps below. When it matches the surface
    // epoch the surface already holds the store pixels, so the full-layer
    // rehydration is skipped.
    const storeEpoch = await getRustEpoch(docId, layerId);
    if (storeEpoch !== null && surface.pixelEpoch !== storeEpoch) {
      await rehydratePaintSurfaceFromRust(docId, layerId, surface);
    }
    if (storeEpoch === null) {
      const seed = surface.readRect(0, 0, w, h);
      await invoke("rust_pixels_init", pixelSeedDispatch(docId, layerId, w, h, seed.data));
    }
    // CANONICAL RASTER: the gradient is computed on the CPU from the canonical
    // bytes read out of the derived surface, never from a GPU readback. The
    // WebGPU gradient kernel is a preview-tier rasteriser; letting its output
    // become the model raster made the document hold pixels no canonical entry
    // could undo, and the two owners disagreed. gradientFill is the same kernel
    // the masked and multi-stop cases already used, on a copy of the canonical
    // read, so the surface itself is untouched until Rust returns its tiles.
    const canonical = surface.readRect(0, 0, w, h);
    const before = new Uint8ClampedArray(canonical.data);
    const raster = gradientFill(
      canonical, gradientType(),
      startLocal.x, startLocal.y,
      endLocal.x, endLocal.y,
      stops, fillMask ?? null,
    );
    const changed = computeChangedRegion(before, raster.data, w, h);
    // A gradient whose result equals the layer already is not an edit, and a
    // drag of zero length paints the first stop everywhere: either way there is
    // nothing to record, so it stops before any write.
    if (!changed) return true;
    // The byte diff stays the authority for WHAT changed; this call only owns
    // the region arithmetic. A non-inverted mask bounds the raster, so the
    // intersection below is that same box and the shipped rgba always covers the
    // region exactly.
    const selRect = selectionUploadRect(engine);
    const region = computeDirtyRegion(changed, selRect
      ? { x: selRect.x, y: selRect.y, w: selRect.width, h: selRect.height }
      : null);
    if (!region) return true;
    assertWriteRegionTarget(docId, layerId, region, w, h);
    assertWriteRegionBytes(changed.rgba.byteLength, region);
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
    // Imperative is entry-owned (tile-memento model): history stores it and
    // replays its before/after tiles on undo/redo; the Rust entry is synced via
    // `rust_pixels_undo` (single step, no second TS-visible entry). `rustOwned`
    // marks it a cursor token for a step Rust already holds, so the undo/redo
    // dispatch takes the pixels from Rust and drains this twin in lockstep
    // instead of replaying tiles no store holds.
    const imperative = {
      layerId,
      surfaceWidth: w,
      surfaceHeight: h,
      // The reply carries the POST-image only, so it has no pre-image to store.
      // This entry is a cursor token for a step Rust already holds: the undo/redo
      // dispatch takes its pixels from Rust and refuses to replay these tiles, so
      // the empty array below is what it already did with them.
      before: [],
      after: res.after.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
      rustOwned: true,
    };
    // TS derived cache updated from Rust's authoritative returned `after` tiles + epoch.
    applyRustTilesToSurface(surface.context, res.after);
    surface.pixelEpoch = res.epoch;
    surface.pixelVersion = res.version;
    syncFacadeVersionFromPixel(docId, res.version);
    renderer?.uploadSurfaceTiles?.(layerId, w, h, res.after.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })));
    // The drawn layer comes from layer.imageBitmap, not from Rust, so rebuild
    // that raster from the surface the canonical tiles were just applied to.
    // Without this the gradient is recorded correctly but stays invisible until
    // an undo/redo happens to rebuild the bitmap.
    await projectRustPixelsToVisibleSurface(engine, renderer, layerId, surface, res.epoch);
    history.commit(preSnapshot, "Gradient Fill", imperative, true);
    scheduler.requestRender();
  } catch (err) {
    showToast(`Gradient fill failed: ${ipcErrorMessage(err)}`, "error");
  } finally {
    if (loadingTimer !== null) clearTimeout(loadingTimer);
    ctx.editor.setStatusLoadingMessage?.(null);
  }

  return true;
}
