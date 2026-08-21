// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Technique C vertical slice: Rust owns the editing ENGINE STATE (a layer's
// pixels + its non-destructive adjustment params) and renders in place into a
// wasm-owned buffer. TS reads the result via `rgba_buffer_view` (zero-copy) and
// uploads it to WebGL2.
//
// This makes Rust the SSOT for layer pixels while keeping the existing WebGL2
// display path (no transparent overlay, no native readback).
//
// Best-practice notes (verified against wasm-bindgen docs):
// - `rgba_buffer_view` returns a raw view into wasm linear memory; it is only
//   valid until the next `memory.grow` (any Rust allocation). The bridge MUST
//   create the view and call `texImage2D` synchronously in the same block and
//   MUST NOT retain the view across another wasm call (see rustEngineC.ts).
// - `Engine` owns its buffer in the global registry; `Drop` frees it so we do
//   not leak 48MB per layer when the Engine is GC'd.
use wasm_bindgen::prelude::*;

use crate::kernel::{alloc_rgba_buffer, apply_adjustment_inplace, free_rgba_buffer, write_buffer};

#[wasm_bindgen]
pub struct Engine {
    buf_id: u32,
    w: u32,
    h: u32,
    brightness: f64,
    contrast: f64,
    saturation: f64,
}

impl Drop for Engine {
    fn drop(&mut self) {
        // Free the wasm-owned pixel buffer so we don't leak it when the Engine is GC'd.
        free_rgba_buffer(self.buf_id);
    }
}

#[wasm_bindgen]
impl Engine {
    #[wasm_bindgen(constructor)]
    pub fn new(w: u32, h: u32) -> Engine {
        let buf_id = alloc_rgba_buffer((w * h * 4) as usize);
        Engine {
            buf_id,
            w,
            h,
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn buffer_id(&self) -> u32 {
        self.buf_id
    }
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.w
    }
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.h
    }

    #[wasm_bindgen(setter)]
    pub fn set_brightness(&mut self, v: f64) {
        self.brightness = v;
    }
    #[wasm_bindgen(setter)]
    pub fn set_contrast(&mut self, v: f64) {
        self.contrast = v;
    }
    #[wasm_bindgen(setter)]
    pub fn set_saturation(&mut self, v: f64) {
        self.saturation = v;
    }

    /// One-time upload of source layer pixels into the wasm-owned buffer (the
    /// only copy; afterwards Rust mutates in place — no per-frame copy).
    pub fn set_layer_pixels(&self, src: &[u8]) {
        assert_eq!(
            src.len(),
            (self.w * self.h * 4) as usize,
            "layer size mismatch"
        );
        write_buffer(self.buf_id, src);
    }

    /// Apply the adjustment stack in place on the wasm-owned buffer (Rust owns compute).
    /// Allocates nothing, so it will not trigger `memory.grow` and invalidate a view
    /// that was created before this call.
    pub fn render(&mut self) {
        apply_adjustment_inplace(self.buf_id, self.brightness, self.contrast, self.saturation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_render_matches_production_adjust() {
        let px = vec![100u8, 100, 100, 255, 50, 50, 50, 255];
        let mut eng = Engine::new(2, 1);
        eng.set_brightness(20.0);
        eng.set_layer_pixels(&px);
        eng.render();
        let out = crate::kernel::buffer_clone(eng.buffer_id());
        let reference = crate::kernel::apply_basic_adjustment_wasm(&px, 20.0, 0.0, 0.0);
        assert_eq!(
            out, reference,
            "Engine render must equal production adjust kernel"
        );
        assert_eq!(out.len(), px.len());
    }

    #[test]
    fn drop_frees_buffer_without_panic() {
        // Engine owns a buffer; dropping must free it (no leak / no panic).
        let eng = Engine::new(4, 1);
        eng.set_layer_pixels(&[0u8; 16]);
        drop(eng); // exercises Drop -> free_rgba_buffer
    }
}
