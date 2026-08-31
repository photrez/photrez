// SPDX-License-Identifier: AGPL-3.0-or-later
//! Phase A0 test-only independent-copy parity oracle.
//!
//! This is the REFERENCE TRUTH model: plain row-major `Vec<u8>`, no structural
//! sharing, no `Arc`. The tile-major `TileStore` / `StateNode` model must
//! byte-match it exactly. NEVER use the new tile-major store as its own oracle.
//! Compiled ONLY under `#[cfg(test)]`; zero production dependency.

use std::collections::HashMap;

/// Canonical tile edge in pixels. Must match the engine value (256).
pub const TILE: u32 = 256;

/// Transient tile-major materialization. Never the persistent authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedView {
    pub w: u32,
    pub h: u32,
    /// tile-major packed, contiguous; edge tiles clipped (w_eff * h_eff * 4).
    pub bytes: Vec<u8>,
}

impl PackedView {
    /// Reverse tile-major -> row-major for round-trip byte-identity checks.
    ///
    /// CANONICAL PACK ORDER: grid_x (tx) outer, grid_y (ty) inner:
    /// `(0,0), (0,1), ... (1,0), ...`. The offset advances by each tile's
    /// `w_eff * h_eff * 4` bytes (edge tiles clipped exactly like `TILE`).
    pub fn to_row_major(&self, tile: u32) -> Vec<u8> {
        assert!(tile > 0);
        let mut row = vec![0u8; (self.w * self.h * 4) as usize];
        let tiles_w = (self.w + tile - 1) / tile;
        let tiles_h = (self.h + tile - 1) / tile;
        let mut off = 0usize;
        for tx in 0..tiles_w {
            for ty in 0..tiles_h {
                let x0 = tx * tile;
                let y0 = ty * tile;
                let tw = (self.w - x0).min(tile);
                let th = (self.h - y0).min(tile);
                let twb = (tw * 4) as usize;
                for r in 0..th {
                    let dst = ((y0 + r) * self.w + x0) as usize * 4;
                    let src = off + (r as usize) * twb;
                    row[dst..dst + twb].copy_from_slice(&self.bytes[src..src + twb]);
                }
                off += (tw * th * 4) as usize;
            }
        }
        row
    }
}

/// Independent read-only row-major oracle. No structural sharing, no Arc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelReader {
    pub w: u32,
    pub h: u32,
    /// row-major, len == w * h * 4.
    pub bytes: Vec<u8>,
}

impl PixelReader {
    pub fn new(w: u32, h: u32, bytes: Vec<u8>) -> Self {
        assert_eq!(
            bytes.len(),
            (w * h * 4) as usize,
            "row-major len mismatch: {}/{}",
            bytes.len(),
            w * h * 4
        );
        PixelReader { w, h, bytes }
    }

    pub fn tiles_w(&self) -> u32 {
        (self.w + TILE - 1) / TILE
    }
    pub fn tiles_h(&self) -> u32 {
        (self.h + TILE - 1) / TILE
    }

    /// Pixel-space bounds `(x0, y0, w_eff, h_eff)` of the tile at `(tx, ty)`,
    /// edge-clipped to layer bounds. Never over-reads.
    pub fn tile_bounds(&self, tx: u32, ty: u32) -> (u32, u32, u32, u32) {
        assert!(tx < self.tiles_w(), "tx out of range");
        assert!(ty < self.tiles_h(), "ty out of range");
        let x0 = tx * TILE;
        let y0 = ty * TILE;
        let tw = (self.w - x0).min(TILE);
        let th = (self.h - y0).min(TILE);
        (x0, y0, tw, th)
    }

    /// Extract ONE clipped tile as row-major bytes within that tile.
    pub fn read_tile(&self, tx: u32, ty: u32) -> Vec<u8> {
        let (x0, y0, tw, th) = self.tile_bounds(tx, ty);
        let mut tile = Vec::with_capacity((tw * th * 4) as usize);
        for r in 0..th {
            let src = ((y0 + r) * self.w + x0) as usize * 4;
            let len = (tw * 4) as usize;
            tile.extend_from_slice(&self.bytes[src..src + len]);
        }
        tile
    }

    /// Row-major -> tile-major packed (independent reconstruction, canonical
    /// pack order). This is the truth the tile-major model must equal.
    pub fn to_tile_major(&self) -> PackedView {
        let mut out = Vec::new();
        for tx in 0..self.tiles_w() {
            for ty in 0..self.tiles_h() {
                out.extend_from_slice(&self.read_tile(tx, ty));
            }
        }
        PackedView {
            w: self.w,
            h: self.h,
            bytes: out,
        }
    }
}

/// Byte diff: `(max_abs_diff, diff_count)`. If lengths differ, reports
/// `(usize::MAX, max_len)` so tests fail loudly rather than silently compare.
pub fn bytes_diff(a: &[u8], b: &[u8]) -> (usize, usize) {
    if a.len() != b.len() {
        return (usize::MAX, a.len().max(b.len()));
    }
    let mut max_diff = 0usize;
    let mut diff_count = 0usize;
    for (x, y) in a.iter().zip(b.iter()) {
        let d = (*x as i32 - *y as i32).unsigned_abs() as usize;
        if d != 0 {
            diff_count += 1;
            if d > max_diff {
                max_diff = d;
            }
        }
    }
    (max_diff, diff_count)
}

