// SPDX-License-Identifier: AGPL-3.0-or-later
//! Per-document pixel store: the canonical per-layer buffers, the per-layer
//! `StateNode` canon, and the document's authoritative history stream.

use std::collections::HashMap;
use std::sync::Arc;

use crate::document::DocumentCore;
use crate::pixel::layer::PixelLayer;
use crate::pixel::state_node::{LayerState, RegionChange, StateNode};
use crate::pixel::tile::TilePatch;

use crate::document::history::PixelDelta;

/// Re-export of the document snapshot DTO so `pixel::store` reaches it through
/// this module only: the pixel -> document edge stays one-way.
pub(crate) use crate::document::DocumentSnapshot;

/// Per-document pixel store: owns one `PixelLayer` per raster layer id AND the
/// document's authoritative history stream. Created/destroyed by the enclosing
/// `PixelStoreRegistry` lifecycle.
#[derive(Default)]
pub struct DocumentPixelStore {
    pub layers: HashMap<String, PixelLayer>,
    /// Single authoritative history cursor for this document. Pixel deltas are
    /// stored as `EntryPayload::Pixel` entries here.
    pub history: DocumentCore,
    /// Per-layer immutable `StateNode` canonical COW seam (arena + current
    /// state). Kept in sync with `PixelLayer`; produces the `Arc<StateNode>`
    /// before/after pairs the history stream stores, and is re-anchored on
    /// undo/redo. Owns NO strong pixel bytes beyond the current + base arcs.
    pub state_nodes: HashMap<String, LayerState>,
}

impl DocumentPixelStore {
    #[cfg(any(feature = "legacy-host", test))]
    #[cfg_attr(not(feature = "legacy-host"), allow(dead_code))]
    pub fn new() -> Self {
        Self {
            layers: HashMap::new(),
            history: DocumentCore::new(),
            state_nodes: HashMap::new(),
        }
    }

    /// Seed (or replace) the canonical buffer for `layer_id`. Replacement is
    /// intentional: it is how a failed commit recovers (re-seed from the host). A
    /// wholesale re-seed invalidates the layer's StateNode canon AND drops its
    /// pixel history entries (the cached before/after `Arc<StateNode>`s are
    /// stale against the new bytes; replaying them would corrupt the layer).
    pub fn init_layer(&mut self, layer_id: &str, w: u32, h: u32, bytes: Vec<u8>) {
        self.layers
            .insert(layer_id.to_string(), PixelLayer::new(w, h, bytes));
        self.state_nodes.remove(layer_id);
        self.history.invalidate_layer(layer_id);
    }

    /// Drop the layer's pixel storage (layer removed).
    pub fn remove_layer(&mut self, layer_id: &str) {
        self.layers.remove(layer_id);
        self.state_nodes.remove(layer_id);
    }

    /// Resize: recreate the buffer at new dimensions (old index math is gone).
    /// The new dims change the tile grid, so the layer's StateNode canon AND its
    /// pixel history entries are both invalidated: undo/redo MUST NOT replay a
    /// stale-dim `Arc<StateNode>` onto the smaller/larger buffer (a real crash
    /// repro — see the resize corrective batch).
    pub fn resize_layer(&mut self, layer_id: &str, w: u32, h: u32, bytes: Vec<u8>) {
        self.layers
            .insert(layer_id.to_string(), PixelLayer::new(w, h, bytes));
        self.state_nodes.remove(layer_id);
        self.history.invalidate_layer(layer_id);
    }

    #[cfg(any(feature = "legacy-host", test))]
    #[cfg_attr(not(feature = "legacy-host"), allow(dead_code))]
    pub fn get_layer(&self, layer_id: &str) -> Option<&PixelLayer> {
        self.layers.get(layer_id)
    }

    #[cfg(any(feature = "legacy-host", test))]
    #[cfg_attr(not(feature = "legacy-host"), allow(dead_code))]
    pub fn get_layer_mut(&mut self, layer_id: &str) -> Option<&mut PixelLayer> {
        // Desync guard: any DIRECT write via a `&mut PixelLayer` is unknown
        // to the per-layer StateNode canon, so force the canon to re-seed from
        // the mutated bytes next time it is accessed (removes the stale state).
        self.state_nodes.remove(layer_id);
        self.layers.get_mut(layer_id)
    }

