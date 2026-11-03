//! Canvas pointer events into the native input chain.
//!
//! This is the production path: the hitbox's gpui mouse events reach these
//! methods, which map window coordinates through the same fit placement the
//! display seam paints, then mutate the loaded core document.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{
    Bounds, Context, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Window,
};
use photrez_core::paint::raster::RasterTip;
use photrez_core::pixel::tile::TilePatch;
use photrez_core::protocol::{Command, CommandEnvelope, TransformPatch, CONTRACT_VERSION};
use photrez_core::ptz::content::{
    LayerType, PtzLayer, SelectionShape as CoreSelectionShape, SelectionState,
};
use photrez_core::ptz::layer_params::{
    ShapeFill, ShapeFillKind, ShapeKind, ShapeParams, ShapeStroke, TextData,
};

use super::transform_move::TransformDragUpdate;
use super::App;
use crate::input::{brush, BrushTarget, CursorContext, CursorPolicy, PaintTarget, PendingGesture};
use crate::render::canvas_bridge::CanvasImage;
use crate::state::{SelectionShape, ToolId};

/// The selected layer's transform record as the properties panel reads it:
/// (layer id, x, y, scale-x, scale-y, rotation, flip-h, flip-v).
type ActiveLayerTransform = (String, f64, f64, f64, f64, f64, bool, bool);

#[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct SetCanvasTool(pub ToolId);

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct CancelCanvasGesture;

/// Alt+Right brush adjustment: the size/hardness and screen origin captured
/// when the drag starts. Live deltas follow the 0.5%/px size and
/// 0.004/px hardness curves the paint tools are specified with.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BrushAdjust {
    pub eraser: bool,
    pub start_size: f64,
    pub start_hardness: f64,
    pub start_screen: (f32, f32),
}

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct ToggleSelectionShape;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct DecreaseBrushSize;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct IncreaseBrushSize;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct DecreaseBrushHardness;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct IncreaseBrushHardness;

impl App {
    pub(crate) fn set_canvas_tool_action(
        &mut self,
        action: &SetCanvasTool,
        cx: &mut Context<Self>,
    ) {
        self.select_canvas_tool(action.0, cx);
    }

    pub(crate) fn cancel_canvas_gesture_action(
        &mut self,
        _: &CancelCanvasGesture,
        cx: &mut Context<Self>,
    ) {
        let (
            brush_menu,
            context_menu,
            open_select,
            more_open,
            active_tool,
            stroke_popover,
            text_stroke_popover,
        ) = {
            let state = self.state.read(cx);
            (
                state.brush_menu.is_some(),
                state.context_menu.is_some(),
                state.open_select.is_some(),
                state.more_open,
                state.active_tool,
                state.stroke_popover_open,
                state.text_stroke_popover_open,
            )
        };
        if brush_menu {
            self.close_brush_menu(cx);
            return;
        }
        if context_menu {
            self.close_context_menu(cx);
            return;
        }
        if active_tool == ToolId::Text && text_stroke_popover {
            self.close_text_stroke_popover(cx);
            return;
        }
        if active_tool == ToolId::Shape && stroke_popover {
            self.close_stroke_popover(cx);
            return;
        }
        if open_select {
            self.state.update(cx, |state, _| state.open_select = None);
            cx.notify();
            return;
        }
        if more_open {
            self.state.update(cx, |state, _| state.more_open = false);
            cx.notify();
            return;
        }

        let move_idle = self.state.read(cx).active_tool == ToolId::Move
            && self.transform_drag.is_none()
            && self.group_transform_drag.is_none()
            && self.move_marquee.is_none()
            && !self.selection_edit_mode;
        if move_idle {
            let has_selection = !self.state.read(cx).selected_layers.is_empty();
            if has_selection {
                self.state.update(cx, |state, _| {
                    state.selected_layers.clear();
                    state.selected_layer = 0;
                    state.multi_bounds = None;
                });
                cx.notify();
            }
            return;
        }

        self.cancel_canvas_gesture(cx);
        self.selection_edit_mode = false;
    }

