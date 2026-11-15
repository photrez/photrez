//! Crop apply/cancel over the pending frame stored in `AppState.crop_rect`.
//!
//! The drag preview is UI state; the crop itself is the core `ApplyCrop`
//! command (non-destructive: layers are recentered and the document resizes in
//! one history entry). Apply then refits the viewport and returns to Move.

use gpui_kit::Context;
use photrez_core::document::content::LayerType;
use photrez_core::protocol::Command;

pub(crate) use crate::crop_geometry::CropHandle;
use crate::crop_geometry::CropSnapshot;
use crate::state::AppState;
use crate::state::ToolId;

use super::App;

#[derive(Clone, Debug)]
pub(crate) struct CropDrag {
    pub handle: Option<CropHandle>,
    pub rotate: bool,
    pub start_snapshot: CropSnapshot,
    pub start_doc: (f64, f64),
    pub start_screen: (f64, f64),
    pub interaction: crate::crop_geometry::CropInteraction,
    pub snap_targets: Option<std::rc::Rc<crate::crop_geometry::CropSnapTargets>>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CropDragInput {
    pub doc: (f64, f64),
    pub screen: (f64, f64),
    pub mode: crate::crop_aspect::CropMode,
    pub ratio: Option<f64>,
    pub zoom: f64,
    pub doc_size: (f64, f64),
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CropPointerTarget {
    Resize(CropHandle),
    Move,
    Rotate,
}

pub(crate) fn crop_snapshot(state: &AppState) -> Option<CropSnapshot> {
    Some(CropSnapshot {
        frame: state.crop_rect?,
        offset: state.crop_offset,
        rotation: state.crop_rotation,
    })
}

pub(crate) fn restore_crop_snapshot(state: &mut AppState, snapshot: CropSnapshot) {
    state.crop_rect = Some(snapshot.frame);
    state.crop_offset = snapshot.offset;
    state.crop_rotation = snapshot.rotation;
}

pub(crate) fn push_crop_history(state: &mut AppState) {
    if let Some(snapshot) = crop_snapshot(state) {
        state.crop_history.push(snapshot);
        state.crop_redo.clear();
    }
}

pub(crate) fn crop_resize_ratio(state: &AppState) -> Option<f64> {
    (state.crop_interaction == crate::crop_geometry::CropInteraction::Modern
        || state.crop_mode == crate::crop_aspect::CropMode::Ratio)
        .then(|| {
            crate::crop_geometry::locked_ratio(
                state.crop_mode,
                state.crop_aspect,
                state.crop_size_target,
            )
        })
        .flatten()
}

pub(crate) fn crop_snap_targets(app: &App) -> Option<crate::crop_geometry::CropSnapTargets> {
    let document = app.active_document.as_ref()?;
    let content = app
        .sessions
        .get(&document.doc_id)?
        .loaded_document()
        .content_snapshot()?;
    let layer_bounds = content
        .layers
        .iter()
        .filter(|layer| layer.visible)
        .filter_map(|layer| {
            let (width, height) = if layer.layer_type == LayerType::Shape {
                let params = layer.shape_params.as_ref()?;
                let (width, height) = photrez_core::render::shape_raster_dimensions(params)?;
                (f64::from(width), f64::from(height))
            } else {
                crate::app::transform_geometry::layer_frame_size(layer)
            };
            Some(crate::app::transform_geometry::layer_aabb(
                &layer.transform,
                width,
                height,
            ))
        });
    Some(crate::crop_geometry::crop_snap_targets(
        content.width,
        content.height,
        layer_bounds,
    ))
}

pub(crate) fn crop_drag_update(drag: &CropDrag, input: CropDragInput) -> CropSnapshot {
    let CropDragInput {
        doc,
        screen,
        mode,
        ratio,
        zoom,
        doc_size,
        shift,
        alt,
    } = input;
    let start = drag.start_snapshot;
    let zoom = zoom.max(0.01);
    if drag.rotate {
        let center = (
            start.frame[0] + start.frame[2] / 2.0,
            start.frame[1] + start.frame[3] / 2.0,
        );
        let angle = |point: (f64, f64)| {
            (point.1 - center.1).atan2(point.0 - center.0) * 180.0 / std::f64::consts::PI
        };
        let rotation = crop_rotation_after_drag(
            start.rotation + angle(doc) - angle(drag.start_doc),
            drag.interaction,
            shift,
        );
        return CropSnapshot {
            frame: start.frame,
            offset: start.offset,
            rotation,
        };
    }
    match drag.interaction {
        crate::crop_geometry::CropInteraction::Modern => {
            if let Some(handle) = drag.handle {
                let (frame, compensation) = crate::crop_geometry::modern_resize(
                    start.frame,
                    handle,
                    (doc.0 - drag.start_doc.0, doc.1 - drag.start_doc.1),
                    crate::crop_geometry::CropResizeOptions {
                        mode,
                        ratio,
                        shift,
                        alt,
                    },
                    crate::crop_geometry::MODERN_MIN_CROP_SCREEN_SIZE / zoom,
                );
                let offset = crate::crop_geometry::inverse_rotate_delta(
                    (compensation.0 * zoom, compensation.1 * zoom),
                    start.rotation,
                );
                CropSnapshot {
                    frame,
                    offset: (start.offset.0 + offset.0, start.offset.1 + offset.1),
                    rotation: start.rotation,
                }
            } else {
                let offset = crate::crop_geometry::inverse_rotate_delta(
                    (
                        screen.0 - drag.start_screen.0,
                        screen.1 - drag.start_screen.1,
                    ),
                    start.rotation,
                );
                CropSnapshot {
                    frame: start.frame,
                    offset: (start.offset.0 + offset.0, start.offset.1 + offset.1),
                    rotation: start.rotation,
                }
            }
        }
        crate::crop_geometry::CropInteraction::Classic => {
            let frame = if let Some(handle) = drag.handle {
                let delta = crate::crop_geometry::inverse_rotate_delta(
                    (doc.0 - drag.start_doc.0, doc.1 - drag.start_doc.1),
                    start.rotation,
                );
                let mut frame = crate::crop_geometry::classic_resize(
                    start.frame,
                    handle,
                    delta,
                    crate::crop_geometry::CropResizeOptions {
                        mode,
                        ratio,
                        shift,
                        alt,
                    },
                );
                if start.rotation != 0.0 && !alt {
                    let old_center = (
                        start.frame[0] + start.frame[2] / 2.0,
                        start.frame[1] + start.frame[3] / 2.0,
                    );
                    let old_anchor = crop_handle_anchor(handle, start.frame[2], start.frame[3]);
                    let new_anchor = crop_handle_anchor(handle, frame[2], frame[3]);
                    let shift = crate::crop_geometry::inverse_rotate_delta(
                        (old_anchor.0 - new_anchor.0, old_anchor.1 - new_anchor.1),
                        -start.rotation,
                    );
                    frame[0] = old_center.0 + shift.0 - frame[2] / 2.0;
                    frame[1] = old_center.1 + shift.1 - frame[3] / 2.0;
                }
                frame
            } else {
                App::crop_drag_rect(drag, doc, doc_size.0, doc_size.1)
            };
            CropSnapshot {
                frame,
                offset: start.offset,
                rotation: start.rotation,
            }
        }
    }
}

fn crop_snap_rotation(rotation: f64) -> f64 {
    ((rotation / 15.0) + 0.5).floor() * 15.0
}

pub(crate) fn crop_rotation_after_drag(
    rotation: f64,
    interaction: crate::crop_geometry::CropInteraction,
    shift: bool,
) -> f64 {
    match interaction {
        crate::crop_geometry::CropInteraction::Modern => {
            let rotation = crate::app::transform_geometry::normalize_rotation(rotation);
            if shift {
                crop_snap_rotation(rotation)
            } else {
                rotation
            }
        }
        crate::crop_geometry::CropInteraction::Classic => {
            let rotation = if shift {
                crop_snap_rotation(rotation)
            } else {
                rotation
            };
            crate::app::transform_geometry::normalize_rotation(rotation)
        }
    }
}

fn crop_handle_anchor(handle: CropHandle, width: f64, height: f64) -> (f64, f64) {
    match handle {
        CropHandle::Nw => (width / 2.0, height / 2.0),
        CropHandle::Ne => (-width / 2.0, height / 2.0),
        CropHandle::Se => (-width / 2.0, -height / 2.0),
        CropHandle::Sw => (width / 2.0, -height / 2.0),
        CropHandle::N => (0.0, height / 2.0),
        CropHandle::S => (0.0, -height / 2.0),
        CropHandle::E => (-width / 2.0, 0.0),
        CropHandle::W => (width / 2.0, 0.0),
    }
}

const HANDLE_HIT_PX: f64 = 28.0;
const ROTATE_BAND_PX: f64 = 100.0;
const ROTATE_CORNER_EXTRA_PX: f64 = 2.0;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct ApplyCropAction;

macro_rules! nudge_action {
    ($name:ident, $method:ident, $dx:expr, $dy:expr) => {
        #[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
        #[action(namespace = photrez, no_json)]
        pub(crate) struct $name;

        impl App {
            pub(crate) fn $method(&mut self, _: &$name, cx: &mut Context<Self>) {
                if self.state.read(cx).open_select.is_some() {
                    self.select_arrow(if $dy > 0.0 { 1 } else { -1 }, cx);
                    return;
                }
                self.nudge_crop($dx, $dy, cx);
            }
        }
    };
}

nudge_action!(NudgeCropLeft, nudge_crop_left, -1.0, 0.0);
nudge_action!(NudgeCropRight, nudge_crop_right, 1.0, 0.0);
nudge_action!(NudgeCropUp, nudge_crop_up, 0.0, -1.0);
nudge_action!(NudgeCropDown, nudge_crop_down, 0.0, 1.0);
nudge_action!(NudgeCropLeftLarge, nudge_crop_left_large, -10.0, 0.0);
nudge_action!(NudgeCropRightLarge, nudge_crop_right_large, 10.0, 0.0);
nudge_action!(NudgeCropUpLarge, nudge_crop_up_large, 0.0, -10.0);
nudge_action!(NudgeCropDownLarge, nudge_crop_down_large, 0.0, 10.0);

impl App {
    pub(crate) fn create_modern_crop_from_drag(
        &mut self,
        start: (f64, f64),
        end: (f64, f64),
        shift: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (viewport_w, viewport_h) = self
            .viewport_size()
            .ok_or_else(|| "the crop viewport is unavailable".to_string())?;
        let camera = self
            .effective_camera()
            .ok_or_else(|| "the crop viewport is unavailable".to_string())?;
        let (mode, aspect, size_target, rotation) = {
            let state = self.state.read(cx);
            (
                state.crop_mode,
                state.crop_aspect,
                state.crop_size_target,
                state.crop_rotation,
            )
        };
        let snapshot =
            crate::crop_geometry::modern_crop_create(crate::crop_geometry::ModernCropCreateInput {
                start,
                end,
                viewport: (f64::from(viewport_w), f64::from(viewport_h)),
                camera: crate::crop_geometry::CropViewport {
                    pan_x: f64::from(camera.x),
                    pan_y: f64::from(camera.y),
                    zoom: f64::from(camera.zoom),
                },
                mode,
                aspect,
                size_target,
                shift,
                rotation,
            });
        self.state.update(cx, |state, _| {
            restore_crop_snapshot(state, snapshot);
            state.crop_hidden_preview = None;
        });
        cx.notify();
        Ok(())
    }

    pub(crate) fn create_default_modern_crop(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (viewport_w, viewport_h) = self
            .viewport_size()
            .ok_or_else(|| "the crop viewport is unavailable".to_string())?;
        let (doc_w, doc_h) = self.active_document_size()?;
        let camera = self
            .effective_camera()
            .ok_or_else(|| "the crop viewport is unavailable".to_string())?;
        let camera = crate::display::canvas_bridge::Camera::centred_at(
            viewport_w,
            viewport_h,
            doc_w as f32,
            doc_h as f32,
            camera.zoom,
        )
        .ok_or_else(|| "the crop viewport is unavailable".to_string())?;
        let aspect = {
            let state = self.state.read(cx);
            match state.crop_mode {
                crate::crop_aspect::CropMode::Free => None,
                crate::crop_aspect::CropMode::Ratio => state.crop_aspect,
                crate::crop_aspect::CropMode::Size => state.crop_size_target,
            }
        };
        let frame = crate::crop_geometry::default_modern_crop_frame(
            (f64::from(viewport_w), f64::from(viewport_h)),
            (doc_w, doc_h),
            f64::from(camera.zoom),
            (f64::from(camera.x), f64::from(camera.y)),
            aspect,
        );
        self.set_camera(camera, cx);
        self.state.update(cx, |state, _| {
            state.crop_rect = Some(frame);
            state.crop_hidden_preview = None;
            state.crop_offset = (0.0, 0.0);
            state.crop_rotation = 0.0;
        });
        cx.notify();
        Ok(())
    }

    /// Crop bar `Delete`: discard cropped-away pixels on Apply.
    pub(crate) fn toggle_crop_delete(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.crop_delete_pixels = !state.crop_delete_pixels;
        });
        cx.notify();
    }

