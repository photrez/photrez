// SPDX-License-Identifier: AGPL-3.0-or-later
// Shape rasterizer: ShapeParams -> ImageBitmap via Canvas2D offscreen.
// Swap-able backend seam (resvg/skia-canvas WASM can replace the body if
// the 0.4.0 WASM profile demands it — API stays `params -> ImageBitmap`).
import type { ShapeParams } from "./types";

const MIN_STROKE_WIDTH = 1;

// Hard cap on the allocated shape canvas. A shape dragged across a heavily
// zoomed-out document could otherwise request a multi-100k px OffscreenCanvas
// and OOM the renderer/WebGL. 16384 is the common max texture size; we clamp
// both the raster allocation and the logical layer dims (see shapeTool.ts).
export const MAX_SHAPE_DIM = 16384;

function makeCanvas(w: number, h: number): {
  canvas: OffscreenCanvas | HTMLCanvasElement;
  ctx: OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D;
} {
  const canvas =
    typeof OffscreenCanvas !== "undefined"
      ? new OffscreenCanvas(w, h)
      : (() => {
          const el = document.createElement("canvas");
          el.width = w;
          el.height = h;
          return el;
        })();
  const ctx = canvas.getContext("2d") as
    | OffscreenCanvasRenderingContext2D
    | CanvasRenderingContext2D
    | null;
  if (!ctx) throw new Error("E_CANVAS: Canvas2D context unavailable for shape rasterization");
  return { canvas, ctx };
}

function toBitmap(canvas: OffscreenCanvas | HTMLCanvasElement): ImageBitmap {
  if (typeof OffscreenCanvas !== "undefined" && canvas instanceof OffscreenCanvas) {
    return canvas.transferToImageBitmap();
  }
  // OffscreenCanvas absent (macOS < 13.3 WebKit, old WebKitGTK).
  // HTMLCanvasElement is accepted by WebGL as a texture source, but consumers
  // call bitmap.close() (saveWorker, history); give it a no-op so saves don't throw.
  const html = canvas as HTMLCanvasElement & { close?: () => void };
  html.close = () => {};
  return html as unknown as ImageBitmap;
}

/** Margin (px) baked around a shape bitmap so the centered stroke (and the
 *  line/arrow round caps + arrow head) is not clipped by the bitmap edge.
 *  0 when the stroke is disabled; otherwise `strokeWidth` for every shape
 *  kind except line/arrow, which reserve extra for caps + arrow head barb.
 *  The shape tool offsets layer placement by this margin and the selection
 *  overlay/hit-test subtract it, so the visible shape keeps its exact size
 *  (no floating gap) while the stroke stays fully painted. */
export function shapeRenderMargin(params: ShapeParams): number {
  if (!params.stroke.enabled) return 0;
  const strokeWidth = Math.max(MIN_STROKE_WIDTH, params.stroke.width || MIN_STROKE_WIDTH);
  if (params.kind !== "line") return strokeWidth;
  // Lines/arrows reserve room for the round caps at both endpoints (otherwise
  // the half-cap past the drag point is clipped at the canvas edge). Arrow
  // lines reserve extra so the arrow head barb sweep isn't clipped.
  const cap = strokeWidth;
  if (!params.arrowHead) return cap;
  const len = Math.max(8, strokeWidth * 3);
  return Math.max(cap, len);
}

