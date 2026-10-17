// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Native engine: Rust owns the editing ENGINE STATE (a layer's pixels + its
// non-destructive adjustment params) and renders in place into its owned
// buffer. Callers read the result through `view` — a raw, zero-copy slice —
// so the pixels reach the display without a copy or a readback step.
//
// Best-practice notes:
// - `view` returns a raw view into memory; it is only valid until the next
//   allocation. Callers MUST consume the view synchronously in the same
//   block and MUST NOT retain it across another call.
// - The pixel buffer lives in the engine-owned storage and `Drop` frees it,
//   so we do not leak per layer when the Engine is reclaimed.
use crate::kernel::apply_basic_adjustment_impl;

/// The owned pixel bytes.
type EngineStorage = Vec<u8>;

fn new_storage(len: usize) -> EngineStorage {
    vec![0u8; len]
}

pub struct Engine {
    storage: EngineStorage,
    w: u32,
    h: u32,
    brightness: f64,
    contrast: f64,
    saturation: f64,
}

impl Engine {
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

    /// Read access to the owned pixels.
    pub fn pixels(&self) -> &[u8] {
        &self.storage
    }

    pub fn width(&self) -> u32 {
        self.w
    }
    pub fn height(&self) -> u32 {
        self.h
    }

    pub fn set_brightness(&mut self, v: f64) {
        self.brightness = v;
    }
    pub fn set_contrast(&mut self, v: f64) {
        self.contrast = v;
    }
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
        self.storage.copy_from_slice(src);
    }

    /// Apply the adjustment stack in place on the owned buffer (Rust owns compute).
    pub fn render(&mut self) {
        apply_basic_adjustment_impl(
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
        let out = eng.pixels().to_vec();
        let reference = crate::kernel::apply_basic_adjustment(&px, 20.0, 0.0, 0.0);
        assert_eq!(
            out, reference,
            "Engine render must equal production adjust kernel"
        );
        assert_eq!(out.len(), px.len());
    }

    #[test]
    fn drop_frees_buffer_without_panic() {
        // Engine owns a buffer; dropping must free it (the owned Vec) — no leak,
        // no panic.
        let mut eng = Engine::new(4, 1);
        eng.set_layer_pixels(&[0u8; 16]);
        drop(eng);
    }
}
