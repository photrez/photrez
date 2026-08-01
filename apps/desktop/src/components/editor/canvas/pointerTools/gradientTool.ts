// SPDX-License-Identifier: AGPL-3.0-or-later
import { documentToLayerLocal } from "@/viewport/transformGeometry";
import { gradientFill, type FillMask, type ColorStop } from "@/features/fill/fillOperations";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { showToast } from "../../Toast";
import { trySetPointerCapture } from "../../tools/pointerCapture";
import type { GradientDragState, PointerToolContext } from "./pointerToolContext";

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
 * commits the resulting bitmap. Returns true when handled.
 */
export function applyGradientFill(
  ctx: PointerToolContext,
  state: GradientDragState,
): boolean {
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
  const resetGradientState = () => {
    state.start = null;
    state.end = null;
    if (typeof setGradientDragLine === "function") {
      setGradientDragLine(null);
    }
  };

  const engine = workspace.getActiveEngine();
  const history = workspace.getActiveHistory();
  if (!engine || !history || !state.start || !state.end) {
    resetGradientState();
    return true;
  }

  const layerId = engine.getActiveLayerId();
  if (!layerId) { resetGradientState(); return true; }
  const layer = engine.getLayer(layerId);
  if (!layer) { resetGradientState(); return true; }

  const bitmap = engine.getLayerImageBitmap(layerId);
  if (!bitmap) { showToast("Layer has no image data", "warn"); resetGradientState(); return true; }

  const offscreen = new OffscreenCanvas(layer.width, layer.height);
  const ctx2d = offscreen.getContext("2d");
  if (!ctx2d) { resetGradientState(); return true; }
  ctx2d.drawImage(bitmap, 0, 0);
  const imgData = ctx2d.getImageData(0, 0, layer.width, layer.height);

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
  const startLocal = documentToLayerLocal(state.start.x, state.start.y, layer.transform, layer.width, layer.height);
  const endLocal = documentToLayerLocal(state.end.x, state.end.y, layer.transform, layer.width, layer.height);

  gradientFill(
    imgData, gradientType(),
    startLocal.x, startLocal.y,
    endLocal.x, endLocal.y,
    stops, fillMask ?? null,
  );

  const preSnapshot = engine.snapshot();
  ctx2d.putImageData(imgData, 0, 0);
  const newBitmap = offscreen.transferToImageBitmap();
  try {
    engine.setLayerImageBitmap(layerId, newBitmap);
    renderer?.uploadImage(layerId, newBitmap);
  } catch (err) {
    showToast(`Gradient fill failed: ${err instanceof Error ? err.message : 'Unknown error'}`, "error");
    resetGradientState();
    return true;
  }
  history.commit(preSnapshot, "Gradient Fill");
  scheduler.requestRender();

  resetGradientState();
  return true;
}
