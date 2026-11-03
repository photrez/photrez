//! Move-tool transform session: frame move, eight-handle resize and rotation
//! around the layer centre. Geometry comes from `transform_geometry`;
//! the live preview recomposites the document with the dragged transform and
//! commits one `TransformLayer` history entry on pointer-up.

use gpui_kit::Context;
use photrez_core::protocol::{Command, TransformPatch};
use photrez_core::ptz::content::{LayerType, Transform2D};

use crate::app::transform_geometry::{
    apply_resize_handle, apply_rotation_drag, detect_press_handle, document_to_layer_local,
    layer_center, layer_corners, Handle, Point,
};
use crate::state::LayerTransformReadout;

use super::App;

/// Axis-aligned document-space bounds: (x, y, width, height).
type LayerBounds = (f64, f64, f64, f64);
fn layer_damage_bounds(
    previous: &Transform2D,
    next: &Transform2D,
    width: f64,
    height: f64,
) -> Option<LayerBounds> {
    let (old_x, old_y, old_w, old_h) =
        crate::app::transform_geometry::layer_aabb(previous, width, height);
    let (new_x, new_y, new_w, new_h) =
        crate::app::transform_geometry::layer_aabb(next, width, height);
    let left = old_x.min(new_x);
    let top = old_y.min(new_y);
    let right = (old_x + old_w).max(new_x + new_w);
    let bottom = (old_y + old_h).max(new_y + new_h);
    if ![left, top, right, bottom].into_iter().all(f64::is_finite) || right <= left || bottom <= top
    {
        return None;
    }
    Some((left, top, right - left, bottom - top))
}

fn merge_damage_bounds(bounds: &mut Option<LayerBounds>, next: LayerBounds) {
    *bounds = Some(match *bounds {
        Some((x, y, width, height)) => {
            let left = x.min(next.0);
            let top = y.min(next.1);
            let right = (x + width).max(next.0 + next.2);
            let bottom = (y + height).max(next.1 + next.3);
            (left, top, right - left, bottom - top)
        }
        None => next,
    });
}

