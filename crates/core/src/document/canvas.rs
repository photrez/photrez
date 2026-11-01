// SPDX-License-Identifier: AGPL-3.0-or-later
// Canvas-size command-arm bodies (Crop Canvas / Apply Crop / Resize Canvas).
// Canvas command family, kept separate from the apply dispatcher so both stay
// under the size cap; this is a sibling `impl DocumentCore` block (method visibility is
// per-impl, not per module).
//
// These arms manage the engine document size (`doc_size`), undoable state that is
// distinct from the LayerSet. Each opens a forward native entry (begin_forward /
// finish_forward) like the metadata arms, sets `doc_size`, and (for the crop arms)
// re-derives every unlocked layer's transform. The delta is an ordered Upsert of
// EVERY layer in final engine order for the crop arms; Resize Canvas emits an
// empty layer delta (its only effect is the document size, which rides the
// snapshot). Document size is restored on undo/redo by the apply() walker, which
// swaps `doc_size` from the entry's captured pre/post values.
use super::*;

/// Build an E_INVALID-style ProtocolError (mirrors the other arms' reject shape).
fn e_invalid(msg: &str) -> ProtocolError {
    ProtocolError {
        code: "E_INVALID".to_string(),
        message: msg.to_string(),
    }
}

/// Normalize a rotation angle in degrees to the canonical (-180, 180] range:
/// `angle % 360`, then `> 180 -> -360`, then `< -180 -> +360`. Verified against
/// the reference viewport geometry (e.g. -270 -> 90, 360 -> 0, 181 -> -179).
fn normalize_rotation(angle_deg: f64) -> f64 {
    let mut a = angle_deg % 360.0;
    if a > 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}

impl DocumentCore {
    /// Crop the canvas: shift every unlocked layer's position by (-x, -y) and set
    /// the document size to (width, height). Follows the reference non-destructive
    /// crop-canvas math, which offsets unlocked layers and
    /// resizes the document. Locked layers are untouched (the guard is `!layer.locked`);
    /// `RenderLayer.locked` is `Option<bool>`, so `Some(true)` is locked and
    /// `None`/`false` is not.
    ///
    /// Trust boundary: reject non-finite inputs with E_INVALID before any mutation
    /// (the reference engine lets NaN/Inf silently slip through its `<= 0` comparisons and
    /// would proceed (a stricter-than-reference divergence). Non-positive width/height
    /// is a SILENT no-op (no history entry; the apply() tail still bumps DV),
    /// mirroring the reference guard `if (width <= 0 || height <= 0) return;`. x/y may be
    /// any sign (negative offsets are legal).
    pub(crate) fn apply_crop_canvas(
        &mut self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Result<Vec<RenderLayerChange>, ProtocolError> {
        if ![x, y, width, height].iter().all(|v| v.is_finite()) {
            return Err(e_invalid("cropCanvas requires finite x/y/width/height"));
        }
        // Silent no-op on non-positive dims: no entry, snapshot unchanged, DV +1.
        if width <= 0.0 || height <= 0.0 {
            return Ok(Vec::new());
        }
        let affected: Vec<String> = self.layers.0.iter().map(|l| l.id.clone()).collect();
        let _e = self.begin_forward("Crop Canvas", &affected);
        // Offset every unlocked layer. COW: only changed (unlocked) layers get a
        // fresh Arc; locked layers keep their shared Arc pointer.
        let mut i = 0;
        while i < self.layers.0.len() {
            let layer = self.layers.get(i).expect("layer present").clone();
            if layer.locked != Some(true) {
                let mut changed = layer;
                changed.x -= x;
                changed.y -= y;
                changed.dirty_rect = Some(Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                });
                self.layers = self.layers.replaced(i, changed);
            }
            i += 1;
        }
        // Document size becomes the crop rect; keep the canonical shadow coherent.
        self.doc_size = Some((width, height));
        if let Some(sh) = &mut self.replica {
            sh.doc.width = width;
            sh.doc.height = height;
        }
        // A crop clears the selection; mirror that onto the shadow too.
        self.set_engine_selection(None);
        self.finish_forward(_e);
        // Delta: ordered Upserts of ALL layers in final engine order.
        Ok(self
            .layers
            .0
            .iter()
            .map(|l| RenderLayerChange::Upsert {
                layer: l.as_ref().clone(),
            })
            .collect())
    }

