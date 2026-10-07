// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tests for `byte_accounting`.
//!
//! Every expectation below is written from INDEPENDENT constants (a literal
//! width/height and the RGBA stride), never by re-running the accessor - a test
//! that recomputes the expected value with the same arithmetic as the
//! implementation proves nothing.
//!
//! Reference figures for the scenarios used here, at 4 bytes per RGBA pixel:
//! - 4x4 layer: 4*4*4 = 64 bytes, one 4x4 tile (tiles are 256px, so a 4x4 layer
//!   is a single clipped tile).
//! - 300x300 layer: 300*300*4 = 360,000 bytes. Divided into 256px tiles that is
//!   2x2 tiles: one full 256x256 (262,144 B) plus three clipped ones
//!   (256*44*4 = 45,056 B twice, 44*44*4 = 7,744 B) - exactly 360,000.
//! - 4096x4096 layer: 16x16 = 256 tiles of 256x256, each 262,144 B, so
//!   256 * 262,144 = 67,108,864 B - exactly 4096*4096*4.
//! - 512x512 layer: 2x2 = 4 tiles of 256x256, each 262,144 B, so 4 * 262,144 =
//!   1,048,576 B - exactly 512*512*4.
//!
//! The unit under test is the TILE, keyed by `(data_ptr, offset)`. A commit that
//! re-tiles tile T gives T a fresh identity in the new state while the other tiles
//! keep the anchor's, so the untouched tiles are the shared ones and the re-tiled
//! ones are private - in BOTH the anchor's copy of T and the fresh copy.

use super::*;
use crate::pixel_store::TilePatch;

fn tile(x: i64, y: i64, w: usize, h: usize, fill: u8) -> TilePatch {
    TilePatch {
        x,
        y,
        w,
        h,
        data: vec![fill; w * h * 4],
    }
}

fn open(doc: &str) -> PixelStoreRegistry {
    let mut reg = PixelStoreRegistry::new();
    reg.open_document(doc);
    reg
}

/// A store with one `w`x`h` layer seeded with `fill`.
fn seeded(w: u32, h: u32, fill: u8) -> (PixelStoreRegistry, String, String) {
    let mut reg = PixelStoreRegistry::new();
    let doc = "acct-doc".to_string();
    let layer = "acct-layer".to_string();
    reg.open_document(&doc.clone());
    reg.add_layer(&doc, &layer, w, h, vec![fill; w as usize * h as usize * 4])
        .expect("seed layer");
    (reg, doc, layer)
}

/// The load-bearing claim: the row-major figure is the buffer's OWN length.
/// A layer at 5x7 has 35 pixels = 140 bytes, and the accessor must report 140
/// from the `Vec` rather than from a formula - so this pins the number, and the
/// DEFEAT below pins the SOURCE.
#[test]
fn row_major_bytes_is_the_buffers_own_length() {
    let (reg, doc, layer) = seeded(5, 7, 3);
    let layer_ref = reg.get_layer(&doc, &layer).expect("layer present");
    assert_eq!(layer_ref.pixels.len(), 140, "fixture: 5*7*4 = 140 bytes");
    assert_eq!(
        layer_ref.row_major_bytes(),
        140,
        "read the buffer length, do not recompute it"
    );
    assert_eq!(
        reg.get_store_bytes(&doc)
            .expect("doc present")
            .row_major_bytes,
        140,
        "one layer, so the document sum is that layer's own buffer length"
    );
}

/// A zero-dimension layer is a REAL state the store accepts at the
/// `PixelLayer::new` level only if `pixels.len() == w*h*4`, so 0x0 with an empty
/// buffer is well-formed and must read as 0 bytes rather than panicking or
/// reading a phantom 4.
#[test]
fn a_zero_dimension_layer_reads_zero_bytes() {
    let mut reg = PixelStoreRegistry::new();
    reg.open_document("zero-doc");
    reg.add_layer("zero-doc", "zero-layer", 0, 0, Vec::new())
        .expect("a 0x0 layer with an empty buffer is well formed");
    let bytes = reg.get_store_bytes("zero-doc").expect("doc present");
    assert_eq!(bytes.layer_count, 1, "the layer exists");
    assert_eq!(
        bytes.row_major_bytes, 0,
        "0x0 RGBA is 0 bytes - not a phantom 4, not a panic"
    );
    assert_eq!(bytes.total_bytes, 0, "no pixel bytes anywhere");
}

