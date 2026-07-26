// SPDX-License-Identifier: AGPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { join, tempDir } from "@tauri-apps/api/path";
import { encodeComposite } from "./exportDocument";
import { writeFileBytes, deleteFile } from "@/tauri/native";
import { showToast } from "./Toast";
import type { DocumentEngine } from "@/engine/document";

// Constants (formerly imported from printGeometry.ts, now inlined)
const TARGET_PRINT_DPI = 300;
const MM_PER_INCH = 25.4;

interface RustPrintSettings {
  selected_printer: string | null;
  copies: number;
  paper_name: string;
  paper_index: number;
  paper_width_mm: number;
  paper_height_mm: number;
  orientation: string;
  margin_mm: number;
  scale_to_fit: boolean;
  scale_percent: number;
  center_image: boolean;
  top_offset_mm: number;
  left_offset_mm: number;
  unit: string;
  show_paper_white: boolean;
}

export async function printDocument(
  engine: DocumentEngine,
  docName?: string,
): Promise<void> {
  let filePath: string | null = null;

  try {
    // 1. Fetch current settings from Rust
    const raw = await invoke<{ ok: boolean; data?: RustPrintSettings }>("get_print_settings");
    const s = raw.data ?? (raw as unknown as RustPrintSettings);

    // 2. Render base document composite image
    const rawImageBytes = await encodeComposite(engine, "png", 100);

    let paperBytes: Uint8Array = rawImageBytes;

    // 3. High-DPI Smart Composition (resilient against mock/test environments)
    try {
      const blob = new Blob([rawImageBytes as BlobPart], { type: "image/png" });
      const dpi = TARGET_PRINT_DPI;
      const mmToInch = MM_PER_INCH;

      // Guard against excessive canvas dimensions (max ~40 inches = ~12000px at 300dpi)
      const MAX_PAPER_MM = 1200;
      const MAX_PX = 10000;
      const clampedPaperW = Math.min(s.paper_width_mm, MAX_PAPER_MM);
      const clampedPaperH = Math.min(s.paper_height_mm, MAX_PAPER_MM);

      let paperPixelWidth = Math.round(Math.max(1, (clampedPaperW / mmToInch) * dpi));
      let paperPixelHeight = Math.round(Math.max(1, (clampedPaperH / mmToInch) * dpi));

      if (paperPixelWidth > MAX_PX || paperPixelHeight > MAX_PX) {
        const scale = MAX_PX / Math.max(paperPixelWidth, paperPixelHeight);
        paperPixelWidth = Math.round(paperPixelWidth * scale);
        paperPixelHeight = Math.round(paperPixelHeight * scale);
      }

      let canvas: HTMLCanvasElement | OffscreenCanvas | null = null;
      let ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null = null;

      if (typeof OffscreenCanvas !== "undefined") {
        canvas = new OffscreenCanvas(paperPixelWidth, paperPixelHeight);
        ctx = canvas.getContext("2d") as OffscreenCanvasRenderingContext2D;
      } else if (typeof document !== "undefined") {
        const el = document.createElement("canvas");
        el.width = paperPixelWidth;
        el.height = paperPixelHeight;
        canvas = el;
        ctx = el.getContext("2d");
      }

      if (canvas && ctx) {
        ctx.fillStyle = "#ffffff";
        ctx.fillRect(0, 0, paperPixelWidth, paperPixelHeight);

        const docW = engine.getWidth();
        const docH = engine.getHeight();

        const scaleFactor = s.scale_percent / 100;
        // Explicit mm-through conversion for physical print sizing
        const unscaledWMm = (docW / dpi) * mmToInch;
        const unscaledHMm = (docH / dpi) * mmToInch;
        const scaledWMm = unscaledWMm * scaleFactor;
        const scaledHMm = unscaledHMm * scaleFactor;
        const imgPixelW = Math.round((scaledWMm / mmToInch) * dpi);
        const imgPixelH = Math.round((scaledHMm / mmToInch) * dpi);

        let leftPx = Math.round((s.left_offset_mm / mmToInch) * dpi);
        let topPx = Math.round((s.top_offset_mm / mmToInch) * dpi);

        if (s.center_image) {
          leftPx = Math.round((paperPixelWidth - imgPixelW) / 2);
          topPx = Math.round((paperPixelHeight - imgPixelH) / 2);
        }

        if (typeof createImageBitmap !== "undefined") {
          const imgBitmap = await createImageBitmap(blob);
          ctx.drawImage(imgBitmap, leftPx, topPx, imgPixelW, imgPixelH);
          imgBitmap.close();

          if ("convertToBlob" in canvas) {
            const compositeBlob = await (canvas as OffscreenCanvas).convertToBlob({
              type: "image/png",
              quality: 1.0,
            });
            const arrayBuffer = await compositeBlob.arrayBuffer();
            paperBytes = new Uint8Array(arrayBuffer);
          } else if (canvas instanceof HTMLCanvasElement && typeof canvas.toBlob === "function") {
            const compositeBlob = await new Promise<Blob>((resolve, reject) => {
              canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("toBlob returned null"))), "image/png", 1.0);
            });
            const arrayBuffer = await compositeBlob.arrayBuffer();
            paperBytes = new Uint8Array(arrayBuffer);
          }
        }
      }
    } catch {
      paperBytes = rawImageBytes;
    }

    // 4. Save paper composite to temp PNG file
    const tmpDir = await tempDir();
    const rand = Math.random().toString(36).slice(2, 8);
    const filename = `photrez-print-${Date.now()}-${rand}.png`;
    filePath = await join(tmpDir, filename);
    await writeFileBytes(filePath, paperBytes);

    // 5. Validate printer selection before dispatch
    if (!s.selected_printer) {
      showToast("No printer selected. Select a printer in Print Settings.", "warn");
      return;
    }

    // 6. Dispatch to native printer spooler
    await invoke("print_image", {
      path: filePath,
      printer: s.selected_printer || null,
      copies: s.copies || 1,
      paperWidthMm: s.paper_width_mm,
      paperHeightMm: s.paper_height_mm,
      paperPreset: s.paper_name,
      paperIndex: s.paper_index,
      documentName: docName || "Untitled",
    });
    showToast("Print job spooled to system printer", "info");

    // 6. Clean up temp file
    try {
      await deleteFile(filePath);
    } catch (cleanupErr) {
      console.error("Failed to clean up temp print file:", cleanupErr);
    }
    filePath = null;
  } catch (err) {
    if (filePath) {
      try {
        await deleteFile(filePath);
      } catch (cleanupErr) {
        console.error("Failed to clean up temp print file:", cleanupErr);
      }
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
