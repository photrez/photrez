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
//      Runs in a Web Worker (printWorker.ts) so GPU readback (getImageData)
//      does not block the main thread. Falls back to main-thread execution
//      when Worker is unavailable (jsdom test env, older runtimes).
//      Exports as raw RGBA pixels — no format encoding.
//   3. invoke("print_image_raw") — sends raw RGBA pixels directly to
//      Rust via Tauri v2 raw IPC (Uint8Array body, dimensions + printer
//      settings in headers). Rust passes them straight to GDI
//      (render_rgba_to_printer) with zero decode. No PNG, no JPEG,
//      no temp file, no base64, no disk I/O.
//
// AbortSignal: caller can cancel an in-flight print job. The worker
// respects the signal between operations. The fallback path checks the
// signal at each step.

import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { encodeComposite } from "./exportDocument";
import { showToast } from "./Toast";
import { MM_PER_INCH, TARGET_PRINT_DPI, MAX_PX } from "./print/printTypes";
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

interface PrintCompositeSettings {
  paperWidthMm: number;
  paperHeightMm: number;
  marginLeftMm: number;
  marginRightMm: number;
  marginTopMm: number;
  marginBottomMm: number;
  scalePercent: number;
  centerImage: boolean;
  leftOffsetMm: number;
  topOffsetMm: number;
  targetDpi: number;
  maxPx: number;
}

/** Guard against "Invalid array length" RangeErrors — NaN/Infinity/zero/oversize
 *  canvas dimensions crash OffscreenCanvas, and pixel budgets beyond typed-array
 *  limits make Uint8Array allocation fail. Keeps the failure readable, not cryptic. */
function assertValidPrintDims(w: number, h: number, tag: string): void {
  const valid =
    Number.isFinite(w) && Number.isFinite(h) && w >= 1 && h >= 1 &&
    w <= MAX_PX && h <= MAX_PX && w * h * 4 <= 0x1_0000_0000; // 2^32-1 byte
  if (!valid) {
    throw new Error(
      `Invalid print composite dimensions ${w}x${h}px (${tag}) — exceeds safe canvas limits`,
    );
  }
}

// ── Web Worker compositing ────────────────────────────────────────────

interface WorkerResult {
  type: "result";
  id: string;
  rawPixels: Uint8Array;
  width: number;
  height: number;
}

interface WorkerError {
  type: "error";
  id: string;
  error: string;
}

type WorkerResponse = WorkerResult | WorkerError;

/** Send compositing work to printWorker.ts. Falls back to main-thread
 *  compositing when Worker is unavailable (jsdom test env, older runtimes). */
