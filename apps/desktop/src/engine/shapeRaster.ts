// SPDX-License-Identifier: AGPL-3.0-or-later
// Shape rasterizer: ShapeParams -> ImageBitmap via Canvas2D offscreen.
// Swap-able backend seam (resvg/skia-canvas WASM can replace the body if
// the 0.4.0 WASM profile demands it — API stays `params -> ImageBitmap`).
import type { ShapeParams } from "./types";

const MIN_STROKE_WIDTH = 1;

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

export function renderShapeToBitmap(params: ShapeParams): ImageBitmap {
  const isLine = params.kind === "line";
  const invalidRect = !(params.width > 0) || !(params.height > 0);
  const invalidLine = !(params.width > 0) && !(params.height > 0);
  if (isLine ? invalidLine : invalidRect) {
    throw new Error(`E_SHAPE_DIM: shape width/height must be > 0 (got ${params.width}x${params.height})`);
  }
  const strokeWidth = params.stroke.enabled
    ? Math.max(MIN_STROKE_WIDTH, params.stroke.width)
    : 0;
  const len = Math.max(8, strokeWidth * 3);
  // margin covers the stroke overhang (half the line width outside the shape
  // box). For line+arrowHead the wings extend len from the tip, so the uniform
  // margin must also fit the head.
  const margin = params.kind === "line" && params.arrowHead
    ? Math.max(strokeWidth, len)
    : strokeWidth;
  // A line may have a 0 axis (vertical/horizontal, stroke-less too); keep the
  // bitmap at least 1px each side so the canvas stays allocatable.
  const w = Math.max(1, params.width + margin * 2);
  const h = Math.max(1, params.height + margin * 2);
  const { canvas, ctx } = makeCanvas(w, h);

  ctx.translate(margin, margin);
  ctx.fillStyle = params.fill.kind === "solid" ? params.fill.color : "transparent";
  ctx.strokeStyle = params.stroke.color;
  ctx.lineWidth = strokeWidth;
  ctx.lineCap = "butt";

  ctx.beginPath();
  if (params.kind === "rect") {
    const radius = Math.max(0, Math.min(params.radius, Math.min(params.width, params.height) / 2));
    if (typeof (ctx as any).roundRect === "function") {
      (ctx as any).roundRect(0, 0, params.width, params.height, radius);
    } else {
      ctx.rect(0, 0, params.width, params.height);
    }
  } else if (params.kind === "ellipse") {
    ctx.ellipse(params.width / 2, params.height / 2, params.width / 2, params.height / 2, 0, 0, Math.PI * 2);
  } else {
    // line: (0,0) -> (width, height); arrowHead triangle at the end
    ctx.moveTo(0, 0);
    ctx.lineTo(params.width, params.height);
    if (params.arrowHead) {
      const angle = Math.atan2(params.height, params.width);
      const p = Math.PI / 6; // 30° spread
      ctx.moveTo(params.width, params.height);
      ctx.lineTo(
        params.width - len * Math.cos(angle - p),
        params.height - len * Math.sin(angle - p)
      );
      ctx.moveTo(params.width, params.height);
      ctx.lineTo(
        params.width - len * Math.cos(angle + p),
        params.height - len * Math.sin(angle + p)
      );
    }
  }

  if (params.fill.kind === "solid") ctx.fill();
  if (strokeWidth > 0) ctx.stroke();

  return toBitmap(canvas);
}