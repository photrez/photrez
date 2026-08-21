// WASM acceleration facade (Phase 4). Importing it does not eagerly load the
// WASM pkg — `getWasmExportModule` only fetches the compiled module on first use.
import { applyBasicAdjustmentWithWasm, getWasmExportModule } from "@/components/editor/wasmExport";
import { adjustRgba } from "@/lib/gpu/gpuCompute";
import { renderLayerPixelsRust } from "@/lib/rustEngineC";

export type BasicAdjustment = {
  brightness: number;
  contrast: number;
  saturation: number;
};

const clampChannel = (value: number): number => {
  if (value < 0) return 0;
  if (value > 255) return 255;
  // rounds to the nearest integer, so an explicit round is redundant.
  return value;
};

const clamp01 = (value: number): number => {
  if (value < 0) return 0;
  if (value > 1) return 1;
  return value;
};

const clampPercent = (value: number): number => {
  if (value < -100) return -100;
  if (value > 100) return 100;
  return value;
};

export function normalizeBasicAdjustment(adjustment: BasicAdjustment): BasicAdjustment {
  return {
    brightness: clampPercent(adjustment.brightness),
    contrast: clampPercent(adjustment.contrast),
    saturation: clampPercent(adjustment.saturation),
  };
}

/**
 * Pure per-channel RGB adjustment in [0,1] straight-alpha space. Mirrors
 * `renderer/shaders.ts::applyAdjustment` so the CPU bake and GPU preview agree.
 */
export function applyAdjustmentToRgb(
  r: number,
  g: number,
  b: number,
  adj: BasicAdjustment,
): [number, number, number] {
  const contrastFactor = (259 * (adj.contrast + 255)) / (255 * (259 - adj.contrast));
  let cr = contrastFactor * (r - 0.5) + 0.5;
  let cg = contrastFactor * (g - 0.5) + 0.5;
  let cb = contrastFactor * (b - 0.5) + 0.5;

  const t = adj.brightness / 100;
  if (t >= 0) {
    cr = cr + (1 - cr) * t * 0.5;
    cg = cg + (1 - cg) * t * 0.5;
    cb = cb + (1 - cb) * t * 0.5;
  } else {
    const f = -t;
    cr = cr - cr * f * 0.5;
    cg = cg - cg * f * 0.5;
    cb = cb - cb * f * 0.5;
  }

  const luminance = cr * 0.2126 + cg * 0.7152 + cb * 0.0722;
  const satFactor = 1 + adj.saturation / 100;
  cr = luminance + (cr - luminance) * satFactor;
  cg = luminance + (cg - luminance) * satFactor;
  cb = luminance + (cb - luminance) * satFactor;

  return [clamp01(cr), clamp01(cg), clamp01(cb)];
}

const hexToRgb = (hex: string): [number, number, number] => {
  let h = hex.replace("#", "");
  if (h.length === 3) h = h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
  const num = parseInt(h, 16);
  return [((num >> 16) & 255) / 255, ((num >> 8) & 255) / 255, (num & 255) / 255];
};

const rgbToHex = (r: number, g: number, b: number): string => {
  const toHex = (v: number): string =>
    Math.round(clamp01(v) * 255)
      .toString(16)
      .padStart(2, "0");
  return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
};

/** Maps a hex color string (#rgb / #rrggbb) through the same math as the pixel pass. */
export function applyBasicAdjustmentToColor(color: string, adj: BasicAdjustment): string {
  if (adj.brightness === 0 && adj.contrast === 0 && adj.saturation === 0) return color;
  const [r, g, b] = hexToRgb(color);
  const [r2, g2, b2] = applyAdjustmentToRgb(r, g, b, normalizeBasicAdjustment(adj));
  return rgbToHex(r2, g2, b2);
}

/**
 * Inverse of `applyBasicAdjustmentToColor`. Given a color chosen in display
 * space and the layer's adjustment, returns the raw (layer-space) color that,
 * once the shader re-applies the adjustment, displays as the chosen color.
 * Used so painting on an adjusted layer is WYSIWYG (picker color == stroke).
 */
export function inverseBasicAdjustmentToColor(color: string, adj: BasicAdjustment): string {
  const normalized = normalizeBasicAdjustment(adj);
  if (normalized.brightness === 0 && normalized.contrast === 0 && normalized.saturation === 0) return color;
  const [r, g, b] = hexToRgb(color);
  const [r2, g2, b2] = inverseAdjustmentToRgb(r, g, b, normalized);
  return rgbToHex(r2, g2, b2);
}

