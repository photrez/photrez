//! Slider input for every track host: click and drag map the pointer's x
//! position over the laid-out track to a value, and the arrow keys step that
//! value in its own units.

use gpui_kit::{AppContext, Context, KeyDownEvent, Window};

use crate::state::{SliderTarget, ToolId};

use super::brush_menu::paint_size_to_slider;
use super::App;

/// A value waiting for the next frame: one adjustment channel or one opacity.
/// The slider holds one drag at a time, so one slot is enough.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PendingSliderUpdate {
    Adjustment { channel: u8, value: f64 },
    Opacity { value: f64 },
}

pub(crate) type PreviewLayers = Vec<(String, u32, u32, Vec<u8>)>;
pub(crate) type PreviewSnapshot = (
    String,
    String,
    photrez_core::document::content::PtzDocumentContent,
    PreviewLayers,
    u32,
    u32,
);

/// Raster-only downscaled stack for a slider drag. `content` carries scaled
/// geometry; `layers` carries `(id, width, height, rgba)` per raster layer.
pub(crate) struct SliderPreview {
    doc_id: String,
    layer_id: String,
    doc_width: u32,
    doc_height: u32,
    content: photrez_core::document::content::PtzDocumentContent,
    layers: PreviewLayers,
}

pub(crate) struct PreviewInput {
    pub(crate) layers: PreviewLayers,
}

impl photrez_core::render::RenderInput for PreviewInput {
    fn layer_pixels(
        &self,
        _document_id: &str,
        layer_id: &str,
    ) -> Option<photrez_core::render::RenderLayerPixels<'_>> {
        let entry = self
            .layers
            .iter()
            .find(|(id, ..)| id.as_str() == layer_id)?;
        Some(photrez_core::render::RenderLayerPixels {
            width: entry.1,
            height: entry.2,
            pixels: &entry.3,
        })
    }
}

impl App {
    pub(crate) fn slider_pointer_down_for(
        &mut self,
        target: SliderTarget,
        x: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.slider_dragging = true;
        self.set_slider_from_x(target, x, window, cx);
    }

    pub(crate) fn slider_pointer_move_for(
        &mut self,
        target: SliderTarget,
        x: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.slider_dragging {
            self.set_slider_from_x(target, x, window, cx);
        }
    }

    pub(crate) fn slider_pointer_up(&mut self, cx: &mut Context<Self>) {
        self.slider_dragging = false;
        let pending = self.pending_slider_update.take();
        let last = self.last_slider_preview_update.take();
        self.slider_preview_next = None;
        self.slider_preview_job_active = false;
        self.slider_preview_job_generation = self.slider_preview_job_generation.wrapping_add(1);
        if self.slider_preview.take().is_some() {
            // Commit the previewed value to core exactly once, then drop
            // the overlay so the exact composite replaces it.
            self.discard_slider_preview_patches(cx);
            if let Some(final_update) = pending.or(last) {
                self.apply_core_slider_update_async(final_update, cx);
            }
            self.slider_frame_pending = false;
            self.slider_frame_generation = self.slider_frame_generation.wrapping_add(1);
            return;
        }
        self.slider_frame_pending = false;
        if let Some(update) = pending {
            self.apply_core_slider_update_async(update, cx);
        }
        self.clear_queued_slider_update();
    }

    fn queue_slider_update(
        &mut self,
        update: PendingSliderUpdate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.slider_frame_pending {
            self.pending_slider_update = Some(update);
            return;
        }
        // The first move of a frame applies now so the preview appears on
        // the very next render; later moves that arrive before that render
        // coalesce into one apply through the frame callback.
        self.slider_frame_pending = true;
        let generation = self.slider_frame_generation;
        cx.on_next_frame(window, move |app, _, cx| {
            if app.slider_frame_generation != generation {
                return;
            }
            app.flush_pending_slider_update(cx);
        });
        self.apply_slider_update(update, cx);
    }

    fn flush_pending_slider_update(&mut self, cx: &mut Context<Self>) {
        self.slider_frame_pending = false;
        let Some(update) = self.pending_slider_update.take() else {
            return;
        };
        self.apply_slider_update(update, cx);
    }

    fn clear_queued_slider_update(&mut self) {
        self.pending_slider_update = None;
        self.slider_frame_pending = false;
        self.slider_frame_generation = self.slider_frame_generation.wrapping_add(1);
    }

