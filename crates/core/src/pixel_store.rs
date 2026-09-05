// SPDX-License-Identifier: AGPL-3.0-or-later
// Rust owns the canonical per-layer pixel buffer for raster layers, namespaced
// by (document_id, layer_id) inside `PixelStoreRegistry`. This replaces the flat
// process-global `PIXEL_STORE: HashMap<layerId, PixelLayer>` (no lifecycle) with
// a document-scoped owner driven by open/close/add/remove/resize lifecycle
// events. History stores before/after TILE-PATCH DELTAS, never a full
// pixel-buffer snapshot. TS PaintTileSurface is a derived cache, validated by an
// `epoch`: every canonical mutation bumps the layer epoch; TS tracks the epoch
// of its derived cache and rehydrates from Rust on mismatch.
//
// smallest production seam. No TileStore/Rayon/SAB/WebGPU. No other
// layer/selection/transform ownership is touched.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

// Each document owns its authoritative history stream (`ProtocolEngine`). Pixel
// deltas live in the stream's `Pixel` entries; the `PixelLayer` keeps only the
// canonical buffer + epoch.
use crate::document_core::ProtocolEngine;
// Bug 1 fix: commit composites the dab onto the EXISTING canonical pixels, so
// `raster_shadow` needs the base buffer + its dab types.
use crate::paint_parity::{raster_shadow, ParityDab, ParityTip};
// The per-layer canonical COW seam (arena + immutable tile states). It produces
// the before/after `Arc<StateNode>`s that the unified history stream stores, and
// (once the flag flips) can become the canonical byte source. The row-major
// `PixelLayer` remains the ACTIVE production default.
use crate::state_node::{LayerState, RegionChange, StateNode};

/// Migration flag. OFF (default): the row-major `PixelLayer` is the ACTIVE
/// production canonical and the TS-facing behavior is unchanged. The
/// `LayerState`/`StateNode` path is AVAILABLE (history entries always store
/// `Arc<StateNode>`) but NOT activated as the byte source.
/// ON: the per-layer `LayerState` becomes the canonical reader for the pixel
/// path (forward path still mirrors into `PixelLayer` so the existing
/// `get_layer` accessor stays stable).
#[allow(dead_code)]
const STATE_NODE_CANONICAL: bool = false;

/// Migration flag — the packed Arc-subarray canonical store.
/// OFF (default): row-major `PixelLayer` is the ACTIVE production byte source;
/// the packed-backed `TileStore` is the pack/upload store only (the state_node
/// `LayerState` packed COW seam is built lazily for history but not the byte
/// source). ON: the packed tile-major `TileStore` becomes the canonical byte
/// source. A single runtime const (analogous to `STATE_NODE_CANONICAL`); the
/// row-major `PixelLayer` remains the rollback path. Flipping to `true` requires
/// the TS mirror wiring + tests first.
#[allow(dead_code)]
const TILE_MAJOR: bool = false;

/// Straight-alpha -> premultiplied (matches `raster_shadow`'s internal math).
fn premul(buf: &[u8], i: usize) -> [u8; 4] {
    let a = buf[i + 3] as u64;
    if a == 0 {
        return [0, 0, 0, 0];
    }
    let half = a / 2;
    [
        ((buf[i] as u64 * a + half) / 255) as u8,
        ((buf[i + 1] as u64 * a + half) / 255) as u8,
        ((buf[i + 2] as u64 * a + half) / 255) as u8,
        buf[i + 3],
    ]
}

/// One absolute tile region of RGBA pixels (the changed region only).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TilePatch {
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}

/// Authoritative pixel owner for one layer. Exactly one `pixels` buffer exists
/// per layer. Pixel history lives in the document's `ProtocolEngine` (bounded to
/// `max_depth`); this struct keeps only the canonical buffer + `epoch`. `epoch`
/// is bumped on every canonical mutation so the derived TS cache can detect
/// staleness.
pub struct PixelLayer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Monotonic cache-validity counter. 0 at creation; +1 per canonical mutation.
    epoch: u64,
}

