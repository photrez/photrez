// SPDX-License-Identifier: AGPL-3.0-or-later
// Structural-command-arm bodies (duplicate / merge-down / merge-selected / flatten
// / rasterize). Kept separate from the apply dispatcher so both stay under the
// size cap; this is a sibling `impl DocumentCore` block (method
// visibility is per-impl, not per module).
//
// These arms are UNDOABLE graph transitions: each opens a forward native entry
// (begin_forward / finish_forward) exactly like the metadata arms, emits a delta of
// Removes (for every vanished id, with its resource_id) followed by an Upsert of
// EVERY remaining / new layer in final engine order. The delta is therefore a
// complete, ordered restatement of the surviving stack: any consumer that applies
// structural deltas must rebuild stacking from this upsert sequence (or refresh the
// full snapshot). The production in-place consumer (editorFacade applyDeltaToSnapshot)
// replaces found ids in place and tail-appends unknown ids, so it cannot reconstruct
// mid-stack inserts today; that consumer does NOT process these structural deltas
// (the host refreshes the full snapshot instead). The document version is bumped via
// the apply() tail.
use super::*;
use crate::document::duplicate_name::next_duplicate_name;
use crate::ptz::content::{BlendMode, LayerType};

/// Build an E_INVALID-style ProtocolError (mirrors the other arms' reject shape).
fn e_invalid(msg: &str) -> ProtocolError {
    ProtocolError {
        code: "E_INVALID".to_string(),
        message: msg.to_string(),
    }
}

/// Read the canonical-shadow document dims, or E_INVALID when no shadow is seeded.
/// The reference engine read the document's own width/height directly; the
/// native arm reads the
/// seeded canonical shadow instead (the engine keeps no doc dims of its own), so a
/// merge/flatten before the host seeds the shadow rejects rather than guessing.
fn canonical_dims(engine: &DocumentCore) -> Result<(f64, f64), ProtocolError> {
    match &engine.replica {
        Some(sh) => Ok((sh.doc.width, sh.doc.height)),
        None => Err(e_invalid(
            "structural merge/flatten requires a seeded canonical document (host must seed before merge/flatten)",
        )),
    }
}

impl DocumentCore {
    /// Duplicate a layer verbatim, inserting the clone directly above the source.
    /// The clone inherits every field (type/visible/opacity/blend/transform incl.
    /// flip, width/height, adjustments, shape/text params) but gets a fresh
    /// resource id (it owns an independent bitmap) and drops the background flag
    /// and all bottom/locks (mirrors duplicate_layer:260-266 / duplicateLayerNode).
    /// Name follows the numeric-suffix rule shared with the document graph mirror.
    pub(crate) fn apply_duplicate(
        &mut self,
        id: &str,
        new_id: &str,
    ) -> Result<Vec<RenderLayerChange>, ProtocolError> {
        // Host owns identity: a non-empty, unique new_id is required.
        if new_id.is_empty() {
            return Err(e_invalid("duplicateLayer newId must not be empty"));
        }
        if self.layers.position_by_id(new_id).is_some() {
            return Err(e_invalid(&format!(
                "duplicateLayer newId already present: {}",
                new_id
            )));
        }
        // Unknown source id is a silent no-op (same as DeleteLayer). The reference
        // engine's duplicate threw on an unknown id - a documented divergence.
        // The host owns identity checks before sending, so this is safe).
        let pos = match self.layers.position_by_id(id) {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };
        let src = self.layers.get(pos).expect("layer present").clone();
        let names: Vec<&str> = self.layers.0.iter().map(|l| l.name.as_str()).collect();
        let name = next_duplicate_name(&names, &src.name);
        let rid = self.next_resource;
        self.next_resource += 1;
        let mut nl = src;
        nl.id = new_id.to_string();
        nl.name = name;
        nl.resource_id = rid;
        nl.locked = Some(false);
        nl.is_background = None;
        nl.lock_position = None;
        nl.lock_rotation = None;
        nl.lock_transparency = None;
        let _e = self.begin_forward("Duplicate Layer", &[new_id.to_string()]);
        // Insert above the source (at the source index; index 0 = top of stack).
        self.layers = self.layers.insert_at(nl.clone(), pos);
        self.finish_forward(_e);
        // Delta: no removes + every layer upserted in final engine order.
        Ok(self
            .layers
            .0
            .iter()
            .map(|l| RenderLayerChange::Upsert {
                layer: l.as_ref().clone(),
            })
            .collect())
    }

