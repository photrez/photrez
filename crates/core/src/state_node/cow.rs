// SPDX-License-Identifier: AGPL-3.0-or-later
//! Production copy-on-write seam for the `state_node` data model. Holds the `Arena`
//! re-anchor helper (`insert_weak_for`) and the `LayerState` COW seam used by
//! `pixel_store.rs` (`cow_batch` / `set_current` / `current_state`). Kept separate
//! from `mod.rs` to keep the model file under the 1000-line guard without losing
//! any logic.

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

// `LayerState` is the production copy-on-write seam over the current canonical
// `StateNode`. The write path is `cow_batch` / `set_current` (both called from
// `pixel_store.rs`); `current_state` returns the live `Arc<StateNode>`. The
// `Arena` re-anchor helper `insert_weak_for` keeps the current state reachable
// via the arena after undo/redo.
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
        }
    }

    /// The live current state = `current_arc` (kept in lockstep with
    /// `current_id`; `cow_batch`/`set_current` re-anchor it directly).
    fn current(&self) -> &Arc<StateNode> {
        &self.current_arc
    }

    pub fn current_state(&self) -> Arc<StateNode> {
        self.current().clone()
    }

    /// Batch-COW: apply a set of change regions (absolute layer px; full-tile
    /// OR sub-tile dirty rects) into the CURRENT state and return a `(before,
    /// after)` pair of `Arc<StateNode>` for the caller's authoritative history
    /// (e.g. `ProtocolEngine`). This is the production seam: it produces the two
    /// `Arc<StateNode>`s an `EntryPayload::Pixel` entry needs WITHOUT touching
    /// this layer's internal state / cursor (the caller owns the cursor).
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
}
