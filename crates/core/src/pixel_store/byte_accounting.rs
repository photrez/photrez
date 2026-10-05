// SPDX-License-Identifier: AGPL-3.0-or-later
//! Byte accounting for the canonical pixel store: how many pixel bytes it
//! actually holds, and how many of them are retained once and referenced by more
//! than one committed state.
//!
//! Two byte populations exist and they share NOTHING with each other:
//!
//! 1. The row-major mirror - `PixelLayer::pixels`, one `Vec<u8>` per layer of
//!    exactly `width*height*4` bytes. It is the ACTIVE canonical byte source
//!    (the `STATE_NODE_CANONICAL` and `TILE_MAJOR` flags are both off), it is
//!    written through on every commit, undo and redo, and it holds the layer's
//!    pixels independently of the tile graph. Its size is read from
//!    `Vec::len()`, never recomputed from the dimensions.
//! 2. The tile graph - the `Arc<[u8]>` storage reached through `TileRef`s on the
//!    per-layer anchor, the per-layer current state, and every `Pixel` history
//!    entry's before/after `Arc<StateNode>`. A TILE counts ONCE however many
//!    states reference it; that de-duplication is the structural sharing a memory
//!    budget has to be measured against.
//!
//!    The unit is the tile, not the `Arc<[u8]>` block. `tiled_state_node` packs a
//!    whole layer into ONE block at distinct offsets, so a block-level key would
//!    hand `shared_bytes` the entire block - including the byte regions of tiles
//!    that were edited and are no longer referenced from it.
//!
//! What is deliberately NOT counted, because the store does not track it:
//! - the `TileRef` / `StateNode` / `Arc` headers and the `Vec`s holding them
//!   (a 256x256 layer carries 16 `TileRef`s per state at ~40 bytes each against
//!   262,144 bytes of pixels);
//! - `Vec` over-allocation, so a buffer that grew and was never shrunk reports
//!   its length, not its capacity;
//! - the host-side `ImageBitmap` generations and the TypeScript undo stack, which
//!   belong to another owner entirely.
//!
//! Cost. `PixelLayer::row_major_bytes` is O(1). `history_tile_bytes` visits every
//! committed state - two per layer plus two per retained history entry, the
//! stream being capped at its `max_depth` - so it is O(states x tiles per state):
//! ~16 tiles per state for a 4096x4096 layer, a few thousand tile visits for a
//! full stream. It is a diagnostic probe; the row-major accessor is the cheap one
//! to read on a hot path.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::history::EntryPayload;

use super::{DocumentPixelStore, PixelLayer, PixelStoreRegistry};

/// Pixel bytes retained by the tile graph, split by how many committed states
/// reference each TILE. Every field counts tiles the store actually holds; none is
/// re-derived from the layer dimensions.
///
/// The unit is the tile (`TileRef::identity_key()` = `(data_ptr, offset)`), not
/// the `Arc<[u8]>` block. `tiled_state_node` packs a whole layer into ONE block at
/// distinct offsets, so a block-level key would attribute every byte of that
/// block - including the regions of tiles that were edited and are no longer
/// referenced from it - to `shared_bytes`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TileByteReport {
    /// Sum of `w*h*4` over the DISTINCT tiles reachable from the per-layer anchor,
    /// the per-layer current state, and every retained history entry. This is the
    /// tile graph's real retained pixel bytes.
    pub total_bytes: u64,
    /// Of `total_bytes`, the bytes of tiles two or more committed states reference.
    /// A fresh copy-on-write tile is referenced by one state and is therefore
    /// private; an untouched tile keeps its `(data_ptr, offset)` identity across
    /// states, so it counts once here however many states reach it. Only the
    /// UNTOUCHED tiles are included - never the whole block an edited tile came
    /// from.
    pub shared_bytes: u64,
    /// `total_bytes - shared_bytes`: tiles only one state references.
    pub private_bytes: u64,
    /// Distinct tiles counted.
    pub tile_count: u64,
    /// Distinct committed states visited (per-layer anchor + current, plus each
    /// retained history entry's before/after).
    pub state_count: u64,
    /// Tile references visited across those states, BEFORE de-duplication. The gap
    /// against `tile_count` is how much sharing is in effect.
    pub tile_reference_count: u64,
}

/// One document's byte footprint, as read by the read-only
/// `rust_pixels_store_bytes` Tauri command.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PixelStoreBytes {
    /// Layers holding a row-major buffer in this document.
    pub layer_count: u64,
    /// Row-major mirror bytes, summed from each layer's `Vec::len()`.
    pub row_major_bytes: u64,
    /// Tile-graph bytes, counted once per distinct tile.
    pub tile_graph: TileByteReport,
    /// `row_major_bytes + tile_graph.total_bytes`: the store's whole pixel
    /// footprint. `u64` because the two populations are summed, and a 32-bit
    /// accumulator wraps at ~64 layers of 4096x4096.
    pub total_bytes: u64,
}

/// Per-tile tally while walking. `owners` is a set rather than a counter so that
/// the same state reached twice (a layer's anchor IS also some entry's `before`)
/// does not inflate the sharing term.
struct TileTally {
    bytes: u64,
    owners: HashSet<usize>,
}

impl PixelLayer {
    /// The row-major buffer's own length, in bytes. O(1), and exact for the
    /// mirror: the buffer is allocated at `width*height*4` and never resized
    /// afterwards (a tile write copies into it, it does not grow it).
    pub fn row_major_bytes(&self) -> u64 {
        self.pixels.len() as u64
    }
}

