// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Print orchestration — image compositing via TS OffscreenCanvas (GPU),
// then Rust GDI dispatch for physical printing.
//
// This follows Tauri best practice: the frontend owns GPU-accelerated
// rendering (Canvas2D), and the Rust backend owns system API calls (GDI).
//
// Flow:
//   1. encodeComposite — render layers onto a document-sized canvas (PNG)
//   2. compositeForPrint — composite onto a paper-sized canvas at
//      printer-native DPI (fallback 300), with MAX_PX clamp.
//      Exports as raw RGBA pixels (via ctx.getImageData) — no format
//      encoding: raw pixels → GDI → printer via Tauri raw IPC.
//   3. invoke("print_image_raw") — sends raw RGBA pixels directly to
//      Rust via Tauri v2 raw IPC (Uint8Array body, dimensions + printer
//      settings in headers). Rust passes them straight to GDI
//      (render_rgba_to_printer) with zero decode. No PNG, no JPEG,
//      no temp file, no base64, no disk I/O.

import { invoke } from "@tauri-apps/api/core";
import { encodeComposite } from "./exportDocument";
import { showToast } from "./Toast";
import type { DocumentEngine } from "@/engine/document";

interface RustPrintSettings {
  selected_printer: string | null;
  copies: number;
  paper_name: string;
  paper_index: number;
  paper_width_mm: number;
  paper_height_mm: number;
  orientation: string;
  margin_mm: number;
  margin_left_mm: number;
  margin_right_mm: number;
  margin_top_mm: number;
  margin_bottom_mm: number;
  scale_to_fit: boolean;
  scale_percent: number;
  center_image: boolean;
  top_offset_mm: number;
  left_offset_mm: number;
  unit: string;
  show_paper_white: boolean;
  printer_dpi?: number | null;
}

const MM_PER_INCH = 25.4;
const TARGET_PRINT_DPI = 300;
const MAX_PX = 10000;

/// Create a Canvas2D with fallback when OffscreenCanvas is unavailable (#3).
function createPrintCanvas(w: number, h: number): { canvas: HTMLCanvasElement | OffscreenCanvas; ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D } {
  if (typeof OffscreenCanvas !== "undefined") {
    const c = new OffscreenCanvas(w, h);
    const ctx = c.getContext("2d")!;
    return { canvas: c, ctx };
  }
  // Fallback for environments without OffscreenCanvas (test, headless, older browser)
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!;
  return { canvas: c, ctx };
}

/// Composite the document image onto a paper-sized white canvas at the target
/// DPI using OffscreenCanvas (GPU-accelerated Canvas2D).
///
/// Returns raw RGBA pixels (via ctx.getImageData) and their dimensions.
/// The canvas is inset by marginMm so the rendered content stays within
/// the printable area — the white background fills the margin border.
/// Returns raw RGBA pixels with zero format encoding.
async function compositeForPrint(
  srcBytes: Uint8Array,
  paperWidthMm: number,
  paperHeightMm: number,
  marginLeftMm: number,
  marginRightMm: number,
  marginTopMm: number,
  marginBottomMm: number,
  scalePercent: number,
  centerImage: boolean,
  leftOffsetMm: number,
  topOffsetMm: number,
  targetDpi: number,
): Promise<{ bytes: Uint8Array; width: number; height: number }> {
  // Decode PNG bytes to ImageBitmap via Blob
  const blob = new Blob([srcBytes as BlobPart], { type: "image/png" });
  const img = await createImageBitmap(blob);

  // Compute printable area dimensions (paper minus per-side margins)
  const printW = Math.max(1, paperWidthMm - marginLeftMm - marginRightMm);
  const printH = Math.max(1, paperHeightMm - marginTopMm - marginBottomMm);

  // Compute canvas pixel dimensions at target DPI (printable area only)
  const pixelW = Math.round((printW / MM_PER_INCH) * targetDpi);
  const pixelH = Math.round((printH / MM_PER_INCH) * targetDpi);

  // Clamp to prevent OOM on absurd paper sizes
  const canvasW = Math.min(pixelW, MAX_PX);
  const canvasH = Math.min(pixelH, MAX_PX);

  // Create canvas — prefer OffscreenCanvas, fallback to regular Canvas (#3)
  const { canvas, ctx } = createPrintCanvas(canvasW, canvasH);

  // Fill white background
  ctx.fillStyle = "#FFFFFF";
  ctx.fillRect(0, 0, canvasW, canvasH);

  // Compute scaled image dimensions
  const scaleFactor = scalePercent / 100;
  const scaledW = Math.round(img.width * scaleFactor);
  const scaledH = Math.round(img.height * scaleFactor);

  // Compute position within the printable area.
  // Preview coordinates are paper-relative (0,0 = paper top-left).
  // Canvas is printable-area-relative (0,0 = top-left of printable area).
  // Subtract margins to convert between coordinate systems (Bug B fix).
  let left: number;
  let top: number;
  if (centerImage) {
    // Center within paper (matches preview), then adjust for printable area offset.
    // Canvas2D drawImage natively clips negative positions (image extends beyond
    // canvas) — no Math.max(0) needed.
    const paperPx = Math.round((paperWidthMm / MM_PER_INCH) * targetDpi);
    const paperHPx = Math.round((paperHeightMm / MM_PER_INCH) * targetDpi);
    const marginLeftPx = Math.round((marginLeftMm / MM_PER_INCH) * targetDpi);
    const marginTopPx = Math.round((marginTopMm / MM_PER_INCH) * targetDpi);
    left = Math.floor((paperPx - scaledW) / 2) - marginLeftPx;
    top = Math.floor((paperHPx - scaledH) / 2) - marginTopPx;
  } else {
    // leftOffsetMm/topOffsetMm are paper-relative → convert to canvas coords
    left = Math.round(((leftOffsetMm - marginLeftMm) / MM_PER_INCH) * targetDpi);
    top = Math.round(((topOffsetMm - marginTopMm) / MM_PER_INCH) * targetDpi);
  }

  // Draw image — hardware-accelerated via Canvas2D GPU backend
  ctx.drawImage(img, left, top, scaledW, scaledH);

  // Cleanup
  img.close();

  // Read raw RGBA pixels from GPU (no format encoding)
  const imageData = ctx.getImageData(0, 0, canvasW, canvasH);
  return {
    bytes: new Uint8Array(imageData.data.buffer, imageData.data.byteOffset, imageData.data.byteLength),
    width: canvasW,
    height: canvasH,
  };
}