    /// Crop bar `Fill BG`: bake the background colour under the crop on Apply.
    pub(crate) fn toggle_crop_fill(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.crop_fill_background = !state.crop_fill_background;
        });
        cx.notify();
    }

    pub(crate) fn set_crop_interaction(
        &mut self,
        interaction: crate::crop_geometry::CropInteraction,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| {
            state.crop_interaction = interaction;
        });
        cx.notify();
    }

    pub(crate) fn set_crop_rotation(&mut self, rotation: f64, cx: &mut Context<Self>) {
        if !rotation.is_finite() {
            return;
        }
        let changed = self.state.update(cx, |state, _| {
            if state.crop_rotation == rotation {
                return false;
            }
            if state.crop_rect.is_some() {
                push_crop_history(state);
            } else {
                state.crop_redo.clear();
            }
            state.crop_rotation = rotation;
            true
        });
        if changed {
            cx.notify();
        }
    }

    pub(crate) fn nudge_crop_rotation(&mut self, delta: f64, cx: &mut Context<Self>) {
        let current = self.state.read(cx).crop_rotation;
        if !current.is_finite() || !delta.is_finite() {
            return;
        }
        let rotation = ((current + delta).clamp(-45.0, 45.0) * 10.0).round() / 10.0;
        self.set_crop_rotation(rotation, cx);
    }

    pub(crate) fn rotate_crop_by(&mut self, delta: f64, cx: &mut Context<Self>) {
        if !delta.is_finite() || delta == 0.0 {
            return;
        }
        let changed = self.state.update(cx, |state, _| {
            let rotation = state.crop_rotation + delta;
            if !rotation.is_finite() {
                return false;
            }

            let frame = state.crop_rect;
            let next_frame = match (state.crop_interaction, frame) {
                (crate::crop_geometry::CropInteraction::Classic, Some([x, y, w, h])) => {
                    let center_x = x + w / 2.0;
                    let center_y = y + h / 2.0;
                    let rotated = [center_x - h / 2.0, center_y - w / 2.0, h, w];
                    if !rotated.iter().all(|value| value.is_finite()) {
                        return false;
                    }
                    Some(rotated)
                }
                (_, frame) => frame,
            };

            if frame.is_some() {
                push_crop_history(state);
            } else {
                state.crop_redo.clear();
            }
            state.crop_rect = next_frame;
            state.crop_rotation = rotation;
            true
        });
        if changed {
            cx.notify();
        }
    }

    pub(crate) fn apply_crop_action(&mut self, _: &ApplyCropAction, cx: &mut Context<Self>) {
        if self.state.read(cx).open_select.is_some() {
            self.select_confirm(cx);
            return;
        }
        let result = self.apply_crop(cx);
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(format!("Crop: {error}")), cx),
        }
    }

    pub(crate) fn apply_crop(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let tool = self.state.read(cx).active_tool;
        if tool != ToolId::Crop {
            return Err("the crop tool is not active".to_string());
        }
        if self.pending_gesture.as_ref().is_some_and(|pending| {
            pending.crop_modern_create
                && (pending.crop_modern_drag_started || pending.crop_start.is_none())
        }) {
            return Ok(());
        }
        let (rect, interaction, offset, rotation, mode, size_target) = {
            let state = self.state.read(cx);
            (
                state
                    .crop_rect
                    .ok_or_else(|| "there is no crop rectangle".to_string())?,
                state.crop_interaction,
                state.crop_offset,
                state.crop_rotation,
                state.crop_mode,
                state.crop_size_target,
            )
        };
        let target = if mode == crate::crop_aspect::CropMode::Size {
            let (width, height) =
                size_target.ok_or_else(|| "the crop Size target is missing".to_string())?;
            Some((f64::from(width), f64::from(height)))
        } else {
            None
        };
        let rect = match interaction {
            crate::crop_geometry::CropInteraction::Modern => {
                let camera = self
                    .effective_camera()
                    .ok_or_else(|| "the crop viewport is unavailable".to_string())?;
                crate::crop_geometry::crop_apply_region(
                    interaction,
                    rect,
                    offset,
                    crate::crop_geometry::CropViewport {
                        pan_x: f64::from(camera.x),
                        pan_y: f64::from(camera.y),
                        zoom: f64::from(camera.zoom),
                    },
                )
            }
            crate::crop_geometry::CropInteraction::Classic => rect,
        };
        if !rect.iter().all(|value| value.is_finite()) {
            return Err("the crop rectangle must contain finite values".to_string());
        }
        let width = rect[2].round().max(1.0);
        let height = rect[3].round().max(1.0);
        if width < 1.0 || height < 1.0 {
            return Err("the crop rectangle is empty".to_string());
        }
        let doc_id = self.active_doc_id()?;
        let (delete_pixels, fill_background) = {
            let state = self.state.read(cx);
            (
                state.crop_delete_pixels,
                if state.crop_fill_background {
                    Some(state.bg_color)
                } else {
                    None
                },
            )
        };
        let request = crate::crop_geometry::CropRequest {
            x: rect[0].round(),
            y: rect[1].round(),
            width,
            height,
            rotation: Some(match interaction {
                crate::crop_geometry::CropInteraction::Modern => {
                    crate::crop_geometry::modern_apply_rotation(rotation)
                }
                crate::crop_geometry::CropInteraction::Classic => rotation,
            }),
            target,
            delete_pixels,
            fill_background,
        };
        #[cfg(test)]
        self.state.update(cx, |state, _| {
            state.last_crop_flags = Some((delete_pixels, fill_background));
        });
        self.apply_core_command(
            &doc_id,
            Command::ApplyCrop {
                x: request.x,
                y: request.y,
                width: request.width,
                height: request.height,
                rotation: request.rotation,
                target_width: request.target.map(|(width, _)| width),
                target_height: request.target.map(|(_, height)| height),
                delete_cropped_pixels: request.delete_pixels,
                fill_background: request.fill_background,
            },
            true,
            cx,
        )?;
        self.crop_straighten_document_id = None;
        self.state.update(cx, |state, _| {
            state.crop_rect = None;
            state.crop_hidden_preview = None;
            state.crop_history.clear();
            state.crop_redo.clear();
            state.crop_offset = (0.0, 0.0);
            state.crop_rotation = 0.0;
            state.selection_drag = None;
            state.active_tool = ToolId::Move;
        });
        // Applying the crop refits the viewport and leaves no active selection.
        if let Some(document) = self.active_document.as_mut() {
            document.camera = None;
        }
        cx.notify();
        Ok(())
    }

    /// Crop-tool Cancel: discard the frame and its step history.
    pub(crate) fn cancel_crop(&mut self, cx: &mut Context<Self>) {
        self.crop_straighten_document_id = None;
        self.state.update(cx, |state, _| {
            state.crop_rect = None;
            state.crop_hidden_preview = None;
            state.crop_history.clear();
            state.crop_redo.clear();
            state.crop_offset = (0.0, 0.0);
            state.crop_rotation = 0.0;
            state.selection_drag = None;
        });
        cx.notify();
    }

    /// Crop-tool Ctrl+Z: restore the previous frame, keeping a redo step.
    pub(crate) fn crop_undo(&mut self, cx: &mut Context<Self>) -> bool {
        let updated = self.state.update(cx, |state, _| {
            let Some(previous) = state.crop_history.pop() else {
                return false;
            };
            if let Some(current) = crop_snapshot(state) {
                state.crop_redo.push(current);
            }
            restore_crop_snapshot(state, previous);
            true
        });
        if updated {
            cx.notify();
        }
        updated
    }

    pub(crate) fn crop_redo(&mut self, cx: &mut Context<Self>) -> bool {
        let updated = self.state.update(cx, |state, _| {
            let Some(next) = state.crop_redo.pop() else {
                return false;
            };
            if let Some(current) = crop_snapshot(state) {
                state.crop_history.push(current);
            }
            restore_crop_snapshot(state, next);
            true
        });
        if updated {
            cx.notify();
        }
        updated
    }

    /// Which crop handle (or the frame interior) the point hits.
    pub(crate) fn crop_handle_at(
        doc: (f64, f64),
        rect: [f64; 4],
        zoom: f64,
    ) -> Option<(CropHandle, bool)> {
        let half_hit = HANDLE_HIT_PX / (2.0 * zoom.max(0.01));
        let [x, y, w, h] = rect;
        for (handle, cx, cy) in [
            (CropHandle::Nw, x, y),
            (CropHandle::N, x + w / 2.0, y),
            (CropHandle::Ne, x + w, y),
            (CropHandle::E, x + w, y + h / 2.0),
            (CropHandle::Se, x + w, y + h),
            (CropHandle::S, x + w / 2.0, y + h),
            (CropHandle::Sw, x, y + h),
            (CropHandle::W, x, y + h / 2.0),
        ] {
            if (doc.0 - cx).abs() <= half_hit && (doc.1 - cy).abs() <= half_hit {
                return Some((handle, false));
            }
        }
        if doc.0 > x && doc.0 < x + w && doc.1 > y && doc.1 < y + h {
            return Some((CropHandle::Nw, true));
        }
        None
    }

    pub(crate) fn crop_pointer_target_at(
        doc: (f64, f64),
        rect: [f64; 4],
        zoom: f64,
        interaction: crate::crop_geometry::CropInteraction,
        rotation: f64,
    ) -> Option<CropPointerTarget> {
        let local = if interaction == crate::crop_geometry::CropInteraction::Classic {
            let center = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
            let delta = crate::crop_geometry::inverse_rotate_delta(
                (doc.0 - center.0, doc.1 - center.1),
                rotation,
            );
            (center.0 + delta.0, center.1 + delta.1)
        } else {
            doc
        };
        if let Some((handle, interior)) = Self::crop_handle_at(local, rect, zoom) {
            return Some(if interior {
                CropPointerTarget::Move
            } else {
                CropPointerTarget::Resize(handle)
            });
        }
        Self::crop_rotate_band_contains(local, rect, zoom, interaction)
            .then_some(CropPointerTarget::Rotate)
    }

    fn crop_rotate_band_contains(
        doc: (f64, f64),
        rect: [f64; 4],
        zoom: f64,
        interaction: crate::crop_geometry::CropInteraction,
    ) -> bool {
        let [x, y, w, h] = rect;
        let scale = zoom.max(0.01);
        let (pad, radius) = match interaction {
            crate::crop_geometry::CropInteraction::Modern => (ROTATE_BAND_PX / scale, 0.0),
            crate::crop_geometry::CropInteraction::Classic => (
                (ROTATE_BAND_PX + ROTATE_CORNER_EXTRA_PX) / scale,
                ROTATE_CORNER_EXTRA_PX / scale,
            ),
        };
        let left = x - pad;
        let top = y - pad;
        let right = x + w + pad;
        let bottom = y + h + pad;
        let in_outer_bounds = doc.0 >= left && doc.0 <= right && doc.1 >= top && doc.1 <= bottom;
        if !in_outer_bounds || (doc.0 >= x && doc.0 <= x + w && doc.1 >= y && doc.1 <= y + h) {
            return false;
        }
        if radius == 0.0 {
            return true;
        }
        let nearest_x = doc.0.clamp(left + radius, right - radius);
        let nearest_y = doc.1.clamp(top + radius, bottom - radius);
        (doc.0 - nearest_x).powi(2) + (doc.1 - nearest_y).powi(2) <= radius.powi(2)
    }

    /// Classic moves may extend beyond the document; resize handles stay clamped.
    pub(crate) fn crop_drag_rect(
        drag: &CropDrag,
        doc: (f64, f64),
        doc_w: f64,
        doc_h: f64,
    ) -> [f64; 4] {
        let [sx, sy, sw, sh] = drag.start_snapshot.frame;
        let Some(handle) = drag.handle else {
            // No handle grabbed: the whole rect follows the pointer move.
            let dx = doc.0 - drag.start_doc.0;
            let dy = doc.1 - drag.start_doc.1;
            return [sx + dx, sy + dy, sw, sh];
        };
        let mut left = sx;
        let mut top = sy;
        let mut right = sx + sw;
        let mut bottom = sy + sh;
        if matches!(handle, CropHandle::Nw | CropHandle::W | CropHandle::Sw) {
            left = (sx + doc.0 - drag.start_doc.0).clamp(0.0, right - 1.0);
        }
        if matches!(handle, CropHandle::Ne | CropHandle::E | CropHandle::Se) {
            right = (sx + sw + doc.0 - drag.start_doc.0).clamp(left + 1.0, doc_w);
        }
        if matches!(handle, CropHandle::Nw | CropHandle::N | CropHandle::Ne) {
            top = (sy + doc.1 - drag.start_doc.1).clamp(0.0, bottom - 1.0);
        }
        if matches!(handle, CropHandle::Sw | CropHandle::S | CropHandle::Se) {
            bottom = (sy + sh + doc.1 - drag.start_doc.1).clamp(top + 1.0, doc_h);
        }
        [left, top, right - left, bottom - top]
    }

    #[cfg(test)]
    pub(crate) fn crop_rect_for_test(&self, cx: &gpui_kit::App) -> Option<[f64; 4]> {
        self.state.read(cx).crop_rect
    }

    /// Arrow keys move the pending crop frame, clamped to the document
    /// (step 1, Shift step 10). Other tools ignore the keys.
    fn nudge_crop(&mut self, dx: f64, dy: f64, cx: &mut Context<Self>) {
        let tool = self.state.read(cx).active_tool;
        if tool == ToolId::Move {
            let result = self.nudge_selected_layer(dx, dy, cx);
            match result {
                Ok(()) => self.set_status_error(None, cx),
                Err(error) => self.set_status_error(Some(format!("Nudge: {error}")), cx),
            }
            return;
        }
        if tool != ToolId::Crop {
            return;
        }
        let Some(rect) = self.state.read(cx).crop_rect else {
            return;
        };
        let Ok((doc_w, doc_h)) = self.active_document_size() else {
            return;
        };
        let max_x = (doc_w - rect[2]).max(0.);
        let max_y = (doc_h - rect[3]).max(0.);
        let next = [
            (rect[0] + dx).clamp(0., max_x),
            (rect[1] + dy).clamp(0., max_y),
            rect[2],
            rect[3],
        ];
        self.state.update(cx, |state, _| {
            push_crop_history(state);
            state.crop_rect = Some(next);
        });
        cx.notify();
    }
}