impl PixelLayer {
    /// `pixels` must be exactly `width*height*4` RGBA bytes (the seeding source).
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        assert_eq!(
            pixels.len(),
            width as usize * height as usize * 4,
            "pixel buffer size mismatch"
        );
        Self {
            width,
            height,
            pixels,
            epoch: 0,
        }
    }

    fn write_tile(&mut self, t: &TilePatch) {
        let x = t.x as usize;
        let y = t.y as usize;
        let w = t.w;
        let h = t.h;
        let stride = self.width as usize * 4;
        for row in 0..h {
            let dst = (y + row) * stride + x * 4;
            let src = row * w * 4;
            self.pixels[dst..dst + w * 4].copy_from_slice(&t.data[src..src + w * 4]);
        }
    }

    /// Read back a region for verification/transport (bounded by the caller).
    pub fn snapshot_region(&self, x: i64, y: i64, w: usize, h: usize) -> Vec<u8> {
        let stride = self.width as usize * 4;
        let mut out = vec![0u8; w * h * 4];
        for row in 0..h {
            let dst = row * w * 4;
            let src = ((y + row as i64) as usize) * stride + (x as usize) * 4;
            out[dst..dst + w * 4].copy_from_slice(&self.pixels[src..src + w * 4]);
        }
        out
    }

    /// Enumerate every tile of the layer (256-grid) as absolute TilePatches.
    /// Used by cache rehydration to refresh a derived TS cache from Rust.
    pub fn all_tiles(&self) -> Vec<TilePatch> {
        const TW: usize = 256;
        let cols = (self.width as usize + TW - 1) / TW;
        let rows = (self.height as usize + TW - 1) / TW;
        let mut out = Vec::new();
        for ty in 0..rows {
            for tx in 0..cols {
                let x = (tx * TW) as i64;
                let y = (ty * TW) as i64;
                let w = (TW).min(self.width as usize - tx * TW);
                let h = (TW).min(self.height as usize - ty * TW);
                if w == 0 || h == 0 {
                    continue;
                }
                let data = self.snapshot_region(x, y, w, h);
                out.push(TilePatch { x, y, w, h, data });
            }
        }
        out
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}

/// Per-document pixel store: owns one `PixelLayer` per raster layer id AND the
/// document's authoritative history stream. Created/destroyed by the enclosing
/// `PixelStoreRegistry` lifecycle.
#[derive(Default)]
pub struct DocumentPixelStore {
    pub layers: HashMap<String, PixelLayer>,
    /// Single authoritative history cursor for this document. Pixel deltas are
    /// stored as `EntryPayload::Pixel` entries here.
    pub history: ProtocolEngine,
    /// Per-layer immutable `StateNode` canonical COW seam (arena + current
    /// state). Kept in sync with `PixelLayer`; produces the `Arc<StateNode>`
    /// before/after pairs the history stream stores, and is re-anchored on
    /// undo/redo. Owns NO strong pixel bytes beyond the current + base arcs.
    pub state_nodes: HashMap<String, LayerState>,
}

impl DocumentPixelStore {
    pub fn new() -> Self {
        Self {
            layers: HashMap::new(),
            history: ProtocolEngine::new(),
            state_nodes: HashMap::new(),
        }
    }

    /// Seed (or replace) the canonical buffer for `layer_id`. Replacement is
    /// intentional: it is how a failed commit recovers (re-seed from TS). A
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