    /// Merge a layer down into the one below it. The merged node is a raster layer
    /// named "top + bottom", blending with the bottom layer's blend mode, locked
    /// when either source is locked. Replaces the two sources at the pair's
    /// position. Unknown id or a bottom-most layer is a silent no-op.
    pub(crate) fn apply_merge_down(
        &mut self,
        id: &str,
        merged_id: &str,
    ) -> Result<Vec<RenderLayerChange>, ProtocolError> {
        if merged_id.is_empty() {
            return Err(e_invalid("mergeDown mergedId must not be empty"));
        }
        if self.layers.position_by_id(merged_id).is_some() {
            return Err(e_invalid(&format!(
                "mergeDown mergedId already present: {}",
                merged_id
            )));
        }
        let pos = match self.layers.position_by_id(id) {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };
        let bottom_pos = pos + 1;
        if bottom_pos >= self.layers.0.len() {
            // Nothing below: cannot merge (index guard).
            return Ok(Vec::new());
        }
        let top = self.layers.get(pos).expect("layer present").clone();
        let bottom = self.layers.get(bottom_pos).expect("layer present").clone();
        let name = format!("{} + {}", top.name, bottom.name);
        let locked = bottom.locked == Some(true) || top.locked == Some(true);
        let blend_mode = bottom.blend_mode.clone().unwrap_or(BlendMode::Normal);
        let (w, h) = canonical_dims(self)?;
        let rid = self.next_resource;
        self.next_resource += 1;
        let merged = RenderLayer {
            id: merged_id.to_string(),
            name,
            visible: true,
            opacity: 1.0,
            resource_id: rid,
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
            blend_mode: Some(blend_mode),
            locked: Some(locked),
            lock_transparency: None,
            lock_position: None,
            lock_rotation: None,
            is_background: None,
            has_adjustments: None,
            width: Some(w),
            height: Some(h),
            flip_h: None,
            flip_v: None,
            shape_params: None,
            text_data: None,
            basic_adjustment: None,
        };
        let affected = vec![top.id.clone(), bottom.id.clone(), merged_id.to_string()];
        let _e = self.begin_forward("Merge Down", &affected);
        // Remove the pair (bottom first, then top - indices are stable because the
        // bottom sits above top's index only relative to itself) and insert the
        // merged node back at the pair's original position.
        let set = self
            .layers
            .removed(bottom_pos)
            .removed(pos)
            .insert_at(merged.clone(), pos);
        self.layers = set;
        self.finish_forward(_e);
        let mut changes = vec![
            RenderLayerChange::Remove {
                id: top.id.clone(),
                resource_id: top.resource_id,
            },
            RenderLayerChange::Remove {
                id: bottom.id.clone(),
                resource_id: bottom.resource_id,
            },
        ];
        for l in self.layers.0.iter() {
            changes.push(RenderLayerChange::Upsert {
                layer: l.as_ref().clone(),
            });
        }
        Ok(changes)
    }

