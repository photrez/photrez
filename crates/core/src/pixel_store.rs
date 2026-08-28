// SPDX-License-Identifier: AGPL-3.0-or-later
// C5.1 + C5.2: Rust owns the canonical per-layer pixel buffer for raster layers,
// namespaced by (document_id, layer_id) inside `PixelStoreRegistry`. This replaces
// the flat C4-pilot `PIXEL_STORE: HashMap<layerId, PixelLayer>` (process-global,
// no lifecycle) with a document-scoped owner driven by open/close/add/remove/resize
// lifecycle events. History stores before/after TILE-PATCH DELTAS (Model B), never a
// full pixel-buffer snapshot. TS PaintTileSurface is a derived cache, validated by an
// `epoch` (C5.2): every canonical mutation bumps the layer epoch; TS tracks the epoch
// of its derived cache and rehydrates from Rust on mismatch.
//
// smallest production seam. No TileStore/Rayon/SAB/WebGPU. No other
// layer/selection/transform ownership is touched.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

// C5.3-A: each document owns its authoritative history stream (ADR-0008
// ProtocolEngine). Pixel deltas live in the stream's `Pixel` entries; the
// `PixelLayer` keeps only the canonical buffer + epoch.
use crate::protocol::ProtocolEngine;
// Bug 1 fix: commit composites the dab onto the EXISTING canonical pixels, so
// `raster_shadow` needs the base buffer + its dab types.
use crate::paint_parity::{raster_shadow, ParityDab, ParityTip};

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
/// staleness (C5.2).
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
    /// Used by C5.2 rehydration to refresh a derived TS cache from Rust.
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
/// document's authoritative history stream (C5.3-A). Created/destroyed by the
/// enclosing `PixelStoreRegistry` lifecycle.
#[derive(Default)]
pub struct DocumentPixelStore {
    pub layers: HashMap<String, PixelLayer>,
    /// C5.3-A: single authoritative history cursor for this document. Pixel
    /// deltas are stored as `EntryPayload::Pixel` entries here.
    pub history: ProtocolEngine,
}

impl DocumentPixelStore {
    pub fn new() -> Self {
        Self {
            layers: HashMap::new(),
            history: ProtocolEngine::new(),
        }
    }

    /// Seed (or replace) the canonical buffer for `layer_id`. Replacement is
    /// intentional: it is how a failed commit recovers (re-seed from TS).
    pub fn init_layer(&mut self, layer_id: &str, w: u32, h: u32, bytes: Vec<u8>) {
        self.layers
            .insert(layer_id.to_string(), PixelLayer::new(w, h, bytes));
    }

    /// Drop the layer's pixel storage (layer removed).
    pub fn remove_layer(&mut self, layer_id: &str) {
        self.layers.remove(layer_id);
    }

    /// Resize: recreate the buffer at new dimensions (old index math is gone).
    pub fn resize_layer(&mut self, layer_id: &str, w: u32, h: u32, bytes: Vec<u8>) {
        self.layers
            .insert(layer_id.to_string(), PixelLayer::new(w, h, bytes));
    }

    pub fn get_layer(&self, layer_id: &str) -> Option<&PixelLayer> {
        self.layers.get(layer_id)
    }

    pub fn get_layer_mut(&mut self, layer_id: &str) -> Option<&mut PixelLayer> {
        self.layers.get_mut(layer_id)
    }
}

