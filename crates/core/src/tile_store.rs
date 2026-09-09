// SPDX-License-Identifier: AGPL-3.0-or-later
//! Production tile-major packed TileStore. This is the
//! canonical byte store for the pack/upload path: ONE contiguous packed buffer
//! per layer, with per-tile `{ offset, w, h }` descriptors so extraction is a
//! zero-allocation `&[u8]` slice. Edge tiles are clipped to layer bounds.
//!
//! CANONICAL PACK ORDER: grid_x (tx) outer, grid_y (ty) inner —
//! `(0,0), (0,1), ... (1,0), ...`. Every tile is `w_eff * h_eff * 4` bytes
//! (full tiles are `TILE*TILE*4`; edge tiles are clipped to layer bounds).
//! `descs` stores per-tile `{ offset, w, h }` so extraction never over-reads.
//!
//! The store API is the canonical pack/upload surface. `TILE_MAJOR` is OFF in
//! the current build, so nothing consumes it yet in the lib build; dead-code is
//! allowed (the surface is intentionally exposed for the flag-enabled path).
#![allow(dead_code)]

use std::collections::HashMap;

// Test-only independent-copy oracle. Production references none of these.
#[cfg(test)]
use crate::parity_oracle::{assert_byte_eq, PackedView, PixelReader};