impl App {
    /// Crop bar ratio trigger: the list expands inline, like the language list
    /// (a deferred popup inside a canvas overlay never receives hits).
    pub(crate) fn toggle_crop_ratio(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.crop_ratio_open = !state.crop_ratio_open;
        });
        cx.notify();
    }

    /// Free mode: the frame stops following a ratio; its rect stays.
    pub(crate) fn crop_choose_free(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.crop_mode = crate::crop_aspect::CropMode::Free;
            state.crop_aspect = None;
            state.crop_ratio_open = false;
        });
        cx.notify();
    }

    /// A preset, a recent, or `Lock Current Shape`: refit the pending frame to
    /// this aspect inside the document and remember it in the recents.
    pub(crate) fn crop_choose_ratio(&mut self, aspect: (u32, u32), cx: &mut Context<Self>) {
        if aspect.0 == 0 || aspect.1 == 0 {
            return;
        }
        let fitted = self.active_document_size().ok().map(|(doc_w, doc_h)| {
            crate::crop_aspect::fit_crop_rect_to_aspect(aspect, doc_w, doc_h, 0.0)
        });
        self.state.update(cx, |state, _| {
            state.crop_mode = crate::crop_aspect::CropMode::Ratio;
            state.crop_aspect = Some(aspect);
            state.crop_ratio_open = false;
            push_recent(&mut state.recent_crop_ratios, aspect);
            if let Some(rect) = fitted {
                push_crop_history(state);
                state.crop_rect = Some(rect);
            }
        });
        cx.notify();
    }

    /// Size mode: the Size fields drive a pixel target.
    pub(crate) fn crop_choose_size(&mut self, cx: &mut Context<Self>) {
        let (target, unit) = {
            let state = self.state.read(cx);
            (
                state.crop_size_target.unwrap_or((800, 600)),
                state.crop_size_unit,
            )
        };
        let fitted = self.active_document_size().ok().map(|(doc_w, doc_h)| {
            crate::crop_aspect::fit_crop_rect_to_aspect(target, doc_w, doc_h, 0.0)
        });
        self.state.update(cx, |state, _| {
            state.crop_mode = crate::crop_aspect::CropMode::Size;
            state.crop_size_target = Some(target);
            state.crop_size_w = format_unit(target.0, unit);
            state.crop_size_h = format_unit(target.1, unit);
            state.crop_ratio_open = false;
            if let Some(rect) = fitted {
                push_crop_history(state);
                state.crop_rect = Some(rect);
            }
        });
        cx.notify();
    }

    /// Lock Shape: keep the pending frame's own proportions as a custom ratio.
    pub(crate) fn crop_lock_shape(&mut self, cx: &mut Context<Self>) {
        let (rect, mode) = {
            let state = self.state.read(cx);
            (state.crop_rect, state.crop_mode)
        };
        if mode != crate::crop_aspect::CropMode::Free {
            return;
        }
        let Some(rect) = rect else { return };
        let w = rect[2].round() as u32;
        let h = rect[3].round() as u32;
        if w == 0 || h == 0 {
            return;
        }
        self.crop_choose_ratio((w, h), cx);
    }

    /// The Size units re-format the display buffers from the pixel target; the
    /// pixel value stays authoritative.
    pub(crate) fn crop_set_size_unit(
        &mut self,
        unit: crate::crop_aspect::CropSizeUnit,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| {
            state.crop_size_unit = unit;
            if let Some((w, h)) = state.crop_size_target {
                state.crop_size_w = format_unit(w, unit);
                state.crop_size_h = format_unit(h, unit);
            }
        });
        cx.notify();
    }

    /// Commit one Size field: convert from its unit at 300 PPI, then refit
    /// the frame.
    pub(crate) fn crop_set_size_value(&mut self, field: usize, text: &str, cx: &mut Context<Self>) {
        let parsed: f64 = match text.trim().parse() {
            Ok(value) => value,
            Err(_) => return,
        };
        if !parsed.is_finite() || parsed <= 0.0 {
            return;
        }
        let (unit, mut target) = {
            let state = self.state.read(cx);
            (
                state.crop_size_unit,
                state.crop_size_target.unwrap_or((800, 600)),
            )
        };
        let px = unit.to_pixels(parsed).round().max(1.0) as u32;
        if field == 0 {
            target.0 = px;
        } else {
            target.1 = px;
        }
        let fitted = self.active_document_size().ok().map(|(doc_w, doc_h)| {
            crate::crop_aspect::fit_crop_rect_to_aspect(target, doc_w, doc_h, 0.0)
        });
        self.state.update(cx, |state, _| {
            state.crop_size_target = Some(target);
            state.crop_size_field = field.min(1);
            if field == 0 {
                state.crop_size_w = text.trim().to_string();
            } else {
                state.crop_size_h = text.trim().to_string();
            }
            if let Some(rect) = fitted {
                push_crop_history(state);
                state.crop_rect = Some(rect);
            }
        });
        cx.notify();
    }

    /// Swap in Size mode: exchange the two target dimensions.
    pub(crate) fn crop_swap_size(&mut self, cx: &mut Context<Self>) {
        let (mut target, w_text, h_text) = {
            let state = self.state.read(cx);
            (
                state.crop_size_target.unwrap_or((800, 600)),
                state.crop_size_w.clone(),
                state.crop_size_h.clone(),
            )
        };
        target = (target.1, target.0);
        let fitted = self.active_document_size().ok().map(|(doc_w, doc_h)| {
            crate::crop_aspect::fit_crop_rect_to_aspect(target, doc_w, doc_h, 0.0)
        });
        self.state.update(cx, |state, _| {
            state.crop_size_target = Some(target);
            state.crop_size_w = h_text;
            state.crop_size_h = w_text;
            if let Some(rect) = fitted {
                push_crop_history(state);
                state.crop_rect = Some(rect);
            }
        });
        cx.notify();
    }
}