    pub fn get_layer(&self, layer_id: &str) -> Option<&PixelLayer> {
        self.layers.get(layer_id)
    }

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
    fn layer_state(&mut self, layer_id: &str) -> Option<&mut LayerState> {
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
    /// B4 (trust boundary): this is the seam a caller (TS via `apply_pixel_patch`
    /// / `write_region`) feeds patch geometry into. Malformed input — zero
    /// w/h, OOB, or len != w*h*4 — is validated HERE and returns `None` so the
    /// commit is gracefully skipped (no cursor move, no history entry) instead
    /// of panicking on `cow_batch`'s internal asserts.
    fn cow_tiles(
        &mut self,
        layer_id: &str,
        after: &[TilePatch],
        epoch: u64,
    ) -> Option<(Arc<StateNode>, Arc<StateNode>)> {
        let (lw, lh) = {
            let layer = self.layers.get(layer_id)?;
            (layer.width, layer.height)
        };
        // B4: validate before COW. Any invalid patch -> None (graceful skip).
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
}

/// Canonical pixel owner, namespaced by document id. This is the long-term owner
/// (replacing the flat process-global `PIXEL_STORE`). Lifecycle is driven from
/// TS via the `rust_pixels_*` commands: open/close/add/remove/resize manage
/// entries; a missing document namespace is created lazily on first layer init.
#[derive(Default)]
pub struct PixelStoreRegistry {
    pub docs: HashMap<String, DocumentPixelStore>,
}

impl PixelStoreRegistry {
    pub fn new() -> Self {
        Self {
            docs: HashMap::new(),
        }
    }

    /// Document created/opened: ensure a (possibly empty) namespace exists so a
    /// later close can release it deterministically.
    pub fn open_document(&mut self, doc_id: &str) {
        self.docs
            .entry(doc_id.to_string())
            .or_insert_with(DocumentPixelStore::new);
    }

    /// Document closed: release ALL of its pixel storage.
    pub fn close_document(&mut self, doc_id: &str) {
        self.docs.remove(doc_id);
    }

    /// Seed a layer's canonical buffer; auto-creates the document namespace.
    pub fn add_layer(
        &mut self,
        doc_id: &str,
        layer_id: &str,
        w: u32,
        h: u32,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        let doc = self
            .docs
            .entry(doc_id.to_string())
            .or_insert_with(DocumentPixelStore::new);
        doc.init_layer(layer_id, w, h, bytes);
        Ok(())
    }

    pub fn remove_layer(&mut self, doc_id: &str, layer_id: &str) {
        if let Some(doc) = self.docs.get_mut(doc_id) {
            doc.remove_layer(layer_id);
        }
    }

    pub fn resize_layer(
        &mut self,
        doc_id: &str,
        layer_id: &str,
        w: u32,
        h: u32,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        let doc = self
            .docs
            .entry(doc_id.to_string())
            .or_insert_with(DocumentPixelStore::new);
        doc.resize_layer(layer_id, w, h, bytes);
        Ok(())
    }

    pub fn get_layer(&self, doc_id: &str, layer_id: &str) -> Option<&PixelLayer> {
        self.docs.get(doc_id).and_then(|d| d.layers.get(layer_id))
    }

    pub fn get_layer_mut(&mut self, doc_id: &str, layer_id: &str) -> Option<&mut PixelLayer> {
        self.docs.get_mut(doc_id).and_then(|d| {
            // Desync guard: see `DocumentPixelStore::get_layer_mut`.
            d.state_nodes.remove(layer_id);
            d.layers.get_mut(layer_id)
        })
    }

    pub fn get_epoch(&self, doc_id: &str, layer_id: &str) -> Result<u64, String> {
        self.get_layer(doc_id, layer_id)
            .map(|l| l.epoch())
            .ok_or_else(|| format!("layer not initialized: {layer_id}"))
    }

    // ── Unified pixel history (authoritative) ──
    // These route through the document's `ProtocolEngine` so the pixel history
    // cursor is the SAME cursor used by metadata history. They write the
    // canonical `PixelLayer.pixels` + bump `epoch` directly. The legacy
    // per-layer `undo_stack`/`redo_stack` history mechanism was removed; the only
    // pixel history is now the document's `ProtocolEngine` (bounded to
    // `max_depth`). Locking is external (the process-lifetime `REGISTRY` Mutex);
    // here we operate directly on the `docs` HashMap.

    /// Commit a brush/baked-pixel op: open a native `Pixel` history entry,
    /// write `after` into the canonical buffer, bump epoch, increment
    /// `DocumentVersion` exactly once. Returns `(after_tiles, epoch, version)`.
    pub fn apply_pixel_patch(
        &mut self,
        doc_id: &str,
        layer_id: &str,
        _before: Vec<TilePatch>,
        after: Vec<TilePatch>,
    ) -> Option<(Vec<TilePatch>, u64, u64)> {
        let doc = self.docs.get_mut(doc_id)?;
        // Validate the layer exists BEFORE opening a history entry, so a failed
        // commit (missing layer) does NOT leave a phantom cursor movement.
        let epoch = doc.layers.get(layer_id)?.epoch;
        // Build the before/after `Arc<StateNode>` pair by COW-ing the change
        // regions into the maintained per-layer canon. The `before`/`after`
        // tile-patch args are the caller's pre/post image; they stay unchanged
        // (row-major + TS surface byte-identical).
        let (before_arc, after_arc) = doc.cow_tiles(layer_id, &after, epoch)?;
        let version = doc
            .history
            .apply_pixel_patch(layer_id, before_arc, after_arc);
        let layer = doc.layers.get_mut(layer_id).unwrap();
        for t in &after {
            layer.write_tile(t);
        }
        layer.epoch += 1;
        Some((after, layer.epoch, version))
    }

    /// Fill: write an explicit RGBA region into the canonical buffer as ONE
    /// `Pixel` history entry (mirrors the brush's single-step contract).
    /// Returns `(before_tiles, after_tiles, epoch, version)`. Bumps epoch+version
    /// exactly once; the unified history cursor advances one step. The Rust entry
    /// is subordinate to the TS `history.commit` that drives undo/redo — there is
    /// NO separate TS-visible step, so an undo is exactly ONE user action.
    pub fn write_region(
        &mut self,
        doc_id: &str,
        layer_id: &str,
        x: i64,
        y: i64,
        w: usize,
        h: usize,
        rgba: Vec<u8>,
    ) -> Option<(Vec<TilePatch>, Vec<TilePatch>, u64, u64)> {
        // Validate layer + region bounds BEFORE opening a history entry, so a
        // failed write (bad layer / out-of-bounds / size mismatch) leaves no
        // phantom cursor movement.
        let (lw, lh) = {
            let layer = self.docs.get(doc_id)?.layers.get(layer_id)?;
            (layer.width as i64, layer.height as i64)
        };
        if x < 0 || y < 0 || w == 0 || h == 0 {
            return None;
        }
        if x + w as i64 > lw || y + h as i64 > lh {
            return None;
        }
        if rgba.len() != w * h * 4 {
            return None;
        }

        const TW: usize = 256;
        let cols = ((lw as usize) + TW - 1) / TW;
        let rows = ((lh as usize) + TW - 1) / TW;
        let tx0 = (x as usize) / TW;
        let ty0 = (y as usize) / TW;
        let tx1 = ((x as usize + w - 1) / TW).min(cols - 1);
        let ty1 = ((y as usize + h - 1) / TW).min(rows - 1);

        let mut before = Vec::new();
        let mut after = Vec::new();
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let tile_x = (tx * TW) as i64;
                let tile_y = (ty * TW) as i64;
                let tile_w = TW.min(lw as usize - tx * TW);
                let tile_h = TW.min(lh as usize - ty * TW);
                // Full-tile BEFORE = current canonical region (exact pre-image).
                let b_data = {
                    let layer = self.docs.get(doc_id)?.layers.get(layer_id)?;
                    layer.snapshot_region(tile_x, tile_y, tile_w, tile_h)
                };
                // Full-tile AFTER = canonical with the region-overlapping span
                // replaced by the incoming rgba (so a tile partially outside the
                // region keeps its existing pixels).
                let mut a_data = b_data.clone();
                let rx0 = x.max(tile_x);
                let ry0 = y.max(tile_y);
                let rx1 = (x + w as i64).min(tile_x + tile_w as i64);
                let ry1 = (y + h as i64).min(tile_y + tile_h as i64);
                if rx0 < rx1 && ry0 < ry1 {
                    for row in ry0..ry1 {
                        let t_row = (row - tile_y) as usize;
                        let r_row = (row - y) as usize;
                        let d_off = t_row * tile_w * 4 + ((rx0 - tile_x) as usize) * 4;
                        let s_off = r_row * w * 4 + ((rx0 - x) as usize) * 4;
                        let len = ((rx1 - rx0) as usize) * 4;
                        a_data[d_off..d_off + len].copy_from_slice(&rgba[s_off..s_off + len]);
                    }
                }
                before.push(TilePatch {
                    x: tile_x,
                    y: tile_y,
                    w: tile_w,
                    h: tile_h,
                    data: b_data,
                });
                after.push(TilePatch {
                    x: tile_x,
                    y: tile_y,
                    w: tile_w,
                    h: tile_h,
                    data: a_data,
                });
            }
        }
        let res = self.apply_pixel_patch(doc_id, layer_id, before.clone(), after.clone())?;
        let (_after, epoch, version) = res;
        Some((before, after, epoch, version))
    }

