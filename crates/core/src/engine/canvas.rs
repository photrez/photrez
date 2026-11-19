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
use crate::document::content::{BlendMode, LayerType, Transform2D};

const MAX_CROP_OUTPUT: f64 = 16_384.0;

/// Build an E_INVALID-style ProtocolError (mirrors the other arms' reject shape).
fn e_invalid(msg: &str) -> ProtocolError {
    ProtocolError {
        code: "E_INVALID".to_string(),
        message: msg.to_string(),
    }
}

/// Normalize a rotation angle in degrees to the canonical (-180, 180] range:
/// `angle % 360`, then `> 180 -> -360`, then `< -180 -> +360`. Checked against
/// the worked examples -270 -> 90, 360 -> 0, 181 -> -179.
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

/// Geometry shared by `ApplyCrop` and its derived preview.
///
/// The rectangle and output dimensions must be finite and positive. Rotation
/// and mapped points must be finite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropTransform {
    /// Crop rectangle in document coordinates.
    pub rect: [f64; 4],
    /// Signed rotation passed to `ApplyCrop`.
    pub rotation: f64,
    /// Final output dimensions.
    pub output_size: (f64, f64),
}

impl CropTransform {
    /// Map a document point into the cropped output.
    pub fn map_point(self, point: (f64, f64)) -> (f64, f64) {
        let [x, y, width, height] = self.rect;
        let crop_center_x = x + width / 2.0;
        let crop_center_y = y + height / 2.0;
        let radians = (-self.rotation * std::f64::consts::PI) / 180.0;
        let cos = radians.cos();
        let sin = radians.sin();
        let dx = point.0 - crop_center_x;
        let dy = point.1 - crop_center_y;
        let scale_x = self.output_size.0 / width;
        let scale_y = self.output_size.1 / height;
        let rotated_x = dx * cos - dy * sin;
        let rotated_y = dx * sin + dy * cos;

        (
            (width / 2.0 + rotated_x) * scale_x,
            (height / 2.0 + rotated_y) * scale_y,
        )
    }
}

/// Apply the same geometry that `ApplyCrop` uses for layer placement.
pub fn crop_layer_transform(
    transform: &Transform2D,
    layer_size: (f64, f64),
    crop: CropTransform,
) -> Transform2D {
    let (layer_width, layer_height) = layer_size;
    let layer_center_x = transform.x + (layer_width * transform.scale_x.abs()) / 2.0;
    let layer_center_y = transform.y + (layer_height * transform.scale_y.abs()) / 2.0;
    let (final_center_x, final_center_y) = crop.map_point((layer_center_x, layer_center_y));
    let final_scale_x = transform.scale_x * (crop.output_size.0 / crop.rect[2]);
    let final_scale_y = transform.scale_y * (crop.output_size.1 / crop.rect[3]);

    Transform2D {
        x: final_center_x - (layer_width * final_scale_x.abs()) / 2.0,
        y: final_center_y - (layer_height * final_scale_y.abs()) / 2.0,
        scale_x: final_scale_x,
        scale_y: final_scale_y,
        rotation: normalize_rotation(transform.rotation - crop.rotation),
        flip_h: transform.flip_h,
        flip_v: transform.flip_v,
    }
}

impl DocumentCore {
    /// Crop the canvas: shift every unlocked layer's position by (-x, -y) and set
    /// the document size to (width, height). The crop is non-destructive: layer
    /// positions move, layer pixels do not. Locked layers are untouched (the
    /// guard is `!layer.locked`); `LayerMeta.locked` is `Option<bool>`, so
    /// `Some(true)` is locked and `None`/`false` is not.
    ///
    /// Trust boundary: reject non-finite inputs with E_INVALID before any
    /// mutation. NaN and Inf pass `<= 0` comparisons, so only this check rejects
    /// them. The `width <= 0 || height <= 0` guard makes a non-positive size a
    /// SILENT no-op (no history entry; the apply() tail still bumps DV). x/y
    /// may be any sign (negative offsets are legal).
    pub(crate) fn apply_crop_canvas(
        &mut self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Result<Vec<LayerChange>, ProtocolError> {
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
            .map(|l| LayerChange::Upsert {
                layer: l.as_ref().clone(),
            })
            .collect())
    }

