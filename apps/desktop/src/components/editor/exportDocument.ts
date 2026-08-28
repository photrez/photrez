import type { LayerNode } from "@/engine/types";
import type { DocumentEngine } from "@/engine/document";
import { getEffectiveMaxDim } from "@/engine/types";
import { writeFileBytes, showSaveDialog } from "@/tauri/native";
import { drawLayerToContext } from "@/engine/layerComposite";
import { bakeAdjustmentToBitmap, bakeAdjustmentToBitmapGpu } from "@/engine/layerAdjustments";
import { encodeImageWithWasm } from "./wasmExport";

export type ExportFormat = "png" | "jpeg" | "webp" | "tiff";

function getMimeType(format: ExportFormat): string {
  switch (format) {
    case "jpeg": return "image/jpeg";
    case "webp": return "image/webp";
    case "tiff": return "image/tiff";
    default: return "image/png";
  }
}

function getExtension(format: ExportFormat): string {
  switch (format) {
    case "jpeg": return "jpg";
    case "webp": return "webp";
    case "tiff": return "tiff";
    default: return "png";
  }
}

// Bakes a layer's basic adjustment (Brightness/Contrast/Saturation) into a
// fresh bitmap for export. The live preview applies the same math on the GPU
// (see renderer/shaders.ts :: applyAdjustment), so the exported file matches
// what the user sees. Operates on straight-alpha RGBA (getImageData), which is
// what applyBasicAdjustmentToPixels expects.
async function bakeAdjustment(layer: LayerNode): Promise<ImageBitmap> {
  // Prefer the WGSL compute bake (GPU, realtime); fall back to the CPU bake
  // when WebGPU is absent or errors (headless export, unsupported GPU).
  try {
    return await bakeAdjustmentToBitmapGpu(layer.imageBitmap!, layer.width, layer.height, layer.basicAdjustment!);
  } catch {
    return bakeAdjustmentToBitmap(layer.imageBitmap!, layer.width, layer.height, layer.basicAdjustment!);
  }
}

export async function encodeComposite(
  engine: DocumentEngine,
  format: ExportFormat,
  quality: number,
): Promise<Uint8Array> {
  const t0 = performance.now();
  const width = Math.min(engine.getWidth(), getEffectiveMaxDim());
  const height = Math.min(engine.getHeight(), getEffectiveMaxDim());
  const layers = engine.getLayers();
  const clampedQuality = Math.max(0, Math.min(1, quality / 100));

  const canvas = new OffscreenCanvas(width, height);
  const ctx = canvas.getContext("2d")!;

  // Start with transparent canvas
  ctx.clearRect(0, 0, width, height);

  // JPEG does not support alpha — composite over white
  if (format === "jpeg") {
    ctx.fillStyle = "#FFFFFF";
    ctx.fillRect(0, 0, width, height);
  }

  // C5.4 bitmap sync: ensure all layer bitmaps reflect Rust canonical before
  // compositing for export.  This is the smallest correct boundary — sync only
  // when the consumer actually needs canonical pixels.
  const docId = engine.getId();
  for (const layer of layers) {
    if (layer.visible && layer.imageBitmap) {
      await engine.ensureBitmapCurrent(docId, layer.id);
    }
  }

  // Composite layers bottom-to-top using the same drawLayerToContext
  // that the frontend renderer uses — ensures blend mode parity.
  // Adjustments are non-destructive (applied in the GPU shader live), so we
  // bake them into a temporary bitmap here for the exported pixels.
  for (let i = layers.length - 1; i >= 0; i--) {
    const layer = layers[i];
    if (!layer.visible || !layer.imageBitmap) continue;
    if (layer.basicAdjustment) {
      const adjusted = await bakeAdjustment(layer);
      drawLayerToContext(ctx, { ...layer, imageBitmap: adjusted });
      adjusted.close();
    } else {
      drawLayerToContext(ctx, layer);
    }
  }

  // Attempt WASM encoding first for zero-copy performance
  const imageData = ctx.getImageData(0, 0, width, height);
  const rgbaBytes = new Uint8Array(imageData.data.buffer, imageData.data.byteOffset, imageData.data.byteLength);
  const wasmResult = await encodeImageWithWasm(width, height, rgbaBytes, format, quality);
  if (wasmResult) {
    return wasmResult;
  }

  // Fallback to Canvas convertToBlob if WASM module unavailable
  const mimeType = getMimeType(format);
  const blob = await canvas.convertToBlob({
    type: mimeType,
    quality: format !== "png" && format !== "tiff" ? clampedQuality : undefined,
  });

  const bytes = new Uint8Array(await blob.arrayBuffer());
  return bytes;
}

export async function exportActiveDocument(
  engine: DocumentEngine,
  displayName: string,
  format: ExportFormat,
  quality: number,
): Promise<string | null> {
  const ext = getExtension(format);
  const defaultName = displayName.replace(/\.[^.]+$/, "") + "." + ext;

  const path = await showSaveDialog(defaultName);
  if (!path) return null;

  const bytes = await encodeComposite(engine, format, quality);
  await writeFileBytes(path, bytes);

  return path;
}

// ── Persisted last-used quality (per format) ────────────────────────────────
// The quality dialog is shown only the first time a lossy format is saved; the
// choice is remembered here so subsequent saves of that format write directly.
// "Save As" always asks and also updates this value. PNG is lossless.
const QUALITY_KEY_PREFIX = "photrez.quality.";

export function getSavedQuality(format: ExportFormat): number | null {
  if (format === "png" || format === "tiff") return null; // lossless, never prompted
  try {
    const raw = localStorage.getItem(QUALITY_KEY_PREFIX + format);
    if (raw === null) return null;
    const n = parseInt(raw, 10);
    return Number.isFinite(n) ? Math.max(1, Math.min(100, n)) : null;
  } catch {
    return null;
  }
}

export function setSavedQuality(format: ExportFormat, quality: number): void {
  if (format === "png" || format === "tiff") return;
  try {
    localStorage.setItem(QUALITY_KEY_PREFIX + format, String(quality));
  } catch {
    // ignore storage failures (private mode / quota) — save still proceeds
  }
}
