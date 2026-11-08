//! The brush engine under a gesture: the Shift axis lock, beginning and
//! committing a stroke, and the preview patches that ride on top of it.

use super::*;

impl App {
    /// Shift rule for the brush: once the pointer has moved 5 screen px,
    /// lock the stroke to the dominant document axis from the start point.
    pub(super) fn brush_axis_point(
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

    pub(super) fn clear_brush_axis(&mut self) {
        self.brush_axis_lock = None;
        self.brush_stroke_start = None;
        self.brush_stroke_start_screen = None;
    }

    pub(super) fn begin_brush(
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
        let paint_background = eraser && target.is_background;
        let (size, hardness, color, opacity, flow, smoothing) = {
            let state = self.state.read(cx);
            if eraser {
                let color = if paint_background {
                    state.bg_color
                } else {
                    state.fg_color
                };
                (
                    state.eraser_size,
                    state.eraser_hardness,
                    color,
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
        let stroke =
            brush::begin_stroke(target, x, y, size, eraser && !paint_background, alpha, tip);
        if !self.input.begin_brush(stroke) {
            self.set_status_error(Some("Brush: a gesture is already active".into()), cx);
            false
        } else {
            true
        }
    }

    pub(super) fn commit_brush(&mut self, cx: &mut Context<Self>) {
        self.clear_brush_axis();
        self.alt_sampling = false;
        let Some(stroke) = self.input.finish_brush() else {
            return;
        };
        let crate::input_policy::BrushStroke {
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
            if let Err(error) = self.sessions.mark_history_dirty(&doc_id) {
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
                    crate::app::measurement::RefreshOutcome::Succeeded
                } else {
                    crate::app::measurement::RefreshOutcome::Failed
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

    pub(super) fn push_brush_preview(&mut self, cx: &mut Context<Self>) -> Result<bool, String> {
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
        let Some(rect) =
            crate::input_policy::preview_rect(&stroke.dabs, &stroke.tip, width, height)?
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
        let patches = crate::input_policy::scaled_preview_patches(
            crate::input_policy::PreviewBase::region(&base, rect, width, height),
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

    pub(super) fn display_after_brush(
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
}