/// The store's row-major figure is the sum over layers, each read from its own
/// buffer. Two layers of DIFFERENT sizes, so a per-layer cross-wired read cannot
/// pass: 5x7 = 140 and 9x2 = 72 sum to 212.
#[test]
fn document_row_major_sums_each_layers_own_buffer() {
    let mut reg = PixelStoreRegistry::new();
    reg.open_document("sum-doc");
    reg.add_layer("sum-doc", "a", 5, 7, vec![0; 5 * 7 * 4])
        .expect("a");
    reg.add_layer("sum-doc", "b", 9, 2, vec![0; 9 * 2 * 4])
        .expect("b");
    let bytes = reg.get_store_bytes("sum-doc").expect("doc present");
    assert_eq!(bytes.layer_count, 2);
    assert_eq!(bytes.row_major_bytes, 140 + 72, "5*7*4 + 9*2*4 = 212");
}

/// THE COST CLAIM, and the sharing baseline. A 4x4 layer is a SINGLE 256px tile,
/// so one copy-on-write of any region inside it re-tiles that tile into a fresh
/// owning block: the tile graph holds TWO 64-byte blocks over TWO states - not
/// one, and not four - and NOTHING is shared. The anchor's tile still reads the
/// old block, the committed state's tile reads the new one, so each block has
/// exactly one owner.
///
/// This is the case any accounting that reported a shared half here would be
/// flattering itself: a single-tile layer has nothing to share, and the
/// multi-tile sharing is measured in the 300x300 case below.
#[test]
fn one_copy_on_write_of_the_only_tile_shares_nothing() {
    let (mut reg, doc, layer) = seeded(4, 4, 0);
    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 9)])
        .expect("commit opens a Pixel entry and copy-on-writes the tile");

    let bytes = reg.get_store_bytes(&doc).expect("doc present");
    let g = &bytes.tile_graph;
    assert_eq!(
        g.state_count, 2,
        "the anchor and the state after the commit"
    );
    assert_eq!(
        g.tile_count, 2,
        "the anchor's one tile, plus the fresh copy-on-write tile"
    );
    assert_eq!(g.total_bytes, 128, "two distinct 64-byte blocks");
    assert_eq!(
        g.shared_bytes, 0,
        "one owner per block: the layer is a single tile, so the commit replaced all of it"
    );
    assert_eq!(g.private_bytes, 128, "both blocks have exactly one owner");
    assert_eq!(g.tile_reference_count, 2, "one tile per state, two states");
}

/// A second commit on the same single-tile layer replaces the first fresh block.
/// The old one is still held by the first entry's `after`, so three distinct
/// 64-byte blocks must be reported across three states - and still nothing is
/// shared, because each block has exactly one owner.
#[test]
fn a_second_commit_retains_the_intermediate_block() {
    let (mut reg, doc, layer) = seeded(4, 4, 0);
    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 9)])
        .expect("first commit");
    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 7)])
        .expect("second commit");

    let g = reg.get_store_bytes(&doc).expect("doc present").tile_graph;
    assert_eq!(
        g.state_count, 3,
        "anchor, after the first commit, after the second"
    );
    assert_eq!(
        g.tile_count, 3,
        "the anchor's tile, plus one fresh tile per commit"
    );
    assert_eq!(g.total_bytes, 192, "three distinct 64-byte blocks");
    assert_eq!(
        g.shared_bytes, 0,
        "three blocks, three owners each one: the history stream retains them, it does not share them"
    );
    assert_eq!(g.private_bytes, 192);
}

/// THE DEFEAT TARGET, part 1: the account must reach the LAYER ANCHOR, not only
/// the states the history stream names. After one commit the history stream's two
/// states already cover both blocks, so this cannot fail on a 1-commit store -
/// it fails on a store whose only state is the anchor (no commit at all), where
/// the tile graph is 64 bytes and not 0.
#[test]
fn the_layer_anchor_is_counted_even_with_no_history() {
    let (mut reg, doc, layer) = seeded(4, 4, 0);
    // Touch the layer's state_node seam the way any commit does, WITHOUT
    // recording an entry, so the anchor is the only reachable state.
    let ls = reg
        .docs
        .get_mut(&doc)
        .expect("doc")
        .layer_state(&layer)
        .expect("layer state materializes");
    let (before, after) = ls.cow_batch(
        &[crate::state_node::RegionChange::new(
            0,
            0,
            2,
            2,
            vec![1; 2 * 2 * 4],
        )],
        1,
    );
    assert_eq!(
        before.id,
        after.id.checked_sub(1).expect("ids advance"),
        "sanity"
    );

    let g = reg.get_store_bytes(&doc).expect("doc present").tile_graph;
    assert_eq!(
        g.total_bytes, 128,
        "the anchor block (64) plus the fresh tile block (64) are both retained"
    );
    assert_eq!(g.state_count, 2, "the anchor and the current state");
    // No history entry exists, so no state is shared between two owners.
    assert_eq!(
        g.shared_bytes, 0,
        "one owner per block here: sharing needs a second referent, not two states per layer"
    );
}