fn damage_region(bounds: LayerBounds) -> Option<photrez_core::render::compositor::CompositeRegion> {
    let (x, y, width, height) = bounds;
    let right = x + width;
    let bottom = y + height;
    if ![x, y, right, bottom].into_iter().all(f64::is_finite) {
        return None;
    }
    let x = (x.floor() - 1.0).max(0.0) as u32;
    let y = (y.floor() - 1.0).max(0.0) as u32;
    let right = (right.ceil() + 1.0).max(f64::from(x)) as u32;
    let bottom = (bottom.ceil() + 1.0).max(f64::from(y)) as u32;
    Some(photrez_core::render::compositor::CompositeRegion {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    })
}

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
            locked: layer.locked,
        })
    }

    /// Commits one Move-bar transform field. The patch carries the whole
    /// transform because `TransformLayer` is absolute; the two flip flags stay
    /// `None` so the layer's mirrored state survives the edit.
    pub(crate) fn set_transform_field(
        &mut self,
        id: &'static str,
        value: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        if layer.locked || !value.is_finite() {
            return Ok(());
        }
        let transform = TransformPatch {
            x: match id {
                "num-x" => value,
                _ => layer.transform.x,
            },
            y: match id {
                "num-y" => value,
                _ => layer.transform.y,
            },
            scale_x: layer.transform.scale_x,
            scale_y: layer.transform.scale_y,
            rotation: match id {
                "num-r" => value,
                _ => layer.transform.rotation,
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

    /// Align the selected layers to the canvas (Properties multi-selection).
    /// `mode`: 0 left, 1 centre horizontally, 2 right, 3 top, 4 centre
    /// vertically, 5 bottom. Locked / position-locked / background members
    /// never move, mirroring `alignTransformEdits`.
    pub(crate) fn align_selected_layers(
        &mut self,
        mode: u8,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc_id = self.active_doc_id()?;
        let (doc_w, doc_h) = self.active_document_size()?;
        let layers = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .map(|content| content.layers.clone())
            .ok_or_else(|| "loaded content is missing".to_string())?;
        let ids = self.selected_core_ids(cx);
        let mut edits = Vec::new();
        for id in ids {
            let Some(layer) = layers.iter().find(|layer| layer.id == id) else {
                continue;
            };
            if layer.locked
                || layer.lock_position == Some(true)
                || layer.is_background == Some(true)
            {
                continue;
            }
            // Align keeps the document-sized layer box; shape-rect alignment is
            // a separate, not-yet-implemented behaviour.
            let layer_w = (layer.width * layer.transform.scale_x).round();
            let layer_h = (layer.height * layer.transform.scale_y).round();
            let (mut x, mut y) = (layer.transform.x, layer.transform.y);
            match mode {
                0 => x = 0.0,
                1 => x = ((doc_w - layer_w) / 2.0).round(),
                2 => x = doc_w - layer_w,
                3 => y = 0.0,
                4 => y = ((doc_h - layer_h) / 2.0).round(),
                5 => y = doc_h - layer_h,
                _ => {}
            }
            if x != layer.transform.x || y != layer.transform.y {
                edits.push((layer.id.clone(), x, y, layer.transform.clone()));
            }
        }
        self.apply_transform_edits(&doc_id, edits, cx)
    }

    /// Evenly distribute the selected layers' spacing along one axis. Keeps
    /// the outermost members fixed.
    pub(crate) fn distribute_selected_layers(
        &mut self,
        horizontal: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc_id = self.active_doc_id()?;
        let layers = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .map(|content| content.layers.clone())
            .ok_or_else(|| "loaded content is missing".to_string())?;
        let mut members: Vec<(String, Transform2D, LayerBounds)> = Vec::new();
        for id in self.selected_core_ids(cx) {
            let Some(layer) = layers.iter().find(|layer| layer.id == id) else {
                continue;
            };
            if layer.locked
                || layer.lock_position == Some(true)
                || layer.is_background == Some(true)
            {
                continue;
            }
            let aabb = crate::app::transform_geometry::layer_aabb(
                &layer.transform,
                layer.width,
                layer.height,
            );
            members.push((layer.id.clone(), layer.transform.clone(), aabb));
        }
        if members.len() < 3 {
            return Ok(());
        }
        members.sort_by(|a, b| {
            if horizontal {
                a.2 .0.total_cmp(&b.2 .0)
            } else {
                a.2 .1.total_cmp(&b.2 .1)
            }
        });
        let first = members[0].2;
        let last = members[members.len() - 1].2;
        let total_span = if horizontal {
            last.0 + last.2 - first.0
        } else {
            last.1 + last.3 - first.1
        };
        let total_extent: f64 = members
            .iter()
            .map(|(_, _, aabb)| if horizontal { aabb.2 } else { aabb.3 })
            .sum();
        let gap = (total_span - total_extent) / (members.len() as f64 - 1.0);
        let mut current = if horizontal { first.0 } else { first.1 };
        let mut edits = Vec::new();
        for (id, start, aabb) in members {
            let delta = if horizontal {
                (current - aabb.0).round()
            } else {
                (current - aabb.1).round()
            };
            current += (if horizontal { aabb.2 } else { aabb.3 }) + gap;
            if delta == 0.0 {
                continue;
            }
            let (x, y) = if horizontal {
                (start.x + delta, start.y)
            } else {
                (start.x, start.y + delta)
            };
            edits.push((id, x, y, start));
        }
        self.apply_transform_edits(&doc_id, edits, cx)
    }

    fn selected_core_ids(&self, cx: &Context<Self>) -> Vec<String> {
        let state = self.state.read(cx);
        state
            .layers
            .iter()
            .filter(|row| state.selected_layers.contains(&row.id))
            .filter_map(|row| row.core_id.clone())
            .collect()
    }

    fn apply_transform_edits(
        &mut self,
        doc_id: &str,
        edits: Vec<(String, f64, f64, Transform2D)>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        for (id, x, y, start) in edits {
            self.apply_core_command(
                doc_id,
                Command::TransformLayer {
                    id,
                    transform: TransformPatch {
                        x,
                        y,
                        scale_x: start.scale_x,
                        scale_y: start.scale_y,
                        rotation: start.rotation,
                        flip_h: Some(start.flip_h),
                        flip_v: Some(start.flip_v),
                    },
                },
                true,
                cx,
            )?;
        }
        Ok(())
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

    fn display_with_transforms(
        &mut self,
        starts: &[GroupMember],
        previews: &[(String, Transform2D)],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc_id = self.active_doc_id()?;
        let (content, region_composite) = {
            let loaded = self
                .sessions
                .get(&doc_id)
                .ok_or_else(|| "active session is missing".to_string())?
                .loaded_document();
            let mut content = loaded
                .content_snapshot()
                .ok_or_else(|| "loaded content is missing".to_string())?
                .clone();
            let mut damage = None;
            let mut can_update_region = starts.len() == previews.len();
            for (index, (id, transform)) in previews.iter().enumerate() {
                let Some(layer) = content.layers.iter_mut().find(|layer| &layer.id == id) else {
                    can_update_region = false;
                    continue;
                };
                let base_transform = match starts.get(index) {
                    Some(member) if &member.id == id => &member.start,
                    _ => {
                        can_update_region = false;
                        &layer.transform
                    }
                };
                if matches!(layer.layer_type, LayerType::Raster) {
                    if let Some(bounds) =
                        layer_damage_bounds(base_transform, transform, layer.width, layer.height)
                    {
                        merge_damage_bounds(&mut damage, bounds);
                    } else {
                        can_update_region = false;
                    }
                } else {
                    can_update_region = false;
                }
                layer.transform = transform.clone();
            }
            let region_composite = if can_update_region {
                damage.and_then(damage_region).and_then(|region| {
                    photrez_core::render::compositor::composite_layers_in_region_with_font_data(
                        &content,
                        &loaded.registry,
                        Some(crate::INTER),
                        region,
                    )
                    .ok()
                })
            } else {
                None
            };
            (content, region_composite)
        };
        if let Some(composite) = region_composite {
            if self.display_region_composite(composite, cx) {
                return Ok(());
            }
        }
        let loaded = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document();
        let composite = photrez_core::render::compositor::composite_layers_with_font_data(
            &content,
            &loaded.registry,
            Some(crate::INTER),
        )
        .map_err(|error| error.to_string())?;
        self.display_composite(composite.width, composite.height, composite.pixels, cx)
    }

    /// Composite the document with one layer's transform overridden for the
    /// live drag; nothing is written to canonical content or history.
    fn display_with_transform(
        &mut self,
        layer_id: &str,
        base_transform: &Transform2D,
        transform: &Transform2D,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc_id = self.active_doc_id()?;
        let (content, region_composite) = {
            let loaded = self
                .sessions
                .get(&doc_id)
                .ok_or_else(|| "active session is missing".to_string())?
                .loaded_document();
            let mut content = loaded
                .content_snapshot()
                .ok_or_else(|| "loaded content is missing".to_string())?
                .clone();
            let layer = content
                .layers
                .iter_mut()
                .find(|layer| layer.id == layer_id)
                .ok_or_else(|| "the transformed layer is missing".to_string())?;
            let damage = matches!(layer.layer_type, LayerType::Raster)
                .then(|| layer_damage_bounds(base_transform, transform, layer.width, layer.height))
                .flatten();
            layer.transform = transform.clone();
            let region_composite = damage.and_then(damage_region).and_then(|region| {
                photrez_core::render::compositor::composite_layers_in_region_with_font_data(
                    &content,
                    &loaded.registry,
                    Some(crate::INTER),
                    region,
                )
                .ok()
            });
            (content, region_composite)
        };
        if let Some(composite) = region_composite {
            if self.display_region_composite(composite, cx) {
                return Ok(());
            }
        }
        let loaded = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document();
        let composite = photrez_core::render::compositor::composite_layers_with_font_data(
            &content,
            &loaded.registry,
            Some(crate::INTER),
        )
        .map_err(|error| error.to_string())?;
        self.display_composite(composite.width, composite.height, composite.pixels, cx)
    }

    fn display_region_composite(
        &mut self,
        composite: photrez_core::render::compositor::CompositedRegionRgba,
        cx: &mut Context<Self>,
    ) -> bool {
        let width = composite.document_width;
        let height = composite.document_height;
        let region = composite.region;
        let Some(active) = self.active_document.as_mut() else {
            return false;
        };
        let tile_size = active.canvas.tile_size().max(1);
        let expected_tiles =
            width.div_ceil(tile_size) as usize * height.div_ceil(tile_size) as usize;
        if active.canvas.dimensions() != (width, height)
            || active.canvas.tile_count() != expected_tiles
        {
            return false;
        }
        if region.width == 0 || region.height == 0 {
            return true;
        }
        let Some(expected_bytes) = (region.width as usize)
            .checked_mul(region.height as usize)
            .and_then(|length| length.checked_mul(4))
        else {
            return false;
        };
        if composite.pixels.len() != expected_bytes {
            return false;
        }
        let Ok(retired) = active.canvas.replace_preview_patch(
            region.x,
            region.y,
            region.width,
            region.height,
            &composite.pixels,
        ) else {
            return false;
        };
        self.retired_textures.extend(retired);
        cx.notify();
        true
    }

    fn display_composite(
        &mut self,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let canvas = crate::render::canvas_bridge::CanvasImage::from_rgba(width, height, pixels)
            .map_err(|error| format!("{error:?}"))?;
        if let Some(active) = self.active_document.as_mut() {
            let retired = std::mem::replace(&mut active.canvas, canvas);
            self.retired_textures.extend(retired.into_textures());
        }
        cx.notify();
        Ok(())
    }

    /// The selected layer regardless of its type, for the transform frame.
    pub(crate) fn selected_layer_for_cursor(
        &self,
        cx: &gpui_kit::App,
    ) -> Result<(String, photrez_core::ptz::content::PtzLayer), String> {
        let doc_id = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
            .ok_or_else(|| "no active document".to_string())?;
        let session = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?;
        let content = session
            .loaded_document()
            .content_snapshot()
            .ok_or_else(|| "loaded content is missing".to_string())?;
        let state = self.state.read(cx);
        if state.selected_layers.is_empty() && state.layers.iter().any(|row| row.core_id.is_some())
        {
            return Err("no layer is selected".to_string());
        }
        let selected = state
            .layers
            .iter()
            .find(|row| row.id == state.selected_layer)
            .and_then(|row| row.core_id.clone());
        let layer = selected
            .as_deref()
            .and_then(|core_id| content.layers.iter().find(|layer| layer.id == core_id))
            .or_else(|| content.layers.first())
            .ok_or_else(|| "the document has no layers".to_string())?
            .clone();
        Ok((doc_id, layer))
    }

    /// Topmost visible, non-background layer under a document-space point.
    /// The layer hit test: text/shape layers are hit by their
    /// whole box, raster layers by in-box alpha above 10%.
    pub(crate) fn layer_hit_at(&self, doc_x: f64, doc_y: f64) -> Option<String> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let session = self.sessions.get(&doc_id)?;
        let loaded = session.loaded_document();
        let content = loaded.content_snapshot()?;
        for layer in &content.layers {
            if !layer.visible || layer.opacity <= 0.0 || layer.is_background == Some(true) {
                continue;
            }
            if layer.width <= 0.0 || layer.height <= 0.0 {
                continue;
            }
            let (frame_w, frame_h) = crate::app::transform_geometry::layer_frame_size(layer);
            let corners = layer_corners(&layer.transform, frame_w, frame_h);
            if !point_in_polygon(doc_x, doc_y, &corners) {
                continue;
            }
            if matches!(layer.layer_type, LayerType::Text | LayerType::Shape) {
                return Some(layer.id.clone());
            }
            let local =
                document_to_layer_local(doc_x, doc_y, &layer.transform, layer.width, layer.height);
            if !local.x.is_finite() || !local.y.is_finite() || local.x < 0.0 || local.y < 0.0 {
                continue;
            }
            let (lx, ly) = (local.x.floor() as usize, local.y.floor() as usize);
            let Some(pixels) = loaded.registry.get_layer(&doc_id, &layer.id) else {
                continue;
            };
            if lx >= pixels.width as usize || ly >= pixels.height as usize {
                continue;
            }
            let index = (ly * pixels.width as usize + lx) * 4;
            let alpha = pixels.pixels.get(index + 3).copied().unwrap_or(0);
            if f32::from(alpha) / 255.0 < 0.1 {
                continue;
            }
            return Some(layer.id.clone());
        }
        None
    }

    /// Move-tool auto-select click. Returns
    /// true when the click changed the selection, in which case no transform
    /// drag starts; clicking the already-active layer keeps the native
    /// drag-to-move interaction.
    pub(crate) fn move_auto_select_click(
        &mut self,
        doc_x: f64,
        doc_y: f64,
        modify: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(core_id) = self.layer_hit_at(doc_x, doc_y) else {
            return false;
        };
        let (row_id, already_selected, already_active) = {
            let state = self.state.read(cx);
            let row_id = state
                .layers
                .iter()
                .find(|row| row.core_id.as_deref() == Some(core_id.as_str()))
                .map(|row| row.id);
            let already_selected = row_id.is_some_and(|row| state.selected_layers.contains(&row));
            (
                row_id,
                already_selected,
                row_id == Some(state.selected_layer),
            )
        };
        let Some(row_id) = row_id else {
            return false;
        };
        if modify {
            self.state.update(cx, |state, _| {
                if let Some(index) = state.selected_layers.iter().position(|id| *id == row_id) {
                    state.selected_layers.remove(index);
                } else {
                    state.selected_layers.push(row_id);
                }
                state.selected_layer = row_id;
            });
            cx.notify();
            return true;
        }
        if already_selected && already_active {
            return false;
        }
        self.state.update(cx, |state, _| {
            state.selected_layer = row_id;
            if !already_selected {
                state.selected_layers = vec![row_id];
            }
        });
        cx.notify();
        true
    }
}

/// Even-odd containment test used by the layer hit test.
fn point_in_polygon(x: f64, y: f64, corners: &[Point; 4]) -> bool {
    let mut inside = false;
    let mut j = corners.len() - 1;
    for i in 0..corners.len() {
        let (xi, yi) = (corners[i].x, corners[i].y);
        let (xj, yj) = (corners[j].x, corners[j].y);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}
