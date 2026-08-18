// SPDX-License-Identifier: AGPL-3.0-or-later

let wasmModulePromise: Promise<any> | null = null;
let wasmModule: any = null;

export async function getWasmExportModule(): Promise<any> {
  if (!wasmModulePromise) {
    wasmModulePromise = (async () => {
      try {
        // Dynamic import of the compiled WASM package
        const mod: any = await import("@/wasm/pkg/photrez_core");
        if (typeof mod.default === "function") {
          await mod.default();
        }
        if (typeof mod.set_panic_hook === "function") mod.set_panic_hook();
        wasmModule = mod;
        return mod;
      } catch (err) {
        // Log warning for missing WASM pkg / non-WASM environment
        console.warn("WASM export module load failed, using Canvas fallback:", err);
        return null;
      }
    })();
  }
  return wasmModulePromise;
}

/// Synchronous accessor for an already-loaded WASM module. Returns null until
/// the module has finished initializing, so callers that must run synchronously
/// (e.g. the Paint Bucket click handler) can feature-detect without awaiting.
export function getLoadedWasmModule(): any | null {
  return wasmModule;
}

export async function encodeImageWithWasm(
  width: number,
  height: number,
  rgbaBytes: Uint8Array,
  formatStr: "png" | "jpeg" | "webp" | "tiff",
  quality: number,
): Promise<Uint8Array | null> {
  // WebP: route to browser Canvas convertToBlob. The Rust `image` 0.25 WebP
  // encoder emits ~5KB degenerate output for non-trivial content (data loss);
  // the browser encoder is correct. See FEATURES.md WebP DEFECT (2026-08-18).
  if (formatStr === "webp") return null;
  try {
    const wasmMod = await getWasmExportModule();
    if (!wasmMod || typeof wasmMod.encode_image_wasm !== "function") {
      return null;
    }
    const result = wasmMod.encode_image_wasm(width, height, rgbaBytes, formatStr, quality);
    return result ? new Uint8Array(result) : null;
  } catch (err) {
    console.warn("WASM image encoding failed, using Canvas fallback:", err);
    return null;
  }
}

// ── Pointwise compute kernels (CPU-accel tier; live) ─────────────────────────
// floodFill / gradientFill / adjustments-bake mirror the TS CPU impls and are
// dispatched by the Paint Bucket, Gradient, and adjustment-bake paths. invert
// moved to WGSL (gpuCompute.ts) and is no longer a WASM kernel.


// ── Flood fill (Phase 3) ──────────────────────────────────────────────────────
// Mirrors `FillMask` from `features/fill/fillOperations.ts`. Returns a full
// filled RGBA copy (buffer-out) or null when the WASM kernel is unavailable.
export function floodFillWithWasm(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  sx: number,
  sy: number,
  fr: number,
  fg: number,
  fb: number,
  fa: number,
  tolerance: number,
  mask: { x: number; y: number; w: number; h: number; shape?: "rect" | "ellipse"; inverted?: boolean } | null,
  contiguous: boolean,
): Uint8Array | null {
  const mod = getLoadedWasmModule();
  if (!mod || typeof mod.flood_fill_wasm !== "function") return null;
  const buf = new Uint8Array(data);
  const hasMask = !!mask;
  const mx = mask?.x ?? 0;
  const my = mask?.y ?? 0;
  const mw = mask?.w ?? 0;
  const mh = mask?.h ?? 0;
  const shape = mask?.shape === "ellipse" ? 1 : 0;
  const inv = mask?.inverted ?? false;
  try {
    const out = mod.flood_fill_wasm(buf, width, height, sx, sy, fr, fg, fb, fa, tolerance, hasMask, mx, my, mw, mh, shape, inv, contiguous);
    return out ? new Uint8Array(out) : null;
  } catch (err) {
    console.warn("WASM flood fill failed, using TS fallback:", err);
    return null;
  }
}

// ── Gradient fill (Phase 4) ────────────────────────────────────────────────────
export function gradientFillWithWasm(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  gradType: "linear" | "radial",
  ax: number,
  ay: number,
  bx: number,
  by: number,
  stops: { offset: number; r: number; g: number; b: number; a: number }[],
  mask: { x: number; y: number; w: number; h: number; shape?: "rect" | "ellipse"; inverted?: boolean } | null,
): Uint8Array | null {
  const mod = getLoadedWasmModule();
  if (!mod || typeof mod.gradient_fill_wasm !== "function") return null;
  const buf = new Uint8Array(data);
  const typeId = gradType === "radial" ? 1 : 0;
  const stopOffsets = new Float64Array(stops.map((s) => s.offset));
  const stopColors = new Uint8Array(stops.length * 4);
  stops.forEach((s, i) => {
    stopColors[i * 4] = s.r;
    stopColors[i * 4 + 1] = s.g;
    stopColors[i * 4 + 2] = s.b;
    stopColors[i * 4 + 3] = s.a;
  });
  const hasMask = !!mask;
  const mx = mask?.x ?? 0;
  const my = mask?.y ?? 0;
  const mw = mask?.w ?? 0;
  const mh = mask?.h ?? 0;
  const shape = mask?.shape === "ellipse" ? 1 : 0;
  const inv = mask?.inverted ?? false;
  try {
    const out = mod.gradient_fill_wasm(buf, width, height, typeId, ax, ay, bx, by, stopOffsets, stopColors, hasMask, mx, my, mw, mh, shape, inv);
    return out ? new Uint8Array(out) : null;
  } catch (err) {
    console.warn("WASM gradient fill failed, using TS fallback:", err);
    return null;
  }
}

// ── Basic adjustment bake (Phase 4) ────────────────────────────────────────────
export function applyBasicAdjustmentWithWasm(
  data: Uint8ClampedArray,
  brightness: number,
  contrast: number,
  saturation: number,
): Uint8Array | null {
  const mod = getLoadedWasmModule();
  if (!mod || typeof mod.apply_basic_adjustment_wasm !== "function") return null;
  const buf = new Uint8Array(data);
  try {
    const out = mod.apply_basic_adjustment_wasm(buf, brightness, contrast, saturation);
    return out ? new Uint8Array(out) : null;
  } catch (err) {
    console.warn("WASM adjustment failed, using TS fallback:", err);
    return null;
  }
}