/// THE SPLIT CLAIM. `shared_bytes` must be the sum of only the UNTOUCHED tiles,
/// keyed per tile - never the whole packed `Arc<[u8]>` a layer is built into.
///
/// A 300x300 layer is 2x2 tiles (256x256, 256x44, 44x256, 44x44) packed into ONE
/// 360,000-byte block. A commit inside the 256x256 tile re-tiles ONLY that tile, so
/// the three others keep the anchor's identity and are genuinely shared:
/// 45,056 + 45,056 + 7,744 = 97,856. Keying on the block pointer instead reports
/// the whole 360,000 here, over-stating by 3.7x.
#[test]
fn shared_bytes_covers_only_the_untouched_tiles_not_the_whole_block() {
    let (mut reg, doc, layer) = seeded(300, 300, 0);
    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 9)])
        .expect("commit inside the first 256px tile");
    let g = reg.get_store_bytes(&doc).expect("doc present").tile_graph;
    assert_eq!(g.state_count, 2, "anchor plus the committed state");
    assert_eq!(
        g.tile_count, 5,
        "the anchor's 4 tiles plus 1 fresh tile for the re-tiled 256x256"
    );
    assert_eq!(
        g.total_bytes, 622_144,
        "360,000 anchor tiles + 262,144 fresh 256*256*4"
    );
    assert_eq!(
        g.shared_bytes, 97_856,
        "only the three untouched tiles (45,056 + 45,056 + 7,744); NOT the 360,000-byte block they happen to live in"
    );
    assert_eq!(
        g.private_bytes, 524_288,
        "the re-tiled 256x256 in the anchor (262,144) plus its fresh copy (262,144): one owner each"
    );
    // NOT `shared + private == total`: `private_bytes` is COMPUTED as
    // `total - shared`, so that partition is an identity by construction and cannot
    // fail for any input. These two are real properties a broken walk changes: the
    // anchor's 4 tiles plus the committed state's 4, and the visit-once dedup
    // collapsing the anchor (which is also the entry's `before`) to one visit.
    assert_eq!(
        g.tile_reference_count, 8,
        "4 tiles in the anchor + 4 in the committed state, each visited once"
    );
    assert_eq!(
        g.state_count, 2,
        "the anchor is also the entry's `before`, so the dedup visits 2 states, not 3"
    );
}

/// THE MAJOR CASE, and the load-bearing test in this file: a MULTI-TILE layer
/// where a commit edits SOME tiles. 4096x4096 is 16x16 = 256 tiles of 256x256
/// (262,144 B each). This commit re-tiles the 8 tiles at tx 0..2, ty 0..4, so:
///
/// - 248 tiles keep the anchor's identity and are reached by BOTH states.
///   248 * 262,144 = 65,011,712 B genuinely shared.
/// - 8 re-tiled tiles are private twice over: the anchor's copy of each (the new
///   state no longer points at it) and the fresh copy. 16 * 262,144 = 4,194,304 B.
///
/// Keying on the block pointer instead reports the ENTIRE 67,108,864-byte anchor as
/// shared - 67,108,864 against the 65,011,712 that is genuinely shared, a 1.032x
/// over-statement, and up to 32x on this same layer once 248 of its 256 tiles have
/// been re-tiled. That is why the unit is the tile.
#[test]
fn a_multi_tile_commit_shares_only_the_tiles_it_did_not_edit() {
    const TILE_BYTES: u64 = 256 * 256 * 4; // 262,144
    const GRID: u64 = 4096 / 256; // 16 tiles per axis
    const EDITED: u64 = 8; // tx 0..2 x ty 0..4

    let (mut reg, doc, layer) = seeded(4096, 4096, 0);
    let mut patches = Vec::new();
    for ty in 0..4i64 {
        for tx in 0..2i64 {
            patches.push(tile(tx * 256, ty * 256, 256, 256, 9));
        }
    }
    reg.apply_pixel_patch(&doc, &layer, vec![], patches)
        .expect("commit over 8 of the 256 tiles");

    let g = reg.get_store_bytes(&doc).expect("doc present").tile_graph;
    let untouched = GRID * GRID - EDITED; // 248
    assert_eq!(g.state_count, 2, "anchor plus the committed state");
    assert_eq!(
        g.tile_count,
        GRID * GRID + EDITED,
        "256 anchor tiles + 8 fresh tiles"
    );
    assert_eq!(
        g.total_bytes,
        GRID * GRID * TILE_BYTES + EDITED * TILE_BYTES,
        "the whole layer plus the 8 re-tiled tiles"
    );
    assert_eq!(
        g.shared_bytes,
        untouched * TILE_BYTES,
        "65,011,712 - the 248 tiles that kept the anchor's identity"
    );
    assert_eq!(
        g.private_bytes,
        EDITED * TILE_BYTES * 2,
        "the anchor's 8 re-tiled tiles plus their 8 fresh copies"
    );
    assert!(
        g.shared_bytes < 4096 * 4096 * 4,
        "the shared figure must be smaller than the whole layer ({} vs {}): a value equal to the \
         full block means the packed Arc was counted as one unit",
        g.shared_bytes,
        4096 * 4096 * 4
    );
}