    pub(crate) fn prepare_crop_bake(
        &self,
        envelope: &crate::wire::command::CommandEnvelope,
    ) -> Option<crate::engine::crop_bake::CropBakePlan> {
        if envelope.contract_version != crate::wire::command::CONTRACT_VERSION
            || envelope
                .expected_version
                .is_some_and(|expected| expected != self.version())
            || self.pending_external.is_some()
        {
            return None;
        }
        let crate::wire::command::Command::ApplyCrop {
            x,
            y,
            width,
            height,
            rotation,
            target_width,
            target_height,
            delete_cropped_pixels,
            fill_background,
        } = &envelope.command
        else {
            return None;
        };
        if !*delete_cropped_pixels && fill_background.is_none() {
            return None;
        }
        let rotation = rotation.unwrap_or(0.0);
        let output_width = target_width.unwrap_or(*width);
        let output_height = target_height.unwrap_or(*height);
        if ![*x, *y, *width, *height, rotation]
            .iter()
            .all(|value| value.is_finite())
            || *width <= 0.0
            || *height <= 0.0
            || target_width.is_some() != target_height.is_some()
            || target_width.is_some_and(|value| !value.is_finite())
            || target_height.is_some_and(|value| !value.is_finite())
            || output_width <= 0.0
            || output_height <= 0.0
            || output_width > MAX_CROP_OUTPUT
            || output_height > MAX_CROP_OUTPUT
        {
            return None;
        }

        let crop_transform = CropTransform {
            rect: [*x, *y, *width, *height],
            rotation,
            output_size: (output_width, output_height),
        };
        if *delete_cropped_pixels
            && self.layers.iter().any(|layer| {
                if layer.locked == Some(true)
                    || layer.layer_type.as_ref() != Some(&LayerType::Raster)
                {
                    return false;
                }
                let (Some(layer_width), Some(layer_height)) = (layer.width, layer.height) else {
                    return false;
                };
                let transformed = crop_layer_transform(
                    &Transform2D {
                        x: layer.x,
                        y: layer.y,
                        scale_x: layer.scale_x,
                        scale_y: layer.scale_y,
                        rotation: layer.rotation,
                        flip_h: layer.flip_h.unwrap_or(false),
                        flip_v: layer.flip_v.unwrap_or(false),
                    },
                    (layer_width, layer_height),
                    crop_transform,
                );
                ![
                    transformed.x,
                    transformed.y,
                    transformed.scale_x,
                    transformed.scale_y,
                    transformed.rotation,
                ]
                .iter()
                .all(|value| value.is_finite())
            })
        {
            return None;
        }

        let (_, new_background_id, background_id) = self.crop_background_ids(*fill_background);
        self.build_crop_bake_plan(
            crop_transform,
            *delete_cropped_pixels,
            *fill_background,
            background_id,
            new_background_id,
        )
    }

    fn crop_background_ids(
        &self,
        fill_background: Option<[u8; 4]>,
    ) -> (Option<String>, Option<String>, Option<String>) {
        let existing = fill_background.and_then(|_| {
            self.layers
                .iter()
                .find(|layer| layer.is_background == Some(true))
                .map(|layer| layer.id.clone())
        });
        let created = if fill_background.is_some() && existing.is_none() {
            let prefix = format!("crop-fill-background-{}", self.next_resource);
            let mut candidate = prefix.clone();
            let mut suffix = 1;
            while self.layers.position_by_id(&candidate).is_some() {
                candidate = format!("{prefix}-{suffix}");
                suffix += 1;
            }
            Some(candidate)
        } else {
            None
        };
        let selected = existing.clone().or_else(|| created.clone());
        (existing, created, selected)
    }