    pub(crate) fn toggle_selection_shape_action(
        &mut self,
        _: &ToggleSelectionShape,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| {
            state.selection_shape = match state.selection_shape {
                SelectionShape::Rect => SelectionShape::Ellipse,
                SelectionShape::Ellipse => SelectionShape::Rect,
            };
        });
        cx.notify();
    }

    pub(crate) fn decrease_brush_size_action(
        &mut self,
        _: &DecreaseBrushSize,
        cx: &mut Context<Self>,
    ) {
        self.step_brush(cx, -1.0, 0.0);
    }

    pub(crate) fn increase_brush_size_action(
        &mut self,
        _: &IncreaseBrushSize,
        cx: &mut Context<Self>,
    ) {
        self.step_brush(cx, 1.0, 0.0);
    }

    pub(crate) fn decrease_brush_hardness_action(
        &mut self,
        _: &DecreaseBrushHardness,
        cx: &mut Context<Self>,
    ) {
        self.step_brush(cx, 0.0, -1.0);
    }

    pub(crate) fn increase_brush_hardness_action(
        &mut self,
        _: &IncreaseBrushHardness,
        cx: &mut Context<Self>,
    ) {
        self.step_brush(cx, 0.0, 1.0);
    }

    /// Bracket steps from the paint-tool chain: size moves by ~10% of the
    /// current size (at least 1 px) and hardness by 0.1, only for the paint
    /// tools. The native eraser shares `brush_size`.
    fn step_brush(&mut self, cx: &mut Context<Self>, size_direction: f64, hardness_direction: f64) {
        let tool = self.state.read(cx).active_tool;
        if !matches!(tool, ToolId::Brush | ToolId::Eraser) {
            return;
        }
        self.state.update(cx, |state, _| {
            let eraser = tool == ToolId::Eraser;
            if size_direction != 0.0 {
                let current = if eraser {
                    state.eraser_size
                } else {
                    state.brush_size
                };
                let step = (current * 0.1).round().max(1.0);
                let next = (current + size_direction * step).clamp(1.0, 2000.0);
                if eraser {
                    state.eraser_size = next;
                } else {
                    state.brush_size = next;
                }
            }
            if hardness_direction != 0.0 {
                if eraser {
                    state.eraser_hardness =
                        (state.eraser_hardness + hardness_direction * 0.1).clamp(0.0, 1.0);
                } else {
                    state.brush_hardness =
                        (state.brush_hardness + hardness_direction * 0.1).clamp(0.0, 1.0);
                }
            }
        });
        cx.notify();
    }

    fn select_canvas_tool(&mut self, tool: ToolId, cx: &mut Context<Self>) {
        self.cancel_canvas_gesture(cx);
        self.selection_edit_mode = false;
        // Entering Crop with no pending frame opens the default frame
        // over the whole page (the crop overlay renders as soon as the tool arms).
        let seed_crop = if tool == ToolId::Crop && self.state.read(cx).crop_rect.is_none() {
            self.active_document_size().ok()
        } else {
            None
        };
        self.state.update(cx, |state, _| {
            state.active_tool = tool;
            if tool == ToolId::Selection {
                state.selection_shape = SelectionShape::Rect;
            }
            if tool != ToolId::Crop {
                state.crop_rect = None;
                state.crop_history.clear();
                state.crop_redo.clear();
            } else if let Some((doc_w, doc_h)) = seed_crop {
                state.crop_rect = Some([0.0, 0.0, doc_w, doc_h]);
                state.crop_history.clear();
                state.crop_redo.clear();
            }
            state.selection_drag = None;
        });
        cx.notify();
    }

    /// Drop the platform pointer capture and the shared slot (lost window).
    pub(crate) fn release_canvas_pointer(&self, window: &mut gpui_kit::Window) {
        window.release_pointer();
        self.canvas_capture.set(None);
    }

    /// Drop any in-flight canvas gesture before the active tool changes.
    pub(crate) fn cancel_canvas_gesture(&mut self, cx: &mut Context<Self>) {
        self.stop_edge_scroll();
        self.clear_brush_axis();
        self.alt_sampling = false;
        self.brush_adjust = None;
        self.cancel_move_marquee(cx);
        self.state.update(cx, |state, _| state.hud = None);
        self.cancel_transform_drag(cx);
        self.cancel_selection_rotation(cx);
        let had_brush = self.input.has_active_brush();
        let had_preview = self.state.read(cx).selection_drag.is_some();
        let tool = self.state.read(cx).active_tool;
        if had_brush || had_preview || self.pending_gesture.is_some() {
            self.input.cancel();
            self.pending_gesture = None;
            self.state.update(cx, |state, _| {
                state.selection_drag = None;
                state.gradient_badge = None;
                if tool == ToolId::Crop {
                    state.crop_rect = None;
                    state.crop_history.clear();
                    state.crop_redo.clear();
                }
            });
            if had_brush {
                let _ = self.refresh_active_canvas(cx);
            }
            cx.notify();
        }
    }

    pub(crate) fn canvas_pointer_down(
        &mut self,
        event: &MouseDownEvent,
        bounds: &Rc<Cell<Option<Bounds<Pixels>>>>,
        doc_size: (u32, u32),
        cx: &mut Context<Self>,
    ) {
        #[cfg(feature = "input-measurement")]
        let down_at = std::time::Instant::now();
        if self.state.read(cx).open_select.is_some() {
            let inside = self
                .state
                .read(cx)
                .select_panel_bounds
                .get()
                .map(|bounds| bounds.contains(&event.position))
                .unwrap_or(false);
            if !inside {
                self.close_select(cx);
            }
            return;
        }
        if self.state.read(cx).stroke_popover_open {
            let inside = self
                .state
                .read(cx)
                .stroke_panel_bounds
                .get()
                .map(|bounds| bounds.contains(&event.position))
                .unwrap_or(false);
            if !inside {
                self.close_stroke_popover(cx);
            }
            return;
        }
        if self.state.read(cx).text_stroke_popover_open {
            let inside = self
                .state
                .read(cx)
                .text_stroke_panel_bounds
                .get()
                .map(|bounds| bounds.contains(&event.position))
                .unwrap_or(false);
            if !inside {
                self.close_text_stroke_popover(cx);
            }
            return;
        }
        if self.state.read(cx).more_open {
            // The panel publishes its own rect. A press outside it dismisses the
            // panel and stops there, exactly as an open dropdown behaves; a press
            // inside belongs to the panel's own rows, so the canvas must not also
            // act on it. With no rect published there is nothing to judge, and the
            // press is handled as it was before this panel existed.
            let published = self.state.read(cx).more_panel_bounds.get();
            if let Some(bounds) = published {
                if !bounds.contains(&event.position) {
                    self.state.update(cx, |state, _| state.more_open = false);
                    cx.notify();
                }
                return;
            }
        }
        if self.begin_pan(event, cx) {
            return;
        }
        {
            let camera = self.active_camera();
            if let Some(point) = canvas_doc_coords_free(event.position, bounds, doc_size, camera) {
                if self.selection_rotate_start(bounds, doc_size, point, cx) {
                    return;
                }
            }
        }
        if event.button == gpui_kit::MouseButton::Right
            && event.modifiers.alt
            && matches!(
                self.state.read(cx).active_tool,
                ToolId::Brush | ToolId::Eraser
            )
        {
            let (eraser, size, hardness) = {
                let state = self.state.read(cx);
                if state.active_tool == ToolId::Eraser {
                    (true, state.eraser_size, state.eraser_hardness)
                } else {
                    (false, state.brush_size, state.brush_hardness)
                }
            };
            self.brush_adjust = Some(BrushAdjust {
                eraser,
                start_size: size,
                start_hardness: hardness,
                start_screen: (f32::from(event.position.x), f32::from(event.position.y)),
            });
            return;
        }
        if event.button == gpui_kit::MouseButton::Right {
            // The brush menu owns the paint tools' right-click;
            // the canvas context menu skips brush and eraser.
            if matches!(
                self.state.read(cx).active_tool,
                ToolId::Brush | ToolId::Eraser
            ) {
                self.open_brush_menu(f32::from(event.position.x), f32::from(event.position.y), cx);
            } else {
                self.state.update(cx, |state, _| {
                    state.context_menu =
                        Some((f32::from(event.position.x), f32::from(event.position.y)));
                });
                cx.notify();
            }
            return;
        }
        if event.button != gpui_kit::MouseButton::Left {
            return;
        }
        if self.state.read(cx).text_edit.is_some() {
            let _ = self.commit_text_edit(cx);
            return;
        }
        let camera = self.active_camera();
        let tool = self.state.read(cx).active_tool;
        let mapped = if tool == ToolId::Move {
            canvas_doc_coords_free(event.position, bounds, doc_size, camera)
        } else {
            canvas_doc_coords(event.position, bounds, doc_size, camera)
        };
        let Some((x, y)) = mapped else {
            return;
        };
        if tool == ToolId::Crop {
            if let Some(rect) = self.state.read(cx).crop_rect {
                let zoom = self
                    .effective_camera()
                    .map(|camera| camera.zoom as f64)
                    .unwrap_or(1.0);
                if let Some((handle, interior)) = App::crop_handle_at((x, y), rect, zoom) {
                    self.crop_drag = Some(crate::app::crop::CropDrag {
                        handle: if interior { None } else { Some(handle) },
                        start_rect: rect,
                        start_doc: (x, y),
                    });
                    return;
                }
            }
        }
        match tool {
            ToolId::Brush | ToolId::Eraser => {
                // The brush and eraser sample the composite under a held Alt
                // instead of painting; the stroke never starts.
                if event.modifiers.alt {
                    self.alt_sampling = true;
                    self.sample_eyedropper(x, y, cx);
                    return;
                }
                self.alt_sampling = false;
                let scale = display_scale(bounds, doc_size, camera);
                if self.begin_brush(x, y, tool == ToolId::Eraser, scale, cx) {
                    self.brush_stroke_start = Some((x, y));
                    self.brush_stroke_start_screen =
                        Some((f32::from(event.position.x), f32::from(event.position.y)));
                    self.brush_axis_lock = None;
                    #[cfg(feature = "input-measurement")]
                    self.measurement.accepted_down(down_at);
                    if let Err(error) = self.push_brush_preview(cx) {
                        eprintln!("photrez: brush preview failed: {error}");
                    }
                }
            }
            ToolId::PaintBucket => self.fill_bucket(x, y, cx),
            ToolId::Eyedropper => self.sample_eyedropper(x, y, cx),
            ToolId::Move => {
                let zoom = display_scale(bounds, doc_size, camera);
                if event.modifiers.alt {
                    if let Err(error) = self.duplicate_selected_layer_id(cx) {
                        self.set_status_error(Some(format!("Alt-copy: {error}")), cx);
                        return;
                    }
                }
                let modify = event.modifiers.shift || event.modifiers.control;
                if self.state.read(cx).move_auto_select
                    && self.move_auto_select_click(x, y, modify, cx)
                {
                    return;
                }
                if self.state.read(cx).show_transform_controls
                    && self.begin_transform_drag((x, y), zoom, cx)
                {
                    return;
                }
                // A press that misses every layer starts the layer marquee;
                // the transform drag above owns presses inside the selection.
                if self.layer_hit_at(x, y).is_none() {
                    let screen = (f32::from(event.position.x), f32::from(event.position.y));
                    self.begin_move_marquee((x, y), screen, modify, cx);
                }
            }
            ToolId::Selection | ToolId::Crop | ToolId::Shape | ToolId::Gradient | ToolId::Text => {
                self.pending_gesture = Some(PendingGesture {
                    tool,
                    start: (x, y),
                    last: (x, y),
                    shift: event.modifiers.shift,
                    alt: event.modifiers.alt,
                });
            }
        }
    }

    pub(crate) fn canvas_pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        bounds: &Rc<Cell<Option<Bounds<Pixels>>>>,
        doc_size: (u32, u32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pointer_alt = event.modifiers.alt;
        if self.update_pan(event, cx) {
            return;
        }
        if self.selection_rotation_drag.is_some() {
            let camera = self.active_camera();
            if let Some(point) = canvas_doc_coords_free(event.position, bounds, doc_size, camera) {
                self.selection_rotate_update(point, event.modifiers.shift, cx);
            }
            return;
        }
        if self.move_marquee.is_some() {
            let camera = self.active_camera();
            if let Some((x, y)) = canvas_doc_coords_free(event.position, bounds, doc_size, camera) {
                self.update_move_marquee(
                    (x, y),
                    (f32::from(event.position.x), f32::from(event.position.y)),
                    cx,
                );
            }
            return;
        }
        if let Some(adjust) = self.brush_adjust {
            let dx = f32::from(event.position.x) - adjust.start_screen.0;
            let dy = adjust.start_screen.1 - f32::from(event.position.y);
            let size = (adjust.start_size * (1.0 + f64::from(dx) * 0.005))
                .round()
                .clamp(1.0, 2000.0);
            let hardness = (adjust.start_hardness + f64::from(dy) * 0.004).clamp(0.0, 1.0);
            self.state.update(cx, |state, _| {
                if adjust.eraser {
                    state.eraser_size = size;
                    state.eraser_hardness = hardness;
                } else {
                    state.brush_size = size;
                    state.brush_hardness = hardness;
                }
                state.hud = Some(crate::state::HudInfo {
                    mode: crate::state::HudMode::Brush,
                    x: f32::from(event.position.x),
                    y: f32::from(event.position.y),
                    a: size,
                    b: hardness * 100.0,
                    scale_percent: 0.0,
                    snap: false,
                });
            });
            cx.notify();
            return;
        }
        if self.transform_drag.is_some() || self.group_transform_drag.is_some() {
            let camera = self.active_camera();
            let Some((x, y)) = canvas_doc_coords_free(event.position, bounds, doc_size, camera)
            else {
                return;
            };
            let modifiers = event.modifiers;
            self.queue_transform_drag_update(
                TransformDragUpdate {
                    point: (x, y),
                    screen: (f32::from(event.position.x), f32::from(event.position.y)),
                    shift: modifiers.shift,
                    alt: modifiers.alt,
                    bypass_snap: modifiers.control,
                },
                window,
                cx,
            );
            return;
        }
        if let Some(drag) = self.crop_drag {
            let camera = self.active_camera();
            let Some((x, y)) = canvas_doc_coords_free(event.position, bounds, doc_size, camera)
            else {
                return;
            };
            let (doc_w, doc_h) = self.active_document_size().unwrap_or((1.0, 1.0));
            let rect = App::crop_drag_rect(drag, (x, y), doc_w, doc_h);
            self.state
                .update(cx, |state, _| state.crop_rect = Some(rect));
            cx.notify();
            return;
        }
        if self.alt_sampling {
            let camera = self.active_camera();
            if let Some((x, y)) = canvas_doc_coords(event.position, bounds, doc_size, camera) {
                self.sample_eyedropper(x, y, cx);
            }
            return;
        }
        let camera = self.active_camera();
        let Some((x, y)) = canvas_doc_coords(event.position, bounds, doc_size, camera) else {
            return;
        };
        let mut gradient_badge = None;
        if let Some(pending) = self.pending_gesture.as_mut() {
            pending.last = (x, y);
            if pending.tool == ToolId::Gradient {
                let dx = x - pending.start.0;
                let dy = y - pending.start.1;
                let distance = (dx * dx + dy * dy).sqrt().round();
                let degrees = (dy.atan2(dx).to_degrees() + 360.0) % 360.0;
                gradient_badge = Some(crate::state::GradientBadge {
                    x: f32::from(event.position.x),
                    y: f32::from(event.position.y),
                    distance,
                    angle: (degrees * 10.0).round() / 10.0,
                });
            }
            let (px, py, pw, ph) = gesture_rect(pending);
            let preview = [px, py, pw, ph];
            match pending.tool {
                ToolId::Selection => {
                    self.state
                        .update(cx, |state, _| state.selection_drag = Some(preview));
                    cx.notify();
                }
                ToolId::Crop => {
                    self.state
                        .update(cx, |state, _| state.crop_rect = Some(preview));
                    cx.notify();
                }
                _ => {}
            }
            if let Some(badge) = gradient_badge {
                self.state
                    .update(cx, |state, _| state.gradient_badge = Some(badge));
            }
            self.track_edge_scroll(event.position, cx);
            return;
        }
        if !self.input.has_active_brush() {
            return;
        }
        let (x, y) = self.brush_axis_point(
            event.modifiers.shift,
            Some((f32::from(event.position.x), f32::from(event.position.y))),
            x,
            y,
        );
        let (smooth_x, smooth_y) = self.paint_smoother.add(x, y);
        if self.input.update_brush(smooth_x, smooth_y) {
            if let Err(error) = self.push_brush_preview(cx) {
                eprintln!("photrez: brush preview failed: {error}");
            }
            cx.notify();
        }
        self.track_edge_scroll(event.position, cx);
    }

    pub(crate) fn canvas_hover(
        &mut self,
        event: &MouseMoveEvent,
        bounds: &Rc<Cell<Option<Bounds<Pixels>>>>,
        doc_size: (u32, u32),
        cx: &mut Context<Self>,
    ) {
        let _ = bounds;
        let _ = doc_size;
        self.hover_pos = Some(event.position);
        let tool = self.state.read(cx).active_tool;
        if (tool == ToolId::Crop && self.state.read(cx).crop_rect.is_some()) || tool == ToolId::Move
        {
            let policy = self.canvas_cursor_policy(cx);
            if policy != self.last_cursor {
                self.last_cursor = policy;
                cx.notify();
            }
        }
    }

    pub(crate) fn close_context_menu(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.context_menu = None);
        cx.notify();
    }

    pub(crate) fn canvas_pointer_up(
        &mut self,
        event: &MouseUpEvent,
        bounds: &Rc<Cell<Option<Bounds<Pixels>>>>,
        doc_size: (u32, u32),
        cx: &mut Context<Self>,
    ) {
        self.stop_edge_scroll();
        if self.selection_rotation_drag.is_some() {
            // Apply the mouse-up coordinate before the commit (the same
            // latest-sample rule the transform drag follows): a fast rotation
            // that releases between two move samples must not lose its final
            // movement.
            let camera = self.active_camera();
            if let Some((x, y)) = canvas_doc_coords_free(event.position, bounds, doc_size, camera) {
                self.selection_rotate_update((x, y), event.modifiers.shift, cx);
            }
            self.selection_rotate_finish(cx);
            return;
        }
        if self.alt_sampling {
            self.alt_sampling = false;
            return;
        }
        if self.brush_adjust.take().is_some() {
            self.state.update(cx, |state, _| state.hud = None);
            return;
        }
        if self.transform_drag.is_some() || self.group_transform_drag.is_some() {
            let camera = self.active_camera();
            if let Some((x, y)) = canvas_doc_coords_free(event.position, bounds, doc_size, camera) {
                let modifiers = event.modifiers;
                self.update_transform_drag_immediately(
                    TransformDragUpdate {
                        point: (x, y),
                        screen: (f32::from(event.position.x), f32::from(event.position.y)),
                        shift: modifiers.shift,
                        alt: modifiers.alt,
                        bypass_snap: modifiers.control,
                    },
                    cx,
                );
            } else {
                self.flush_pending_transform_drag_update(cx);
            }
        }
        if self.finish_transform_drag(cx) {
            return;
        }
        if self.move_marquee.is_some() {
            self.finish_move_marquee(cx);
            return;
        }
        if let Some(drag) = self.crop_drag.take() {
            let changed = self
                .state
                .read(cx)
                .crop_rect
                .is_some_and(|rect| rect != drag.start_rect);
            if changed {
                self.state.update(cx, |state, _| {
                    state.crop_history.push(drag.start_rect);
                    state.crop_redo.clear();
                });
            }
            cx.notify();
            return;
        }
        #[cfg(feature = "input-measurement")]
        let accepted_up_at = self.input.has_active_brush().then(std::time::Instant::now);
        if self.end_pan(cx) {
            return;
        }
        let camera = self.active_camera();
        let mapped = canvas_doc_coords(event.position, bounds, doc_size, camera);
        let inside_surface = bounds
            .get()
            .map(|surface| surface.contains(&event.position))
            .unwrap_or(false);
        if let Some((x, y)) = mapped {
            if let Some(pending) = self.pending_gesture.as_mut() {
                pending.last = (x, y);
            }
            if self.input.has_active_brush() {
                let (x, y) = self.brush_axis_point(
                    event.modifiers.shift,
                    Some((f32::from(event.position.x), f32::from(event.position.y))),
                    x,
                    y,
                );
                let (smooth_x, smooth_y) = self.paint_smoother.add(x, y);
                self.input.update_brush(smooth_x, smooth_y);
            }
        }
        if let Some(pending) = self.pending_gesture.take() {
            // A release outside the canvas surface (a toolbar click while the
            // gesture was live) must not apply the old gesture. A release
            // inside the surface but off the page keeps its last page point.
            if inside_surface {
                self.finish_pointer_gesture(pending, cx);
            }
            return;
        }
        if self.input.has_active_brush() {
            #[cfg(feature = "input-measurement")]
            if let Some(at) = accepted_up_at {
                self.measurement.accepted_up(at);
            }
            self.commit_brush(cx);
        }
    }

    /// Reference brush Shift behaviour: once the pointer has moved 5 screen px,
    /// lock the stroke to the dominant document axis from the start point.
    fn brush_axis_point(
        &mut self,
        shift: bool,
        screen: Option<(f32, f32)>,
        x: f64,
        y: f64,
    ) -> (f64, f64) {
        if !shift {
            self.brush_axis_lock = None;
            return (x, y);
        }
        let (Some(start), Some(screen_start)) =
            (self.brush_stroke_start, self.brush_stroke_start_screen)
        else {
            return (x, y);
        };
        if self.brush_axis_lock.is_none() {
            if let Some((screen_x, screen_y)) = screen {
                let dx = screen_x - screen_start.0;
                let dy = screen_y - screen_start.1;
                if dx.abs() >= 5.0 || dy.abs() >= 5.0 {
                    self.brush_axis_lock = Some((x - start.0).abs() > (y - start.1).abs());
                }
            }
        }
        match self.brush_axis_lock {
            Some(true) => (x, start.1),
            Some(false) => (start.0, y),
            None => (x, y),
        }
    }

    fn clear_brush_axis(&mut self) {
        self.brush_axis_lock = None;
        self.brush_stroke_start = None;
        self.brush_stroke_start_screen = None;
    }

    fn begin_brush(
        &mut self,
        x: f64,
        y: f64,
        eraser: bool,
        display_scale: f64,
        cx: &mut Context<Self>,
    ) -> bool {
        let target = match self.active_brush_target(display_scale, cx) {
            Ok(target) => target,
            Err(error) => {
                self.set_status_error(Some(format!("Brush: {error}")), cx);
                return false;
            }
        };
        let (size, hardness, color, opacity, flow, smoothing) = {
            let state = self.state.read(cx);
            if eraser {
                (
                    state.eraser_size,
                    state.eraser_hardness,
                    state.fg_color,
                    state.eraser_opacity,
                    state.eraser_flow,
                    state.eraser_smoothing,
                )
            } else {
                (
                    state.brush_size,
                    state.brush_hardness,
                    state.fg_color,
                    state.brush_opacity,
                    state.brush_flow,
                    state.brush_smoothing,
                )
            }
        };
        let tip = self.brush_tips.get(size, hardness, color);
        self.paint_smoother.set_strength(smoothing);
        self.paint_smoother.reset();
        let alpha = (opacity * flow).clamp(0.0, 1.0);
        let stroke = brush::begin_stroke(target, x, y, size, eraser, alpha, tip);
        if !self.input.begin_brush(stroke) {
            self.set_status_error(Some("Brush: a gesture is already active".into()), cx);
            false
        } else {
            true
        }
    }

    fn commit_brush(&mut self, cx: &mut Context<Self>) {
        self.clear_brush_axis();
        self.alt_sampling = false;
        let Some(stroke) = self.input.finish_brush() else {
            return;
        };
        let crate::input::BrushStroke {
            target,
            dabs,
            tip,
            eraser,
            ..
        } = stroke;
        let BrushTarget {
            doc_id,
            layer_id,
            width,
            height,
            ..
        } = target;
        let raster_tip = RasterTip {
            width: tip.width as usize,
            height: tip.height as usize,
            data: &tip.rgba_data_size,
        };
        // The renderer draws the tip at its raster diameter, not the nominal
        // size: the data carries support padding around the nominal radius.
        let brush = tip.diameter as f64;
        #[cfg(feature = "input-measurement")]
        let mut commit_phases = photrez_core::pixel::CommitPhases::default();
        let committed = self.sessions.get_mut(&doc_id).and_then(|session| {
            #[cfg(feature = "input-measurement")]
            {
                session
                    .loaded_document_mut()
                    .registry
                    .commit_pixels_with_phases(
                        &doc_id,
                        &layer_id,
                        Vec::new(),
                        width as usize,
                        height as usize,
                        eraser,
                        brush,
                        &dabs,
                        &raster_tip,
                        true,
                        Some(&mut commit_phases),
                    )
            }
            #[cfg(not(feature = "input-measurement"))]
            {
                session.loaded_document_mut().registry.commit_pixels(
                    &doc_id,
                    &layer_id,
                    Vec::new(),
                    width as usize,
                    height as usize,
                    eraser,
                    brush,
                    &dabs,
                    &raster_tip,
                    true,
                )
            }
        });
        #[cfg(feature = "input-measurement")]
        let commit_stamp = committed
            .as_ref()
            .map(|(_, _, epoch, version)| (std::time::Instant::now(), *epoch, *version));
        let after = committed.map(|(_, after, _, _)| after);
        let committed_ok = after.is_some();
        let single_layer = self.can_patch_single_layer(&doc_id);

        self.input.cleanup();
        // The commit replaces the page with exact pixels; a scaled preview from
        // the last move must never survive it, whichever display arm runs.
        let retired_previews = self
            .active_document
            .as_mut()
            .map(|active| active.canvas.discard_preview_patches())
            .unwrap_or_default();
        self.retired_textures.extend(retired_previews);
        if let Some(after) = after {
            #[cfg(feature = "input-measurement")]
            let mut refresh_succeeded = false;
            if let Err(error) = self.sessions.mark_dirty(&doc_id) {
                self.set_status_error(Some(format!("Brush: dirty state failed: {error:?}")), cx);
            } else {
                let display = if single_layer {
                    self.display_after_brush(&doc_id, &after, cx)
                } else {
                    self.refresh_active_canvas(cx)
                };
                match display {
                    Ok(()) => {
                        self.set_status_error(None, cx);
                        #[cfg(feature = "input-measurement")]
                        {
                            refresh_succeeded = true;
                        }
                    }
                    Err(patch_error) => {
                        eprintln!(
                            "photrez: brush display patch failed: {patch_error}; falling back to full refresh"
                        );
                        match self.refresh_active_canvas(cx) {
                            Ok(()) => {
                                self.set_status_error(None, cx);
                                #[cfg(feature = "input-measurement")]
                                {
                                    refresh_succeeded = true;
                                }
                            }
                            Err(full_error) => {
                                self.set_status_error(
                                    Some(format!("Brush: display refresh failed: {full_error}")),
                                    cx,
                                );
                            }
                        }
                    }
                }
            }
            #[cfg(feature = "input-measurement")]
            if let Some((at, epoch, version)) = commit_stamp {
                let refresh = if refresh_succeeded {
                    super::measurement::RefreshOutcome::Succeeded
                } else {
                    super::measurement::RefreshOutcome::Failed
                };
                if let Some(receipt) = self.measurement.finish_commit(
                    at,
                    &doc_id,
                    &layer_id,
                    epoch,
                    version,
                    refresh,
                    commit_phases,
                ) {
                    cx.emit(receipt);
                }
            }
        } else {
            #[cfg(feature = "input-measurement")]
            self.measurement.refused_commit();
            let _ = self.refresh_active_canvas(cx);
            self.set_status_error(Some("Brush: the stroke commit was refused".into()), cx);
        }
        if committed_ok {
            if let Some(session) = self.sessions.get_mut(&doc_id) {
                session
                    .loaded_document_mut()
                    .registry
                    .set_history_label(&doc_id, if eraser { "Eraser" } else { "Brush" });
            }
        }
        self.project_history(cx);
        cx.notify();
    }

    fn push_brush_preview(&mut self, cx: &mut Context<Self>) -> Result<bool, String> {
        let Some(doc_id) = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
        else {
            return Ok(false);
        };
        let max_edge = preview_max_edge();
        let Some(stroke) = self.input.active_brush() else {
            return Ok(false);
        };
        if stroke.target.doc_id != doc_id {
            return Err("brush stroke targets a different document".to_string());
        }
        // The preview is drawn over the display composite, which is only
        // correct while the painted layer is the topmost visible one.
        let topmost_visible = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .and_then(|content| {
                content
                    .layers
                    .iter()
                    .find(|layer| layer.visible && layer.opacity > 0.0)
                    .map(|layer| layer.id.clone())
            });
        if topmost_visible.as_deref() != Some(stroke.target.layer_id.as_str()) {
            return Ok(false);
        }
        let width = stroke.target.width;
        let height = stroke.target.height;
        let display_scale = stroke.target.display_scale;
        let Some(rect) = crate::input::preview_rect(&stroke.dabs, &stroke.tip, width, height)?
        else {
            return Ok(false);
        };
        let (retired, base) = {
            let active = self
                .active_document
                .as_mut()
                .ok_or_else(|| "no active document".to_string())?;
            if active.doc_id != doc_id {
                return Err("active document changed during the brush preview".to_string());
            }
            let retired = active.canvas.discard_preview_patches();
            let base = active
                .canvas
                .read_region_rgba(rect.x, rect.y, rect.width, rect.height)
                .map_err(|error| format!("brush preview base: {error:?}"))?;
            (retired, base)
        };
        self.retired_textures.extend(retired);
        let patches = crate::input::scaled_preview_patches(
            crate::input::PreviewBase::region(&base, rect, width, height),
            &stroke.dabs,
            &stroke.tip,
            stroke.eraser,
            display_scale,
            max_edge,
        )?;
        {
            let active = self
                .active_document
                .as_mut()
                .ok_or_else(|| "no active document".to_string())?;
            for patch in &patches {
                active
                    .canvas
                    .push_scaled_patch(
                        patch.doc_x,
                        patch.doc_y,
                        patch.doc_width,
                        patch.doc_height,
                        patch.image_width,
                        patch.image_height,
                        &patch.data,
                    )
                    .map_err(|error| format!("brush preview patch: {error:?}"))?;
            }
        }
        if patches.is_empty() {
            return Ok(false);
        }
        #[cfg(test)]
        self.preview_pushes.set(self.preview_pushes.get() + 1);
        #[cfg(feature = "input-measurement")]
        self.measurement.accepted_preview(std::time::Instant::now());
        cx.notify();
        Ok(true)
    }

    pub(crate) fn can_patch_single_layer(&self, doc_id: &str) -> bool {
        let Some(session) = self.sessions.get(doc_id) else {
            return false;
        };
        let Some(content) = session.loaded_document().content_snapshot() else {
            return false;
        };
        content.layers.len() == 1
            && matches!(
                content.layers.first().map(|layer| &layer.layer_type),
                Some(LayerType::Raster)
            )
    }

    fn display_after_brush(
        &mut self,
        doc_id: &str,
        patches: &[TilePatch],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.as_str())
            != Some(doc_id)
        {
            return Err("active document changed during the brush commit".to_string());
        }
        let retired = {
            let active = self
                .active_document
                .as_mut()
                .ok_or_else(|| "no active document".to_string())?;
            let mut retired = active.canvas.discard_preview_patches();
            for patch in patches {
                let x = u32::try_from(patch.x).map_err(|_| "negative brush patch x".to_string())?;
                let y = u32::try_from(patch.y).map_err(|_| "negative brush patch y".to_string())?;
                let width =
                    u32::try_from(patch.w).map_err(|_| "brush patch width overflow".to_string())?;
                let height = u32::try_from(patch.h)
                    .map_err(|_| "brush patch height overflow".to_string())?;
                active
                    .canvas
                    .push_patch(x, y, width, height, &patch.data)
                    .map_err(|error| format!("brush canvas patch: {error:?}"))?;
            }
            retired.extend(
                active
                    .canvas
                    .commit_patches()
                    .map_err(|error| format!("brush canvas patch commit: {error:?}"))?,
            );
            retired
        };
        self.retired_textures.extend(retired);
        cx.notify();
        Ok(())
    }

    fn finish_pointer_gesture(&mut self, pending: PendingGesture, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.selection_drag = None;
            state.gradient_badge = None;
        });
        let result = match pending.tool {
            ToolId::Selection => {
                let rect = gesture_rect(&pending);
                self.apply_selection(rect, cx)
            }
            ToolId::Crop => {
                let (x, y, w, h) = gesture_rect(&pending);
                self.state
                    .update(cx, |state, _| state.crop_rect = Some([x, y, w, h]));
                cx.notify();
                Ok(())
            }
            ToolId::Shape => self.apply_shape(pending.start, pending.last, cx),
            ToolId::Gradient => self.apply_gradient(pending.start, pending.last, cx),
            ToolId::Move => self.apply_move(pending.start, pending.last, cx),
            ToolId::Text => self.apply_text(pending.start, cx),
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.set_status_error(Some(error), cx);
        } else {
            self.set_status_error(None, cx);
        }
    }

    /// Reorder one projected layer row through the core history, so the move
    /// survives reprojection and undo. Unprojected demo rows (no core id) keep
    /// the legacy UI-state fallback used by the shell snapshots.
    pub(crate) fn reorder_layer_rows(
        &mut self,
        from_row: u64,
        to_row: u64,
        cx: &mut Context<Self>,
    ) {
        let projected = {
            let state = self.state.read(cx);
            let from = state.layers.iter().find(|row| row.id == from_row);
            let to = state.layers.iter().find(|row| row.id == to_row);
            match (from, to) {
                (Some(from), Some(to)) => from.core_id.clone().map(|core_id| {
                    (
                        core_id,
                        state
                            .layers
                            .iter()
                            .position(|row| row.id == to.id)
                            .unwrap_or(0),
                    )
                }),
                _ => None,
            }
        };
        let Some((core_id, to_index)) = projected else {
            self.state.update(cx, |state, _| {
                if state.move_layer(from_row, to_row) {
                    state.selected_layer = from_row;
                }
            });
            cx.notify();
            return;
        };
        let selected_core = core_id.clone();
        let result = self.active_doc_id().and_then(|doc_id| {
            self.apply_core_command(
                &doc_id,
                Command::Reorder {
                    id: core_id,
                    to: to_index,
                },
                true,
                cx,
            )
        });
        match result {
            Ok(()) => {
                self.state.update(cx, |state, _| {
                    if let Some(row) = state
                        .layers
                        .iter()
                        .find(|row| row.core_id.as_deref() == Some(selected_core.as_str()))
                    {
                        state.selected_layer = row.id;
                    }
                });
                self.set_status_error(None, cx);
            }
            Err(error) => self.set_status_error(Some(format!("Reorder: {error}")), cx),
        }
        cx.notify();
    }

    pub(crate) fn active_doc_id(&self) -> Result<String, String> {
        self.active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
            .ok_or_else(|| "no active document".to_string())
    }

    pub(crate) fn active_document_size(&self) -> Result<(f64, f64), String> {
        let doc_id = self.active_doc_id()?;
        let loaded = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document();
        let content = loaded
            .content_snapshot()
            .ok_or_else(|| "loaded content is missing".to_string())?;
        Ok((content.width, content.height))
    }

    fn active_layer_transform(&self, cx: &Context<Self>) -> Result<ActiveLayerTransform, String> {
        let (_, layer) = self.selected_layer(cx)?;
        let t = &layer.transform;
        Ok((
            layer.id.clone(),
            t.x,
            t.y,
            t.scale_x,
            t.scale_y,
            t.rotation,
            t.flip_h,
            t.flip_v,
        ))
    }

    /// One live rotation of the committed pixel selection about its centre.
    /// The rectangle stays fixed; only the angle moves
    /// until the pointer is released, which commits one `SetSelection`.
    pub(crate) fn committed_selection(&self) -> Option<SelectionState> {
        let doc_id = self
            .active_document
            .as_ref()
            .map(|doc| doc.doc_id.clone())?;
        self.sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .and_then(|content| content.selection.clone())
    }

    /// Start a selection rotation when the press lands on the rotate pin.
    fn selection_rotate_start(
        &mut self,
        bounds: &Rc<Cell<Option<Bounds<Pixels>>>>,
        doc_size: (u32, u32),
        point: (f64, f64),
        cx: &mut Context<Self>,
    ) -> bool {
        if self.state.read(cx).active_tool != ToolId::Selection || !self.selection_edit_mode {
            return false;
        }
        let Some(selection) = self.committed_selection() else {
            return false;
        };
        if selection.inverted.unwrap_or(false) {
            return false;
        }
        let scale = display_scale(bounds, doc_size, self.active_camera());
        if !scale.is_finite() || scale <= 0.0 {
            return false;
        }
        let (top_mid, normal) = crate::app::transform_geometry::selection_pin(
            selection.x,
            selection.y,
            selection.width,
            selection.height,
            selection.angle,
        );
        let pin = (
            top_mid.x + normal.x * 24.0 / scale,
            top_mid.y + normal.y * 24.0 / scale,
        );
        let (dx, dy) = (point.0 - pin.0, point.1 - pin.1);
        if (dx * dx + dy * dy).sqrt() > 12.0 / scale {
            return false;
        }
        let center = crate::app::transform_geometry::Point {
            x: selection.x + selection.width / 2.0,
            y: selection.y + selection.height / 2.0,
        };
        let reference = crate::app::transform_geometry::angle_about(
            center,
            crate::app::transform_geometry::Point {
                x: point.0,
                y: point.1,
            },
        );
        self.selection_rotation_drag = Some(SelectionRotationDrag {
            selection: selection.clone(),
            start_angle: selection.angle,
            reference_angle: reference,
            preview_angle: selection.angle,
        });
        cx.notify();
        true
    }

    fn selection_rotate_update(&mut self, point: (f64, f64), shift: bool, cx: &mut Context<Self>) {
        let Some(mut drag) = self.selection_rotation_drag.clone() else {
            return;
        };
        let center = crate::app::transform_geometry::Point {
            x: drag.selection.x + drag.selection.width / 2.0,
            y: drag.selection.y + drag.selection.height / 2.0,
        };
        let current = crate::app::transform_geometry::angle_about(
            center,
            crate::app::transform_geometry::Point {
                x: point.0,
                y: point.1,
            },
        );
        let reference = drag.reference_angle;
        let mut angle = drag.start_angle + (current - reference);
        if shift {
            angle = (angle / 15.0).round() * 15.0;
        }
        drag.preview_angle = crate::app::transform_geometry::normalize_rotation(angle);
        self.selection_rotation_drag = Some(drag);
        cx.notify();
    }

    fn selection_rotate_finish(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.selection_rotation_drag.take() else {
            return;
        };
        let selection = SelectionState {
            angle: drag.preview_angle,
            ..drag.selection.clone()
        };
        let result = self.active_doc_id().and_then(|doc_id| {
            self.apply_core_command(&doc_id, Command::SetSelection { selection }, false, cx)
        });
        if let Err(error) = result {
            self.set_status_error(Some(error), cx);
        }
        cx.notify();
    }

    fn cancel_selection_rotation(&mut self, cx: &mut Context<Self>) {
        if self.selection_rotation_drag.take().is_some() {
            cx.notify();
        }
    }

    fn apply_selection(
        &mut self,
        rect: (f64, f64, f64, f64),
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (x, y, width, height) = rect;
        let shape = match self.state.read(cx).selection_shape {
            SelectionShape::Rect => CoreSelectionShape::Rect,
            SelectionShape::Ellipse => CoreSelectionShape::Ellipse,
        };
        let selection = SelectionState {
            x,
            y,
            width,
            height,
            angle: 0.0,
            shape: Some(shape),
            inverted: None,
        };
        let doc_id = self.active_doc_id()?;
        self.apply_core_command(&doc_id, Command::SetSelection { selection }, false, cx)
    }

    fn apply_move(
        &mut self,
        start: (f64, f64),
        end: (f64, f64),
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (id, x, y, scale_x, scale_y, rotation, flip_h, flip_v) =
            self.active_layer_transform(cx)?;
        let transform = TransformPatch {
            x: x + end.0 - start.0,
            y: y + end.1 - start.1,
            scale_x,
            scale_y,
            rotation,
            flip_h: Some(flip_h),
            flip_v: Some(flip_v),
        };
        let doc_id = self.active_doc_id()?;
        self.apply_core_command(&doc_id, Command::TransformLayer { id, transform }, true, cx)
    }

    fn apply_shape(
        &mut self,
        start: (f64, f64),
        end: (f64, f64),
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (x, y, width, height) = normalized_rect(start, end);
        let (doc_w, doc_h) = self.active_document_size()?;
        let (
            kind,
            fill_enabled,
            stroke_enabled,
            stroke_width,
            stroke_color,
            radius,
            arrow_head,
            color,
        ) = {
            let state = self.state.read(cx);
            (
                shape_tool_kind(state.shape_kind),
                state.shape_fill,
                state.shape_stroke,
                state.shape_stroke_width,
                hex_color(state.shape_stroke_color),
                state.shape_radius,
                state.shape_arrow_head,
                hex_color(state.fg_color),
            )
        };
        let params = ShapeParams {
            kind,
            width,
            height,
            radius: radius.clamp(0.0, width.min(height) / 2.0),
            fill: ShapeFill {
                kind: if fill_enabled {
                    ShapeFillKind::Solid
                } else {
                    ShapeFillKind::None
                },
                color,
            },
            stroke: ShapeStroke {
                enabled: stroke_enabled,
                color: stroke_color,
                width: stroke_width.max(1.0),
            },
            arrow_head,
        };
        let id = next_layer_id();
        let doc_id = self.active_doc_id()?;
        let result = self.apply_core_command(
            &doc_id,
            Command::AddLayer {
                id: id.clone(),
                name: "Shape".to_string(), // i18n-exempt: layer name set by the shape tool
                width: doc_w,
                height: doc_h,
                index: 0,
                x: Some(x),
                y: Some(y),
                layer_type: Some(LayerType::Shape),
                shape_params: Some(params),
                text_data: None,
            },
            true,
            cx,
        );
        if result.is_ok() {
            // The shape tool selects the layer it just created; reproject with the
            // new core id so the Properties/Inspector fields follow it too.
            self.project_layers_with_selection(Some(id.as_str()), cx);
        }
        result
    }

    fn apply_text(&mut self, at: (f64, f64), cx: &mut Context<Self>) -> Result<(), String> {
        let (doc_w, doc_h) = self.active_document_size()?;
        let (color, font_size, font_weight, italic, align, stroke) = {
            let state = self.state.read(cx);
            (
                hex_color(state.fg_color),
                state.text_font_size,
                state.text_font_weight,
                state.text_italic,
                state.text_align,
                photrez_core::ptz::layer_params::TextStroke {
                    width: state.text_stroke_width,
                    color: crate::palette::to_hex(state.text_stroke_color),
                    align: Some(state.text_stroke_align),
                },
            )
        };
        let data = TextData {
            content: String::new(),
            font_family: "Arial".to_string(), // i18n-exempt: technical token
            font_size,
            font_weight,
            font_style: if italic {
                photrez_core::ptz::layer_params::TextFontStyle::Italic
            } else {
                photrez_core::ptz::layer_params::TextFontStyle::Normal
            },
            align: match align {
                crate::state::TextAlignTool::Left => {
                    photrez_core::ptz::layer_params::TextAlign::Left
                }
                crate::state::TextAlignTool::Center => {
                    photrez_core::ptz::layer_params::TextAlign::Center
                }
                crate::state::TextAlignTool::Right => {
                    photrez_core::ptz::layer_params::TextAlign::Right
                }
            },
            color,
            stroke,
            ..Default::default()
        };
        let id = next_layer_id();
        let edit_id = id.clone();
        let doc_id = self.active_doc_id()?;
        self.apply_core_command(
            &doc_id,
            Command::AddLayer {
                id,
                name: format!("Text at {:.0},{:.0}", at.0, at.1),
                width: doc_w,
                height: doc_h,
                index: 0,
                x: Some(at.0),
                y: Some(at.1),
                layer_type: Some(LayerType::Text),
                shape_params: None,
                text_data: Some(data.clone()),
            },
            true,
            cx,
        )?;
        self.begin_text_edit(edit_id, at.0, at.1, data, cx);
        Ok(())
    }

    fn selection_fill_mask(&self, cx: &Context<Self>) -> (bool, i32, i32, i32, i32, u32, bool) {
        let Some(doc_id) = self.active_document.as_ref().map(|doc| doc.doc_id.clone()) else {
            return (false, 0, 0, 0, 0, 0, false);
        };
        let Some(selection) = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .and_then(|content| content.selection.clone())
        else {
            return (false, 0, 0, 0, 0, 0, false);
        };
        let _ = cx;
        let shape = match selection.shape {
            Some(CoreSelectionShape::Ellipse) => 1,
            _ => 0,
        };
        (
            true,
            selection.x.round() as i32,
            selection.y.round() as i32,
            selection.width.round() as i32,
            selection.height.round() as i32,
            shape,
            selection.inverted.unwrap_or(false),
        )
    }

    fn apply_gradient(
        &mut self,
        start: (f64, f64),
        end: (f64, f64),
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let target = self
            .active_paint_target(false, cx)
            .map_err(|error| format!("Gradient: {error}"))?;
        let (fg, bg, radial, reverse, fg_transparent) = {
            let state = self.state.read(cx);
            (
                state.fg_color,
                state.bg_color,
                state.gradient_radial,
                state.gradient_reverse,
                state.gradient_fg_transparent,
            )
        };
        let (from, to) = if fg_transparent {
            (fg, [fg[0], fg[1], fg[2], 0])
        } else if reverse {
            (bg, fg)
        } else {
            (fg, bg)
        };
        let (has_mask, mx, my, mw, mh, shape, inverted) = self.selection_fill_mask(cx);
        let filled = photrez_core::pixel::kernel::gradient_fill(
            &target.base_pixels,
            target.width,
            target.height,
            u32::from(radial),
            start.0,
            start.1,
            end.0,
            end.1,
            &[0.0, 1.0],
            &[
                from[0], from[1], from[2], from[3], to[0], to[1], to[2], to[3],
            ],
            has_mask,
            mx,
            my,
            mw,
            mh,
            shape,
            inverted,
        );
        self.apply_pixel_region(target, filled, "Gradient", cx)
    }

    fn fill_bucket(&mut self, x: f64, y: f64, cx: &mut Context<Self>) {
        let result = (|| {
            let target = self
                .active_paint_target(false, cx)
                .map_err(|error| format!("Fill: {error}"))?;
            let (color, tolerance, contiguous) = {
                let state = self.state.read(cx);
                (
                    state.fg_color,
                    state.fill_tolerance.min(255),
                    state.fill_contiguous,
                )
            };
            let (has_mask, mx, my, mw, mh, shape, inverted) = self.selection_fill_mask(cx);
            let sx = (x.max(0.0) as u32).min(target.width.saturating_sub(1));
            let sy = (y.max(0.0) as u32).min(target.height.saturating_sub(1));
            let filled = photrez_core::pixel::kernel::flood_fill(
                &target.base_pixels,
                target.width,
                target.height,
                sx,
                sy,
                color[0] as u32,
                color[1] as u32,
                color[2] as u32,
                color[3] as u32,
                tolerance,
                has_mask,
                mx,
                my,
                mw,
                mh,
                shape,
                inverted,
                contiguous,
            );
            self.apply_pixel_region(target, filled, "Fill", cx)
        })();
        if let Err(error) = result {
            self.set_status_error(Some(error), cx);
        }
    }

    fn sample_eyedropper(&mut self, x: f64, y: f64, cx: &mut Context<Self>) {
        let result = (|| {
            let doc_id = self.active_doc_id()?;
            let (doc_w, doc_h) = self.active_document_size()?;
            let px = (x.max(0.0) as u32).min((doc_w as u32).saturating_sub(1));
            let py = (y.max(0.0) as u32).min((doc_h as u32).saturating_sub(1));
            let composite = {
                let loaded = self
                    .sessions
                    .get(&doc_id)
                    .ok_or_else(|| "active session is missing".to_string())?
                    .loaded_document();
                let content = loaded
                    .content_snapshot()
                    .ok_or_else(|| "loaded content is missing".to_string())?;
                photrez_core::render::compositor::composite_layers_with_font_data(
                    content,
                    &loaded.registry,
                    Some(crate::INTER),
                )
                .map_err(|error| error.to_string())?
            };
            if px >= composite.width || py >= composite.height {
                return Err("the eyedropper is outside the document".to_string());
            }
            let index = (py as usize * composite.width as usize + px as usize) * 4;
            let rgba = &composite.pixels[index..index + 4];
            let color = [rgba[0], rgba[1], rgba[2], rgba[3]];
            let auto_copy = self.state.read(cx).auto_copy_hex;
            self.state.update(cx, |state, _| state.fg_color = color);
            if auto_copy {
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(hex_color(color)));
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.set_status_error(Some(error), cx);
        } else {
            self.set_status_error(None, cx);
        }
        cx.notify();
    }

    fn apply_pixel_region(
        &mut self,
        target: PaintTarget,
        filled: Vec<u8>,
        label: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if filled == target.base_pixels {
            return Ok(());
        }
        let PaintTarget {
            doc_id,
            layer_id,
            width,
            height,
            ..
        } = target;
        let committed = {
            let session = self
                .sessions
                .get_mut(&doc_id)
                .ok_or_else(|| "active session is missing".to_string())?;
            session.loaded_document_mut().registry.write_region(
                &doc_id,
                &layer_id,
                0,
                0,
                width as usize,
                height as usize,
                filled,
            )
        };
        if committed.is_none() {
            return Err("pixel region write was refused".to_string());
        }
        self.sessions
            .mark_dirty(&doc_id)
            .map_err(|error| format!("dirty state failed: {error:?}"))?;
        if let Some(session) = self.sessions.get_mut(&doc_id) {
            session
                .loaded_document_mut()
                .registry
                .set_history_label(&doc_id, label);
        }
        self.refresh_active_canvas(cx)
    }

    pub(crate) fn apply_core_command(
        &mut self,
        doc_id: &str,
        command: Command,
        mark_dirty: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let added_layer = match &command {
            Command::AddLayer { id, .. } => Some(id.clone()),
            _ => None,
        };
        let envelope = CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command,
        };
        {
            let session = self
                .sessions
                .get_mut(doc_id)
                .ok_or_else(|| "active session is missing".to_string())?;
            session
                .loaded_document_mut()
                .registry
                .apply_command(doc_id, envelope)
                .map_err(|error| format!("{}: {}", error.code, error.message))?;
        }
        if mark_dirty {
            self.sessions
                .mark_dirty(doc_id)
                .map_err(|error| format!("dirty state failed: {error:?}"))?;
            self.project_layers_with_selection(added_layer.as_deref(), cx);
            self.refresh_active_canvas(cx)?;
        }
        Ok(())
    }

    /// The document layer selected in the Layers panel.
    ///
    /// Falls back to the first raster layer when the rows are not projected
    /// (demo/seed state). A projected selection names one concrete core layer.
    pub(crate) fn selected_layer(&self, cx: &Context<Self>) -> Result<(String, PtzLayer), String> {
        let doc_id = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
            .ok_or_else(|| "no active document".to_string())?;
        let loaded = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document();
        let content = loaded
            .content_snapshot()
            .ok_or_else(|| "loaded content is missing".to_string())?;
        let state = self.state.read(cx);
        if state.selected_layers.is_empty() && state.layers.iter().any(|row| row.core_id.is_some())
        {
            return Err("no layer is selected".to_string());
        }
        let selected_core = state
            .layers
            .iter()
            .find(|row| row.id == state.selected_layer)
            .and_then(|row| row.core_id.clone());
        let layer = match selected_core {
            Some(core_id) => content
                .layers
                .iter()
                .find(|layer| layer.id == core_id)
                .ok_or_else(|| "the selected layer is missing from the document".to_string())?,
            None => content
                .layers
                .iter()
                .find(|layer| matches!(layer.layer_type, LayerType::Raster))
                .ok_or_else(|| "no raster layer is active".to_string())?,
        };
        Ok((doc_id, layer.clone()))
    }

    /// The cursor the canvas should show, from the reachable native inputs.
    pub(crate) fn canvas_cursor_policy(&self, cx: &Context<Self>) -> CursorPolicy {
        if self.state.read(cx).active_tool == ToolId::Crop {
            if let (Some(rect), Some(position), Some(bounds)) = (
                self.state.read(cx).crop_rect,
                self.hover_pos,
                self.canvas_bounds.get(),
            ) {
                let (doc_w, doc_h) = self
                    .active_document
                    .as_ref()
                    .map(|document| document.canvas.dimensions())
                    .unwrap_or((1, 1));
                if let Some((x, y)) = canvas_doc_coords(
                    position,
                    &self.canvas_bounds,
                    (doc_w, doc_h),
                    self.active_camera(),
                ) {
                    let _ = bounds;
                    let zoom = self
                        .effective_camera()
                        .map(|camera| camera.zoom as f64)
                        .unwrap_or(1.0);
                    if let Some((handle, interior)) = App::crop_handle_at((x, y), rect, zoom) {
                        if interior {
                            return CursorPolicy::SystemDefault;
                        }
                        return match handle {
                            crate::app::crop::CropHandle::Nw | crate::app::crop::CropHandle::Se => {
                                CursorPolicy::ResizeNwse
                            }
                            crate::app::crop::CropHandle::Ne | crate::app::crop::CropHandle::Sw => {
                                CursorPolicy::ResizeNesw
                            }
                            crate::app::crop::CropHandle::W | crate::app::crop::CropHandle::E => {
                                CursorPolicy::ResizeEw
                            }
                            crate::app::crop::CropHandle::N | crate::app::crop::CropHandle::S => {
                                CursorPolicy::ResizeNs
                            }
                        };
                    }
                }
            }
        }
        if self.state.read(cx).active_tool == ToolId::Move
            && self.state.read(cx).show_transform_controls
        {
            if let (Some(position), Some((doc_w, doc_h))) = (
                self.hover_pos,
                self.active_document
                    .as_ref()
                    .map(|document| document.canvas.dimensions()),
            ) {
                if let Some((x, y)) = canvas_doc_coords(
                    position,
                    &self.canvas_bounds,
                    (doc_w, doc_h),
                    self.active_camera(),
                ) {
                    if let Ok((_, layer)) = self.selected_layer_for_cursor(cx) {
                        let zoom = self
                            .effective_camera()
                            .map(|camera| camera.zoom as f64)
                            .unwrap_or(1.0);
                        let (frame_w, frame_h) =
                            crate::app::transform_geometry::layer_frame_size(&layer);
                        if let Some(handle) = crate::app::transform_geometry::detect_handle(
                            crate::app::transform_geometry::Point { x, y },
                            &layer.transform,
                            frame_w,
                            frame_h,
                            zoom,
                        ) {
                            use crate::app::transform_geometry::Handle;
                            return match handle {
                                Handle::Move => CursorPolicy::SystemDefault,
                                Handle::Rotate => CursorPolicy::Crosshair,
                                Handle::Nw | Handle::Se => CursorPolicy::ResizeNwse,
                                Handle::Ne | Handle::Sw => CursorPolicy::ResizeNesw,
                                Handle::W | Handle::E => CursorPolicy::ResizeEw,
                                Handle::N | Handle::S => CursorPolicy::ResizeNs,
                            };
                        }
                    }
                }
            }
        }
        if self.pan.is_some() {
            return CursorPolicy::Grabbing;
        }
        if self.space_pressed {
            return CursorPolicy::OpenHand;
        }
        let selected = self.selected_layer(cx).ok();
        let layer_locked = selected
            .as_ref()
            .map(|(_, layer)| layer.locked)
            .unwrap_or(false);
        let layer_paintable = selected
            .as_ref()
            .map(|(_, layer)| matches!(layer.layer_type, LayerType::Raster))
            .unwrap_or(true);
        crate::input::cursor_policy_for_context(CursorContext {
            tool: self.state.read(cx).active_tool,
            phase: self.input.phase(),
            layer_locked,
            layer_paintable,
            dragging: self.transform_drag.is_some(),
            cloning: self.pointer_alt,
        })
    }

    /// The selected raster layer a paint operation may write to.
    pub(crate) fn selected_paint_layer(
        &self,
        allow_locked: bool,
        cx: &Context<Self>,
    ) -> Result<(String, PtzLayer), String> {
        let (doc_id, layer) = self.selected_layer(cx)?;
        if !allow_locked && layer.locked {
            return Err("the selected layer is locked".to_string());
        }
        if !matches!(layer.layer_type, LayerType::Raster) {
            return Err("the selected layer is not a raster layer".to_string());
        }
        Ok((doc_id, layer))
    }

    /// The raster layer a brush stroke targets. Unlike [`App::active_paint_target`]
    /// this copies no pixels: the preview reads the canonical crop and the
    /// commit reads the store.
    fn active_brush_target(
        &self,
        display_scale: f64,
        cx: &Context<Self>,
    ) -> Result<BrushTarget, String> {
        let (doc_id, layer) = self.selected_paint_layer(false, cx)?;
        let loaded = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document();
        let canonical = loaded
            .registry
            .get_layer(&doc_id, &layer.id)
            .ok_or_else(|| "the selected layer has no pixel store".to_string())?;
        Ok(BrushTarget {
            doc_id,
            layer_id: layer.id,
            width: canonical.width,
            height: canonical.height,
            display_scale,
        })
    }

    /// The selected raster layer as an owned base-pixel buffer, for the
    /// non-brush pixel tools. `allow_locked` is true for the read-only
    /// eyedropper; write tools pass false.
    fn active_paint_target(
        &self,
        allow_locked: bool,
        cx: &Context<Self>,
    ) -> Result<PaintTarget, String> {
        let (doc_id, layer) = self.selected_paint_layer(allow_locked, cx)?;
        let loaded = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document();
        let canonical = loaded
            .registry
            .get_layer(&doc_id, &layer.id)
            .ok_or_else(|| "the selected layer has no pixel store".to_string())?;
        Ok(PaintTarget {
            doc_id,
            layer_id: layer.id,
            width: canonical.width,
            height: canonical.height,
            base_pixels: canonical.pixels.clone(),
        })
    }

    /// Recompose the active document after a core mutation.
    pub(crate) fn refresh_active_canvas(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        #[cfg(test)]
        self.full_composites.set(self.full_composites.get() + 1);
        let doc_id = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
            .ok_or_else(|| "no active document".to_string())?;
        let composite = {
            let loaded = self
                .sessions
                .get(&doc_id)
                .ok_or_else(|| "active session is missing".to_string())?
                .loaded_document();
            let content = loaded
                .content_snapshot()
                .ok_or_else(|| "loaded content is missing".to_string())?;
            photrez_core::render::compositor::composite_layers_with_font_data(
                content,
                &loaded.registry,
                Some(crate::INTER),
            )
            .map_err(|error| error.to_string())?
        };
        let canvas = CanvasImage::from_rgba(composite.width, composite.height, composite.pixels)
            .map_err(|error| format!("{error:?}"))?;
        if let Some(active) = self.active_document.as_mut() {
            let retired = std::mem::replace(&mut active.canvas, canvas);
            self.retired_textures.extend(retired.into_textures());
        }
        self.project_history(cx);
        self.project_tabs(cx);
        cx.notify();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn full_composites_for_test(&self) -> usize {
        self.full_composites.get()
    }

    #[cfg(test)]
    pub(crate) fn preview_pushes_for_test(&self) -> usize {
        self.preview_pushes.get()
    }

    #[cfg(test)]
    pub(crate) fn last_shape_kind_for_test(&self) -> Option<ShapeKind> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        content.layers.iter().find_map(|layer| {
            layer
                .shape_params
                .as_ref()
                .map(|params| params.kind.clone())
        })
    }

    #[cfg(test)]
    pub(crate) fn last_shape_stroke_for_test(&self) -> Option<(bool, String, f64)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        content.layers.iter().find_map(|layer| {
            layer.shape_params.as_ref().map(|params| {
                (
                    params.stroke.enabled,
                    params.stroke.color.clone(),
                    params.stroke.width,
                )
            })
        })
    }

    #[cfg(test)]
    pub(crate) fn active_composite_pixel_for_test(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let loaded = self.sessions.get(&doc_id)?.loaded_document();
        let content = loaded.content_snapshot()?;
        let composite = photrez_core::render::compositor::composite_layers_with_font_data(
            content,
            &loaded.registry,
            Some(crate::INTER),
        )
        .ok()?;
        if x >= composite.width || y >= composite.height {
            return None;
        }
        let index = (y as usize * composite.width as usize + x as usize) * 4;
        composite.pixels[index..index + 4].try_into().ok()
    }

    #[cfg(test)]
    pub(crate) fn active_document_dims_for_test(&self) -> Option<(u32, u32)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        Some((content.width as u32, content.height as u32))
    }

    #[cfg(test)]
    pub(crate) fn selection_drag_for_test(&self, cx: &gpui_kit::App) -> Option<[f64; 4]> {
        self.state.read(cx).selection_drag
    }

    #[cfg(test)]
    pub(crate) fn input_phase_for_test(&self) -> crate::input::GesturePhase {
        self.input.phase()
    }

    #[cfg(test)]
    pub(crate) fn active_core_pixels_for_test(&self) -> Option<(u32, u32, Vec<u8>)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let loaded = self.sessions.get(&doc_id)?.loaded_document();
        let content = loaded.content_snapshot()?;
        let layer = content
            .layers
            .iter()
            .find(|layer| matches!(layer.layer_type, LayerType::Raster))?;
        let canonical = loaded.registry.get_layer(&doc_id, &layer.id)?;
        Some((canonical.width, canonical.height, canonical.pixels.clone()))
    }

    #[cfg(test)]
    pub(crate) fn active_core_layer_ids_for_test(&self) -> Vec<String> {
        let Some(doc_id) = self.active_document.as_ref().map(|d| d.doc_id.clone()) else {
            return Vec::new();
        };
        self.sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .map(|content| {
                content
                    .layers
                    .iter()
                    .map(|layer| layer.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn active_core_layer_origin_for_test(
        &self,
        layer_type: LayerType,
    ) -> Option<(f64, f64)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        content
            .layers
            .iter()
            .find(|layer| layer.layer_type == layer_type)
            .map(|layer| (layer.transform.x, layer.transform.y))
    }

    #[cfg(test)]
    pub(crate) fn core_layer_origins_for_test(&self, layer_type: LayerType) -> Vec<(f64, f64)> {
        let Some(doc_id) = self.active_document.as_ref().map(|d| d.doc_id.clone()) else {
            return Vec::new();
        };
        self.sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .map(|content| {
                content
                    .layers
                    .iter()
                    .filter(|layer| layer.layer_type == layer_type)
                    .map(|layer| (layer.transform.x, layer.transform.y))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn core_layer_pixels_for_test(&self, layer_id: &str) -> Option<(u32, u32, Vec<u8>)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let layer = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .registry
            .get_layer(&doc_id, layer_id)?;
        Some((layer.width, layer.height, layer.pixels.clone()))
    }

    #[cfg(test)]
    pub(crate) fn active_core_layer_count_for_test(&self) -> Option<usize> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let loaded = self.sessions.get(&doc_id)?.loaded_document();
        Some(loaded.content_snapshot()?.layers.len())
    }

    #[cfg(test)]
    pub(crate) fn active_core_layer_types_for_test(&self) -> Option<Vec<LayerType>> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let loaded = self.sessions.get(&doc_id)?.loaded_document();
        Some(
            loaded
                .content_snapshot()?
                .layers
                .iter()
                .map(|layer| layer.layer_type.clone())
                .collect(),
        )
    }

    #[cfg(test)]
    pub(crate) fn active_core_selection_for_test(&self) -> Option<SelectionState> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let loaded = self.sessions.get(&doc_id)?.loaded_document();
        loaded.content_snapshot()?.selection.clone()
    }

    #[cfg(test)]
    pub(crate) fn active_core_transform_for_test(&self) -> Option<(f64, f64)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let loaded = self.sessions.get(&doc_id)?.loaded_document();
        let content = loaded.content_snapshot()?;
        let layer = content
            .layers
            .iter()
            .find(|layer| matches!(layer.layer_type, LayerType::Raster))?;
        Some((layer.transform.x, layer.transform.y))
    }

    #[cfg(test)]
    pub(crate) fn active_core_history_version_for_test(&self) -> Option<u64> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        self.sessions
            .get(&doc_id)?
            .loaded_document()
            .registry
            .get_history_version(&doc_id)
    }

    #[cfg(test)]
    pub(crate) fn active_core_history_cursor_for_test(&self) -> Option<usize> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        self.sessions
            .get(&doc_id)?
            .loaded_document()
            .registry
            .get_history_cursor(&doc_id)
    }
}

