//! Pointer routing for the canvas: what a press, a move, a hover and a release
//! do, and how the context menu gets closed.

use super::*;

impl App {
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
}