/// THE SAME CLAIM, cheap. A 512x512 layer is 2x2 = 4 tiles of 262,144 B. A commit
/// re-tiling 3 of them leaves ONE untouched tile shared: exactly 262,144, not the
/// 1,048,576-byte block it lives in - a 4x over-statement if the unit were the
/// block. Cheap enough to run everywhere, so the 4096x4096 case above is not the
/// only thing standing between this and a wrong figure.
#[test]
fn one_untouched_tile_of_four_shares_exactly_that_tile() {
    const TILE_BYTES: u64 = 262_144;
    let (mut reg, doc, layer) = seeded(512, 512, 0);
    let patches = vec![
        tile(0, 0, 256, 256, 9),
        tile(256, 0, 256, 256, 9),
        tile(0, 256, 256, 256, 9),
    ];
    reg.apply_pixel_patch(&doc, &layer, vec![], patches)
        .expect("commit over 3 of the 4 tiles");

    let g = reg.get_store_bytes(&doc).expect("doc present").tile_graph;
    assert_eq!(g.shared_bytes, TILE_BYTES, "exactly the one untouched tile");
    assert_eq!(
        g.private_bytes,
        TILE_BYTES * 6,
        "3 re-tiled in the anchor + 3 fresh copies"
    );
    assert_eq!(g.total_bytes, TILE_BYTES * 7);
    assert_eq!(g.tile_count, 7, "4 anchor tiles + 3 fresh");
}

/// THE ARENA-RESET CASE. `StateNode` ids are minted per `LayerState` from that
/// LayerState's own `Arena`, starting at 0 (`cow.rs:30-37`). `init_layer` and
/// `resize_layer` both drop the `LayerState` (`pixel_store.rs:195-200`, `:213-218`),
/// and both call `history.invalidate_layer`, which EARLY-RETURNS while a
/// `pending_external` barrier is set (`document_core.rs:433`). A barrier is set
/// whenever the Undo/Redo walker lands on an `External` entry
/// (`document_core_apply.rs:975`) and is cleared only by a successful
/// `history_cursor_commit` (`history.rs:477`) - and production documents that a
/// FAILED `confirmExternalCursor` leaves it set (`facadeHistoryHandoff.ts`, the
/// `E_EXTERNAL_PENDING` arm).
///
/// So: one commit mints ids 0 and 1, then a barrier is set and the layer is
/// re-seeded, so the stale `Pixel` entry survives AND the next commit's fresh
/// `LayerState` mints ids 0 and 1 AGAIN. A dedup key of (slot, node.id) then
/// collides, and the first generation's blocks - still held alive by the stale
/// entry - are never visited, so their bytes vanish from the tally.
///
/// Four 64-byte tiles are live here (two generations x anchor + fresh).
#[test]
fn bytes_survive_an_arena_reset_while_the_invalidate_barrier_is_set() {
    let (mut reg, doc, layer) = seeded(4, 4, 0);

    // Generation 1: ids 0 (anchor) and 1 (committed).
    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 9)])
        .expect("first commit");
    assert_eq!(
        reg.get_store_bytes(&doc)
            .expect("doc")
            .tile_graph
            .total_bytes,
        128,
        "generation 1 holds two 64-byte tiles"
    );

    // The barrier the walker sets on an External handoff, left set because
    // `history_cursor_commit` failed. `rust_pixels_init` has no barrier guard.
    reg.docs
        .get_mut(&doc)
        .expect("doc")
        .history
        .pending_external = Some((1, "undo".to_string()));

    // Re-seed: drops the LayerState (so the next commit gets a FRESH Arena) and
    // calls `invalidate_layer`, which early-returns and keeps the stale entry.
    reg.add_layer(&doc, &layer, 4, 4, vec![0; 4 * 4 * 4])
        .expect("re-seed under the barrier");
    {
        let store = reg.docs.get(&doc).expect("doc");
        assert_eq!(
            store.state_nodes.len(),
            0,
            "the re-seed dropped the LayerState, so the next commit starts a new Arena"
        );
        assert!(
            store.history.pending_external.is_some(),
            "the barrier is still set"
        );
        assert_eq!(
            store.history.entries.len(),
            1,
            "`invalidate_layer` early-returned, so the stale Pixel entry survived and still \\
             holds generation 1's two Arc<StateNode>s alive"
        );
    }

    // Generation 2 mints ids 0 and 1 again - the collision.
    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 7)])
        .expect("second commit");

    let g = reg.get_store_bytes(&doc).expect("doc present").tile_graph;
    assert_eq!(
        g.total_bytes, 256,
        "four live 64-byte tiles across two arena generations; the first generation's \\
         tiles are still held by the stale entry and must still be counted"
    );
    assert_eq!(
        g.tile_count, 4,
        "the dedup key must be injective ACROSS an arena reset: 2 tiles per generation"
    );
    assert_eq!(
        g.state_count, 4,
        "two states per generation, so an id-collision key would collapse these to 2"
    );
    assert_eq!(
        g.tile_reference_count, 4,
        "4 states x 1 tile each. The re-seed emptied `state_nodes`, so generation 1 is \\
         reached ONLY through the stale entry and generation 2 through both the new \\
         LayerState and the new entry - each state still visited exactly once"
    );
}

