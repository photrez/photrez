// SPDX-License-Identifier: AGPL-3.0-or-later
// Canvas-size command-arm bodies (Crop Canvas / Apply Crop / Resize Canvas).
// Extracted from document_core_apply.rs so that module stays under the 1000-line
// guard; this is a sibling `impl ProtocolEngine` block (method visibility is
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

/// Normalize a rotation angle in degrees to the canonical (-180, 180] range,
/// mirroring the TS `normalizeRotation` (apps/desktop/src/viewport/transformGeometry.ts):
/// `angle % 360`, then `> 180 -> -360`, then `< -180 -> +360`. Verified against the
/// TS semantics (e.g. -270 -> 90, 360 -> 0, 181 -> -179).
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

impl ProtocolEngine {
    /// Crop the canvas: shift every unlocked layer's position by (-x, -y) and set
    /// the document size to (width, height). Mirrors the TS `DocumentEngine.cropCanvas`
    /// / `performCropCanvas` non-destructive math, which offsets unlocked layers and
    /// resizes the document. Locked layers are untouched (oracle: `if (!layer.locked)`);
    /// `RenderLayer.locked` is `Option<bool>`, so `Some(true)` is locked and
    /// `None`/`false` is not.
    ///
    /// Trust boundary: reject non-finite inputs with E_INVALID before any mutation
    /// (the TS oracle lets NaN/Inf silently slip through its `<= 0` comparisons and
    /// would proceed (a stricter-than-host divergence). Non-positive width/height
    /// is a SILENT no-op (no history entry; the apply() tail still bumps DV),
    /// mirroring the oracle's `if (width <= 0 || height <= 0) return;`. x/y may be
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
        if let Some(sh) = &mut self.canonical {
            sh.doc.width = width;
            sh.doc.height = height;
        }
        // The oracle resets selection on crop; mirror it onto the shadow too.
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
    /// Mirrors the TS `DocumentEngine.applyCrop` / `performApplyCrop` non-destructive
    /// branch exclusively, the pixel-baking `deleteCroppedPixels` and
    /// `fillBackgroundColor` variants remain host-side and are NOT represented on
    /// this command. For each unlocked layer, recenters it into the crop region,
    /// rotates it around the crop center, scales it to the (optional) target
    /// dimensions, and re-derives its final transform. Locked layers are skipped;
    /// `flips` are preserved unchanged.
    ///
    /// Trust boundary: reject non-finite x/y/width/height/rotation with E_INVALID
    /// before any mutation (the TS oracle would proceed with NaN). A target size
    /// given as a half-pair (one of targetWidth/targetHeight without the other) is
    /// invalid (the oracle takes targetSize as a complete {w,h} pair). Non-positive
    /// width/height is a SILENT no-op (the oracle's first guard, before reading the
    /// target size).
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
        // Silent no-op on non-positive crop rect (oracle's first guard).
        if width <= 0.0 || height <= 0.0 {
            return Ok(Vec::new());
        }
        let final_w = target_width.unwrap_or(width);
        let final_h = target_height.unwrap_or(height);
        let crop_center_x = x + width / 2.0;
        let crop_center_y = y + height / 2.0;
        let rad = (-rot * std::f64::consts::PI) / 180.0;
        let cos = rad.cos();
        let sin = rad.sin();
        let export_scale_x = final_w / width;
        let export_scale_y = final_h / height;

        let affected: Vec<String> = self.layers.0.iter().map(|l| l.id.clone()).collect();
        let _e = self.begin_forward("Crop Canvas", &affected);
        let mut i = 0;
        while i < self.layers.0.len() {
            let layer = self.layers.get(i).expect("layer present").clone();
            if layer.locked != Some(true) {
                // NOTE: layer dims are Option<f64> on the render layer (absent
                // for engine-minted layers); None is an unreachable-in-parity case
                // (the wasm mirror always seeds width/height), so 0.0 keeps the
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
                // flips stay unchanged (oracle non-destructive branch)
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
        if let Some(sh) = &mut self.canonical {
            sh.doc.width = final_w;
            sh.doc.height = final_h;
        }
        self.set_engine_selection(None);
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

    /// Resize the canvas to (width, height). Mirrors the TS `DocumentEngine.resizeCanvas`
    /// non-layer effect: only the document size changes, no layer is touched.
    ///
    /// Trust boundary: reject non-finite dims with E_INVALID before mutation (the TS
    /// oracle would proceed with NaN). Non-positive dims are a SILENT no-op (no
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
        if let Some(sh) = &mut self.canonical {
            sh.doc.width = width;
            sh.doc.height = height;
        }
        self.finish_forward(_e);
        // No layer changed: empty delta. Document size rides the snapshot.
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod canvas_rotation_tests {
    use super::*;

    #[test]
    fn normalize_rotation_wraps_negative_dividend() {
        // -270 % 360 in JS/f64 == -270, then < -180 -> +360 == 90.
        assert_eq!(normalize_rotation(-270.0), 90.0);
    }
    #[test]
    fn normalize_rotation_full_turn_is_zero() {
        assert_eq!(normalize_rotation(360.0), 0.0);
    }
    #[test]
    fn normalize_rotation_passthrough_in_range() {
        assert_eq!(normalize_rotation(45.0), 45.0);
    }
    #[test]
    fn normalize_rotation_above_180_wraps_negative() {
        assert_eq!(normalize_rotation(181.0), -179.0);
    }
}
