// SPDX-License-Identifier: AGPL-3.0-or-later
// --- Pro Print Settings Types ---

export type PaperPresetId = "A4" | "Letter" | "A3" | "Legal" | "A5" | "Custom";
export type PrintUnit = "in" | "cm" | "mm" | "px";
export type PrintOrientation = "portrait" | "landscape";
export type PPIQualityLevel = "optimal" | "draft" | "low";

export interface PaperDimensions {
  widthMm: number;
  heightMm: number;
  label: string;
}

export const PAPER_PRESETS: Record<PaperPresetId, PaperDimensions> = {
  A4: { widthMm: 210, heightMm: 297, label: "A4 (210 × 297 mm)" },
  Letter: { widthMm: 215.9, heightMm: 279.4, label: "Letter (8.5 × 11 in)" },
  A3: { widthMm: 297, heightMm: 420, label: "A3 (297 × 420 mm)" },
  Legal: { widthMm: 215.9, heightMm: 355.6, label: "Legal (8.5 × 14 in)" },
  A5: { widthMm: 148, heightMm: 210, label: "A5 (148 × 210 mm)" },
  Custom: { widthMm: 210, heightMm: 297, label: "Custom" },
};

export interface PrintOptions {
  selectedPrinter: string;
  copies: number;
  orientation: PrintOrientation;
  paperPreset: PaperPresetId;
  paperWidthMm: number;
  paperHeightMm: number;
  marginMm: number;
  
  // Color management (placeholder)
  colorHandling: "printer_manages" | "photrez_manages";
  renderingIntent: "perceptual" | "relative" | "saturation";
  blackPointCompensation: boolean;
  
  // Position & Size
  centerImage: boolean;
  topOffsetMm: number;
  leftOffsetMm: number;
  scalePercent: number;
  scaleToFit: boolean;
  unit: PrintUnit;
  
  // UX options
  showPaperWhite: boolean;
}

export const DEFAULT_PRINT_OPTIONS: PrintOptions = {
  selectedPrinter: "",
  copies: 1,
  orientation: "portrait",
  paperPreset: "A4",
  paperWidthMm: 210,
  paperHeightMm: 297,
  marginMm: 5,
  
  colorHandling: "printer_manages",
  renderingIntent: "perceptual",
  blackPointCompensation: true,
  
  centerImage: true,
  topOffsetMm: 0,
  leftOffsetMm: 0,
  scalePercent: 100,
  scaleToFit: false,
  unit: "cm",
  
  showPaperWhite: true,
};