// Saturation mixes channels through a luminance term, so the inverse is not
// separable per channel. We invert the real `applyAdjustmentToRgb` with a few
// Newton steps (numerical Jacobian). Cheap — runs once per stroke, not per pixel.
function inverseAdjustmentToRgb(
  r: number,
  g: number,
  b: number,
  adj: BasicAdjustment,
): [number, number, number] {
  const target = [r, g, b];
  let x: [number, number, number] = [r, g, b];
  const h = 1e-4;
  for (let iter = 0; iter < 16; iter++) {
    const f = applyAdjustmentToRgb(x[0], x[1], x[2], adj);
    const err = [f[0] - target[0], f[1] - target[1], f[2] - target[2]];
    if (Math.abs(err[0]) < 1e-7 && Math.abs(err[1]) < 1e-7 && Math.abs(err[2]) < 1e-7) break;
    // Numerical Jacobian.
    const fp: [number, number, number][] = [[0, 0, 0], [0, 0, 0], [0, 0, 0]];
    for (let j = 0; j < 3; j++) {
      const xp: [number, number, number] = [...x];
      xp[j] += h;
      const out = applyAdjustmentToRgb(xp[0], xp[1], xp[2], adj);
      fp[0][j] = (out[0] - f[0]) / h;
      fp[1][j] = (out[1] - f[1]) / h;
      fp[2][j] = (out[2] - f[2]) / h;
    }
    const dx = solve3x3(fp, [-err[0], -err[1], -err[2]]);
    x = [clamp01(x[0] + dx[0]), clamp01(x[1] + dx[1]), clamp01(x[2] + dx[2])];
  }
  return x;
}

// Solve A * x = b for 3x3 A via Gaussian elimination with partial pivoting.
function solve3x3(A: number[][], b: number[]): [number, number, number] {
  const m = [
    [A[0][0], A[0][1], A[0][2], b[0]],
    [A[1][0], A[1][1], A[1][2], b[1]],
    [A[2][0], A[2][1], A[2][2], b[2]],
  ];
  for (let col = 0; col < 3; col++) {
    let pivot = col;
    for (let row = col + 1; row < 3; row++) {
      if (Math.abs(m[row][col]) > Math.abs(m[pivot][col])) pivot = row;
    }
    [m[col], m[pivot]] = [m[pivot], m[col]];
    const d = m[col][col] || 1e-9;
    for (let k = col; k < 4; k++) m[col][k] /= d;
    for (let row = 0; row < 3; row++) {
      if (row === col) continue;
      const factor = m[row][col];
      for (let k = col; k < 4; k++) m[row][k] -= factor * m[col][k];
    }
  }
  return [m[0][3], m[1][3], m[2][3]];
}

export function applyBasicAdjustmentToPixels(
  pixels: Uint8ClampedArray,
  adjustment: BasicAdjustment,
): Uint8ClampedArray {
  // WASM-accelerated path (Phase 4). Falls back to the pure-TS impl if the
  // kernel is unavailable; kicks off module init so the next bake uses WASM.
  const out = applyBasicAdjustmentWithWasm(pixels, adjustment.brightness, adjustment.contrast, adjustment.saturation);
  if (out) {
    return new Uint8ClampedArray(out);
  }
  void getWasmExportModule();
  return applyBasicAdjustmentToPixelsTs(pixels, adjustment);
}

// Pure TypeScript implementation. Kept as the WASM fallback and exported for
// benchmarking (Phase 4). per-channel contrast/brightness/saturation.
export function applyBasicAdjustmentToPixelsTs(
  pixels: Uint8ClampedArray,
  adjustment: BasicAdjustment,
): Uint8ClampedArray {
  const next = new Uint8ClampedArray(pixels);
  const normalized = normalizeBasicAdjustment(adjustment);

  for (let i = 0; i < next.length; i += 4) {
    // Apply the same math as the GPU preview (shaders.ts::applyAdjustment).
    const [r, g, b] = applyAdjustmentToRgb(
      next[i] / 255,
      next[i + 1] / 255,
      next[i + 2] / 255,
      normalized,
    );
    next[i] = clampChannel(r * 255);
    next[i + 1] = clampChannel(g * 255);
    next[i + 2] = clampChannel(b * 255);
  }

  return next;
}

