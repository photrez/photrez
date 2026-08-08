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

function containsCJK(text: string): boolean {
  for (const ch of text) {
    const c = ch.codePointAt(0);
    if (c === undefined) continue;
    if ((c >= 0x4e00 && c <= 0x9fff) || (c >= 0x3040 && c <= 0x30ff) || (c >= 0xac00 && c <= 0xd7af)) {
      return true;
    }
  }
  return false;
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

function wrapWords(paragraph: string, measure: TextMeasurer, maxWidth: number): string[] {
  const lines: string[] = [];
  let current = "";
  for (const word of paragraph.split(" ")) {
    if (word === "") continue;
    const candidate = current === "" ? word : `${current} ${word}`;
    if (measure.measureText(candidate).width <= maxWidth) {
      current = candidate;
      continue;
    }
    if (current !== "") {
      lines.push(current);
      current = "";
    }
    if (measure.measureText(word).width > maxWidth) {
      const chunks = splitWord(word, measure, maxWidth);
      for (let i = 0; i < chunks.length - 1; i++) lines.push(chunks[i]);
      current = chunks[chunks.length - 1];
    } else {
      current = word;
    }
  }
  if (current !== "") lines.push(current);
  return lines.length > 0 ? lines : [""];
}

/** CJK paragraphs wrap per character (rules favor char-level breaks for CJK text). */
function wrapCJK(paragraph: string, measure: TextMeasurer, maxWidth: number): string[] {
  const lines: string[] = [];
  let current = "";
  for (const ch of paragraph) {
    const candidate = current + ch;
    if (current !== "" && measure.measureText(candidate).width > maxWidth) {
      lines.push(current);
      current = ch;
    } else {
      current = candidate;
    }
  }
  if (current !== "") lines.push(current);
  return lines.length > 0 ? lines : [""];
}

/**
 * Paragraph-aware word wrap. "\n" separates paragraphs; empty paragraphs are
 * preserved as empty lines; a word longer than maxWidth is character-breaked.
 * Whitespace within paragraphs is normalized during word wrapping
 * (consecutive spaces collapse); multi-space runs may render collapsed.
 * Never returns [] (empty input yields [""]) and never throws for a
 * non-finite maxWidth.
 */
export function wrapText(measure: TextMeasurer, text: string, maxWidth: number): string[] {
  const width = Number.isFinite(maxWidth) && maxWidth > 0 ? maxWidth : 1;
  const lines: string[] = [];
  for (const paragraph of text.split("\n")) {
    if (paragraph === "") {
      lines.push("");
    } else if (containsCJK(paragraph)) {
      lines.push(...wrapCJK(paragraph, measure, width));
    } else {
      lines.push(...wrapWords(paragraph, measure, width));
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

  const lines =
    normalized.boxMode === "area"
      ? wrapText(ctx, normalized.content, normalized.boxWidth * effScale)
      : normalized.content.split("\n");

  const lineWidths: number[] = [];
  let ascent = fontPx * 0.8;
  let descent = fontPx * 0.2;
  for (const line of lines) {
    const m = ctx.measureText(line);
    lineWidths.push(m.width);
    if (typeof m.fontBoundingBoxAscent === "number" && m.fontBoundingBoxAscent > ascent) {
      ascent = m.fontBoundingBoxAscent;
    }
    if (typeof m.fontBoundingBoxDescent === "number" && m.fontBoundingBoxDescent > descent) {
      descent = m.fontBoundingBoxDescent;
    }
  }
  const maxLineWidth = Math.max(0, ...lineWidths);

  const totalWidth =
    normalized.boxMode === "area" ? normalized.boxWidth * effScale : maxLineWidth;
  const canvasW = Math.min(MAX_CANVAS_DIM, Math.max(1, Math.ceil(totalWidth + PADDING * 2)));
  const canvasH = Math.min(
    MAX_CANVAS_DIM,
    Math.max(1, Math.ceil(lines.length * normalized.lineHeight * fontPx + ascent + descent + PADDING * 2)),
  );
  canvas.width = canvasW;
  canvas.height = canvasH;

  // canvas resize resets context state; re-apply everything.
  ctx.font = buildCSSFont(normalized, fontPx);
  ctx.fillStyle = normalized.color;
  ctx.textBaseline = "top";
  const drawCtx = ctx as OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D;
  if ("letterSpacing" in drawCtx) {
    drawCtx.letterSpacing = `${normalized.letterSpacing}px`;
  }

  for (let i = 0; i < lines.length; i++) {
    const lw = lineWidths[i];
    const x =
      normalized.align === "left"
        ? PADDING
        : normalized.align === "center"
          ? PADDING + (totalWidth - lw) / 2
          : PADDING + (totalWidth - lw);
    ctx.fillText(lines[i], x, PADDING + i * (normalized.lineHeight * fontPx));
  }

  return { imageBitmap: toBitmap(canvas), width: canvasW / effScale, height: canvasH / effScale };
}