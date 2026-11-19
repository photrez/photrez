//! Pointer routing for the canvas: what a press, a move, a hover and a release
//! do, and how the context menu gets closed.

use super::*;

fn crop_pointer_screen_point(screen: (f64, f64)) -> Point<Pixels> {
    gpui_kit::point(gpui_kit::px(screen.0 as f32), gpui_kit::px(screen.1 as f32))
}

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
            if let Some(point) = canvas_doc_coords_free(event.position, bounds, doc_size, camera)
                && self.selection_rotate_start(bounds, doc_size, point, cx)
            {
                return;
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
        let camera = self.active_camera();
        let tool = self.state.read(cx).active_tool;
        let free_mapped = canvas_doc_coords_free(event.position, bounds, doc_size, camera);
        let contained_mapped = canvas_doc_coords(event.position, bounds, doc_size, camera);
        let crop_free_hit = if tool == ToolId::Crop && contained_mapped.is_none() {
            let zoom = self
                .effective_camera()
                .map(|camera| f64::from(camera.zoom))
                .unwrap_or(1.0);
            let state = self.state.read(cx);
            state.crop_rect.and_then(|rect| {
                free_mapped.filter(|&(x, y)| {
                    App::crop_pointer_target_at(
                        (x, y),
                        rect,
                        zoom,
                        state.crop_interaction,
                        state.crop_rotation,
                    )
                    .is_some()
                })
            })
        } else {
            None
        };
        let mapped = if tool == ToolId::Move {
            free_mapped
        } else if tool == ToolId::Crop {
            contained_mapped.or(crop_free_hit)
        } else {
            contained_mapped
        };
        if tool == ToolId::Crop
            && contained_mapped.is_none()
            && crop_free_hit.is_none()
            && let Some((x, y)) = free_mapped
        {
            let (interaction, snapshot) = {
                let state = self.state.read(cx);
                (
                    state.crop_interaction,
                    crate::app::crop::crop_snapshot(state),
                )
            };
            if interaction == crate::crop_geometry::CropInteraction::Classic {
                if let Some(snapshot) = snapshot {
                    let screen = (f64::from(event.position.x), f64::from(event.position.y));
                    self.pending_gesture = Some(PendingGesture {
                        tool: ToolId::Crop,
                        start: (x, y),
                        last: (x, y),
                        start_screen: screen,
                        last_screen: screen,
                        crop_start: Some(snapshot),
                        crop_pasteboard: true,
                        crop_replacement_started: false,
                        crop_modern_create: false,
                        crop_modern_drag_started: false,
                        crop_modern_preview: None,
                        crop_snap_targets: None,
                        shift: event.modifiers.shift,
                        alt: event.modifiers.alt,
                    });
                }
            } else {
                let screen = (f64::from(event.position.x), f64::from(event.position.y));
                self.pending_gesture = Some(PendingGesture {
                    tool: ToolId::Crop,
                    start: (x, y),
                    last: (x, y),
                    start_screen: screen,
                    last_screen: screen,
                    crop_start: snapshot,
                    crop_pasteboard: true,
                    crop_replacement_started: false,
                    crop_modern_create: true,
                    crop_modern_drag_started: false,
                    crop_modern_preview: None,
                    crop_snap_targets: crate::app::crop::crop_snap_targets(self),
                    shift: event.modifiers.shift,
                    alt: event.modifiers.alt,
                });
            }
            return;
        }
        if let Some(session) = self.state.read(cx).text_edit.clone() {
            let inside_editor = free_mapped.is_some_and(|(x, y)| {
                let zoom = display_scale(bounds, doc_size, camera).max(f64::EPSILON);
                let min_box = 24.0 / zoom;
                let width = f64::from(session.preview.0).max(min_box);
                let height = f64::from(session.preview.1).max(min_box);
                x >= session.doc_x
                    && x <= session.doc_x + width
                    && y >= session.doc_y
                    && y <= session.doc_y + height
            });
            if !inside_editor {
                let _ = self.commit_text_edit(cx);
            }
            return;
        }
        let Some((x, y)) = mapped else {
            return;
        };
        if event.click_count >= 2
            && matches!(tool, ToolId::Move | ToolId::Selection)
            && let Some(core_id) = self.layer_hit_at(x, y)
        {
            let text_layer = self.active_document.as_ref().and_then(|document| {
                self.sessions
                    .get(&document.doc_id)
                    .and_then(|session| session.loaded_document().content_snapshot())
                    .and_then(|content| {
                        content
                            .layers
                            .iter()
                            .find(|layer| layer.id == core_id)
                            .filter(|layer| {
                                matches!(layer.layer_type, LayerType::Text)
                                    && layer.text_data.is_some()
                            })
                            .cloned()
                    })
            });
            if let Some(layer) = text_layer {
                let Some(text_data) = layer.text_data.clone() else {
                    return;
                };
                self.move_auto_select_click(x, y, false, cx);
                self.select_canvas_tool(ToolId::Text, cx);
                self.begin_text_edit_existing(
                    layer.id,
                    layer.transform.x,
                    layer.transform.y,
                    text_data,
                    cx,
                );
                return;
            }
        }
        if tool == ToolId::Crop
            && let Some(rect) = self.state.read(cx).crop_rect
        {
            let zoom = self
                .effective_camera()
                .map(|camera| camera.zoom as f64)
                .unwrap_or(1.0);
            let (target, start_snapshot, interaction) = {
                let state = self.state.read(cx);
                (
                    App::crop_pointer_target_at(
                        (x, y),
                        rect,
                        zoom,
                        state.crop_interaction,
                        state.crop_rotation,
                    ),
                    crate::crop_geometry::CropSnapshot {
                        frame: rect,
                        offset: state.crop_offset,
                        rotation: state.crop_rotation,
                    },
                    state.crop_interaction,
                )
            };
            if event.click_count >= 2
                && self.crop_drag.is_none()
                && self.pending_gesture.is_none()
                && target == Some(crate::app::crop::CropPointerTarget::Move)
            {
                match self.apply_crop(cx) {
                    Ok(()) => self.set_status_error(None, cx),
                    Err(error) => self.set_status_error(Some(format!("Crop: {error}")), cx),
                }
                return;
            }
            if let Some(target) = target {
                let snap_targets = if interaction == crate::crop_geometry::CropInteraction::Classic
                {
                    crate::app::crop::crop_snap_targets(self).map(std::rc::Rc::new)
                } else {
                    None
                };
                self.snap_guides.clear();
                self.crop_drag = Some(crate::app::crop::CropDrag {
                    handle: match target {
                        crate::app::crop::CropPointerTarget::Resize(handle) => Some(handle),
                        crate::app::crop::CropPointerTarget::Move
                        | crate::app::crop::CropPointerTarget::Rotate => None,
                    },
                    rotate: target == crate::app::crop::CropPointerTarget::Rotate,
                    start_snapshot,
                    start_doc: (x, y),
                    start_screen: (f64::from(event.position.x), f64::from(event.position.y)),
                    interaction,
                    snap_targets,
                });
                return;
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
                if event.modifiers.alt
                    && let Err(error) = self.duplicate_selected_layer_id(cx)
                {
                    self.set_status_error(Some(format!("Alt-copy: {error}")), cx);
                    return;
                }
                let modify = event.modifiers.shift || event.modifiers.control;
                if self.state.read(cx).move_auto_select
                    && self.move_auto_select_click(x, y, modify, cx)
                {
                    return;
                }
                if self.begin_transform_drag((x, y), zoom, cx) {
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
                let (crop_start, crop_modern_create) = if tool == ToolId::Crop {
                    let state = self.state.read(cx);
                    (
                        crate::app::crop::crop_snapshot(state),
                        state.crop_interaction == crate::crop_geometry::CropInteraction::Modern,
                    )
                } else {
                    (None, false)
                };
                let crop_snap_targets = if crop_modern_create {
                    crate::app::crop::crop_snap_targets(self)
                } else {
                    None
                };
                self.pending_gesture = Some(PendingGesture {
                    tool,
                    start: (x, y),
                    last: (x, y),
                    start_screen: (f64::from(event.position.x), f64::from(event.position.y)),
                    last_screen: (f64::from(event.position.x), f64::from(event.position.y)),
                    crop_start,
                    crop_pasteboard: false,
                    crop_replacement_started: false,
                    crop_modern_create,
                    crop_modern_drag_started: false,
                    crop_modern_preview: None,
                    crop_snap_targets,
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
        if let Some(drag) = self.crop_drag.clone() {
            let camera = self.active_camera();
            let Some((x, y)) = canvas_doc_coords_free(event.position, bounds, doc_size, camera)
            else {
                return;
            };
            let (mode, ratio, snap_enabled) = {
                let state = self.state.read(cx);
                (
                    state.crop_mode,
                    crate::app::crop::crop_resize_ratio(state),
                    state.snap_enabled,
                )
            };
            let snap_zoom = bounds
                .get()
                .and_then(|viewport| {
                    crate::display::canvas_bridge::camera_placement(
                        viewport.size.width.into(),
                        viewport.size.height.into(),
                        doc_size.0 as f32,
                        doc_size.1 as f32,
                        self.active_camera(),
                    )
                })
                .map(|(_, _, page_width, _)| f64::from(page_width) / f64::from(doc_size.0))
                .filter(|zoom| zoom.is_finite() && *zoom > 0.0)
                .unwrap_or(1.0)
                .max(0.01);
            let zoom = self
                .effective_camera()
                .map(|camera| f64::from(camera.zoom))
                .unwrap_or(1.0);
            let mut update = crate::app::crop::crop_drag_update(
                &drag,
                crate::app::crop::CropDragInput {
                    doc: (x, y),
                    screen: (f64::from(event.position.x), f64::from(event.position.y)),
                    mode,
                    ratio,
                    zoom,
                    doc_size: self.active_document_size().unwrap_or((1.0, 1.0)),
                    shift: event.modifiers.shift,
                    alt: event.modifiers.alt,
                },
            );
            self.snap_guides.clear();
            if drag.interaction == crate::crop_geometry::CropInteraction::Classic
                && !drag.rotate
                && !event.modifiers.alt
                && snap_enabled
                && let Some(targets) = drag.snap_targets.as_deref()
            {
                let (frame, guides) = crate::crop_geometry::snap_classic_crop(
                    update.frame,
                    drag.handle,
                    targets,
                    12.0 / snap_zoom,
                );
                update.frame = frame;
                self.snap_guides = guides;
            }
            self.state.update(cx, |state, _| {
                crate::app::crop::restore_crop_snapshot(state, update)
            });
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
        let pending_crop_free_coordinates = self
            .pending_gesture
            .as_ref()
            .is_some_and(|pending| pending.crop_pasteboard || pending.crop_modern_create);
        let mapped = if pending_crop_free_coordinates {
            canvas_doc_coords_free(event.position, bounds, doc_size, camera)
        } else {
            canvas_doc_coords(event.position, bounds, doc_size, camera)
        };
        let Some((x, y)) = mapped else {
            return;
        };
        let snap_zoom = bounds
            .get()
            .and_then(|viewport| {
                crate::display::canvas_bridge::camera_placement(
                    viewport.size.width.into(),
                    viewport.size.height.into(),
                    doc_size.0 as f32,
                    doc_size.1 as f32,
                    camera,
                )
            })
            .map(|(_, _, page_width, _)| f64::from(page_width) / f64::from(doc_size.0))
            .filter(|zoom| zoom.is_finite() && *zoom > 0.0)
            .unwrap_or(1.0)
            .max(0.01);
        let mut gradient_badge = None;
        if let Some(pending) = self.pending_gesture.as_mut() {
            pending.last = (x, y);
            pending.last_screen = (f64::from(event.position.x), f64::from(event.position.y));
            if pending.crop_modern_create {
                if let Some(start) = canvas_doc_coords_free(
                    crop_pointer_screen_point(pending.start_screen),
                    bounds,
                    doc_size,
                    camera,
                ) {
                    pending.start = start;
                }
                if !pending.crop_modern_drag_started {
                    if !modern_crop_drag_started(pending) {
                        return;
                    }
                    pending.crop_modern_drag_started = true;
                }
            } else if pending.crop_pasteboard && !crop_replacement_drag_started(pending) {
                return;
            }
            if pending.crop_pasteboard
                && !pending.crop_modern_create
                && (pending.last.0 == pending.start.0 || pending.last.1 == pending.start.1)
            {
                return;
            }
            if pending.crop_pasteboard && !pending.crop_modern_create {
                pending.crop_replacement_started = true;
            }
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
            let preview = if pending.crop_modern_create {
                let mut preview = [
                    pending.start.0.min(pending.last.0),
                    pending.start.1.min(pending.last.1),
                    (pending.last.0 - pending.start.0).abs(),
                    (pending.last.1 - pending.start.1).abs(),
                ];
                let guides = if event.modifiers.alt || !self.state.read(cx).snap_enabled {
                    Vec::new()
                } else if let Some(targets) = &pending.crop_snap_targets {
                    let Some(viewport) = bounds.get() else {
                        return;
                    };
                    let viewport_doc_origin = (
                        (pending.start_screen.0 - f64::from(viewport.origin.x)) / snap_zoom
                            - pending.start.0,
                        (pending.start_screen.1 - f64::from(viewport.origin.y)) / snap_zoom
                            - pending.start.1,
                    );
                    let mut snap_preview = preview;
                    snap_preview[0] += viewport_doc_origin.0;
                    snap_preview[1] += viewport_doc_origin.1;
                    let (snapped, guides) = crate::crop_geometry::snap_crop_creation(
                        snap_preview,
                        targets,
                        12.0 / snap_zoom,
                    );
                    preview = snapped;
                    preview[0] -= viewport_doc_origin.0;
                    preview[1] -= viewport_doc_origin.1;
                    guides
                } else {
                    Vec::new()
                };
                self.snap_guides = guides;
                pending.crop_modern_preview = Some(preview);
                preview
            } else if pending.tool == ToolId::Crop {
                let state = self.state.read(cx);
                crate::crop_geometry::drag_create(
                    pending.start,
                    pending.last,
                    (!pending.crop_pasteboard)
                        .then(|| crate::app::crop::crop_resize_ratio(state))
                        .flatten(),
                )
            } else {
                let (x, y, width, height) = gesture_rect(pending);
                [x, y, width, height]
            };
            match pending.tool {
                ToolId::Selection => {
                    self.state
                        .update(cx, |state, _| state.selection_drag = Some(preview));
                    cx.notify();
                }
                ToolId::Crop => {
                    let classic_crop_pasteboard =
                        pending.crop_pasteboard && !pending.crop_modern_create;
                    if !pending.crop_modern_create {
                        self.state.update(cx, |state, _| {
                            state.crop_rect = Some(preview);
                            if classic_crop_pasteboard {
                                state.crop_hidden_preview = None;
                                state.crop_rotation = 0.0;
                            }
                        });
                    }
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
        let policy = self.canvas_cursor_policy(cx);
        crate::app::cursor::set_tool_cursor(crate::app::cursor::tool_cursor_index(tool, policy));
        if matches!(tool, ToolId::Brush | ToolId::Eraser) {
            let ring = self.brush_ring_for_overlay(cx);
            let visible = ring.is_some();
            let hide = visible && policy == CursorPolicy::SystemDefault;
            crate::app::cursor::set_brush_hidden(hide);
            let native =
                ring.is_some_and(|ring| crate::app::ring_overlay::show(ring, self.window_scale));
            if native != self.native_ring_active {
                self.native_ring_active = native;
                cx.notify();
            } else if !native && visible {
                cx.notify();
            }
            if !visible {
                crate::app::ring_overlay::hide();
                self.native_ring_active = false;
            }
        }
        if (tool == ToolId::Crop && self.state.read(cx).crop_rect.is_some()) || tool == ToolId::Move
        {
            let rotate =
                matches!(tool, ToolId::Crop | ToolId::Move) && policy == CursorPolicy::Rotate;
            let direction = if rotate {
                self.rotate_cursor_direction(cx).unwrap_or(0)
            } else {
                0
            };
            crate::app::cursor::set_rotate(rotate, direction);
            if policy != self.last_cursor
                || (rotate && !crate::app::cursor::native_rotate_available())
            {
                self.last_cursor = policy;
                cx.notify();
            }
        }
        self.sync_photrez_system_pointer(cx, tool, policy);
    }

    /// The 22.5-degree bucket of the pointer around the layer centre.
    pub(crate) fn rotate_cursor_direction(&self, cx: &Context<Self>) -> Option<u8> {
        let position = self.hover_pos?;
        let (doc_w, doc_h) = self.active_document.as_ref()?.canvas.dimensions();
        let (x, y) = canvas_doc_coords_free(
            position,
            &self.canvas_bounds,
            (doc_w, doc_h),
            self.active_camera(),
        )?;
        if self.state.read(cx).active_tool == ToolId::Crop {
            let rect = self.state.read(cx).crop_rect?;
            let center = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
            return Some(crate::app::cursor::direction_bucket(center, (x, y)));
        }
        let (_, layer) = self.selected_layer_for_cursor(cx).ok()?;
        let (frame_w, frame_h) = crate::app::transform_geometry::layer_frame_size(&layer);
        let center =
            crate::app::transform_geometry::layer_center(&layer.transform, frame_w, frame_h);
        Some(crate::app::cursor::direction_bucket(
            (center.x, center.y),
            (x, y),
        ))
    }

    /// The pointer left the canvas: no hover cursor or sprite may outlive it.
    pub(crate) fn canvas_hover_end(&mut self, cx: &mut Context<Self>) {
        let erase_brush_ring = self.hover_pos.is_some()
            && matches!(
                self.state.read(cx).active_tool,
                ToolId::Brush | ToolId::Eraser
            );
        self.hover_pos = None;
        crate::app::cursor::set_rotate(false, 0);
        crate::app::cursor::set_brush_hidden(false);
        crate::app::cursor::set_tool_cursor(None);
        crate::app::ring_overlay::hide();
        self.native_ring_active = false;
        let policy_changed = self.last_cursor != CursorPolicy::SystemDefault;
        if policy_changed {
            self.last_cursor = CursorPolicy::SystemDefault;
        }
        if erase_brush_ring || policy_changed {
            cx.notify();
        }
        self.release_photrez_system_pointer();
    }

    /// Re-evaluate native hover cursors after a key or tool change without a move.
    pub(crate) fn sync_hover_cursor(&mut self, cx: &Context<Self>) {
        crate::app::ring_overlay::hide();
        self.native_ring_active = false;
        let tool = self.state.read(cx).active_tool;
        let policy = self.canvas_cursor_policy(cx);
        let rotate = matches!(tool, ToolId::Crop | ToolId::Move) && policy == CursorPolicy::Rotate;
        let direction = if rotate {
            self.rotate_cursor_direction(cx).unwrap_or(0)
        } else {
            0
        };
        crate::app::cursor::set_rotate(rotate, direction);
        let tool_cursor = if self.hover_pos.is_some() {
            crate::app::cursor::tool_cursor_index(tool, policy)
        } else {
            None
        };
        crate::app::cursor::set_tool_cursor(tool_cursor);
        let brush_hidden =
            self.brush_cursor_shape(cx).is_some() && policy == CursorPolicy::SystemDefault;
        crate::app::cursor::set_brush_hidden(brush_hidden);
        self.sync_photrez_system_pointer(cx, tool, policy);
    }

    fn photrez_owns_pointer(&self, cx: &Context<Self>, tool: ToolId, policy: CursorPolicy) -> bool {
        if self.space_pressed || self.pan.is_some() || self.state.read(cx).overlay_owns_canvas() {
            return false;
        }
        if policy == CursorPolicy::NotAllowed {
            return false;
        }
        if matches!(tool, ToolId::Brush | ToolId::Eraser)
            && self.brush_cursor_shape(cx).is_some()
            && policy == CursorPolicy::SystemDefault
        {
            return !cfg!(windows);
        }
        if matches!(tool, ToolId::Crop | ToolId::Move)
            && policy == CursorPolicy::Rotate
            && !crate::app::cursor::native_rotate_available()
        {
            return true;
        }
        if crate::app::cursor::tool_cursor_index(tool, policy).is_some()
            && !crate::app::cursor::native_tool_cursor_available()
        {
            return true;
        }
        false
    }

    fn sync_photrez_system_pointer(
        &mut self,
        cx: &Context<Self>,
        tool: ToolId,
        policy: CursorPolicy,
    ) {
        let hide = self.photrez_owns_pointer(cx, tool, policy);
        if self.system_pointer_hidden == hide {
            return;
        }
        self.system_pointer_hidden = hide;
        crate::app::cursor::set_photrez_pointer_active(hide);
        let _ = cx;
    }

    pub(crate) fn system_pointer_hidden_for_gpui(&self) -> bool {
        self.system_pointer_hidden
    }

    fn release_photrez_system_pointer(&mut self) {
        if !self.system_pointer_hidden {
            return;
        }
        self.system_pointer_hidden = false;
        crate::app::cursor::set_photrez_pointer_active(false);
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
            self.snap_guides.clear();
            let changed = crate::app::crop::crop_snapshot(self.state.read(cx))
                .is_some_and(|snapshot| snapshot != drag.start_snapshot);
            if changed {
                self.state.update(cx, |state, _| {
                    state.crop_history.push(drag.start_snapshot);
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
        let pending_crop_free_coordinates = self
            .pending_gesture
            .as_ref()
            .is_some_and(|pending| pending.crop_pasteboard || pending.crop_modern_create);
        let mapped = if pending_crop_free_coordinates {
            canvas_doc_coords_free(event.position, bounds, doc_size, camera)
        } else {
            canvas_doc_coords(event.position, bounds, doc_size, camera)
        };
        let inside_surface = bounds
            .get()
            .map(|surface| surface.contains(&event.position))
            .unwrap_or(false);
        if let Some(pending) = self.pending_gesture.as_mut() {
            if pending.crop_modern_create {
                pending.shift = event.modifiers.shift;
                if pending.crop_modern_drag_started
                    && let (Some(start), Some(end)) = (
                        canvas_doc_coords_free(
                            crop_pointer_screen_point(pending.start_screen),
                            bounds,
                            doc_size,
                            camera,
                        ),
                        canvas_doc_coords_free(
                            crop_pointer_screen_point(pending.last_screen),
                            bounds,
                            doc_size,
                            camera,
                        ),
                    )
                {
                    pending.start = start;
                    pending.last = end;
                }
            } else {
                pending.last_screen = (f64::from(event.position.x), f64::from(event.position.y));
                if let Some((x, y)) = mapped {
                    pending.last = (x, y);
                }
            }
        }
        if let Some((x, y)) = mapped
            && self.input.has_active_brush()
        {
            let (x, y) = self.brush_axis_point(
                event.modifiers.shift,
                Some((f32::from(event.position.x), f32::from(event.position.y))),
                x,
                y,
            );
            let (smooth_x, smooth_y) = self.paint_smoother.add(x, y);
            self.input.update_brush(smooth_x, smooth_y);
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
