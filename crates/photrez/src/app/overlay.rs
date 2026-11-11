//! The canvas overlay: what the shell draws on top of the document, plus the
//! accessors that let a test read the cursor and frame it chose.

use super::*;

use crate::display::canvas_bridge::{BrushCursor, BrushRing};

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
        if state.active_tool == crate::state::ToolId::Selection && !state.overlay_owns_canvas() {
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
        // The Move tool paints the same rotate sprite on a rotate hover, so
        // the pointer reads as a rotation rather than a crosshair. Windows
        // uses the native cursor instead, so the sprite would double it.
        if state.active_tool == crate::state::ToolId::Move
            && state.show_transform_controls
            && self.pan.is_none()
            && !self.space_pressed
            && !crate::app::cursor::native_rotate_available()
            && !state.overlay_owns_canvas()
        {
            if let (Some(position), Some(doc_size)) = (
                self.hover_pos,
                self.active_document
                    .as_ref()
                    .map(|document| document.canvas.dimensions()),
            ) {
                if let Ok((_, layer)) = self.selected_layer_for_cursor(cx) {
                    if crate::app::transform_geometry::layer_can_transform(&layer) {
                        let zoom = self
                            .effective_camera()
                            .map(|camera| camera.zoom as f64)
                            .unwrap_or(1.0);
                        let (frame_w, frame_h) =
                            crate::app::transform_geometry::layer_frame_size(&layer);
                        if let Some((x, y)) = crate::app::canvas_input::canvas_doc_coords_free(
                            position,
                            &self.canvas_bounds,
                            doc_size,
                            self.active_camera(),
                        ) {
                            let hover = crate::app::transform_geometry::detect_handle(
                                crate::app::transform_geometry::Point { x, y },
                                &layer.transform,
                                frame_w,
                                frame_h,
                                zoom,
                            );
                            if hover == Some(crate::app::transform_geometry::Handle::Rotate) {
                                let centre = crate::app::transform_geometry::layer_center(
                                    &layer.transform,
                                    frame_w,
                                    frame_h,
                                );
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
            if state.selected_layers.len() > 1 {
                if let Some(frame) = self.group_overlay_frame(cx) {
                    overlay.transform = Some(frame);
                    overlay.transform_controls = state.show_transform_controls;
                }
            } else if let Ok((_, layer)) = self.selected_layer_for_cursor(cx) {
                // A single selected layer under Move keeps its frame at
                // rest, like the reference editor: the bounding box and its
                // handles stay visible so the layer is always grabbable. A
                // fully locked layer is skipped: its handles would promise a
                // drag the transform refuses.
                let transformable = crate::app::transform_geometry::layer_can_transform(&layer);
                overlay.transform_controls = state.show_transform_controls && transformable;
                if transformable {
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
        if !crate::app::ring_overlay::visible() {
            overlay.brush_cursor = self.brush_cursor_shape(cx);
        }
        if !crate::app::cursor::native_tool_cursor_available() {
            let state = self.state.read(cx);
            let policy = self.canvas_cursor_policy(cx);
            if self.hover_pos.is_some()
                && !state.overlay_owns_canvas()
                && self.pan.is_none()
                && !self.space_pressed
            {
                if let (Some(tool_index), Some(position)) = (
                    crate::app::cursor::tool_cursor_index(state.active_tool, policy),
                    self.hover_pos,
                ) {
                    overlay.tool_cursor = Some(crate::display::canvas_bridge::ToolCursorOverlay {
                        position: [f32::from(position.x), f32::from(position.y)],
                        index: tool_index,
                    });
                }
            }
        }
        overlay
    }

    fn active_canvas_dimensions(&self) -> Option<(u32, u32)> {
        self.active_document
            .as_ref()
            .map(|document| document.canvas.dimensions())
            .or_else(|| {
                self.demo_canvas
                    .as_ref()
                    .map(crate::display::canvas_bridge::CanvasImage::dimensions)
            })
    }

    /// The brush ring in document space, or `None` when no ring may show.
    pub(crate) fn brush_cursor_shape(&self, cx: &Context<Self>) -> Option<BrushCursor> {
        if self.pan.is_some() || self.space_pressed {
            return None;
        }
        let (is_eraser, size, hardness) = {
            let state = self.state.read(cx);
            if state.overlay_owns_canvas() {
                return None;
            }
            match state.active_tool {
                crate::state::ToolId::Eraser => (true, state.eraser_size, state.eraser_hardness),
                crate::state::ToolId::Brush => (false, state.brush_size, state.brush_hardness),
                _ => return None,
            }
        };
        let position = self.hover_pos?;
        let (doc_w, doc_h) = self.active_canvas_dimensions()?;
        let center = crate::app::canvas_input::canvas_doc_position(
            position,
            &self.canvas_bounds,
            (doc_w, doc_h),
            self.active_camera(),
        )?;
        let radius_scale =
            photrez_core::raster::brush::tip::get_brush_cursor_radius_scale(hardness);
        let base = (size / 2.0) * radius_scale;
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
        Some(BrushCursor {
            center: [center.0, center.1],
            radii: [base * scale_x.abs(), base * scale_y.abs()],
            rotation,
            crosshair: !is_eraser,
        })
    }

    /// The brush ring in window space for the native overlay and the painted
    /// fallback.
    pub(crate) fn brush_ring_for_overlay(&self, cx: &Context<Self>) -> Option<BrushRing> {
        let cursor = self.brush_cursor_shape(cx)?;
        let position = self.hover_pos?;
        let (doc_w, doc_h) = self.active_canvas_dimensions()?;
        let scale = crate::app::canvas_input::display_scale(
            &self.canvas_bounds,
            (doc_w, doc_h),
            self.active_camera(),
        ) as f32;
        Some(BrushRing {
            center: [f32::from(position.x), f32::from(position.y)],
            radii: [
                cursor.radii[0] as f32 * scale,
                cursor.radii[1] as f32 * scale,
            ],
            rotation: cursor.rotation as f32,
            crosshair: cursor.crosshair,
        })
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
        self.brush_cursor_shape(cx)
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
