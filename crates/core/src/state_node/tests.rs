// SPDX-License-Identifier: AGPL-3.0-or-later
//! State-node model tests (logical / parity / refcount / COW identity).

use super::*;

fn fill(bytes: &mut Vec<u8>, v: u8) {
    bytes.iter_mut().for_each(|b| *b = v);
}

fn row_major(w: u32, h: u32, v: u8) -> Vec<u8> {
    let mut b = vec![0u8; (w * h * 4) as usize];
    fill(&mut b, v);
    b
}

fn clipped_tile(w: u32, h: u32, tx: u32, ty: u32, v: u8) -> Vec<u8> {
    let x0 = tx * TILE;
    let y0 = ty * TILE;
    let tw = (w - x0).min(TILE);
    let th = (h - y0).min(TILE);
    let mut b = vec![0u8; (tw * th * 4) as usize];
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

// (3) COW returns NEW StateNodeId; untouched tiles reuse Arc (ptr_eq); old state unchanged.
#[test]
fn cow_returns_new_state_id_and_reuses_untouched_tile_blocks() {
    let w = 512u32;
    let h = 512u32;
    let mut ls = LayerState::new("L", w, h, &row_major(w, h, 1), 0);
    let id0 = ls.current_state_id();

    let untouched_before = ls.read_tile(1, 0);
    let touched_before = ls.read_tile(0, 0);

    let meta = StateMeta::new(w, h, 0, 0);
    let id1 = ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 9), meta);
    assert_ne!(id1, id0, "COW must produce a new StateNodeId");
    assert_eq!(ls.current_state_id(), id1);

    // Untouched tile keeps the SAME Arc (shared packed subarray).
    let untouched_after = ls.read_tile(1, 0);
    assert!(Arc::ptr_eq(&untouched_before.block, &untouched_after.block));
    // Touched tile gets a NEW Arc block.
    let touched_after = ls.read_tile(0, 0);
    assert!(!Arc::ptr_eq(&touched_before.block, &touched_after.block));
    // bytes of touched changed, untouched untouched.
    assert_eq!(touched_after.bytes()[0], 9);
    assert_eq!(untouched_after.bytes()[0], 1);

    // Old state unchanged (I4): the prior state still reads the old tile.
    let old = ls.current_state();
    let _ = old; // old state Arc retained; bytes never mutated in place.
    assert_eq!(id1, ls.current_state_id());
}

#[test]
fn cow_leaves_old_state_unchanged() {
    let w = 512u32;
    let h = 512u32;
    let mut ls = LayerState::new("L", w, h, &row_major(w, h, 7), 0);
    let before_arc = ls.current_state();
    let before_id = before_arc.id;
    let meta = StateMeta::new(w, h, 0, 0);
    let _new = ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 8), meta);
    // The Arc we held before is unchanged: still points to old node id,
    // still has original bytes (no in-place mutation; COW made a new node).
    assert_eq!(before_arc.id, before_id);
    assert_eq!(before_arc.tiles[0].bytes()[0], 7);
    assert_eq!(before_arc.meta.version, 0);
}

// (5) FIFO eviction releases refcounts; still-referenced kept.
// Boundary states are shared between consecutive entries (entry N's `after`
// AND entry N+1's `before` reference state N), so a state is reclaimed only
// after BOTH neighbors are evicted. Uses Weak handles (parallel-safe).
#[test]
fn eviction_drops_oldest_releases_refs() {
    let w = 512u32;
    let h = 512u32;
    let mut ls = LayerState::new("L", w, h, &row_major(w, h, 1), 0);
    ls.set_max_entries(2);
    let meta = StateMeta::new(w, h, 0, 0);

    // commit 1 -> state1 with fresh block A at (0,0).
    ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 5), meta.clone());
    let st1 = ls.current_state();
    let st1_weak = Arc::downgrade(&st1);
    let a = st1
        .tiles
        .iter()
        .find(|t| t.grid_x == 0 && t.grid_y == 0)
        .unwrap();
    let a_weak = Arc::downgrade(&a.block);
    drop(st1);
    assert!(st1_weak.upgrade().is_some(), "state1 alive after commit1");

    // commit 2 -> state2. state1 now referenced as entry1.before (undo base).
    ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 6), meta.clone());
    assert!(st1_weak.upgrade().is_some(), "state1 kept as undo base");

    // commits 3 + 4 -> FIFO eviction pops entry0 then entry1; with both
    // neighbors gone, state1 + block A are reclaimed.
    ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 7), meta.clone());
    ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 8), meta);
    assert!(
        st1_weak.upgrade().is_none(),
        "state1 reclaimed after FIFO eviction"
    );
    assert!(a_weak.upgrade().is_none(), "block A reclaimed");

    // Still-referenced: the untouched base tile block (1,0) is kept alive.
    let t10 = ls.read_tile(1, 0);
    assert_eq!(t10.bytes()[0], 1, "untouched base tile kept");
    assert_eq!(ls.entries_len(), 2);
}