    /// Merge multiple selected layers into one raster node at the highest stack
    /// position (first occurrence index) among them. Blend mode is always Normal;
    /// locked when ANY selected layer is locked. Fewer than two matched ids is a
    /// silent no-op (only ids present in the engine count).
    pub(crate) fn apply_merge_selected(
        &mut self,
        ids: &[String],
        merged_id: &str,
    ) -> Result<Vec<RenderLayerChange>, ProtocolError> {
        if merged_id.is_empty() {
            return Err(e_invalid("mergeSelected mergedId must not be empty"));
        }
        if self.layers.position_by_id(merged_id).is_some() {
            return Err(e_invalid(&format!(
                "mergeSelected mergedId already present: {}",
                merged_id
            )));
        }
        if ids.len() < 2 {
            return Ok(Vec::new());
        }
        // selected = engine layers with id in ids, IN STACK ORDER (top first).
        let mut selected: Vec<RenderLayer> = Vec::new();
        for l in self.layers.0.iter() {
            if ids.contains(&l.id) {
                selected.push(l.as_ref().clone());
            }
        }
        if selected.len() < 2 {
            return Ok(Vec::new());
        }
        let name = if selected.len() == 2 {
            format!("{} + {}", selected[0].name, selected[1].name)
        } else {
            format!("{} (+{} merged)", selected[0].name, selected.len() - 1)
        };
        let locked = selected.iter().any(|l| l.locked == Some(true));
        let (w, h) = canonical_dims(self)?;
        let rid = self.next_resource;
        self.next_resource += 1;
        let merged = RenderLayer {
            id: merged_id.to_string(),
            name,
            visible: true,
            opacity: 1.0,
            resource_id: rid,
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
            locked: Some(locked),
            lock_transparency: None,
            lock_position: None,
            lock_rotation: None,
            is_background: None,
            has_adjustments: None,
            width: Some(w),
            height: Some(h),
            flip_h: None,
            flip_v: None,
            shape_params: None,
            text_data: None,
            basic_adjustment: None,
        };
        // Highest stack position (first occurrence index) among the selected.
        let highest_pos = selected
            .iter()
            .filter_map(|sel| self.layers.position_by_id(&sel.id))
            .min()
            .expect("selected layer present in engine");
        let mut affected: Vec<String> = selected.iter().map(|s| s.id.clone()).collect();
        affected.push(merged_id.to_string());
        let _e = self.begin_forward("Merge Selected Layers", &affected);
        // Remove all selected ids in descending index order (keeps indices stable),
        // then insert the merged node at the highest selected position.
        let mut positions: Vec<usize> = selected
            .iter()
            .filter_map(|s| self.layers.position_by_id(&s.id))
            .collect();
        positions.sort_unstable_by(|a, b| b.cmp(a));
        let mut set = self.layers.clone();
        for p in &positions {
            set = set.removed(*p);
        }
        set = set.insert_at(merged.clone(), highest_pos);
        self.layers = set;
        self.finish_forward(_e);
        let mut changes = Vec::new();
        for sel in &selected {
            changes.push(RenderLayerChange::Remove {
                id: sel.id.clone(),
                resource_id: sel.resource_id,
            });
        }
        for l in self.layers.0.iter() {
            changes.push(RenderLayerChange::Upsert {
                layer: l.as_ref().clone(),
            });
        }
        Ok(changes)
    }

    /// Flatten every layer into a single Background node. The node is a raster
    /// layer named "Background", not locked, but carrying the background flag and
    /// position/rotation locks the real Background layers carry. A single-layer
    /// document is a silent no-op.
    pub(crate) fn apply_flatten(
        &mut self,
        merged_id: &str,
    ) -> Result<Vec<RenderLayerChange>, ProtocolError> {
        if merged_id.is_empty() {
            return Err(e_invalid("flatten mergedId must not be empty"));
        }
        if self.layers.position_by_id(merged_id).is_some() {
            return Err(e_invalid(&format!(
                "flatten mergedId already present: {}",
                merged_id
            )));
        }
        if self.layers.0.len() <= 1 {
            return Ok(Vec::new());
        }
        let (w, h) = canonical_dims(self)?;
        let rid = self.next_resource;
        self.next_resource += 1;
        let merged = RenderLayer {
            id: merged_id.to_string(),
            name: "Background".to_string(),
            visible: true,
            opacity: 1.0,
            resource_id: rid,
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
            // flatten rule: locked stays false; the bg flags carry
            // the locks.
            locked: Some(false),
            lock_transparency: None,
            lock_position: Some(true),
            lock_rotation: Some(true),
            is_background: Some(true),
            has_adjustments: None,
            width: Some(w),
            height: Some(h),
            flip_h: None,
            flip_v: None,
            shape_params: None,
            text_data: None,
            basic_adjustment: None,
        };
        let old_ids: Vec<(String, ResourceId)> = self
            .layers
            .0
            .iter()
            .map(|l| (l.id.clone(), l.resource_id))
            .collect();
        let affected: Vec<String> = old_ids
            .iter()
            .map(|(id, _)| id.clone())
            .chain(std::iter::once(merged_id.to_string()))
            .collect();
        let _e = self.begin_forward("Flatten Image", &affected);
        // Replace the entire layer set with the single merged node.
        self.layers = LayerSet::from_layers(vec![merged.clone()]);
        self.finish_forward(_e);
        let mut changes: Vec<RenderLayerChange> = Vec::new();
        for (id, rid2) in &old_ids {
            changes.push(RenderLayerChange::Remove {
                id: id.clone(),
                resource_id: *rid2,
            });
        }
        changes.push(RenderLayerChange::Upsert { layer: merged });
        Ok(changes)
    }

