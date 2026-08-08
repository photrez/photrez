// Pure data model for text layers.
// Zero imports by design (extraction-first): this module must be liftable into
// a standalone library without dragging in engine/UI/Tauri dependencies.

export interface TextData {
  content: string; // may be "" (valid pre-commit state)
  fontFamily: string; // user-selected, may be "Arial" default
  fontSize: number; // 1..2000
  fontWeight: number; // 100..900, 100 steps
  fontStyle: "normal" | "italic";
  color: string; // "#RRGGBB"
  align: "left" | "center" | "right";
  lineHeight: number; // 0.5..5.0
  letterSpacing: number; // -100..500
  boxMode: "point" | "area";
  boxWidth: number; // >0 only when boxMode === "area"; 0 sentinel for "point"
}

export const DEFAULT_TEXT_DATA: TextData = {
  content: "",
  fontFamily: "Arial",
  fontSize: 48,
  fontWeight: 400,
  fontStyle: "normal",
  color: "#000000",
  align: "left",
  lineHeight: 1.4,
  letterSpacing: 0,
  boxMode: "point",
  boxWidth: 0,
};

const HEX_COLOR_RE = /^#[0-9A-Fa-f]{6}$/;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function finiteNumber(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** Normalizes arbitrary input into a valid TextData by clamping, never throwing. */
export function normalizeTextData(raw: unknown): TextData {
  const src = isRecord(raw) ? raw : {};
  const boxMode: TextData["boxMode"] = src.boxMode === "area" ? "area" : "point";
  const fontSize = clamp(finiteNumber(src.fontSize, DEFAULT_TEXT_DATA.fontSize), 1, 2000);
  const fontWeight = clamp(
    Math.round(finiteNumber(src.fontWeight, DEFAULT_TEXT_DATA.fontWeight) / 100) * 100,
    100,
    900,
  );
  const lineHeight = clamp(finiteNumber(src.lineHeight, DEFAULT_TEXT_DATA.lineHeight), 0.5, 5);
  const letterSpacing = clamp(finiteNumber(src.letterSpacing, DEFAULT_TEXT_DATA.letterSpacing), -100, 500);
  const color =
    typeof src.color === "string" && HEX_COLOR_RE.test(src.color) ? src.color : DEFAULT_TEXT_DATA.color;
  return {
    content: typeof src.content === "string" ? src.content : "",
    fontFamily: typeof src.fontFamily === "string" ? src.fontFamily : DEFAULT_TEXT_DATA.fontFamily,
    fontSize,
    fontWeight,
    fontStyle: src.fontStyle === "italic" ? "italic" : "normal",
    color,
    align: src.align === "center" || src.align === "right" ? src.align : "left",
    lineHeight,
    letterSpacing,
    boxMode,
    boxWidth: boxMode === "area" ? Math.max(1, finiteNumber(src.boxWidth, 1)) : 0,
  };
}