async function compositeWithWorker(
  pngBytes: Uint8Array,
  settings: PrintCompositeSettings,
  signal?: AbortSignal,
): Promise<{ bytes: Uint8Array; width: number; height: number }> {
  if (typeof Worker === "undefined") {
    return compositeFallback(pngBytes, settings, signal);
  }

  const id = `print-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

  const worker = new Worker(
    new URL("./print/printWorker.ts", import.meta.url),
    { type: "module" },
  );

  return new Promise<{ bytes: Uint8Array; width: number; height: number }>((resolve, reject) => {
    const onAbort = () => {
      worker.terminate();
      reject(new DOMException("Print was cancelled", "AbortError"));
    };

    if (signal?.aborted) {
      worker.terminate();
      reject(new DOMException("Print was cancelled", "AbortError"));
      return;
    }
    const abortHandler = signal ? () => onAbort() : null;
    if (abortHandler && signal) {
      signal.addEventListener("abort", abortHandler, { once: true });
    }

    const cleanup = () => {
      if (abortHandler) signal?.removeEventListener("abort", abortHandler);
    };

    worker.onmessage = (e: MessageEvent<WorkerResponse>) => {
      const msg = e.data;
      if (msg.type === "result" && msg.id === id) {
        cleanup();
        worker.terminate();
        resolve({ bytes: msg.rawPixels, width: msg.width, height: msg.height });
      } else if (msg.type === "error" && msg.id === id) {
        cleanup();
        worker.terminate();
        reject(new Error(`Print compositing failed: ${msg.error}`));
      }
    };

    worker.onerror = (err) => {
      cleanup();
      worker.terminate();
      reject(new Error(`Print worker error: ${err.message}`));
    };

    // Transfer the PNG buffer to the worker (zero-copy)
    worker.postMessage(
      { type: "composite", id, pngBytes, settings },
      { transfer: [pngBytes.buffer] },
    );
  });
}

// ── Fallback: main-thread compositing (test env, no Worker) ───────────

/** Create a Canvas2D with fallback when OffscreenCanvas is unavailable.
 *  Only used in fallback path (Worker unavailable). */
function createPrintCanvas(w: number, h: number): { canvas: HTMLCanvasElement | OffscreenCanvas; ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D } {
  if (typeof OffscreenCanvas !== "undefined") {
    const c = new OffscreenCanvas(w, h);
    // willReadFrequently: we call getImageData() once after rendering —
    // hint the browser to keep pixels CPU-accessible for faster readback.
    const ctx = c.getContext("2d", { willReadFrequently: true })!;
    return { canvas: c, ctx };
  }
  // Fallback for environments without OffscreenCanvas (test, headless, older browser)
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!;
  return { canvas: c, ctx };
}

/** Main-thread compositing fallback. In production, compositeWithWorker uses
 *  printWorker.ts to keep GPU readback off the main thread. */
async function compositeFallback(
  srcBytes: Uint8Array,
  s: PrintCompositeSettings,
  signal?: AbortSignal,
): Promise<{ bytes: Uint8Array; width: number; height: number }> {
  signal?.throwIfAborted();

  // Decode PNG bytes to ImageBitmap via Blob
  const blob = new Blob([srcBytes as BlobPart], { type: "image/png" });
  const img = await createImageBitmap(blob);
  try {
    signal?.throwIfAborted();

    const printW = Math.max(1, s.paperWidthMm - s.marginLeftMm - s.marginRightMm);
    const printH = Math.max(1, s.paperHeightMm - s.marginTopMm - s.marginBottomMm);

    const pixelW = Math.round((printW / MM_PER_INCH) * s.targetDpi);
    const pixelH = Math.round((printH / MM_PER_INCH) * s.targetDpi);

    const canvasW = Math.min(pixelW, s.maxPx);
    const canvasH = Math.min(pixelH, s.maxPx);

    // Guard "Invalid array length" RangeErrors from NaN/Infinity/oversize dims.
    assertValidPrintDims(canvasW, canvasH, `paper ${s.paperWidthMm}x${s.paperHeightMm}mm dpi ${s.targetDpi}`);

    const { canvas, ctx } = createPrintCanvas(canvasW, canvasH);

    signal?.throwIfAborted();

    ctx.fillStyle = "#FFFFFF";
    ctx.fillRect(0, 0, canvasW, canvasH);

    const scaleFactor = s.scalePercent / 100;
    const scaledW = Math.round(img.width * scaleFactor);
    const scaledH = Math.round(img.height * scaleFactor);

    let left: number;
    let top: number;
    if (s.centerImage) {
      const paperPx = Math.round((s.paperWidthMm / MM_PER_INCH) * s.targetDpi);
      const paperHPx = Math.round((s.paperHeightMm / MM_PER_INCH) * s.targetDpi);
      const marginLeftPx = Math.round((s.marginLeftMm / MM_PER_INCH) * s.targetDpi);
      const marginTopPx = Math.round((s.marginTopMm / MM_PER_INCH) * s.targetDpi);
      left = Math.floor((paperPx - scaledW) / 2) - marginLeftPx;
      top = Math.floor((paperHPx - scaledH) / 2) - marginTopPx;
    } else {
      left = Math.round(((s.leftOffsetMm - s.marginLeftMm) / MM_PER_INCH) * s.targetDpi);
      top = Math.round(((s.topOffsetMm - s.marginTopMm) / MM_PER_INCH) * s.targetDpi);
    }

    ctx.drawImage(img, left, top, scaledW, scaledH);

    signal?.throwIfAborted();

    const imageData = ctx.getImageData(0, 0, canvasW, canvasH);
    const bytes = new Uint8Array(imageData.data.buffer, imageData.data.byteOffset, imageData.data.byteLength);

    return { bytes, width: canvasW, height: canvasH };
  } finally {
    img.close();
  }
}

// ── Public API ────────────────────────────────────────────────────────

/** Print the document on a system printer.
 *
 *  @param engine - document engine with layer data
 *  @param docName - optional document name for the print spooler
 *  @param signal - optional AbortSignal to cancel the print job
 *  @throws AbortError if cancelled via signal
 *  @throws Error if print fails
 *  @returns false if the user cancelled the PDF output dialog (no job sent) */
export async function printDocument(
  engine: DocumentEngine,
  docName?: string,
  signal?: AbortSignal,
): Promise<boolean> {
  signal?.throwIfAborted();

  try {
    // 1. Fetch current settings from Rust
    const raw = await invoke<{ ok: boolean; data?: RustPrintSettings }>("get_print_settings");
    signal?.throwIfAborted();
    const s = raw.data ?? (raw as unknown as RustPrintSettings);

    // Validate printer selection early
    if (!s.selected_printer) {
      showToast("No printer selected. Select a printer in Print Settings.", "warn");
      return false;
    }

    // Compute effective dimensions for landscape output.
    // Rust stores paper_width_mm / paper_height_mm in canonical portrait form
    // (width ≤ height).  For landscape printing we swap them for the canvas.
    const effW = s.orientation === "landscape" ? s.paper_height_mm : s.paper_width_mm;
    const effH = s.orientation === "landscape" ? s.paper_width_mm : s.paper_height_mm;

    // 2. Render layers onto document-sized canvas (GPU-accelerated)
    const rawImageBytes = await encodeComposite(engine, "png", 100);
    signal?.throwIfAborted();

    // 3. Determine print DPI — prefer printer-native DPI, fallback to 300.
    //    Compositing at printer DPI means StretchDIBits sees src == dst
    //    size, triggering the 1:1 GDI fast path (no CPU scaling).
    //    Cap at 300 DPI (industry standard for photo print; browsers
    //    rasterize at 300 too). PDF drivers report 600 DPI → 4× buffer
    //    (139MB vs 35MB for A4) with no visible gain at normal viewing
    //    distance — GDI scales 300 → printer DPI cheaply.
    const dpi = Math.min(s.printer_dpi ?? TARGET_PRINT_DPI, TARGET_PRINT_DPI);

    // Apply MAX_PX clamp: if the canvas at this DPI exceeds the safety
    // limit, proportionally reduce DPI so the longest dimension fits.
    const maxDimPx = Math.round((Math.max(effW, effH) / MM_PER_INCH) * dpi);
    const effectiveDpi = maxDimPx > MAX_PX
      ? ((MAX_PX / maxDimPx) * dpi)
      : dpi;

    // 4. Use per-side margins for compositing
    const mL = s.margin_left_mm ?? s.margin_mm;
    const mR = s.margin_right_mm ?? s.margin_mm;
    const mT = s.margin_top_mm ?? s.margin_mm;
    const mB = s.margin_bottom_mm ?? s.margin_mm;

    // 5. Composite onto paper-sized canvas at target DPI.
    //    Uses printWorker.ts → GPU readback off main thread.
    //    Falls back to main-thread composite when Worker unavailable.
    const settings: PrintCompositeSettings = {
      paperWidthMm: effW,
      paperHeightMm: effH,
      marginLeftMm: mL,
      marginRightMm: mR,
      marginTopMm: mT,
      marginBottomMm: mB,
      scalePercent: s.scale_percent,
      centerImage: s.center_image,
      leftOffsetMm: s.left_offset_mm,
      topOffsetMm: s.top_offset_mm,
      targetDpi: effectiveDpi,
      maxPx: MAX_PX,
    };

    const { bytes, width, height } = await compositeWithWorker(rawImageBytes, settings, signal);
    signal?.throwIfAborted();

    // Print-to-PDF drivers (PORTPROMPT, e.g. "Microsoft Print to PDF") fail
    // StartDocW with ERROR_ACCESS_DENIED when lpszOutput is NULL — they require
    // an explicit output path. Ask the user where to save the PDF here.
    let outputPath: string | undefined;
    if (/print to pdf|printtopdf/i.test(s.selected_printer)) {
      const chosen = await save({
        defaultPath: `${(docName || "print").replace(/\.[^.]+$/, "")}.pdf`,
        filters: [{ name: "PDF Document (*.pdf)", extensions: ["pdf"] }],
      });
      if (!chosen) {
        // User cancelled the save dialog — no print job, keep the print
        // dialog open (caller treats `false` as "not sent").
        return false;
      }
      outputPath = chosen;
    }

    // 6. Send raw RGBA pixels directly to Rust via Tauri v2 raw IPC.
    //    Raw pixels (no format encoding) in body, dimensions + printer
    //    settings in headers. Rust passes straight to GDI with zero decode.
    await invoke("print_image_raw", bytes, {
      headers: {
        printer: s.selected_printer,
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
        ...(outputPath ? { outputPath } : {}),
      },
    });
    showToast(
      outputPath ? `PDF saved: ${outputPath.split(/[/\\]/).pop() ?? outputPath}` : "Print job sent to printer",
      "info",
    );
    return true;
  } catch (err) {
    // M3: Distinguish AbortError — silent, no toast
    if (err instanceof DOMException && err.name === "AbortError") {
      console.log("[PRINT] Cancelled by user");
      return false;
    }

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
