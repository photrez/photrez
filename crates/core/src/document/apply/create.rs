// SPDX-License-Identifier: AGPL-3.0-or-later
// Layer creation/mutation arms for DocumentCore::apply.

use super::*;
use crate::ptz::content::{BlendMode, LayerType};

impl DocumentCore {
    pub(super) fn apply_create_or_delete_arm(
        &mut self,
        command: Command,
    ) -> Result<Vec<RenderLayerChange>, ProtocolError> {
        match command {
            Command::Ping { echo } => {
                let id = format!("ping:{}", echo);
                let layer = RenderLayer {
                    id: id.clone(),
                    name: echo.clone(),
                    visible: true,
                    opacity: 1.0,
                    resource_id: self.next_resource,
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
                    layer_type: None,
                    blend_mode: None,
                    locked: None,
                    lock_transparency: None,
                    lock_position: None,
                    lock_rotation: None,
                    is_background: None,
                    has_adjustments: Some(false),
                    width: None,
                    height: None,
                    flip_h: None,
                    flip_v: None,
                    shape_params: None,
                    text_data: None,
                    basic_adjustment: None,
                };
                if let Some(pos) = self.layers.position_by_id(&id) {
                    self.layers = self.layers.replaced(pos, layer.clone());
                } else {
                    self.next_resource += 1;
                    self.layers = self.layers.pushed(layer.clone());
                }
                Ok(vec![RenderLayerChange::Upsert { layer }])
            }
            Command::AddLayer {
                id,
                name,
                width,
                height,
                index,
                x,
                y,
                layer_type,
                shape_params,
                text_data,
            } => {
                // Host owns identity + placement: the active layer is UI state the
                // engine must not assume, so the id (TS-minted) and insertion
                // index (above the active layer) travel with the command. Reject a
                // duplicate or empty id so the host's id space stays unambiguous.
                if id.is_empty() {
                    return Err(ProtocolError {
                        code: "E_INVALID".to_string(),
                        message: "addLayer id must not be empty".to_string(),
                    });
                }
                if self.layers.position_by_id(&id).is_some() {
                    return Err(ProtocolError {
                        code: "E_INVALID".to_string(),
                        message: format!("addLayer id already present: {}", id),
                    });
                }
                let layer_x = x.unwrap_or(0.0);
                let layer_y = y.unwrap_or(0.0);
                if !layer_x.is_finite() || !layer_y.is_finite() {
                    return Err(ProtocolError {
                        code: "E_INVALID".to_string(),
                        message: "addLayer origin must be finite".to_string(),
                    });
                }
                let _e = self.begin_forward("Add Layer", std::slice::from_ref(&id));
                let mut layer = RenderLayer {
                    id: id.clone(),
                    name: name.clone(),
                    visible: true,
                    opacity: 1.0,
                    resource_id: self.next_resource,
                    x: layer_x,
                    y: layer_y,
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
                    locked: None,
                    lock_transparency: None,
                    lock_position: None,
                    lock_rotation: None,
                    is_background: None,
                    has_adjustments: Some(false),
                    width: Some(width),
                    height: Some(height),
                    flip_h: None,
                    flip_v: None,
                    shape_params: None,
                    text_data: None,
                    basic_adjustment: None,
                };
                // Typed add (metadata only; rasterization stays host-side): when the
                // envelope carries a layer type, project the true value. Mirrors the
                // TS createShapeLayerNode / createTextLayerNode defaults — type is
                // taken from the command, blendMode stays Normal, and the matching
                // nested payload (shape_params / text_data) is carried verbatim.
                if let Some(lt) = layer_type {
                    layer.layer_type = Some(lt.clone());
                    match lt {
                        LayerType::Shape => {
                            if let Some(p) = shape_params.clone() {
                                layer.shape_params = Some(p);
                            }
                        }
                        LayerType::Text => {
                            if let Some(t) = text_data.clone() {
                                layer.text_data = Some(t);
                            }
                        }
                        _ => {}
                    }
                }
                self.next_resource += 1;
                // Insert at the host-supplied index (clamped), not pushed at end.
                self.layers = self.layers.insert_at(layer.clone(), index);
                self.finish_forward(_e);
                Ok(vec![RenderLayerChange::Upsert { layer }])
            }
            Command::AddLayerWithState {
                id,
                name,
                width,
                height,
                index,
                transform,
                opacity,
                blend_mode,
                visible,
                locked,
                layer_type,
                shape_params,
                text_data,
            } => {
                if id.is_empty() || self.layers.position_by_id(&id).is_some() {
                    return Err(ProtocolError {
                        code: "E_INVALID".to_string(),
                        message: format!("addLayerWithState id is empty or present: {}", id),
                    });
                }
                if !transform.x.is_finite()
                    || !transform.y.is_finite()
                    || !transform.scale_x.is_finite()
                    || !transform.scale_y.is_finite()
                    || !transform.rotation.is_finite()
                    || !opacity.is_finite()
                {
                    return Err(ProtocolError {
                        code: "E_INVALID".to_string(),
                        message: "addLayerWithState state must be finite".to_string(),
                    });
                }
                let _e = self.begin_forward("Add Layer", std::slice::from_ref(&id));
                let mut layer = RenderLayer {
                    id: id.clone(),
                    name: name.clone(),
                    visible,
                    opacity,
                    resource_id: self.next_resource,
                    x: transform.x,
                    y: transform.y,
                    scale_x: transform.scale_x,
                    scale_y: transform.scale_y,
                    rotation: transform.rotation,
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
                    has_adjustments: Some(false),
                    width: Some(width),
                    height: Some(height),
                    flip_h: Some(transform.flip_h),
                    flip_v: Some(transform.flip_v),
                    shape_params: None,
                    text_data: None,
                    basic_adjustment: None,
                };
                if let Some(lt) = layer_type {
                    layer.layer_type = Some(lt.clone());
                    match lt {
                        LayerType::Shape => {
                            if let Some(p) = shape_params {
                                layer.shape_params = Some(p);
                            }
                        }
                        LayerType::Text => {
                            if let Some(t) = text_data {
                                layer.text_data = Some(t);
                            }
                        }
                        _ => {}
                    }
                }
                self.next_resource += 1;
                self.layers = self.layers.insert_at(layer.clone(), index);
                self.finish_forward(_e);
                Ok(vec![RenderLayerChange::Upsert { layer }])
            }
            Command::DeleteLayer { id } => {
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Delete Layer", std::slice::from_ref(&id));
                    let resource_id = self.layers.get(pos).expect("layer present").resource_id;
                    self.layers = self.layers.removed(pos);
                    self.finish_forward(_e);
                    Ok(vec![RenderLayerChange::Remove {
                        id: id.clone(),
                        resource_id,
                    }])
                } else {
                    Ok(Vec::new())
                }
            }
            _ => unreachable!("non-create/delete command reached create arm"),
        }
    }
}