impl App {
    /// Start editing one of the Size or Custom fields (click on the field).
    pub(crate) fn crop_begin_edit(&mut self, field: usize, custom: bool, cx: &mut Context<Self>) {
        // The edit buffer starts empty, so the first typed digit replaces the
        // shown value rather than appending to it.
        self.state.update(cx, |state, _| {
            state.crop_edit = Some(String::new());
            state.crop_edit_field = if custom {
                crate::crop_geometry::CropField::Custom
            } else {
                crate::crop_geometry::CropField::Size
            };
            state.crop_size_field = field.min(1);
        });
        cx.notify();
    }

    /// Keys while a Size/Custom buffer is open: digits, one decimal point for
    /// physical units, Backspace, Enter to commit, Escape to cancel.
    pub(crate) fn crop_edit_key(
        &mut self,
        event: &gpui_kit::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        let (unit, custom) = {
            let state = self.state.read(cx);
            (
                state.crop_size_unit,
                state.crop_edit_field == crate::crop_geometry::CropField::Custom,
            )
        };
        let Some(mut buffer) = self.state.read(cx).crop_edit.clone() else {
            return false;
        };
        match key {
            "enter" => {
                self.state.update(cx, |state, _| state.crop_edit = None);
                cx.notify();
                true
            }
            "escape" => {
                self.state.update(cx, |state, _| state.crop_edit = None);
                cx.notify();
                true
            }
            "backspace" => {
                buffer.pop();
                self.state
                    .update(cx, |state, _| state.crop_edit = Some(buffer));
                cx.notify();
                true
            }
            "tab" => {
                let next = 1 - self.state.read(cx).crop_size_field.min(1);
                self.crop_begin_edit(next, custom, cx);
                true
            }
            _ => {
                let text = event
                    .keystroke
                    .key_char
                    .clone()
                    .unwrap_or_else(|| event.keystroke.key.clone());
                let decimal = text == "." && (custom || unit.accepts_decimal());
                if !text.chars().all(|c| c.is_ascii_digit()) && !decimal {
                    return true;
                }
                if decimal && buffer.contains('.') {
                    return true;
                }
                buffer.push_str(&text);
                self.state
                    .update(cx, |state, _| state.crop_edit = Some(buffer.clone()));
                let field = self.state.read(cx).crop_size_field;
                if custom {
                    self.crop_set_custom_value(field, &buffer, cx);
                } else {
                    self.crop_set_size_value(field, &buffer, cx);
                }
                cx.notify();
                true
            }
        }
    }