/// The zoom the display currently applies to the document, clamped to the
/// native-resolution ceiling. The live preview renders at this scale; the
/// pointer-up commit stays full-resolution.
/// Weighted ring-buffer smoothing: each
/// move is averaged with recent points, newest weighted heaviest.
pub(crate) struct PaintSmoother {
    xs: [f64; 10],
    ys: [f64; 10],
    head: usize,
    size: usize,
    window: usize,
}

impl Default for PaintSmoother {
    fn default() -> Self {
        Self {
            xs: [0.0; 10],
            ys: [0.0; 10],
            head: 0,
            size: 0,
            window: 1,
        }
    }
}

impl PaintSmoother {
    pub(crate) fn set_strength(&mut self, value: f64) {
        let window = if value <= 0.0 {
            1
        } else if value <= 30.0 {
            2 + ((value - 1.0) / 29.0).floor() as usize
        } else if value <= 70.0 {
            4 + ((value - 31.0) / 39.0 * 3.0).floor() as usize
        } else {
            7 + ((value - 71.0) / 29.0 * 4.0).floor() as usize
        };
        self.window = window.clamp(1, 10);
    }

    pub(crate) fn reset(&mut self) {
        self.head = 0;
        self.size = 0;
    }

    pub(crate) fn add(&mut self, x: f64, y: f64) -> (f64, f64) {
        self.xs[self.head] = x;
        self.ys[self.head] = y;
        self.head = (self.head + 1) % self.xs.len();
        self.size = (self.size + 1).min(self.xs.len());
        let n = self.size.min(self.window);
        if n <= 1 {
            return (x, y);
        }
        let mut total = 0.0;
        let mut wx = 0.0;
        let mut wy = 0.0;
        for i in 0..n {
            let index = (self.head + self.xs.len() * 2 - n + i) % self.xs.len();
            let weight = 2f64.powi(i as i32);
            total += weight;
            wx += self.xs[index] * weight;
            wy += self.ys[index] * weight;
        }
        (wx / total, wy / total)
    }
}

