// SPDX-License-Identifier: AGPL-3.0-or-later
//! Production canonical persistent-pixel-ownership data model:
//! `StateNode -> TileRef -> Arc<[u8]>` (packed, tile-major) + an ARENA index
//! (Weak, not strong), plus a byte-free `LayerAccess`-style seam (COW).
//!
//! PACKED-STORE MODEL: the base/live layer is ONE packed `Arc<[u8]>`
//! (tile-major, contiguous, edge tiles clipped) — a single allocation, not N
//! per-tile allocations. Each `TileRef` is a SUBARRAY into that shared buffer:
//! `block` holds the shared packed `Arc<[u8]>` and `offset` is the tile's byte
//! offset within it. Untouched tiles keep sharing the same `Arc` (and the same
//! offset); an edited tile (COW) gets a FRESH `Arc<[u8]>` (offset 0) holding just
//! that tile's bytes. `bytes()` yields a zero-allocation `&[u8]` slice.
//!
//! Invariants honored: I1 immutable published state;
//! I2 no public mutable backing; I3 mutation => COW; I4 old state unchanged on
//! COW; I5 one refcount mechanism (Arc); I6 zero-ref reclaimed; I7 redo
//! invalidation releases; I8 per-layer scope (per-layer packed buffer); I9
//! immutable nodes never edited in place.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

// `state_node` is now a PRODUCTION module. The independent-copy parity
// oracle (test-only) is referenced ONLY by the `#[cfg(test)]` helpers below
// (`to_packed` / `to_row_major`) — production code keeps zero
// parity-oracle dependency.
#[cfg(test)]
use crate::parity_oracle::{assert_byte_eq, PackedView, PixelReader};

/// Canonical tile edge in pixels. Must match the engine value (256).
pub const TILE: u32 = 256;

pub type StateNodeId = u64;

/// Live-object counter for leak tests (test build only). Tile byte-buffers are
/// tracked via `Arc` refcounts (`Weak`/`strong_count`), not a global counter,
/// because a packed `Arc<[u8]>` is shared by many `TileRef`s.
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

/// The sharing unit is an `Arc<[u8]>` (immutable straight-alpha tile bytes). For
/// a tile in the live packed buffer it is a SUBARRAY of the shared packed
/// `Arc<[u8]>` (via `offset`); for an edited tile it is a FRESH `Arc<[u8]>` that
/// owns just that tile's bytes (`offset == 0`). Immutable after publication;
/// mutation goes through COW (a fresh Arc).
#[derive(Debug)]
pub struct TileRef {
    pub grid_x: u32,
    pub grid_y: u32,
    /// clipped (right/partial) width.
    pub w: u32,
    /// clipped (bottom/partial) height.
    pub h: u32,
    /// Byte offset of this tile within `block` (0 for a fresh COW tile).
    pub offset: usize,
    /// THE tile byte buffer (Arc, not StateNodeId). Shared by untouched tiles.
    pub block: Arc<[u8]>,
}

impl TileRef {
    /// Build a tile ref that is a SUBARRAY of the shared packed buffer.
    pub fn subarray(
        grid_x: u32,
        grid_y: u32,
        w: u32,
        h: u32,
        offset: usize,
        block: Arc<[u8]>,
    ) -> Self {
        let len = (w * h * 4) as usize;
        assert!(offset + len <= block.len(), "subarray over-read");
        TileRef {
            grid_x,
            grid_y,
            w,
            h,
            offset,
            block,
        }
    }

    /// Build a tile ref that OWNS a fresh block (COW edited tile). `bytes` is
    /// copied exactly once into the fresh `Arc<[u8]>` (no double-copy).
    pub fn owning(grid_x: u32, grid_y: u32, w: u32, h: u32, bytes: &[u8]) -> Self {
        assert_eq!((w * h * 4) as usize, bytes.len(), "owning tile byte len");
        TileRef {
            grid_x,
            grid_y,
            w,
            h,
            offset: 0,
            block: bytes.into(),
        }
    }