    /// Bug 1 fix — canonical-owner commit.
    /// Compute the `before` (exact pre-stroke canonical tiles) and `after`
    /// (composite of canonical pixels + dab batch) for the dirty region WITHOUT
    /// mutating the canonical buffer. The caller applies `after` via
    /// `apply_pixel_patch`. `before` is the history pre-image; `after` is the
    /// final canonical tile state after compositing (not dab-on-white).
    pub fn compute_commit(
        &self,
        doc_id: &str,
        layer_id: &str,
        w: usize,
        h: usize,
        eraser: bool,
        brush: f64,
        dabs: &[ParityDab],
        tip: &ParityTip,
        include_tiles: bool,
    ) -> Option<(Vec<TilePatch>, Vec<TilePatch>)> {
        let layer = self.docs.get(doc_id)?.layers.get(layer_id)?;
        // Canonical store is straight-alpha; `raster_shadow` composites in
        // premultiplied space, so premultiply the base before handing it over.
        let mut base_premult = Vec::with_capacity(layer.pixels.len());
        let mut i = 0;
        while i < layer.pixels.len() {
            let p = premul(&layer.pixels, i);
            base_premult.push(p[0]);
            base_premult.push(p[1]);
            base_premult.push(p[2]);
            base_premult.push(p[3]);
            i += 4;
        }
        let (_, after_out) = raster_shadow(
            w,
            h,
            true,
            Some(base_premult),
            eraser,
            brush,
            dabs,
            tip,
            include_tiles,
        );
        let after: Vec<TilePatch> = after_out
            .iter()
            .map(|t| TilePatch {
                x: t.x,
                y: t.y,
                w: t.w,
                h: t.h,
                data: t.data.clone(),
            })
            .collect();
        // `before` = same geometry, straight-alpha, extracted from the canonical buffer.
        let before: Vec<TilePatch> = after_out
            .iter()
            .map(|t| {
                let mut data = vec![0u8; t.w * t.h * 4];
                for row in 0..t.h as i64 {
                    let src = (((t.y + row) * w as i64 + t.x) * 4) as usize;
                    let dst = (row * t.w as i64 * 4) as usize;
                    data[dst..dst + t.w * 4].copy_from_slice(&layer.pixels[src..src + t.w * 4]);
                }
                TilePatch {
                    x: t.x,
                    y: t.y,
                    w: t.w,
                    h: t.h,
                    data,
                }
            })
            .collect();
        Some((before, after))
    }

