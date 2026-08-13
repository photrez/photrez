// SPDX-License-Identifier: AGPL-3.0-or-later
// Text rasterizer: TextData -> ImageBitmap via Canvas2D offscreen.
// Pure module by design (extraction-first): imports only ./textTypes, so it
// can be lifted into a standalone library without engine/UI/Tauri deps.
// Mirrors the shapeRaster.ts canvas seam exactly (OffscreenCanvas with an
// HTMLCanvasElement + no-op close() fallback for ancient WebKit).
import type { TextData } from "./textTypes";
import { normalizeTextData } from "./textTypes";

const RASTER_SCALE = 2;
const MIN_SCALE = 1 / 64;
const MAX_SCALE = 64;
const MAX_CANVAS_DIM = 8192;
const PADDING = 2;

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
  if (!ctx) throw new Error("E_CANVAS: Canvas2D context unavailable for text rasterization");
  return { canvas, ctx };
}

function toBitmap(canvas: OffscreenCanvas | HTMLCanvasElement): ImageBitmap {
  if (typeof OffscreenCanvas !== "undefined" && canvas instanceof OffscreenCanvas) {
    return canvas.transferToImageBitmap();
  }
  // OffscreenCanvas absent: HTMLCanvasElement works as a texture source, but
  // consumers call bitmap.close(); give it a no-op so saves don't throw.
  const html = canvas as HTMLCanvasElement & { close?: () => void };
  html.close = () => {};
  return html as unknown as ImageBitmap;
}

/**
 * Set device-space letterSpacing when the context supports it (no-op
 * otherwise — ancient engines silently ignore the property). Shared by the
 * probe (measurement) and the post-resize re-apply so the two can never
 * diverge.
 */
function applyLetterSpacing(
  ctx: OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D,
  devicePx: number,
): void {
  if ("letterSpacing" in ctx) {
    ctx.letterSpacing = `${devicePx}px`;
  }
}

/** CSS font shorthand: style/weight prefixes only when non-default, always px + quoted family. */
export function buildCSSFont(data: TextData, scaledFontSize: number): string {
  const style = data.fontStyle === "italic" ? "italic " : "";
  const weight = data.fontWeight !== 400 ? `${data.fontWeight} ` : "";
  return `${style}${weight}${scaledFontSize}px "${data.fontFamily}"`;
}

/** Minimal measurement seam so wrapText runs without a canvas (library-friendly). */
export interface TextMeasurer {
  measureText(s: string): { width: number };
}

/** True when the character is CJK (per-char wrap allowed — CSS breaks CJK anywhere). */
function isCJKChar(ch: string): boolean {
  const c = ch.codePointAt(0);
  if (c === undefined) return false;
  return (c >= 0x4e00 && c <= 0x9fff) || (c >= 0x3040 && c <= 0x30ff) || (c >= 0xac00 && c <= 0xd7af);
}

/** Greedy per-character chunks of a word that cannot fit on one line. */
function splitWord(word: string, measure: TextMeasurer, maxWidth: number): string[] {
  const chunks: string[] = [];
  let chunk = "";
  for (const ch of word) {
    const candidate = chunk + ch;
    if (chunk !== "" && measure.measureText(candidate).width > maxWidth) {
      chunks.push(chunk);
      chunk = ch;
    } else {
      chunk = candidate;
    }
  }
  if (chunk !== "") chunks.push(chunk);
  return chunks;
}

/**
 * Hybrid paragraph wrap: keeps Latin runs whole (a word only splits when it
 * alone exceeds maxWidth), breaks CJK per character, and preserves the
 * literal space runs (consecutive/leading spaces match the pre-wrap textarea,
 * WYSIWYG). A space run that would overflow is skipped — CSS pre-wrap removes
 * the white space at the wrap point. "\n" is handled by the caller; empty
 * paragraphs are preserved by wrapText. Never returns [] and never throws
 * for a non-finite maxWidth.
 */