    fn apply_slider_update(&mut self, update: PendingSliderUpdate, cx: &mut Context<Self>) {
        if self.schedule_slider_preview(update, cx) {
            self.set_status_error(None, cx);
            return;
        }
        self.apply_core_slider_update(update, cx);
    }

    fn apply_core_slider_update(&mut self, update: PendingSliderUpdate, cx: &mut Context<Self>) {
        let (label, result) = match update {
            PendingSliderUpdate::Adjustment { channel, value } => {
                ("Adjust", self.set_selected_adjustment(channel, value, cx))
            }
            PendingSliderUpdate::Opacity { value } => {
                ("Opacity", self.set_selected_layer_opacity(value, cx))
            }
        };
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(format!("{label}: {error}")), cx),
        }
    }

    fn apply_core_slider_update_async(
        &mut self,
        update: PendingSliderUpdate,
        cx: &mut Context<Self>,
    ) {
        let result = match update {
            PendingSliderUpdate::Adjustment { channel, value } => {
                self.apply_adjustment_async(channel, value, cx)
            }
            PendingSliderUpdate::Opacity { value } => self.apply_opacity_async(value, cx),
        };
        let label = match update {
            PendingSliderUpdate::Adjustment { .. } => "Adjust",
            PendingSliderUpdate::Opacity { .. } => "Opacity",
        };
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(format!("{label}: {error}")), cx),
        }
    }

    fn apply_adjustment_async(
        &mut self,
        channel: u8,
        value: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let mut adjustment = layer.basic_adjustment.clone().unwrap_or_default();
        let value = value.clamp(-100.0, 100.0);
        let current = match channel {
            0 => adjustment.brightness,
            1 => adjustment.contrast,
            _ => adjustment.saturation,
        };
        if current == value {
            return Ok(());
        }
        match channel {
            0 => adjustment.brightness = value,
            1 => adjustment.contrast = value,
            _ => adjustment.saturation = value,
        }
        self.apply_slider_core_command(
            &doc_id,
            photrez_core::protocol::Command::SetAdjustment {
                id: layer.id.clone(),
                adjustment: Some(photrez_core::document::content::BasicAdjustment {
                    brightness: adjustment.brightness,
                    contrast: adjustment.contrast,
                    saturation: adjustment.saturation,
                }),
            },
            cx,
        )
    }

    fn apply_opacity_async(&mut self, opacity: f64, cx: &mut Context<Self>) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer(cx)?;
        if layer.locked {
            return Ok(());
        }
        let opacity = opacity.clamp(0.0, 1.0);
        if (layer.opacity - opacity).abs() < 1e-9 {
            return Ok(());
        }
        self.apply_slider_core_command(
            &doc_id,
            photrez_core::protocol::Command::SetOpacity {
                id: layer.id.clone(),
                opacity,
            },
            cx,
        )
    }

    fn apply_slider_core_command(
        &mut self,
        doc_id: &str,
        command: photrez_core::protocol::Command,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.apply_core_command(doc_id, command, false, cx)?;
        self.sessions
            .mark_history_dirty(doc_id)
            .map_err(|error| format!("dirty state failed: {error:?}"))?;
        self.project_layers_with_selection(None, cx);
        self.refresh_active_canvas_async(cx)
    }

    fn preview_snapshot(
        &mut self,
        update: PendingSliderUpdate,
        cx: &mut Context<Self>,
    ) -> Option<PreviewSnapshot> {
        if !self.ensure_slider_preview(update, cx) {
            return None;
        }
        let preview = self.slider_preview.as_ref()?;
        let active_doc_id = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone());
        if active_doc_id.as_deref() != Some(preview.doc_id.as_str()) {
            return None;
        }
        let mut content = preview.content.clone();
        let target_id = preview.layer_id.clone();
        let layer = content
            .layers
            .iter_mut()
            .find(|layer| layer.id == target_id)?;
        match update {
            PendingSliderUpdate::Adjustment { channel, value } => {
                let mut adjustment = layer.basic_adjustment.clone().unwrap_or_default();
                match channel {
                    0 => adjustment.brightness = value.clamp(-100.0, 100.0),
                    1 => adjustment.contrast = value.clamp(-100.0, 100.0),
                    _ => adjustment.saturation = value.clamp(-100.0, 100.0),
                }
                layer.has_adjustments = Some(
                    adjustment.brightness != 0.0
                        || adjustment.contrast != 0.0
                        || adjustment.saturation != 0.0,
                );
                layer.basic_adjustment = Some(adjustment);
            }
            PendingSliderUpdate::Opacity { value } => {
                layer.opacity = value.clamp(0.0, 1.0);
            }
        }
        Some((
            preview.doc_id.clone(),
            preview.layer_id.clone(),
            content,
            preview.layers.clone(),
            preview.doc_width,
            preview.doc_height,
        ))
    }

    fn prepare_slider_preview(
        &mut self,
        update: PendingSliderUpdate,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.ensure_slider_preview(update, cx) {
            return false;
        }
        let preview = match self.slider_preview.as_ref() {
            Some(preview) => preview,
            None => return false,
        };
        let active_doc_id = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone());
        active_doc_id.as_deref() == Some(preview.doc_id.as_str())
    }

    fn schedule_slider_preview(
        &mut self,
        update: PendingSliderUpdate,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.prepare_slider_preview(update, cx) {
            return false;
        }
        self.last_slider_preview_update = Some(update);
        self.slider_preview_next = Some(update);
        self.state.update(cx, |state, _| match update {
            PendingSliderUpdate::Adjustment { channel, value } => match channel {
                0 => state.adjustment_brightness = value,
                1 => state.adjustment_contrast = value,
                _ => state.adjustment_saturation = value,
            },
            PendingSliderUpdate::Opacity { value } => state.layer_opacity = value,
        });
        cx.notify();
        if self.slider_preview_job_active {
            return true;
        }
        let Some((doc_id, layer_id, content, layers, doc_width, doc_height)) =
            self.preview_snapshot(update, cx)
        else {
            return false;
        };
        let _ = (doc_id, layer_id, doc_width, doc_height);
        self.slider_preview_job_active = true;
        self.slider_preview_job_generation = self.slider_preview_job_generation.wrapping_add(1);
        let generation = self.slider_preview_job_generation;
        cx.spawn(async move |this, cx| {
            let composite = cx
                .background_spawn(async move {
                    let input = PreviewInput { layers };
                    photrez_core::render::compositor::composite_layers_with_font_data(
                        &content,
                        &input,
                        Some(crate::INTER),
                    )
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.finish_slider_preview_job(generation, update, composite, cx);
            });
        })
        .detach();
        true
    }

    fn finish_slider_preview_job(
        &mut self,
        generation: u64,
        update: PendingSliderUpdate,
        composite: Result<
            photrez_core::render::compositor::CompositedRgba,
            photrez_core::render::compositor::CompositeError,
        >,
        cx: &mut Context<Self>,
    ) {
        self.slider_preview_job_active = false;
        if generation != self.slider_preview_job_generation || !self.slider_dragging {
            return;
        }
        let next = self.slider_preview_next.take();
        match composite {
            Ok(composite) => {
                self.present_slider_preview_composite(composite, cx);
            }
            Err(_) => {
                self.apply_core_slider_update(update, cx);
            }
        }
        if let Some(next) = next.filter(|next| *next != update) {
            self.schedule_slider_preview(next, cx);
        }
    }

    fn present_slider_preview_composite(
        &mut self,
        composite: photrez_core::render::compositor::CompositedRgba,
        cx: &mut Context<Self>,
    ) {
        let Some(preview) = self.slider_preview.as_ref() else {
            return;
        };
        let doc_width = preview.doc_width;
        let doc_height = preview.doc_height;
        let Some(active) = self.active_document.as_mut() else {
            return;
        };
        let retired = active.canvas.discard_preview_patches();
        let push = active.canvas.push_scaled_patch(
            0,
            0,
            doc_width,
            doc_height,
            composite.width,
            composite.height,
            &composite.pixels,
        );
        if push.is_err() {
            self.retired_textures.extend(retired);
            return;
        }
        self.retired_textures.extend(retired);
        cx.notify();
    }

    /// Build the downscaled runnable preview stack on the first move of a
    /// drag. Returns false when the drag must fall back to the full exact
    /// path: no document or layer, any non-raster layer, or a pixel store
    /// that cannot be read.
    fn ensure_slider_preview(
        &mut self,
        update: PendingSliderUpdate,
        cx: &mut Context<Self>,
    ) -> bool {
        let (doc_id, layer_id) = match update {
            PendingSliderUpdate::Adjustment { .. } => match self.selected_layer_for_cursor(cx) {
                Ok((doc_id, layer)) => (doc_id, layer.id),
                Err(_) => return false,
            },
            PendingSliderUpdate::Opacity { .. } => match self.selected_layer(cx) {
                Ok((doc_id, layer)) => (doc_id, layer.id),
                Err(_) => return false,
            },
        };
        if let Some(preview) = &self.slider_preview {
            if preview.doc_id == doc_id && preview.layer_id == layer_id {
                return true;
            }
        }
        let Some(session) = self.sessions.get(&doc_id) else {
            return false;
        };
        let loaded = session.loaded_document();
        let Some(snapshot) = loaded.content_snapshot() else {
            return false;
        };
        if snapshot.layers.is_empty()
            || snapshot
                .layers
                .iter()
                .any(|layer| layer.layer_type != photrez_core::document::content::LayerType::Raster)
        {
            return false;
        }
        let doc_w = snapshot.width.round().max(1.0) as u32;
        let doc_h = snapshot.height.round().max(1.0) as u32;
        let scale = crate::app::canvas_input::display_scale(
            &self.canvas_bounds,
            (doc_w, doc_h),
            self.active_camera(),
        )
        .clamp(0.01, 1.0);
        let preview_doc_w = ((doc_w as f64 * scale).round().max(1.0)) as u32;
        let preview_doc_h = ((doc_h as f64 * scale).round().max(1.0)) as u32;
        let mut scaled_layers = Vec::with_capacity(snapshot.layers.len());
        let mut scaled_pixels: Vec<(String, u32, u32, Vec<u8>)> =
            Vec::with_capacity(snapshot.layers.len());
        for layer in &snapshot.layers {
            let Some(canonical) = photrez_core::render::RenderInput::layer_pixels(
                &loaded.registry,
                &doc_id,
                &layer.id,
            ) else {
                return false;
            };
            let pw = canonical.width;
            let ph = canonical.height;
            if pw == 0 || ph == 0 {
                if canonical.pixels.is_empty() {
                    // Zero-size raster layers paint nothing; keep them as-is.
                    let entry = (layer.id.clone(), 0, 0, Vec::new());
                    scaled_pixels.push(entry);
                    let mut scaled = layer.clone();
                    scaled.width = 0.0;
                    scaled.height = 0.0;
                    scaled.transform.x *= scale;
                    scaled.transform.y *= scale;
                    scaled_layers.push(scaled);
                    continue;
                }
                return false;
            }
            let expected = (pw as usize)
                .checked_mul(ph as usize)
                .and_then(|count| count.checked_mul(4));
            if expected != Some(canonical.pixels.len()) {
                return false;
            }
            let scaled_pw = ((pw as f64 * scale).round().max(1.0)) as u32;
            let scaled_ph = ((ph as f64 * scale).round().max(1.0)) as u32;
            let bytes = if scaled_pw == pw && scaled_ph == ph {
                canonical.pixels.to_vec()
            } else {
                match crate::input_policy::downsample_premultiplied(
                    canonical.pixels,
                    pw as usize,
                    ph as usize,
                    0,
                    0,
                    pw as usize,
                    ph as usize,
                    scaled_pw as usize,
                    scaled_ph as usize,
                ) {
                    Ok(bytes) => bytes,
                    Err(_) => return false,
                }
            };
            scaled_pixels.push((layer.id.clone(), scaled_pw, scaled_ph, bytes));
            let mut scaled = layer.clone();
            scaled.width = f64::from(scaled_pw);
            scaled.height = f64::from(scaled_ph);
            scaled.transform.x *= scale;
            scaled.transform.y *= scale;
            scaled_layers.push(scaled);
        }
        let mut content = snapshot.clone();
        content.width = f64::from(preview_doc_w);
        content.height = f64::from(preview_doc_h);
        content.layers = scaled_layers;
        self.slider_preview = Some(SliderPreview {
            doc_id,
            layer_id,
            doc_width: doc_w,
            doc_height: doc_h,
            content,
            layers: scaled_pixels,
        });
        true
    }

    fn discard_slider_preview_patches(&mut self, cx: &mut Context<Self>) {
        if let Some(active) = self.active_document.as_mut() {
            let retired = active.canvas.discard_preview_patches();
            self.retired_textures.extend(retired);
        }
        cx.notify();
    }

    /// The keyboard path for a track. Each target reads its own current value
    /// and writes through the setter the pointer path uses.
    pub(crate) fn slider_key(
        &mut self,
        target: SliderTarget,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        match target {
            SliderTarget::Tolerance => {
                let current = self.state.read(cx).fill_tolerance as f64;
                let Some(value) = Self::stepped(current, key, 0.0, 255.0) else {
                    return false;
                };
                let result = self.set_fill_tolerance(value, cx);
                self.report_slider_write("Tolerance", result, cx);
            }
            SliderTarget::TextStrokeWidth => {
                let current = self.effective_text_stroke_width(cx);
                let Some(value) = Self::stepped(current, key, 1.0, 100.0) else {
                    return false;
                };
                let result = self.set_text_stroke_width(value, cx);
                self.report_slider_write("Text", result, cx);
            }
            SliderTarget::BrushMenuSize
            | SliderTarget::BrushMenuHardness
            | SliderTarget::BrushMenuStrength => {
                // The menu's three tracks are 0..=100 positions; Size converts
                // through the same curve the pointer path uses, so a key and a
                // drag of the same position land on the same pixel width.
                let eraser = self.state.read(cx).active_tool == ToolId::Eraser;
                let current = {
                    let state = self.state.read(cx);
                    match target {
                        SliderTarget::BrushMenuSize => paint_size_to_slider(if eraser {
                            state.eraser_size
                        } else {
                            state.brush_size
                        }),
                        SliderTarget::BrushMenuHardness => {
                            100.0
                                * if eraser {
                                    state.eraser_hardness
                                } else {
                                    state.brush_hardness
                                }
                        }
                        _ => {
                            100.0
                                * if eraser {
                                    state.eraser_opacity
                                } else {
                                    state.brush_opacity
                                }
                        }
                    }
                };
                let Some(position) = Self::stepped(current, key, 0.0, 100.0) else {
                    return false;
                };
                self.brush_menu_set_from_fraction(target, position / 100.0, cx);
            }
            SliderTarget::ExportQuality => {
                let current = self.state.read(cx).export_quality as f64;
                let Some(value) = Self::stepped(current, key, 1.0, 100.0) else {
                    return false;
                };
                let quality = value.round() as u8;
                if self.state.read(cx).export_quality == quality {
                    return true;
                }
                self.state
                    .update(cx, |state, _| state.export_quality = quality);
                cx.notify();
            }
            SliderTarget::AdjustmentBrightness
            | SliderTarget::AdjustmentContrast
            | SliderTarget::AdjustmentSaturation => {
                let channel = match target {
                    SliderTarget::AdjustmentBrightness => 0,
                    SliderTarget::AdjustmentContrast => 1,
                    _ => 2,
                };
                let current = {
                    let state = self.state.read(cx);
                    match channel {
                        0 => state.adjustment_brightness,
                        1 => state.adjustment_contrast,
                        _ => state.adjustment_saturation,
                    }
                };
                let Some(value) = Self::stepped(current, key, -100.0, 100.0) else {
                    return false;
                };
                let result = self.set_selected_adjustment(channel, value, cx);
                self.report_slider_write("Adjust", result, cx);
            }
            SliderTarget::PrintScale => {
                let current = self.state.read(cx).print_scale_percent as f64;
                let Some(value) = Self::stepped(current, key, 10.0, 400.0) else {
                    return false;
                };
                let percent = value.round() as u32;
                let state = self.state.read(cx);
                if state.print_scale_percent == percent && !state.print_scale_to_fit {
                    return true;
                }
                self.state.update(cx, |state, _| {
                    state.print_scale_percent = percent;
                    state.print_scale_to_fit = false;
                });
                cx.notify();
            }
            SliderTarget::LayerOpacity => {
                let current = self.state.read(cx).layer_opacity * 100.0;
                let Some(value) = Self::stepped(current, key, 0.0, 100.0) else {
                    return false;
                };
                let result = self.set_selected_layer_opacity(value / 100.0, cx);
                self.report_slider_write("Opacity", result, cx);
            }
        }
        true
    }

    /// One key on a `min..=max` track, in that track's own units: arrows step
    /// one unit and Home/End go to the ends, as a range input does.
    fn stepped(current: f64, key: &str, min: f64, max: f64) -> Option<f64> {
        match key {
            "left" | "down" => Some((current - 1.0).clamp(min, max)),
            "right" | "up" => Some((current + 1.0).clamp(min, max)),
            "home" => Some(min),
            "end" => Some(max),
            _ => None,
        }
    }

    /// A rejected track write becomes a status line; an accepted one clears the
    /// previous warning.
    fn report_slider_write(
        &mut self,
        label: &str,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(format!("{label}: {error}")), cx),
        }
    }

    fn set_slider_from_x(
        &mut self,
        target: SliderTarget,
        x: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = {
            let state = self.state.read(cx);
            match target {
                SliderTarget::Tolerance => state.tolerance_slider_bounds.get(),
                SliderTarget::TextStrokeWidth => state.text_stroke_slider_bounds.get(),
                SliderTarget::BrushMenuSize => state.brush_menu_size_bounds.get(),
                SliderTarget::BrushMenuHardness => state.brush_menu_hardness_bounds.get(),
                SliderTarget::BrushMenuStrength => state.brush_menu_strength_bounds.get(),
                SliderTarget::ExportQuality => state.export_quality_slider_bounds.get(),
                SliderTarget::AdjustmentBrightness => state.adjustment_slider_bounds[0].get(),
                SliderTarget::AdjustmentContrast => state.adjustment_slider_bounds[1].get(),
                SliderTarget::AdjustmentSaturation => state.adjustment_slider_bounds[2].get(),
                SliderTarget::PrintScale => state.print_scale_slider_bounds.get(),
                SliderTarget::LayerOpacity => state.opacity_slider_bounds.get(),
            }
        };
        let Some(bounds) = bounds else {
            return;
        };
        let width = f32::from(bounds.size.width).max(1.0);
        let fraction = ((x - f32::from(bounds.origin.x)) / width).clamp(0.0, 1.0) as f64;
        match target {
            SliderTarget::Tolerance => {
                let result = self.set_fill_tolerance((fraction * 255.0).round(), cx);
                match result {
                    Ok(()) => self.set_status_error(None, cx),
                    Err(error) => self.set_status_error(Some(format!("Tolerance: {error}")), cx),
                }
            }
            SliderTarget::TextStrokeWidth => {
                let result = self.set_text_stroke_width(1.0 + fraction * 99.0, cx);
                match result {
                    Ok(()) => self.set_status_error(None, cx),
                    Err(error) => self.set_status_error(Some(format!("Text: {error}")), cx),
                }
            }
            SliderTarget::BrushMenuSize
            | SliderTarget::BrushMenuHardness
            | SliderTarget::BrushMenuStrength => {
                self.brush_menu_set_from_fraction(target, fraction, cx);
            }
            SliderTarget::ExportQuality => {
                let quality = (1.0 + fraction * 99.0).round().clamp(1.0, 100.0) as u8;
                if self.state.read(cx).export_quality == quality {
                    return;
                }
                self.state
                    .update(cx, |state, _| state.export_quality = quality);
                cx.notify();
            }
            SliderTarget::PrintScale => {
                // The print scale field: min 10, max 400.
                let pct = (10.0 + fraction * 390.0).round().clamp(10.0, 400.0) as u32;
                let state = self.state.read(cx);
                if state.print_scale_percent == pct && !state.print_scale_to_fit {
                    return;
                }
                self.state.update(cx, |state, _| {
                    state.print_scale_percent = pct;
                    state.print_scale_to_fit = false;
                });
                cx.notify();
            }
            SliderTarget::AdjustmentBrightness
            | SliderTarget::AdjustmentContrast
            | SliderTarget::AdjustmentSaturation => {
                // Centre-origin track: 50% is 0, the ends are -100/+100.
                let channel = match target {
                    SliderTarget::AdjustmentBrightness => 0,
                    SliderTarget::AdjustmentContrast => 1,
                    _ => 2,
                };
                let value = (fraction * 200.0 - 100.0).round();
                self.queue_slider_update(
                    PendingSliderUpdate::Adjustment { channel, value },
                    window,
                    cx,
                );
            }
            SliderTarget::LayerOpacity => {
                self.queue_slider_update(
                    PendingSliderUpdate::Opacity { value: fraction },
                    window,
                    cx,
                );
            }
        }
    }
}