/// A seeded layer that has never been committed has ONE state, not two: base and
/// current are the same `Arc` (`cow.rs:33-36`), so the visit-once dedup collapses
/// them. Pinned so a future reader is not surprised by `state_count == 1` on an
/// otherwise-untouched layer.
#[test]
fn a_seeded_but_uncommitted_layer_reports_one_state() {
    // Touch the state_node seam the way a commit does, WITHOUT a history entry,
    // so the only reachable state is the anchor.
    let (mut reg, doc, layer) = seeded(4, 4, 0);
    reg.docs
        .get_mut(&doc)
        .expect("doc")
        .layer_state(&layer)
        .expect("layer state materializes");
    let g = reg.get_store_bytes(&doc).expect("doc present").tile_graph;
    assert_eq!(
        g.state_count, 1,
        "base and current are the SAME Arc on an uncommitted layer, so the dedup \\
         collapses them: 1 state, not 2"
    );
    assert_eq!(g.total_bytes, 64, "the anchor's single 4x4 tile");
}

/// `total_bytes` and `tile_count` are UNAFFECTED by which unit the split uses:
/// summing per-tile `w*h*4` over distinct tiles covers exactly the same bytes as
/// summing distinct block lengths, because a layer's tiles tile its packed buffer
/// with no gaps. This is the regression pin for the split fix - the totals were
/// right under the block key and had to STAY right.
#[test]
fn the_totals_do_not_depend_on_the_key_the_split_uses() {
    let (mut reg, doc, layer) = seeded(300, 300, 0);
    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 9)])
        .expect("commit");
    let bytes = reg.get_store_bytes(&doc).expect("doc present");
    // 300*300*4 in the mirror, and 360,000 (anchor tiles) + 262,144 (fresh tile)
    // in the graph - the same two figures the block-level key produced.
    assert_eq!(bytes.row_major_bytes, 360_000);
    assert_eq!(bytes.tile_graph.total_bytes, 622_144);
    assert_eq!(bytes.total_bytes, 982_144, "360,000 + 622,144");
}

/// The whole footprint is the two populations added, and they are genuinely
/// independent: after one commit on a 4x4 layer the mirror is still 64 bytes even
/// though the tile graph now holds 128.
#[test]
fn total_is_the_two_populations_added_and_they_stay_independent() {
    let (mut reg, doc, layer) = seeded(4, 4, 0);
    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 9)])
        .expect("commit");
    let bytes = reg.get_store_bytes(&doc).expect("doc present");
    assert_eq!(
        bytes.row_major_bytes, 64,
        "a tile write copies INTO the mirror, so its length never moved"
    );
    assert_eq!(bytes.tile_graph.total_bytes, 128);
    assert_eq!(bytes.total_bytes, 192, "64 + 128");
}

/// Read-only-ness, in the shape `pixel_history_depth.rs` pins it: two calls on an
/// unchanged document are byte-identical, and neither the cursor nor the version
/// moved.
#[test]
fn accounting_is_read_only_and_repeatable() {
    let (mut reg, doc, layer) = seeded(4, 4, 0);
    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 9)])
        .expect("commit");
    let cursor_before = reg.get_history_cursor(&doc).expect("cursor");
    let version_before = reg.get_history_version(&doc).expect("version");
    let epoch_before = reg.get_epoch(&doc, &layer).expect("epoch");

    let first = reg.get_store_bytes(&doc).expect("first read");
    let second = reg.get_store_bytes(&doc).expect("second read");
    assert_eq!(first, second, "two identical read-only calls");
    assert_eq!(
        reg.get_history_cursor(&doc),
        Some(cursor_before),
        "the cursor must not advance"
    );
    assert_eq!(
        reg.get_history_version(&doc),
        Some(version_before),
        "the version must not bump"
    );
    assert_eq!(
        reg.get_epoch(&doc, &layer).expect("epoch"),
        epoch_before,
        "no canonical mutation, so the epoch holds"
    );
}

