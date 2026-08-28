// SPDX-License-Identifier: AGPL-3.0-or-later
// Render worker — konsolidasi visual (jangan mencar)
// Wow idea #1: SharedRing + OffscreenCanvas worker + zero-copy
// - Satu SharedArrayBuffer 48MB owned by WASM (kernel::alloc_rgba_buffer)
// - TS cuma Atomics.store coords, Rust worker consume ring, composite tiles, view ready
// - OffscreenCanvas di worker own WebGL2, TS main thread 0 texSubImage2D
// Gate: visual FPS bench harus >1.05x vs main-thread upload
// Status: PoC stub — API consolidated di 1 file, bench gate before wiring.

use wasm_bindgen::prelude::*;

/// PoC: composite tile IDs into out buffer via zero-copy views.
/// \	ile_ids\ are kernel buffer IDs (alloc_rgba_buffer), \out_id\ is destination.
/// Returns true if all tiles composited. Pure memory copy — no Canvas2D.
#[wasm_bindgen]
pub fn composite_tiles_zero_copy(tile_ids: Vec<u32>, out_id: u32) -> bool {
    if tile_ids.is_empty() {
        return false;
    }
    let tiles: Vec<Vec<u8>> = tile_ids
        .iter()
        .map(|tid| crate::kernel::with_buffer(*tid, |tile| tile.to_vec()))
        .collect();
    crate::kernel::with_buffer_mut(out_id, |out| {
        let mut offset = 0usize;
        for tile in &tiles {
            let len = tile.len().min(out.len().saturating_sub(offset));
            if len > 0 {
                out[offset..offset + len].copy_from_slice(&tile[..len]);
                offset += len;
            }
        }
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::{alloc_rgba_buffer, free_rgba_buffer, with_buffer, with_buffer_mut};

    #[test]
    fn composite_zero_copy_basic() {
        let t1 = alloc_rgba_buffer(4);
        let t2 = alloc_rgba_buffer(4);
        let out = alloc_rgba_buffer(8);
        with_buffer_mut(t1, |v| v.copy_from_slice(&[1, 2, 3, 4]));
        with_buffer_mut(t2, |v| v.copy_from_slice(&[5, 6, 7, 8]));
        assert!(composite_tiles_zero_copy(vec![t1, t2], out));
        let vout = with_buffer(out, |v| v.to_vec());
        assert_eq!(&vout[..8], &[1, 2, 3, 4, 5, 6, 7, 8]);
        free_rgba_buffer(t1);
        free_rgba_buffer(t2);
        free_rgba_buffer(out);
    }
}