    /// Get-or-create the per-layer `LayerState` canon, seeded lazily from
    /// the row-major `PixelLayer` (the ONLY full ingest — once per layer, never
    /// per op; aligned with the perf warning). Returns `None` if the layer
    /// has no pixel storage.
    pub(crate) fn layer_state(&mut self, layer_id: &str) -> Option<&mut LayerState> {
        // Initial dims snapshot for the cross-check below (asserted on the
        // already-present state so a stale-dim canon is caught before replay).
        let (lw, lh) = {
            let layer = self.layers.get(layer_id)?;
            (layer.width, layer.height)
        };
        if !self.state_nodes.contains_key(layer_id) {
            let layer = self.layers.get(layer_id).expect("checked above");
            let ls = LayerState::new(
                layer_id,
                layer.width,
                layer.height,
                &layer.pixels,
                layer.epoch,
            );
            self.state_nodes.insert(layer_id.to_string(), ls);
        }
        let ls = self.state_nodes.get_mut(layer_id).expect("present");
        // Desync cross-check (I1/I9): the per-layer canon must track the live
        // PixelLayer dims; a stale-dim canon would panic on replay.
        let cur = ls.current_state();
        debug_assert_eq!(cur.meta.width, lw, "StateNode canon width != PixelLayer");
        debug_assert_eq!(cur.meta.height, lh, "StateNode canon height != PixelLayer");
        Some(ls)
    }

    /// Build the `(before, after)` `Arc<StateNode>` pair for a commit by
    /// COW-ing the change regions into the current per-layer state. `after` may
    /// contain full-tile OR sub-tile (dirty-rect) patches — the touched tiles
    /// are re-tiled copy-on-touch (no full-layer ingest).
    ///
    /// Trust boundary: this is the seam a caller (the host via
    /// `apply_pixel_patch` / `write_region`) feeds patch geometry into. Malformed
    /// input — zero w/h, OOB, or len != w*h*4 — is validated HERE and returns
    /// `None` so the commit is gracefully skipped (no cursor move, no history
    /// entry) instead of panicking on `cow_batch`'s internal asserts.
    pub(crate) fn cow_tiles(
        &mut self,
        layer_id: &str,
        after: &[TilePatch],
        epoch: u64,
    ) -> Option<(Arc<StateNode>, Arc<StateNode>)> {
        let (lw, lh) = {
            let layer = self.layers.get(layer_id)?;
            (layer.width, layer.height)
        };
        // Validate before COW. Any invalid patch -> None (graceful skip).
        for t in after {
            if t.w == 0 || t.h == 0 {
                return None;
            }
            if (t.w as u64).wrapping_mul(t.h as u64).wrapping_mul(4) != t.data.len() as u64 {
                return None;
            }
            if t.x < 0 || t.y < 0 {
                return None;
            }
            if (t.x as u64) + (t.w as u64) > lw as u64 || (t.y as u64) + (t.h as u64) > lh as u64 {
                return None;
            }
        }
        let ls = self.layer_state(layer_id)?;
        let regions: Vec<RegionChange> = after
            .iter()
            .map(|t| {
                RegionChange::new(
                    t.x as u32,
                    t.y as u32,
                    t.w as u32,
                    t.h as u32,
                    t.data.clone(),
                )
            })
            .collect();
        Some(ls.cow_batch(&regions, epoch))
    }

    /// Execute one protocol command for this document and COMPLETE any pixel
    /// step the engine started.
    ///
    /// `DocumentCore::apply` owns the history cursor but not the pixel
    /// buffer, so when the cursor tip is a `Pixel` entry the walker moves the
    /// cursor and parks `(layer, tiles, node)` in `pending_pixel`. This method
    /// is the other half: it replays the tiles into the canonical `PixelLayer`,
    /// re-anchors the layer's `StateNode` canon so the next commit COWs from the
    /// restored state, bumps the epoch, and reports the tiles to the host on
    /// `CommandResult::pixel_patches`.
    ///
    /// The host is the PROJECTION for that result: it must upload these tiles
    /// and must not run a pixel step of its own, or one press undoes twice.
    /// Apply one typed command and project any pending pixel step onto the
    /// canonical buffers. The host uses this for native input and undo/redo; it
    /// does not expose raw registry internals.
    pub fn apply_command(
        &mut self,
        envelope: crate::wire::command::CommandEnvelope,
    ) -> Result<crate::wire::command::CommandResult, crate::protocol::ProtocolError> {
        let mut result = self.history.apply(envelope)?;
        if let Some(plan) = self.history.pending_crop_bake.take() {
            self.apply_crop_bake(plan);
        }
        if let Some(plan) = self.history.pending_rasterize.take() {
            self.apply_rasterize_pixels(plan);
        }
        let pending = std::mem::take(&mut self.history.pending_pixels);
        if pending.is_empty() {
            return Ok(result);
        }
        let mut handoffs = Vec::with_capacity(pending.len());
        for (layer_id, tiles, node) in pending {
            let epoch = self.write_restored_tiles(&layer_id, &tiles, &node);
            if let Some(ls) = self.state_nodes.get_mut(&layer_id) {
                ls.set_current(node);
            }
            handoffs.push(crate::wire::command::PixelPatchHandoff {
                layer_id,
                tiles,
                epoch,
                version: self.history.version(),
            });
        }
        result.pixel_patches = Some(handoffs);
        Ok(result)
    }