/// Undo re-anchors the layer canon onto a state the history stream already holds,
/// so the block set is unchanged by the undo itself. A drift here would mean the
/// walk is reading a state that the undo released.
#[test]
fn undo_re_anchors_without_changing_the_block_set() {
    let (mut reg, doc, layer) = seeded(4, 4, 0);
    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 9)])
        .expect("commit");
    let before_undo = reg.get_store_bytes(&doc).expect("bytes before undo");
    assert_eq!(before_undo.tile_graph.total_bytes, 128);

    reg.undo_pixel(&doc).expect("undo restores the anchor");
    let after_undo = reg.get_store_bytes(&doc).expect("bytes after undo");
    assert_eq!(
        after_undo.tile_graph.total_bytes, 128,
        "the redo branch still holds the committed state, so the bytes stay"
    );
    assert_eq!(
        after_undo.tile_graph.state_count, 2,
        "the canon re-anchored onto a state the stream already named"
    );
    assert_eq!(
        after_undo.row_major_bytes, 64,
        "the mirror never changed size"
    );
}

/// A closed document is not a zero-byte document: the accessor must return `None`
/// so a caller cannot read "this document costs nothing" for an id the registry
/// does not hold. This is the accessor half of the probe's bare-string rejection.
#[test]
fn an_unknown_or_closed_document_reads_none_not_zero() {
    let (mut reg, doc, _layer) = seeded(4, 4, 0);
    assert!(
        reg.get_store_bytes("never-opened").is_none(),
        "an id the registry never held must not resolve to a zero-byte report"
    );
    reg.close_document(&doc);
    assert!(
        reg.get_store_bytes(&doc).is_none(),
        "a closed document releases its storage and must not read as zero bytes"
    );
}

/// An opened document with no layers is a real report, not an error - the same
/// distinction the history probes draw between "no history" and "no document".
#[test]
fn an_open_but_empty_document_reports_zero() {
    let reg = open("empty-doc");
    let bytes = reg
        .get_store_bytes("empty-doc")
        .expect("an open document reads");
    assert_eq!(bytes.layer_count, 0);
    assert_eq!(bytes.row_major_bytes, 0);
    assert_eq!(bytes.total_bytes, 0);
    assert_eq!(bytes.tile_graph.state_count, 0);
    assert_eq!(bytes.tile_graph.tile_count, 0);
}

/// THE DEFEAT TARGET for the row-major figure: it must read the STORED buffer
/// length, not re-derive it from the dimensions. A buffer truncated after seeding
/// can no longer agree with `width*height*4`, so a derived formula reports 64
/// while the honest answer is 20. `pixels` is a public field, so a caller that
/// resized the buffer behind the store's back is reachable in a test - and the
/// accessor must describe what is actually stored, which is the only reading that
/// can be called a measurement rather than an estimate.
///
/// (The `u32` accumulator is the other half of the claim: a 32-bit sum wraps at
/// ~64 layers of 4096x4096, so the figure is `u64`. A buffer large enough to prove
/// that needs 4 GiB of real allocation, which is not worth a unit test; the type
/// is pinned by the fixture below exceeding a 16-bit accumulator.)
#[test]
fn row_major_bytes_reads_the_buffer_not_the_dimensions() {
    let (mut reg, doc, layer) = seeded(4, 4, 0);
    {
        let layer_ref = reg
            .docs
            .get_mut(&doc)
            .expect("doc")
            .layers
            .get_mut(&layer)
            .expect("layer");
        // 5 RGBA pixels = 20 bytes, while the dimensions still say 4*4*4 = 64.
        layer_ref.pixels.truncate(20);
    }
    let layer_ref = reg.get_layer(&doc, &layer).expect("layer present");
    assert_eq!(layer_ref.width, 4, "the dimensions are unchanged");
    assert_eq!(layer_ref.height, 4, "the dimensions are unchanged");
    assert_eq!(
        layer_ref.pixels.len(),
        20,
        "fixture: the stored buffer disagrees with width*height*4"
    );
    assert_eq!(
        layer_ref.row_major_bytes(),
        20,
        "report the stored length; a derived formula would say 64"
    );
    assert_eq!(
        reg.get_store_bytes(&doc)
            .expect("doc present")
            .row_major_bytes,
        20,
        "the document sum reads the same stored length"
    );
}

