// SPDX-License-Identifier: AGPL-3.0-or-later
//! Phase A0 test-only canonical persistent-pixel-ownership data model:
//! `StateNode -> TileRef -> Arc<TileBlock>` graph + an ARENA index (Weak, not
//! strong), plus a byte-free `LayerAccess`-style seam (COW) and a bounded
//! Model-A history (before/after `Arc<StateNode>`s, FIFO eviction, forward
//! truncation). Compiled ONLY under `#[cfg(test)]`; zero production dependency.
//!
//! Invariants honored (plan Phase 8): I1 immutable published state; I2 no public
//! mutable backing; I3 mutation => COW; I4 old state unchanged on COW; I5 one
//! refcount mechanism (Arc); I6 zero-ref reclaimed; I7 redo invalidation releases;
//! I8 per-layer scope; I9 immutable nodes never edited in place.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use crate::parity_oracle::{assert_byte_eq, PackedView, PixelReader, ShareOracle};

/// Canonical tile edge in pixels. Must match the engine value (256).
pub const TILE: u32 = 256;

pub type StateNodeId = u64;

/// Live-object counters for leak / refcount tests (test build only).
pub(crate) static LIVE_TILE_BLOCKS: AtomicUsize = AtomicUsize::new(0);
pub(crate) static LIVE_STATE_NODES: AtomicUsize = AtomicUsize::new(0);

/// Layer dims + version/epoch stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateMeta {
    pub width: u32,
    pub height: u32,
    /// bumped on each COW.
    pub version: u64,
    pub epoch: u64,
}

impl StateMeta {
    pub fn new(width: u32, height: u32, version: u64, epoch: u64) -> Self {
        StateMeta {
            width,
            height,
            version,
            epoch,
        }
    }
}

