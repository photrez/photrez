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
  let strokeEnabled = editor.shapeStrokeEnabled();
  const strokeWidth = Math.max(1, editor.shapeStrokeWidth());
  let fillEnabled = editor.shapeFillEnabled();
  // Visibility guard: if both fill and stroke are disabled, enable stroke
  // so newly created shapes are never rendered as invisible 0-alpha bitmaps.
  if (!strokeEnabled && !fillEnabled) {
    strokeEnabled = true;
  }
  const fg = editor.fgColor();

  let w = Math.abs(end.x - start.x);
  let h = Math.abs(end.y - start.y);
  if (shiftKey && kind !== "line") {
    const side = Math.max(w, h);
    w = side;
    h = side;
  }
  // Shift+alt: square pre-doubling, then double → square box centered on start.
  if (kind !== "line") {
    w = Math.max(1, w);
    h = Math.max(1, h);
  } else {
    // Vertical (w=0) and horizontal (h=0) lines are legal; only the fully
    // degenerate 0×0 point is clamped so the initial temp layer rasterizes.
    w = Math.max(0, w);
    h = Math.max(0, h);
    if (w === 0 && h === 0) w = 1;
  }
  let docX = Math.min(start.x, end.x);
  let docY = Math.min(start.y, end.y);
  if (altKey) {
    // Alt: the drag start is the CENTER of the shape. The box spans
    // start ± delta (cursor at the far corner), so width/height are double
    // the cursor delta while the top-left corner sits one delta back from
    // the center.
    const extentX = w;
    const extentY = h;
    w = extentX * 2;
    h = extentY * 2;
    docX = start.x - extentX;
    docY = start.y - extentY;
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
  const layers = engine.getLayers();
  const selectedId = editor.selectedLayerId();

  // If clicking inside an existing shape layer or selected shape layer, don't create a new shape layer;
  // select it and let layer drag move it instead.
  for (let i = 0; i < layers.length; i++) {
    const l = layers[i];
    if (l.locked || !l.visible) continue;
    const w = l.width * Math.abs(l.transform.scaleX);
    const h = l.height * Math.abs(l.transform.scaleY);
    if (
      coords.x >= l.transform.x &&
      coords.x <= l.transform.x + w &&
      coords.y >= l.transform.y &&
      coords.y <= l.transform.y + h
    ) {
      if (l.type === "shape") {
        engine.setActiveLayer(l.id);
        editor.setSelectedLayerId(l.id);
        return false;
      }
    }
  }

  state.start = { x: coords.x, y: coords.y };
  state.isDragging = true;
  // Snapshot BEFORE the temp layer exists so undo of "Add Shape" removes it.
  state.preSnapshot = engine.snapshot();

  try {
    const first = buildParams(ctx, state.start, state.start, false, false);
    const layer = engine.addShapeLayer("Shape", first.params);
    state.tempLayerId = layer.id;
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

  // Accidental click guard. For lines, height 0 is legal (rasterizer allows
  // horizontal lines), so measure the drag length via hypot instead of the
  // per-axis check (which would delete a valid horizontal line).
  const isClick = params.kind === "line"
    ? Math.hypot(params.width, params.height) < MIN_DRAG_PX
    : Math.abs(params.width) < MIN_DRAG_PX || Math.abs(params.height) < MIN_DRAG_PX;
  if (isClick) {
    engine.deleteLayer(tempId);
    state.reset();
    return true;
  }

  const preSnapshot = state.preSnapshot;
  if (!preSnapshot) {
    engine.deleteLayer(tempId);
    state.reset();
    return true;
  }
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