/// A seeded-but-never-committed document used to be reported as HALF of what the
/// store actually holds, with nothing in the response to say so: the packed canon
/// is built on a layer's first commit, so `tile_graph.total_bytes` was 0 and
/// `total_bytes` equalled the mirror alone. A caller reading that as "the
/// document's footprint" was wrong by exactly a layer - and the error looked like
/// a cheap document rather than a missing measurement.
///
/// The fix is a DERIVED figure, not a behaviour change and not a new
/// measurement: the canon is still built lazily (nothing here allocates one
/// eagerly), and `owed_anchor_bytes` names the anchor a not-yet-committed layer
/// will pack. Both halves are pinned: it must equal the anchor the first commit
/// actually allocates, and it must fall to zero.
#[test]
fn a_seeded_but_never_committed_document_reports_the_tile_graph_it_owes() {
    let (mut reg, doc, layer) = seeded(512, 512, 0);

    // 512*512*4 = 1,048,576 - one mirror, and (before this field existed) that
    // was the whole reported footprint.
    let before_commit = reg.get_store_bytes(&doc).expect("doc present");
    assert_eq!(before_commit.layer_count, 1);
    assert_eq!(before_commit.row_major_bytes, 1_048_576);
    assert_eq!(
        before_commit.tile_graph.total_bytes, 0,
        "the canon is built on the first commit, so the graph really is empty"
    );
    assert_eq!(
        before_commit.owed_anchor_layer_count, 1,
        "one layer holds a mirror and no canon"
    );
    assert_eq!(
        before_commit.owed_anchor_bytes, 1_048_576,
        "DERIVED from the dimensions: the anchor will be one packed block of width*height*4"
    );
    // The failure this exists to stop, stated as arithmetic rather than prose: the
    // old figure was half the truth, with no field carrying the other half.
    assert_eq!(
        before_commit.tile_graph.total_bytes + before_commit.owed_anchor_bytes,
        before_commit.row_major_bytes,
        "without the owed-anchor figure the report is half the store, silently"
    );

    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 9)])
        .expect("first commit");

    let after_commit = reg.get_store_bytes(&doc).expect("doc present");
    assert_eq!(
        after_commit.owed_anchor_layer_count, 0,
        "the canon exists now, so no layer owes one"
    );
    assert_eq!(after_commit.owed_anchor_bytes, 0);
    // 512*512 is 2x2 tiles of 256x256 at 262,144 B each = 1,048,576, which is
    // exactly the owed anchor. The equality is what makes the derived figure
    // trustworthy: it predicts the anchor, not the whole committed graph.
    assert_eq!(
        after_commit.tile_graph.total_bytes,
        1_048_576 + 262_144,
        "the anchor that was owed, PLUS a private copy of the one tile the 4x4 \
         commit re-tiled: 512*512 is 2x2 tiles of 262,144, and a sub-tile commit \
         still re-tiles its whole containing tile"
    );
    assert_eq!(
        after_commit.tile_graph.private_bytes,
        2 * 262_144,
        "TWO private tiles, not one: the anchor still holds its own copy of the \
         re-tiled tile and only the anchor reaches it, and the fresh copy is \
         reached only by the new state"
    );
    assert_eq!(
        after_commit.tile_graph.shared_bytes,
        3 * 262_144,
        "the other three tiles keep the anchor's identity and are shared"
    );
    assert_eq!(
        after_commit.row_major_bytes, 1_048_576,
        "the mirror is unchanged: the store now holds two layers' worth"
    );
}

/// A second layer joins a document whose first layer is already committed: only
/// the NEW layer owes an anchor, so the figure has to be per-layer rather than a
/// document-wide boolean. A document-level flag would say "something is owed"
/// without saying how much, which is the same incompleteness in a smaller hole.
#[test]
fn owed_anchor_bytes_counts_only_the_layers_that_owe_one() {
    let (mut reg, doc, layer) = seeded(512, 512, 0);
    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 9)])
        .expect("first commit");
    // 256x256*4 = 262,144: a second, still-uncommitted layer.
    reg.add_layer(&doc, "acct-layer-2", 256, 256, vec![0; 256 * 256 * 4])
        .expect("seed second layer");

    let bytes = reg.get_store_bytes(&doc).expect("doc present");
    assert_eq!(bytes.layer_count, 2);
    assert_eq!(
        bytes.owed_anchor_layer_count, 1,
        "only the layer that has never been committed"
    );
    assert_eq!(bytes.owed_anchor_bytes, 262_144);
    assert_eq!(
        bytes.row_major_bytes,
        1_048_576 + 262_144,
        "both mirrors count even though only one canon exists"
    );
}