export async function printDocument(
  engine: DocumentEngine,
  docName?: string,
): Promise<void> {
  try {
    // 1. Fetch current settings from Rust
    const raw = await invoke<{ ok: boolean; data?: RustPrintSettings }>("get_print_settings");
    const s = raw.data ?? (raw as unknown as RustPrintSettings);

    // Compute effective dimensions for landscape output.
    // Rust stores paper_width_mm / paper_height_mm in canonical portrait form
    // (width ≤ height).  For landscape printing we swap them for the canvas.
    const effW = s.orientation === "landscape" ? s.paper_height_mm : s.paper_width_mm;
    const effH = s.orientation === "landscape" ? s.paper_width_mm : s.paper_height_mm;

    // 2. Render layers onto document-sized canvas (GPU-accelerated)
    const rawImageBytes = await encodeComposite(engine, "png", 100);

    // 3. Determine print DPI — prefer printer-native DPI, fallback to 300.
    //    Compositing at printer DPI means StretchDIBits sees src == dst
    //    size, triggering the 1:1 GDI fast path (no CPU scaling).
    const dpi = s.printer_dpi ?? TARGET_PRINT_DPI;

    // Apply MAX_PX clamp: if the canvas at this DPI exceeds the safety
    // limit, proportionally reduce DPI so the longest dimension fits.
    // This preserves the 1:1 property (both source canvas and printer page
    // are at the same clamped resolution).
    const maxDimPx = Math.round((Math.max(effW, effH) / MM_PER_INCH) * dpi);
    const effectiveDpi = maxDimPx > MAX_PX
      ? ((MAX_PX / maxDimPx) * dpi)
      : dpi;

    // 4. Use per-side margins for compositing (fix #2).
    //    Margin values are physical paper edges, consistent between
    //    frontend and GDI regardless of orientation.
    const mL = s.margin_left_mm ?? s.margin_mm;
    const mR = s.margin_right_mm ?? s.margin_mm;
    const mT = s.margin_top_mm ?? s.margin_mm;
    const mB = s.margin_bottom_mm ?? s.margin_mm;

    // 5. Composite onto paper-sized canvas at target DPI (GPU-accelerated).
    //    Returns raw RGBA pixels — no format encoding.
    const { bytes, width, height } = await compositeForPrint(
      rawImageBytes,
      effW,
      effH,
      mL, mR, mT, mB,
      s.scale_percent,
      s.center_image,
      s.left_offset_mm,
      s.top_offset_mm,
      effectiveDpi,
    );

    // 6. Validate printer selection before dispatch
    if (!s.selected_printer) {
      showToast("No printer selected. Select a printer in Print Settings.", "warn");
      return;
    }

    // 7. Send raw RGBA pixels directly to Rust via Tauri v2 raw IPC.
    //    Raw pixels (no format encoding) in body, dimensions + printer
    //    settings in headers. Rust passes straight to GDI with zero decode.
    await invoke("print_image_raw", bytes, {
      headers: {
        printer: s.selected_printer!,
        copies: String(s.copies || 1),
        paperWidthMm: String(effW),
        paperHeightMm: String(effH),
        marginMm: String(s.margin_mm),
        marginLeftMm: String(mL),
        marginRightMm: String(mR),
        marginTopMm: String(mT),
        marginBottomMm: String(mB),
        paperIndex: String(s.paper_index),
        documentName: docName || "Untitled",
        orientation: s.orientation,
        width: String(width),
        height: String(height),
      },
    });
    showToast("Print job spooled to system printer", "info");
  } catch (err) {
    let msg: string;
    if (err instanceof Error) { msg = err.message; }
    else {
      const e = err as { error?: { message?: string }; message?: string };
      msg = e.error?.message || e.message || "unknown error (see console)";
    }
    console.error("Print error details:", err);
    showToast(`Print failed: ${msg}`, "error");
    throw err;
  }
}