impl DocumentPixelStore {
    /// Row-major mirror bytes for every layer in this document. O(layers).
    pub fn row_major_bytes(&self) -> u64 {
        self.layers
            .values()
            .map(PixelLayer::row_major_bytes)
            .fold(0u64, |acc, n| acc.saturating_add(n))
    }

    /// Tile-graph bytes, de-duplicated by TILE identity.
    ///
    /// The key is `TileRef::identity_key()` - the `(data_ptr, offset)` pair the
    /// production touch-set already uses - NOT the block pointer alone.
    /// `tiled_state_node` packs every tile of a layer into ONE `Arc<[u8]>` at
    /// distinct offsets, so keying on the pointer alone makes a 4096x4096 layer's
    /// whole 67,108,864-byte anchor read as ONE unit and hands the entire block to
    /// `shared_bytes` as soon as two states reach it - including the regions of
    /// tiles that were edited and are no longer referenced from it. Keying per
    /// tile and accumulating `w*h*4` is the same arithmetic
    /// `state_node_delta_memory_cost` already uses.
    ///
    /// Distinct keys cannot alias: every block visited stays alive behind a live
    /// `Arc` reachable from `&self` for the whole walk, so the allocator cannot
    /// hand a freed block's address to a new one mid-walk.
    ///
    /// The STATE key is the `Arc<StateNode>` allocation address, NOT
    /// `StateNode::id`. Ids are minted per `LayerState` from that LayerState's
    /// own `Arena` starting at 0 (`cow.rs:30-37`), and `init_layer` /
    /// `resize_layer` drop the `LayerState` while `history.invalidate_layer`
    /// EARLY-RETURNS under a `pending_external` barrier
    /// (`document_core.rs:433`). A stale `Pixel` entry therefore survives with its
    /// `Arc`s alive while the next commit's fresh `LayerState` mints id 0 again -
    /// and an id-keyed dedup would skip the stale generation's states entirely,
    /// dropping bytes that are demonstrably still live. The allocation address
    /// cannot collide for the same reason tile keys cannot: both states are held by
    /// a strong `Arc` for the whole walk.
    ///
    /// States walked, and nothing else holds pixel bytes: each layer's anchor
    /// and current `Arc<StateNode>`, then each `Pixel` entry's before/after pair.
    /// The `Arena` holds only `Weak`, so it retains nothing and is not walked.
    /// `pending_pixel` is drained by `take()` inside the command that fills it
    /// (`DocumentPixelStore::apply_command`), so it is `None` whenever this walk
    /// can run. Its `Vec<TilePatch>` half holds row-major bytes that no walked
    /// state references, and is likewise not resident here.
    pub fn history_tile_bytes(&self) -> TileByteReport {
        let mut tiles: HashMap<(usize, usize), TileTally> = HashMap::new();
        let mut states: HashSet<usize> = HashSet::new();
        let mut tile_reference_count: u64 = 0;

        let visit = |tiles: &mut HashMap<(usize, usize), TileTally>,
                     states: &mut HashSet<usize>,
                     refs: &mut u64,
                     node: &Arc<crate::state_node::StateNode>| {
            // A layer's anchor is ALSO the `before` of its first entry, so the
            // same state arrives more than once. Visiting it once keeps
            // `tile_reference_count` the number of references that EXIST rather
            // than the number of times the walk passed over them.
            let state = Arc::as_ptr(node) as usize;
            if !states.insert(state) {
                return;
            }
            for tile in &node.tiles {
                *refs += 1;
                tiles
                    .entry(tile.identity_key())
                    .or_insert_with(|| TileTally {
                        bytes: (tile.w * tile.h * 4) as u64,
                        owners: HashSet::new(),
                    })
                    .owners
                    .insert(state);
            }
        };

        for layer_state in self.state_nodes.values() {
            for node in [layer_state.base_state(), layer_state.current_state()] {
                visit(&mut tiles, &mut states, &mut tile_reference_count, &node);
            }
        }

        for entry in &self.history.entries {
            let EntryPayload::Pixel { before, after, .. } = &entry.payload else {
                continue;
            };
            for node in [before, after] {
                visit(&mut tiles, &mut states, &mut tile_reference_count, node);
            }
        }

        let total_bytes = tiles.values().map(|t| t.bytes).sum();
        let shared_bytes = tiles
            .values()
            .filter(|t| t.owners.len() > 1)
            .map(|t| t.bytes)
            .sum();
        TileByteReport {
            total_bytes,
            shared_bytes,
            private_bytes: total_bytes - shared_bytes,
            tile_count: tiles.len() as u64,
            state_count: states.len() as u64,
            tile_reference_count,
        }
    }
}

impl PixelStoreRegistry {
    /// Read-only byte footprint for one document: the row-major mirror summed
    /// from each layer's buffer length, plus the tile graph counted once per
    /// distinct block.
    ///
    /// `&self` only - no buffer write, epoch bump, cursor move, `version()` bump
    /// or `Arc<StateNode>` mutation - and no pixel bytes are returned, so two
    /// calls on an unchanged document are byte-identical. `None` for a document
    /// this registry does not hold, so an unknown id can never read as a
    /// zero-byte document.
    pub fn get_store_bytes(&self, doc_id: &str) -> Option<PixelStoreBytes> {
        let doc = self.docs.get(doc_id)?;
        let row_major_bytes = doc.row_major_bytes();
        let tile_graph = doc.history_tile_bytes();
        Some(PixelStoreBytes {
            layer_count: doc.layers.len() as u64,
            row_major_bytes,
            total_bytes: row_major_bytes.saturating_add(tile_graph.total_bytes),
            tile_graph,
        })
    }
}

#[cfg(test)]
mod tests;
