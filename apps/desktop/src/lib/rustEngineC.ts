// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Technique C vertical slice bridge (best-practice hardened).
// Rust (Engine, in @/wasm/pkg/photrez_core) owns the layer pixels + adjustment
// state, renders in place, and TS uploads the result ZERO-COPY to WebGL2.
//
// CRITICAL (wasm-bindgen contract): `rgba_buffer_view` returns a raw view into
// wasm linear memory that becomes detached if `memory.grow` happens. So we create
// the view and call `texImage2D` synchronously in the SAME block and never retain
// the view across another wasm call. No 48MB per-frame copy, no native readback.
//
// The wasm module is loaded through the repo's `getWasmExportModule` (which
// initializes the wasm-pack web target and gracefully returns null when the wasm
// cannot be fetched — e.g. in headless test envs). Callers fall back to the GPU/CPU
// bake paths on that null.
import { getWasmExportModule } from "@/components/editor/wasmExport";

export interface CAdjustment {
  brightness: number;
  contrast: number;
  saturation: number;
}

async function ensureWasm(): Promise<any> {
  const m = await getWasmExportModule();
  if (!m) throw new Error("Rust engine (wasm) unavailable");
  return m;
}

/**
 * Render one layer through the Rust engine (technique C) and upload it ZERO-COPY
 * to the given WebGL2 texture. The Rust view is consumed synchronously here and
 * never retained, so it cannot be invalidated by a later memory.grow.
 */
export async function renderLayerC(
  w: number,
  h: number,
  src: Uint8Array,
  adj: CAdjustment,
  gl: WebGL2RenderingContext,
  target: number,
): Promise<any> {
  const m = await ensureWasm();
  const eng: any = new m.Engine(w, h);
  eng.brightness = adj.brightness;
  eng.contrast = adj.contrast;
  eng.saturation = adj.saturation;
  eng.set_layer_pixels(src);
  eng.render();

  // Zero-copy: view into wasm memory, uploaded immediately, not retained.
  const view = m.rgba_buffer_view(eng.buffer_id);
  gl.texImage2D(
    target,
    0,
    gl.RGBA,
    w,
    h,
    0,
    gl.RGBA,
    gl.UNSIGNED_BYTE,
    view,
  );

  return eng;
}

/**
 * Rust-Engine adjustment bake (no GL): render `src` through the Engine and return
 * the adjusted pixels as a stable `Uint8Array` (copied out of wasm memory so the
 * buffer can be freed). Used as the CPU-tier bake backend when WebGPU is absent.
 */
export async function renderLayerPixelsRust(
  w: number,
  h: number,
  src: Uint8Array,
  adj: CAdjustment,
): Promise<Uint8Array> {
  const m = await ensureWasm();
  const eng: any = new m.Engine(w, h);
  eng.brightness = adj.brightness;
  eng.contrast = adj.contrast;
  eng.saturation = adj.saturation;
  eng.set_layer_pixels(src);
  eng.render();
  const view = m.rgba_buffer_view(eng.buffer_id);
  const out = new Uint8Array(view); // copy out before freeing the wasm buffer
  m.free_rgba_buffer(eng.buffer_id);
  return out;
}

/**
 * Production bake: Rust-Engine computes the adjusted pixels, returns an
 * ImageBitmap (alpha preserved). Mirrors `bakeAdjustmentToBitmap` but routes the
 * pixel pass through the Rust Engine (SSOT for layer pixels + adjustments).
 */
export async function bakeAdjustmentToBitmapRust(
  bitmap: ImageBitmap,
  w: number,
  h: number,
  adj: CAdjustment,
): Promise<ImageBitmap> {
  const canvas = new OffscreenCanvas(w, h);
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Failed to acquire 2D context for Rust adjustment bake");
  ctx.drawImage(bitmap, 0, 0);
  const imageData = ctx.getImageData(0, 0, w, h);
  const out = await renderLayerPixelsRust(w, h, new Uint8Array(imageData.data), adj);
  imageData.data.set(out);
  ctx.putImageData(imageData, 0, 0);
  return canvas.transferToImageBitmap();
}