    /// Commit one Custom W/H value: it becomes the ratio and the frame refits.
    pub(crate) fn crop_set_custom_value(
        &mut self,
        field: usize,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let Ok(value) = text.trim().parse::<u32>() else {
            return;
        };
        if value == 0 {
            return;
        }
        let current = self.state.read(cx).crop_aspect.unwrap_or((1, 1));
        let aspect = if field == 0 {
            (value, current.1)
        } else {
            (current.0, value)
        };
        self.crop_choose_ratio(aspect, cx);
    }
}

impl App {
    /// `Custom...` row: stay in Ratio mode and open the W:H field pair.
    pub(crate) fn crop_choose_custom(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.crop_mode = crate::crop_aspect::CropMode::Ratio;
            state.crop_aspect = state.crop_aspect.or(Some((1, 1)));
            state.crop_ratio_open = false;
            state.crop_edit = Some(
                state
                    .crop_aspect
                    .map(|(w, _)| w.to_string())
                    .unwrap_or_else(|| "1".to_string()),
            );
            state.crop_edit_field = crate::crop_geometry::CropField::Custom;
            state.crop_size_field = 0;
        });
        cx.notify();
    }

    pub(crate) fn toggle_crop_unit(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.crop_unit_open = !state.crop_unit_open;
        });
        cx.notify();
    }
}

/// The recent-ratio list: newest first, de-duplicated, at most three.
fn push_recent(recents: &mut Vec<(u32, u32)>, aspect: (u32, u32)) {
    recents.retain(|existing| *existing != aspect);
    recents.insert(0, aspect);
    recents.truncate(3);
}

fn format_unit(px: u32, unit: crate::crop_aspect::CropSizeUnit) -> String {
    let value = unit.to_unit(f64::from(px));
    if unit.accepts_decimal() {
        format!("{value:.2}")
    } else {
        format!("{}", value.round() as i64)
    }
}