/// A 0x0 layer is DERIVED-owed as zero bytes while still counting as an unbuilt
/// layer, because `width*height*4` is 0. That combination is a REAL response,
/// not a malformed one, and it is why the TypeScript guard on these two figures
/// has to point the other way: guarding "layers owed but zero bytes" would
/// reject this document. Pinned here so the reason survives being "tidied up".
#[test]
fn a_zero_dimension_unbuilt_layer_owes_no_bytes_but_still_counts() {
    let (reg, doc, _layer) = seeded(0, 0, 0);
    let bytes = reg.get_store_bytes(&doc).expect("doc present");
    assert_eq!(bytes.layer_count, 1, "the layer exists");
    assert_eq!(bytes.row_major_bytes, 0, "0x0 RGBA is 0 bytes");
    assert_eq!(bytes.owed_anchor_layer_count, 1, "never committed");
    assert_eq!(
        bytes.owed_anchor_bytes, 0,
        "width*height*4 is 0, so nothing is owed even though a layer owes"
    );
    assert_eq!(bytes.total_bytes, 0);
}

/// The two one-way guards the TypeScript wrapper enforces, pinned from the Rust
/// side so they cannot be tightened into rejecting a real response.
///
/// Every unbuilt layer is a SUBSET of the document's layers, and a non-zero
/// owed-bytes figure can only come from at least one layer having been counted.
/// The converse is NOT true (see the 0x0 case above), which is why there are two
/// guards and not one symmetric identity.
#[test]
fn owed_anchor_figures_never_exceed_the_layers_they_are_derived_from() {
    let (mut reg, doc, layer) = seeded(512, 512, 0);
    let before = reg.get_store_bytes(&doc).expect("doc present");
    assert!(
        before.owed_anchor_layer_count <= before.layer_count,
        "owed layers are a subset of the document's layers"
    );
    assert!(
        before.owed_anchor_bytes == 0 || before.owed_anchor_layer_count > 0,
        "non-zero owed bytes imply at least one layer was counted"
    );

    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 9)])
        .expect("first commit");
    let after = reg.get_store_bytes(&doc).expect("doc present");
    assert_eq!(after.owed_anchor_layer_count, 0);
    assert_eq!(after.owed_anchor_bytes, 0);
    assert!(
        after.owed_anchor_layer_count <= after.layer_count,
        "still a subset after the commit"
    );
}

/// Clause #10: does evicting old pixel-history entries actually release the
/// pixel bytes from the accounted tile graph?
///
/// Setup: a 4x4 layer is a single 256px tile, so every commit copy-on-writes
/// that tile into a fresh 64-byte block. A linear run of commits chains
/// s0 -> s1 -> ... -> sK, and the history walk counts every distinct block
/// still referenced from the anchor, the current state, or any retained
/// Pixel entry's before/after pair.
///
/// If eviction releases the bytes, the accounted total must PLATEAU once the
/// 50-entry cap is exceeded: each commit adds one fresh block (+64) and each
/// eviction drops the oldest entry, whose now-unreferenced state is freed
/// (-64), so at_N+1 == at_N+2. If the accounting retained evicted blocks,
/// the total would grow by 64 per commit with no bound.
#[test]
fn eviction_releases_the_evicted_blocks_from_the_tile_graph() {
    let (mut reg, doc, layer) = seeded(4, 4, 0);

    // N = 50 commits fills the cap exactly.
    for fill in 0..50u8 {
        reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, fill)])
            .expect("commit within the cap");
    }
    let at_n = reg.get_store_bytes(&doc).expect("doc present").tile_graph;
    assert_eq!(
        at_n.total_bytes,
        51 * 64,
        "anchor plus one fresh 64B block per commit"
    );

    // N+1 = 51 commits is the first eviction.
    reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, 50)])
        .expect("first evicting commit");
    let at_n_plus_1 = reg.get_store_bytes(&doc).expect("doc present").tile_graph;

    // Push 9 more past the cap: the oldest entries are evicted one by one.
    for fill in 51..60u8 {
        reg.apply_pixel_patch(&doc, &layer, vec![], vec![tile(0, 0, 4, 4, fill)])
            .expect("evicting commit");
    }
    let at_n_plus_10 = reg.get_store_bytes(&doc).expect("doc present").tile_graph;

    assert_eq!(
        at_n_plus_10.total_bytes, at_n_plus_1.total_bytes,
        "plateau required: at_51 = {} but at_60 = {}; \
         a leak would grow by 576 bytes over the 9 commits after the first eviction",
        at_n_plus_1.total_bytes, at_n_plus_10.total_bytes,
    );
    assert!(
        at_n_plus_10.total_bytes < at_n.total_bytes + 10 * 64,
        "must not retain every block the stream ever held"
    );
}
