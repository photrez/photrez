//! Move-tool transform session: frame move, eight-handle resize and rotation
//! around the layer centre. Geometry comes from `transform_geometry`;
//! the live preview recomposites the document with the dragged transform and
//! commits one `TransformLayer` history entry on pointer-up.

use gpui_kit::Context;
use photrez_core::document::content::Transform2D;
use photrez_core::protocol::{Command, TransformPatch};

use crate::app::transform_geometry::{
    apply_resize_handle, apply_rotation_drag, detect_press_handle, layer_center, Handle, Point,
};
use crate::state::LayerTransformReadout;

use super::App;

/// Axis-aligned document-space bounds: (x, y, width, height).
pub(super) type LayerBounds = (f64, f64, f64, f64);

/// A snap guide proposed while dragging: (is-vertical, document position).
type GuideLine = (bool, f64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TransformMode {
    Move,
    Resize(Handle),
    Rotate,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupMember {
    pub id: String,
    pub start: Transform2D,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupTransformDrag {
    pub members: Vec<GroupMember>,
    pub group_start: (f64, f64, f64, f64),
    pub mode: TransformMode,
    pub start_doc: Point,
    pub previews: Vec<(String, Transform2D)>,
}

#[derive(Clone, Debug)]
pub(crate) struct TransformDrag {
    pub layer_id: String,
    pub start: Transform2D,
    pub width: f64,
    pub height: f64,
    pub mode: TransformMode,
    pub start_doc: Point,
    pub preview: Transform2D,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TransformDragUpdate {
    pub point: (f64, f64),
    pub screen: (f32, f32),
    pub shift: bool,
    pub alt: bool,
    pub bypass_snap: bool,
}

impl App {
    /// The Move bar's live transform readout: the same layer the transform
    /// frame, the align group and Reset all act on, so the five numbers always
    /// describe what the canvas is holding.
    pub(crate) fn transform_readout(
        &self,
        cx: &gpui_kit::App,
    ) -> Option<crate::state::LayerTransformReadout> {
        let (_, layer) = self.selected_layer_for_cursor(cx).ok()?;
        Some(LayerTransformReadout {
            x: layer.transform.x,
            y: layer.transform.y,
            width: layer.width * layer.transform.scale_x,
            height: layer.height * layer.transform.scale_y,
            rotation: layer.transform.rotation,
            scale_x: layer.transform.scale_x,
            scale_y: layer.transform.scale_y,
            locked: layer.locked,
            lock_position: layer.lock_position.unwrap_or(false),
            lock_rotation: layer.lock_rotation.unwrap_or(false),
        })
    }

    /// Commits one transform field, from the Move bar (`num-*`) or the
    /// Properties panel (`prop-*`). The patch carries the whole transform
    /// because `TransformLayer` is absolute; the two flip flags stay `None` so
    /// the layer's mirrored state survives the edit.
    ///
    /// W and H are size in document pixels, so they commit as a scale: the
    /// 0.1 build divides by the layer's source size. With the ratio held, the
    /// other axis follows at the same magnitude.
    pub(crate) fn set_transform_field(
        &mut self,
        id: &'static str,
        value: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let axis = id.chars().next_back().unwrap_or(' ');
        let blocked = layer.locked
            || (matches!(axis, 'x' | 'y') && layer.lock_position.unwrap_or(false))
            || (axis == 'r' && layer.lock_rotation.unwrap_or(false));
        if blocked || !value.is_finite() {
            return Ok(());
        }
        let constrain = self.state.read(cx).constrain_ratio;
        let mut scale_x = layer.transform.scale_x;
        let mut scale_y = layer.transform.scale_y;
        if matches!(axis, 'w' | 'h') {
            let source = if axis == 'w' {
                layer.width
            } else {
                layer.height
            };
            if value <= 0. || source <= 0. {
                return Ok(());
            }
            let next = value / source;
            scale_x = if axis == 'w' || constrain {
                next
            } else {
                scale_x
            };
            scale_y = if axis == 'h' || constrain {
                next
            } else {
                scale_y
            };
        }
        let transform = TransformPatch {
            x: if axis == 'x' {
                value
            } else {
                layer.transform.x
            },
            y: if axis == 'y' {
                value
            } else {
                layer.transform.y
            },
            scale_x,
            scale_y,
            rotation: if axis == 'r' {
                value
            } else {
                layer.transform.rotation
            },
            flip_h: None,
            flip_v: None,
        };
        self.apply_core_command(
            &doc_id,
            Command::TransformLayer {
                id: layer.id,
                transform,
            },
            true,
            cx,
        )
    }

    #[cfg(test)]
    pub(crate) fn selected_transform_for_test(
        &self,
        cx: &gpui_kit::App,
    ) -> Option<(f64, f64, f64, f64, f64)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        let state = self.state.read(cx);
        let selected = state
            .layers
            .iter()
            .find(|row| row.id == state.selected_layer)
            .and_then(|row| row.core_id.clone());
        let layer = selected
            .as_deref()
            .and_then(|core_id| content.layers.iter().find(|layer| layer.id == core_id))
            .or_else(|| content.layers.first())?;
        Some((
            layer.transform.x,
            layer.transform.y,
            layer.transform.scale_x,
            layer.transform.scale_y,
            layer.transform.rotation,
        ))
    }

    #[cfg(test)]
    pub(crate) fn snap_guides_for_test(&self) -> &[(bool, f64)] {
        &self.snap_guides
    }

    #[cfg(test)]
    pub(crate) fn transform_drag_for_test(&self) -> Option<&TransformDrag> {
        self.transform_drag.as_ref()
    }

    /// Start a transform gesture when the pointer hits a handle or the frame of
    /// the selected layer(s) while the Move tool is active.
    pub(crate) fn begin_transform_drag(
        &mut self,
        point: (f64, f64),
        zoom: f64,
        cx: &Context<Self>,
    ) -> bool {
        if self.begin_group_transform_drag(point, zoom, cx) {
            return true;
        }
        let Ok((_, layer)) = self.selected_layer_for_cursor(cx) else {
            return false;
        };
        let (frame_w, frame_h) = crate::app::transform_geometry::layer_frame_size(&layer);
        let Some(handle) = detect_press_handle(
            Point {
                x: point.0,
                y: point.1,
            },
            &layer.transform,
            frame_w,
            frame_h,
            zoom,
        ) else {
            return false;
        };
        let mode = match handle {
            Handle::Move => TransformMode::Move,
            Handle::Rotate => TransformMode::Rotate,
            other => TransformMode::Resize(other),
        };
        self.clear_queued_transform_drag_update();
        self.transform_drag = Some(TransformDrag {
            layer_id: layer.id,
            start: layer.transform.clone(),
            width: frame_w,
            height: frame_h,
            mode,
            start_doc: Point {
                x: point.0,
                y: point.1,
            },
            preview: layer.transform,
        });
        true
    }

    fn group_members(&self, cx: &Context<Self>) -> Option<(Vec<GroupMember>, LayerBounds)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        let state = self.state.read(cx);
        let selected_ids: Vec<String> = state
            .layers
            .iter()
            .filter(|row| state.selected_layers.contains(&row.id))
            .filter_map(|row| row.core_id.clone())
            .collect();
        if selected_ids.len() < 2 {
            return None;
        }
        let mut members = Vec::new();
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for id in selected_ids {
            let layer = content.layers.iter().find(|layer| layer.id == id)?;
            if !layer.visible {
                continue;
            }
            members.push(GroupMember {
                id: layer.id.clone(),
                start: layer.transform.clone(),
            });
            let (frame_w, frame_h) = crate::app::transform_geometry::layer_frame_size(layer);
            let (x, y, w, h) =
                crate::app::transform_geometry::layer_aabb(&layer.transform, frame_w, frame_h);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x + w);
            max_y = max_y.max(y + h);
        }
        if members.len() < 2 || !min_x.is_finite() || !min_y.is_finite() {
            return None;
        }
        Some((
            members,
            (
                min_x,
                min_y,
                (max_x - min_x).max(1.0),
                (max_y - min_y).max(1.0),
            ),
        ))
    }

    fn begin_group_transform_drag(
        &mut self,
        point: (f64, f64),
        zoom: f64,
        cx: &Context<Self>,
    ) -> bool {
        let Some((members, group_start)) = self.group_members(cx) else {
            return false;
        };
        let group_transform = Transform2D {
            x: group_start.0,
            y: group_start.1,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        };
        let Some(handle) = detect_press_handle(
            Point {
                x: point.0,
                y: point.1,
            },
            &group_transform,
            group_start.2,
            group_start.3,
            zoom,
        ) else {
            return false;
        };
        let mode = match handle {
            Handle::Move => TransformMode::Move,
            Handle::Rotate => TransformMode::Rotate,
            other => TransformMode::Resize(other),
        };
        let previews = members
            .iter()
            .map(|member| (member.id.clone(), member.start.clone()))
            .collect();
        self.clear_queued_transform_drag_update();
        self.group_transform_drag = Some(GroupTransformDrag {
            members,
            group_start,
            mode,
            start_doc: Point {
                x: point.0,
                y: point.1,
            },
            previews,
        });
        true
    }

    pub(crate) fn queue_transform_drag_update(
        &mut self,
        update: TransformDragUpdate,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) {
        if self.transform_drag_frame_pending {
            self.pending_transform_drag_update = Some(update);
            return;
        }

        // The first move of a frame applies now so the preview and HUD appear
        // on the very next render; moves that arrive before that render
        // coalesce into one update through the frame callback.
        self.transform_drag_frame_pending = true;
        let generation = self.transform_drag_frame_generation;
        cx.on_next_frame(window, move |app, _, cx| {
            if app.transform_drag_frame_generation != generation {
                return;
            }
            app.flush_pending_transform_drag_update(cx);
        });
        self.update_transform_drag(
            update.point,
            update.screen,
            update.shift,
            update.alt,
            update.bypass_snap,
            cx,
        );
    }

    pub(crate) fn update_transform_drag_immediately(
        &mut self,
        update: TransformDragUpdate,
        cx: &mut Context<Self>,
    ) {
        self.pending_transform_drag_update = Some(update);
        self.flush_pending_transform_drag_update(cx);
    }

    pub(crate) fn flush_pending_transform_drag_update(&mut self, cx: &mut Context<Self>) {
        self.transform_drag_frame_pending = false;
        let Some(update) = self.pending_transform_drag_update.take() else {
            return;
        };
        self.update_transform_drag(
            update.point,
            update.screen,
            update.shift,
            update.alt,
            update.bypass_snap,
            cx,
        );
    }

    fn clear_queued_transform_drag_update(&mut self) {
        self.pending_transform_drag_update = None;
        self.transform_drag_frame_pending = false;
        self.transform_drag_frame_generation = self.transform_drag_frame_generation.wrapping_add(1);
    }

    /// Update the preview for one pointer move. Returns true when the gesture
    /// consumed the event.
    pub(crate) fn update_transform_drag(
        &mut self,
        point: (f64, f64),
        screen: (f32, f32),
        shift: bool,
        alt: bool,
        bypass_snap: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.group_transform_drag.is_some() {
            return self.update_group_transform_drag(point, screen, shift, alt, cx);
        }
        let Some(drag) = self.transform_drag.clone() else {
            return false;
        };
        let doc = Point {
            x: point.0,
            y: point.1,
        };
        let mut next = match drag.mode {
            TransformMode::Move => {
                let dx = doc.x - drag.start_doc.x;
                let dy = doc.y - drag.start_doc.y;
                Transform2D {
                    x: drag.start.x + dx,
                    y: drag.start.y + dy,
                    ..drag.start.clone()
                }
            }
            TransformMode::Resize(handle) => apply_resize_handle(
                &drag.start,
                drag.width,
                drag.height,
                handle,
                doc.x - drag.start_doc.x,
                doc.y - drag.start_doc.y,
                shift,
                alt,
            ),
            TransformMode::Rotate => {
                let center = layer_center(&drag.start, drag.width, drag.height);
                let rotation =
                    apply_rotation_drag(center, drag.start_doc, doc, drag.start.rotation, shift);
                Transform2D {
                    rotation,
                    ..drag.start.clone()
                }
            }
        };
        self.snap_guides.clear();
        if matches!(drag.mode, TransformMode::Move)
            && !bypass_snap
            && self.state.read(cx).snap_enabled
        {
            let camera_zoom = self
                .active_camera()
                .map(|camera| camera.zoom as f64)
                .unwrap_or(1.0);
            if let Some((dx, dy, guides)) = self.snap_offsets(&drag.layer_id, &next, camera_zoom) {
                next.x += dx;
                next.y += dy;
                self.snap_guides = guides;
            }
        }
        if let Some(state) = self.transform_drag.as_mut() {
            state.preview = next.clone();
        }
        let hud = match drag.mode {
            TransformMode::Move => crate::state::HudInfo {
                mode: crate::state::HudMode::Move,
                x: screen.0,
                y: screen.1,
                a: next.x - drag.start.x,
                b: next.y - drag.start.y,
                scale_percent: 0.0,
                snap: !self.snap_guides.is_empty(),
            },
            TransformMode::Resize(_) => crate::state::HudInfo {
                mode: crate::state::HudMode::Resize,
                x: screen.0,
                y: screen.1,
                a: drag.width * next.scale_x.abs(),
                b: drag.height * next.scale_y.abs(),
                scale_percent: next.scale_x.abs() * 100.0,
                snap: false,
            },
            TransformMode::Rotate => crate::state::HudInfo {
                mode: crate::state::HudMode::Rotate,
                x: screen.0,
                y: screen.1,
                a: crate::app::transform_geometry::normalize_rotation(next.rotation),
                b: 0.0,
                scale_percent: 0.0,
                snap: shift,
            },
        };
        self.state.update(cx, |state, _| state.hud = Some(hud));
        if let Err(error) = self.display_with_transform(&drag.layer_id, &drag.start, &next, cx) {
            self.set_status_error(Some(format!("Transform preview: {error}")), cx);
        }
        cx.notify();
        true
    }

    fn update_group_transform_drag(
        &mut self,
        point: (f64, f64),
        screen: (f32, f32),
        shift: bool,
        alt: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.group_transform_drag.clone() else {
            return false;
        };
        let doc = Point {
            x: point.0,
            y: point.1,
        };
        let (gx, gy, gw, gh) = drag.group_start;
        let group_center = Point {
            x: gx + gw / 2.0,
            y: gy + gh / 2.0,
        };
        let previews: Vec<(String, Transform2D)> = match drag.mode {
            TransformMode::Move => {
                let dx = doc.x - drag.start_doc.x;
                let dy = doc.y - drag.start_doc.y;
                drag.members
                    .iter()
                    .map(|member| {
                        (
                            member.id.clone(),
                            Transform2D {
                                x: member.start.x + dx,
                                y: member.start.y + dy,
                                ..member.start.clone()
                            },
                        )
                    })
                    .collect()
            }
            TransformMode::Resize(handle) => {
                let group_transform = Transform2D {
                    x: gx,
                    y: gy,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    flip_h: false,
                    flip_v: false,
                };
                let res = apply_resize_handle(
                    &group_transform,
                    gw,
                    gh,
                    handle,
                    doc.x - drag.start_doc.x,
                    doc.y - drag.start_doc.y,
                    shift,
                    alt,
                );
                let factor_x = res.scale_x.abs();
                let factor_y = res.scale_y.abs();
                drag.members
                    .iter()
                    .map(|member| {
                        let relative_x = member.start.x - gx;
                        let relative_y = member.start.y - gy;
                        (
                            member.id.clone(),
                            Transform2D {
                                x: res.x + relative_x * factor_x,
                                y: res.y + relative_y * factor_y,
                                scale_x: member.start.scale_x * factor_x,
                                scale_y: member.start.scale_y * factor_y,
                                ..member.start.clone()
                            },
                        )
                    })
                    .collect()
            }
            TransformMode::Rotate => {
                let delta = apply_rotation_drag(group_center, drag.start_doc, doc, 0.0, shift);
                let radians = delta * crate::app::transform_geometry::DEG;
                let (cos, sin) = (radians.cos(), radians.sin());
                drag.members
                    .iter()
                    .map(|member| {
                        let relative_x = member.start.x - group_center.x;
                        let relative_y = member.start.y - group_center.y;
                        let new_x = relative_x * cos - relative_y * sin;
                        let new_y = relative_x * sin + relative_y * cos;
                        (
                            member.id.clone(),
                            Transform2D {
                                x: group_center.x + new_x,
                                y: group_center.y + new_y,
                                rotation: member.start.rotation + delta,
                                ..member.start.clone()
                            },
                        )
                    })
                    .collect()
            }
        };
        let hud = match drag.mode {
            TransformMode::Move => Some(crate::state::HudInfo {
                mode: crate::state::HudMode::Move,
                x: screen.0,
                y: screen.1,
                a: doc.x - drag.start_doc.x,
                b: doc.y - drag.start_doc.y,
                scale_percent: 0.0,
                snap: false,
            }),
            TransformMode::Rotate => Some(crate::state::HudInfo {
                mode: crate::state::HudMode::Rotate,
                x: screen.0,
                y: screen.1,
                a: previews
                    .first()
                    .map(|(id, transform)| {
                        drag.members
                            .iter()
                            .find(|member| &member.id == id)
                            .map(|member| {
                                crate::app::transform_geometry::normalize_rotation(
                                    transform.rotation - member.start.rotation,
                                )
                            })
                            .unwrap_or(0.0)
                    })
                    .unwrap_or(0.0),
                b: 0.0,
                scale_percent: 0.0,
                snap: shift,
            }),
            TransformMode::Resize(_) => None,
        };
        if let Some(hud) = hud {
            self.state.update(cx, |state, _| state.hud = Some(hud));
        }
        if let Some(state) = self.group_transform_drag.as_mut() {
            state.previews = previews.clone();
        }
        if let Err(error) = self.display_with_transforms(&drag.members, &previews, cx) {
            self.set_status_error(Some(format!("Transform preview: {error}")), cx);
        }
        cx.notify();
        true
    }

    /// Candidate snap offsets from the dragged layer's AABB to the canvas and
    /// other visible layers, within 6 screen pixels.
    fn snap_offsets(
        &self,
        layer_id: &str,
        next: &Transform2D,
        zoom: f64,
    ) -> Option<(f64, f64, Vec<GuideLine>)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        let dragged = content.layers.iter().find(|layer| layer.id == layer_id)?;
        let (dragged_w, dragged_h) = crate::app::transform_geometry::layer_frame_size(dragged);
        let (left, top, width, height) =
            crate::app::transform_geometry::layer_aabb(next, dragged_w, dragged_h);
        let mut vertical_targets = vec![0.0, content.width / 2.0, content.width];
        let mut horizontal_targets = vec![0.0, content.height / 2.0, content.height];
        for layer in &content.layers {
            if layer.id == layer_id || !layer.visible {
                continue;
            }
            let (other_w, other_h) = crate::app::transform_geometry::layer_frame_size(layer);
            let (x, y, w, h) =
                crate::app::transform_geometry::layer_aabb(&layer.transform, other_w, other_h);
            vertical_targets.extend([x, x + w / 2.0, x + w]);
            horizontal_targets.extend([y, y + h / 2.0, y + h]);
        }
        let threshold = 6.0 / zoom.max(0.01);
        let mut best_x: Option<(f64, f64)> = None;
        for source in [left, left + width / 2.0, left + width] {
            for target in &vertical_targets {
                let delta = target - source;
                if delta.abs() <= threshold
                    && best_x
                        .map(|(_, best)| delta.abs() < best.abs())
                        .unwrap_or(true)
                {
                    best_x = Some((delta, *target));
                }
            }
        }
        let mut best_y: Option<(f64, f64)> = None;
        for source in [top, top + height / 2.0, top + height] {
            for target in &horizontal_targets {
                let delta = target - source;
                if delta.abs() <= threshold
                    && best_y
                        .map(|(_, best)| delta.abs() < best.abs())
                        .unwrap_or(true)
                {
                    best_y = Some((delta, *target));
                }
            }
        }
        if best_x.is_none() && best_y.is_none() {
            return None;
        }
        let mut guides = Vec::new();
        if let Some((_, target)) = best_x {
            guides.push((true, target));
        }
        if let Some((_, target)) = best_y {
            guides.push((false, target));
        }
        Some((
            best_x.map(|(delta, _)| delta).unwrap_or(0.0),
            best_y.map(|(delta, _)| delta).unwrap_or(0.0),
            guides,
        ))
    }

    /// Commit the previewed transform as one history entry (one per member for
    /// a group transform).
    pub(crate) fn finish_transform_drag(&mut self, cx: &mut Context<Self>) -> bool {
        if self.transform_drag.is_some() || self.group_transform_drag.is_some() {
            self.clear_queued_transform_drag_update();
        }
        self.state.update(cx, |state, _| state.hud = None);
        if let Some(drag) = self.group_transform_drag.take() {
            let Ok(doc_id) = self.active_doc_id() else {
                return true;
            };
            let mut result = Ok(());
            for (id, transform) in drag.previews {
                let applied = self.apply_core_command(
                    &doc_id,
                    Command::TransformLayer {
                        id,
                        transform: TransformPatch {
                            x: transform.x,
                            y: transform.y,
                            scale_x: transform.scale_x,
                            scale_y: transform.scale_y,
                            rotation: transform.rotation,
                            flip_h: Some(transform.flip_h),
                            flip_v: Some(transform.flip_v),
                        },
                    },
                    true,
                    cx,
                );
                if let Err(error) = applied {
                    result = Err(error);
                    break;
                }
            }
            match result {
                Ok(()) => self.set_status_error(None, cx),
                Err(error) => self.set_status_error(Some(format!("Transform: {error}")), cx),
            }
            cx.notify();
            return true;
        }
        let Some(drag) = self.transform_drag.take() else {
            return false;
        };
        self.snap_guides.clear();
        let Ok(doc_id) = self.active_doc_id() else {
            return true;
        };
        let transform = TransformPatch {
            x: drag.preview.x,
            y: drag.preview.y,
            scale_x: drag.preview.scale_x,
            scale_y: drag.preview.scale_y,
            rotation: drag.preview.rotation,
            flip_h: Some(drag.preview.flip_h),
            flip_v: Some(drag.preview.flip_v),
        };
        let result = self.apply_core_command(
            &doc_id,
            Command::TransformLayer {
                id: drag.layer_id,
                transform,
            },
            true,
            cx,
        );
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => {
                self.set_status_error(Some(format!("Transform: {error}")), cx);
                let _ = self.refresh_active_canvas(cx);
            }
        }
        cx.notify();
        true
    }

    /// Arrow-key nudge of the selected layer; one history entry per key.
    pub(crate) fn nudge_selected_layer(
        &mut self,
        dx: f64,
        dy: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        self.apply_core_command(
            &doc_id,
            Command::TransformLayer {
                id: layer.id,
                transform: TransformPatch {
                    x: layer.transform.x + dx,
                    y: layer.transform.y + dy,
                    scale_x: layer.transform.scale_x,
                    scale_y: layer.transform.scale_y,
                    rotation: layer.transform.rotation,
                    flip_h: Some(layer.transform.flip_h),
                    flip_v: Some(layer.transform.flip_v),
                },
            },
            true,
            cx,
        )
    }

    /// Drop the preview without committing.
    pub(crate) fn cancel_transform_drag(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_queued_transform_drag_update();
        self.state.update(cx, |state, _| state.hud = None);
        if self.group_transform_drag.take().is_some() {
            let _ = self.refresh_active_canvas(cx);
            cx.notify();
            return true;
        }
        if self.transform_drag.take().is_none() {
            return false;
        }
        self.snap_guides.clear();
        let _ = self.refresh_active_canvas(cx);
        cx.notify();
        true
    }
}
