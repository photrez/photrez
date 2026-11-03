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

mod actions;
mod brush_stroke;
mod pointer;
mod selection_rotation;
mod test_accessors;
mod tool_applies;

impl App {
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