fn display_scale(
    bounds: &Rc<Cell<Option<Bounds<Pixels>>>>,
    doc_size: (u32, u32),
    camera: Option<crate::render::canvas_bridge::Camera>,
) -> f64 {
    let Some(bounds) = bounds.get() else {
        return 1.0;
    };
    if doc_size.0 == 0 || doc_size.1 == 0 {
        return 1.0;
    }
    let viewport_w: f32 = bounds.size.width.into();
    let viewport_h: f32 = bounds.size.height.into();
    crate::render::canvas_bridge::camera_placement(
        viewport_w,
        viewport_h,
        doc_size.0 as f32,
        doc_size.1 as f32,
        camera,
    )
    .map(|(_, _, page_w, _)| (page_w / doc_size.0 as f32) as f64)
    .filter(|scale| scale.is_finite() && *scale > 0.0)
    .map(|scale| scale.min(1.0))
    .unwrap_or(1.0)
}

/// Edge cap for one scaled preview raster. `PHOTREZ_PREVIEW_MAX_EDGE` allows
/// the A/B sweep (128/256/384) without a rebuild.
fn preview_max_edge() -> u32 {
    std::env::var("PHOTREZ_PREVIEW_MAX_EDGE")
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .unwrap_or(256)
        .clamp(64, 1024)
}

