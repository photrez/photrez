// SPDX-License-Identifier: AGPL-3.0-or-later
// --- Pro Print Settings Types & Geometry Utilities ---

export type PrintUnit = "in" | "cm" | "mm" | "px";
export type PrintOrientation = "portrait" | "landscape";
export type PPIQualityLevel = "optimal" | "draft" | "low";

export interface PaperDimensions {
  widthMm: number;
  heightMm: number;
  label: string;
}

// ── Geometry & Unit Utilities (moved from printGeometry.ts) ──────

export const MM_PER_INCH = 25.4;
export const CM_PER_INCH = 2.54;
export const DEFAULT_SCREEN_DPI = 96;
export const TARGET_PRINT_DPI = 300;

export function convertMmToUnit(valMm: number, unit: PrintUnit): number {
  switch (unit) {
    case "in":
      return Number((valMm / MM_PER_INCH).toFixed(2));
    case "cm":
      return Number((valMm / 10).toFixed(2));
    case "mm":
      return Number(valMm.toFixed(1));
    case "px":
      return Math.round((valMm / MM_PER_INCH) * TARGET_PRINT_DPI);
  }
}

export function convertUnitToMm(val: number, unit: PrintUnit): number {
  switch (unit) {
    case "in":
      return val * MM_PER_INCH;
    case "cm":
      return val * 10;
    case "mm":
      return val;
    case "px":
      return (val / TARGET_PRINT_DPI) * MM_PER_INCH;
  }
}

export function formatPhysicalDimensions(
  widthMm: number,
  heightMm: number,
  unit: PrintUnit
): string {
  const w = convertMmToUnit(widthMm, unit);
  const h = convertMmToUnit(heightMm, unit);
  const unitLabel = unit === "px" ? "px" : unit;
  return `${w} ${unitLabel} × ${h} ${unitLabel}`;
}

export function getEffectiveOrientation(
  widthMm: number,
  heightMm: number
): PrintOrientation {
  return widthMm >= heightMm ? "landscape" : "portrait";
}

export function calculateEffectivePPI(
  docWidthPx: number,
  docHeightPx: number,
  printWidthMm: number,
  printHeightMm: number
): number {
  if (printWidthMm <= 0 || printHeightMm <= 0) return 0;
  const widthInches = printWidthMm / MM_PER_INCH;
  const heightInches = printHeightMm / MM_PER_INCH;
  const ppiX = docWidthPx / widthInches;
  const ppiY = docHeightPx / heightInches;
  return Math.round(Math.min(ppiX, ppiY));
}

export function getPPIQualityLevel(ppi: number): {
  level: PPIQualityLevel;
  badgeText: string;
  colorClass: string;
} {
  if (ppi >= 300) {
    return {
      level: "optimal",
      badgeText: "Optimal (300+ PPI)",
      colorClass: "text-emerald-400 bg-emerald-950/40 border-emerald-500/30",
    };
  } else if (ppi >= 150) {
    return {
      level: "draft",
      badgeText: "Acceptable (150-299 PPI)",
      colorClass: "text-amber-400 bg-amber-950/40 border-amber-500/30",
    };
  } else {
    return {
      level: "low",
      badgeText: "Low resolution (min 150 PPI recommended)",
      colorClass: "text-rose-400 bg-rose-950/40 border-rose-500/30",
    };
  }
}

export interface CalculateFitResult {
  scalePercent: number;
  printWidthMm: number;
  printHeightMm: number;
  leftOffsetMm: number;
  topOffsetMm: number;
}

export function calculateScaleToFit(
  docWidthPx: number,
  docHeightPx: number,
  paperWidthMm: number,
  paperHeightMm: number,
  marginMm: number = 0
): CalculateFitResult {
  const availW = Math.max(1, paperWidthMm - marginMm * 2);
  const availH = Math.max(1, paperHeightMm - marginMm * 2);

  const safeW = Math.max(1, docWidthPx);
  const safeH = Math.max(1, docHeightPx);
  
  const docAspect = safeW / safeH;
  const availAspect = availW / availH;

  let printW: number;
  let printH: number;

  if (docAspect > availAspect) {
    printW = availW;
    printH = availW / docAspect;
  } else {
    printH = availH;
    printW = availH * docAspect;
  }

  const unscaledWMm = (safeW / TARGET_PRINT_DPI) * MM_PER_INCH;
  const scalePercent = Number(((printW / unscaledWMm) * 100).toFixed(2));

  const leftOffsetMm = Number(((paperWidthMm - printW) / 2).toFixed(1));
  const topOffsetMm = Number(((paperHeightMm - printH) / 2).toFixed(1));

  return {
    scalePercent,
    printWidthMm: Number(printW.toFixed(1)),
    printHeightMm: Number(printH.toFixed(1)),
    leftOffsetMm,
    topOffsetMm,
  };
}

// ── Legacy types (kept for test backward compatibility) ──────────

export interface PrintOptions {
  selectedPrinter: string;
  copies: number;
  orientation: PrintOrientation;
  paperPreset: string;
  paperIndex: number;
  paperWidthMm: number;
  paperHeightMm: number;
  marginMm: number;
  colorHandling: "printer_manages" | "photrez_manages";
  renderingIntent: "perceptual" | "relative" | "saturation" | "absolute";
  blackPointCompensation: boolean;
  centerImage: boolean;
  topOffsetMm: number;
  leftOffsetMm: number;
  scalePercent: number;
  scaleToFit: boolean;
  unit: PrintUnit;
  showPaperWhite: boolean;
}