/// C5.1 canonical pixel owner, namespaced by document id. This is the long-term
/// owner (replacing the C4-pilot flat `PIXEL_STORE`). Lifecycle is driven from TS
/// via the `rust_pixels_*` commands: open/close/add/remove/resize manage entries;
/// a missing document namespace is created lazily on first layer init.
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
        self.docs
            .get_mut(doc_id)
            .and_then(|d| d.layers.get_mut(layer_id))
    }

    pub fn get_epoch(&self, doc_id: &str, layer_id: &str) -> Result<u64, String> {
        self.get_layer(doc_id, layer_id)
            .map(|l| l.epoch())
            .ok_or_else(|| format!("layer not initialized: {layer_id}"))
    }

    // ── C5.3-A: unified pixel history (authoritative) ──
    // These route through the document's `ProtocolEngine` so the pixel history
    // cursor is the SAME cursor used by metadata history. They write the
    // canonical `PixelLayer.pixels` + bump `epoch` directly. The legacy
    // per-layer `undo_stack`/`redo_stack` history mechanism was REMOVED in
    // C5.3-B; the only pixel history is now the document's `ProtocolEngine`
    // (bounded to `max_depth`). Locking is external (the process-lifetime
    // `REGISTRY` Mutex); here we operate directly on the `docs` HashMap.

    /// Commit a brush/baked-pixel op: open a native `Pixel` history entry,
    /// write `after` into the canonical buffer, bump epoch, increment
    /// `DocumentVersion` exactly once. Returns `(after_tiles, epoch, version)`.
    pub fn apply_pixel_patch(
        &mut self,
        doc_id: &str,
        layer_id: &str,
        before: Vec<TilePatch>,
        after: Vec<TilePatch>,
    ) -> Option<(Vec<TilePatch>, u64, u64)> {
        let doc = self.docs.get_mut(doc_id)?;
        // Validate the layer exists BEFORE opening a history entry, so a failed
        // commit (missing layer) does NOT leave a phantom cursor movement.
        if doc.layers.get_mut(layer_id).is_none() {
            return None;
        }
        let version = doc
            .history
            .apply_pixel_patch(layer_id, before, after.clone());
        let layer = doc.layers.get_mut(layer_id).unwrap();
        for t in &after {
            layer.write_tile(t);
        }
        layer.epoch += 1;
        Some((after, layer.epoch, version))
    }

    /// C5.4 fill pilot: write an explicit RGBA region into the canonical buffer
    /// as ONE `Pixel` history entry (mirrors the brush's single-step contract).
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
                before.push(TilePatch { x: tile_x, y: tile_y, w: tile_w, h: tile_h, data: b_data });
                after.push(TilePatch { x: tile_x, y: tile_y, w: tile_w, h: tile_h, data: a_data });
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
    pub fn undo_pixel(&mut self, doc_id: &str) -> Option<(String, Vec<TilePatch>, u64, u64)> {
        let doc = self.docs.get_mut(doc_id)?;
        let res = doc.history.undo_pixel().ok()?;
        match res {
            (Some(layer_id), Some(tiles)) => {
                let layer = doc.layers.get_mut(&layer_id)?;
                for t in &tiles {
                    layer.write_tile(t);
                }
                layer.epoch += 1;
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
            (Some(layer_id), Some(tiles)) => {
                let layer = doc.layers.get_mut(&layer_id)?;
                for t in &tiles {
                    layer.write_tile(t);
                }
                layer.epoch += 1;
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
}

// Process-lifetime registry. The C4-pilot `static PIXEL_STORE: Option<HashMap<..>>`
// is replaced here by a document-namespaced owner; the outer `Option` keeps the
// lazy-init pattern already used by the Tauri command layer.
static REGISTRY: Mutex<Option<PixelStoreRegistry>> = Mutex::new(None);

pub fn registry() -> MutexGuard<'static, Option<PixelStoreRegistry>> {
    REGISTRY.lock().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(x: i64, y: i64, w: usize, h: usize, fill: u8) -> TilePatch {
        TilePatch {
            x,
            y,
            w,
            h,
            data: vec![fill; w * h * 4],
        }
    }

    #[test]
    fn snapshot_region_matches_canonical() {
        let w = 128u32;
        let h = 128u32;
        let mut layer = PixelLayer::new(w, h, vec![0u8; w as usize * h as usize * 4]);
        layer.pixels[0..4].copy_from_slice(&[42, 42, 42, 42]);
        let region = layer.snapshot_region(0, 0, 64, 64);
        assert_eq!(region[0], 42);
        assert_eq!(region.len(), 64 * 64 * 4);
    }

    #[test]
    fn all_tiles_covers_full_layer() {
        let w = 512u32;
        let h = 256u32;
        let mut layer = PixelLayer::new(w, h, vec![0u8; w as usize * h as usize * 4]);
        let tiles = layer.all_tiles();
        // 512/256 = 2 cols x 1 row = 2 tiles of 256x256.
        assert_eq!(tiles.len(), 2);
        let total: usize = tiles.iter().map(|t| t.w * t.h).sum();
        assert_eq!(total, w as usize * h as usize);
        // Every byte of the canonical buffer is represented by exactly one tile.
        let mut cover = vec![0u8; w as usize * h as usize * 4];
        for t in &tiles {
            for row in 0..t.h {
                let dst = ((t.y + row as i64) as usize * w as usize + t.x as usize) * 4;
                let src = row * t.w * 4;
                cover[dst..dst + t.w * 4].copy_from_slice(&t.data[src..src + t.w * 4]);
            }
        }
        assert_eq!(cover, layer.pixels);
    }

    // ── C5.1 lifecycle ──

    fn reg() -> PixelStoreRegistry {
        PixelStoreRegistry::new()
    }

    #[test]
    fn document_close_releases_all_pixel_storage() {
        let mut r = reg();
        r.open_document("docA");
        r.add_layer("docA", "L1", 64, 64, vec![0; 64 * 64 * 4])
            .unwrap();
        r.add_layer("docA", "L2", 32, 32, vec![0; 32 * 32 * 4])
            .unwrap();
        assert!(r.get_layer("docA", "L1").is_some());
        assert!(r.get_layer("docA", "L2").is_some());
        r.close_document("docA");
        assert!(r.get_layer("docA", "L1").is_none());
        assert!(r.get_layer("docA", "L2").is_none());
    }

    #[test]
    fn remove_layer_releases_storage() {
        let mut r = reg();
        r.open_document("docA");
        r.add_layer("docA", "L1", 64, 64, vec![0; 64 * 64 * 4])
            .unwrap();
        r.remove_layer("docA", "L1");
        assert!(r.get_layer("docA", "L1").is_none());
    }

    #[test]
    fn resize_layer_recreates_dimensions() {
        let mut r = reg();
        r.open_document("docA");
        r.add_layer("docA", "L1", 64, 64, vec![0; 64 * 64 * 4])
            .unwrap();
        // Mutate, then "resize" to 32x32 — old 64x64 index math must no longer apply.
        r.resize_layer("docA", "L1", 32, 32, vec![1; 32 * 32 * 4])
            .unwrap();
        let layer = r.get_layer("docA", "L1").unwrap();
        assert_eq!(layer.width, 32);
        assert_eq!(layer.height, 32);
        assert_eq!(layer.pixels.len(), 32 * 32 * 4);
        // Old 64x64 offset (row 1 at x0,y64) is now out of bounds for the new buffer.
        assert_eq!(layer.pixels[0], 1);
    }

    #[test]
    fn same_layer_id_in_different_documents_isolated() {
        let mut r = reg();
        r.open_document("docA");
        r.open_document("docB");
        r.add_layer("docA", "L", 64, 64, vec![0; 64 * 64 * 4])
            .unwrap();
        r.add_layer("docB", "L", 64, 64, vec![0; 64 * 64 * 4])
            .unwrap();
        // Mutate only docA's "L".
        r.get_layer_mut("docA", "L").unwrap().pixels[0] = 42;
        // docB's "L" canonical must be unchanged (still all-zero).
        let b = r.get_layer("docB", "L").unwrap();
        assert!(b.pixels.iter().all(|&v| v == 0), "docB isolated from docA");
        let a = r.get_layer("docA", "L").unwrap();
        assert_eq!(a.pixels[0], 42);
    }

    // ── C5.2 epoch ──

    #[test]
    fn epoch_starts_zero_and_increments_on_mutation() {
        let mut r = reg();
        r.open_document("docA");
        r.add_layer("docA", "L1", 64, 64, vec![0u8; 64 * 64 * 4])
            .unwrap();
        assert_eq!(r.get_epoch("docA", "L1").unwrap(), 0);
        r.apply_pixel_patch(
            "docA",
            "L1",
            vec![tile(0, 0, 32, 32, 0)],
            vec![tile(0, 0, 32, 32, 5)],
        );
        assert_eq!(r.get_epoch("docA", "L1").unwrap(), 1);
        r.undo_pixel("docA");
        assert_eq!(r.get_epoch("docA", "L1").unwrap(), 2);
        r.redo_pixel("docA");
        assert_eq!(r.get_epoch("docA", "L1").unwrap(), 3);
    }

    #[test]
    fn registry_epoch_per_layer_independent() {
        let mut r = reg();
        r.open_document("docA");
        r.add_layer("docA", "L1", 64, 64, vec![0; 64 * 64 * 4])
            .unwrap();
        r.add_layer("docA", "L2", 64, 64, vec![0; 64 * 64 * 4])
            .unwrap();
        assert_eq!(r.get_epoch("docA", "L1").unwrap(), 0);
        r.apply_pixel_patch(
            "docA",
            "L1",
            vec![tile(0, 0, 32, 32, 0)],
            vec![tile(0, 0, 32, 32, 3)],
        );
        assert_eq!(r.get_epoch("docA", "L1").unwrap(), 1);
        // L2 untouched.
        assert_eq!(r.get_epoch("docA", "L2").unwrap(), 0);
    }

    // ── C5.3-A: unified pixel history (ProtocolEngine is the sole authoritative cursor) ──

    #[test]
    fn apply_pixel_patch_moves_cursor_and_bumps_version_once() {
        let mut r = reg();
        r.open_document("docA");
        r.add_layer("docA", "L1", 8, 8, vec![0; 8 * 8 * 4]).unwrap();
        let _ = r.apply_pixel_patch(
            "docA",
            "L1",
            vec![tile(0, 0, 8, 8, 0)],
            vec![tile(0, 0, 8, 8, 10)],
        );
        let _ = r.apply_pixel_patch(
            "docA",
            "L1",
            vec![tile(0, 0, 8, 8, 10)],
            vec![tile(0, 0, 8, 8, 20)],
        );
        let _ = r.apply_pixel_patch(
            "docA",
            "L1",
            vec![tile(0, 0, 8, 8, 20)],
            vec![tile(0, 0, 8, 8, 30)],
        );
        assert_eq!(r.get_history_cursor("docA"), Some(3));
        assert_eq!(r.get_history_version("docA"), Some(3));
        assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 30);
    }

    #[test]
    fn mixed_brush_adjustment_brush_undo_redo_exact() {
        let mut r = reg();
        r.open_document("docA");
        r.add_layer("docA", "L1", 1, 1, vec![0, 0, 0, 255]).unwrap();

        // A: brush -> 11
        r.apply_pixel_patch(
            "docA",
            "L1",
            vec![tile(0, 0, 1, 1, 0)],
            vec![tile(0, 0, 1, 1, 11)],
        );
        // B: adjustment/baked -> 22
        r.apply_pixel_patch(
            "docA",
            "L1",
            vec![tile(0, 0, 1, 1, 11)],
            vec![tile(0, 0, 1, 1, 22)],
        );
        // C: brush -> 33
        r.apply_pixel_patch(
            "docA",
            "L1",
            vec![tile(0, 0, 1, 1, 22)],
            vec![tile(0, 0, 1, 1, 33)],
        );

        assert_eq!(r.get_history_cursor("docA"), Some(3));
        assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 33);

        // undo -> B (22)
        let u = r.undo_pixel("docA").unwrap();
        assert_eq!(u.0, "L1");
        assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 22);
        assert_eq!(r.get_history_cursor("docA"), Some(2));

        // undo -> A (11)
        let _ = r.undo_pixel("docA").unwrap();
        assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 11);
        assert_eq!(r.get_history_cursor("docA"), Some(1));

        // redo -> B (22)
        let _ = r.redo_pixel("docA").unwrap();
        assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 22);
        assert_eq!(r.get_history_cursor("docA"), Some(2));

        // redo -> C (33)
        let _ = r.redo_pixel("docA").unwrap();
        assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 33);
        assert_eq!(r.get_history_cursor("docA"), Some(3));
    }

    #[test]
    fn cross_layer_undo_reverts_only_last_entry_layer() {
        let mut r = reg();
        r.open_document("docA");
        r.add_layer("docA", "L1", 1, 1, vec![0, 0, 0, 255]).unwrap();
        r.add_layer("docA", "L2", 1, 1, vec![0, 0, 0, 255]).unwrap();
        r.apply_pixel_patch(
            "docA",
            "L1",
            vec![tile(0, 0, 1, 1, 0)],
            vec![tile(0, 0, 1, 1, 11)],
        );
        r.apply_pixel_patch(
            "docA",
            "L2",
            vec![tile(0, 0, 1, 1, 0)],
            vec![tile(0, 0, 1, 1, 22)],
        );
        // undo reverts L2 only
        r.undo_pixel("docA").unwrap();
        assert_eq!(r.get_layer("docA", "L2").unwrap().pixels[0], 0);
        assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 11);
    }

    #[test]
    fn apply_pixel_patch_missing_layer_does_not_move_cursor() {
        let mut r = reg();
        r.open_document("docA");
        // No layer seeded — the command must fail WITHOUT pushing a history entry.
        let res = r.apply_pixel_patch(
            "docA",
            "L-missing",
            vec![tile(0, 0, 1, 1, 0)],
            vec![tile(0, 0, 1, 1, 5)],
        );
        assert!(res.is_none(), "no entry pushed for missing layer");
        assert_eq!(
            r.get_history_cursor("docA"),
            Some(0),
            "cursor unchanged on failure"
        );
    }

    #[test]
    fn document_isolation_of_history_cursor() {
        let mut r = reg();
        r.open_document("docA");
        r.open_document("docB");
        r.add_layer("docA", "L", 1, 1, vec![0, 0, 0, 255]).unwrap();
        r.add_layer("docB", "L", 1, 1, vec![0, 0, 0, 255]).unwrap();
        r.apply_pixel_patch(
            "docA",
            "L",
            vec![tile(0, 0, 1, 1, 0)],
            vec![tile(0, 0, 1, 1, 77)],
        );
        // undo on docB (empty) must be a no-op
        assert!(r.undo_pixel("docB").is_none());
        assert_eq!(r.get_history_cursor("docA"), Some(1));
        assert_eq!(r.get_history_cursor("docB"), Some(0));
    }

    #[test]
    fn new_command_after_undo_invalidates_redo() {
        let mut r = reg();
        r.open_document("docA");
        r.add_layer("docA", "L1", 1, 1, vec![0, 0, 0, 255]).unwrap();
        r.apply_pixel_patch(
            "docA",
            "L1",
            vec![tile(0, 0, 1, 1, 0)],
            vec![tile(0, 0, 1, 1, 1)],
        );
        r.apply_pixel_patch(
            "docA",
            "L1",
            vec![tile(0, 0, 1, 1, 1)],
            vec![tile(0, 0, 1, 1, 2)],
        );
        r.undo_pixel("docA").unwrap(); // cursor 1
                                       // new branch
        r.apply_pixel_patch(
            "docA",
            "L1",
            vec![tile(0, 0, 1, 1, 1)],
            vec![tile(0, 0, 1, 1, 9)],
        );
        assert_eq!(r.get_history_cursor("docA"), Some(2));
        // redo must be inert (truncated)
        assert!(r.redo_pixel("docA").is_none());
        assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 9);
    }

    #[test]
    fn deep_history_cursor_unchanged_across_many_entries() {
        let mut r = reg();
        r.open_document("docA");
        r.add_layer("docA", "L1", 1, 1, vec![0, 0, 0, 255]).unwrap();
        let n: u8 = 50;
        for i in 1..=n {
            r.apply_pixel_patch(
                "docA",
                "L1",
                vec![tile(0, 0, 1, 1, i - 1)],
                vec![tile(0, 0, 1, 1, i)],
            );
        }
        assert_eq!(r.get_history_cursor("docA"), Some(n as usize));
        assert_eq!(r.get_history_version("docA"), Some(n as u64));
        // undo all the way down
        for k in (0..n).rev() {
            let _ = r.undo_pixel("docA").unwrap();
            assert_eq!(r.get_history_cursor("docA"), Some(k as usize));
        }
        assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 0);
        // redo all the way up
        for k in 1..=n {
            let _ = r.redo_pixel("docA").unwrap();
            assert_eq!(r.get_history_cursor("docA"), Some(k as usize));
        }
        assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], n);
    }

    // ── Bug 1: overlapping strokes must composite onto canonical, not wipe ──
    #[test]
    fn commit_pixels_composites_onto_canonical_overlapping() {
        let mut r = PixelStoreRegistry::new();
        r.open_document("d");
        r.add_layer("d", "L", 4, 4, vec![255; 4 * 4 * 4]); // white 4x4
                                                           // stroke A: 2x2 red opaque dab at (1,1) — covers (0,0) within tile (0,0).
        let mut tip_a_data = Vec::new();
        for _ in 0..4 {
            tip_a_data.extend_from_slice(&[255u8, 0, 0, 255]);
        }
        let tip_a = ParityTip {
            width: 2,
            height: 2,
            data: &tip_a_data,
        };
        let dab = vec![ParityDab {
            x: 1.0,
            y: 1.0,
            alpha: 1.0,
        }];
        let _ = r
            .commit_pixels(
                "d",
                "L",
                vec![0; 4 * 4 * 4],
                4,
                4,
                false,
                2.0,
                &dab,
                &tip_a,
                true,
            )
            .unwrap();
        let l = r.get_layer("d", "L").unwrap();
        assert_eq!(l.pixels[0], 255); // R
        assert_eq!(l.pixels[1], 0); // G (red) at corner (0,0)
        assert_eq!(l.pixels[(3 * 4 + 3) * 4 + 1], 255); // far corner white

        // stroke B: 1x1 blue opaque dab at (1,1) — overlaps A within the same tile.
        let tip_b = ParityTip {
            width: 1,
            height: 1,
            data: &[0u8, 0, 255, 255],
        };
        let _ = r
            .commit_pixels(
                "d",
                "L",
                vec![0; 4 * 4 * 4],
                4,
                4,
                false,
                1.0,
                &dab,
                &tip_b,
                true,
            )
            .unwrap();
        let l2 = r.get_layer("d", "L").unwrap();
        // Bug 1 assertion: A's pixel (0,0) must SURVIVE B's tile REPLACE
        // (G stays 0 red, NOT 255 white). Old dab-on-white path would wipe it.
        assert_eq!(
            l2.pixels[1], 0,
            "A's pixel wiped by overlapping B tile REPLACE"
        );
        assert_eq!(l2.pixels[(1 * 4 + 1) * 4], 0); // overlap (1,1) now blue R==0
        assert_eq!(l2.pixels[(1 * 4 + 1) * 4 + 2], 255); // blue B==255
        assert_eq!(l2.pixels[(3 * 4 + 3) * 4 + 1], 255); // far corner still white
    }

    #[test]
    fn commit_pixels_reinit_after_close_is_idempotent() {
        let mut r = PixelStoreRegistry::new();
        r.open_document("d");
        r.add_layer("d", "L", 2, 2, vec![255; 16]);
        let tip = ParityTip {
            width: 1,
            height: 1,
            data: &[10u8, 20, 30, 255],
        };
        let dab = vec![ParityDab {
            x: 0.5,
            y: 0.5,
            alpha: 1.0,
        }];
        let _ = r
            .commit_pixels("d", "L", vec![0; 16], 2, 2, false, 1.0, &dab, &tip, true)
            .unwrap();
        assert_eq!(r.get_layer("d", "L").unwrap().pixels[0], 10);
        // close releases the store (simulating doc close).
        r.close_document("d");
        assert!(r.get_layer("d", "L").is_none());
        // reopen: ensure-if-absent re-inits from bytes; commit still composites.
        r.open_document("d");
        let _ = r
            .commit_pixels("d", "L", vec![255; 16], 2, 2, false, 1.0, &dab, &tip, true)
            .unwrap();
        assert_eq!(r.get_layer("d", "L").unwrap().pixels[0], 10);
    }

    #[test]
    fn commit_pixels_doc_namespace_independent() {
        let mut r = PixelStoreRegistry::new();
        r.open_document("A");
        r.open_document("B");
        r.add_layer("A", "L", 2, 2, vec![255; 16]);
        r.add_layer("B", "L", 2, 2, vec![255; 16]); // same layerId, different doc
        let tip = ParityTip {
            width: 1,
            height: 1,
            data: &[1u8, 2, 3, 255],
        };
        let dab = vec![ParityDab {
            x: 0.5,
            y: 0.5,
            alpha: 1.0,
        }];
        let _ = r
            .commit_pixels("A", "L", vec![0; 16], 2, 2, false, 1.0, &dab, &tip, true)
            .unwrap();
        // doc B must be unaffected (separate namespace).
        assert_eq!(r.get_layer("B", "L").unwrap().pixels[0], 255);
        let _ = r
            .commit_pixels("B", "L", vec![0; 16], 2, 2, false, 1.0, &dab, &tip, true)
            .unwrap();
        assert_eq!(r.get_layer("B", "L").unwrap().pixels[0], 1);
        assert_eq!(r.get_layer("A", "L").unwrap().pixels[0], 1);
    }

    // ── C5.4 fill pilot: write_region ──

    #[test]
    fn c5_4_write_region_writes_canonical_and_advances_history_once() {
        let mut r = PixelStoreRegistry::new();
        r.open_document("d");
        r.add_layer("d", "L", 8, 8, vec![0u8; 8 * 8 * 4]).unwrap();
        let before_ver = r.get_history_version("d").unwrap();
        let before_epoch = r.get_epoch("d", "L").unwrap();
        let mut rgba = vec![0u8; 4 * 4 * 4];
        for c in rgba.chunks_mut(4) {
            c.copy_from_slice(&[255, 0, 0, 255]);
        }
        let (b, a, epoch, version) = r.write_region("d", "L", 2, 2, 4, 4, rgba).expect("write");
        let px = r.get_layer("d", "L").unwrap().pixels.clone();
        assert_eq!(px[(3 * 8 + 3) * 4 + 0], 255, "inside region → filled");
        assert_eq!(px[(0 * 8 + 0) * 4 + 0], 0, "outside region → untouched");
        // Exactly ONE history step + ONE epoch bump.
        assert_eq!(version, before_ver + 1, "version +1");
        assert_eq!(epoch, before_epoch + 1, "epoch +1");
        assert_eq!(r.get_history_cursor("d"), Some(1), "cursor +1");
        // before/after are full 256-tiles (layer < 256) covering the region.
        assert_eq!(b.len(), 1);
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn c5_4_write_region_tiles_localized_and_full() {
        let mut r = PixelStoreRegistry::new();
        r.open_document("d");
        r.add_layer("d", "L", 300, 300, vec![0u8; 300 * 300 * 4]).unwrap();
        let mut rgba = vec![0u8; 10 * 10 * 4];
        for c in rgba.chunks_mut(4) {
            c.copy_from_slice(&[0, 255, 0, 255]);
        }
        // Region (250,150,10,10) crosses the x=256 tile boundary.
        let (b, a, _, _) = r.write_region("d", "L", 250, 150, 10, 10, rgba).expect("write");
        assert_eq!(b.len(), 2, "exactly two 256-tiles intersect the region");
        for t in b.iter().chain(a.iter()) {
            // Tiles are 256-grid-aligned (edge tiles clipped to layer bounds).
            assert_eq!(t.x % 256, 0, "tile x on 256 grid");
            assert_eq!(t.y % 256, 0, "tile y on 256 grid");
        }
    }

    #[test]
    fn c5_4_write_region_undo_redo_roundtrip() {
        let mut r = PixelStoreRegistry::new();
        r.open_document("d");
        r.add_layer("d", "L", 8, 8, vec![0u8; 8 * 8 * 4]).unwrap();
        let before = r.get_layer("d", "L").unwrap().snapshot_region(0, 0, 8, 8);
        let mut rgba = vec![0u8; 4 * 4 * 4];
        for c in rgba.chunks_mut(4) {
            c.copy_from_slice(&[1, 2, 3, 255]);
        }
        r.write_region("d", "L", 2, 2, 4, 4, rgba).unwrap();
        assert_eq!(r.get_layer("d", "L").unwrap().pixels[(3 * 8 + 3) * 4 + 0], 1);
        let (_lid, _tiles, _e, _v) = r.undo_pixel("d").expect("undo");
        let after_undo = r.get_layer("d", "L").unwrap().snapshot_region(0, 0, 8, 8);
        assert_eq!(after_undo, before, "undo restores pre-fill canonical");
        let _ = r.redo_pixel("d").expect("redo");
        assert_eq!(r.get_layer("d", "L").unwrap().pixels[(3 * 8 + 3) * 4 + 0], 1, "redo re-applies fill");
    }

    #[test]
    fn c5_4_write_region_new_write_truncates_redo() {
        let mut r = PixelStoreRegistry::new();
        r.open_document("d");
        r.add_layer("d", "L", 8, 8, vec![0u8; 8 * 8 * 4]).unwrap();
        let mut red = vec![0u8; 4 * 4 * 4];
        for c in red.chunks_mut(4) {
            c.copy_from_slice(&[9, 9, 9, 255]);
        }
        r.write_region("d", "L", 0, 0, 4, 4, red.clone()).unwrap();
        r.undo_pixel("d");
        assert!(r.redo_pixel("d").is_some(), "redo available after undo");
        // New write after undo → future redo severed.
        r.write_region("d", "L", 4, 4, 4, 4, red.clone()).unwrap();
        assert!(r.redo_pixel("d").is_none(), "redo truncated after new write");
        assert_eq!(r.get_history_cursor("d"), Some(2));
    }

    #[test]
    fn c5_4_write_region_out_of_bounds_rejected_no_cursor() {
        let mut r = PixelStoreRegistry::new();
        r.open_document("d");
        r.add_layer("d", "L", 8, 8, vec![0u8; 8 * 8 * 4]).unwrap();
        let rgba = vec![0u8; 4 * 4 * 4];
        assert!(r.write_region("d", "L", 6, 6, 4, 4, rgba.clone()).is_none(), "overflow rejected");
        assert!(r.write_region("d", "L", 0, 0, 4, 4, vec![0u8; 3]).is_none(), "size mismatch rejected");
        assert_eq!(r.get_history_cursor("d"), Some(0), "no cursor movement on rejected write");
    }

    #[test]
    fn c5_4_write_region_history_bounded_to_50() {
        let mut r = PixelStoreRegistry::new();
        r.open_document("d");
        r.add_layer("d", "L", 64, 64, vec![0u8; 64 * 64 * 4]).unwrap();
        let mut rgba = vec![0u8; 4 * 4 * 4];
        for c in rgba.chunks_mut(4) {
            c.copy_from_slice(&[7, 7, 7, 255]);
        }
        for i in 0..60u8 {
            let x = (i % 8) as i64 * 4;
            let y = (i / 8) as i64 * 4;
            r.write_region("d", "L", x, y, 4, 4, rgba.clone()).unwrap();
        }
        assert_eq!(r.get_history_cursor("d"), Some(50), "cursor capped at max_depth");
        for _ in 0..50 {
            assert!(r.undo_pixel("d").is_some());
        }
        assert!(r.undo_pixel("d").is_none(), "history bounded to 50 entries");
    }

    // (The external-pending barrier rejection is covered by the ProtocolEngine
    //  unit test `pixel_undo_redo_rejected_while_external_pending` in protocol.rs.)
}