    /// Write replayed tiles into the canonical buffer and bump its epoch.
    /// A layer with no buffer here has nothing to restore into, so it reports
    /// epoch 0 (the cursor move stays authoritative - see `apply_command`).
    /// Execute the pixel half of a parametric rasterize: materialize the
    /// host-rendered bitmap as the rasterized layer's canonical buffer. The
    /// buffer rides no pixel-history entry — the rasterize's own metadata
    /// entry is the undo unit, and undoing it returns the layer to its
    /// parametric form (the materialized buffer then sits unused until a
    /// re-rasterize replaces it).
    fn apply_rasterize_pixels(&mut self, plan: crate::document::rasterize_bake::RasterizePlan) {
        self.init_layer(&plan.layer_id, plan.width, plan.height, plan.rgba);
    }

    /// Execute the pixel half of a destructive crop: discard every pixel that
    /// fell outside the new document, then fold the resulting before/after
    /// states into the crop's metadata entry. One command, one history step.
    fn apply_crop_bake(&mut self, plan: crate::document::crop_bake::CropBakePlan) {
        let mut deltas = Vec::new();
        if let Some(fill) = &plan.fill {
            // Bake the colour UNDER the Background and replace its bitmap with
            // the document-sized result of "fill the canvas, then draw the
            // cropped Background over it", baked into the Background layer.
            let doc_w = plan.doc_width.max(0.0) as u32;
            let doc_h = plan.doc_height.max(0.0) as u32;
            let before = self
                .layer_state(&fill.layer_id)
                .map(|ls| ls.current_state().clone());
            let source = self.layers.get(&fill.layer_id).map(|px| px.pixels.clone());
            if let (Some(before), Some(source)) = (before, source) {
                let bytes = crate::document::crop_bake::composite_layer_over_fill(
                    &source,
                    fill.width,
                    fill.height,
                    &fill.placement,
                    doc_w,
                    doc_h,
                    fill.color,
                );
                self.state_nodes.remove(&fill.layer_id);
                if let Some(layer) = self.layers.get_mut(&fill.layer_id) {
                    layer.width = doc_w;
                    layer.height = doc_h;
                    layer.pixels = bytes;
                    layer.epoch += 1;
                }
                if let Some(ls) = self.layer_state(&fill.layer_id) {
                    let after = ls.current_state().clone();
                    deltas.push(PixelDelta {
                        layer_id: fill.layer_id.clone(),
                        before,
                        after,
                    });
                }
            }
        }
        for layer in &plan.layers {
            let Some(px) = self.layers.get(&layer.layer_id) else {
                continue;
            };
            let (w, h, epoch) = (px.width, px.height, px.epoch);
            if w == 0 || h == 0 {
                continue;
            }
            let mut bytes = px.pixels.clone();
            let discarded = crate::document::crop_bake::discard_outside_document(
                &mut bytes,
                w,
                h,
                &layer.placement,
                plan.doc_width,
                plan.doc_height,
            );
            if discarded == 0 {
                continue;
            }
            let patch = TilePatch {
                x: 0,
                y: 0,
                w: w as usize,
                h: h as usize,
                data: bytes,
            };
            let Some((before, after)) =
                self.cow_tiles(&layer.layer_id, std::slice::from_ref(&patch), epoch)
            else {
                continue;
            };
            if let Some(px) = self.layers.get_mut(&layer.layer_id) {
                px.write_tile(&patch);
                px.epoch += 1;
            }
            deltas.push(PixelDelta {
                layer_id: layer.layer_id.clone(),
                before,
                after,
            });
        }
        if !deltas.is_empty() {
            self.history.attach_pixel_deltas(plan.entry_index, deltas);
        }
    }

    fn write_restored_tiles(
        &mut self,
        layer_id: &str,
        tiles: &[TilePatch],
        node: &Arc<StateNode>,
    ) -> u64 {
        let (node_w, node_h) = (node.meta.width, node.meta.height);
        let needs_resize = self
            .layers
            .get(layer_id)
            .is_some_and(|layer| layer.width != node_w || layer.height != node_h);
        if needs_resize {
            // A destructive step may change a layer's bitmap size (a baked
            // Background). Tiles are meaningless across sizes, so replay the
            // whole state and re-seed the canon from those bytes.
            let bytes = node.to_row_major();
            self.state_nodes.remove(layer_id);
            let epoch = match self.layers.get_mut(layer_id) {
                Some(layer) => {
                    *layer = PixelLayer::new(node_w, node_h, bytes);
                    layer.epoch += 1;
                    layer.epoch
                }
                None => 0,
            };
            if self.layer_state(layer_id).is_some() {
                if let Some(ls) = self.state_nodes.get_mut(layer_id) {
                    ls.set_current(node.clone());
                }
            }
            return epoch;
        }
        match self.layers.get_mut(layer_id) {
            Some(layer) => {
                for t in tiles {
                    layer.write_tile(t);
                }
                layer.epoch += 1;
                layer.epoch
            }
            None => 0,
        }
    }
}
