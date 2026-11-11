//! Display refresh policy for core edits: the dirty-region path, the lazy
//! exact-base barrier, and the viewport-resolution projection for large
//! documents. Input handlers call into this module; they do not own the
//! display strategy.

use std::sync::Arc;

use gpui_kit::{AppContext, Context};
use photrez_core::document::content::{LayerType, PtzDocumentContent};
use photrez_core::protocol::{Command, CommandEnvelope, CONTRACT_VERSION};

use super::slider::PreviewInput;
use super::App;
use crate::display::canvas_bridge::{image_from_rgba, Camera, CanvasImage};

pub(super) struct ProjectionCache {
    doc_id: String,
    version: u64,
    content: PtzDocumentContent,
    input: Arc<ProjectionInput>,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ProjectionKey {
    doc_id: String,
    version: u64,
    bounds: (u32, u32),
    camera: (u32, u32, u32),
}

/// One raster layer's source pixels. A cache keeps the same shared buffer
/// while the layer's pixel epoch is unchanged, so metadata edits never copy
/// pixels on the window thread.
struct ProjectionLayer {
    id: String,
    width: u32,
    height: u32,
    epoch: u64,
    pixels: Arc<Vec<u8>>,
}

struct ProjectionInput {
    layers: Vec<ProjectionLayer>,
}

impl photrez_core::render::RenderInput for ProjectionInput {
    fn layer_pixels(
        &self,
        _document_id: &str,
        layer_id: &str,
    ) -> Option<photrez_core::render::RenderLayerPixels<'_>> {
        let layer = self.layers.iter().find(|layer| layer.id == layer_id)?;
        Some(photrez_core::render::RenderLayerPixels {
            width: layer.width,
            height: layer.height,
            pixels: &layer.pixels,
        })
    }
}

/// At this size a full composite and tile rebuild no longer belong on the
/// window thread. Smaller documents keep the synchronous seam so their edits
/// paint in the same frame.
const LARGE_CANVAS_PIXELS: u64 = 2_500_000;

/// The region path composites on the window thread. The measured compositor
/// rate is about 8 ms per megapixel, so 2 MP stays near 16 ms before the tile
/// copy; a larger edit takes the projection path instead.
const MAX_REGION_REFRESH_PIXELS: u64 = 2_000_000;