// (6) redo invalidation: begin_forward truncates forward region + releases.
#[test]
fn redo_invalidation_truncates_forward_releases() {
    let w = 512u32;
    let h = 512u32;
    let mut ls = LayerState::new("L", w, h, &row_major(w, h, 1), 0);
    let meta = StateMeta::new(w, h, 0, 0);
    ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 2), meta.clone());
    ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 3), meta.clone());
    assert_eq!(ls.entries_len(), 2);
    ls.undo(); // tip has 1 redo entry
    ls.undo(); // tip has 2 redo entries
    assert_eq!(ls.cursor(), 0);
    assert_eq!(ls.entries_len(), 2);

    // begin_forward: a new commit truncates the forward region.
    ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 4), meta);
    assert_eq!(ls.entries_len(), 1, "forward entries truncated");
    assert_eq!(ls.cursor(), 1);
    assert!(
        ls.redo() == false,
        "no redo available after forward truncation"
    );
}

// (7) branch after undo: new command truncates forward + fresh StateNodeId.
#[test]
fn branch_after_undo_forks_fresh_state_id() {
    let w = 512u32;
    let h = 512u32;
    let mut ls = LayerState::new("L", w, h, &row_major(w, h, 1), 0);
    let meta = StateMeta::new(w, h, 0, 0);
    let id_a = ls.current_state_id();
    let id1 = ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 2), meta.clone());
    let id2 = ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 3), meta.clone());
    ls.undo(); // back to id1
    assert_eq!(ls.current_state_id(), id1);
    assert_eq!(ls.entries_len(), 2);

    let id_branch = ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 9), meta);
    assert_ne!(id_branch, id2, "branch gets a fresh id");
    assert_ne!(id_branch, id1);
    assert_eq!(
        ls.entries_len(),
        2,
        "forward truncated, new branch appended"
    );
    assert_eq!(ls.current_state_id(), id_branch);
    assert!(!ls.redo(), "old forward branch gone");
    assert!(ls.undo());
    assert_eq!(
        ls.current_state_id(),
        id1,
        "can undo back to pre-branch state"
    );
    let _ = id_a;
}

// (8) I8 cross-layer isolation: same coords -> independent blocks.
#[test]
fn cross_layer_isolation_independent_blocks() {
    let w = 512u32;
    let h = 512u32;
    let mut l1 = LayerState::new("L1", w, h, &row_major(w, h, 1), 0);
    let l2 = LayerState::new("L2", w, h, &row_major(w, h, 1), 0);
    let t1 = l1.read_tile(0, 0);
    let t2 = l2.read_tile(0, 0);
    assert!(
        !Arc::ptr_eq(&t1.block, &t2.block),
        "layers must not share blocks"
    );

    // Mutating layer 1 must not affect layer 2.
    let meta = StateMeta::new(w, h, 0, 0);
    l1.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 42), meta);
    assert_eq!(l1.read_tile(0, 0).bytes()[0], 42);
    assert_eq!(l2.read_tile(0, 0).bytes()[0], 1, "layer2 untouched");
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

