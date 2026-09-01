// SPDX-License-Identifier: AGPL-3.0-or-later
//! Production copy-on-write seam for the `state_node` data model. Holds the `Arena`
//! re-anchor helper (`insert_weak_for`) and the full `impl LayerState` COW /
//! history methods. Kept separate from `mod.rs` to keep the model file under the
//! 1000-line guard without losing any logic.

use super::*;

impl Arena {
    /// Re-index an ALREADY-EXISTING `Arc` into the arena (Weak only). Used to
    /// re-anchor an externally-returned state after undo/redo so the current
    /// state stays reachable via the arena.
    /// Idempotent: re-inserting an existing id just refreshes the Weak.
    pub fn insert_weak_for(&mut self, node: &Arc<StateNode>) {
        self.map.insert(node.id, Arc::downgrade(node));
    }
}

// `LayerState` is the production copy-on-write seam. Not all of its methods are
// wired into the row-major production path (row-major stays active); the
// write_tile_cow / undo / redo / evict / to_packed helpers are retained for the
// test-only coverage and the future canonical (flag-enabled) path.
#[allow(dead_code)]
impl LayerState {
    pub fn new(layer_id: impl Into<String>, w: u32, h: u32, row_major: &[u8], epoch: u64) -> Self {
        assert!(w > 0 && h > 0, "layer dims must be > 0");
        let mut arena = Arena::new();
        let base_id = arena.allocate_id();
        let base = arena.insert(tiled_state_node(base_id, w, h, row_major, epoch));
        let current_id = base.id;
        let current_arc = base.clone();
        LayerState {
            layer_id: layer_id.into(),
            arena,
            current_id,
            base,
            current_arc,
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

    /// The live current state = `current_arc` (kept in lockstep with
    /// `current_id` regardless of `entries`/`cursor`; `cow_batch`/undo/redo
    /// re-anchor it directly).
    fn current(&self) -> &Arc<StateNode> {
        &self.current_arc
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
    /// StateNodeId. Unchanged tiles reuse the same `Arc<[u8]>` shared packed
    /// subarray (I4, I9); the changed tile gets a FRESH `Arc<[u8]>` (I3).
    /// Truncates forward redo first (I7). Publishes the new state through the
    /// arena.
    pub fn write_tile_cow(
        &mut self,
        tx: u32,
        ty: u32,
        bytes: &[u8],
        meta: StateMeta,
    ) -> StateNodeId {
        let w = meta.width;
        let h = meta.height;
        let tiles_w = w.div_ceil(TILE);
        let tiles_h = h.div_ceil(TILE);
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
                // COW: the changed tile gets a FRESH Arc<[u8]> owning its bytes.
                tiles.push(TileRef::owning(tx, ty, tw, th, bytes));
            } else {
                // Reuse the same Arc<[u8]> + offset (untouched tile keeps identity).
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
        self.current_arc = after;
        self.evict_to_cap();
        id
    }

    /// Batch-COW: apply a set of change regions (absolute layer px; full-tile
    /// OR sub-tile dirty rects) into the CURRENT state and return a `(before,
    /// after)` pair of `Arc<StateNode>` for the caller's authoritative history
    /// (e.g. `ProtocolEngine`). This is the production seam: it produces the two
    /// `Arc<StateNode>`s an `EntryPayload::Pixel` entry needs WITHOUT touching
    /// this layer's internal history / cursor (the caller owns the cursor).
    /// Only tiles that INTERSECT a change region get a FRESH `Arc<[u8]>`
    /// (the region is spliced into the tile's current bytes); untouched tiles
    /// share the packed `Arc<[u8]>` subarray identity (I3/I4/I9). Invariant:
    /// no full-layer ingest per op — the changed tiles are rebuilt
    /// copy-on-touch, never the whole layer.
    pub fn cow_batch(
        &mut self,
        changes: &[RegionChange],
        epoch: u64,
    ) -> (Arc<StateNode>, Arc<StateNode>) {
        let before = self.current().clone();
        let w = before.meta.width;
        let h = before.meta.height;
        let mut tiles: Vec<TileRef> = before.tiles.iter().map(|t| t.clone_for_ret()).collect();
        for ch in changes {
            assert!(ch.w > 0 && ch.h > 0, "empty change region");
            assert_eq!(
                (ch.w * ch.h * 4) as usize,
                ch.data.len(),
                "RegionChange byte len"
            );
            assert!(
                ch.x + ch.w <= w && ch.y + ch.h <= h,
                "change region out of layer bounds"
            );
            let tx0 = ch.x / TILE;
            let ty0 = ch.y / TILE;
            let tx1 = (ch.x + ch.w - 1) / TILE;
            let ty1 = (ch.y + ch.h - 1) / TILE;
            for ty in ty0..=ty1 {
                for tx in tx0..=tx1 {
                    let idx = tiles
                        .iter()
                        .position(|t| t.grid_x == tx && t.grid_y == ty)
                        .expect("changed tile already present");
                    let tile_x = tx * TILE;
                    let tile_y = ty * TILE;
                    let tile_w = (w - tile_x).min(TILE);
                    let tile_h = (h - tile_y).min(TILE);
                    let mut new_bytes = tiles[idx].bytes().to_vec();
                    let rx0 = ch.x.max(tile_x);
                    let ry0 = ch.y.max(tile_y);
                    let rx1 = (ch.x + ch.w).min(tile_x + tile_w);
                    let ry1 = (ch.y + ch.h).min(tile_y + tile_h);
                    if rx0 < rx1 && ry0 < ry1 {
                        for row in ry0..ry1 {
                            let t_row = (row - tile_y) as usize;
                            let r_row = (row - ch.y) as usize;
                            let d_off = t_row * tile_w as usize * 4 + ((rx0 - tile_x) as usize) * 4;
                            let s_off = r_row * ch.w as usize * 4 + ((rx0 - ch.x) as usize) * 4;
                            let len = ((rx1 - rx0) as usize) * 4;
                            new_bytes[d_off..d_off + len]
                                .copy_from_slice(&ch.data[s_off..s_off + len]);
                        }
                    }
                    tiles[idx] = TileRef::owning(tx, ty, tile_w, tile_h, &new_bytes);
                }
            }
        }
        let id = self.arena.allocate_id();
        let after = self.arena.insert(StateNode::new(
            id,
            tiles,
            StateMeta::new(w, h, before.meta.version + 1, epoch),
        ));
        self.current_id = id;
        self.current_arc = after.clone();
        (before, after)
    }

    /// Re-anchor the current state to an externally-owned `Arc<StateNode>`
    /// (used to keep the canon in sync with `ProtocolEngine` after undo/redo).
    /// Publishes the node into the arena (Weak) so `arena_has_current` holds.
    pub fn set_current(&mut self, node: Arc<StateNode>) {
        self.arena.insert_weak_for(&node);
        self.current_id = node.id;
        self.current_arc = node;
    }

    /// Re-derive `current_arc`/`current_id` from the `entries`+`cursor` position
    /// (used after a cursor move in `undo`/`redo`). Keeps `current_arc` in
    /// lockstep with the cursor so `current()` is ALWAYS correct.
    fn sync_current_from_cursor(&mut self) {
        if self.cursor == 0 {
            self.current_arc = self.base.clone();
        } else {
            self.current_arc = self.entries[self.cursor - 1].after.clone();
        }
        self.current_id = self.current_arc.id;
    }

    /// Undo the most recent applied entry. Returns false if nothing to undo.
    pub fn undo(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        self.sync_current_from_cursor();
        true
    }

    /// Redo the next forward entry. Returns false if at the tip.
    pub fn redo(&mut self) -> bool {
        if self.cursor >= self.entries.len() {
            return false;
        }
        self.cursor += 1;
        self.sync_current_from_cursor();
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
    /// (Test-only: depends on the test-only `parity_oracle::PackedView`.)
    #[cfg(test)]
    pub fn to_packed(&self) -> PackedView {
        let state = self.current();
        let cap: usize = state.tiles.iter().map(|t| (t.w * t.h * 4) as usize).sum();
        let mut out = Vec::with_capacity(cap);
        for t in &state.tiles {
            out.extend_from_slice(t.bytes());
        }
        PackedView {
            w: state.meta.width,
            h: state.meta.height,
            bytes: out,
        }
    }

    /// Reconstruct the current state as row-major (via tile-major round-trip).
    #[cfg(test)]
    pub fn to_row_major(&self) -> Vec<u8> {
        self.to_packed().to_row_major(TILE)
    }

    /// Real per-tile identity (as usize), for ShareOracle. A tile
    /// is the (sharing `Arc<[u8]>`, `offset`) pair: distinct positions must never
    /// alias, and an untouched position must keep the SAME identity across COW.
    /// Packed-buffer subarray tiles share one `Arc` but differ by `offset`, so
    /// the identity mixes the buffer pointer with the tile's byte offset.
    #[cfg(test)]
    pub fn real_ptr_map(&self) -> HashMap<(u32, u32), usize> {
        let state = self.current();
        let mut m = HashMap::with_capacity(state.tiles.len());
        for t in &state.tiles {
            m.insert((t.grid_x, t.grid_y), t.identity());
        }
        m
    }
}