pub(crate) fn canvas_doc_coords_free(
    position: Point<Pixels>,
    bounds: &Rc<Cell<Option<Bounds<Pixels>>>>,
    doc_size: (u32, u32),
    camera: Option<crate::render::canvas_bridge::Camera>,
) -> Option<(f64, f64)> {
    let bounds = bounds.get()?;
    let local_x: f32 = f32::from(position.x) - f32::from(bounds.origin.x);
    let local_y: f32 = f32::from(position.y) - f32::from(bounds.origin.y);
    let viewport_w: f32 = bounds.size.width.into();
    let viewport_h: f32 = bounds.size.height.into();
    let (doc_w, doc_h) = (doc_size.0 as f32, doc_size.1 as f32);
    let (page_x, page_y, page_w, page_h) = crate::render::canvas_bridge::camera_placement(
        viewport_w, viewport_h, doc_w, doc_h, camera,
    )?;
    if page_w <= 0.0 || page_h <= 0.0 {
        return None;
    }
    Some((
        ((local_x - page_x) / page_w * doc_w) as f64,
        ((local_y - page_y) / page_h * doc_h) as f64,
    ))
}

pub(crate) fn canvas_doc_coords(
    position: Point<Pixels>,
    bounds: &Rc<Cell<Option<Bounds<Pixels>>>>,
    doc_size: (u32, u32),
    camera: Option<crate::render::canvas_bridge::Camera>,
) -> Option<(f64, f64)> {
    let bounds = bounds.get()?;
    let local_x: f32 = position.x.into();
    let local_y: f32 = position.y.into();
    let local_x = local_x - f32::from(bounds.origin.x);
    let local_y = local_y - f32::from(bounds.origin.y);
    let viewport_w: f32 = bounds.size.width.into();
    let viewport_h: f32 = bounds.size.height.into();
    let (doc_w, doc_h) = (doc_size.0 as f32, doc_size.1 as f32);
    let (page_x, page_y, page_w, page_h) = crate::render::canvas_bridge::camera_placement(
        viewport_w, viewport_h, doc_w, doc_h, camera,
    )?;
    if local_x < page_x
        || local_y < page_y
        || local_x >= page_x + page_w
        || local_y >= page_y + page_h
    {
        return None;
    }
    let x = ((local_x - page_x) / page_w * doc_w).clamp(0.0, (doc_w - 1.0).max(0.0));
    let y = ((local_y - page_y) / page_h * doc_h).clamp(0.0, (doc_h - 1.0).max(0.0));
    Some((x as f64, y as f64))
}