    /// Zero-allocation borrow of this tile's clipped bytes.
    pub fn bytes(&self) -> &[u8] {
        let len = (self.w * self.h * 4) as usize;
        &self.block[self.offset..self.offset + len]
    }

    /// Exact per-tile identity for the production touch-set: an injective
    /// `(data_ptr, offset)` pair. Two tiles share the same packed `Arc<[u8]>`
    /// subarray iff both the data pointer AND the byte offset are equal — this
    /// is the HashMap key for `state_node_touched_patches`, so distinct tiles
    /// never collide and an untouched tile keeps the SAME key across COW.
    pub(crate) fn identity_key(&self) -> (usize, usize) {
        (self.block.as_ref().as_ptr() as usize, self.offset)
    }

    fn clone_for_ret(&self) -> TileRef {
        TileRef {
            grid_x: self.grid_x,
            grid_y: self.grid_y,
            w: self.w,
            h: self.h,
            offset: self.offset,
            block: self.block.clone(),
        }
    }
}

/// Immutable state descriptor. Owns NO pixel bytes directly; bytes live in the
/// `Arc<[u8]>` (shared packed subarray) referenced by the tiles. Immutable
/// after publication.
#[derive(Debug)]
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

/// Byte-free LayerAccess-style seam over the current canonical StateNode.
/// Owns NO pixel bytes; bytes live in `Arc<[u8]>` (shared packed subarray) via
/// the current StateNode. The base layer is the ONE packed `Arc<[u8]>`.
// `layer_id` scopes the state per layer (I8) and is retained for the model.
#[allow(dead_code)]
pub struct LayerState {
    layer_id: String,
    arena: Arena,
    current_id: StateNodeId,
    /// The layer's permanent anchor state (state[0]); always alive.
    base: Arc<StateNode>,
    /// The STRONG Arc of the live current state, kept in lockstep with
    /// `current_id`. `cow_batch` / `set_current` advance it directly (the
    /// caller owns any cursor / history stream).
    current_arc: Arc<StateNode>,
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
    let tiles_w = w.div_ceil(TILE);
    let tiles_h = h.div_ceil(TILE);
    // ONE tile-major packed buffer for the whole base layer (single alloc;
    // pre-sized to avoid the realloc-doubling ingest regression). Edge tiles
    // are clipped to layer bounds; concatenating them yields exactly w*h*4 bytes.
    let tile_count = (tiles_w * tiles_h) as usize;
    let mut packed = Vec::with_capacity((w * h * 4) as usize);
    let mut offsets = Vec::with_capacity(tile_count);
    for tx in 0..tiles_w {
        for ty in 0..tiles_h {
            let x0 = tx * TILE;
            let y0 = ty * TILE;
            let tw = (w - x0).min(TILE);
            let th = (h - y0).min(TILE);
            offsets.push(packed.len());
            for r in 0..th {
                let src = ((y0 + r) * w + x0) as usize * 4;
                let len = (tw * 4) as usize;
                packed.extend_from_slice(&row_major[src..src + len]);
            }
        }
    }
    let packed: Arc<[u8]> = packed.into();
    let mut tiles = Vec::with_capacity(tile_count);
    let mut oi = 0usize;
    for tx in 0..tiles_w {
        for ty in 0..tiles_h {
            let x0 = tx * TILE;
            let y0 = ty * TILE;
            let tw = (w - x0).min(TILE);
            let th = (h - y0).min(TILE);
            tiles.push(TileRef::subarray(
                tx,
                ty,
                tw,
                th,
                offsets[oi],
                packed.clone(),
            ));
            oi += 1;
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

/// A change region in absolute layer pixel coords (top-left origin, RGBA rows).
/// `data` is `w*h*4` straight-alpha. Used by the production COW seam; it can
/// be a FULL clipped tile OR an arbitrary sub-tile dirty rect (the tile the
/// region intersects is re-tiled / spliced on write).
pub struct RegionChange {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

impl RegionChange {
    pub fn new(x: u32, y: u32, w: u32, h: u32, data: Vec<u8>) -> Self {
        RegionChange { x, y, w, h, data }
    }
}

#[cfg(test)]
mod bench;
mod cow;
#[cfg(test)]
mod tests;