function wrapParagraph(paragraph: string, measure: TextMeasurer, maxWidth: number): string[] {
  const lines: string[] = [];
  let current = "";
  // Split into words + literal whitespace runs (`\s+` runs stay intact so
  // "a  b" renders as "a  b", not "a b").
  const tokens = paragraph.match(/\S+|\s+/g) ?? [];
  for (const tok of tokens) {
    if (tok.trim() === "") {
      // Space run: glue when it fits; skip when it overflows (wrap point).
      const candidate = current + tok;
      if (measure.measureText(candidate).width <= maxWidth) {
        current = candidate;
      }
      continue;
    }
    // Split the word token into CJK chars (per-char break) and Latin runs.
    const chunks: string[] = [];
    let latin = "";
    for (const ch of tok) {
      if (isCJKChar(ch)) {
        if (latin !== "") {
          chunks.push(latin);
          latin = "";
        }
        chunks.push(ch);
      } else {
        latin += ch;
      }
    }
    if (latin !== "") chunks.push(latin);

    for (const chunk of chunks) {
      if (isCJKChar(chunk)) {
        // Single CJK char — appends or starts a new line.
        const candidate = current + chunk;
        if (current === "" || measure.measureText(candidate).width <= maxWidth) {
          current = candidate;
        } else {
          lines.push(current);
          current = chunk;
        }
        continue;
      }
      // Latin run — keep whole unless it alone overflows.
      const candidate = current === "" ? chunk : current + chunk;
      if (measure.measureText(candidate).width <= maxWidth) {
        current = candidate;
        continue;
      }
      if (current !== "") {
        lines.push(current);
        current = "";
      }
      if (measure.measureText(chunk).width > maxWidth) {
        const sub = splitWord(chunk, measure, maxWidth);
        for (let i = 0; i < sub.length - 1; i++) lines.push(sub[i]);
        current = sub[sub.length - 1];
      } else {
        current = chunk;
      }
    }
  }
  if (current !== "") lines.push(current);
  return lines.length > 0 ? lines : [""];
}

/**
 * Paragraph-aware wrap. "\n" separates paragraphs; empty paragraphs are
 * preserved as empty lines; a word longer than maxWidth is character-breaked;
 * space runs are preserved (WYSIWYG with the pre-wrap edit overlay). Never
 * returns [] (empty input yields [""]) and never throws for a non-finite
 * maxWidth.
 */
export function wrapText(measure: TextMeasurer, text: string, maxWidth: number): string[] {
  const width = Number.isFinite(maxWidth) && maxWidth > 0 ? maxWidth : 1;
  const lines: string[] = [];
  for (const paragraph of text.split("\n")) {
    if (paragraph === "") {
      lines.push("");
    } else {
      lines.push(...wrapParagraph(paragraph, measure, width));
    }
  }
  return lines.length > 0 ? lines : [""];
}

function resolveScale(scale: number | undefined): number {
  if (typeof scale !== "number" || !Number.isFinite(scale) || scale <= 0) return RASTER_SCALE;
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
}

export interface RasterizeResult {
  imageBitmap: ImageBitmap;
  /** Document-space dimensions (unscaled). */
  width: number;
  height: number;
}

/**
 * Rasterizes text data at RASTER_SCALE (2x) for crisp rendering. Invalid
 * input is normalized (never throws); bitmap dims are clamped to 8192.
 * The returned ImageBitmap is owned by the caller: call `imageBitmap.close()`
 * when it is replaced or disposed.
 */
