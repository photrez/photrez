//! The canvas overlay: what the shell draws on top of the document, plus the
//! accessors that let a test read the cursor and frame it chose.

use super::*;

impl App {
    /// The selection and crop chrome painted over the page this frame.
    ///
    /// The committed selection is document state; the drag preview is UI
    /// state. A live drag wins so the marquee follows the pointer before the
    /// core command commits.
    pub(super) fn canvas_overlay(&self, cx: &Context<Self>) -> CanvasOverlay {
        use photrez_core::document::content::SelectionShape as CoreSelectionShape;
        let mut overlay = CanvasOverlay::default();
        let mut selection_inverted = false;
        if let Some(doc_id) = self.active_document.as_ref().map(|doc| doc.doc_id.clone()) {
            if let Some(selection) = self
                .sessions
                .get(&doc_id)
                .and_then(|session| session.loaded_document().content_snapshot())
                .and_then(|content| content.selection.clone())
            {
                overlay.selection = Some(crate::display::canvas_bridge::SelectionOverlay {
                    x: selection.x,
                    y: selection.y,
                    w: selection.width,
                    h: selection.height,
                    ellipse: matches!(selection.shape, Some(CoreSelectionShape::Ellipse)),
                    angle: selection.angle,
                    edit_mode: false,
                });
                selection_inverted = selection.inverted.unwrap_or(false);
            }
        }
        let state = self.state.read(cx);
        if let Some(drag) = state.selection_drag {
            overlay.selection = Some(crate::display::canvas_bridge::SelectionOverlay {
                x: drag[0],
                y: drag[1],
                w: drag[2],
                h: drag[3],
                ellipse: state.selection_shape == crate::state::SelectionShape::Ellipse,
                angle: 0.0,
                edit_mode: false,
            });
        }
        if let Some(rotation) = self.selection_rotation_drag.as_ref() {
            if let Some(selection) = overlay.selection.as_mut() {
                selection.angle = rotation.preview_angle;
            }
        }
        if let Some(selection) = overlay.selection.as_mut() {
            selection.edit_mode = self.selection_edit_mode
                && state.active_tool == crate::state::ToolId::Selection
                && state.selection_drag.is_none()
                && !selection_inverted;
        }
        // The rotate sprite follows the pointer angle about
        // the selection centre while a pin drag rotates, and on the pin hover
        // in edit mode.
        if state.active_tool == crate::state::ToolId::Selection {
            let mut sprite_point = None;
            if self.selection_rotation_drag.is_some() {
                sprite_point = self.hover_pos;
            } else if overlay
                .selection
                .as_ref()
                .is_some_and(|selection| selection.edit_mode)
            {
                if let (Some(position), Some(doc_size)) = (
                    self.hover_pos,
                    self.active_document
                        .as_ref()
                        .map(|document| document.canvas.dimensions()),
                ) {
                    if let Some(selection) = overlay.selection.as_ref() {
                        let zoom = self
                            .effective_camera()
                            .map(|camera| camera.zoom as f64)
                            .unwrap_or(1.0);
                        let (top_mid, normal) = crate::app::transform_geometry::selection_pin(
                            selection.x,
                            selection.y,
                            selection.w,
                            selection.h,
                            selection.angle,
                        );
                        let pin = (
                            top_mid.x + normal.x * 24.0 / zoom,
                            top_mid.y + normal.y * 24.0 / zoom,
                        );
                        if let Some((x, y)) = crate::app::canvas_input::canvas_doc_coords(
                            position,
                            &self.canvas_bounds,
                            doc_size,
                            self.active_camera(),
                        ) {
                            let distance =
                                ((x - pin.0).powi(2) + (y - pin.1).powi(2)).sqrt() * zoom;
                            if distance < 12.0 {
                                sprite_point = Some(position);
                            }
                        }
                    }
                }
            }
            if let (Some(position), Some(selection)) = (sprite_point, overlay.selection.as_ref()) {
                if let Some(doc_size) = self
                    .active_document
                    .as_ref()
                    .map(|document| document.canvas.dimensions())
                {
                    if let Some((x, y)) = crate::app::canvas_input::canvas_doc_coords(
                        position,
                        &self.canvas_bounds,
                        doc_size,
                        self.active_camera(),
                    ) {
                        let centre = crate::app::transform_geometry::Point {
                            x: selection.x + selection.w / 2.0,
                            y: selection.y + selection.h / 2.0,
                        };
                        let angle = crate::app::transform_geometry::angle_about(
                            centre,
                            crate::app::transform_geometry::Point { x, y },
                        ) + 45.0;
                        overlay.rotate_cursor =
                            Some(crate::display::canvas_bridge::RotateCursorOverlay {
                                position: [f32::from(position.x), f32::from(position.y)],
                                angle: angle as f32,
                            });
                    }
                }
            }
        }
        if let Some(crop) = state.crop_rect {
            overlay.crop = Some((crop[0], crop[1], crop[2], crop[3]));
        }
        overlay.guides = self.snap_guides.clone();
        if let Some(pending) = self
            .pending_gesture
            .as_ref()
            .filter(|gesture| gesture.tool == crate::state::ToolId::Gradient)
        {
            overlay.gradient_line = Some(GradientLine {
                start: [pending.start.0, pending.start.1],
                end: [pending.last.0, pending.last.1],
                radial: state.gradient_radial,
            });
        }
        overlay.move_marquee = self
            .move_marquee
            .as_ref()
            .and_then(|marquee| marquee.rect());
        // A cleared selection keeps row 0 as the native primary; it must not
        // paint a transform frame for a layer the user no longer selected.
        // The canvas viewport paints the frame for the Shape tool too, so
        // a shape drawn and auto-selected keeps its handles until the tool
        // changes; the frame bounds come from `layer_frame_size`, which reads
        // the shape rectangle rather than the document-sized bitmap.
        if matches!(
            state.active_tool,
            crate::state::ToolId::Move | crate::state::ToolId::Shape
        ) && state.selected_layers.contains(&state.selected_layer)
        {
            overlay.transform_controls = state.show_transform_controls;
            if state.selected_layers.len() > 1 {
                if let Some(frame) = self.group_overlay_frame(cx) {
                    overlay.transform = Some(frame);
                }
            } else if let Ok((_, layer)) = self.selected_layer_for_cursor(cx) {
                // The 0.1 build gates its rest frame on a multi-layer
                // selection: with one layer selected and Move active its
                // canvas area holds 0 accent pixels, where this build held
                // 2,653. A single layer gets handles only while a transform
                // is actually being dragged.
                let dragging = self
                    .transform_drag
                    .as_ref()
                    .is_some_and(|drag| drag.layer_id == layer.id);
                if dragging || !matches!(state.active_tool, crate::state::ToolId::Move) {
                    let transform = self
                        .transform_drag
                        .as_ref()
                        .filter(|drag| drag.layer_id == layer.id)
                        .map(|drag| drag.preview.clone())
                        .unwrap_or_else(|| layer.transform.clone());
                    let (frame_w, frame_h) =
                        crate::app::transform_geometry::layer_frame_size(&layer);
                    let corners =
                        crate::app::transform_geometry::layer_corners(&transform, frame_w, frame_h);
                    let mid =
                        |a: crate::app::transform_geometry::Point,
                         b: crate::app::transform_geometry::Point| {
                            crate::app::transform_geometry::Point {
                                x: (a.x + b.x) / 2.0,
                                y: (a.y + b.y) / 2.0,
                            }
                        };
                    let handles = [
                        corners[0],
                        corners[1],
                        corners[2],
                        corners[3],
                        mid(corners[0], corners[1]),
                        mid(corners[1], corners[2]),
                        mid(corners[2], corners[3]),
                        mid(corners[3], corners[0]),
                    ];
                    overlay.transform = Some(TransformFrame {
                        corners: corners.map(|point| [point.x, point.y]),
                        handles: handles.map(|point| [point.x, point.y]),
                    });
                }
            }
        }
        // A non-active layer under the Move pointer gets a
        // 1 px accent outline. No drag, marquee or transform session may own the
        // pointer while it paints.
        if state.active_tool == crate::state::ToolId::Move
            && self.pending_gesture.is_none()
            && self.transform_drag.is_none()
            && self.move_marquee.is_none()
        {
            if let (Some(position), Some(_)) = (self.hover_pos, self.canvas_bounds.get()) {
                let doc_size = self
                    .active_document
                    .as_ref()
                    .map(|document| document.canvas.dimensions())
                    .or_else(|| self.demo_canvas.as_ref().map(CanvasImage::dimensions));
                if let Some(doc_size) = doc_size {
                    if let Some((x, y)) = crate::app::canvas_input::canvas_doc_coords(
                        position,
                        &self.canvas_bounds,
                        doc_size,
                        self.active_camera(),
                    ) {
                        if let Some(core_id) = self.layer_hit_at(x, y) {
                            let active_core = state
                                .layers
                                .iter()
                                .find(|row| row.id == state.selected_layer)
                                .and_then(|row| row.core_id.as_deref());
                            if active_core != Some(core_id.as_str()) {
                                if let Some(doc_id) = self
                                    .active_document
                                    .as_ref()
                                    .map(|document| document.doc_id.clone())
                                {
                                    overlay.hover_highlight = self
                                        .sessions
                                        .get(&doc_id)
                                        .and_then(|session| {
                                            session.loaded_document().content_snapshot()
                                        })
                                        .and_then(|content| {
                                            content
                                                .layers
                                                .iter()
                                                .find(|layer| layer.id == core_id)
                                                .map(|layer| {
                                                    (
                                                        layer.transform.x,
                                                        layer.transform.y,
                                                        layer.width * layer.transform.scale_x.abs(),
                                                        layer.height
                                                            * layer.transform.scale_y.abs(),
                                                    )
                                                })
                                        });
                                }
                            }
                        }
                    }
                }
            }
        }
        // Brush and eraser hide the system cursor and
        // paint the 20% alpha ring at the pointer. Alt/space take over the
        // pointer for sampling and panning, and the ring is hidden then.
        if matches!(
            state.active_tool,
            crate::state::ToolId::Brush | crate::state::ToolId::Eraser
        ) && self.pan.is_none()
            && !self.space_pressed
        {
            if let (Some(position), Some(bounds)) = (self.hover_pos, self.canvas_bounds.get()) {
                let px_x: f32 = position.x.into();
                let px_y: f32 = position.y.into();
                let origin_x = f32::from(bounds.origin.x);
                let origin_y = f32::from(bounds.origin.y);
                let width: f32 = bounds.size.width.into();
                let height: f32 = bounds.size.height.into();
                let inside = px_x >= origin_x
                    && px_y >= origin_y
                    && px_x < origin_x + width
                    && px_y < origin_y + height;
                if inside {
                    let doc_size = self
                        .active_document
                        .as_ref()
                        .map(|document| document.canvas.dimensions())
                        .or_else(|| self.demo_canvas.as_ref().map(CanvasImage::dimensions));
                    if let Some(doc_size) = doc_size {
                        if let Some(center) = crate::app::canvas_input::canvas_doc_position(
                            position,
                            &self.canvas_bounds,
                            doc_size,
                            self.active_camera(),
                        ) {
                            let is_eraser = state.active_tool == crate::state::ToolId::Eraser;
                            let (size, hardness) = if is_eraser {
                                (state.eraser_size, state.eraser_hardness)
                            } else {
                                (state.brush_size, state.brush_hardness)
                            };
                            let scale =
                                photrez_core::raster::brush::tip::get_brush_cursor_radius_scale(
                                    hardness,
                                );
                            let base = (size / 2.0) * scale;
                            let (scale_x, scale_y, rotation) = self
                                .selected_layer_for_cursor(cx)
                                .map(|(_, layer)| {
                                    (
                                        layer.transform.scale_x,
                                        layer.transform.scale_y,
                                        layer.transform.rotation,
                                    )
                                })
                                .unwrap_or((1.0, 1.0, 0.0));
                            overlay.brush_cursor = Some(BrushCursor {
                                center: [center.0, center.1],
                                radii: [base * scale_x.abs(), base * scale_y.abs()],
                                rotation,
                                crosshair: !is_eraser,
                            });
                        }
                    }
                }
            }
        }
        overlay
    }
    #[cfg(test)]
    pub(crate) fn hover_highlight_for_test(
        &self,
        cx: &Context<Self>,
    ) -> Option<(f64, f64, f64, f64)> {
        self.canvas_overlay(cx).hover_highlight
    }
    #[cfg(test)]
    pub(crate) fn rotate_cursor_for_test(
        &self,
        cx: &Context<Self>,
    ) -> Option<crate::display::canvas_bridge::RotateCursorOverlay> {
        self.canvas_overlay(cx).rotate_cursor
    }
    #[cfg(test)]
    pub(crate) fn brush_cursor_for_test(&self, cx: &Context<Self>) -> Option<BrushCursor> {
        self.canvas_overlay(cx).brush_cursor
    }
    #[cfg(test)]
    pub(crate) fn gradient_line_for_test(&self, cx: &Context<Self>) -> Option<GradientLine> {
        self.canvas_overlay(cx).gradient_line
    }
    #[cfg(test)]
    pub(crate) fn transform_frame_for_test(&self, cx: &Context<Self>) -> Option<TransformFrame> {
        self.canvas_overlay(cx).transform
    }
}