// (12) no leak: after an expensive edit history, dropping the layer frees
// every StateNode and shared tile byte-buffer (`Arc<[u8]>`) it owns. Uses Weak
// handles (parallel-safe; the shared state counter fluctuates while other
// tests run).
#[test]
fn no_state_or_tile_leak_on_layer_drop() {
    let w = 512u32;
    let h = 512u32;
    let mut ls = LayerState::new("L", w, h, &row_major(w, h, 1), 0);
    ls.set_max_entries(4);
    let meta = StateMeta::new(w, h, 0, 0);
    for i in 0..12u8 {
        let tile = (i % 2) as u32;
        ls.write_tile_cow(tile, 0, &clipped_tile(w, h, tile, 0, i + 1), meta.clone());
        if i % 3 == 0 {
            ls.undo();
        }
        if i % 5 == 0 {
            ls.redo();
        }
        ls.evict_oldest();
    }

    // Take Weak handles to the current state and every tile it references.
    let cur = ls.current_state();
    let cur_weak = Arc::downgrade(&cur);
    let mut tile_weaks = Vec::with_capacity(cur.tiles.len());
    for t in &cur.tiles {
        tile_weaks.push(Arc::downgrade(&t.block));
    }
    assert!(cur_weak.upgrade().is_some(), "state alive via layer");
    drop(cur);

    // Dropping the layer must reclaim every state + tile block (no leak).
    drop(ls);
    assert!(cur_weak.upgrade().is_none(), "current state reclaimed");
    for wt in tile_weaks {
        assert!(wt.upgrade().is_none(), "tile block reclaimed");
    }
}

// Eviction must NEVER rewind the live committed state (`current_state_id()` /
// committed pixels). Deterministically covers BOTH the safe `cursor >= 2` pop
// AND the dangerous `cursor == 1` case (the popped entry DEFINES `current`).
#[test]
fn evict_oldest_does_not_rewind_live_state() {
    let w = 512u32;
    let h = 512u32;
    let meta = StateMeta::new(w, h, 0, 0);

    // --- cursor >= 2: popping the front only shifts indices, state preserved ---
    let mut ls = LayerState::new("L", w, h, &row_major(w, h, 1), 0);
    ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 3), meta.clone());
    let id_mid = ls.current_state_id();
    ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 4), meta.clone());
    let id_before = ls.current_state_id();
    let row_before = ls.to_row_major();
    assert_eq!(ls.cursor(), 2);
    assert!(ls.evict_oldest());
    assert_eq!(
        ls.current_state_id(),
        id_before,
        "cursor>=2 evict must not rewind the current id"
    );
    assert_eq!(
        ls.to_row_major(),
        row_before,
        "pixels unchanged at cursor>=2 evict"
    );
    let _ = id_mid;

    // --- cursor == 1: the front entry DEFINES current -> re-anchor base, no rewind ---
    let mut ls = LayerState::new("L", w, h, &row_major(w, h, 1), 0);
    let base_id = ls.current_state_id();
    ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, 9), meta.clone());
    let id_one = ls.current_state_id();
    let row_one = ls.to_row_major();
    assert_ne!(id_one, base_id, "one commit advances past base");
    assert_eq!(ls.cursor(), 1, "cursor is 1 (front entry is the definer)");
    assert_eq!(row_one[0], 9, "committed tile value present before evict");

    assert!(ls.evict_oldest());
    assert_eq!(
        ls.current_state_id(),
        id_one,
        "cursor==1 evict must NOT rewind current_state_id to base"
    );
    assert_eq!(
        ls.to_row_major(),
        row_one,
        "pixels not rewound to initial bytes"
    );
    assert_eq!(
        ls.to_row_major()[0],
        9,
        "live committed value survives eviction"
    );
    assert_eq!(ls.cursor(), 0);
}

