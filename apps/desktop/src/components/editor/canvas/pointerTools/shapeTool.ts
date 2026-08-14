// SPDX-License-Identifier: AGPL-3.0-or-later
import { trySetPointerCapture } from "../../tools/pointerCapture";
import { showToast } from "../../Toast";
import { hitTestLayer } from "@/viewport/layerHitTest";
import { shapeRenderMargin, MAX_SHAPE_DIM } from "@/engine/shapeRaster";
import type { DocumentEngine } from "@/engine/document";
import type { ShapeDragState, PointerToolContext } from "./pointerToolContext";
import type { ShapeParams, ShapeKind } from "@/engine/types";

const MIN_DRAG_PX = 3;

/** Type guard: an editor signal accessor is present only when it is callable. */
function isFn(v: unknown): v is () => unknown {
  return typeof v === "function";
}

export interface ShapeBox {
  width: number;
  height: number;
  docX: number;
  docY: number;
  flipH: boolean;
  flipV: boolean;
}

/** Pure geometry for a shape drag. Returns the box plus the flips needed so a
 *  line/arrow runs from the press point to the release point (arrow head at
 *  release). Kept pure so it can be unit-tested without a pointer context. */
export function computeShapeBox(
  kind: ShapeKind,
  start: { x: number; y: number },
  end: { x: number; y: number },
  shiftKey: boolean,
  altKey: boolean,
): ShapeBox {
  let cur = end;
  // Shift on a line snaps the drag vector to the nearest 45° increment
  // (0/45/90/135/...) so the user can draw perfectly straight/diagonal lines.
  if (shiftKey && kind === "line") {
    const dx = end.x - start.x;
    const dy = end.y - start.y;
    const len = Math.hypot(dx, dy);
    if (len > 0) {
      const ang = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
      cur = { x: start.x + len * Math.cos(ang), y: start.y + len * Math.sin(ang) };
    }
  }
  let w = Math.abs(cur.x - start.x);
  let h = Math.abs(cur.y - start.y);
  if (shiftKey && kind !== "line") {
    const side = Math.max(w, h);
    w = side;
    h = side;
  }
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
  // Guard against an absurdly large shape (e.g. dragging across a heavily
  // zoomed-out canvas) which would blow the rasterizer's canvas allocation.
  w = Math.min(w, MAX_SHAPE_DIM);
  h = Math.min(h, MAX_SHAPE_DIM);
  let docX = Math.min(start.x, cur.x);
  let docY = Math.min(start.y, cur.y);
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
  // Flip the local (0,0)->(w,h) diagonal so the line runs press→release and
  // the arrow head lands at the release point for any drag direction.
  const flipH = cur.x < start.x;
  const flipV = cur.y < start.y;
  return { width: w, height: h, docX, docY, flipH, flipV };
}

/** Abort an in-progress shape drag: delete the temp layer (if any) and reset
 *  state. No history entry is written. Shared by pointercancel, lost-capture,
 *  and the Escape-key cancel path. */
export function cancelShapeDrag(
  state: ShapeDragState,
  engine: DocumentEngine | null,
  scheduler?: { requestRender: () => void },
): void {
  if (state.tempLayerId && engine) {
    engine.deleteLayer(state.tempLayerId);
  }
  state.reset();
  scheduler?.requestRender();
}

function buildParams(
  ctx: PointerToolContext,
  start: { x: number; y: number },
  end: { x: number; y: number },
  shiftKey: boolean,
  altKey: boolean,
): { params: ShapeParams; docX: number; docY: number; flipH: boolean; flipV: boolean } {
  const { editor } = ctx;
  // Uniform accessor contract: every editor signal is treated as optionally
  // present (test harnesses may pass a partial editor), so all reads use the
  // same `isFn` guard with a sensible default. Mirrors textTool.buildSessionTextData.
  const kind = isFn(editor.shapeKind) ? editor.shapeKind() : "rect";
  let strokeEnabled = isFn(editor.shapeStrokeEnabled) ? editor.shapeStrokeEnabled() : true;
  const strokeWidth = Math.max(1, isFn(editor.shapeStrokeWidth) ? editor.shapeStrokeWidth() : 1);
  let fillEnabled = isFn(editor.shapeFillEnabled) ? editor.shapeFillEnabled() : true;
  // Visibility guard: if both fill and stroke are disabled, enable stroke
  // so newly created shapes are never rendered as invisible 0-alpha bitmaps.
  if (!strokeEnabled && !fillEnabled) {
    strokeEnabled = true;
  }
  const fg = isFn(editor.fgColor) ? editor.fgColor() : "#000000";

  const box = computeShapeBox(kind, start, end, shiftKey, altKey);

  const params: ShapeParams = {
    kind,
    width: box.width,
    height: box.height,
    radius: kind === "rect" ? Math.max(0, isFn(editor.shapeRadius) ? editor.shapeRadius() : 0) : 0,
    fill: { kind: fillEnabled ? "solid" : "none", color: fg },
    stroke: {
      enabled: strokeEnabled,
      color: isFn(editor.shapeStrokeColor) ? editor.shapeStrokeColor() : "#000000",
      width: strokeWidth,
    },
    arrowHead: kind === "line" && (isFn(editor.shapeArrowHead) ? editor.shapeArrowHead() : false),
  };
  return { params, docX: box.docX, docY: box.docY, flipH: box.flipH, flipV: box.flipV };
}

export function startShapeDrag(ctx: PointerToolContext, e: PointerEvent, state: ShapeDragState): boolean {
  const { editor } = ctx;
  const { workspace } = editor;
  if (editor.activeTool() !== "shape") return false;

  const engine = workspace.getActiveEngine();
  if (!engine) return true;

  const coords = ctx.getDocCoords(e);
  const layers = engine.getLayers();

  // If the press lands inside an existing SHAPE layer, select it (rotation/
  // scale/flip-aware via the shared hit test) instead of starting a new shape.
  // Non-shape hits are skipped so a new shape can still be drawn on top of
  // raster layers.
  for (let i = 0; i < layers.length; i++) {
    const l = layers[i];
    if (l.locked || !l.visible) continue;
    if (!hitTestLayer(coords, l)) continue;
    if (l.type === "shape") {
      engine.setActiveLayer(l.id);
      editor.setSelectedLayerId(l.id);
      return false;
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
  const { params, docX, docY, flipH, flipV } = buildParams(ctx, state.start, coords, e.shiftKey, e.altKey);
  const margin = shapeRenderMargin(params);
  try {
    engine.updateShapeParams(state.tempLayerId, params);
    engine.transformLayer(state.tempLayerId, { x: docX - margin, y: docY - margin, flipH, flipV });
    const bitmap = engine.getLayerImageBitmap(state.tempLayerId);
    if (bitmap) renderer?.uploadImage(state.tempLayerId, bitmap);
    scheduler.requestRender();
  } catch (err) {
    // Transient render error during drag-move; the final apply() surfaces
    // real failures. Log in dev instead of swallowing silently.
    if (import.meta.env.DEV) console.warn("[shape-tool] transient track render error (ignored):", err);
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
  const { params, docX, docY, flipH, flipV } = buildParams(ctx, state.start, coords, e.shiftKey, e.altKey);
  const margin = shapeRenderMargin(params);

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
    engine.transformLayer(tempId, { x: docX - margin, y: docY - margin, flipH, flipV });
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