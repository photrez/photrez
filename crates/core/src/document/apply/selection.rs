// SPDX-License-Identifier: AGPL-3.0-or-later
// Selection, transform, opacity, and brush arms for DocumentCore::apply.

use super::*;
use crate::ptz::content::SelectionState;

impl DocumentCore {
    pub(super) fn apply_selection_or_transform_arm(
        &mut self,
        command: Command,
    ) -> Result<Vec<RenderLayerChange>, ProtocolError> {
        match command {
            // ── Selection arms ──
            // Selection rides whole-document snapshots; it commits NO history entry and
            // produces an empty delta. The apply() tail still bumps the document
            // version (accepted transition) and reconciles the shadow.
            Command::SetSelection { selection } => {
                // Trust boundary: reject non-finite or negative-dimension geometry
                // before any mutation (selection is host-driven; garbage in must
                // not reach the engine state).
                let bad = !selection.x.is_finite()
                    || !selection.y.is_finite()
                    || !selection.width.is_finite()
                    || !selection.height.is_finite()
                    || !selection.angle.is_finite()
                    || selection.width < 0.0
                    || selection.height < 0.0;
                if bad {
                    return Err(ProtocolError {
                        code: "E_INVALID".to_string(),
                        message: "setSelection requires finite x/y/width/height/angle and width/height >= 0".to_string(),
                    });
                }
                self.set_engine_selection(Some(selection));
                Ok(Vec::new())
            }
            Command::ClearSelection => {
                self.set_engine_selection(None);
                Ok(Vec::new())
            }
            Command::SelectAll => {
                // Engine has no document dims; the canonical shadow does. Host must
                // seed the document before select-all — reject with an E_INVALID-
                // shaped error before any mutation if the shadow is absent.
                let (w, h) = match &self.replica {
                    Some(sh) => (sh.doc.width, sh.doc.height),
                    None => {
                        return Err(ProtocolError {
                            code: "E_INVALID".to_string(),
                            message: "selectAll requires a seeded canonical document (host must seed before select-all)".to_string(),
                        });
                    }
                };
                self.set_engine_selection(Some(SelectionState {
                    x: 0.0,
                    y: 0.0,
                    width: w,
                    height: h,
                    angle: 0.0,
                    shape: None,
                    inverted: None,
                }));
                Ok(Vec::new())
            }
            Command::InvertSelection => {
                // Mirrors the host op, which falls back to select-all when nothing
                // is selected: no engine selection -> build the same full-canvas
                // rect the SelectAll arm builds from the canonical shadow dims
                // (canonical absent -> same E_INVALID-shaped error as SelectAll).
                // With a selection present, toggle the inverted flag as before.
                // DV still bumps via the tail; no history entry; shadow mirrored;
                // delta empty.
                if let Some(mut sel) = self.selection.clone() {
                    sel.inverted = Some(!sel.inverted.unwrap_or(false));
                    self.set_engine_selection(Some(sel));
                } else {
                    let (w, h) = match &self.replica {
                        Some(sh) => (sh.doc.width, sh.doc.height),
                        None => {
                            return Err(ProtocolError {
                                code: "E_INVALID".to_string(),
                                message: "invertSelection with no selection falls back to select-all, which requires a seeded canonical document (host must seed before select-all)".to_string(),
                            });
                        }
                    };
                    self.set_engine_selection(Some(SelectionState {
                        x: 0.0,
                        y: 0.0,
                        width: w,
                        height: h,
                        angle: 0.0,
                        shape: None,
                        inverted: None,
                    }));
                }
                Ok(Vec::new())
            }
            Command::TransformLayer { id, transform } => {
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Transform Layer", std::slice::from_ref(&id));
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    layer.x = transform.x;
                    layer.y = transform.y;
                    layer.scale_x = transform.scale_x;
                    layer.scale_y = transform.scale_y;
                    layer.rotation = transform.rotation;
                    // Project flip flags when the envelope carries them (additive
                    // Option fields on TransformPatch; None leaves the field unset).
                    if let Some(fh) = transform.flip_h {
                        layer.flip_h = Some(fh);
                    }
                    if let Some(fv) = transform.flip_v {
                        layer.flip_v = Some(fv);
                    }
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    Ok(vec![RenderLayerChange::Upsert { layer }])
                } else {
                    Ok(Vec::new())
                }
            }
            Command::SetOpacity { id, opacity } => {
                let clamped = opacity.clamp(0.0, 1.0);
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Set Opacity", std::slice::from_ref(&id));
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    layer.opacity = clamped;
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    Ok(vec![RenderLayerChange::Upsert { layer }])
                } else {
                    Ok(Vec::new())
                }
            }
            Command::BrushStroke {
                layer_id,
                points,
                settings,
            } => {
                if points.is_empty() {
                    Ok(Vec::new())
                } else if let Some(pos) = self.layers.position_by_id(&layer_id) {
                    let _e = self.begin_forward("Brush Stroke", std::slice::from_ref(&layer_id));
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    // dirtyRect must account for brush footprint, not just point bbox
                    let mut min_x = f64::INFINITY;
                    let mut max_x = f64::NEG_INFINITY;
                    let mut min_y = f64::INFINITY;
                    let mut max_y = f64::NEG_INFINITY;
                    for p in &points {
                        if p.x < min_x {
                            min_x = p.x;
                        }
                        if p.x > max_x {
                            max_x = p.x;
                        }
                        if p.y < min_y {
                            min_y = p.y;
                        }
                        if p.y > max_y {
                            max_y = p.y;
                        }
                    }
                    let radius = (settings.size / 2.0).ceil() as i32;
                    let x = (min_x.floor() as i32) - radius;
                    let y = (min_y.floor() as i32) - radius;
                    let w =
                        ((max_x - min_x).ceil() as i32 + settings.size.ceil() as i32).max(1) as u32;
                    let h =
                        ((max_y - min_y).ceil() as i32 + settings.size.ceil() as i32).max(1) as u32;
                    layer.dirty_rect = Some(Rect {
                        x,
                        y,
                        width: w,
                        height: h,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    Ok(vec![RenderLayerChange::Upsert { layer }])
                } else {
                    Ok(Vec::new())
                }
            }
            _ => unreachable!("non-selection/transform command reached selection arm"),
        }
    }
}