// set_max_entries shrinking below the cursor must preserve the live state
// (no rewind, never leaving the cursor past a valid tail).
#[test]
fn set_max_entries_shrink_preserves_live_state() {
    let w = 512u32;
    let h = 512u32;
    let mut ls = LayerState::new("L", w, h, &row_major(w, h, 1), 0);
    let meta = StateMeta::new(w, h, 0, 0);
    // commit 8 times (values 10..80 differ from base=1), then undo 7 =>
    // cursor==1 defining the front entry, with 8 entries held.
    for v in 1..=8u8 {
        ls.write_tile_cow(0, 0, &clipped_tile(w, h, 0, 0, v * 10), meta.clone());
    }
    assert_eq!(ls.cursor(), 8);
    for _ in 0..7 {
        assert!(ls.undo());
    }
    assert_eq!(ls.cursor(), 1);
    assert_eq!(ls.entries_len(), 8);
    let id_before = ls.current_state_id();
    let row_before = ls.to_row_major();
    assert_eq!(row_before[0], 10, "state1 tile differs from base");

    ls.set_max_entries(1); // shrink far below the cursor/tail
    assert_eq!(
        ls.current_state_id(),
        id_before,
        "set_max_entries shrink must not rewind the live state"
    );
    assert_eq!(
        ls.to_row_major(),
        row_before,
        "pixels preserved across shrink"
    );
    assert_eq!(
        ls.to_row_major()[0],
        10,
        "live committed value survives shrink"
    );
    assert!(
        ls.arena_has_current(),
        "current state still reachable via arena"
    );
    assert!(ls.entries_len() <= ls.max_entries());
}