    /// Full canonical-owner commit: ensure the layer exists (init only when Rust
    /// has no store entry — lifecycle-safe, never goes stale), compute the
    /// composite, then apply it through the unified history. Returns
    /// `(before, after, epoch, version)`.
    pub fn commit_pixels(
        &mut self,
        doc_id: &str,
        layer_id: &str,
        bytes: Vec<u8>,
        w: usize,
        h: usize,
        eraser: bool,
        brush: f64,
        dabs: &[ParityDab],
        tip: &ParityTip,
        include_tiles: bool,
    ) -> Option<(Vec<TilePatch>, Vec<TilePatch>, u64, u64)> {
        // Bug 2 fix: ensure-if-absent (idempotent). No TS-side seeded flag needed;
        // Rust is the single source of truth for whether the layer exists.
        if self.get_layer(doc_id, layer_id).is_none() {
            self.add_layer(doc_id, layer_id, w as u32, h as u32, bytes)
                .ok()?;
        }
        let (before, after) = self.compute_commit(
            doc_id,
            layer_id,
            w,
            h,
            eraser,
            brush,
            dabs,
            tip,
            include_tiles,
        )?;
        let res = self.apply_pixel_patch(doc_id, layer_id, before.clone(), after.clone())?;
        // res = (after_tiles, epoch, version)
        let (_after_tiles, epoch, version) = res;
        Some((before, after, epoch, version))
    }