export function renderShapeToBitmap(params: ShapeParams): ImageBitmap {
  const isLine = params.kind === "line";
  const invalidRect = !(params.width > 0) || !(params.height > 0);
  const invalidLine = !(params.width > 0) && !(params.height > 0);
  if (isLine ? invalidLine : invalidRect) {
    throw new Error(`E_SHAPE_DIM: shape width/height must be > 0 (got ${params.width}x${params.height})`);
  }
  const strokeEnabled = isLine ? true : params.stroke.enabled;
  const strokeWidth = strokeEnabled
    ? Math.max(MIN_STROKE_WIDTH, params.stroke.width || (isLine ? 2 : MIN_STROKE_WIDTH))
    : 0;
  const strokeColor = isLine && !params.stroke.enabled && params.fill.kind === "solid"
    ? params.fill.color
    : params.stroke.color;

  const margin = shapeRenderMargin(params);

  const w = Math.max(1, Math.round(Math.min(params.width + margin * 2, MAX_SHAPE_DIM)));
  const h = Math.max(1, Math.round(Math.min(params.height + margin * 2, MAX_SHAPE_DIM)));
  const { canvas, ctx } = makeCanvas(w, h);

  if (margin > 0) {
    ctx.translate(margin, margin);
  }

  const renderW = params.width;
  const renderH = params.height;

  ctx.fillStyle = params.fill.kind === "solid" ? params.fill.color : (isLine ? strokeColor : "transparent");
  ctx.strokeStyle = strokeColor;
  ctx.lineWidth = strokeWidth;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";

  ctx.beginPath();
  if (params.kind === "rect") {
    const radius = Math.max(0, Math.min(params.radius, Math.min(renderW, renderH) / 2));
    if (typeof (ctx as any).roundRect === "function") {
      (ctx as any).roundRect(0, 0, renderW, renderH, radius);
    } else {
      ctx.rect(0, 0, renderW, renderH);
    }
  } else if (params.kind === "ellipse") {
    ctx.ellipse(renderW / 2, renderH / 2, renderW / 2, renderH / 2, 0, 0, Math.PI * 2);
  } else if (params.kind === "triangle") {
    ctx.moveTo(renderW / 2, 0);
    ctx.lineTo(renderW, renderH);
    ctx.lineTo(0, renderH);
    ctx.closePath();
  } else if (params.kind === "star") {
    const rx = renderW * 0.525731;
    const ry = renderH * 0.552786;
    const cx = renderW / 2;
    const cy = ry;
    const innerRatio = 0.382;
    const points = 5;
    for (let i = 0; i < points * 2; i++) {
      const rScale = i % 2 === 0 ? 1 : innerRatio;
      const angle = (i * Math.PI) / points - Math.PI / 2;
      const x = cx + rx * rScale * Math.cos(angle);
      const y = cy + ry * rScale * Math.sin(angle);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.closePath();
  } else if (params.kind === "block-arrow") {
    const headW = renderW * 0.4;
    const stemY1 = renderH * 0.25;
    const stemY2 = renderH * 0.75;
    ctx.moveTo(0, stemY1);
    ctx.lineTo(renderW - headW, stemY1);
    ctx.lineTo(renderW - headW, 0);
    ctx.lineTo(renderW, renderH / 2);
    ctx.lineTo(renderW - headW, renderH);
    ctx.lineTo(renderW - headW, stemY2);
    ctx.lineTo(0, stemY2);
    ctx.closePath();
  } else if (params.kind === "heart") {
    const w = renderW;
    const h = renderH;
    ctx.moveTo(w / 2, h * 0.28);
    ctx.bezierCurveTo(w * 0.32, h * 0.02, 0, h * 0.12, 0, h * 0.4);
    ctx.bezierCurveTo(0, h * 0.65, w * 0.28, h * 0.82, w / 2, h);
    ctx.bezierCurveTo(w * 0.72, h * 0.82, w, h * 0.65, w, h * 0.4);
    ctx.bezierCurveTo(w, h * 0.12, w * 0.68, h * 0.02, w / 2, h * 0.28);
    ctx.closePath();
  } else if (params.kind === "diamond") {
    ctx.moveTo(renderW / 2, 0);
    ctx.lineTo(renderW, renderH / 2);
    ctx.lineTo(renderW / 2, renderH);
    ctx.lineTo(0, renderH / 2);
    ctx.closePath();
  } else if (params.kind === "speech-bubble") {
    const w = renderW;
    const h = renderH;
    const bodyH = h * 0.8;
    const r = Math.min(12, bodyH / 4);
    ctx.moveTo(r, 0);
    ctx.lineTo(w - r, 0);
    ctx.quadraticCurveTo(w, 0, w, r);
    ctx.lineTo(w, bodyH - r);
    ctx.quadraticCurveTo(w, bodyH, w - r, bodyH);
    ctx.lineTo(w * 0.35, bodyH);
    ctx.lineTo(w * 0.1, h);
    ctx.lineTo(w * 0.2, bodyH);
    ctx.lineTo(r, bodyH);
    ctx.quadraticCurveTo(0, bodyH, 0, bodyH - r);
    ctx.lineTo(0, r);
    ctx.quadraticCurveTo(0, 0, r, 0);
    ctx.closePath();
  } else if (params.kind === "hexagon") {
    ctx.moveTo(renderW * 0.25, 0);
    ctx.lineTo(renderW * 0.75, 0);
    ctx.lineTo(renderW, renderH / 2);
    ctx.lineTo(renderW * 0.75, renderH);
    ctx.lineTo(renderW * 0.25, renderH);
    ctx.lineTo(0, renderH / 2);
    ctx.closePath();
  } else {
    // line: (0,0) -> (renderW, renderH); arrowHead at the end
    ctx.moveTo(0, 0);
    ctx.lineTo(renderW, renderH);
    if (params.arrowHead) {
      const angle = Math.atan2(renderH, renderW);
      const p = Math.PI / 6; // 30° spread
      const headLen = Math.max(12, strokeWidth * 4);
      ctx.moveTo(renderW, renderH);
      ctx.lineTo(
        renderW - headLen * Math.cos(angle - p),
        renderH - headLen * Math.sin(angle - p)
      );
      ctx.lineTo(
        renderW - headLen * Math.cos(angle + p),
        renderH - headLen * Math.sin(angle + p)
      );
      ctx.closePath();
    }
  }

  if (params.fill.kind === "solid" && !isLine) ctx.fill();
  if (strokeWidth > 0) {
    if (isLine && params.arrowHead) {
      ctx.fill();
    }
    ctx.stroke();
  }

  return toBitmap(canvas);
}