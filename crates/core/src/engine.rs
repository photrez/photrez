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
// - On wasm the pixel buffer lives in the global registry and `Drop` frees it,
//   so we do not leak 48MB per layer when the Engine is GC'd. Off-wasm the
//   Engine owns a plain `Vec<u8>` (no registry) freed automatically on drop.
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::kernel::{alloc_rgba_buffer, apply_adjustment_inplace, free_rgba_buffer, write_buffer};

/// Wasm: a registry id whose bytes JS views zero-copy via `rgba_buffer_view`.
/// Off-wasm: the owned pixel bytes.
#[cfg(target_arch = "wasm32")]
type EngineStorage = u32;
#[cfg(not(target_arch = "wasm32"))]
type EngineStorage = Vec<u8>;

#[cfg(target_arch = "wasm32")]
fn new_storage(len: usize) -> EngineStorage {
    alloc_rgba_buffer(len)
}

#[cfg(not(target_arch = "wasm32"))]
fn new_storage(len: usize) -> EngineStorage {
    vec![0u8; len]
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Engine {
    storage: EngineStorage,
    w: u32,
    h: u32,
    brightness: f64,
    contrast: f64,
    saturation: f64,
}

#[cfg(target_arch = "wasm32")]
impl Drop for Engine {
    fn drop(&mut self) {
        // Free the wasm-owned pixel buffer so we don't leak it when the Engine is GC'd.
        free_rgba_buffer(self.storage);
    }
}

/// Wasm-only accessor for the registry id JS views via `rgba_buffer_view`.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl Engine {
    #[wasm_bindgen(getter)]
    pub fn buffer_id(&self) -> u32 {
        self.storage
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Engine {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(w: u32, h: u32) -> Engine {
        Engine {
            storage: new_storage((w * h * 4) as usize),
            w,
            h,
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
        }
    }

    /// Off-wasm read access to the owned pixels (the wasm build exposes the
    /// zero-copy view via `rgba_buffer_view` instead).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn pixels(&self) -> &[u8] {
        &self.storage
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn width(&self) -> u32 {
        self.w
    }
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    pub fn height(&self) -> u32 {
        self.h
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(setter))]
    pub fn set_brightness(&mut self, v: f64) {
        self.brightness = v;
    }
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(setter))]
    pub fn set_contrast(&mut self, v: f64) {
        self.contrast = v;
    }
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(setter))]
    pub fn set_saturation(&mut self, v: f64) {
        self.saturation = v;
    }

    /// One-time upload of source layer pixels into the owned buffer (the
    /// only copy; afterwards Rust mutates in place — no per-frame copy).
    pub fn set_layer_pixels(&mut self, src: &[u8]) {
        assert_eq!(
            src.len(),
            (self.w * self.h * 4) as usize,
            "layer size mismatch"
        );
        #[cfg(target_arch = "wasm32")]
        write_buffer(self.storage, src);
        #[cfg(not(target_arch = "wasm32"))]
        self.storage.copy_from_slice(src);
    }

    /// Apply the adjustment stack in place on the owned buffer (Rust owns compute).
    /// Allocates nothing, so it will not trigger `memory.grow` and invalidate a view
    /// that was created before this call.
    pub fn render(&mut self) {
        #[cfg(target_arch = "wasm32")]
        apply_adjustment_inplace(
            self.storage,
            self.brightness,
            self.contrast,
            self.saturation,
        );
        #[cfg(not(target_arch = "wasm32"))]
        crate::kernel::apply_basic_adjustment_impl(
            &mut self.storage,
            self.brightness,
            self.contrast,
            self.saturation,
        );
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
        #[cfg(target_arch = "wasm32")]
        let out = crate::kernel::buffer_clone(eng.buffer_id());
        #[cfg(not(target_arch = "wasm32"))]
        let out = eng.pixels().to_vec();
        let reference = crate::kernel::apply_basic_adjustment_wasm(&px, 20.0, 0.0, 0.0);
        assert_eq!(
            out, reference,
            "Engine render must equal production adjust kernel"
        );
        assert_eq!(out.len(), px.len());
    }

    #[test]
    fn drop_frees_buffer_without_panic() {
        // Engine owns a buffer; dropping must free it (wasm: the registry
        // slot, off-wasm: the owned Vec) — no leak, no panic.
        let mut eng = Engine::new(4, 1);
        eng.set_layer_pixels(&[0u8; 16]);
        drop(eng);
    }
}
