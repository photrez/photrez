//! Live transform preview: the damaged document region a drag paints over,
//! recomposited with the dragged transform(s) and pushed onto the canvas.
//! Nothing here writes canonical content or history.

use std::sync::Arc;

use gpui_kit::Context;
use photrez_core::document::content::{LayerType, PtzDocumentContent, Transform2D};
use photrez_core::render::compositor::{
    CompositeError, CompositeRegion, CompositedRegionRgba, CompositedRgba,
};

use super::slider::PreviewInput;
use super::transform_move::{GroupMember, LayerBounds};
use super::App;

enum TransformPreviewComposite {
    Region(CompositedRegionRgba),
    Full(CompositedRgba),
}

fn layer_damage_bounds(
    previous: &Transform2D,
    next: &Transform2D,
    width: f64,
    height: f64,
) -> Option<LayerBounds> {
    let (old_x, old_y, old_w, old_h) =
        crate::app::transform_geometry::layer_aabb(previous, width, height);
    let (new_x, new_y, new_w, new_h) =
        crate::app::transform_geometry::layer_aabb(next, width, height);
    let left = old_x.min(new_x);
    let top = old_y.min(new_y);
    let right = (old_x + old_w).max(new_x + new_w);
    let bottom = (old_y + old_h).max(new_y + new_h);
    if ![left, top, right, bottom].into_iter().all(f64::is_finite) || right <= left || bottom <= top
    {
        return None;
    }
    Some((left, top, right - left, bottom - top))
}

fn merge_damage_bounds(bounds: &mut Option<LayerBounds>, next: LayerBounds) {
    *bounds = Some(match *bounds {
        Some((x, y, width, height)) => {
            let left = x.min(next.0);
            let top = y.min(next.1);
            let right = (x + width).max(next.0 + next.2);
            let bottom = (y + height).max(next.1 + next.3);
            (left, top, right - left, bottom - top)
        }
        None => next,
    });
}

fn damage_region(bounds: LayerBounds) -> Option<photrez_core::render::compositor::CompositeRegion> {
    let (x, y, width, height) = bounds;
    let right = x + width;
    let bottom = y + height;
    if ![x, y, right, bottom].into_iter().all(f64::is_finite) {
        return None;
    }
    let x = (x.floor() - 1.0).max(0.0) as u32;
    let y = (y.floor() - 1.0).max(0.0) as u32;
    let right = (right.ceil() + 1.0).max(f64::from(x)) as u32;
    let bottom = (bottom.ceil() + 1.0).max(f64::from(y)) as u32;
    Some(photrez_core::render::compositor::CompositeRegion {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    })
}