/// Window position to document coordinates without the page containment test.
/// Pointer-following chrome (the paint cursor ring) stays visible over the
/// pasteboard because it maps through the camera's screen-to-document transform
/// without the page containment test.
pub(crate) fn canvas_doc_position(
    position: Point<Pixels>,
    bounds: &Rc<Cell<Option<Bounds<Pixels>>>>,
    doc_size: (u32, u32),
    camera: Option<crate::render::canvas_bridge::Camera>,
) -> Option<(f64, f64)> {
    let bounds = bounds.get()?;
    let local_x = f32::from(position.x) - f32::from(bounds.origin.x);
    let local_y = f32::from(position.y) - f32::from(bounds.origin.y);
    let viewport_w: f32 = bounds.size.width.into();
    let viewport_h: f32 = bounds.size.height.into();
    let (doc_w, doc_h) = (doc_size.0 as f32, doc_size.1 as f32);
    let (page_x, page_y, page_w, page_h) = crate::render::canvas_bridge::camera_placement(
        viewport_w, viewport_h, doc_w, doc_h, camera,
    )?;
    let x = (local_x - page_x) / page_w * doc_w;
    let y = (local_y - page_y) / page_h * doc_h;
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    Some((x as f64, y as f64))
}

fn normalized_rect(start: (f64, f64), end: (f64, f64)) -> (f64, f64, f64, f64) {
    let x = start.0.min(end.0);
    let y = start.1.min(end.1);
    let width = (start.0 - end.0).abs().max(1.0);
    let height = (start.1 - end.1).abs().max(1.0);
    (x, y, width, height)
}

