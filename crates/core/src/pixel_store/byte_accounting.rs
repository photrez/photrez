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
//! - the `TileRef` / `StateNode` / `Arc` headers and the `Vec`s holding them;
//! - `Vec` over-allocation, so a buffer that grew and was never shrunk reports
//!   its length, not its capacity;
//! - the host-side `ImageBitmap` generations and the TypeScript undo stack, which
//!   belong to another owner entirely.
//!
//!   The header term is small enough not to matter at the sizes this is measured
//!   at, and it was measured rather than assumed: a 4096x4096 layer at the
//!   50-entry stream cap holds 52 states x 256 `TileRef`s x ~40 bytes of header =
//!   ~532 KB against a reported 214,433,792 bytes, i.e. **0.25%**. It is not
//!   negligible in principle, though - it scales as states x tiles, exactly like
//!   the figure it is excluded from, so it grows at the same rate and never
//!   becomes relatively smaller as history deepens.
//!
//! Cost. `PixelLayer::row_major_bytes` is O(1). `history_tile_bytes` visits every
//! committed state - two per layer plus two per retained history entry, the
//! stream being capped at its `max_depth` - so it is O(states x tiles per state).
//! The tile grid is 256px on a side, so tiles-per-state is `(side / 256)^2`:
//! **256** per state for a 4096x4096 layer and **64** for 2048x2048, not 16 (16
//! is the grid's extent per side, which is where that figure came from). At the
//! 50-entry cap that is 13,056 tile visits at 4096x4096 and 3,264 at 2048x2048 -
//! the visit COUNT is exact arithmetic and is asserted; the wall time is not.
//! Two runs of the same unoptimised build, in process and with no IPC hop, gave
//! 18.6-31.6 ms at 4096x4096 and 4.3-9.0 ms at 2048x2048 for that read: a 1.7-2.1x
//! spread from machine load alone, so quote it as an order of magnitude and
//! never as a figure. It is a diagnostic probe either way; the row-major
//! accessor is the cheap one to read on a hot path.

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
    ///
    /// A commit that re-tiles the whole layer does NOT drive this to zero. It
    /// reads exactly zero only when nothing else is retained: with a history
    /// stream behind it, the older states still reach the tiles they never
    /// touched, so the term DILUTES instead of collapsing. Measured: 16,515,072
    /// bytes at 2048x2048 (35.39% of the tile graph) and 66,846,720 at
    /// 4096x4096 (45.37%) after a 50-entry sub-tile stream plus one full-layer
    /// commit, against 0 for a document whose only commit re-tiles the layer.
    /// The distinction matters because a diluted ratio means sharing is working -
    /// one commit added a layer's worth of private tiles - while a zero on a
    /// document with history would mean it had stopped working.
    ///
    /// `shared_bytes` itself does not decay with depth, PROVIDED each commit
    /// touches fewer tiles than the layer has - one private tile per commit in
    /// the measured case, where every commit was a 4x4 region inside a single
    /// 256px tile. A commit that re-tiles N tiles adds N private ones instead, so
    /// the invariant is "sub-tile commits add a bounded, tile-local number of
    /// private tiles", not a fixed constant. What holds in every case is that
    /// untouched tiles keep their identity, so the shared term is unchanged by a
    /// commit and only the denominator grows.
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
    ///
    /// This is the ALLOCATED footprint, and it is not the whole story while
    /// `owed_anchor_bytes` is non-zero - read them together, never this field
    /// alone.
    pub total_bytes: u64,
    /// DERIVED, NOT MEASURED. `width*height*4` for each layer that holds a
    /// row-major buffer but has never been committed, so its packed canon does
    /// not exist yet (`layer_state` builds it on the layer's first commit).
    /// Zero once every layer has been committed at least once.
    ///
    /// It is named `owed_anchor` because that is exactly what it is: the ANCHOR
    /// block the first commit will pack, derived from the layer's DIMENSIONS
    /// (`width*height*4`) - deliberately not from the mirror's buffer length.
    /// Those two are not the same number: `row_major_bytes` reads `Vec::len()`,
    /// and a buffer can be shorter than its dimensions imply, which is exactly
    /// the state `row_major_bytes_reads_the_buffer_not_the_dimensions` pins. So
    /// this field and `row_major_bytes` are computed differently and can
    /// disagree, and neither is a correction of the other. Nothing here has been
    /// allocated, so this is not a measurement of memory in use and must never be
    /// added into a footprint figure as though it were.
    ///
    /// It says what a not-yet-committed layer WILL cost, and the arithmetic is
    /// pinned: a 512x512 layer owes 512*512*4 = 1,048,576, and after the first
    /// commit the anchor really is that size.
    ///
    /// It is NOT the committed tile graph either, and after the commit it is 0.
    /// A commit re-tiles every tile its region intersects, each becoming a fresh
    /// private copy, so a 4x4 commit into a 512x512 layer leaves the tile graph
    /// at 1,310,720 while `owed_anchor_bytes` is 0: 1,048,576 for the anchor
    /// plus TWO private 256x256 tiles - the anchor's own copy of the re-tiled
    /// tile, which only the anchor reaches, and the fresh copy only the new state
    /// reaches. Those per-commit private tiles are what `tile_graph` reports.
    ///
    /// Without this figure a freshly imported document reported its mirror and a
    /// tile graph of zero, and `total_bytes` read as a complete footprint when
    /// the store would hold exactly twice that the moment the first stroke landed.
    pub owed_anchor_bytes: u64,
    /// Layers counted in `owed_anchor_bytes`.
    pub owed_anchor_layer_count: u64,
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
    /// Layers whose packed canon does not exist yet, and the bytes it will cost
    /// when it is built: `width*height*4` each.
    ///
    /// `layer_state` creates a `LayerState` on a layer's FIRST commit, not at
    /// ingest, so a document that has been seeded but never committed has an
    /// empty tile graph by construction. Reporting that as "the tile graph is
    /// free" is the failure this pair of accessors exists to prevent: the number
    /// is not wrong, it is incomplete, and completeness is the caller's
    /// assumption.
    fn owed_anchor(&self) -> (u64, u64) {
        self.layers
            .iter()
            .filter(|(id, _)| !self.state_nodes.contains_key(*id))
            .fold((0u64, 0u64), |(bytes, count), (_, layer)| {
                (
                    bytes.saturating_add(layer.width as u64 * layer.height as u64 * 4),
                    count + 1,
                )
            })
    }

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
    /// distinct tile, plus what the tile graph still OWES for layers whose canon
    /// has not been built yet.
    ///
    /// `&self` only - no buffer write, epoch bump, cursor move, `version()` bump
    /// or `Arc<StateNode>` mutation - and no pixel bytes are returned, so two
    /// calls on an unchanged document are byte-identical. `None` for a document
    /// this registry does not hold, so an unknown id can never read as a
    /// zero-byte document.
    ///
    /// Consumers, and there is exactly one: the dev-only byte-accounting row in
    /// `perfAuditDev.ts`, which drives the `rust_pixels_store_bytes` command from
    /// the audit harness. Nothing in the shipping UI, and no production command,
    /// reads this - the probe is a diagnostic, so its cost never lands on a paint
    /// or undo path. The sibling `rust_pixels_history_depth` probe still has NO
    /// consumer at all: it answers a cursor-parity question nothing asks yet.
    pub fn get_store_bytes(&self, doc_id: &str) -> Option<PixelStoreBytes> {
        let doc = self.docs.get(doc_id)?;
        let row_major_bytes = doc.row_major_bytes();
        let tile_graph = doc.history_tile_bytes();
        let (owed_anchor_bytes, owed_anchor_layer_count) = doc.owed_anchor();
        Some(PixelStoreBytes {
            layer_count: doc.layers.len() as u64,
            row_major_bytes,
            total_bytes: row_major_bytes.saturating_add(tile_graph.total_bytes),
            owed_anchor_bytes,
            owed_anchor_layer_count,
            tile_graph,
        })
    }
}

#[cfg(test)]
mod tests;