    /// Undo via the unified stream. Returns `(layer_id, tiles, epoch, version)`.
    /// `None` when there is nothing to undo (or a pending-external barrier).
    /// The undone state's `Arc<StateNode>` is also re-anchored onto the
    /// per-layer canon so a subsequent commit COWs from the post-undo state.
    pub fn undo_pixel(&mut self, doc_id: &str) -> Option<(String, Vec<TilePatch>, u64, u64)> {
        let doc = self.docs.get_mut(doc_id)?;
        let res = doc.history.undo_pixel().ok()?;
        match res {
            (Some(layer_id), Some(tiles), Some(node)) => {
                let layer = doc.layers.get_mut(&layer_id)?;
                for t in &tiles {
                    layer.write_tile(t);
                }
                layer.epoch += 1;
                if let Some(ls) = doc.state_nodes.get_mut(&layer_id) {
                    ls.set_current(node);
                }
                Some((layer_id, tiles, layer.epoch, doc.history.version()))
            }
            _ => None,
        }
    }

    /// Redo via the unified stream. Symmetric to `undo_pixel`.
    pub fn redo_pixel(&mut self, doc_id: &str) -> Option<(String, Vec<TilePatch>, u64, u64)> {
        let doc = self.docs.get_mut(doc_id)?;
        let res = doc.history.redo_pixel().ok()?;
        match res {
            (Some(layer_id), Some(tiles), Some(node)) => {
                let layer = doc.layers.get_mut(&layer_id)?;
                for t in &tiles {
                    layer.write_tile(t);
                }
                layer.epoch += 1;
                if let Some(ls) = doc.state_nodes.get_mut(&layer_id) {
                    ls.set_current(node);
                }
                Some((layer_id, tiles, layer.epoch, doc.history.version()))
            }
            _ => None,
        }
    }

    /// Current authoritative history cursor for the document.
    pub fn get_history_cursor(&self, doc_id: &str) -> Option<usize> {
        self.docs.get(doc_id).map(|d| d.history.cursor())
    }

    /// Current `DocumentVersion` for the document.
    pub fn get_history_version(&self, doc_id: &str) -> Option<u64> {
        self.docs.get(doc_id).map(|d| d.history.version())
    }

