// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pixel write paths: patch commits, region writes, composite commits and the
//! pixel undo/redo replay.

use super::PixelStoreRegistry;
use crate::raster::paint::raster::{raster_shadow, RasterDab, RasterTip};
use crate::raster::tile::TilePatch;
use std::time::Instant;

/// Where one brush commit spent its time, plus the tile-aligned crop it
/// rasterized. `crop_w == width && crop_h == height` means the stroke covered
/// the whole layer. Timing fields are diagnostic only.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CommitPhases {
    pub crop_x: u32,
    pub crop_y: u32,
    pub crop_w: u32,
    pub crop_h: u32,
    pub premultiply_us: u64,
    pub raster_us: u64,
    pub patch_us: u64,
    pub apply_us: u64,
}

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

impl PixelStoreRegistry {
    // ── Unified pixel history (authoritative) ──
    // These route through the document's `DocumentCore` so the pixel history
    // cursor is the SAME cursor used by metadata history. They write the
    // canonical `PixelLayer.pixels` + bump `epoch` directly. The legacy
    // per-layer `undo_stack`/`redo_stack` history mechanism was removed; the only
    // pixel history is now the document's `DocumentCore` (bounded to
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
        let epoch = doc.layers.get(layer_id)?.epoch();
        // Build the before/after `Arc<StateNode>` pair by COW-ing the change
        // regions into the maintained per-layer canon. The `before`/`after`
        // tile-patch args are the caller's pre/post image; they stay unchanged
        // (row-major + host surface byte-identical).
        let (before_arc, after_arc) = doc.cow_tiles(layer_id, &after, epoch)?;
        let version = doc
            .history
            .apply_pixel_patch(layer_id, before_arc, after_arc);
        let layer = doc.layers.get_mut(layer_id).unwrap();
        for t in &after {
            layer.write_tile(t);
        }
        let epoch_after = layer.bump_epoch();
        Some((after, epoch_after, version))
    }

    /// Fill: write an explicit RGBA region into the canonical buffer as ONE
    /// `Pixel` history entry (mirrors the brush's single-step contract).
    /// Returns `(before_tiles, after_tiles, epoch, version)`. Bumps epoch+version
    /// exactly once; the unified history cursor advances one step. The Rust entry
    /// is subordinate to the document history stream that drives undo/redo — there
    /// is NO separate host-visible step, so an undo is exactly ONE user action.
    #[allow(clippy::too_many_arguments)] // flat args mirror the pixel-write command struct
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
        if x < 0 || y < 0 || w == 0 || h == 0 {
            return None;
        }
        let w_i64 = i64::try_from(w).ok()?;
        let h_i64 = i64::try_from(h).ok()?;
        let x_end = x.checked_add(w_i64)?;
        let y_end = y.checked_add(h_i64)?;
        let byte_len = w.checked_mul(h)?.checked_mul(4)?;
        if rgba.len() != byte_len {
            return None;
        }

        let (lw, lh) = {
            let layer = self.docs.get(doc_id)?.layers.get(layer_id)?;
            (layer.width as i64, layer.height as i64)
        };
        if x_end > lw || y_end > lh {
            return None;
        }

        const TW: usize = 256;
        let cols = (lw as usize).div_ceil(TW);
        let rows = (lh as usize).div_ceil(TW);
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
                let rx1 = x_end.min(tile_x + tile_w as i64);
                let ry1 = y_end.min(tile_y + tile_h as i64);
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
    ///
    /// The raster is bounded to the tile-aligned rectangle covering every dab's
    /// footprint. A dab cannot change a pixel outside that rectangle, so the
    /// emitted tiles are byte-identical to a full-layer pass while the
    /// premultiply/allocation cost scales with the stroke, not the document.
    #[allow(clippy::too_many_arguments)] // flat args mirror the compute-commit command struct
    pub fn compute_commit(
        &self,
        doc_id: &str,
        layer_id: &str,
        w: usize,
        h: usize,
        eraser: bool,
        brush: f64,
        dabs: &[RasterDab],
        tip: &RasterTip,
        include_tiles: bool,
    ) -> Option<(Vec<TilePatch>, Vec<TilePatch>)> {
        let mut phases = CommitPhases::default();
        self.compute_commit_inner(
            doc_id,
            layer_id,
            w,
            h,
            eraser,
            brush,
            dabs,
            tip,
            include_tiles,
            &mut phases,
        )
    }

    #[allow(clippy::too_many_arguments)] // see `compute_commit`
    fn compute_commit_inner(
        &self,
        doc_id: &str,
        layer_id: &str,
        w: usize,
        h: usize,
        eraser: bool,
        brush: f64,
        dabs: &[RasterDab],
        tip: &RasterTip,
        include_tiles: bool,
        phases: &mut CommitPhases,
    ) -> Option<(Vec<TilePatch>, Vec<TilePatch>)> {
        let layer = self.docs.get(doc_id)?.layers.get(layer_id)?;
        if dabs.is_empty() {
            return Some((Vec::new(), Vec::new()));
        }
        let (crop_x, crop_y, crop_w, crop_h) = commit_crop_rect(w, h, brush, dabs)?;
        phases.crop_x = crop_x as u32;
        phases.crop_y = crop_y as u32;
        phases.crop_w = crop_w as u32;
        phases.crop_h = crop_h as u32;

        // Canonical store is straight-alpha; `raster_shadow` composites in
        // premultiplied space, so premultiply only the crop before handing it over.
        let started = Instant::now();
        let mut base_premult = Vec::with_capacity(crop_w.saturating_mul(crop_h).saturating_mul(4));
        let row_bytes = crop_w * 4;
        for row in crop_y..crop_y + crop_h {
            let start = (row * w + crop_x) * 4;
            let end = start + row_bytes;
            let mut i = start;
            while i < end {
                let p = premul(&layer.pixels, i);
                base_premult.push(p[0]);
                base_premult.push(p[1]);
                base_premult.push(p[2]);
                base_premult.push(p[3]);
                i += 4;
            }
        }
        phases.premultiply_us = micros(started);
        let shifted: Vec<RasterDab> = dabs
            .iter()
            .map(|dab| RasterDab {
                x: dab.x - crop_x as f64,
                y: dab.y - crop_y as f64,
                alpha: dab.alpha,
            })
            .collect();
        let (meta, after_out) = raster_shadow(
            crop_w,
            crop_h,
            true,
            Some(base_premult),
            eraser,
            brush,
            &shifted,
            tip,
            include_tiles,
        );
        phases.raster_us = u64::try_from(meta.raster_us).unwrap_or(u64::MAX);
        phases.patch_us = u64::try_from(meta.patchgen_us).unwrap_or(u64::MAX);

        // Shift the crop-local tiles back into document space.
        let after: Vec<TilePatch> = after_out
            .iter()
            .map(|t| TilePatch {
                x: t.x + crop_x as i64,
                y: t.y + crop_y as i64,
                w: t.w,
                h: t.h,
                data: t.data.clone(),
            })
            .collect();
        // `before` = same geometry, straight-alpha, extracted from the canonical buffer.
        let before: Vec<TilePatch> = after
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

    /// Full-layer reference used only to prove the bounded pass emits the same
    /// tiles. Kept out of production builds.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compute_commit_full(
        &self,
        doc_id: &str,
        layer_id: &str,
        w: usize,
        h: usize,
        eraser: bool,
        brush: f64,
        dabs: &[RasterDab],
        tip: &RasterTip,
        include_tiles: bool,
    ) -> Option<(Vec<TilePatch>, Vec<TilePatch>)> {
        let layer = self.docs.get(doc_id)?.layers.get(layer_id)?;
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
    #[allow(clippy::too_many_arguments)] // flat args mirror the commit-pixels command struct
    pub fn commit_pixels(
        &mut self,
        doc_id: &str,
        layer_id: &str,
        bytes: Vec<u8>,
        w: usize,
        h: usize,
        eraser: bool,
        brush: f64,
        dabs: &[RasterDab],
        tip: &RasterTip,
        include_tiles: bool,
    ) -> Option<(Vec<TilePatch>, Vec<TilePatch>, u64, u64)> {
        self.commit_pixels_with_phases(
            doc_id,
            layer_id,
            bytes,
            w,
            h,
            eraser,
            brush,
            dabs,
            tip,
            include_tiles,
            None,
        )
    }

    /// [`Self::commit_pixels`] plus the bounded-crop/phase record used by the
    /// input measurement harness. `phases` is diagnostic only; the committed
    /// pixels and history entry are identical.
    #[allow(clippy::too_many_arguments)] // mirror of `commit_pixels`
    pub fn commit_pixels_with_phases(
        &mut self,
        doc_id: &str,
        layer_id: &str,
        bytes: Vec<u8>,
        w: usize,
        h: usize,
        eraser: bool,
        brush: f64,
        dabs: &[RasterDab],
        tip: &RasterTip,
        include_tiles: bool,
        phases: Option<&mut CommitPhases>,
    ) -> Option<(Vec<TilePatch>, Vec<TilePatch>, u64, u64)> {
        // Bug 2 fix: ensure-if-absent (idempotent). No host-side seeded flag
        // needed; Rust is the single source of truth for whether the layer exists.
        if self.get_layer(doc_id, layer_id).is_none() {
            self.add_layer(doc_id, layer_id, w as u32, h as u32, bytes)
                .ok()?;
        }
        let mut fallback = CommitPhases::default();
        let phases = match phases {
            Some(phases) => phases,
            None => &mut fallback,
        };
        let (before, after) = self.compute_commit_inner(
            doc_id,
            layer_id,
            w,
            h,
            eraser,
            brush,
            dabs,
            tip,
            include_tiles,
            phases,
        )?;
        let started = Instant::now();
        let res = self.apply_pixel_patch(doc_id, layer_id, before.clone(), after.clone())?;
        phases.apply_us = micros(started);
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
                let epoch_after = layer.bump_epoch();
                if let Some(ls) = doc.state_nodes.get_mut(&layer_id) {
                    ls.set_current(node);
                }
                Some((layer_id, tiles, epoch_after, doc.history.version()))
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
                let epoch_after = layer.bump_epoch();
                if let Some(ls) = doc.state_nodes.get_mut(&layer_id) {
                    ls.set_current(node);
                }
                Some((layer_id, tiles, epoch_after, doc.history.version()))
            }
            _ => None,
        }
    }
}

