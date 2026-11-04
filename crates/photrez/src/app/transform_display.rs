//! Live transform preview: the damaged document region a drag paints over,
//! recomposited with the dragged transform(s) and pushed onto the canvas.
//! Nothing here writes canonical content or history.

use gpui_kit::Context;
use photrez_core::ptz::content::{LayerType, Transform2D};

use super::transform_move::{GroupMember, LayerBounds};
use super::App;

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
    pub(super) fn display_with_transforms(
        &mut self,
        starts: &[GroupMember],
        previews: &[(String, Transform2D)],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc_id = self.active_doc_id()?;
        let (content, region_composite) = {
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
            let region_composite = if can_update_region {
                damage.and_then(damage_region).and_then(|region| {
                    photrez_core::render::compositor::composite_layers_in_region_with_font_data(
                        &content,
                        &loaded.registry,
                        Some(crate::INTER),
                        region,
                    )
                    .ok()
                })
            } else {
                None
            };
            (content, region_composite)
        };
        if let Some(composite) = region_composite {
            if self.display_region_composite(composite, cx) {
                return Ok(());
            }
        }
        let loaded = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document();
        let composite = photrez_core::render::compositor::composite_layers_with_font_data(
            &content,
            &loaded.registry,
            Some(crate::INTER),
        )
        .map_err(|error| error.to_string())?;
        self.display_composite(composite.width, composite.height, composite.pixels, cx)
    }

    /// Composite the document with one layer's transform overridden for the
    /// live drag; nothing is written to canonical content or history.
    pub(super) fn display_with_transform(
        &mut self,
        layer_id: &str,
        base_transform: &Transform2D,
        transform: &Transform2D,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc_id = self.active_doc_id()?;
        let (content, region_composite) = {
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
            let region_composite = damage.and_then(damage_region).and_then(|region| {
                photrez_core::render::compositor::composite_layers_in_region_with_font_data(
                    &content,
                    &loaded.registry,
                    Some(crate::INTER),
                    region,
                )
                .ok()
            });
            (content, region_composite)
        };
        if let Some(composite) = region_composite {
            if self.display_region_composite(composite, cx) {
                return Ok(());
            }
        }
        let loaded = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document();
        let composite = photrez_core::render::compositor::composite_layers_with_font_data(
            &content,
            &loaded.registry,
            Some(crate::INTER),
        )
        .map_err(|error| error.to_string())?;
        self.display_composite(composite.width, composite.height, composite.pixels, cx)
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

    fn display_composite(
        &mut self,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let canvas = crate::render::canvas_bridge::CanvasImage::from_rgba(width, height, pixels)
            .map_err(|error| format!("{error:?}"))?;
        if let Some(active) = self.active_document.as_mut() {
            let retired = std::mem::replace(&mut active.canvas, canvas);
            self.retired_textures.extend(retired.into_textures());
        }
        cx.notify();
        Ok(())
    }
}