    fn build_crop_bake_plan(
        &self,
        crop_transform: CropTransform,
        delete_cropped_pixels: bool,
        fill_background: Option<[u8; 4]>,
        background_id: Option<String>,
        new_background_id: Option<String>,
    ) -> Option<crate::engine::crop_bake::CropBakePlan> {
        let placement = |layer: &crate::document::layer::LayerMeta| {
            let transform = if layer.locked == Some(true) {
                Transform2D {
                    x: layer.x,
                    y: layer.y,
                    scale_x: layer.scale_x,
                    scale_y: layer.scale_y,
                    rotation: layer.rotation,
                    flip_h: layer.flip_h.unwrap_or(false),
                    flip_v: layer.flip_v.unwrap_or(false),
                }
            } else {
                crop_layer_transform(
                    &Transform2D {
                        x: layer.x,
                        y: layer.y,
                        scale_x: layer.scale_x,
                        scale_y: layer.scale_y,
                        rotation: layer.rotation,
                        flip_h: layer.flip_h.unwrap_or(false),
                        flip_v: layer.flip_v.unwrap_or(false),
                    },
                    (layer.width.unwrap_or(0.0), layer.height.unwrap_or(0.0)),
                    crop_transform,
                )
            };
            crate::engine::crop_bake::CropLayerPlacement {
                x: transform.x,
                y: transform.y,
                scale_x: transform.scale_x,
                scale_y: transform.scale_y,
                rotation: transform.rotation,
                flip_h: transform.flip_h,
                flip_v: transform.flip_v,
            }
        };
        let fill = match (fill_background, background_id) {
            (Some(color), Some(layer_id)) => {
                let create_background = new_background_id.as_deref() == Some(layer_id.as_str());
                if create_background {
                    Some(crate::engine::crop_bake::FillBakePlan {
                        layer_id,
                        create_background: true,
                        placement: crate::engine::crop_bake::CropLayerPlacement {
                            x: 0.0,
                            y: 0.0,
                            scale_x: 1.0,
                            scale_y: 1.0,
                            rotation: 0.0,
                            flip_h: false,
                            flip_v: false,
                        },
                        width: 0,
                        height: 0,
                        adjustment: None,
                        visible: true,
                        opacity: 1.0,
                        blend_mode: Some(BlendMode::Normal),
                        color,
                    })
                } else {
                    self.layers
                        .iter()
                        .find(|layer| layer.id == layer_id)
                        .and_then(|layer| {
                            Some(crate::engine::crop_bake::FillBakePlan {
                                layer_id,
                                create_background: false,
                                placement: placement(layer),
                                width: layer.width? as u32,
                                height: layer.height? as u32,
                                adjustment: layer.basic_adjustment.clone(),
                                visible: layer.visible,
                                opacity: layer.opacity,
                                blend_mode: layer.blend_mode.clone(),
                                color,
                            })
                        })
                }
            }
            _ => None,
        };
        let layers = if delete_cropped_pixels {
            self.layers
                .iter()
                .filter(|layer| {
                    layer.locked != Some(true)
                        && layer.layer_type.as_ref() == Some(&LayerType::Raster)
                        && layer.width.is_some()
                        && layer.height.is_some()
                        && fill.as_ref().is_none_or(|fill| fill.layer_id != layer.id)
                })
                .map(|layer| crate::engine::crop_bake::CropBakeLayer {
                    layer_id: layer.id.clone(),
                    placement: placement(layer),
                    adjustment: layer.basic_adjustment.clone(),
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if layers.is_empty() && fill.is_none() {
            return None;
        }
        Some(crate::engine::crop_bake::CropBakePlan {
            entry_seq: 0,
            doc_width: crop_transform.output_size.0,
            doc_height: crop_transform.output_size.1,
            layers,
            fill,
        })
    }

    /// Apply a crop with optional rotation, target size, pixel deletion, and fill.
    /// Destructive raster layers use identity metadata after the pixel store bakes
    /// their post-crop transform into the output-sized bitmap.
    /// For each unlocked layer, recenters it into the crop region,
    /// rotates it around the crop center, scales it to the (optional) target
    /// dimensions, and re-derives its final transform. Locked layers are skipped;
    /// `flips` are preserved unchanged.
    ///
    /// Trust boundary: reject non-finite x/y/width/height/rotation with E_INVALID
    /// before any mutation, so no NaN reaches the layer transforms. A target size
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
    ) -> Result<Vec<LayerChange>, ProtocolError> {
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
        if target_width.is_some_and(|value| !value.is_finite())
            || target_height.is_some_and(|value| !value.is_finite())
        {
            return Err(e_invalid(
                "applyCrop requires finite targetWidth and targetHeight",
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
        // apply_apply_crop can never collapse the document size to 0x0.
        if final_w <= 0.0 || final_h <= 0.0 {
            return Ok(Vec::new());
        }
        if final_w > MAX_CROP_OUTPUT || final_h > MAX_CROP_OUTPUT {
            return Err(e_invalid(
                "applyCrop output dimensions must not exceed 16384 pixels",
            ));
        }
        let crop_transform = CropTransform {
            rect: [x, y, width, height],
            rotation: rot,
            output_size: (final_w, final_h),
        };
        if delete_cropped_pixels {
            for layer in self.layers.iter() {
                if layer.locked == Some(true)
                    || layer.layer_type.as_ref() != Some(&LayerType::Raster)
                {
                    continue;
                }
                let (Some(layer_width), Some(layer_height)) = (layer.width, layer.height) else {
                    continue;
                };
                let transformed = crop_layer_transform(
                    &Transform2D {
                        x: layer.x,
                        y: layer.y,
                        scale_x: layer.scale_x,
                        scale_y: layer.scale_y,
                        rotation: layer.rotation,
                        flip_h: layer.flip_h.unwrap_or(false),
                        flip_v: layer.flip_v.unwrap_or(false),
                    },
                    (layer_width, layer_height),
                    crop_transform,
                );
                if ![
                    transformed.x,
                    transformed.y,
                    transformed.scale_x,
                    transformed.scale_y,
                    transformed.rotation,
                ]
                .iter()
                .all(|value| value.is_finite())
                {
                    return Err(e_invalid(
                        "applyCrop produced a non-finite raster transform",
                    ));
                }
            }
        }

        // Existing Background content is sampled after the crop transform. If
        // no Background exists, the fill creates one at the end of layer order.
        let (_, new_background_id, background_id) = self.crop_background_ids(fill_background);
        let mut planned_bake = self.build_crop_bake_plan(
            crop_transform,
            delete_cropped_pixels,
            fill_background,
            background_id,
            new_background_id.clone(),
        );

        let mut affected: Vec<String> = self.layers.0.iter().map(|l| l.id.clone()).collect();
        if let Some(id) = &new_background_id {
            affected.push(id.clone());
        }
        let _e = self.begin_forward("Crop Canvas", &affected);
        let mut i = 0;
        while i < self.layers.0.len() {
            let Some(layer) = self.layers.get(i).cloned() else {
                i += 1;
                continue;
            };
            if layer.locked != Some(true) {
                // NOTE: layer dims are Option<f64> on the render layer (absent
                // for engine-minted layers); None is unreachable in this engine
                // (the native shadow always seeds width/height), so 0.0 keeps the
                // center math finite rather than panicking on an unwrap.
                let lw = layer.width.unwrap_or(0.0);
                let lh = layer.height.unwrap_or(0.0);
                let lsx = layer.scale_x;
                let lsy = layer.scale_y;
                let transformed = crop_layer_transform(
                    &Transform2D {
                        x: layer.x,
                        y: layer.y,
                        scale_x: lsx,
                        scale_y: lsy,
                        rotation: layer.rotation,
                        flip_h: layer.flip_h.unwrap_or(false),
                        flip_v: layer.flip_v.unwrap_or(false),
                    },
                    (lw, lh),
                    crop_transform,
                );
                let mut changed = layer;
                changed.x = transformed.x;
                changed.y = transformed.y;
                changed.scale_x = transformed.scale_x;
                changed.scale_y = transformed.scale_y;
                changed.rotation = transformed.rotation;
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
        if let Some(id) = &new_background_id {
            let resource_id = self.next_resource;
            self.next_resource += 1;
            self.layers = self.layers.pushed(LayerMeta {
                id: id.clone(),
                name: "Background".to_string(),
                visible: true,
                opacity: 1.0,
                resource_id,
                x: 0.0,
                y: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation: 0.0,
                dirty_rect: Some(Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                }),
                layer_type: Some(LayerType::Raster),
                blend_mode: Some(BlendMode::Normal),
                locked: Some(false),
                lock_transparency: None,
                lock_position: Some(true),
                lock_rotation: Some(true),
                is_background: Some(true),
                has_adjustments: Some(false),
                width: Some(final_w),
                height: Some(final_h),
                flip_h: Some(false),
                flip_v: Some(false),
                shape_params: None,
                text_data: None,
                basic_adjustment: None,
            });
        }
        let background_fill = planned_bake.as_ref().and_then(|plan| plan.fill.as_ref());
        if let Some(fill) = background_fill {
            for index in 0..self.layers.0.len() {
                let Some(mut layer) = self.layers.get(index).cloned() else {
                    continue;
                };
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
                layer.basic_adjustment = None;
                layer.has_adjustments = Some(false);
                layer.dirty_rect = Some(Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                });
                self.layers = self.layers.replaced(index, layer);
            }
        }
        if let Some(plan) = &planned_bake {
            for bake in &plan.layers {
                let Some(index) = self.layers.position_by_id(&bake.layer_id) else {
                    continue;
                };
                let Some(mut layer) = self.layers.get(index).cloned() else {
                    continue;
                };
                layer.x = 0.0;
                layer.y = 0.0;
                layer.scale_x = 1.0;
                layer.scale_y = 1.0;
                layer.rotation = 0.0;
                layer.flip_h = Some(false);
                layer.flip_v = Some(false);
                layer.width = Some(final_w);
                layer.height = Some(final_h);
                layer.basic_adjustment = None;
                layer.has_adjustments = Some(false);
                layer.dirty_rect = Some(Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                });
                self.layers = self.layers.replaced(index, layer);
            }
        }
        if let Some(mut plan) = planned_bake.take() {
            plan.entry_seq = self.entries[_e].seq;
            self.pending_crop_bake = Some(plan);
        }
        self.finish_forward(_e);
        Ok(self
            .layers
            .0
            .iter()
            .map(|l| LayerChange::Upsert {
                layer: l.as_ref().clone(),
            })
            .collect())
    }

    /// Resize the canvas to (width, height). The effect is non-layer: only the
    /// document size changes, no layer is touched.
    ///
    /// Trust boundary: reject non-finite dims with E_INVALID before mutation,
    /// since NaN passes a `<= 0` test. Non-positive dims are a SILENT no-op (no
    /// entry; DV still bumps at the tail). The arm emits an EMPTY layer delta; its
    /// real effect (the document size) is not expressible as a layer delta, so
    /// consumers must re-read the snapshot (which now carries the dims). The entry
    /// records the document size so undo/redo restores it.
    pub(crate) fn apply_resize_canvas(
        &mut self,
        width: f64,
        height: f64,
    ) -> Result<Vec<LayerChange>, ProtocolError> {
        if !width.is_finite() || !height.is_finite() {
            return Err(e_invalid("Resizing requires finite width and height"));
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
