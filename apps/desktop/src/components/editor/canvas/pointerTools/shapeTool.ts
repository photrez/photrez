// SPDX-License-Identifier: AGPL-3.0-or-later
import { trySetPointerCapture } from "../../tools/pointerCapture";
import { showToast } from "../../Toast";
import type { ShapeDragState, PointerToolContext } from "./pointerToolContext";
import type { ShapeParams } from "@/engine/types";

const MIN_DRAG_PX = 3;

function buildParams(
  ctx: PointerToolContext,
  start: { x: number; y: number },
  end: { x: number; y: number },
  shiftKey: boolean,
  altKey: boolean,
): { params: ShapeParams; docX: number; docY: number } {
  const { editor } = ctx;
  const kind = editor.shapeKind();
  const strokeEnabled = editor.shapeStrokeEnabled();
  const strokeWidth = Math.max(1, editor.shapeStrokeWidth());
  const fillEnabled = editor.shapeFillEnabled();
  const fg = editor.fgColor();

  let w = Math.abs(end.x - start.x);
  let h = Math.abs(end.y - start.y);
  if (shiftKey && kind !== "line") {
    const side = Math.max(w, h);
    w = side;
    h = side;
  }
  if (kind !== "line") {
    w = Math.max(1, w);
    h = Math.max(1, h);
  } else {
    w = Math.max(1, w);
    h = Math.max(0, h);
  }
  let docX = Math.min(start.x, end.x);
  let docY = Math.min(start.y, end.y);
  if (altKey) {
    docX = start.x - w;
    docY = start.y - h;
  }

  const params: ShapeParams = {
    kind,
    width: w,
    height: h,
    radius: kind === "rect" ? Math.max(0, editor.shapeRadius()) : 0,
    fill: { kind: fillEnabled ? "solid" : "none", color: fg },
    stroke: { enabled: strokeEnabled, color: editor.shapeStrokeColor(), width: strokeWidth },
    arrowHead: kind === "line" && editor.shapeArrowHead(),
  };
  return { params, docX, docY };
}

export function startShapeDrag(ctx: PointerToolContext, e: PointerEvent, state: ShapeDragState): boolean {
  const { editor } = ctx;
  const { workspace } = editor;
  if (editor.activeTool() !== "shape") return false;

  const engine = workspace.getActiveEngine();
  if (!engine) return true;

  const coords = ctx.getDocCoords(e);
  state.start = { x: coords.x, y: coords.y };
  state.isDragging = true;

  try {
    const first = buildParams(ctx, state.start, state.start, false, false);
    const layer = engine.addShapeLayer("Shape", first.params);
    state.tempLayerId = layer.id;
    state.params = null;
  } catch (err) {
    showToast(`Shape failed: ${err instanceof Error ? err.message : "Unknown error"}`, "error");
    state.reset();
    return true;
  }
  trySetPointerCapture(ctx.getCanvasRef(), e.pointerId);
  return true;
}

export function trackShapeDrag(ctx: PointerToolContext, e: PointerEvent, state: ShapeDragState): boolean {
  const { editor } = ctx;
  const { workspace, scheduler, renderer } = editor;
  if (editor.activeTool() !== "shape" || !state.isDragging || !state.start || !state.tempLayerId) return false;

  const engine = workspace.getActiveEngine();
  if (!engine) return true;

  const coords = ctx.getDocCoords(e);
  const { params, docX, docY } = buildParams(ctx, state.start, coords, e.shiftKey, e.altKey);
  state.params = params;
  try {
    engine.updateShapeParams(state.tempLayerId, params);
    engine.transformLayer(state.tempLayerId, { x: docX, y: docY });
    const bitmap = engine.getLayerImageBitmap(state.tempLayerId);
    if (bitmap) renderer?.uploadImage(state.tempLayerId, bitmap);
    scheduler.requestRender();
  } catch {
    // ignore transient render errors; final apply will surface real failures
  }
  return true;
}

export function applyShapeDrag(ctx: PointerToolContext, e: PointerEvent, state: ShapeDragState): boolean {
  const { editor } = ctx;
  const { workspace } = editor;
  if (editor.activeTool() !== "shape" || !state.isDragging || !state.start || !state.tempLayerId) return false;

  const engine = workspace.getActiveEngine();
  const history = workspace.getActiveHistory();
  if (!engine || !history) return true;

  const tempId = state.tempLayerId;
  const coords = ctx.getDocCoords(e);
  const { params, docX, docY } = buildParams(ctx, state.start, coords, e.shiftKey, e.altKey);

  if (Math.abs(params.width) < MIN_DRAG_PX || Math.abs(params.height) < MIN_DRAG_PX) {
    engine.deleteLayer(tempId);
    state.reset();
    return true;
  }

  const preSnapshot = engine.snapshot();
  try {
    engine.updateShapeParams(tempId, params);
    engine.transformLayer(tempId, { x: docX, y: docY });
    const bitmap = engine.getLayerImageBitmap(tempId);
    if (bitmap) editor.renderer?.uploadImage(tempId, bitmap);
  } catch (err) {
    engine.deleteLayer(tempId);
    showToast(`Shape failed: ${err instanceof Error ? err.message : "Unknown error"}`, "error");
    state.reset();
    return true;
  }
  history.commit(preSnapshot, "Add Shape");
  editor.scheduler.requestRender();
  state.reset();
  return true;
}