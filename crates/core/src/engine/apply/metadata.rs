// SPDX-License-Identifier: AGPL-3.0-or-later
// Metadata command arms for DocumentCore::apply. The dispatcher in
// document/apply.rs only selects the family; the bodies live here.

use super::*;
use crate::document::content::BasicAdjustment;

impl DocumentCore {
    pub(super) fn apply_metadata_arm(
        &mut self,
        command: Command,
    ) -> Result<Vec<LayerChange>, ProtocolError> {
        match command {
            // ── Metadata arms ──
            // Unknown id is ignored: no error, no history entry.
            Command::SetVisible { id, visible } => {
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Set Visible", std::slice::from_ref(&id));
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    layer.visible = visible;
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    Ok(vec![LayerChange::Upsert { layer }])
                } else {
                    Ok(Vec::new())
                }
            }
            Command::SetLocked { id, kind, locked } => {
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Set Lock", std::slice::from_ref(&id));
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    match kind {
                        LockKind::Base => layer.locked = Some(locked),
                        LockKind::Transparency => layer.lock_transparency = Some(locked),
                        LockKind::Position => layer.lock_position = Some(locked),
                        LockKind::Rotation => layer.lock_rotation = Some(locked),
                    }
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    Ok(vec![LayerChange::Upsert { layer }])
                } else {
                    Ok(Vec::new())
                }
            }
            Command::Rename { id, name } => {
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Rename", std::slice::from_ref(&id));
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    layer.name = name.clone();
                    if layer.is_background == Some(true) {
                        // Explicit false, not None: the replica merge treats
                        // None as "no opinion" and would keep the old flags.
                        layer.is_background = Some(false);
                        layer.lock_position = Some(false);
                        layer.lock_rotation = Some(false);
                    }
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    Ok(vec![LayerChange::Upsert { layer }])
                } else {
                    Ok(Vec::new())
                }
            }
            Command::Reorder { id, to } => {
                // Reject an out-of-range target before any mutation, history entry,
                // or DV bump - the contract of this arm's reorder guard. The
                // index is host-supplied and absolute; only [0, len) is valid.
                if to >= self.layers.0.len() {
                    return Err(ProtocolError {
                        code: "E_INVALID".to_string(),
                        message: format!(
                            "reorder target index {} out of range [0, {})",
                            to,
                            self.layers.0.len()
                        ),
                    });
                }
                if let Some(pos) = self.layers.position_by_id(&id) {
                    // The Background is pinned to the bottom
                    // and is never reordered (a layer beneath it would be unreachable).
                    if self
                        .layers
                        .get(pos)
                        .map(|l| l.is_background == Some(true))
                        .unwrap_or(false)
                    {
                        Ok(Vec::new())
                    } else {
                        let _e = self.begin_forward("Reorder", std::slice::from_ref(&id));
                        let layer = self.layers.get(pos).expect("layer present").clone();
                        // Remove at the current index, then insert at the clamped target
                        // (LayerSet::insert_at clamps the index to len).
                        let removed = self.layers.removed(pos);
                        let clamped = to.min(removed.0.len());
                        self.layers = removed.insert_at(layer.clone(), clamped);
                        // Re-seat the Background at the bottom if the move pushed it off.
                        if let Some(bg_pos) = self
                            .layers
                            .0
                            .iter()
                            .position(|l| l.is_background == Some(true))
                        {
                            if bg_pos != self.layers.0.len() - 1 {
                                let bg = self.layers.get(bg_pos).expect("bg present").clone();
                                let without = self.layers.removed(bg_pos);
                                self.layers = without.insert_at(bg.clone(), without.0.len());
                            }
                        }
                        self.finish_forward(_e);
                        // Emit every layer as an Upsert in engine order so the host
                        // reconciles the new stacking from a single delta.
                        Ok(self
                            .layers
                            .0
                            .iter()
                            .map(|l| LayerChange::Upsert {
                                layer: l.as_ref().clone(),
                            })
                            .collect())
                    }
                } else {
                    Ok(Vec::new())
                }
            }
            Command::SetBackgroundFlag { id } => {
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Set Background Flag", std::slice::from_ref(&id));
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    // Marking a layer as Background sets the flag and locks position/rotation.
                    layer.is_background = Some(true);
                    layer.lock_position = Some(true);
                    layer.lock_rotation = Some(true);
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    Ok(vec![LayerChange::Upsert { layer }])
                } else {
                    Ok(Vec::new())
                }
            }
            Command::SetBlendMode { id, mode } => {
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Set Blend Mode", std::slice::from_ref(&id));
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    layer.blend_mode = Some(mode);
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    Ok(vec![LayerChange::Upsert { layer }])
                } else {
                    Ok(Vec::new())
                }
            }
            Command::SetLayerParams {
                id,
                shape_params,
                text_data,
            } => {
                // Both None is an invalid no-op (a params update always
                // carries the payload). Reject before any mutation so no history
                // entry / DV bump is produced for a meaningless command.
                if shape_params.is_none() && text_data.is_none() {
                    return Err(ProtocolError {
                        code: "E_INVALID".to_string(),
                        message: "setting layer parameters requires shapeParams or textData"
                            .to_string(),
                    });
                }
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Set Layer Params", std::slice::from_ref(&id));
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    if let Some(p) = shape_params.clone() {
                        layer.shape_params = Some(p);
                    }
                    if let Some(t) = text_data.clone() {
                        layer.text_data = Some(t);
                    }
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    Ok(vec![LayerChange::Upsert { layer }])
                } else {
                    Ok(Vec::new())
                }
            }
            Command::SetAdjustment { id, adjustment } => {
                // Set/clear contract: Some sets the adjustment
                // (basic_adjustment Some + has_adjustments derived from
                // whether any channel is non-zero), None clears it (basic_adjustment
                // None + has_adjustments false). Unknown id is a silent no-op.
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Set Adjustment", std::slice::from_ref(&id));
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    match adjustment {
                        Some(adj) => {
                            // Normalise on write: clamp each channel
                            // to [-100, 100]. has_adjustments is exact
                            // (true only when a channel is non-zero), not always true.
                            let b = adj.brightness.clamp(-100.0, 100.0);
                            let c = adj.contrast.clamp(-100.0, 100.0);
                            let s = adj.saturation.clamp(-100.0, 100.0);
                            layer.basic_adjustment = Some(BasicAdjustment {
                                brightness: b,
                                contrast: c,
                                saturation: s,
                            });
                            layer.has_adjustments = Some(b != 0.0 || c != 0.0 || s != 0.0);
                        }
                        None => {
                            layer.basic_adjustment = None;
                            layer.has_adjustments = Some(false);
                        }
                    }
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    Ok(vec![LayerChange::Upsert { layer }])
                } else {
                    Ok(Vec::new())
                }
            }
            _ => unreachable!("non-metadata command reached metadata arm"),
        }
    }
}
