// SPDX-License-Identifier: AGPL-3.0-or-later
// apply() command-arm dispatch for ProtocolEngine. Extracted from
// document_core.rs so that module stays under the 1000-line guard; this is a
// sibling impl ProtocolEngine block (method visibility is per-impl, not per module).
use super::*;
use crate::canonical_model::{BasicAdjustment, BlendMode, LayerType, SelectionState};

impl ProtocolEngine {
    // Selection is engine-local UI state, not an undoable transition. Set the
    // engine field AND mirror it onto the canonical shadow (when seeded) so the
    // protocol_canonical_native read-back stays truthful. No history entry, empty
    // delta — the caller's apply() tail bumps the version and reconciles.
    // Selection is engine-local UI state, not an undoable transition. Set the
    // engine field AND mirror it onto the canonical shadow (when seeded) so the
    // protocol_canonical_native read-back stays truthful. No history entry, empty
    // delta — the caller's apply() tail bumps the version and reconciles.
    pub(crate) fn set_engine_selection(&mut self, sel: Option<SelectionState>) {
        self.selection = sel.clone();
        if let Some(shadow) = &mut self.canonical {
            shadow.doc.selection = sel;
        }
    }

    pub fn apply(&mut self, envelope: CommandEnvelope) -> Result<CommandResult, ProtocolError> {
        if envelope.contract_version != CONTRACT_VERSION {
            return Err(ProtocolError {
                code: "E_CONTRACT_VERSION".to_string(),
                message: format!(
                    "expected contractVersion {} got {}",
                    CONTRACT_VERSION, envelope.contract_version
                ),
            });
        }
        if let Some(expected) = envelope.expected_version {
            if expected != self.version {
                return Err(ProtocolError {
                    code: "E_VERSION_MISMATCH".to_string(),
                    message: format!("expected version {} got {}", expected, self.version),
                });
            }
        }
        // External-pending barrier (H0 invariant): a host handoff is
        // outstanding — no new command may enter until the cursor commit lands.
        if let Some((seq, dir)) = &self.pending_external {
            return Err(ProtocolError {
                code: "E_EXTERNAL_PENDING".to_string(),
                message: format!(
                    "external history transition pending: commit seq {} direction {} first",
                    seq, dir
                ),
            });
        }
        let base = self.version;
        // H0: external record is its own transition event.
        if let Command::RecordExternalTransition {
            label,
            affected_layer_ids,
            adapter_id,
            token,
            memory_cost_bytes,
        } = envelope.command
        {
            self.record_external(
                &label,
                &affected_layer_ids,
                &adapter_id,
                &token,
                memory_cost_bytes,
            )?;
            // Note: record_external already bumps version; do not bump again.
            // Layers unchanged here - reconcile intentionally skipped (host executes
            // external mutation out-of-band).
            return Ok(CommandResult {
                document_version: self.version,
                delta: RenderDelta {
                    base_version: base,
                    version: self.version,
                    changes: Vec::new(),
                },
                status: Some("external-recorded".to_string()),
                external_seq: None,
            });
        }
        // Walker handoff marker: set when an undo/redo step lands on an
        // EXTERNAL entry — host executes via its adapter then confirms with
        // protocol_history_cursor_commit (which performs the DV bump).
        let mut external_handoff: Option<u64> = None;
        let mut handoff_dir = "";
        let changes = match envelope.command {
            Command::Noop => Vec::new(),
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
                    has_adjustments: None,
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
                vec![RenderLayerChange::Upsert { layer }]
            }
            Command::AddLayer {
                id,
                name,
                width,
                height,
                index,
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
                let _e = self.begin_forward("Add Layer", std::slice::from_ref(&id));
                let mut layer = RenderLayer {
                    id: id.clone(),
                    name: name.clone(),
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
                    layer_type: Some(LayerType::Raster),
                    blend_mode: Some(BlendMode::Normal),
                    locked: None,
                    lock_transparency: None,
                    lock_position: None,
                    lock_rotation: None,
                    is_background: None,
                    has_adjustments: None,
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
                vec![RenderLayerChange::Upsert { layer }]
            }
            Command::DeleteLayer { id } => {
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Delete Layer", std::slice::from_ref(&id));
                    let resource_id = self.layers.get(pos).expect("layer present").resource_id;
                    self.layers = self.layers.removed(pos);
                    self.finish_forward(_e);
                    vec![RenderLayerChange::Remove {
                        id: id.clone(),
                        resource_id,
                    }]
                } else {
                    Vec::new()
                }
            }
            // ── Metadata arms ──
            // Unknown id mirrors DeleteLayer: no-op (the TS engine's apply ops are
            // guarded the same way), so the native-authority path stays bug-compatible
            // with the TS engine until the flip.
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
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
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
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
                }
            }
            Command::Rename { id, name } => {
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Rename", std::slice::from_ref(&id));
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    layer.name = name.clone();
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
                }
            }
            Command::Reorder { id, to } => {
                // Reject an out-of-range target before any mutation, history entry,
                // or DV bump - mirrors the TS graph-mirror reorder_layer guard. The
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
                    // Mirror TS applyReorderLayer: the Background is pinned to the bottom
                    // and is never reordered (a layer beneath it would be unreachable).
                    if self
                        .layers
                        .get(pos)
                        .map(|l| l.is_background == Some(true))
                        .unwrap_or(false)
                    {
                        Vec::new()
                    } else {
                        let _e = self.begin_forward("Reorder", std::slice::from_ref(&id));
                        let layer = self.layers.get(pos).expect("layer present").clone();
                        // Remove at the current index, then insert at the clamped target
                        // (LayerSet::insert_at clamps to len, mirroring TS' splice).
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
                        self.layers
                            .0
                            .iter()
                            .map(|l| RenderLayerChange::Upsert {
                                layer: l.as_ref().clone(),
                            })
                            .collect()
                    }
                } else {
                    Vec::new()
                }
            }
            Command::SetBackgroundFlag { id } => {
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Set Background Flag", std::slice::from_ref(&id));
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    // Mirror document.ts markLayerAsBackground: flag + lock position/rotation.
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
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
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
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
                }
            }
            Command::SetLayerParams {
                id,
                shape_params,
                text_data,
            } => {
                // Both None is an invalid no-op (mirrors TS: a params update always
                // carries the payload). Reject before any mutation so no history
                // entry / DV bump is produced for a meaningless command.
                if shape_params.is_none() && text_data.is_none() {
                    return Err(ProtocolError {
                        code: "E_INVALID".to_string(),
                        message: "setLayerParams requires shapeParams or textData".to_string(),
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
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
                }
            }
            Command::SetAdjustment { id, adjustment } => {
                // Mirrors TS applyBasicAdjustment / clearBasicAdjustments: Some sets
                // the adjustment (basic_adjustment Some + has_adjustments derived from
                // whether any channel is non-zero), None clears it (basic_adjustment
                // None + has_adjustments false). Unknown id is a silent no-op.
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Set Adjustment", std::slice::from_ref(&id));
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    match adjustment {
                        Some(adj) => {
                            // Mirror TS normalizeBasicAdjustment: clamp each channel
                            // to [-100, 100]. has_adjustments follows TS exactly
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
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
                }
            }
            // ── Selection arms ──
            // Selection rides Model-A snapshots; it commits NO history entry and
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
                Vec::new()
            }
            Command::ClearSelection => {
                self.set_engine_selection(None);
                Vec::new()
            }
            Command::SelectAll => {
                // Engine has no document dims; the canonical shadow does. Host must
                // seed the document before select-all — reject with an E_INVALID-
                // shaped error before any mutation if the shadow is absent.
                let (w, h) = match &self.canonical {
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
                Vec::new()
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
                    let (w, h) = match &self.canonical {
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
                Vec::new()
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
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
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
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
                }
            }
            Command::BrushStroke {
                layer_id,
                points,
                settings,
            } => {
                if points.is_empty() {
                    Vec::new()
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
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
                }
            }
            // -- Structural command arms (duplicate / merge-down / merge-selected /
            //    flatten / rasterize) --
            // Each arm delegates to a private helper in document_core_structural.rs
            // that performs the full begin_forward -> mutate -> finish_forward
            // sequence and returns the delta. The helpers reject E_INVALID before
            // mutation and treat unknown / not-mergeable ids as silent no-ops
            // (empty delta), matching the TS oracle apply ops.
            Command::DuplicateLayer { id, new_id } => self.apply_duplicate(&id, &new_id)?,
            Command::MergeDown { id, merged_id } => self.apply_merge_down(&id, &merged_id)?,
            Command::MergeSelected { ids, merged_id } => {
                self.apply_merge_selected(&ids, &merged_id)?
            }
            Command::Flatten { merged_id } => self.apply_flatten(&merged_id)?,
            Command::RasterizeLayer { id } => self.apply_rasterize(&id)?,
            // -- Canvas-size command arms (Crop Canvas / Apply Crop / Resize Canvas) --
            // Each delegates to a private helper in document_core_canvas.rs that
            // performs begin_forward -> mutate doc_size + (optional) layers ->
            // finish_forward and returns the delta (ordered all-layer Upserts for
            // the crop arms; an empty delta for resize, whose only effect is the
            // document size). Helpers reject E_INVALID before mutation and treat
            // non-positive dims as a silent no-op (matching the TS oracle).
            Command::CropCanvas {
                x,
                y,
                width,
                height,
            } => self.apply_crop_canvas(x, y, width, height)?,
            Command::ApplyCrop {
                x,
                y,
                width,
                height,
                rotation,
                target_width,
                target_height,
            } => {
                self.apply_apply_crop(x, y, width, height, rotation, target_width, target_height)?
            }
            Command::ResizeCanvas { width, height } => self.apply_resize_canvas(width, height)?,
            // Handled by the early-return above (kept for exhaustiveness).
            Command::RecordExternalTransition { .. } => Vec::new(),
            Command::Undo => {
                if self.cursor == 0 {
                    // No-op undo: DV still bumps (accepted transition event).
                    Vec::new()
                } else {
                    let e = &self.entries[self.cursor - 1];
                    match &e.payload {
                        EntryPayload::Native {
                            before,
                            doc_size_before,
                            ..
                        } => {
                            let new_layers = before.clone();
                            let changes = Self::diff(&self.layers, &new_layers);
                            self.layers = new_layers;
                            // Restore the document size for canvas arms. This is
                            // always written (Some or None) so a None->Some crop
                            // undoes back to None; metadata arms leave the value
                            // unchanged, so restoring it is a no-op for them.
                            self.doc_size = *doc_size_before;
                            if let Some(d) = doc_size_before {
                                if let Some(sh) = &mut self.canonical {
                                    sh.doc.width = d.0;
                                    sh.doc.height = d.1;
                                }
                            }
                            // Hazard: a full canonical re-seed replaces the history
                            // stream wholesale (gated path), so this walker never
                            // runs against re-seeded shadow dims in production. If
                            // that gating is ever relaxed, a subsequent undo/redo
                            // would overwrite the re-seeded shadow dimensions with
                            // this entry's captured pair. Resolve before native
                            // authority cutover.
                            self.cursor -= 1;
                            changes
                        }
                        EntryPayload::Pixel { .. } => {
                            // Pixel undo is handled by undo_pixel/redo_pixel
                            // (the separate authoritative path); the metadata
                            // apply walker leaves pixel entries untouched.
                            Vec::new()
                        }
                        EntryPayload::Snapshot { .. } => {
                            // Snapshot undo/redo is handled by the dedicated
                            // undo_snapshot/redo_snapshot path (it restores the
                            // metadata + bitmap-token reference); the metadata
                            // walker leaves Snapshot entries untouched.
                            Vec::new()
                        }
                        EntryPayload::External { .. } => {
                            // Host handoff: adapter executes, then
                            // protocol_history_cursor_commit moves the cursor
                            // and bumps DV. Nothing changes here.
                            external_handoff = Some(e.seq);
                            handoff_dir = "undo";
                            Vec::new()
                        }
                    }
                }
            }
            Command::Redo => {
                if self.cursor >= self.entries.len() {
                    Vec::new()
                } else {
                    let e = &self.entries[self.cursor];
                    match &e.payload {
                        EntryPayload::Native {
                            after,
                            doc_size_after,
                            ..
                        } => {
                            let new_layers = after.clone();
                            let changes = Self::diff(&self.layers, &new_layers);
                            self.layers = new_layers;
                            // Restore the document size for canvas arms (see undo
                            // branch: always written, Some or None).
                            self.doc_size = *doc_size_after;
                            if let Some(d) = doc_size_after {
                                if let Some(sh) = &mut self.canonical {
                                    sh.doc.width = d.0;
                                    sh.doc.height = d.1;
                                }
                            }
                            // Hazard: a full canonical re-seed replaces the history
                            // stream wholesale (gated path), so this walker never
                            // runs against re-seeded shadow dims in production. If
                            // that gating is ever relaxed, a subsequent undo/redo
                            // would overwrite the re-seeded shadow dimensions with
                            // this entry's captured pair. Resolve before native
                            // authority cutover.
                            self.cursor += 1;
                            changes
                        }
                        EntryPayload::Pixel { .. } => {
                            // See undo branch: pixel entries are owned by
                            // undo_pixel/redo_pixel, not the metadata walker.
                            Vec::new()
                        }
                        EntryPayload::Snapshot { .. } => {
                            // See undo branch: snapshot entries are owned by
                            // undo_snapshot/redo_snapshot, not the walker.
                            Vec::new()
                        }
                        EntryPayload::External { .. } => {
                            external_handoff = Some(e.seq);
                            handoff_dir = "redo";
                            Vec::new()
                        }
                    }
                }
            }
        };
        if let Some(seq) = external_handoff {
            self.pending_external = Some((seq, handoff_dir.to_string()));
            // Layers unchanged here - reconcile intentionally skipped (host executes
            // external mutation out-of-band); the cursor commit lands it separately.
            return Ok(CommandResult {
                document_version: self.version,
                delta: RenderDelta {
                    base_version: base,
                    version: self.version,
                    changes: Vec::new(),
                },
                status: Some("external".to_string()),
                external_seq: Some(seq),
            });
        }
        self.version += 1;
        self.reconcile_shadow();
        Ok(CommandResult {
            document_version: self.version,
            delta: RenderDelta {
                base_version: base,
                version: self.version,
                changes,
            },
            status: None,
            external_seq: None,
        })
    }
}