/// Assert two byte buffers are identical (maxDiff=0, diffCount=0, same len).
pub fn assert_byte_eq(a: &[u8], b: &[u8], label: &str) {
    assert_eq!(
        a.len(),
        b.len(),
        "{label}: length mismatch {} vs {}",
        a.len(),
        b.len()
    );
    let (max_diff, diff_count) = bytes_diff(a, b);
    assert_eq!(
        max_diff, 0,
        "{label}: maxDiff={max_diff} diffCount={diff_count}"
    );
    assert_eq!(diff_count, 0, "{label}: diffCount={diff_count}");
}

/// Independent SHARE-ORACLE (S3/S4/S5): tracks a *symbolic* token per tile
/// position and reconciles it against a StateNode's ACTUAL `Arc<TileBlock>`
/// pointers (via `Arc::as_ptr`). Verifies the per-layer tile-identity
/// invariant: each `(tx,ty)` is a distinct block, and a COW write re-allocates
/// only the written tile while untouched tiles keep identity.
pub struct ShareOracle {
    cur: HashMap<(u32, u32), u64>,
    undo_stack: Vec<HashMap<(u32, u32), u64>>,
    redo_stack: Vec<HashMap<(u32, u32), u64>>,
    next: u64,
}

impl ShareOracle {
    pub fn new(w: u32, h: u32) -> Self {
        let tiles_w = (w + TILE - 1) / TILE;
        let tiles_h = (h + TILE - 1) / TILE;
        let mut cur = HashMap::new();
        let mut token = 0u64;
        for tx in 0..tiles_w {
            for ty in 0..tiles_h {
                cur.insert((tx, ty), token);
                token += 1;
            }
        }
        ShareOracle {
            cur,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            next: token,
        }
    }

    /// A COW write touched these tiles; give each a fresh token.
    pub fn mark_written<I: IntoIterator<Item = (u32, u32)>>(&mut self, tiles: I) {
        for t in tiles {
            let tok = self.next;
            self.next += 1;
            self.cur.insert(t, tok);
        }
    }

    /// Snapshot the current token map (call after a commit). Clears redo.
    pub fn commit_head(&mut self) {
        self.undo_stack.push(self.cur.clone());
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            let cur = std::mem::replace(&mut self.cur, prev);
            self.redo_stack.push(cur);
        }
    }

    pub fn redo(&mut self) {
        if let Some(nxt) = self.redo_stack.pop() {
            let cur = std::mem::replace(&mut self.cur, nxt);
            self.undo_stack.push(cur);
        }
    }

    /// Drop the oldest snapshot (mirror FIFO eviction of the real history).
    pub fn evict_oldest(&mut self) {
        if !self.undo_stack.is_empty() {
            self.undo_stack.remove(0);
        }
    }

    /// Reconcile a map of REAL `Arc::as_ptr` values (as usize) per tile
    /// position against the symbolic token map. Returns true iff the mapping
    /// is a bijection: same token => same pointer, and same pointer => same
    /// token, and every tile position is covered exactly once. This proves
    /// clean per-position block identity (distinct positions never alias).
    pub fn reconcile(&self, real: &HashMap<(u32, u32), usize>) -> bool {
        let mut token_to_ptr: HashMap<u64, usize> = HashMap::new();
        let mut ptr_to_token: HashMap<usize, u64> = HashMap::new();
        let mut covered = 0usize;
        for (pos, ptr) in real {
            let Some(&token) = self.cur.get(pos) else {
                return false;
            };
            if let Some(&p) = token_to_ptr.get(&token) {
                if p != *ptr {
                    return false;
                }
            } else {
                token_to_ptr.insert(token, *ptr);
            }
            if let Some(&t) = ptr_to_token.get(ptr) {
                if t != token {
                    return false;
                }
            } else {
                ptr_to_token.insert(*ptr, token);
            }
            covered += 1;
        }
        covered == self.cur.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_roundtrip_tile_major_matches_row_major() {
        let w = 300u32;
        let h = 257u32;
        let bytes: Vec<u8> = (0..(w * h * 4) as u32).map(|i| (i % 251) as u8).collect();
        let pr = PixelReader::new(w, h, bytes.clone());
        let packed = pr.to_tile_major();
        assert_eq!(packed.w, w);
        assert_eq!(packed.h, h);
        let row = packed.to_row_major(TILE);
        assert_byte_eq(&row, &bytes, "oracle roundtrip");
    }

    #[test]
    fn oracle_tile_bounds_edge_clipped() {
        let pr = PixelReader::new(100, 100, vec![0u8; 100 * 100 * 4]);
        let (x0, y0, tw, th) = pr.tile_bounds(0, 0);
        assert_eq!((x0, y0, tw, th), (0, 0, 100, 100));
    }

    #[test]
    fn bytes_diff_detects_difference() {
        assert_eq!(bytes_diff(&[1, 2, 3], &[1, 2, 3]), (0, 0));
        assert_eq!(bytes_diff(&[1, 9, 3], &[1, 2, 3]), (7, 1));
        assert_eq!(bytes_diff(&[1], &[1, 2]), (usize::MAX, 2));
    }
}