fn micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Tile-aligned rectangle covering every dab's raster footprint.
///
/// Alignment matters: `raster_shadow` extracts whole 256 px tiles, so a crop
/// that started mid-tile would emit a partial edge tile whose bytes no longer
/// match the full pass. A tile-aligned crop reproduces the same tile set after
/// shifting local coordinates back by the crop origin.
fn commit_crop_rect(
    w: usize,
    h: usize,
    brush: f64,
    dabs: &[RasterDab],
) -> Option<(usize, usize, usize, usize)> {
    if w == 0 || h == 0 || dabs.is_empty() {
        return None;
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for dab in dabs {
        if !dab.x.is_finite() || !dab.y.is_finite() {
            return None;
        }
        min_x = min_x.min(dab.x);
        min_y = min_y.min(dab.y);
        max_x = max_x.max(dab.x);
        max_y = max_y.max(dab.y);
    }
    // Must match `raster_shadow`'s own dirty bbox margins exactly.
    let margin = (brush / 2.0).ceil() as i64 + 2;
    let x0 = ((min_x.floor() as i64) - margin).clamp(0, w as i64 - 1);
    let y0 = ((min_y.floor() as i64) - margin).clamp(0, h as i64 - 1);
    let x1 = ((max_x.ceil() as i64) + margin).clamp(x0 + 1, w as i64);
    let y1 = ((max_y.ceil() as i64) + margin).clamp(y0 + 1, h as i64);
    const TILE: i64 = 256;
    let crop_x = (x0 / TILE) * TILE;
    let crop_y = (y0 / TILE) * TILE;
    let crop_right = (((x1 - 1) / TILE + 1) * TILE).min(w as i64);
    let crop_bottom = (((y1 - 1) / TILE + 1) * TILE).min(h as i64);
    Some((
        crop_x as usize,
        crop_y as usize,
        (crop_right - crop_x) as usize,
        (crop_bottom - crop_y) as usize,
    ))
}