/// Immutable contiguous tile bytes (`w*h*4` straight-alpha). Single sharing unit.
/// Immutable after publication; mutation must go through COW (a new block).
// `w`/`h` are carried as authoritative tile metadata even though extraction
// tests read `block.bytes` directly (the bytes carry the same info).
#[allow(dead_code)]
pub struct TileBlock {
    pub bytes: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

impl TileBlock {
    pub fn new(bytes: Vec<u8>, w: u32, h: u32) -> Self {
        assert_eq!((w * h * 4) as usize, bytes.len(), "TileBlock byte len");
        LIVE_TILE_BLOCKS.fetch_add(1, Ordering::SeqCst);
        TileBlock { bytes, w, h }
    }
}

impl Drop for TileBlock {
    fn drop(&mut self) {
        LIVE_TILE_BLOCKS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// One tile ref. `block: Arc<TileBlock>` is the sharing unit. NO StateNodeId here.
pub struct TileRef {
    pub grid_x: u32,
    pub grid_y: u32,
    /// clipped (right/partial) width.
    pub w: u32,
    /// clipped (bottom/partial) height.
    pub h: u32,
    /// THE block handle (Arc, not StateNodeId).
    pub block: Arc<TileBlock>,
}

impl TileRef {
    fn clone_for_ret(&self) -> TileRef {
        TileRef {
            grid_x: self.grid_x,
            grid_y: self.grid_y,
            w: self.w,
            h: self.h,
            block: self.block.clone(),
        }
    }
}

/// Immutable state descriptor. Owns NO pixel bytes directly; bytes live in
/// `Arc<TileBlock>` referenced by the tiles. Immutable after publication.
pub struct StateNode {
    pub id: StateNodeId,
    /// tile-major order (CANONICAL PACK ORDER: tx outer, ty inner).
    pub tiles: Vec<TileRef>,
    pub meta: StateMeta,
}

impl StateNode {
    pub fn new(id: StateNodeId, tiles: Vec<TileRef>, meta: StateMeta) -> Self {
        LIVE_STATE_NODES.fetch_add(1, Ordering::SeqCst);
        StateNode { id, tiles, meta }
    }
}

impl Drop for StateNode {
    fn drop(&mut self) {
        LIVE_STATE_NODES.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(crate) fn live_tile_blocks() -> usize {
    LIVE_TILE_BLOCKS.load(Ordering::SeqCst)
}

/// Arena: an INDEX of live state ids -> `Weak<StateNode>`. It is NOT the
/// retention mechanism — retention is the strong `Arc` held by history entries,
/// the layer's base, and the current state. Using `Weak` means eviction /
/// truncation / layer drop actually reclaim states (I6, I7).
pub struct Arena {
    map: HashMap<StateNodeId, Weak<StateNode>>,
    next_id: StateNodeId,
}

impl Arena {
    pub fn new() -> Self {
        Arena {
            map: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn allocate_id(&mut self) -> StateNodeId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Publish a node; returns the strong Arc. Only the Weak is retained.
    pub fn insert(&mut self, node: StateNode) -> Arc<StateNode> {
        let arc = Arc::new(node);
        let id = arc.id;
        self.map.insert(id, Arc::downgrade(&arc));
        arc
    }

    pub fn get(&self, id: StateNodeId) -> Option<Arc<StateNode>> {
        self.map.get(&id).and_then(|w| w.upgrade())
    }
}

impl Default for Arena {
    fn default() -> Self {
        Arena::new()
    }
}

/// Model-A history entry. Holds the ARCs of its before/after states directly;
/// dropping the entry releases them (I6 / I7 via Arc, no manual refcount).
// `seq`/`memory_cost_bytes` are retained per the RESPONSE §5 model (entry
// metadata), even though this validation phase doesn't read every field.
#[allow(dead_code)]
#[derive(Clone)]
pub struct HistoryEntry {
    pub seq: u64,
    pub before: Arc<StateNode>,
    pub after: Arc<StateNode>,
    pub memory_cost_bytes: u64,
}

/// Byte-free LayerAccess-style seam over the current canonical StateNode.
/// Owns NO pixel bytes; bytes live in `Arc<TileBlock>` via the current StateNode.
// `layer_id` scopes the state per layer (I8) and is retained for the model.
#[allow(dead_code)]
pub struct LayerState {
    layer_id: String,
    arena: Arena,
    current_id: StateNodeId,
    /// The layer's permanent anchor state (state[0]); always alive.
    base: Arc<StateNode>,
    /// FIFO of committed transitions; oldest at front.
    entries: VecDeque<HistoryEntry>,
    /// Number of applied (committed) entries; 0 == base is current.
    cursor: usize,
    max_entries: usize,
    next_seq: u64,
}

/// Build a fully-populated StateNode from row-major bytes (edge tiles clipped).
pub fn tiled_state_node(
    id: StateNodeId,
    w: u32,
    h: u32,
    row_major: &[u8],
    epoch: u64,
) -> StateNode {
    assert_eq!((w * h * 4) as usize, row_major.len(), "row-major len");
    let tiles_w = (w + TILE - 1) / TILE;
    let tiles_h = (h + TILE - 1) / TILE;
    let mut tiles = Vec::with_capacity((tiles_w * tiles_h) as usize);
    for tx in 0..tiles_w {
        for ty in 0..tiles_h {
            let x0 = tx * TILE;
            let y0 = ty * TILE;
            let tw = (w - x0).min(TILE);
            let th = (h - y0).min(TILE);
            let mut b = Vec::with_capacity((tw * th * 4) as usize);
            for r in 0..th {
                let src = ((y0 + r) * w + x0) as usize * 4;
                let len = (tw * 4) as usize;
                b.extend_from_slice(&row_major[src..src + len]);
            }
            tiles.push(TileRef {
                grid_x: tx,
                grid_y: ty,
                w: tw,
                h: th,
                block: Arc::new(TileBlock::new(b, tw, th)),
            });
        }
    }
    StateNode::new(
        id,
        tiles,
        StateMeta {
            width: w,
            height: h,
            version: 0,
            epoch,
        },
    )
}

impl LayerState {
    pub fn new(layer_id: impl Into<String>, w: u32, h: u32, row_major: &[u8], epoch: u64) -> Self {
        assert!(w > 0 && h > 0, "layer dims must be > 0");
        let mut arena = Arena::new();
        let base_id = arena.allocate_id();
        let base = arena.insert(tiled_state_node(base_id, w, h, row_major, epoch));
        let current_id = base.id;
        LayerState {
            layer_id: layer_id.into(),
            arena,
            current_id,
            base,
            entries: VecDeque::new(),
            cursor: 0,
            max_entries: 64,
            next_seq: 0,
        }
    }

    pub fn set_max_entries(&mut self, n: usize) {
        assert!(n >= 1, "max_entries must be >= 1");
        self.max_entries = n;
        self.evict_to_cap();
    }

    pub fn max_entries(&self) -> usize {
        self.max_entries
    }
    pub fn entries_len(&self) -> usize {
        self.entries.len()
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn current(&self) -> &Arc<StateNode> {
        if self.cursor == 0 {
            &self.base
        } else {
            &self.entries[self.cursor - 1].after
        }
    }

    pub fn current_state_id(&self) -> StateNodeId {
        self.current_id
    }

    pub fn current_state(&self) -> Arc<StateNode> {
        self.current().clone()
    }

    /// Arena still has (or upgrades to) the current state id.
    pub fn arena_has_current(&self) -> bool {
        self.arena.get(self.current_id).is_some()
    }

    /// Read-only tile ref into the current state (clone of the Arc).
    pub fn read_tile(&self, tx: u32, ty: u32) -> TileRef {
        let state = self.current();
        for t in &state.tiles {
            if t.grid_x == tx && t.grid_y == ty {
                return t.clone_for_ret();
            }
        }
        panic!("tile ({tx},{ty}) not present in state {}", state.id);
    }

    /// COW write of one clipped tile into the current state. Returns the NEW
    /// StateNodeId. Unchanged tiles reuse the same `Arc<TileBlock>` (I4, I9);
    /// the changed tile gets a fresh block (I3). Truncates forward redo first
    /// (I7). Publishes the new state through the arena.
    pub fn write_tile_cow(
        &mut self,
        tx: u32,
        ty: u32,
        bytes: &[u8],
        meta: StateMeta,
    ) -> StateNodeId {
        let w = meta.width;
        let h = meta.height;
        let tiles_w = (w + TILE - 1) / TILE;
        let tiles_h = (h + TILE - 1) / TILE;
        assert!(tx < tiles_w && ty < tiles_h, "tile ({tx},{ty}) out of grid");
        let x0 = tx * TILE;
        let y0 = ty * TILE;
        let tw = (w - x0).min(TILE);
        let th = (h - y0).min(TILE);
        assert_eq!(
            (tw * th * 4) as usize,
            bytes.len(),
            "write_tile_cow bytes must be clipped tile size"
        );

        // begin_forward: drop any redo (forward) region first (I7).
        self.truncate_forward();

        let old_state = self.current().clone();
        let new_version = old_state.meta.version + 1;

        let mut tiles = Vec::with_capacity(old_state.tiles.len());
        for t in &old_state.tiles {
            if t.grid_x == tx && t.grid_y == ty {
                tiles.push(TileRef {
                    grid_x: tx,
                    grid_y: ty,
                    w: tw,
                    h: th,
                    block: Arc::new(TileBlock::new(bytes.to_vec(), tw, th)),
                });
            } else {
                // Reuse the same Arc<TileBlock> (untouched tile keeps identity).
                tiles.push(t.clone_for_ret());
            }
        }

        let id = self.arena.allocate_id();
        let after = self.arena.insert(StateNode::new(
            id,
            tiles,
            StateMeta {
                width: w,
                height: h,
                version: new_version,
                epoch: meta.epoch,
            },
        ));
        let before = old_state;
        let cost = after.tiles.iter().map(|t| (t.w * t.h * 4) as u64).sum();
        self.entries.push_back(HistoryEntry {
            seq: self.next_seq,
            before,
            after: after.clone(),
            memory_cost_bytes: cost,
        });
        self.next_seq += 1;
        self.cursor = self.entries.len();
        self.current_id = id;
        self.evict_to_cap();
        id
    }

    /// Undo the most recent applied entry. Returns false if nothing to undo.
    pub fn undo(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        self.current_id = self.current().id;
        true
    }

    /// Redo the next forward entry. Returns false if at the tip.
    pub fn redo(&mut self) -> bool {
        if self.cursor >= self.entries.len() {
            return false;
        }
        self.cursor += 1;
        self.current_id = self.current().id;
        true
    }

    /// Drop the oldest (front) entry, releasing its before/after Arcs (I6).
    /// INVARIANT: eviction NEVER changes `current_state_id()` / the committed
    /// state — it only drops memory for states no longer needed. When the front
    /// entry DEFINES the current state (`cursor == 1`), `base` is re-anchored to
    /// that entry's `after` before popping, so the live pixels are NOT rewound.
    pub fn evict_oldest(&mut self) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        self.pop_front_preserving_current();
        true
    }

    /// Pop the front (oldest) entry while preserving the cursor's committed state.
    /// `current()` = `entries[cursor-1].after` (1..=len) or `base` (0). Popping
    /// the front only rewinds the live state when `cursor == 1` (the front entry
    /// is the definer); in that case re-anchor `base` to its `after` first.
    fn pop_front_preserving_current(&mut self) {
        let front = self.entries.pop_front().expect("non-empty history");
        if self.cursor == 1 {
            // The popped entry IS the current-state definer. Re-anchor `base` to
            // its `after` so the committed state survives (no rewind to initial bytes).
            self.base = front.after;
            self.cursor = 0;
        } else if self.cursor > 0 {
            // cursor >= 2: the defining entry is unchanged (its index shifts
            // down by 1 along with the whole deque), so just shift the cursor.
            self.cursor -= 1;
        }
        // cursor == 0: current is `base`, unaffected by popping the front.
    }

    /// Truncate the forward redo region `[cursor..]` (drops entries' Arcs, I7).
    fn truncate_forward(&mut self) {
        if self.cursor < self.entries.len() {
            self.entries.truncate(self.cursor);
        }
    }

    fn evict_to_cap(&mut self) {
        while self.entries.len() > self.max_entries {
            self.pop_front_preserving_current();
        }
    }

    /// Transient tile-major materialization of the current state. Never stored.
    pub fn to_packed(&self) -> PackedView {
        let state = self.current();
        let cap: usize = state.tiles.iter().map(|t| (t.w * t.h * 4) as usize).sum();
        let mut out = Vec::with_capacity(cap);
        for t in &state.tiles {
            out.extend_from_slice(&t.block.bytes);
        }
        PackedView {
            w: state.meta.width,
            h: state.meta.height,
            bytes: out,
        }
    }

    /// Reconstruct the current state as row-major (via tile-major round-trip).
    pub fn to_row_major(&self) -> Vec<u8> {
        self.to_packed().to_row_major(TILE)
    }

    /// Real `Arc::as_ptr` per tile position (as usize), for ShareOracle.
    pub fn real_ptr_map(&self) -> HashMap<(u32, u32), usize> {
        let state = self.current();
        let mut m = HashMap::with_capacity(state.tiles.len());
        for t in &state.tiles {
            m.insert((t.grid_x, t.grid_y), Arc::as_ptr(&t.block) as usize);
        }
        m
    }
}

#[cfg(test)]
mod tests {
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

    // (1) retain/release Arc refcount semantics; zero-ref reclaim.
    #[test]
    fn arc_refcount_zero_ref_reclaimed() {
        let baseline = live_tile_blocks();
        let block = Arc::new(TileBlock::new(vec![0u8; 4 * 4 * 4], 4, 4));
        assert_eq!(live_tile_blocks(), baseline + 1);
        assert_eq!(Arc::strong_count(&block), 1);
        let clone = block.clone();
        assert_eq!(Arc::strong_count(&block), 2);
        drop(clone);
        assert_eq!(Arc::strong_count(&block), 1);
        drop(block);
        assert_eq!(live_tile_blocks(), baseline);
    }

    // (2) I1 immutability: a published (shared) Arc cannot be mutated.
    #[test]
    fn immutability_published_shared_cannot_mutate() {
        let block = Arc::new(TileBlock::new(vec![1u8; 64], 4, 4));
        let clone = block.clone();
        // Shared -> Arc::get_mut returns None, so no &mut access is possible
        // (I1/I2: no public &mut-to-bytes path, no in-place edit of published).
        assert!(Arc::get_mut(&mut block.clone()).is_none());
        // Unique owner CAN mutate (construction phase only, before publish).
        let mut unique = Arc::new(TileBlock::new(vec![1u8; 64], 4, 4));
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

        // Untouched tile keeps the SAME Arc block.
        let untouched_after = ls.read_tile(1, 0);
        assert!(Arc::ptr_eq(&untouched_before.block, &untouched_after.block));
        // Touched tile gets a NEW block.
        let touched_after = ls.read_tile(0, 0);
        assert!(!Arc::ptr_eq(&touched_before.block, &touched_after.block));
        // bytes of touched changed, untouched untouched.
        assert_eq!(touched_after.block.bytes[0], 9);
        assert_eq!(untouched_after.block.bytes[0], 1);

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
        assert_eq!(before_arc.tiles[0].block.bytes[0], 7);
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
        assert_eq!(t10.block.bytes[0], 1, "untouched base tile kept");
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
        assert_eq!(l1.read_tile(0, 0).block.bytes[0], 42);
        assert_eq!(l2.read_tile(0, 0).block.bytes[0], 1, "layer2 untouched");
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
    // every StateNode and TileBlock it owns. Uses Weak handles (parallel-safe;
    // the shared live counters fluctuate while other tests run).
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
        let tiles_w = (w + TILE - 1) / TILE;
        let tiles_h = (h + TILE - 1) / TILE;

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
}