    /// Route a TS (non-pixel) logical mutation into the SAME unified
    /// `ProtocolEngine` cursor so mixed TS/Rust operations share one history
    /// position. Creates an `External` entry (no pixel delta) that advances the
    /// cursor + bumps `DocumentVersion` exactly once. The actual metadata revert
    /// on undo/redo remains TS-side; this entry only keeps the ordering unified.
    pub fn record_external(
        &mut self,
        doc_id: &str,
        label: &str,
        affected: &[String],
        adapter_id: &str,
        token: &str,
        memory_cost_bytes: u64,
    ) -> Result<(), String> {
        let doc = self
            .docs
            .get_mut(doc_id)
            .ok_or_else(|| format!("document not open: {doc_id}"))?;
        // TS ops are external to the Rust engine; register the adapter once so
        // record_external's adapter check passes.
        doc.history.register_adapter(adapter_id);
        doc.history
            .record_external(label, affected, adapter_id, token, memory_cost_bytes)
            .map_err(|e| e.message)
    }

    /// Record an atomic metadata+pixel `Snapshot` entry into the document's
    /// unified `ProtocolEngine` cursor. Mirrors `record_external` (bumps
    /// `DocumentVersion` exactly once, truncates the redo branch) but stores BOTH
    /// the `before` and `after` snapshot so undo restores the `before` and redo
    /// re-applies the `after` — restoring the metadata AND (via the per-layer
    /// opaque `bitmap_token`) the pixel reference from ONE entry.
    ///
    /// Boundary rules: an unopen/unknown document -> `Err`; a `before`/`after`
    /// `doc_id` that mismatches `doc_id` -> `Err` (never mutate the wrong doc);
    /// layers that have no canonical buffer in the store are left untouched
    /// (no-op) — the snapshot's pixel restore happens TS-side via the token, so
    /// Rust does not need a PixelLayer for every referenced layer.
    pub fn record_snapshot(
        &mut self,
        doc_id: &str,
        before: crate::snapshot::DocumentSnapshot,
        after: crate::snapshot::DocumentSnapshot,
    ) -> Result<(), String> {
        let doc = self
            .docs
            .get_mut(doc_id)
            .ok_or_else(|| format!("document not open: {doc_id}"))?;
        if before.doc_id != doc_id {
            return Err(format!(
                "snapshot before.doc_id mismatch: {} != {doc_id}",
                before.doc_id
            ));
        }
        if after.doc_id != doc_id {
            return Err(format!(
                "snapshot after.doc_id mismatch: {} != {doc_id}",
                after.doc_id
            ));
        }
        doc.history
            .record_snapshot(before, after)
            .map_err(|e| e.message)
    }

    /// Undo the entry just below the cursor IF it is a `Snapshot` entry, returning
    /// the metadata snapshot (with per-layer bitmap tokens). Non-snapshot entries
    /// return `None` WITHOUT moving the cursor (so the caller dispatches the
    /// actual undo through `undo_pixel`/metadata paths).
    pub fn undo_snapshot(&mut self, doc_id: &str) -> Option<crate::snapshot::DocumentSnapshot> {
        self.docs
            .get_mut(doc_id)?
            .history
            .undo_snapshot()
            .ok()
            .flatten()
    }

    /// Redo the entry at the cursor IF it is a `Snapshot` entry. Symmetric to
    /// `undo_snapshot`.
    pub fn redo_snapshot(&mut self, doc_id: &str) -> Option<crate::snapshot::DocumentSnapshot> {
        self.docs
            .get_mut(doc_id)?
            .history
            .redo_snapshot()
            .ok()
            .flatten()
    }
}

// Process-lifetime registry. The `static PIXEL_STORE: Option<HashMap<..>>`
// global is replaced here by a document-namespaced owner; the outer `Option`
// keeps the lazy-init pattern already used by the Tauri command layer.
static REGISTRY: Mutex<Option<PixelStoreRegistry>> = Mutex::new(None);

pub fn registry() -> MutexGuard<'static, Option<PixelStoreRegistry>> {
    REGISTRY.lock().unwrap()
}

#[cfg(test)]
mod tests;