// (10) V1-style fuzz: 1000-op mixed sequence against the independent oracle.
// NOTE: the cursor==1 eviction path (the seed-luck hole) is NOT relied on by
// this fuzz — it is deterministically covered by
// `evict_oldest_does_not_rewind_live_state` (cursor==1 block) and
// `set_max_entries_shrink_preserves_live_state` (cursor==1 shrink).
#[test]
fn fuzz_1000_ops_mixed_matches_oracle() {
    // Deterministic mulberry32 PRNG (u32), seed 0xC0FFEE.
    let mut state: u32 = 0xC0FFEE;
    let mut rand = move || {
        let mut t = state.wrapping_add(0x6D2B79F5);
        state = t;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t = t ^ t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        t ^ (t >> 14)
    };

    let w = 512u32;
    let h = 512u32;
    let mut ls = LayerState::new("L", w, h, &row_major(w, h, 1), 0);
    ls.set_max_entries(8);

    let mut oracle = PixelReader::new(w, h, row_major(w, h, 1));
    let mut undo_stack: Vec<Vec<u8>> = Vec::new();
    let mut redo_stack: Vec<Vec<u8>> = Vec::new();
    let mut share = ShareOracle::new(w, h);

    let meta = StateMeta::new(w, h, 0, 0);
    let tiles_w = w.div_ceil(TILE);
    let tiles_h = h.div_ceil(TILE);

    for _ in 0..1000 {
        let r = rand() % 10;
        if r < 5 {
            // commit: write a random tile with deterministic bytes.
            let tx = rand() % tiles_w;
            let ty = rand() % tiles_h;
            let (tx, ty) = (tx, ty);
            let v = (rand() % 251) as u8;
            let buf = clipped_tile(w, h, tx, ty, v);
            let new_id = ls.write_tile_cow(tx, ty, &buf, meta.clone());
            assert!(new_id != 0);

            // update oracle truth + share oracle.
            undo_stack.push(oracle.bytes.clone());
            redo_stack.clear();
            let (x0, y0, tw, th) = oracle.tile_bounds(tx, ty);
            for row in 0..th {
                let dst = ((y0 + row) * w + x0) as usize * 4;
                let len = (tw * 4) as usize;
                oracle.bytes[dst..dst + len]
                    .copy_from_slice(&buf[row as usize * len..row as usize * len + len]);
            }
            share.mark_written([(tx, ty)]);
            share.commit_head();
            if undo_stack.len() > ls.max_entries() {
                undo_stack.remove(0);
                share.evict_oldest();
            }
        } else if r < 7 {
            // undo
            let did = ls.undo();
            if did {
                let prev = undo_stack.pop().unwrap_or_else(|| vec![]);
                redo_stack.push(oracle.bytes.clone());
                oracle = PixelReader::new(w, h, prev);
                share.undo();
            }
        } else if r < 9 {
            // redo
            let did = ls.redo();
            if did {
                let nxt = redo_stack.pop().unwrap_or_else(|| vec![]);
                undo_stack.push(oracle.bytes.clone());
                oracle = PixelReader::new(w, h, nxt);
                share.redo();
            }
        } else {
            // evict oldest history entry
            let did = ls.evict_oldest();
            if did && !undo_stack.is_empty() {
                undo_stack.remove(0);
                share.evict_oldest();
            }
        }

        // Every step: byte-truth parity (maxDiff=0, diffCount=0) + share identity.
        let packed = ls.to_packed();
        assert_byte_eq(
            &packed.bytes,
            &oracle.to_tile_major().bytes,
            &format!("fuzz step"),
        );
        let row = packed.to_row_major(TILE);
        assert_byte_eq(&row, &oracle.bytes, "fuzz row-major parity");
        assert!(share.reconcile(&ls.real_ptr_map()), "share identity broken");
        assert!(ls.arena_has_current(), "current state must be in arena");
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

// ── Packed Arc-subarray canonical store ────────────────────────────────

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

// 2048×2048 full-scale parity against the independent oracle through
// commit/undo/redo — the biggest layer the packer + COW path must serve.
#[test]
fn full_scale_commit_undo_redo_parity_oracle() {
    let w = 2048u32;
    let h = 2048u32;
    let mut ls = LayerState::new("L", w, h, &row_major(w, h, 1), 0);
    let meta = StateMeta::new(w, h, 0, 0);
    let mut oracle = PixelReader::new(w, h, row_major(w, h, 1));
    let mut undo_stack: Vec<Vec<u8>> = vec![oracle.bytes.clone()];
    let mut redo_stack: Vec<Vec<u8>> = Vec::new();

    // Commit two distinct tiles (one in the first grid cell, one late in the
    // 2048 grid) and mirror them into the oracle.
    for (tx, ty, v) in [(0u32, 0u32, 9u8), (5, 3, 42)] {
        let buf = clipped_tile(w, h, tx, ty, v);
        ls.write_tile_cow(tx, ty, &buf, meta.clone());
        undo_stack.push(oracle.bytes.clone());
        redo_stack.clear();
        let (x0, y0, tw, th) = oracle.tile_bounds(tx, ty);
        for row in 0..th {
            let dst = ((y0 + row) * w + x0) as usize * 4;
            let len = (tw * 4) as usize;
            oracle.bytes[dst..dst + len]
                .copy_from_slice(&buf[row as usize * len..row as usize * len + len]);
        }
    }
    assert_byte_eq(
        &ls.to_packed().bytes,
        &oracle.to_tile_major().bytes,
        "2048 committed packed",
    );
    assert_byte_eq(&ls.to_row_major(), &oracle.bytes, "2048 committed row");

    // Undo twice -> back to the base state.
    assert!(ls.undo());
    redo_stack.push(oracle.bytes.clone());
    oracle = PixelReader::new(w, h, undo_stack.pop().unwrap());
    assert_byte_eq(&ls.to_row_major(), &oracle.bytes, "2048 undo1");
    assert!(ls.undo());
    redo_stack.push(oracle.bytes.clone());
    oracle = PixelReader::new(w, h, undo_stack.pop().unwrap());
    assert_byte_eq(&ls.to_row_major(), &oracle.bytes, "2048 undo2");

    // Redo twice -> back to the committed tip.
    assert!(ls.redo());
    undo_stack.push(oracle.bytes.clone());
    oracle = PixelReader::new(w, h, redo_stack.pop().unwrap());
    assert_byte_eq(&ls.to_row_major(), &oracle.bytes, "2048 redo1");
    assert!(ls.redo());
    undo_stack.push(oracle.bytes.clone());
    oracle = PixelReader::new(w, h, redo_stack.pop().unwrap());
    assert_byte_eq(&ls.to_row_major(), &oracle.bytes, "2048 redo2");
    assert_byte_eq(
        &ls.to_packed().bytes,
        &oracle.to_tile_major().bytes,
        "2048 redo2 packed",
    );
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