impl App {
    /// Show the single-layer preview at display scale when possible, falling
    /// back to a full-resolution local composite otherwise.
    pub(super) fn preview_transform_single(
        &mut self,
        layer_id: &str,
        base_transform: &Transform2D,
        transform: &Transform2D,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, content, region) =
            self.preview_plan_single(layer_id, base_transform, transform)?;
        if self.install_transform_preview(&doc_id, content.clone(), region, cx) {
            return Ok(());
        }
        let loaded = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document();
        let composite =
            composite_preview(&content, &loaded.registry, region).map_err(|e| e.to_string())?;
        self.install_preview_composite(composite, cx)
    }

    /// Show the multi-layer preview at display scale when possible, falling
    /// back to a full-resolution local composite otherwise.
    pub(super) fn preview_transform_group(
        &mut self,
        starts: &[GroupMember],
        previews: &[(String, Transform2D)],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, content, region) = self.preview_plan_group(starts, previews)?;
        if self.install_transform_preview(&doc_id, content.clone(), region, cx) {
            return Ok(());
        }
        let loaded = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document();
        let composite =
            composite_preview(&content, &loaded.registry, region).map_err(|e| e.to_string())?;
        self.install_preview_composite(composite, cx)
    }

    fn install_preview_composite(
        &mut self,
        composite: TransformPreviewComposite,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match composite {
            TransformPreviewComposite::Region(composite) => {
                if self.display_region_composite(composite, cx) {
                    return Ok(());
                }
                Err("preview region could not be installed".to_string())
            }
            TransformPreviewComposite::Full(composite) => {
                self.display_composite(composite.width, composite.height, composite.pixels, cx)
            }
        }
    }

    fn preview_plan_single(
        &self,
        layer_id: &str,
        base_transform: &Transform2D,
        transform: &Transform2D,
    ) -> Result<(String, PtzDocumentContent, Option<CompositeRegion>), String> {
        let doc_id = self.active_doc_id()?;
        let loaded = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document();
        let mut content = loaded
            .content_snapshot()
            .ok_or_else(|| "loaded content is missing".to_string())?
            .clone();
        let layer = content
            .layers
            .iter_mut()
            .find(|layer| layer.id == layer_id)
            .ok_or_else(|| "the transformed layer is missing".to_string())?;
        let damage = matches!(layer.layer_type, LayerType::Raster)
            .then(|| layer_damage_bounds(base_transform, transform, layer.width, layer.height))
            .flatten();
        layer.transform = transform.clone();
        let region = damage.and_then(damage_region);
        Ok((doc_id, content, region))
    }

    fn preview_plan_group(
        &self,
        starts: &[GroupMember],
        previews: &[(String, Transform2D)],
    ) -> Result<(String, PtzDocumentContent, Option<CompositeRegion>), String> {
        let doc_id = self.active_doc_id()?;
        let loaded = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document();
        let mut content = loaded
            .content_snapshot()
            .ok_or_else(|| "loaded content is missing".to_string())?
            .clone();
        let mut damage = None;
        let mut can_update_region = starts.len() == previews.len();
        for (index, (id, transform)) in previews.iter().enumerate() {
            let Some(layer) = content.layers.iter_mut().find(|layer| &layer.id == id) else {
                can_update_region = false;
                continue;
            };
            let base_transform = match starts.get(index) {
                Some(member) if &member.id == id => &member.start,
                _ => {
                    can_update_region = false;
                    &layer.transform
                }
            };
            if matches!(layer.layer_type, LayerType::Raster) {
                if let Some(bounds) =
                    layer_damage_bounds(base_transform, transform, layer.width, layer.height)
                {
                    merge_damage_bounds(&mut damage, bounds);
                } else {
                    can_update_region = false;
                }
            } else {
                can_update_region = false;
            }
            layer.transform = transform.clone();
        }
        let region = if can_update_region {
            damage.and_then(damage_region)
        } else {
            None
        };
        Ok((doc_id, content, region))
    }

    /// Build (once per drag) the screen-resolution raster buffers so each
    /// per-move composite runs at display resolution instead of full document
    /// resolution and keeps up with the pointer. Returns false when the
    /// document has a non-raster layer (the caller then uses the synchronous
    /// path, which renders shapes and text from their parameters).
    fn ensure_transform_preview_cache(&mut self, doc_id: &str) -> bool {
        if let Some((cached_id, _, _)) = &self.transform_preview_cache {
            if cached_id == doc_id {
                return true;
            }
        }
        let Some(session) = self.sessions.get(doc_id) else {
            return false;
        };
        let loaded = session.loaded_document();
        let Some(snapshot) = loaded.content_snapshot() else {
            return false;
        };
        if snapshot
            .layers
            .iter()
            .any(|layer| layer.layer_type != LayerType::Raster)
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
        let mut layers = Vec::with_capacity(snapshot.layers.len());
        for layer in &snapshot.layers {
            let Some(canonical) = photrez_core::render::RenderInput::layer_pixels(
                &loaded.registry,
                doc_id,
                &layer.id,
            ) else {
                return false;
            };
            let pw = canonical.width;
            let ph = canonical.height;
            if pw == 0 || ph == 0 {
                if canonical.pixels.is_empty() {
                    layers.push((layer.id.clone(), 0, 0, Vec::new()));
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
            layers.push((layer.id.clone(), scaled_pw, scaled_ph, bytes));
        }
        self.transform_preview_cache =
            Some((doc_id.to_string(), scale, Arc::new(PreviewInput { layers })));
        true
    }

    /// Paint the preview for one drag frame at display scale. `content` and
    /// `region` are document-space; the cached display scale shrinks both, so
    /// the composite cost follows the viewport instead of the document. The
    /// result is painted into the same document-space rectangle, so the frame
    /// the user sees and the pixels always come from the same pointer sample.
    /// Returns false when the screen-resolution stack cannot be built and the
    /// caller must composite at full resolution.
    pub(super) fn install_transform_preview(
        &mut self,
        doc_id: &str,
        content: PtzDocumentContent,
        region: Option<CompositeRegion>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(region) = region else {
            return false;
        };
        if !self.ensure_transform_preview_cache(doc_id) {
            return false;
        }
        let (scale, input) = match &self.transform_preview_cache {
            Some((_, scale, input)) => (*scale, Arc::clone(input)),
            None => return false,
        };
        // The damage rectangle can run past the document when a layer is
        // dragged off the edge. Clip in document space so the scaled image and
        // its destination rectangle describe the same area.
        let Some((canvas_w, canvas_h)) = self
            .active_document
            .as_ref()
            .map(|active| active.canvas.dimensions())
        else {
            return false;
        };
        let right = (region.x + region.width).min(canvas_w);
        let bottom = (region.y + region.height).min(canvas_h);
        let dest = CompositeRegion {
            x: region.x.min(canvas_w),
            y: region.y.min(canvas_h),
            width: right.saturating_sub(region.x.min(canvas_w)),
            height: bottom.saturating_sub(region.y.min(canvas_h)),
        };
        if dest.width == 0 || dest.height == 0 {
            return true;
        }
        let mut scaled_content = content;
        scale_content(&mut scaled_content, scale);
        let scaled_region = scale_region(dest, scale);
        let composite = match composite_preview(&scaled_content, &*input, Some(scaled_region)) {
            Ok(TransformPreviewComposite::Region(composite)) => composite,
            _ => return false,
        };
        // A compositor-clipped region no longer matches `dest`; refuse rather
        // than stretch one area over another.
        if composite.region != scaled_region {
            return false;
        }
        self.display_scaled_preview_patch(dest, &composite, cx)
    }

    /// Drop the cached preview buffers. The drag lifecycle calls this so the
    /// next drag re-reads pixels and display scale from the current document.
    pub(super) fn invalidate_transform_preview(&mut self) {
        self.transform_preview_cache = None;
    }

    fn display_region_composite(
        &mut self,
        composite: photrez_core::render::compositor::CompositedRegionRgba,
        cx: &mut Context<Self>,
    ) -> bool {
        let width = composite.document_width;
        let height = composite.document_height;
        let region = composite.region;
        let Some(active) = self.active_document.as_mut() else {
            return false;
        };
        let tile_size = active.canvas.tile_size().max(1);
        let expected_tiles =
            width.div_ceil(tile_size) as usize * height.div_ceil(tile_size) as usize;
        if active.canvas.dimensions() != (width, height)
            || active.canvas.tile_count() != expected_tiles
        {
            return false;
        }
        if region.width == 0 || region.height == 0 {
            return true;
        }
        let Some(expected_bytes) = (region.width as usize)
            .checked_mul(region.height as usize)
            .and_then(|length| length.checked_mul(4))
        else {
            return false;
        };
        if composite.pixels.len() != expected_bytes {
            return false;
        }
        let Ok(retired) = active.canvas.replace_preview_patch(
            region.x,
            region.y,
            region.width,
            region.height,
            &composite.pixels,
        ) else {
            return false;
        };
        self.retired_textures.extend(retired);
        cx.notify();
        true
    }

    /// Push a screen-resolution preview image into its document-space
    /// destination rectangle, replacing earlier preview overlays. The canvas
    /// itself is never resized here: the display maps document rectangles to
    /// the page, so a resized canvas would move every layer.
    fn display_scaled_preview_patch(
        &mut self,
        dest: CompositeRegion,
        composite: &CompositedRegionRgba,
        cx: &mut Context<Self>,
    ) -> bool {
        if dest.width == 0 || dest.height == 0 {
            return true;
        }
        let image_w = composite.region.width;
        let image_h = composite.region.height;
        if image_w == 0 || image_h == 0 {
            return true;
        }
        let expected = (image_w as usize)
            .checked_mul(image_h as usize)
            .and_then(|length| length.checked_mul(4));
        if expected != Some(composite.pixels.len()) {
            return false;
        }
        let Some(active) = self.active_document.as_mut() else {
            return false;
        };
        let (canvas_w, canvas_h) = active.canvas.dimensions();
        if dest.x + dest.width > canvas_w || dest.y + dest.height > canvas_h {
            return false;
        }
        let retired = active.canvas.discard_preview_patches();
        let pushed = active.canvas.push_scaled_patch(
            dest.x,
            dest.y,
            dest.width,
            dest.height,
            image_w,
            image_h,
            &composite.pixels,
        );
        self.retired_textures.extend(retired);
        if pushed.is_err() {
            return false;
        }
        cx.notify();
        true
    }

    fn display_composite(
        &mut self,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let canvas = crate::display::canvas_bridge::CanvasImage::from_rgba(width, height, pixels)
            .map_err(|error| format!("{error:?}"))?;
        if let Some(active) = self.active_document.as_mut() {
            let retired = std::mem::replace(&mut active.canvas, canvas);
            self.retired_textures.extend(retired.into_textures());
        }
        cx.notify();
        Ok(())
    }
}

/// Composite the override-applied content: the damaged region when one is
/// known, otherwise the whole document. Pure, so the caller can reuse it for
/// the exact fallback composite.
fn composite_preview(
    content: &PtzDocumentContent,
    input: &dyn photrez_core::render::RenderInput,
    region: Option<CompositeRegion>,
) -> Result<TransformPreviewComposite, CompositeError> {
    if let Some(region) = region {
        let composite =
            photrez_core::render::compositor::composite_layers_in_region_with_font_data(
                content,
                input,
                Some(crate::INTER),
                region,
            )?;
        return Ok(TransformPreviewComposite::Region(composite));
    }
    let composite = photrez_core::render::compositor::composite_layers_with_font_data(
        content,
        input,
        Some(crate::INTER),
    )?;
    Ok(TransformPreviewComposite::Full(composite))
}

/// Scale the document geometry into the preview coordinate space: document
/// size and layer origins scale, while per-layer scale factors stay put
/// because layer `width`/`height` already carry the scaled pixel size.
fn scale_content(content: &mut PtzDocumentContent, scale: f64) {
    if (scale - 1.0).abs() < f64::EPSILON {
        return;
    }
    content.width *= scale;
    content.height *= scale;
    for layer in &mut content.layers {
        layer.width *= scale;
        layer.height *= scale;
        layer.transform.x *= scale;
        layer.transform.y *= scale;
    }
}

fn scale_region(region: CompositeRegion, scale: f64) -> CompositeRegion {
    if (scale - 1.0).abs() < f64::EPSILON {
        return region;
    }
    let x = (f64::from(region.x) * scale).floor().max(0.0) as u32;
    let y = (f64::from(region.y) * scale).floor().max(0.0) as u32;
    let right = (f64::from(region.x + region.width) * scale).ceil() as u32;
    let bottom = (f64::from(region.y + region.height) * scale).ceil() as u32;
    CompositeRegion {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}