/// Canonical tile edge in pixels. Must match the engine value (256).
pub const TILE: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileDesc {
    /// byte offset into `packed`.
    pub offset: usize,
    /// clipped tile width (edge tiles).
    pub w: u32,
    /// clipped tile height (edge tiles).
    pub h: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileStore {
    /// layer pixel width.
    pub w: u32,
    /// layer pixel height.
    pub h: u32,
    pub tiles_w: u32,
    pub tiles_h: u32,
    pub tile_size: u32,
    /// tile-major packed, contiguous. Full tiles `TILE*TILE*4`; edge tiles clipped.
    pub packed: Vec<u8>,
    /// per-tile `{ offset, w, h }`, in canonical pack order.
    pub descs: Vec<TileDesc>,
}

impl TileStore {
    /// Build a tile-major packed store from a row-major buffer. Edge tiles are
    /// clipped to layer bounds; descs are never over-read.
    pub fn new(w: u32, h: u32, row_major: &[u8]) -> Self {
        assert!(w > 0 && h > 0, "dims must be > 0");
        assert_eq!(
            (w * h * 4) as usize,
            row_major.len(),
            "row-major len mismatch"
        );
        let tiles_w = w.div_ceil(TILE);
        let tiles_h = h.div_ceil(TILE);
        // Pre-size `packed` to the exact final byte count (concatenating clipped
        // edge tiles yields exactly `w*h*4` bytes) — avoids the realloc-doubling
        // ingest regression measured at ~44ms @4096².
        let mut packed = Vec::with_capacity((w * h * 4) as usize);
        let mut descs = Vec::with_capacity((tiles_w * tiles_h) as usize);
        for tx in 0..tiles_w {
            for ty in 0..tiles_h {
                let x0 = tx * TILE;
                let y0 = ty * TILE;
                let tw = (w - x0).min(TILE);
                let th = (h - y0).min(TILE);
                let offset = packed.len();
                for r in 0..th {
                    let src = ((y0 + r) * w + x0) as usize * 4;
                    let len = (tw * 4) as usize;
                    packed.extend_from_slice(&row_major[src..src + len]);
                }
                descs.push(TileDesc {
                    offset,
                    w: tw,
                    h: th,
                });
            }
        }
        TileStore {
            w,
            h,
            tiles_w,
            tiles_h,
            tile_size: TILE,
            packed,
            descs,
        }
    }

    /// Flat tile index (into `descs`) for `(tx,ty)` in canonical pack order.
    pub fn index_of(&self, tx: u32, ty: u32) -> usize {
        assert!(tx < self.tiles_w, "tx out of range");
        assert!(ty < self.tiles_h, "ty out of range");
        (tx * self.tiles_h + ty) as usize
    }

    /// Pixel-space bounds `(x0, y0, w_eff, h_eff)` of tile `(tx,ty)`.
    pub fn tile_bounds(&self, tx: u32, ty: u32) -> (u32, u32, u32, u32) {
        let idx = self.index_of(tx, ty);
        let x0 = tx * self.tile_size;
        let y0 = ty * self.tile_size;
        let tw = self.descs[idx].w;
        let th = self.descs[idx].h;
        (x0, y0, tw, th)
    }

    /// Borrow the clipped tile bytes for `(tx,ty)`. Never over-reads.
    pub fn extract_tile(&self, tx: u32, ty: u32) -> &[u8] {
        let idx = self.index_of(tx, ty);
        let d = self.descs[idx];
        let len = (d.w * d.h * 4) as usize;
        &self.packed[d.offset..d.offset + len]
    }

    /// Owned copy of the clipped tile bytes for `(tx,ty)`.
    pub fn to_owned_tile(&self, tx: u32, ty: u32) -> Vec<u8> {
        self.extract_tile(tx, ty).to_vec()
    }

    /// Transient tile-major materialization as a `PackedView` (layer dims, not
    /// the padded tile-grid dims). Test-only (independent oracle round-trips).
    #[cfg(test)]
    pub fn to_packed(&self) -> PackedView {
        PackedView {
            w: self.w,
            h: self.h,
            bytes: self.packed.clone(),
        }
    }
}

/// Reconstruct a tile-major packed buffer from clipped per-tile bytes given in
/// canonical pack order. Missing tiles are zero-filled. Used as the independent
/// `packed_from_tiles` reconstruction (test 11).
pub fn packed_from_tiles(
    w: u32,
    h: u32,
    tiles: &[(u32, u32, u32, u32, Vec<u8>)],
) -> (Vec<u8>, Vec<TileDesc>) {
    assert!(w > 0 && h > 0);
    let tiles_w = w.div_ceil(TILE);
    let tiles_h = h.div_ceil(TILE);
    let mut by_idx: HashMap<usize, &(u32, u32, u32, u32, Vec<u8>)> = HashMap::new();
    for t in tiles {
        by_idx.insert((t.0 * tiles_h + t.1) as usize, t);
    }
    let mut packed = Vec::new();
    let mut descs = Vec::with_capacity((tiles_w * tiles_h) as usize);
    for tx in 0..tiles_w {
        for ty in 0..tiles_h {
            let x0 = tx * TILE;
            let y0 = ty * TILE;
            let tw = (w - x0).min(TILE);
            let th = (h - y0).min(TILE);
            let flat = (tx * tiles_h + ty) as usize;
            let offset = packed.len();
            match by_idx.get(&flat) {
                Some((_, _, _, _, bytes)) => {
                    debug_assert_eq!(
                        (tw * th * 4) as usize,
                        bytes.len(),
                        "clipped tile byte len mismatch"
                    );
                    packed.extend_from_slice(bytes);
                }
                None => packed.extend_from_slice(&vec![0u8; (tw * th * 4) as usize]),
            }
            descs.push(TileDesc {
                offset,
                w: tw,
                h: th,
            });
        }
    }
    (packed, descs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(b: &mut [u8], v: u8) {
        b.iter_mut().for_each(|p| *p = v);
    }
    fn row_major(w: u32, h: u32, v: u8) -> Vec<u8> {
        let mut b = vec![0u8; (w * h * 4) as usize];
        fill(&mut b, v);
        b
    }

    // (4) tile boundary math: top-left / edge / corner / partial / cross-256.
    #[test]
    fn tile_boundary_math_exact_1x1() {
        let s = TileStore::new(100, 100, &row_major(100, 100, 3));
        assert_eq!((s.tiles_w, s.tiles_h), (1, 1));
        assert_eq!(s.descs.len(), 1);
        let d = s.descs[0];
        assert_eq!((d.offset, d.w, d.h), (0, 100, 100));
        let (x0, y0, tw, th) = s.tile_bounds(0, 0);
        assert_eq!((x0, y0, tw, th), (0, 0, 100, 100));
    }

    #[test]
    fn tile_boundary_math_cross_256_full_and_edge() {
        // 300x257 -> tiles_w=2 (256 + 44), tiles_h=2 (256 + 1).
        let s = TileStore::new(300, 257, &row_major(300, 257, 4));
        assert_eq!((s.tiles_w, s.tiles_h), (2, 2));
        assert_eq!(s.descs.len(), 4);
        // (0,0) full 256x256.
        assert_eq!(s.descs[0].w, 256);
        assert_eq!(s.descs[0].h, 256);
        // (1,0) right edge clipped to 44x256.
        let idx10 = s.index_of(1, 0);
        assert_eq!((s.descs[idx10].w, s.descs[idx10].h), (44, 256));
        // (0,1) bottom edge clipped to 256x1.
        let idx01 = s.index_of(0, 1);
        assert_eq!((s.descs[idx01].w, s.descs[idx01].h), (256, 1));
        // (1,1) corner clipped to 44x1.
        let idx11 = s.index_of(1, 1);
        assert_eq!((s.descs[idx11].w, s.descs[idx11].h), (44, 1));
        // offsets are cumulative and never overlap.
        let end = s.descs[idx11].offset + 44 * 4;
        assert_eq!(end, s.packed.len(), "packed len equals last tile end");
    }

    #[test]
    fn tile_extract_no_overread_no_neighbor_corruption() {
        // Distinct per-pixel values so neighbor corruption is detectable.
        let w = 300u32;
        let h = 257u32;
        let mut bytes = vec![0u8; (w * h * 4) as usize];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        let s = TileStore::new(w, h, &bytes.clone());
        for ty in 0..s.tiles_h {
            for tx in 0..s.tiles_w {
                let t = s.extract_tile(tx, ty);
                let (x0, y0, tw, th) = s.tile_bounds(tx, ty);
                // Compare to the row-major source clipped directly.
                let mut expected = Vec::with_capacity((tw * th * 4) as usize);
                for r in 0..th {
                    let src = ((y0 + r) * w + x0) as usize * 4;
                    let len = (tw * 4) as usize;
                    expected.extend_from_slice(&bytes[src..src + len]);
                }
                assert_eq!(t.len(), (tw * th * 4) as usize);
                assert_byte_eq(t, &expected, &format!("tile {tx},{ty}"));
            }
        }
    }

    // (11) packing identity: new(w,h,row) == packed_from_tiles == round-trip row-major.
    #[test]
    fn packing_roundtrip_identity() {
        for (w, h) in [(100u32, 100u32), (257, 300), (512, 513)] {
            let bytes = row_major(w, h, 9);
            let s = TileStore::new(w, h, &bytes);
            // Reconstruct from the store's own clipped tiles (independent fn).
            let mut tiles: Vec<(u32, u32, u32, u32, Vec<u8>)> = Vec::new();
            for ty in 0..s.tiles_h {
                for tx in 0..s.tiles_w {
                    tiles.push((tx, ty, 0, 0, s.to_owned_tile(tx, ty)));
                }
            }
            let (recon, _descs) = packed_from_tiles(w, h, &tiles);
            assert_byte_eq(&recon, &s.packed, &format!("{w}x{h} recon identity"));
            // Round-trip the packed buffer back to row-major == original.
            let row = s.to_packed().to_row_major(TILE);
            assert_byte_eq(&row, &bytes, &format!("{w}x{h} packed->row"));
        }
    }

    // (9) tile-major extract vs independent-copy oracle across several sizes.
    #[test]
    fn tile_major_extract_matches_oracle() {
        for (w, h) in [(100u32, 100u32), (257, 300), (512, 513)] {
            let bytes = row_major(w, h, 11);
            let s = TileStore::new(w, h, &bytes);
            let pr = PixelReader::new(w, h, bytes.clone());
            // Whole packed buffer must byte-match the oracle's independent tile-major.
            assert_byte_eq(&s.packed, &pr.to_tile_major().bytes, &format!("{w}x{h}"));
            // Per-tile extract must match the oracle per-tile.
            for ty in 0..s.tiles_h {
                for tx in 0..s.tiles_w {
                    assert_byte_eq(
                        s.extract_tile(tx, ty),
                        &pr.read_tile(tx, ty),
                        &format!("{w}x{h} tile {tx},{ty}"),
                    );
                }
            }
        }
    }

    #[test]
    fn packed_uses_canonical_tx_outer_ty_inner_order() {
        // 300x257 -> 2x2 tiles; index_of(1,0) must be the right-edge tile.
        let s = TileStore::new(300, 257, &row_major(300, 257, 1));
        // index_of(1,0) = 1*tiles_h + 0 = 2.
        assert_eq!(s.index_of(1, 0), 2);
        assert_eq!(s.descs[2].w, 44, "right-edge tile width clipped");
        assert_eq!(s.descs[2].h, 256);
    }

    // ── Packed tile store (canonical order / zero-copy) ──

    // Index mapping is tx-outer/ty-inner AND the packed buffer is
    // contiguous in canonical order (each tile's offset is the cumulative byte
    // length of the preceding tiles; the last tile ends exactly at packed.len()).
    #[test]
    fn tile_store_index_mapping_contiguous_multitile() {
        let w = 513u32;
        let h = 300u32;
        let s = TileStore::new(w, h, &row_major(w, h, 21));
        assert_eq!((s.tiles_w, s.tiles_h), (3, 2), "513x300 -> 3x2 grid");
        // index_of is tx-outer/ty-inner: (tx,ty) -> tx*tiles_h + ty.
        for ty in 0..s.tiles_h {
            for tx in 0..s.tiles_w {
                assert_eq!(s.index_of(tx, ty), (tx * s.tiles_h + ty) as usize);
            }
        }
        // Contiguity: offsets are cumulative and never overlap.
        let mut cur = 0usize;
        for (i, d) in s.descs.iter().enumerate() {
            assert_eq!(d.offset, cur, "descs[{i}] offset is cumulative");
            cur += (d.w * d.h * 4) as usize;
        }
        assert_eq!(cur, s.packed.len(), "packed len equals last tile end");
        assert_eq!(cur, (w * h * 4) as usize, "contiguity yields exactly w*h*4");
    }

    // `extract_tile` is ZERO-COPY — it returns a slice that ALIASES the
    // packed buffer (no per-tile allocation, no copy).
    #[test]
    fn extract_zero_copy_aliases_packed() {
        let w = 300u32;
        let h = 257u32;
        let s = TileStore::new(w, h, &row_major(w, h, 5));
        for ty in 0..s.tiles_h {
            for tx in 0..s.tiles_w {
                let d = &s.descs[s.index_of(tx, ty)];
                let t = s.extract_tile(tx, ty);
                let len = (d.w * d.h * 4) as usize;
                assert_eq!(t.len(), len);
                // The returned slice points INTO `packed` (no separate alloc).
                assert!(
                    std::ptr::eq(t.as_ptr(), s.packed[d.offset..].as_ptr()),
                    "tile {tx},{ty} must alias the packed buffer"
                );
                assert!(t.as_ptr() == s.packed[d.offset..].as_ptr());
            }
        }
    }

    // Export/upload reconstruction — packed -> row-major must be
    // byte-equal to the original row-major source (and to the independent
    // oracle's packed view) at the canonical edge/partial sizes.
    #[test]
    fn export_reconstructs_row_major_equal_to_oracle() {
        for (w, h) in [(100u32, 100u32), (300u32, 300u32), (513u32, 513u32)] {
            let bytes = row_major(w, h, 13);
            let s = TileStore::new(w, h, &bytes);
            // packed tile-major == independent oracle tile-major.
            let pr = PixelReader::new(w, h, bytes.clone());
            assert_byte_eq(
                &s.packed,
                &pr.to_tile_major().bytes,
                &format!("{w}x{h} packed"),
            );
            // packed -> row-major == original bytes.
            let row = s.to_packed().to_row_major(TILE);
            assert_byte_eq(&row, &bytes, &format!("{w}x{h} reconstruct"));
        }
    }
}
