// SPDX-License-Identifier: AGPL-3.0-or-later
//! State-node model tests (logical / parity / refcount / COW identity).

use super::*;

fn fill(bytes: &mut [u8], v: u8) {
    bytes.iter_mut().for_each(|b| *b = v);
}

fn row_major(w: u32, h: u32, v: u8) -> Vec<u8> {
    let mut b = vec![0u8; (w * h * 4) as usize];
    fill(&mut b, v);
    b
}

// (1) retain/release Arc refcount semantics; zero-ref reclaim (I5/I6). The
// tile bytes are an `Arc<[u8]>`; zero-ref reclaims the allocation.
#[test]
fn arc_refcount_zero_ref_reclaimed() {
    let block: Arc<[u8]> = Arc::from(vec![0u8; 4 * 4 * 4]);
    let weak = Arc::downgrade(&block);
    assert_eq!(Arc::strong_count(&block), 1);
    assert!(weak.upgrade().is_some());
    let clone = block.clone();
    assert_eq!(Arc::strong_count(&block), 2);
    drop(clone);
    assert_eq!(Arc::strong_count(&block), 1);
    drop(block);
    assert!(weak.upgrade().is_none(), "zero-ref reclaimed");
}

// (2) I1 immutability: a published (shared) Arc cannot be mutated.
#[test]
fn immutability_published_shared_cannot_mutate() {
    let block: Arc<[u8]> = Arc::from(vec![1u8; 64]);
    let clone = block.clone();
    // Shared -> Arc::get_mut returns None, so no &mut access is possible
    // (I1/I2: no public &mut-to-bytes path, no in-place edit of published).
    assert!(Arc::get_mut(&mut block.clone()).is_none());
    // Unique owner CAN mutate (construction phase only, before publish).
    let mut unique: Arc<[u8]> = Arc::from(vec![1u8; 64]);
    assert!(Arc::get_mut(&mut unique).is_some());
    drop(clone);
}

// (9) tile-major extract vs independent-copy oracle (state side).
#[test]
fn state_tile_major_matches_oracle_at_sizes() {
    for (w, h) in [(100u32, 100u32), (257, 300), (512, 513)] {
        let bytes = row_major(w, h, 17u8);
        let ls = LayerState::new("L", w, h, &bytes, 0);
        let packed = ls.to_packed();
        let oracle = PixelReader::new(w, h, bytes.clone());
        assert_byte_eq(
            &packed.bytes,
            &oracle.to_tile_major().bytes,
            &format!("{w}x{h}"),
        );
        // reconstructed row-major == original.
        assert_byte_eq(&ls.to_row_major(), &bytes, &format!("{w}x{h} row"));
    }
}

// `cow_batch` (the production COW seam) must preserve `Arc::ptr_eq` for
// untouched tiles and replace ONLY the touched tiles with a fresh block. This is
// the direct sharing proof for the seam the history stream depends on (I3/I4/I9).
#[test]
fn cow_batch_preserves_untouched_tile_identity() {
    let w = 512u32;
    let h = 512u32;
    let mut ls = LayerState::new("L", w, h, &row_major(w, h, 1), 0);

    // A change region fully inside tile (0,0) -> only that tile is re-tiled.
    let region = RegionChange::new(0, 0, 16, 16, vec![9u8; 16 * 16 * 4]);
    let (before, after) = ls.cow_batch(&[region], 0);

    // Untouched tile (1,0) keeps the SAME Arc block identity.
    let untouched_before = before.tiles.iter().find(|t| t.grid_x == 1).unwrap();
    let untouched_after = after.tiles.iter().find(|t| t.grid_x == 1).unwrap();
    assert!(
        Arc::ptr_eq(&untouched_before.block, &untouched_after.block),
        "untouched tile must share Arc identity"
    );
    // Touched tile (0,0) gets a NEW block; bytes reflect the spliced region.
    let touched_before = before.tiles.iter().find(|t| t.grid_x == 0).unwrap();
    let touched_after = after.tiles.iter().find(|t| t.grid_x == 0).unwrap();
    assert!(
        !Arc::ptr_eq(&touched_before.block, &touched_after.block),
        "touched tile must get a fresh block"
    );
    assert_eq!(touched_after.bytes()[0], 9);
    // Untouched byte preserved in the untouched tile.
    assert_eq!(untouched_after.bytes()[0], 1);
}

// A packed layer is ONE allocation (one shared `Arc<[u8]>`), not N
// per-tile allocations. Every tile in the base state references the SAME packed
// buffer Arc; `strong_count` equals the tile count.
#[test]
fn packed_layer_single_allocation_tiles_share() {
    let w = 512u32;
    let h = 512u32;
    let ls = LayerState::new("L", w, h, &row_major(w, h, 3), 0);
    let state = ls.current_state();
    assert_eq!(state.tiles.len(), 4, "512x512 -> 2x2 grid, 4 tiles");
    let first = &state.tiles[0].block;
    for t in &state.tiles {
        assert!(
            Arc::ptr_eq(first, &t.block),
            "all tiles share one packed Arc"
        );
    }
    // The packed buffer is referenced exactly once per tile -> 1 alloc, 4 refs.
    assert_eq!(Arc::strong_count(first), state.tiles.len());
}