    /// Rasterize a parametric (shape/text) layer to a plain raster layer: drop the
    /// shape/text params, keep bitmap/dims/transform/adjustments intact. A
    /// non-parametric layer is a silent no-op (mirrors shapeLayerToRaster /
    /// textLayerToRaster's type guard, which returns silently).
    ///
    /// `pixels` carries the host-rendered bitmap that materializes the layer's
    /// canonical buffer (parametric layers are metadata-only — "typed add;
    /// rasterization stays host-side"). With it, the layer's dimensions become
    /// the bitmap's, matching the rule that a text layer always carries its
    /// bitmap at bitmap-sized dims; the buffer itself is drained by
    /// `DocumentPixelStore` through `pending_rasterize`. Without it the flip is
    /// metadata-only (the historical behaviour that left the layer
    /// un-compositable — the documented rasterize defect).
    pub(crate) fn apply_rasterize(
        &mut self,
        id: &str,
        pixels: Option<&crate::wire::command::RasterizePixels>,
    ) -> Result<Vec<RenderLayerChange>, ProtocolError> {
        let pos = match self.layers.position_by_id(id) {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };
        // Capture the original parametric kind before overwriting.
        let kind = {
            let l = self.layers.get(pos).expect("layer present");
            match &l.layer_type {
                Some(LayerType::Shape) | Some(LayerType::Text) => l.layer_type.clone().unwrap(),
                _ => return Ok(Vec::new()),
            }
        };
        let material = match pixels {
            Some(pixels) => {
                let width = pixels.width.max(0.0) as u32;
                let height = pixels.height.max(0.0) as u32;
                let expected = (width as usize)
                    .checked_mul(height as usize)
                    .and_then(|len| len.checked_mul(4))
                    .ok_or_else(|| ProtocolError {
                        code: "E_INVALID".to_string(),
                        message: "rasterize pixel buffer overflows".to_string(),
                    })?;
                if width == 0
                    || height == 0
                    || !pixels.width.is_finite()
                    || !pixels.height.is_finite()
                {
                    return Err(ProtocolError {
                        code: "E_INVALID".to_string(),
                        message: "rasterize pixel dimensions must be positive and finite"
                            .to_string(),
                    });
                }
                if pixels.rgba.len() != expected {
                    return Err(ProtocolError {
                        code: "E_INVALID".to_string(),
                        message: format!(
                            "rasterize pixel buffer is {} bytes, expected {expected}",
                            pixels.rgba.len()
                        ),
                    });
                }
                Some((width, height, pixels.rgba.clone()))
            }
            None => None,
        };
        let _e = self.begin_forward("Rasterize Layer", &[id.to_string()]);
        let mut nl = self.layers.get(pos).expect("layer present").clone();
        nl.layer_type = Some(LayerType::Raster);
        match kind {
            LayerType::Shape => nl.shape_params = None,
            LayerType::Text => nl.text_data = None,
            _ => {}
        }
        if let Some((width, height, rgba)) = &material {
            // The materialized bitmap defines the layer frame (text layers
            // always carry bitmap-sized dims).
            nl.width = Some(f64::from(*width));
            nl.height = Some(f64::from(*height));
            self.pending_rasterize = Some(crate::document::rasterize_bake::RasterizePlan {
                layer_id: id.to_string(),
                width: *width,
                height: *height,
                rgba: rgba.clone(),
            });
        }
        // basic_adjustment / has_adjustments, bitmap dims (without a
        // materialized bitmap), and transform are kept.
        nl.dirty_rect = Some(Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        });
        self.layers = self.layers.replaced(pos, nl.clone());
        self.finish_forward(_e);
        Ok(vec![RenderLayerChange::Upsert { layer: nl }])
    }
}