    /// Apply a (non-destructive) crop with optional rotation and target size.
    /// Implements the non-destructive apply-crop branch exclusively; the
    /// pixel-baking cropped-pixel-delete and fill-background-color variants remain
    /// outside this command and are NOT represented here.
    /// For each unlocked layer, recenters it into the crop region,
    /// rotates it around the crop center, scales it to the (optional) target
    /// dimensions, and re-derives its final transform. Locked layers are skipped;
    /// `flips` are preserved unchanged.
    ///
    /// Trust boundary: reject non-finite x/y/width/height/rotation with E_INVALID
    /// before any mutation (the reference engine would proceed with NaN). A target size
    /// given as a half-pair (one of targetWidth/targetHeight without the other) is
    /// invalid (the target size is taken as a complete {w,h} pair). Non-positive
    /// width/height is a SILENT no-op (the first guard, checked before reading the
    /// target size); a target size that resolves to <= 0 is also a SILENT no-op so
    /// the document size can never collapse to 0x0.
    #[allow(clippy::too_many_arguments)] // flat args mirror the apply-crop command struct
    pub(crate) fn apply_apply_crop(
        &mut self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        rotation: Option<f64>,
        target_width: Option<f64>,
        target_height: Option<f64>,
        delete_cropped_pixels: bool,
        fill_background: Option<[u8; 4]>,
    ) -> Result<Vec<RenderLayerChange>, ProtocolError> {
        let rot = rotation.unwrap_or(0.0);
        if ![x, y, width, height, rot].iter().all(|v| v.is_finite()) {
            return Err(e_invalid(
                "applyCrop requires finite x/y/width/height/rotation",
            ));
        }
        // Target size is a complete pair or absent; a half-pair is meaningless.
        if target_width.is_some() != target_height.is_some() {
            return Err(e_invalid(
                "applyCrop target size requires both targetWidth and targetHeight",
            ));
        }
        // Silent no-op on non-positive crop rect (the first guard).
        if width <= 0.0 || height <= 0.0 {
            return Ok(Vec::new());
        }
        let final_w = target_width.unwrap_or(width);
        let final_h = target_height.unwrap_or(height);
        // A target size that collapses to zero or below is a silent no-op (same
        // convention as the non-positive crop rect above) so a zero-target
        // applyCrop can never collapse the document size to 0x0.
        if final_w <= 0.0 || final_h <= 0.0 {
            return Ok(Vec::new());
        }
        let crop_center_x = x + width / 2.0;
        let crop_center_y = y + height / 2.0;
        let rad = (-rot * std::f64::consts::PI) / 180.0;
        let cos = rad.cos();
        let sin = rad.sin();
        let export_scale_x = final_w / width;
        let export_scale_y = final_h / height;

        // Fill bakes a document-sized background, so capture the Background's
        // ORIGINAL placement (the sampling transform) before the crop rewrites
        // it, plus the colour to bake under it.
        let background_fill = fill_background.and_then(|color| {
            let layer = self
                .layers
                .0
                .iter()
                .find(|l| l.is_background == Some(true))?;
            let (width, height) = (layer.width?, layer.height?);
            Some(crate::document::crop_bake::FillBakePlan {
                layer_id: layer.id.clone(),
                placement: crate::document::crop_bake::CropLayerPlacement {
                    x: layer.x,
                    y: layer.y,
                    scale_x: layer.scale_x,
                    scale_y: layer.scale_y,
                    rotation: layer.rotation,
                    flip_h: layer.flip_h.unwrap_or(false),
                    flip_v: layer.flip_v.unwrap_or(false),
                },
                width: width as u32,
                height: height as u32,
                color,
            })
        });

        let affected: Vec<String> = self.layers.0.iter().map(|l| l.id.clone()).collect();
        let _e = self.begin_forward("Crop Canvas", &affected);
        let mut i = 0;
        while i < self.layers.0.len() {
            let layer = self.layers.get(i).expect("layer present").clone();
            if layer.locked != Some(true) {
                // NOTE: layer dims are Option<f64> on the render layer (absent
                // for engine-minted layers); None is an unreachable-in-parity case
                // (the native shadow always seeds width/height), so 0.0 keeps the
                // center math finite rather than panicking on an unwrap.
                let lw = layer.width.unwrap_or(0.0);
                let lh = layer.height.unwrap_or(0.0);
                let lsx = layer.scale_x;
                let lsy = layer.scale_y;
                let lcx = layer.x + (lw * lsx.abs()) / 2.0;
                let lcy = layer.y + (lh * lsy.abs()) / 2.0;
                let vx = lcx - crop_center_x;
                let vy = lcy - crop_center_y;
                let rvx = vx * cos - vy * sin;
                let rvy = vx * sin + vy * cos;
                let nlcx = width / 2.0 + rvx;
                let nlcy = height / 2.0 + rvy;
                let final_cx = nlcx * export_scale_x;
                let final_cy = nlcy * export_scale_y;
                let final_scale_x = lsx * export_scale_x;
                let final_scale_y = lsy * export_scale_y;
                let final_rotation = normalize_rotation(layer.rotation - rot);
                let new_x = final_cx - (lw * final_scale_x.abs()) / 2.0;
                let new_y = final_cy - (lh * final_scale_y.abs()) / 2.0;
                let mut changed = layer;
                changed.x = new_x;
                changed.y = new_y;
                changed.scale_x = final_scale_x;
                changed.scale_y = final_scale_y;
                changed.rotation = final_rotation;
                // flips stay unchanged (non-destructive branch)
                changed.dirty_rect = Some(Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                });
                self.layers = self.layers.replaced(i, changed);
            }
            i += 1;
        }
        self.doc_size = Some((final_w, final_h));
        if let Some(sh) = &mut self.replica {
            sh.doc.width = final_w;
            sh.doc.height = final_h;
        }
        self.set_engine_selection(None);
        if let Some(fill) = &background_fill {
            // The baked Background is document-sized and axis aligned; its
            // pixels are replaced by the store, so the metadata must describe
            // that identity placement in the same entry.
            for index in 0..self.layers.0.len() {
                let mut layer = self.layers.get(index).expect("layer present").clone();
                if layer.id != fill.layer_id {
                    continue;
                }
                layer.x = 0.0;
                layer.y = 0.0;
                layer.scale_x = 1.0;
                layer.scale_y = 1.0;
                layer.rotation = 0.0;
                layer.flip_h = Some(false);
                layer.flip_v = Some(false);
                layer.width = Some(final_w);
                layer.height = Some(final_h);
                layer.dirty_rect = Some(Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                });
                self.layers = self.layers.replaced(index, layer);
            }
        }
        if delete_cropped_pixels || background_fill.is_some() {
            let layers: Vec<crate::document::crop_bake::CropBakeLayer> = if delete_cropped_pixels {
                self.layers
                    .0
                    .iter()
                    .filter(|l| l.locked != Some(true) && l.width.is_some() && l.height.is_some())
                    .map(|l| crate::document::crop_bake::CropBakeLayer {
                        layer_id: l.id.clone(),
                        placement: crate::document::crop_bake::CropLayerPlacement {
                            x: l.x,
                            y: l.y,
                            scale_x: l.scale_x,
                            scale_y: l.scale_y,
                            rotation: l.rotation,
                            flip_h: l.flip_h.unwrap_or(false),
                            flip_v: l.flip_v.unwrap_or(false),
                        },
                    })
                    .collect()
            } else {
                Vec::new()
            };
            if !layers.is_empty() || background_fill.is_some() {
                self.pending_crop_bake = Some(crate::document::crop_bake::CropBakePlan {
                    entry_index: _e,
                    doc_width: final_w,
                    doc_height: final_h,
                    layers,
                    fill: background_fill,
                });
            }
        }
        self.finish_forward(_e);
        Ok(self
            .layers
            .0
            .iter()
            .map(|l| RenderLayerChange::Upsert {
                layer: l.as_ref().clone(),
            })
            .collect())
    }

    /// Resize the canvas to (width, height). The non-layer effect of the reference
    /// resize: only the document size changes, no layer is touched.
    ///
    /// Trust boundary: reject non-finite dims with E_INVALID before mutation (the
    /// reference engine would proceed with NaN). Non-positive dims are a SILENT no-op (no
    /// entry; DV still bumps at the tail). The arm emits an EMPTY layer delta; its
    /// real effect (the document size) is not expressible as a layer delta, so
    /// consumers must re-read the snapshot (which now carries the dims). The entry
    /// records the document size so undo/redo restores it.
    pub(crate) fn apply_resize_canvas(
        &mut self,
        width: f64,
        height: f64,
    ) -> Result<Vec<RenderLayerChange>, ProtocolError> {
        if !width.is_finite() || !height.is_finite() {
            return Err(e_invalid("resizeCanvas requires finite width/height"));
        }
        // Silent no-op on non-positive dims: no entry, snapshot unchanged, DV +1.
        if width <= 0.0 || height <= 0.0 {
            return Ok(Vec::new());
        }
        let _e = self.begin_forward("Resize Canvas", &[]);
        self.doc_size = Some((width, height));
        if let Some(sh) = &mut self.replica {
            sh.doc.width = width;
            sh.doc.height = height;
        }
        self.finish_forward(_e);
        // No layer changed: empty delta. Document size rides the snapshot.
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests;