fn region_refresh_fits(region_area: u64) -> bool {
    region_area <= MAX_REGION_REFRESH_PIXELS
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DocRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl DocRect {
    fn union(self, other: DocRect) -> DocRect {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        DocRect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    }
}

#[derive(Clone, Debug)]
enum EditRefresh {
    None,
    Region { layer_id: String, old: DocRect },
    Full,
}

/// Composite the visible page region at display scale. The layer sources stay
/// full-resolution; only their transforms are scaled, so the compositor
/// samples the source at the output resolution instead of materializing a
/// second full-document buffer.
fn projection_work(
    mut content: photrez_core::document::content::PtzDocumentContent,
    input: Arc<ProjectionInput>,
    viewport: (f32, f32),
    camera: Camera,
) -> Option<(u32, u32, u32, u32, Arc<gpui_kit::RenderImage>)> {
    let doc_w = content.width;
    let doc_h = content.height;
    if !doc_w.is_finite() || !doc_h.is_finite() || doc_w <= 0.0 || doc_h <= 0.0 {
        return None;
    }
    let (page_x, page_y, page_w, page_h) = crate::display::canvas_bridge::camera_placement(
        viewport.0,
        viewport.1,
        doc_w as f32,
        doc_h as f32,
        Some(camera),
    )?;
    if !page_w.is_finite() || page_w <= 0.0 || !page_h.is_finite() || page_h <= 0.0 {
        return None;
    }
    let left = page_x.max(0.0);
    let top = page_y.max(0.0);
    let right = (page_x + page_w).min(viewport.0);
    let bottom = (page_y + page_h).min(viewport.1);
    if right <= left || bottom <= top {
        return None;
    }
    let scale = (f64::from(page_w) / doc_w).clamp(0.01, 1.0);
    let doc_left = ((f64::from(left - page_x) / f64::from(page_w)) * doc_w)
        .floor()
        .max(0.0);
    let doc_top = ((f64::from(top - page_y) / f64::from(page_h)) * doc_h)
        .floor()
        .max(0.0);
    let doc_right = ((f64::from(right - page_x) / f64::from(page_w)) * doc_w)
        .ceil()
        .min(doc_w);
    let doc_bottom = ((f64::from(bottom - page_y) / f64::from(page_h)) * doc_h)
        .ceil()
        .min(doc_h);
    let doc_left = (doc_left - 2.0).max(0.0);
    let doc_top = (doc_top - 2.0).max(0.0);
    let doc_right = (doc_right + 2.0).min(doc_w);
    let doc_bottom = (doc_bottom + 2.0).min(doc_h);
    if doc_right <= doc_left || doc_bottom <= doc_top {
        return None;
    }

    let scaled_w = (doc_w * scale).max(1.0);
    let scaled_h = (doc_h * scale).max(1.0);
    let region = photrez_core::render::compositor::CompositeRegion {
        x: (doc_left * scale).floor().clamp(0.0, scaled_w - 1.0) as u32,
        y: (doc_top * scale).floor().clamp(0.0, scaled_h - 1.0) as u32,
        width: ((doc_right * scale).ceil().clamp(0.0, scaled_w) as u32)
            .saturating_sub((doc_left * scale).floor().clamp(0.0, scaled_w) as u32)
            .max(1),
        height: ((doc_bottom * scale).ceil().clamp(0.0, scaled_h) as u32)
            .saturating_sub((doc_top * scale).floor().clamp(0.0, scaled_h) as u32)
            .max(1),
    };
    content.width = scaled_w;
    content.height = scaled_h;
    for layer in &mut content.layers {
        layer.transform.x *= scale;
        layer.transform.y *= scale;
        layer.transform.scale_x *= scale;
        layer.transform.scale_y *= scale;
    }
    let output = photrez_core::render::compositor::composite_layers_in_region_with_font_data(
        &content,
        &*input,
        Some(crate::INTER),
        region,
    )
    .ok()?;
    let image = image_from_rgba(output.region.width, output.region.height, output.pixels).ok()?;
    let rect_x = (f64::from(output.region.x) / scale).floor().max(0.0) as u32;
    let rect_y = (f64::from(output.region.y) / scale).floor().max(0.0) as u32;
    let rect_right =
        ((f64::from(output.region.x + output.region.width) / scale).ceil()).min(doc_w) as u32;
    let rect_bottom =
        ((f64::from(output.region.y + output.region.height) / scale).ceil()).min(doc_h) as u32;
    Some((
        rect_x,
        rect_y,
        rect_right.saturating_sub(rect_x),
        rect_bottom.saturating_sub(rect_y),
        image,
    ))
}

impl App {
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
        let edit_refresh = self.command_edit_refresh(doc_id, &command);
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
                .mark_history_dirty(doc_id)
                .map_err(|error| format!("dirty state failed: {error:?}"))?;
            self.project_layers_with_selection(added_layer.as_deref(), cx);
            match edit_refresh {
                EditRefresh::None => {
                    self.project_history(cx);
                    self.project_tabs(cx);
                    cx.notify();
                }
                EditRefresh::Region { layer_id, old } => {
                    if !self.refresh_region_after_edit(doc_id, &layer_id, old, cx) {
                        self.refresh_after_edit(cx)?;
                    }
                }
                EditRefresh::Full => self.refresh_after_edit(cx)?,
            }
        }
        Ok(())
    }

    /// Classify a command by the display work it needs. Layer-local metadata
    /// edits recomposite only the union of the old and new layer bounds.
    fn command_edit_refresh(&self, doc_id: &str, command: &Command) -> EditRefresh {
        let id = match command {
            Command::SetLocked { .. }
            | Command::Rename { .. }
            | Command::SetBackgroundFlag { .. } => return EditRefresh::None,
            Command::SetVisible { id, .. }
            | Command::SetOpacity { id, .. }
            | Command::SetBlendMode { id, .. }
            | Command::SetLayerParams { id, .. }
            | Command::SetAdjustment { id, .. }
            | Command::TransformLayer { id, .. } => id,
            _ => return EditRefresh::Full,
        };
        let Some(old) = self.layer_doc_rect(doc_id, id) else {
            return EditRefresh::Full;
        };
        EditRefresh::Region {
            layer_id: id.clone(),
            old,
        }
    }

    fn layer_doc_rect(&self, doc_id: &str, layer_id: &str) -> Option<DocRect> {
        let content = self
            .sessions
            .get(doc_id)?
            .loaded_document()
            .content_snapshot()?;
        let layer = content.layers.iter().find(|layer| layer.id == layer_id)?;
        let (width, height) = crate::app::transform_geometry::layer_frame_size(layer);
        let (x, y, width, height) =
            crate::app::transform_geometry::layer_aabb(&layer.transform, width, height);
        (x.is_finite()
            && y.is_finite()
            && width.is_finite()
            && height.is_finite()
            && width > 0.0
            && height > 0.0)
            .then_some(DocRect {
                x,
                y,
                width,
                height,
            })
    }

    /// Recomposite only the changed layer bounds and replace the intersecting
    /// display tiles. Returns `false` when the region is too large or cannot be
    /// patched, so the caller can fall back to the full refresh seam.
    fn refresh_region_after_edit(
        &mut self,
        doc_id: &str,
        layer_id: &str,
        old: DocRect,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .active_document
            .as_ref()
            .is_some_and(|active| active.canvas_stale)
        {
            return false;
        }
        let Some(session) = self.sessions.get(doc_id) else {
            return false;
        };
        let loaded = session.loaded_document();
        let Some(content) = loaded.content_snapshot() else {
            return false;
        };
        let new = self.layer_doc_rect(doc_id, layer_id);
        let union = match new {
            Some(new) => old.union(new),
            None => old,
        };
        let (doc_w, doc_h) = (content.width as u32, content.height as u32);
        let left = (union.x.floor() - 2.0).clamp(0.0, f64::from(doc_w)) as u32;
        let top = (union.y.floor() - 2.0).clamp(0.0, f64::from(doc_h)) as u32;
        let right = ((union.x + union.width).ceil() + 2.0).clamp(0.0, f64::from(doc_w)) as u32;
        let bottom = ((union.y + union.height).ceil() + 2.0).clamp(0.0, f64::from(doc_h)) as u32;
        if right <= left || bottom <= top {
            self.project_history(cx);
            self.project_tabs(cx);
            cx.notify();
            return true;
        }
        let region = photrez_core::render::compositor::CompositeRegion {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        };
        let region_area = u64::from(region.width) * u64::from(region.height);
        if !region_refresh_fits(region_area) {
            return false;
        }
        let Ok(composite) =
            photrez_core::render::compositor::composite_layers_in_region_with_font_data(
                content,
                &loaded.registry,
                Some(crate::INTER),
                region,
            )
        else {
            return false;
        };
        let Some(active) = self
            .active_document
            .as_mut()
            .filter(|active| active.doc_id == doc_id)
        else {
            return false;
        };
        let retired = active.canvas.discard_preview_patches();
        self.retired_textures.extend(retired);
        let patched = active.canvas.update_region(
            composite.region.x,
            composite.region.y,
            composite.region.width,
            composite.region.height,
            &composite.pixels,
        );
        match patched {
            Ok(retired) => {
                self.retired_textures.extend(retired);
                #[cfg(test)]
                self.region_refreshes.set(self.region_refreshes.get() + 1);
                self.project_history(cx);
                self.project_tabs(cx);
                cx.notify();
                true
            }
            Err(_) => false,
        }
    }

    /// Full refresh for a normal edit. Large documents keep the base tiles stale
    /// and paint a viewport-resolution projection instead; the exact base is
    /// rebuilt only when a reader asks for it.
    pub(crate) fn refresh_after_edit(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let (large, dims_match) = self
            .active_document
            .as_ref()
            .map(|document| {
                let (width, height) = document.canvas.dimensions();
                let core_dims = self
                    .sessions
                    .get(&document.doc_id)
                    .and_then(|session| session.loaded_document().content_snapshot())
                    .map(|content| (content.width as u32, content.height as u32));
                (
                    u64::from(width) * u64::from(height) >= LARGE_CANVAS_PIXELS,
                    core_dims == Some((width, height)),
                )
            })
            .unwrap_or((false, true));
        if large && dims_match {
            if let Some(active) = self.active_document.as_mut() {
                active.canvas_stale = true;
                if let Some(retired) = active.canvas.clear_projection() {
                    self.retired_textures.push(retired);
                }
            }
            self.projection_key = None;
            self.project_history(cx);
            self.project_tabs(cx);
            cx.notify();
            self.maybe_schedule_viewport_projection(cx);
            Ok(())
        } else if large {
            self.refresh_active_canvas_async(cx)
        } else {
            self.refresh_active_canvas(cx)
        }
    }

    /// Build or reuse the full-resolution source cache for a projection.
    /// Buffers are keyed by layer epoch; an edit that leaves a layer's pixels
    /// untouched reuses the shared buffer instead of copying it.
    fn projection_cache_for(&mut self, doc_id: &str, version: u64) -> Option<Arc<ProjectionCache>> {
        if let Some(cache) = &self.projection_cache {
            if cache.doc_id == doc_id && cache.version == version {
                return Some(Arc::clone(cache));
            }
        }
        let session = self.sessions.get(doc_id)?;
        let loaded = session.loaded_document();
        let content = loaded.content_snapshot()?.clone();
        let previous = self
            .projection_cache
            .as_ref()
            .filter(|cache| cache.doc_id == doc_id);
        let mut layers = Vec::new();
        for layer in content
            .layers
            .iter()
            .filter(|layer| layer.layer_type == LayerType::Raster)
        {
            let canonical = loaded.registry.get_layer(doc_id, &layer.id)?;
            let epoch = loaded.registry.get_epoch(doc_id, &layer.id).ok()?;
            let reused = previous
                .and_then(|cache| {
                    cache
                        .input
                        .layers
                        .iter()
                        .find(|cached| cached.id == layer.id)
                })
                .filter(|cached| {
                    cached.epoch == epoch
                        && cached.width == canonical.width
                        && cached.height == canonical.height
                });
            let pixels = match reused {
                Some(cached) => Arc::clone(&cached.pixels),
                None => {
                    #[cfg(test)]
                    self.projection_pixel_copies
                        .set(self.projection_pixel_copies.get() + 1);
                    Arc::new(canonical.pixels.clone())
                }
            };
            layers.push(ProjectionLayer {
                id: layer.id.clone(),
                width: canonical.width,
                height: canonical.height,
                epoch,
                pixels,
            });
        }
        let cache = Arc::new(ProjectionCache {
            doc_id: doc_id.to_string(),
            version,
            content,
            input: Arc::new(ProjectionInput { layers }),
        });
        self.projection_cache = Some(Arc::clone(&cache));
        Some(cache)
    }

    pub(super) fn invalidate_projection_for_document_replacement(&mut self, doc_id: &str) {
        self.projection_generation = self.projection_generation.wrapping_add(1);
        self.projection_key = None;
        if self
            .projection_cache
            .as_ref()
            .is_some_and(|cache| cache.doc_id == doc_id)
        {
            self.projection_cache = None;
        }
    }

    /// Queue one viewport-resolution composite for the current camera and
    /// document version. The full-resolution base stays untouched.
    pub(crate) fn maybe_schedule_viewport_projection(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active_document.as_ref() else {
            return;
        };
        if !active.canvas_stale {
            return;
        }
        let Some(bounds) = self.canvas_bounds.get() else {
            return;
        };
        let Some(camera) = self.effective_camera() else {
            return;
        };
        let doc_id = active.doc_id.clone();
        let Some(version) = self.sessions.get(&doc_id).and_then(|session| {
            session
                .loaded_document()
                .registry
                .get_history_version(&doc_id)
        }) else {
            return;
        };
        let width: f32 = bounds.size.width.into();
        let height: f32 = bounds.size.height.into();
        let key = ProjectionKey {
            doc_id: doc_id.clone(),
            version,
            bounds: (width.ceil() as u32, height.ceil() as u32),
            camera: (
                camera.x.to_bits(),
                camera.y.to_bits(),
                camera.zoom.to_bits(),
            ),
        };
        if self.projection_key.as_ref() == Some(&key) {
            return;
        }
        self.projection_key = Some(key.clone());
        self.projection_generation = self.projection_generation.wrapping_add(1);
        let generation = self.projection_generation;
        let Some(cache) = self.projection_cache_for(&doc_id, version) else {
            return;
        };
        let input = Arc::clone(&cache.input);
        let content = cache.content.clone();
        let viewport = (width, height);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { projection_work(content, input, viewport, camera) })
                .await;
            let _ = this.update(cx, |app, cx| {
                if generation != app.projection_generation
                    || app.projection_key.as_ref() != Some(&key)
                {
                    return;
                }
                let Some(active) = app
                    .active_document
                    .as_mut()
                    .filter(|active| active.doc_id == doc_id)
                else {
                    return;
                };
                if let Some((x, y, width, height, image)) = result {
                    if let Some(retired) = active.canvas.set_projection(x, y, width, height, image)
                    {
                        app.retired_textures.push(retired);
                    }
                    cx.notify();
                } else {
                    let _ = app.refresh_active_canvas_async(cx);
                }
            });
        })
        .detach();
    }

    /// Bring the exact base tiles up to date. Projection-only edits leave the
    /// base stale; tools that read its pixels call this first.
    pub(crate) fn ensure_canvas_current(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if self
            .active_document
            .as_ref()
            .is_some_and(|active| active.canvas_stale)
        {
            self.refresh_active_canvas(cx)?;
        }
        Ok(())
    }

    /// Recompose the active document after a core mutation.
    pub(crate) fn refresh_active_canvas(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        self.canvas_refresh_generation = self.canvas_refresh_generation.wrapping_add(1);
        self.canvas_refresh_active = false;
        self.projection_generation = self.projection_generation.wrapping_add(1);
        self.projection_key = None;
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
            active.canvas_stale = false;
        }
        self.project_history(cx);
        self.project_tabs(cx);
        cx.notify();
        Ok(())
    }

    /// Composite the active document on a background executor, then swap the
    /// canvas on the foreground. Stale completions are ignored when the
    /// document or history version moves.
    pub(crate) fn refresh_active_canvas_async(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc_id = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
            .ok_or_else(|| "no active document".to_string())?;
        let (content, pixels, version) = {
            let session = self
                .sessions
                .get(&doc_id)
                .ok_or_else(|| "active session is missing".to_string())?;
            let loaded = session.loaded_document();
            let content = loaded
                .content_snapshot()
                .ok_or_else(|| "loaded content is missing".to_string())?
                .clone();
            let version = loaded
                .registry
                .get_history_version(&doc_id)
                .ok_or_else(|| "history version is missing".to_string())?;
            let mut pixels = Vec::with_capacity(content.layers.len());
            for layer in content
                .layers
                .iter()
                .filter(|layer| layer.layer_type == LayerType::Raster)
            {
                let canonical = loaded
                    .registry
                    .get_layer(&doc_id, &layer.id)
                    .ok_or_else(|| "loaded layer pixels are missing".to_string())?;
                pixels.push((
                    layer.id.clone(),
                    canonical.width,
                    canonical.height,
                    canonical.pixels.to_vec(),
                ));
            }
            (content, pixels, version)
        };
        self.canvas_refresh_generation = self.canvas_refresh_generation.wrapping_add(1);
        self.canvas_refresh_active = true;
        let generation = self.canvas_refresh_generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let input = PreviewInput { layers: pixels };
                    let composite =
                        photrez_core::render::compositor::composite_layers_with_font_data(
                            &content,
                            &input,
                            Some(crate::INTER),
                        )
                        .map_err(|error| error.to_string())?;
                    CanvasImage::from_rgba(composite.width, composite.height, composite.pixels)
                        .map_err(|error| format!("{error:?}"))
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.finish_canvas_refresh_async(&doc_id, version, generation, result, cx);
            });
        })
        .detach();
        Ok(())
    }

    fn finish_canvas_refresh_async(
        &mut self,
        doc_id: &str,
        version: u64,
        generation: u64,
        result: Result<CanvasImage, String>,
        cx: &mut Context<Self>,
    ) {
        self.canvas_refresh_active = false;
        if generation != self.canvas_refresh_generation {
            return;
        }
        let active = match self.active_document.as_mut() {
            Some(active) if active.doc_id == doc_id => active,
            _ => return,
        };
        if self.sessions.get(doc_id).and_then(|session| {
            session
                .loaded_document()
                .registry
                .get_history_version(doc_id)
        }) != Some(version)
        {
            return;
        }
        let canvas = match result {
            Ok(canvas) => canvas,
            Err(error) => {
                self.set_status_error(Some(format!("Canvas refresh: {error}")), cx);
                return;
            }
        };
        self.projection_generation = self.projection_generation.wrapping_add(1);
        self.projection_key = None;
        #[cfg(test)]
        self.full_composites.set(self.full_composites.get() + 1);
        let retired = std::mem::replace(&mut active.canvas, canvas);
        self.retired_textures.extend(retired.into_textures());
        active.canvas_stale = false;
        self.project_history(cx);
        self.project_tabs(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Half of a 6 MP page is 3 MP on the window thread. The old page-share
    /// rule allowed it; the pixel budget rejects it.
    #[test]
    fn region_refresh_budget_rejects_large_window_thread_composites() {
        assert!(region_refresh_fits(2_000_000));
        assert!(!region_refresh_fits(2_000_001));
        assert!(!region_refresh_fits(3_000_000), "half a 6 MP page");
        assert!(
            region_refresh_fits(120_000),
            "a small document stays inline"
        );
    }
}