// Cross-layer isolation (I8) at the packed-buffer level — layer 1 and
// layer 2 have DISTINCT packed buffers, and within a layer all tiles share it.
#[test]
fn cross_layer_packed_buffers_isolated() {
    let w = 512u32;
    let h = 512u32;
    let l1 = LayerState::new("L1", w, h, &row_major(w, h, 1), 0);
    let l2 = LayerState::new("L2", w, h, &row_major(w, h, 2), 0);
    let s1 = l1.current_state();
    let s2 = l2.current_state();
    assert!(!Arc::ptr_eq(&s1.tiles[0].block, &s2.tiles[0].block));
    for (a, b) in s1.tiles.iter().zip(s2.tiles.iter()) {
        // within-layer sharing
        assert!(Arc::ptr_eq(&s1.tiles[0].block, &a.block));
        assert!(Arc::ptr_eq(&s2.tiles[0].block, &b.block));
        // cross-layer isolation
        assert!(!Arc::ptr_eq(&a.block, &b.block));
    }
    // Buffer contents differ between layers (per-layer packed buffer).
    assert_ne!(s1.tiles[0].bytes()[0], s2.tiles[0].bytes()[0]);
}

// `read_tile(...).bytes()` is ZERO-COPY — it aliases the shared packed
// `Arc<[u8]>` (no per-tile copy), at an edge/partial-canonical size.
#[test]
fn extract_zero_copy_slice_shares_packed() {
    let w = 300u32;
    let h = 257u32;
    let ls = LayerState::new("L", w, h, &row_major(w, h, 9), 0);
    let state = ls.current_state();
    for t in &state.tiles {
        let bytes = t.bytes();
        assert!(bytes.len() == (t.w * t.h * 4) as usize);
        assert!(
            std::ptr::eq(bytes.as_ptr(), t.block[t.offset..].as_ptr()),
            "tile ({},{}) must alias the packed buffer",
            t.grid_x,
            t.grid_y
        );
    }
}

// Full-byte parity vs the independent `parity_oracle` at the
// canonical non-multiple-of-256 sizes 100x100 / 300x300 / 513x513.
#[test]
fn edge_partial_byte_parity_oracle() {
    for (w, h) in [(100u32, 100u32), (300u32, 300u32), (513u32, 513u32)] {
        let bytes = row_major(w, h, 31u8);
        let ls = LayerState::new("L", w, h, &bytes, 0);
        let packed = ls.to_packed();
        let oracle = PixelReader::new(w, h, bytes.clone());
        assert_byte_eq(
            &packed.bytes,
            &oracle.to_tile_major().bytes,
            &format!("{w}x{h} packed"),
        );
        assert_byte_eq(&ls.to_row_major(), &bytes, &format!("{w}x{h} row"));
    }
}

// A single `cow_batch` commit carrying MULTIPLE disconnected change regions
// must produce ONE transition whose delta tiles are exactly the tiles BOTH
// regions intersect, with correct bytes and preserved identity for untouched.
#[test]
fn cow_batch_multi_region_single_commit_delta_and_parity() {
    let w = 512u32;
    let h = 512u32;
    let mut ls = LayerState::new("L", w, h, &row_major(w, h, 1), 0);

    // Two disconnected dirty rects: tile (0,0) and tile (1,1).
    let r1 = RegionChange::new(4, 4, 8, 8, vec![9u8; 8 * 8 * 4]);
    let r2 = RegionChange::new(300, 300, 8, 8, vec![5u8; 8 * 8 * 4]);
    let (before, after) = ls.cow_batch(&[r1, r2], 0);

    // Byte parity: splice both rects into a row-major oracle and compare to the
    // post-commit current state.
    let mut oracle_bytes = row_major(w, h, 1);
    for (x, y, ww, hh, v) in [(4u32, 4u32, 8u32, 8u32, 9u8), (300, 300, 8, 8, 5)] {
        let fill = vec![v; (ww * hh * 4) as usize];
        for row in 0..hh {
            let dst = ((y + row) * w + x) as usize * 4;
            let len = (ww * 4) as usize;
            let src = (row * ww * 4) as usize;
            oracle_bytes[dst..dst + len].copy_from_slice(&fill[src..src + len]);
        }
    }
    assert_byte_eq(&ls.to_row_major(), &oracle_bytes, "multi-region row parity");

    // Delta = exactly the two intersecting tiles get a fresh block; untouched
    // tiles share the packed identity (I3/I4/I9).
    let touched: Vec<(u32, u32)> = after
        .tiles
        .iter()
        .filter(|t| {
            let bb = before
                .tiles
                .iter()
                .find(|b| b.grid_x == t.grid_x && b.grid_y == t.grid_y)
                .unwrap();
            !Arc::ptr_eq(&bb.block, &t.block)
        })
        .map(|t| (t.grid_x, t.grid_y))
        .collect();
    assert_eq!(touched.len(), 2, "two tiles touched by two regions");
    assert!(touched.contains(&(0, 0)), "tile (0,0) touched");
    assert!(touched.contains(&(1, 1)), "tile (1,1) touched");
    for t in &after.tiles {
        if !touched.contains(&(t.grid_x, t.grid_y)) {
            let bb = before
                .tiles
                .iter()
                .find(|b| b.grid_x == t.grid_x && b.grid_y == t.grid_y)
                .unwrap();
            assert!(
                Arc::ptr_eq(&bb.block, &t.block),
                "untouched tile ({},{}) shares packed identity",
                t.grid_x,
                t.grid_y
            );
        }
    }
    // Touched tile (0,0) reflects the r1 splice at pixel (4,4).
    let t00 = after
        .tiles
        .iter()
        .find(|t| t.grid_x == 0 && t.grid_y == 0)
        .unwrap();
    let off = (4 * 256 + 4) as usize * 4;
    assert_eq!(
        &t00.bytes()[off..off + 4],
        &[9, 9, 9, 9],
        "r1 spliced into tile (0,0)"
    );
}