/**
 * Bakes a BasicAdjustment into a fresh ImageBitmap (CPU pixel pass). Used to
 * commit a non-destructive adjustment on release (or at export) so the layer's
 * stored pixels reflect the adjustment and subsequent paint shows raw colors.
 * The source bitmap is left untouched (undo snapshots may still reference it).
 */
export function bakeAdjustmentToBitmap(
  bitmap: ImageBitmap,
  width: number,
  height: number,
  adjustment: BasicAdjustment,
): ImageBitmap {
  const canvas = new OffscreenCanvas(width, height);
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Failed to acquire 2D context for adjustment bake");
  ctx.drawImage(bitmap, 0, 0);
  const imageData = ctx.getImageData(0, 0, width, height);
  imageData.data.set(applyBasicAdjustmentToPixels(imageData.data, adjustment));
  ctx.putImageData(imageData, 0, 0);
  return canvas.transferToImageBitmap();
}

// Technique A — Rust wasm owns WebGPU compute + readback (fastest per 2026-08-18 bench: 5.9× vs C at 12 Mpx, 2.9× at 2 Mpx).
// Cached per-size WebGpuAdjustRenderer so the pipeline isn't recreated per bake.
let poaCache: { w: number; h: number; renderer: any } | null = null;

async function tryAdjustRgbaPoa(
  pixels: Uint8Array,
  adj: BasicAdjustment,
  w: number,
  h: number,
): Promise<Uint8Array | null> {
  if (typeof navigator === "undefined" || !(navigator as any).gpu) return null;
  const m = await getWasmExportModule();
  if (!m?.WebGpuAdjustRenderer) return null;
  try {
    if (!poaCache || poaCache.w !== w || poaCache.h !== h) {
      poaCache = { w, h, renderer: await m.WebGpuAdjustRenderer.create(w, h) };
    }
    const out: Uint8Array = await poaCache.renderer.render(pixels, adj.brightness, adj.contrast, adj.saturation);
    console.log(`[bake] WebGPU A used ${w}x${h}`);
    return out;
  } catch {
    poaCache = null;
    return null;
  }
}

/**
 * GPU-compute variant of bakeAdjustmentToBitmap: routes the B/C/S pixel pass
 * through the Rust WebGPU path (WebGpuAdjustRenderer, Technique A, fastest) when WebGPU
 * is present, falling back to the TS WGSL path (GpuCompute.adjustRgba) and then
 * to the Rust CPU Engine (C) / TS CPU. Used by commit + export so the
 * adjustment bake runs on the GPU (realtime) instead of the CPU pixel loop.
 * Alpha preserved; source bitmap left untouched.
 */
export async function bakeAdjustmentToBitmapGpu(
  bitmap: ImageBitmap,
  width: number,
  height: number,
  adjustment: BasicAdjustment,
): Promise<ImageBitmap> {
  const canvas = new OffscreenCanvas(width, height);
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Failed to acquire 2D context for adjustment bake");
  ctx.drawImage(bitmap, 0, 0);
  const imageData = ctx.getImageData(0, 0, width, height);
  // Try A first (Rust wasm + WebGPU, fastest per 2026-08-18 bench: 5.9× vs C at 12 Mpx)
  const poaData = await tryAdjustRgbaPoa(new Uint8Array(imageData.data), adjustment, width, height);
  if (poaData) {
    imageData.data.set(poaData);
    ctx.putImageData(imageData, 0, 0);
    return canvas.transferToImageBitmap();
  }
  const res = await adjustRgba(imageData.data, adjustment);
  if (res.usedGpu) {
    imageData.data.set(res.data);
  } else {
    // WebGPU unavailable: prefer the Rust Engine (SSOT for layer pixels) over the
    // TS CPU pass. Fall back to the TS result already computed by adjustRgba on any error.
    try {
      const rust = await renderLayerPixelsRust(
        width,
        height,
        new Uint8Array(imageData.data),
        adjustment,
      );
      imageData.data.set(rust);
    } catch {
      imageData.data.set(res.data);
    }
  }
  ctx.putImageData(imageData, 0, 0);
  return canvas.transferToImageBitmap();
}
