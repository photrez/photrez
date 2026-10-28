// SPDX-License-Identifier: AGPL-3.0-or-later
//! The canonical per-layer pixel buffer: row-major RGBA bytes plus the
//! monotonic cache-validity epoch the host-side derived cache tracks.

use crate::pixel::tile::TilePatch;

/// Authoritative pixel owner for one layer. Exactly one `pixels` buffer exists
/// per layer. Pixel history lives in the document's `DocumentCore` (bounded to
/// `max_depth`); this struct keeps only the canonical buffer + `epoch`. `epoch`
/// is bumped on every canonical mutation so the derived TS cache can detect
/// staleness.
pub struct PixelLayer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Monotonic cache-validity counter. 0 at creation; +1 per canonical mutation.
    pub(crate) epoch: u64,
}

impl PixelLayer {
    /// `pixels` must be exactly `width*height*4` RGBA bytes (the seeding source).
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        assert_eq!(
            pixels.len(),
            width as usize * height as usize * 4,
            "pixel buffer size mismatch"
        );
        Self {
            width,
            height,
            pixels,
            epoch: 0,
        }
    }

    pub(crate) fn write_tile(&mut self, t: &TilePatch) {
        let x = t.x as usize;
        let y = t.y as usize;
        let w = t.w;
        let h = t.h;
        let stride = self.width as usize * 4;
        for row in 0..h {
            let dst = (y + row) * stride + x * 4;
            let src = row * w * 4;
            self.pixels[dst..dst + w * 4].copy_from_slice(&t.data[src..src + w * 4]);
        }
    }

    /// Read back a region for verification/transport (bounded by the caller).
    pub fn snapshot_region(&self, x: i64, y: i64, w: usize, h: usize) -> Vec<u8> {
        let stride = self.width as usize * 4;
        let mut out = vec![0u8; w * h * 4];
        for row in 0..h {
            let dst = row * w * 4;
            let src = ((y + row as i64) as usize) * stride + (x as usize) * 4;
            out[dst..dst + w * 4].copy_from_slice(&self.pixels[src..src + w * 4]);
        }
        out
    }

    /// Enumerate every tile of the layer (256-grid) as absolute TilePatches.
    /// Used by cache rehydration to refresh a derived TS cache from Rust.
    pub fn all_tiles(&self) -> Vec<TilePatch> {
        const TW: usize = 256;
        let cols = (self.width as usize).div_ceil(TW);
        let rows = (self.height as usize).div_ceil(TW);
        let mut out = Vec::new();
        for ty in 0..rows {
            for tx in 0..cols {
                let x = (tx * TW) as i64;
                let y = (ty * TW) as i64;
                let w = (TW).min(self.width as usize - tx * TW);
                let h = (TW).min(self.height as usize - ty * TW);
                if w == 0 || h == 0 {
                    continue;
                }
                let data = self.snapshot_region(x, y, w, h);
                out.push(TilePatch { x, y, w, h, data });
            }
        }
        out
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}
