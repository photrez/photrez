/// <reference lib="webworker" />
// apps/desktop/src/components/editor/print/printWorker.ts
//
// Web Worker for print compositing.
// Receives PNG-encoded document + print settings, composites onto
// a paper-sized OffscreenCanvas, and returns raw RGBA pixels.
//
// Running OffscreenCanvas + getImageData inside a Worker keeps the
// GPU readback (getImageData) off the main thread, preventing UI
// stutter during print.
//
// Message protocol:
//   → { type: "composite", id, pngBytes, settings }
//   ← { type: "result", id, rawPixels: Uint8Array, width, height }
//   ← { type: "error", id, error: string }

interface PrintSettings {
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

interface CompositeRequest {
  type: "composite";
  id: string;
  pngBytes: Uint8Array;
  settings: PrintSettings;
}

const MM_PER_INCH = 25.4;

self.onmessage = async (e: MessageEvent<CompositeRequest>) => {
  const { type, id, pngBytes, settings } = e.data;

  if (type !== "composite") return;

  try {
    const result = await compositeInWorker(pngBytes, settings);

    // Transfer the raw pixels buffer for zero-copy delivery
    self.postMessage(
      { type: "result", id, rawPixels: result.bytes, width: result.width, height: result.height },
      { transfer: [result.bytes.buffer] },
    );
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    self.postMessage({ type: "error", id, error: message });
  }
};

async function compositeInWorker(
  pngBytes: Uint8Array,
  s: PrintSettings,
): Promise<{ bytes: Uint8Array; width: number; height: number }> {
  // ── 1. Decode PNG bytes to ImageBitmap ──────────────
  const blob = new Blob([pngBytes as BlobPart], { type: "image/png" });
  const img = await createImageBitmap(blob);

  try {
    // ── 2. Compute printable area ─────────────────────
    const printW = Math.max(1, s.paperWidthMm - s.marginLeftMm - s.marginRightMm);
    const printH = Math.max(1, s.paperHeightMm - s.marginTopMm - s.marginBottomMm);

    const pixelW = Math.round((printW / MM_PER_INCH) * s.targetDpi);
    const pixelH = Math.round((printH / MM_PER_INCH) * s.targetDpi);

    const canvasW = Math.min(pixelW, s.maxPx);
    const canvasH = Math.min(pixelH, s.maxPx);

    // ── 3. OffscreenCanvas compositing ────────────────
    const canvas = new OffscreenCanvas(canvasW, canvasH);
    // willReadFrequently hint: we call getImageData() once at the end.
    // Keeps pixel data CPU-accessible for faster GPU→CPU readback.
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    if (!ctx) {
      throw new Error("OffscreenCanvas 2d context is null");
    }

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

    // ── 4. Extract raw RGBA pixels ───────────────────
    const imageData = ctx.getImageData(0, 0, canvasW, canvasH);
    const bytes = new Uint8Array(imageData.data.buffer, imageData.data.byteOffset, imageData.data.byteLength);

    return { bytes, width: canvasW, height: canvasH };
  } finally {
    // H2: Always release GPU bitmap on error or success
    img.close();
  }
}