/// The drag rectangle for the pending tool: pixel selections honour Shift
/// (square) and Alt (grow from the centre); every other tool keeps the plain
/// normalized drag rectangle.
fn gesture_rect(pending: &PendingGesture) -> (f64, f64, f64, f64) {
    if pending.tool != ToolId::Selection {
        return normalized_rect(pending.start, pending.last);
    }
    let (start_x, start_y) = pending.start;
    let (end_x, end_y) = pending.last;
    let dx = end_x - start_x;
    let dy = end_y - start_y;
    let mut width = dx.abs();
    let mut height = dy.abs();
    if pending.shift {
        let side = width.max(height);
        width = side;
        height = side;
    }
    let (x, y) = if pending.alt {
        (start_x - width / 2.0, start_y - height / 2.0)
    } else {
        (
            if dx >= 0.0 { start_x } else { start_x - width },
            if dy >= 0.0 { start_y } else { start_y - height },
        )
    };
    (x, y, width.max(1.0), height.max(1.0))
}

fn hex_color(color: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

fn shape_tool_kind(kind: crate::state::ShapeTool) -> ShapeKind {
    use crate::state::ShapeTool;
    match kind {
        ShapeTool::Rect => ShapeKind::Rect,
        ShapeTool::Ellipse => ShapeKind::Ellipse,
        ShapeTool::Line => ShapeKind::Line,
        ShapeTool::Triangle => ShapeKind::Triangle,
        ShapeTool::Star => ShapeKind::Star,
        ShapeTool::BlockArrow => ShapeKind::BlockArrow,
        ShapeTool::Heart => ShapeKind::Heart,
        ShapeTool::Diamond => ShapeKind::Diamond,
        ShapeTool::SpeechBubble => ShapeKind::SpeechBubble,
        ShapeTool::Hexagon => ShapeKind::Hexagon,
    }
}

pub(crate) fn next_layer_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("native-layer-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// One in-flight rotation of the committed pixel selection.
#[derive(Clone, Debug)]
pub(crate) struct SelectionRotationDrag {
    selection: SelectionState,
    start_angle: f64,
    reference_angle: f64,
    pub(crate) preview_angle: f64,
}