export function rasterizeText(data: TextData, scale?: number): RasterizeResult {
  const normalized = normalizeTextData(data);
  const effScale = resolveScale(scale);
  const fontPx = normalized.fontSize * effScale;

  // Probe with a 1x1 canvas, then resize the SAME canvas to the measured size
  // (single allocation; ctx state is re-applied after the resize).
  const { canvas, ctx } = makeCanvas(1, 1);
  ctx.font = buildCSSFont(normalized, fontPx);
  // Apply BEFORE any measurement (wrap + line widths): letterSpacing is
  // document-space but the canvas runs at RASTER_SCALE, and real canvas
  // measureText INCLUDES the spacing — so it must be set here, scaled by
  // effScale, for the measured widths (and thus the box size, wrapping and
  // center/right alignment) to match the drawn text. Without this the box
  // came out smaller than the text and glyphs clipped at the right edge
  // (@bug 2026-08-09 B3).
  applyLetterSpacing(ctx, normalized.letterSpacing * effScale);

  const lines =
    normalized.boxMode === "area"
      ? wrapText(ctx, normalized.content, normalized.boxWidth * effScale)
      : normalized.content.split("\n");

  const lineWidths: number[] = [];
  let ascent = fontPx * 0.8;
  let descent = fontPx * 0.25;
  for (const line of lines) {
    const m = ctx.measureText(line);
    lineWidths.push(m.width);
    // Prefer actual ink bounding box metrics for a tight box fit.
    const lineAscent =
      typeof m.actualBoundingBoxAscent === "number" && m.actualBoundingBoxAscent > 0
        ? m.actualBoundingBoxAscent
        : typeof m.fontBoundingBoxAscent === "number"
          ? m.fontBoundingBoxAscent
          : fontPx * 0.8;
    const lineDescent =
      typeof m.actualBoundingBoxDescent === "number" && m.actualBoundingBoxDescent > 0
        ? m.actualBoundingBoxDescent
        : typeof m.fontBoundingBoxDescent === "number"
          ? m.fontBoundingBoxDescent
          : fontPx * 0.25;

    if (lineAscent > ascent) ascent = lineAscent;
    if (lineDescent > descent) descent = lineDescent;
  }
  // Tight cap guard: cap single-line ink height to max 1.25 * fontPx so box never bloats unnecessarily
  const maxInkHeight = fontPx * 1.25;
  if (ascent + descent > maxInkHeight) {
    const ratio = maxInkHeight / (ascent + descent);
    ascent *= ratio;
    descent *= ratio;
  }
  const maxLineWidth = Math.max(0, ...lineWidths);
  // Outline bleed: strokeText centers its ink on the glyph outline, so each
  // side needs (deviceStrokeWidth) margin total strokePad*2 per axis. Multiplied
  // by effScale because stroke.width is document-space.
  const strokeWidth = normalized.stroke?.width ?? 0;
  const strokePad = strokeWidth * effScale;

  const totalWidth =
    normalized.boxMode === "area" ? normalized.boxWidth * effScale : maxLineWidth;
  const canvasW = Math.min(MAX_CANVAS_DIM, Math.max(1, Math.ceil(totalWidth + strokePad * 2 + PADDING * 2)));
  // Vertical layout model: with textBaseline "top" each line's em box occupies
  // `lineHeight * fontPx`, and that line's glyph ink (ascent + descent) sits
  // inside it. The bitmap is exactly n-1 line spacings plus ONE ink height for
  // the last line. The old formula added `ascent + descent` on top of ALL n
  // line boxes, double-counting the ink — a 48px single-line label came out
  // ~123px tall instead of ~69px, leaving a huge dead band of transparent
  // padding that oversized the layer box (@bug 2026-08-09, follow-up to the
  // box-hittable fix). `inkPerLine` also guards tight line-heights
  // (< ~1.08em) where the spacing is smaller than the ink, so glyphs never
  // clip even though the box stays tight.
  //
  // Assumption: standard font geometry where each line's ink fits within its
  // own line box (fontBoundingBox metrics >= the true ascent, true for common
  // fonts). Fonts with unusual internal leading at line-heights below ~1.0em
  // could overhang the box bottom by a few px — accepted trade-off for the
  // tight box (the old formula never clipped but wasted a full line-height).
  const inkPerLine = Math.max(normalized.lineHeight * fontPx, ascent + descent);
  const minBoxHeight = normalized.boxMode === "area" && normalized.boxHeight > 0 ? normalized.boxHeight * effScale : 0;
  const canvasH = Math.min(
    MAX_CANVAS_DIM,
    Math.max(
      1,
      Math.ceil(
        Math.max(
          minBoxHeight,
          (lines.length - 1) * normalized.lineHeight * fontPx + inkPerLine + strokePad * 2 + PADDING * 2,
        ),
      ),
    ),
  );
  canvas.width = canvasW;
  canvas.height = canvasH;

  // canvas resize resets context state; re-apply everything.
  ctx.font = buildCSSFont(normalized, fontPx);
  ctx.fillStyle = normalized.color;
  ctx.textBaseline = "top";
  const drawCtx = ctx as OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D;
  // Canvas resize reset the state, so re-apply the same device-space value
  // the measurements were taken with.
  applyLetterSpacing(drawCtx, normalized.letterSpacing * effScale);
  const strokeEnabled = strokeWidth > 0;
  const strokeAlign = normalized.stroke?.align ?? "outside";

  if (strokeEnabled) {
    ctx.strokeStyle = normalized.stroke!.color;
    // For outside & inside alignment, half the stroke line is clipped/hidden by fill, so double the line width.
    // For center alignment, the stroke centers on the glyph outline (half in, half out), so use 1x strokePad.
    ctx.lineWidth = strokeAlign === "center" ? strokePad : strokePad * 2;
    ctx.lineJoin = "round";
    ctx.miterLimit = 2;
  }

  const baseX = PADDING + strokePad;

  if (strokeEnabled && strokeAlign === "outside") {
    // Outside (default): strokeText first (2x width), then fillText over it
    for (let i = 0; i < lines.length; i++) {
      const lw = lineWidths[i];
      const x =
        normalized.align === "left"
          ? baseX
          : normalized.align === "center"
            ? baseX + (totalWidth - lw) / 2
            : baseX + (totalWidth - lw);
      const y = PADDING + strokePad + i * (normalized.lineHeight * fontPx);
      ctx.strokeText(lines[i], x, y);
      ctx.fillText(lines[i], x, y);
    }
  } else if (strokeEnabled && strokeAlign === "center") {
    // Center: fillText first, then strokeText centered on top (1x width: half in, half out)
    for (let i = 0; i < lines.length; i++) {
      const lw = lineWidths[i];
      const x =
        normalized.align === "left"
          ? baseX
          : normalized.align === "center"
            ? baseX + (totalWidth - lw) / 2
            : baseX + (totalWidth - lw);
      const y = PADDING + strokePad + i * (normalized.lineHeight * fontPx);
      ctx.fillText(lines[i], x, y);
      ctx.strokeText(lines[i], x, y);
    }
  } else if (strokeEnabled && strokeAlign === "inside") {
    // Inside: fillText first, then strokeText (2x width) clipped inside the fill via source-atop
    for (let i = 0; i < lines.length; i++) {
      const lw = lineWidths[i];
      const x =
        normalized.align === "left"
          ? baseX
          : normalized.align === "center"
            ? baseX + (totalWidth - lw) / 2
            : baseX + (totalWidth - lw);
      const y = PADDING + strokePad + i * (normalized.lineHeight * fontPx);
      ctx.fillText(lines[i], x, y);
    }
    ctx.globalCompositeOperation = "source-atop";
    for (let i = 0; i < lines.length; i++) {
      const lw = lineWidths[i];
      const x =
        normalized.align === "left"
          ? baseX
          : normalized.align === "center"
            ? baseX + (totalWidth - lw) / 2
            : baseX + (totalWidth - lw);
      const y = PADDING + strokePad + i * (normalized.lineHeight * fontPx);
      ctx.strokeText(lines[i], x, y);
    }
    ctx.globalCompositeOperation = "source-over";
  } else {
    // No stroke: fillText only
    for (let i = 0; i < lines.length; i++) {
      const lw = lineWidths[i];
      const x =
        normalized.align === "left"
          ? baseX
          : normalized.align === "center"
            ? baseX + (totalWidth - lw) / 2
            : baseX + (totalWidth - lw);
      const y = PADDING + strokePad + i * (normalized.lineHeight * fontPx);
      ctx.fillText(lines[i], x, y);
    }
  }

  return { imageBitmap: toBitmap(canvas), width: canvasW / effScale, height: canvasH / effScale };
}