//! Display refresh policy for core edits: the dirty-region path, the lazy
//! exact-base barrier, and the viewport-resolution projection for large
//! documents. Input handlers call into this module; they do not own the
//! display strategy.

use std::sync::Arc;

use gpui_kit::{AppContext, Context};
use photrez_core::document::content::{LayerType, PtzDocumentContent};
use photrez_core::protocol::{CONTRACT_VERSION, Command, CommandEnvelope};

use super::App;
use super::slider::PreviewInput;
use crate::display::canvas_bridge::{Camera, CanvasImage, image_from_rgba};

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

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CropPreviewKey {
    doc_id: String,
    version: u64,
    cache_generation: u64,
    viewport: (f32, f32),
    window_scale: u32,
    camera: (u32, u32, u32),
    frame: Option<[u64; 4]>,
    offset: [u64; 2],
    rotation: u64,
    target_size: Option<(u32, u32)>,
}

#[derive(Clone, Copy)]
struct CropPreviewTransform {
    frame: Option<[f64; 4]>,
    offset: (f64, f64),
    rotation: f64,
    target_size: Option<(f64, f64)>,
}

impl From<crate::crop_geometry::CropSnapshot> for CropPreviewTransform {
    fn from(crop: crate::crop_geometry::CropSnapshot) -> Self {
        Self {
            frame: Some(crop.frame),
            offset: crop.offset,
            rotation: crop.rotation,
            target_size: None,
        }
    }
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

fn source_raster_scale(scale: f64) -> f64 {
    scale.clamp(0.01, 1.0)
}

fn crop_preview_raster_scale(width: u32, height: u32, zoom: f64) -> f64 {
    if u64::from(width) * u64::from(height) >= LARGE_CANVAS_PIXELS {
        source_raster_scale(zoom)
    } else {
        1.0
    }
}

fn crop_preview_pixels(
    content: &PtzDocumentContent,
    input: &ProjectionInput,
    viewport: (f32, f32),
    window_scale: f32,
    camera: Camera,
    crop: impl Into<CropPreviewTransform>,
) -> Option<(u32, u32, Vec<u8>)> {
    let CropPreviewTransform {
        frame,
        offset,
        rotation,
        target_size,
    } = crop.into();
    if !window_scale.is_finite() || window_scale <= 0.0 {
        return None;
    }
    let scale = f64::from(window_scale);
    let logical_width = f64::from(viewport.0);
    let logical_height = f64::from(viewport.1);
    let (width, height) = (
        viewport_edge((logical_width * scale) as f32)?,
        viewport_edge((logical_height * scale) as f32)?,
    );
    let zoom = f64::from(camera.zoom);
    if !zoom.is_finite()
        || zoom <= 0.0
        || frame.is_some_and(|frame| {
            !frame.into_iter().all(f64::is_finite) || frame[2] <= 0.0 || frame[3] <= 0.0
        })
        || !offset.0.is_finite()
        || !offset.1.is_finite()
        || !rotation.is_finite()
        || target_size.is_some_and(|(width, height)| {
            !width.is_finite()
                || !height.is_finite()
                || width <= 0.0
                || height <= 0.0
                || width > f64::from(crate::display::canvas_bridge::MAX_CANVAS_DIM)
                || height > f64::from(crate::display::canvas_bridge::MAX_CANVAS_DIM)
        })
    {
        return None;
    }

    let document_width = content.width;
    let document_height = content.height;
    if !document_width.is_finite()
        || !document_height.is_finite()
        || document_width <= 0.0
        || document_height <= 0.0
    {
        return None;
    }
    let document_width_edge = scaled_document_edge(document_width, 1.0)?;
    let document_height_edge = scaled_document_edge(document_height, 1.0)?;
    let round_screen = |coordinate: f64| (coordinate + 0.5).floor();
    let rotation = if frame.is_some() { rotation } else { 0.0 };
    let crop_transform = if let Some(frame) = frame {
        let rect = crate::crop_geometry::crop_apply_region(
            crate::crop_geometry::CropInteraction::Modern,
            frame,
            offset,
            crate::crop_geometry::CropViewport {
                pan_x: f64::from(camera.x),
                pan_y: f64::from(camera.y),
                zoom,
            },
        );
        if !rect.into_iter().all(f64::is_finite) {
            return None;
        }
        let width = rect[2].round().max(1.0);
        let height = rect[3].round().max(1.0);
        let output_size = target_size.unwrap_or((width, height));
        Some(photrez_core::engine::CropTransform {
            rect: [rect[0].round(), rect[1].round(), width, height],
            rotation: crate::crop_geometry::modern_apply_rotation(rotation),
            output_size,
        })
    } else {
        None
    };
    let pivot = frame.map(|[frame_x, frame_y, frame_width, frame_height]| {
        let frame_screen = (
            round_screen(frame_x * zoom + f64::from(camera.x)),
            round_screen(frame_y * zoom + f64::from(camera.y)),
            round_screen(frame_width * zoom),
            round_screen(frame_height * zoom),
        );
        let pivot_screen = (
            frame_screen.0 + frame_screen.2 / 2.0,
            frame_screen.1 + frame_screen.3 / 2.0,
        );
        let pivot_document = (
            (pivot_screen.0 - f64::from(camera.x) - offset.0) / zoom,
            (pivot_screen.1 - f64::from(camera.y) - offset.1) / zoom,
        );
        (pivot_screen, pivot_document)
    });
    let radians = rotation.to_radians();
    let (sin, cos) = radians.sin_cos();
    let inverse = |x: f64, y: f64| match pivot {
        Some((pivot_screen, pivot_document)) => {
            let (dx, dy) = (x - pivot_screen.0, y - pivot_screen.1);
            (
                pivot_document.0 + (dx * cos + dy * sin) / zoom,
                pivot_document.1 + (-dx * sin + dy * cos) / zoom,
            )
        }
        None => (
            (x - f64::from(camera.x) - offset.0) / zoom,
            (y - f64::from(camera.y) - offset.1) / zoom,
        ),
    };
    let corners = [
        inverse(0.0, 0.0),
        inverse(logical_width, 0.0),
        inverse(0.0, logical_height),
        inverse(logical_width, logical_height),
    ];
    if !corners.iter().all(|(x, y)| x.is_finite() && y.is_finite()) {
        return None;
    }
    let min_x = corners
        .iter()
        .map(|point| point.0)
        .fold(f64::INFINITY, f64::min);
    let min_y = corners
        .iter()
        .map(|point| point.1)
        .fold(f64::INFINITY, f64::min);
    let max_x = corners
        .iter()
        .map(|point| point.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = corners
        .iter()
        .map(|point| point.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let (source, crop_sampler) = if let Some(crop_transform) = crop_transform {
        let crop_output_width = scaled_document_edge(crop_transform.output_size.0, 1.0)?;
        let crop_output_height = scaled_document_edge(crop_transform.output_size.1, 1.0)?;
        let raster_scale = crop_preview_raster_scale(crop_output_width, crop_output_height, zoom);
        if !raster_scale.is_finite() || raster_scale <= 0.0 {
            return None;
        }
        let crop_raster_width = scaled_document_edge(crop_transform.output_size.0, raster_scale)?;
        let crop_raster_height = scaled_document_edge(crop_transform.output_size.1, raster_scale)?;
        let output_corners = corners.map(|point| {
            let point = crop_transform.map_point(point);
            (point.0 * raster_scale, point.1 * raster_scale)
        });
        if !output_corners
            .iter()
            .all(|(x, y)| x.is_finite() && y.is_finite())
        {
            return None;
        }
        let output_min_x = output_corners
            .iter()
            .map(|point| point.0)
            .fold(f64::INFINITY, f64::min);
        let output_min_y = output_corners
            .iter()
            .map(|point| point.1)
            .fold(f64::INFINITY, f64::min);
        let output_max_x = output_corners
            .iter()
            .map(|point| point.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let output_max_y = output_corners
            .iter()
            .map(|point| point.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let origin_x = (output_min_x - 1.0).floor();
        let origin_y = (output_min_y - 1.0).floor();
        let region_width = viewport_edge(((output_max_x + 1.0).ceil() - origin_x) as f32)?;
        let region_height = viewport_edge(((output_max_y + 1.0).ceil() - origin_y) as f32)?;

        let mut transformed_content = content.clone();
        transformed_content.width = f64::from(region_width);
        transformed_content.height = f64::from(region_height);
        for layer in &mut transformed_content.layers {
            if !layer.locked {
                layer.transform = photrez_core::engine::crop_layer_transform(
                    &layer.transform,
                    (layer.width, layer.height),
                    crop_transform,
                );
            }
            layer.transform.x = layer.transform.x * raster_scale - origin_x;
            layer.transform.y = layer.transform.y * raster_scale - origin_y;
            layer.transform.scale_x *= raster_scale;
            layer.transform.scale_y *= raster_scale;
        }
        let source = photrez_core::render::compositor::composite_layers_in_region_with_font_data(
            &transformed_content,
            input,
            Some(crate::INTER),
            photrez_core::render::compositor::CompositeRegion {
                x: 0,
                y: 0,
                width: region_width,
                height: region_height,
            },
        )
        .ok()?;
        (
            source,
            Some((
                crop_transform,
                raster_scale,
                origin_x,
                origin_y,
                crop_raster_width,
                crop_raster_height,
            )),
        )
    } else {
        let region_start =
            |min: f64, edge: u32| ((min - 1.0).floor() as i64).clamp(0, i64::from(edge)) as u32;
        let region_end =
            |max: f64, edge: u32| ((max + 1.0).ceil() as i64).clamp(0, i64::from(edge)) as u32;
        let x = region_start(min_x, document_width_edge);
        let y = region_start(min_y, document_height_edge);
        let right = region_end(max_x, document_width_edge);
        let bottom = region_end(max_y, document_height_edge);
        if right <= x || bottom <= y {
            return Some((width, height, vec![0; rgba_byte_len(width, height)?]));
        }
        let source = photrez_core::render::compositor::composite_layers_in_region_with_font_data(
            content,
            input,
            Some(crate::INTER),
            photrez_core::render::compositor::CompositeRegion {
                x,
                y,
                width: right - x,
                height: bottom - y,
            },
        )
        .ok()?;
        (source, None)
    };
    let mut pixels = vec![0; rgba_byte_len(width, height)?];
    for y in 0..height {
        for x in 0..width {
            let screen_x = (f64::from(x) + 0.5) / scale;
            let screen_y = (f64::from(y) + 0.5) / scale;
            let (document_x, document_y) = inverse(screen_x, screen_y);
            if crop_sampler.is_none()
                && (document_x < 0.0
                    || document_y < 0.0
                    || document_x >= document_width
                    || document_y >= document_height)
            {
                continue;
            }
            let (sample_x, sample_y) =
                if let Some((crop, raster_scale, origin_x, origin_y, crop_width, crop_height)) =
                    crop_sampler
                {
                    let point = crop.map_point((document_x, document_y));
                    let (mut sample_x, mut sample_y) =
                        (point.0 * raster_scale, point.1 * raster_scale);
                    if point.0 >= 0.0
                        && point.1 >= 0.0
                        && point.0 < crop.output_size.0
                        && point.1 < crop.output_size.1
                    {
                        sample_x = (sample_x - 0.5).clamp(0.0, f64::from(crop_width - 1)) + 0.5;
                        sample_y = (sample_y - 0.5).clamp(0.0, f64::from(crop_height - 1)) + 0.5;
                    }
                    (sample_x - origin_x, sample_y - origin_y)
                } else {
                    (
                        document_x - f64::from(source.region.x),
                        document_y - f64::from(source.region.y),
                    )
                };
            let sample = sample_rgba_bilinear(
                &source.pixels,
                source.region.width,
                source.region.height,
                sample_x,
                sample_y,
            );
            let index = (y as usize * width as usize + x as usize) * 4;
            pixels[index..index + 4].copy_from_slice(&sample);
        }
    }
    Some((width, height, pixels))
}

fn viewport_edge(value: f32) -> Option<u32> {
    (value.is_finite()
        && value > 0.0
        && f64::from(value.ceil()) <= f64::from(crate::display::canvas_bridge::MAX_CANVAS_DIM))
    .then(|| value.ceil() as u32)
}

fn scaled_document_edge(value: f64, scale: f64) -> Option<u32> {
    let scaled = value * scale;
    (scaled.is_finite() && scaled > 0.0 && scaled.ceil() <= f64::from(u32::MAX))
        .then(|| scaled.ceil() as u32)
}

fn rgba_byte_len(width: u32, height: u32) -> Option<usize> {
    (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)
}

fn sample_rgba_bilinear(source: &[u8], width: u32, height: u32, u: f64, v: f64) -> [u8; 4] {
    let x = (u - 0.5).clamp(0.0, f64::from(width - 1));
    let y = (v - 0.5).clamp(0.0, f64::from(height - 1));
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let fx = x - f64::from(x0);
    let fy = y - f64::from(y0);
    let mut channels = [0.0; 4];
    for (sample_x, sample_y, weight) in [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x1, y0, fx * (1.0 - fy)),
        (x0, y1, (1.0 - fx) * fy),
        (x1, y1, fx * fy),
    ] {
        if weight <= 0.0 {
            continue;
        }
        let index = (sample_y as usize * width as usize + sample_x as usize) * 4;
        let alpha = f64::from(source[index + 3]);
        channels[0] += weight * f64::from(source[index]) * alpha;
        channels[1] += weight * f64::from(source[index + 1]) * alpha;
        channels[2] += weight * f64::from(source[index + 2]) * alpha;
        channels[3] += weight * alpha;
    }
    if channels[3] <= 0.0 {
        return [0; 4];
    }
    [
        (channels[0] / channels[3]).round().clamp(0.0, 255.0) as u8,
        (channels[1] / channels[3]).round().clamp(0.0, 255.0) as u8,
        (channels[2] / channels[3]).round().clamp(0.0, 255.0) as u8,
        channels[3].round().clamp(0.0, 255.0) as u8,
    ]
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
    let scale = source_raster_scale(f64::from(page_w) / doc_w);
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

    let scaled_w = f64::from(scaled_document_edge(doc_w, scale)?);
    let scaled_h = f64::from(scaled_document_edge(doc_h, scale)?);
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
    fn crop_preview_key(&self, cx: &Context<Self>) -> Option<CropPreviewKey> {
        let state = self.state.read(cx);
        if state.active_tool != crate::state::ToolId::Crop
            || state.crop_interaction != crate::crop_geometry::CropInteraction::Modern
        {
            return None;
        }
        let creating_frame = self
            .pending_gesture
            .as_ref()
            .is_some_and(|gesture| gesture.crop_modern_create && gesture.crop_modern_drag_started);
        let frame = if creating_frame {
            None
        } else {
            state.crop_rect
        };
        let rotation = if frame.is_some() {
            state.crop_rotation
        } else {
            0.0
        };
        let target_size =
            if frame.is_some() && state.crop_mode == crate::crop_aspect::CropMode::Size {
                state.crop_size_target
            } else {
                None
            };
        if state.crop_offset == (0.0, 0.0) && rotation == 0.0 && target_size.is_none() {
            return None;
        }
        let document = self.active_document.as_ref()?;
        let version = self
            .sessions
            .get(&document.doc_id)?
            .loaded_document()
            .registry
            .get_history_version(&document.doc_id)?;
        let bounds = self.canvas_bounds.get()?;
        let viewport = (bounds.size.width.into(), bounds.size.height.into());
        viewport_edge(viewport.0)?;
        viewport_edge(viewport.1)?;
        let window_scale = self.window_scale;
        if !window_scale.is_finite() || window_scale <= 0.0 {
            return None;
        }
        let camera = self.effective_camera()?;
        Some(CropPreviewKey {
            doc_id: document.doc_id.clone(),
            version,
            cache_generation: self.crop_preview_cache_generation,
            viewport,
            window_scale: window_scale.to_bits(),
            camera: (
                camera.x.to_bits(),
                camera.y.to_bits(),
                camera.zoom.to_bits(),
            ),
            frame: frame.map(|frame| frame.map(f64::to_bits)),
            offset: [state.crop_offset.0.to_bits(), state.crop_offset.1.to_bits()],
            rotation: rotation.to_bits(),
            target_size,
        })
    }

    pub(super) fn crop_preview_active_for_overlay(&self, cx: &Context<Self>) -> bool {
        let Some(key) = self.crop_preview_key(cx) else {
            return false;
        };
        self.crop_preview_displayed_key.as_ref() == Some(&key)
            && self
                .active_document
                .as_ref()
                .is_some_and(|document| document.canvas.has_crop_preview())
    }

    pub(crate) fn maybe_schedule_crop_preview(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.crop_preview_key(cx) else {
            self.crop_preview_displayed_key = None;
            self.crop_preview_failed = None;
            if let Some(retired) = self
                .active_document
                .as_mut()
                .and_then(|document| document.canvas.clear_crop_preview())
            {
                self.retired_textures.push(retired);
            }
            return;
        };
        if self
            .crop_preview_displayed_key
            .as_ref()
            .is_some_and(|current| current == &key)
            || self.crop_preview_running.is_some()
            || self.crop_preview_failed.as_ref() == Some(&key)
        {
            return;
        }
        let Some(cache) = self.projection_cache_for(&key.doc_id, key.version) else {
            self.crop_preview_failed = Some(key);
            return;
        };
        let content = cache.content.clone();
        let input = Arc::clone(&cache.input);
        self.crop_preview_running = Some(key.clone());
        #[cfg(test)]
        let test_gate = self.crop_preview_test_gate.take();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    #[cfg(test)]
                    if let Some(test_gate) = test_gate {
                        let _ = test_gate.await;
                    }
                    let camera = Camera {
                        x: f32::from_bits(key.camera.0),
                        y: f32::from_bits(key.camera.1),
                        zoom: f32::from_bits(key.camera.2),
                    };
                    crop_preview_pixels(
                        &content,
                        &input,
                        key.viewport,
                        f32::from_bits(key.window_scale),
                        camera,
                        CropPreviewTransform {
                            frame: key.frame.map(|frame| frame.map(f64::from_bits)),
                            offset: (f64::from_bits(key.offset[0]), f64::from_bits(key.offset[1])),
                            rotation: f64::from_bits(key.rotation),
                            target_size: key
                                .target_size
                                .map(|(width, height)| (f64::from(width), f64::from(height))),
                        },
                    )
                    .and_then(|(width, height, pixels)| image_from_rgba(width, height, pixels).ok())
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.crop_preview_running.as_ref() != Some(&key) {
                    return;
                }
                app.crop_preview_running = None;
                if app.crop_preview_key(cx).as_ref() == Some(&key) {
                    if let Some(image) = result {
                        let retired = app
                            .active_document
                            .as_mut()
                            .filter(|document| document.doc_id == key.doc_id)
                            .and_then(|document| document.canvas.set_crop_preview(image));
                        if app.active_document.as_ref().is_some_and(|document| {
                            document.doc_id == key.doc_id && document.canvas.has_crop_preview()
                        }) {
                            app.crop_preview_displayed_key = Some(key.clone());
                        }
                        if let Some(retired) = retired {
                            app.retired_textures.push(retired);
                        }
                        app.crop_preview_failed = None;
                    } else {
                        app.crop_preview_failed = Some(key);
                    }
                }
                app.maybe_schedule_crop_preview(cx);
                cx.notify();
            });
        })
        .detach();
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
        if let Some(cache) = self
            .projection_cache
            .as_ref()
            .filter(|cache| cache.doc_id == doc_id && cache.version == version)
        {
            return Some(Arc::clone(cache));
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
        self.crop_preview_cache_generation = self.crop_preview_cache_generation.wrapping_add(1);
        self.crop_preview_displayed_key = None;
        self.crop_preview_failed = None;
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

    fn crop_preview_fixture() -> (PtzDocumentContent, ProjectionInput) {
        use photrez_core::document::content::{BlendMode, LayerType, PtzLayer, Transform2D};

        let pixels: Vec<u8> = (1..=9).flat_map(|red| [red, 0, 0, 255]).collect();
        let layer = PtzLayer {
            id: "preview".to_owned(),
            name: "Preview".to_owned(),
            layer_type: LayerType::Raster,
            visible: true,
            opacity: 1.0,
            locked: false,
            is_background: None,
            lock_transparency: None,
            lock_position: None,
            lock_rotation: None,
            has_adjustments: None,
            basic_adjustment: None,
            resource_id: None,
            blend_mode: BlendMode::Normal,
            transform: Transform2D {
                x: 0.0,
                y: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
            },
            width: 3.0,
            height: 3.0,
            shape_params: None,
            text_data: None,
        };
        let content = PtzDocumentContent {
            id: "crop-preview".to_owned(),
            name: "Crop preview".to_owned(),
            width: 3.0,
            height: 3.0,
            layers: vec![layer],
            selection: None,
        };
        let input = ProjectionInput {
            layers: vec![ProjectionLayer {
                id: "preview".to_owned(),
                width: 3,
                height: 3,
                epoch: 1,
                pixels: Arc::new(pixels),
            }],
        };
        (content, input)
    }

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

    #[test]
    fn crop_preview_source_scale_switches_at_large_output_threshold() {
        assert_eq!(crop_preview_raster_scale(2499, 1000, 0.5), 1.0);
        assert_eq!(crop_preview_raster_scale(2500, 1000, 0.5), 0.5);
        assert_eq!(crop_preview_raster_scale(2501, 1000, 0.5), 0.5);
        assert_eq!(crop_preview_raster_scale(2500, 1000, 1.5), 1.0);
    }

    #[test]
    fn source_raster_scale_clamps_to_camera_zoom_limits() {
        let below_minimum = Camera::new(0.0, 0.0, 0.005).expect("valid camera");
        assert_eq!(below_minimum.zoom, 0.01);
        assert_eq!(source_raster_scale(0.005), 0.01);
        assert_eq!(source_raster_scale(f64::from(below_minimum.zoom)), 0.01);
        assert_eq!(source_raster_scale(0.0099), 0.01);
        assert_eq!(source_raster_scale(0.01), 0.01);
        assert_eq!(source_raster_scale(0.0101), 0.0101);
        assert_eq!(source_raster_scale(1.01), 1.0);
    }

    #[test]
    fn viewport_projection_uses_the_minimum_source_scale() {
        let (mut content, input) = crop_preview_fixture();
        content.width = 4096.0;
        content.height = 4096.0;
        let camera =
            Camera::centred_at(800.0, 600.0, 4096.0, 4096.0, 0.01).expect("minimum zoom camera");
        let (_, _, _, _, image) = projection_work(content, Arc::new(input), (800.0, 600.0), camera)
            .expect("minimum-scale projection");
        let size = image.size(0);
        assert_eq!((size.width.0 as u32, size.height.0 as u32), (41, 41));
    }

    #[test]
    fn crop_preview_transforms_the_composited_pixels_in_viewport_space() {
        let (content, input) = crop_preview_fixture();
        let camera = Camera::new(0.0, 0.0, 1.0).expect("valid camera");
        let rotated = crop_preview_pixels(
            &content,
            &input,
            (3.0, 3.0),
            1.0,
            camera,
            crate::crop_geometry::CropSnapshot {
                frame: [0.0, 0.0, 3.0, 3.0],
                offset: (0.0, 0.0),
                rotation: 90.0,
            },
        )
        .expect("rotation preview");
        assert_eq!(
            &rotated.2[..4],
            &[7, 0, 0, 255],
            "positive rotation moves the bottom-left source pixel to the top-left"
        );

        let moved = crop_preview_pixels(
            &content,
            &input,
            (3.0, 3.0),
            1.0,
            camera,
            crate::crop_geometry::CropSnapshot {
                frame: [0.0, 0.0, 3.0, 3.0],
                offset: (1.0, 0.0),
                rotation: 0.0,
            },
        )
        .expect("offset preview");
        assert_eq!(&moved.2[..4], &[0, 0, 0, 0]);
        assert_eq!(&moved.2[4..8], &[1, 0, 0, 255]);
        assert_eq!(&moved.2[8..12], &[2, 0, 0, 255]);

        let moved_past_right_edge = crop_preview_pixels(
            &content,
            &input,
            (3.0, 3.0),
            1.0,
            camera,
            crate::crop_geometry::CropSnapshot {
                frame: [0.0, 0.0, 3.0, 3.0],
                offset: (-1.0, 0.0),
                rotation: 0.0,
            },
        )
        .expect("right-edge clipping preview");
        assert_eq!(&moved_past_right_edge.2[..4], &[2, 0, 0, 255]);
        assert_eq!(&moved_past_right_edge.2[4..8], &[3, 0, 0, 255]);
        assert_eq!(&moved_past_right_edge.2[8..12], &[0, 0, 0, 0]);
    }

    #[test]
    fn crop_preview_without_a_frame_keeps_the_offset_and_ignores_rotation() {
        let (content, input) = crop_preview_fixture();
        let camera = Camera::new(0.0, 0.0, 1.0).expect("valid camera");
        let preview = |rotation| {
            crop_preview_pixels(
                &content,
                &input,
                (3.0, 3.0),
                1.0,
                camera,
                CropPreviewTransform {
                    frame: None,
                    offset: (1.0, 0.0),
                    rotation,
                    target_size: None,
                },
            )
            .expect("no-frame preview")
        };
        let rotated = preview(37.0);
        assert_eq!(rotated, preview(0.0));
        assert_eq!(&rotated.2[..4], &[0, 0, 0, 0]);
        assert_eq!(&rotated.2[4..8], &[1, 0, 0, 255]);
        assert_eq!(&rotated.2[8..12], &[2, 0, 0, 255]);
    }

    #[test]
    fn crop_preview_composites_transformed_layers_before_viewport_sampling() {
        let (mut content, input) = crop_preview_fixture();
        content.layers[0].transform = photrez_core::document::content::Transform2D {
            x: 0.25,
            y: -0.5,
            scale_x: 1.25,
            scale_y: 0.75,
            rotation: 27.0,
            flip_h: true,
            flip_v: false,
        };
        let composited =
            photrez_core::render::compositor::composite_layers_in_region_with_font_data(
                &content,
                &input,
                Some(crate::INTER),
                photrez_core::render::compositor::CompositeRegion {
                    x: 0,
                    y: 0,
                    width: 3,
                    height: 3,
                },
            )
            .expect("transformed composite");
        let preview = crop_preview_pixels(
            &content,
            &input,
            (3.0, 3.0),
            1.0,
            Camera::new(0.0, 0.0, 1.0).expect("valid camera"),
            crate::crop_geometry::CropSnapshot {
                frame: [0.0, 0.0, 3.0, 3.0],
                offset: (0.0, 0.0),
                rotation: 0.0,
            },
        )
        .expect("transformed-layer preview");
        assert_eq!(preview.2, composited.pixels);
        assert_ne!(preview.2.as_slice(), input.layers[0].pixels.as_slice());
    }

    fn assert_rotated_offset_multilayer_crop_preview_matches_the_executed_apply_output(
        rotation: f64,
        zoom: f64,
        window_scale: f32,
        target_size: Option<(f64, f64)>,
        transformed_locked_layers: bool,
        document_size: Option<(f64, f64)>,
    ) {
        assert_rotated_offset_multilayer_crop_preview_matches_the_executed_apply_output_with_fill(
            rotation,
            zoom,
            window_scale,
            target_size,
            transformed_locked_layers,
            document_size,
            None,
        );
    }

    fn assert_rotated_offset_multilayer_crop_preview_matches_the_executed_apply_output_with_fill(
        rotation: f64,
        zoom: f64,
        window_scale: f32,
        target_size: Option<(f64, f64)>,
        transformed_locked_layers: bool,
        document_size: Option<(f64, f64)>,
        fill_background: Option<[u8; 4]>,
    ) {
        use photrez_core::document::content::{LayerType, Transform2D};
        use photrez_core::pixel::store::PixelStoreRegistry;

        let (mut content, mut input) = crop_preview_fixture();
        if let Some((width, height)) = document_size {
            content.width = width;
            content.height = height;
        }
        let mut upper = content.layers[0].clone();
        upper.id = "preview-upper".to_owned();
        upper.name = "Preview upper".to_owned();
        upper.opacity = 0.75;
        upper.blend_mode = photrez_core::document::content::BlendMode::Multiply;
        upper.basic_adjustment = Some(photrez_core::document::layer_params::BasicAdjustment {
            brightness: 8.0,
            contrast: -5.0,
            saturation: 15.0,
        });
        upper.has_adjustments = Some(true);
        upper.transform.x = -1.0;
        upper.transform.y = 1.0;
        let (upper_scale_x, upper_scale_y, upper_rotation, upper_flip_h, upper_flip_v) =
            if transformed_locked_layers {
                (0.75, 1.25, 17.0, true, false)
            } else {
                (1.0, 1.0, 0.0, false, false)
            };
        upper.transform.scale_x = upper_scale_x;
        upper.transform.scale_y = upper_scale_y;
        upper.transform.rotation = upper_rotation;
        upper.transform.flip_h = upper_flip_h;
        upper.transform.flip_v = upper_flip_v;
        content.layers.insert(0, upper);
        let mut upper_pixels = vec![0; 3 * 3 * 4];
        upper_pixels[4..8].copy_from_slice(&[10, 220, 30, 128]);
        upper_pixels[16..20].copy_from_slice(&[240, 20, 180, 128]);
        upper_pixels[20..24].copy_from_slice(&[30, 60, 240, 255]);
        input.layers.push(ProjectionLayer {
            id: "preview-upper".to_owned(),
            width: 3,
            height: 3,
            epoch: 2,
            pixels: Arc::new(upper_pixels),
        });
        if transformed_locked_layers {
            let mut locked = content.layers[0].clone();
            locked.id = "preview-locked".to_owned();
            locked.name = "Preview locked".to_owned();
            locked.locked = true;
            locked.opacity = 1.0;
            locked.blend_mode = photrez_core::document::content::BlendMode::Normal;
            locked.basic_adjustment = None;
            locked.has_adjustments = Some(false);
            locked.transform.x = 1.0;
            locked.transform.y = -1.0;
            locked.transform.scale_x = 0.8;
            locked.transform.scale_y = 1.1;
            locked.transform.rotation = -13.0;
            locked.transform.flip_h = false;
            locked.transform.flip_v = true;
            content.layers.insert(0, locked);
            let locked_pixels: Vec<u8> = (0..9_u8)
                .flat_map(|index| [20 + index * 17, 240, 180, 255])
                .collect();
            input.layers.push(ProjectionLayer {
                id: "preview-locked".to_owned(),
                width: 3,
                height: 3,
                epoch: 3,
                pixels: Arc::new(locked_pixels),
            });
        }
        let frame = [0.0, 0.0, 3.0, 3.0];
        let offset = (1.0, 0.0);
        let viewport = if zoom == 1.0 { (3.0, 3.0) } else { (8.0, 6.0) };
        let camera = Camera::centred_at(viewport.0, viewport.1, 3.0, 3.0, zoom as f32)
            .expect("valid camera");
        let preview = crop_preview_pixels(
            &content,
            &input,
            viewport,
            window_scale,
            camera,
            CropPreviewTransform {
                frame: Some(frame),
                offset,
                rotation,
                target_size,
            },
        )
        .expect("rotated crop preview");
        let region = crate::crop_geometry::crop_apply_region(
            crate::crop_geometry::CropInteraction::Modern,
            frame,
            offset,
            crate::crop_geometry::CropViewport {
                pan_x: 0.0,
                pan_y: 0.0,
                zoom,
            },
        );

        let mut pixel_store = PixelStoreRegistry::new();
        pixel_store.open_document(&content.id);
        for layer in &input.layers {
            pixel_store
                .add_layer(
                    &content.id,
                    &layer.id,
                    layer.width,
                    layer.height,
                    layer.pixels.as_ref().clone(),
                )
                .expect("seed raster pixels");
        }
        pixel_store
            .seed_document(&content.id, content.clone())
            .expect("seed document content");
        pixel_store
            .apply_command(
                &content.id,
                CommandEnvelope {
                    contract_version: CONTRACT_VERSION,
                    expected_version: None,
                    command: Command::ApplyCrop {
                        x: region[0].round(),
                        y: region[1].round(),
                        width: region[2].round(),
                        height: region[3].round(),
                        rotation: Some(crate::crop_geometry::modern_apply_rotation(rotation)),
                        target_width: target_size.map(|size| size.0),
                        target_height: target_size.map(|size| size.1),
                        delete_cropped_pixels: true,
                        fill_background,
                    },
                },
            )
            .expect("pixel-store ApplyCrop command");
        let applied = pixel_store
            .document_content(&content.id)
            .expect("applied document content");
        let output_size = target_size.unwrap_or((3.0, 3.0));
        assert_eq!((applied.width, applied.height), output_size);
        assert_eq!(
            applied.layers.len(),
            (if transformed_locked_layers { 3 } else { 2 })
                + usize::from(fill_background.is_some())
        );
        for layer in applied
            .layers
            .iter()
            .filter(|layer| layer.layer_type == LayerType::Raster && !layer.locked)
        {
            assert_eq!(
                layer.transform,
                Transform2D {
                    x: 0.0,
                    y: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    flip_h: false,
                    flip_v: false,
                },
                "unlocked raster layers use identity transforms after baking"
            );
            assert_eq!((layer.width, layer.height), output_size);
            assert_eq!(layer.basic_adjustment, None);
            assert_ne!(
                layer.has_adjustments,
                Some(true),
                "baked raster must not keep an adjustment: {}",
                layer.id
            );
        }
        let upper = applied
            .layers
            .iter()
            .find(|layer| layer.id == "preview-upper")
            .expect("upper raster layer");
        assert_eq!(upper.opacity, 0.75);
        assert_eq!(
            upper.blend_mode,
            photrez_core::document::content::BlendMode::Multiply
        );
        assert_eq!(upper.basic_adjustment, None);
        assert_eq!(upper.has_adjustments, Some(false));
        if transformed_locked_layers {
            let locked = applied
                .layers
                .iter()
                .find(|layer| layer.id == "preview-locked")
                .expect("locked raster layer");
            assert_eq!(locked.basic_adjustment, None);
            assert_eq!(locked.has_adjustments, Some(false));
        }
        if transformed_locked_layers {
            let locked = applied
                .layers
                .iter()
                .find(|layer| layer.id == "preview-locked")
                .expect("locked raster layer");
            assert_eq!(
                locked.transform,
                Transform2D {
                    x: 1.0,
                    y: -1.0,
                    scale_x: 0.8,
                    scale_y: 1.1,
                    rotation: -13.0,
                    flip_h: false,
                    flip_v: true,
                }
            );
            assert_eq!((locked.width, locked.height), (3.0, 3.0));
        }
        let output = photrez_core::render::compositor::composite_layers_in_region_with_font_data(
            applied,
            &pixel_store,
            Some(crate::INTER),
            photrez_core::render::compositor::CompositeRegion {
                x: 0,
                y: 0,
                width: output_size.0 as u32,
                height: output_size.1 as u32,
            },
        )
        .expect("composite after ApplyCrop");
        if zoom == 1.0 && window_scale == 1.0 && target_size.is_none() && !transformed_locked_layers
        {
            let preview_with_fill: Vec<u8> = fill_background.map_or_else(
                || preview.2.clone(),
                |fill| {
                    assert_eq!(frame, [0.0, 0.0, 3.0, 3.0]);
                    assert_eq!(viewport, (3.0, 3.0));
                    let normal = photrez_core::document::content::BlendMode::Normal;
                    preview
                        .2
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .flat_map(|pixel| {
                            photrez_core::render::compositor::composite_rgba_over(
                                fill,
                                [pixel[0], pixel[1], pixel[2], pixel[3]],
                                1.0,
                                &normal,
                            )
                        })
                        .collect()
                },
            );
            assert_eq!(
                (preview.0, preview.1),
                (output.region.width, output.region.height)
            );
            assert_eq!(preview_with_fill, output.pixels, "rotation {rotation}");
            if rotation == 90.0 {
                assert_eq!(&preview.2[12..16], &[7, 0, 0, 255]);
            }
        } else {
            let frame_screen = (
                (frame[0] * zoom + f64::from(camera.x) + 0.5).floor(),
                (frame[1] * zoom + f64::from(camera.y) + 0.5).floor(),
                (frame[2] * zoom + 0.5).floor(),
                (frame[3] * zoom + 0.5).floor(),
            );
            let pivot_screen = (
                frame_screen.0 + frame_screen.2 / 2.0,
                frame_screen.1 + frame_screen.3 / 2.0,
            );
            let pivot_document = (
                (pivot_screen.0 - f64::from(camera.x) - offset.0) / zoom,
                (pivot_screen.1 - f64::from(camera.y) - offset.1) / zoom,
            );
            let radians = rotation.to_radians();
            let (sin, cos) = radians.sin_cos();
            let crop_transform = photrez_core::engine::CropTransform {
                rect: [
                    region[0].round(),
                    region[1].round(),
                    region[2].round().max(1.0),
                    region[3].round().max(1.0),
                ],
                rotation: crate::crop_geometry::modern_apply_rotation(rotation),
                output_size,
            };
            let mut compared_samples = 0;
            let mut edge_samples = 0;
            let mut visible_samples = 0;
            let mut locked_layer_samples = 0;
            let mut flipped_layer_samples = 0;
            for y in 0..preview.1 {
                for x in 0..preview.0 {
                    let (dx, dy) = (
                        (f64::from(x) + 0.5) / f64::from(window_scale) - pivot_screen.0,
                        (f64::from(y) + 0.5) / f64::from(window_scale) - pivot_screen.1,
                    );
                    let document_point = (
                        pivot_document.0 + (dx * cos + dy * sin) / zoom,
                        pivot_document.1 + (-dx * sin + dy * cos) / zoom,
                    );
                    let output_point = crop_transform.map_point(document_point);
                    if output_point.0 < 0.0
                        || output_point.1 < 0.0
                        || output_point.0 >= f64::from(output.region.width)
                        || output_point.1 >= f64::from(output.region.height)
                    {
                        continue;
                    }
                    compared_samples += 1;
                    if output_point.0 < 0.5
                        || output_point.1 < 0.5
                        || output_point.0 >= output_size.0 - 0.5
                        || output_point.1 >= output_size.1 - 0.5
                    {
                        edge_samples += 1;
                    }
                    let expected = sample_rgba_bilinear(
                        &output.pixels,
                        output.region.width,
                        output.region.height,
                        output_point.0,
                        output_point.1,
                    );
                    if expected[3] > 0 {
                        visible_samples += 1;
                    }
                    if expected[1] == 240 && expected[2] == 180 {
                        locked_layer_samples += 1;
                    }
                    if expected[2] > expected[0] && expected[2] > expected[1] {
                        flipped_layer_samples += 1;
                    }
                    let index = (y as usize * preview.0 as usize + x as usize) * 4;
                    assert_eq!(
                        &preview.2[index..index + 4],
                        expected,
                        "rotation {rotation}, zoom {zoom}, pixel ({x}, {y})"
                    );
                }
            }
            assert!(
                compared_samples > 0,
                "preview must compare at least one applied pixel"
            );
            assert!(
                visible_samples > 0,
                "preview must compare non-transparent applied pixels"
            );
            if transformed_locked_layers {
                assert!(locked_layer_samples > 0, "the locked layer must contribute");
                assert!(
                    flipped_layer_samples > 0,
                    "the flipped layer must contribute"
                );
            }
            if rotation == 30.5 && zoom == 1.5 && target_size.is_none() {
                assert!(
                    edge_samples > 0,
                    "the comparison must sample the output edge"
                );
            }
        }
    }

    #[test]
    fn rotated_offset_multilayer_crop_preview_matches_the_executed_apply_output() {
        for rotation in [90.0, 30.5] {
            assert_rotated_offset_multilayer_crop_preview_matches_the_executed_apply_output(
                rotation, 1.0, 1.0, None, false, None,
            );
        }
        assert_rotated_offset_multilayer_crop_preview_matches_the_executed_apply_output(
            30.5, 1.5, 1.0, None, false, None,
        );
    }

    #[test]
    fn large_document_crop_preview_matches_the_executed_apply_output() {
        for rotation in [0.0, 30.5] {
            assert_rotated_offset_multilayer_crop_preview_matches_the_executed_apply_output(
                rotation,
                0.5,
                1.0,
                None,
                false,
                Some((25_000.0, 100.0)),
            );
        }
    }

    #[test]
    fn large_document_crop_preview_window_scale_only_changes_sampling() {
        let (mut content, mut input) = crop_preview_fixture();
        const EDGE: u32 = 8;
        let pixels: Vec<u8> = (0..EDGE)
            .flat_map(|y| {
                (0..EDGE).flat_map(move |x| {
                    [
                        (3 + x * 23 + y * 7) as u8,
                        (5 + x * 11 + y * 19) as u8,
                        (7 + x * 17 + y * 13) as u8,
                        255,
                    ]
                })
            })
            .collect();
        content.width = 25_000.0;
        content.height = 100.0;
        content.layers[0].width = f64::from(EDGE);
        content.layers[0].height = f64::from(EDGE);
        input.layers[0].width = EDGE;
        input.layers[0].height = EDGE;
        input.layers[0].pixels = Arc::new(pixels);

        let camera = Camera::new(0.0, 0.0, 0.5).expect("valid camera");
        let crop = CropPreviewTransform {
            frame: Some([0.0, 0.0, 8.0, 8.0]),
            offset: (0.0, 0.0),
            rotation: 0.0,
            target_size: Some((2500.0, 1000.0)),
        };
        let low_dpi = crop_preview_pixels(&content, &input, (4.0, 4.0), 1.0, camera, crop)
            .expect("low-DPI preview");
        let high_dpi = crop_preview_pixels(&content, &input, (4.0, 4.0), 2.0, camera, crop)
            .expect("high-DPI preview");
        assert_eq!((low_dpi.0, low_dpi.1), (4, 4));
        assert_eq!((high_dpi.0, high_dpi.1), (8, 8));

        for y in 1..high_dpi.1 - 1 {
            for x in 1..high_dpi.0 - 1 {
                let expected = sample_rgba_bilinear(
                    &low_dpi.2,
                    low_dpi.0,
                    low_dpi.1,
                    (f64::from(x) + 0.5) / 2.0,
                    (f64::from(y) + 0.5) / 2.0,
                );
                let index = (y as usize * high_dpi.0 as usize + x as usize) * 4;
                for (actual, expected) in high_dpi.2[index..index + 4].iter().zip(expected) {
                    assert!(
                        actual.abs_diff(expected) <= 1,
                        "DPI sample ({x}, {y}): actual {actual}, expected {expected}"
                    );
                }
            }
        }
    }

    #[test]
    fn crop_preview_matches_apply_at_low_zoom() {
        assert_rotated_offset_multilayer_crop_preview_matches_the_executed_apply_output(
            0.0, 0.5, 1.0, None, false, None,
        );
    }

    #[test]
    fn crop_preview_matches_apply_with_delete_and_background_fill() {
        assert_rotated_offset_multilayer_crop_preview_matches_the_executed_apply_output_with_fill(
            0.0,
            1.0,
            1.0,
            None,
            false,
            None,
            Some([24, 48, 72, 255]),
        );
    }

    #[test]
    fn rotated_crop_preview_matches_apply_at_low_zoom() {
        assert_rotated_offset_multilayer_crop_preview_matches_the_executed_apply_output(
            30.5, 0.5, 1.0, None, false, None,
        );
    }

    #[test]
    fn crop_preview_matches_apply_at_non_unit_window_scale() {
        assert_rotated_offset_multilayer_crop_preview_matches_the_executed_apply_output(
            90.0, 1.0, 2.0, None, false, None,
        );
    }

    #[test]
    fn crop_preview_matches_apply_for_size_target() {
        assert_rotated_offset_multilayer_crop_preview_matches_the_executed_apply_output(
            0.0,
            1.0,
            1.0,
            Some((6.0, 6.0)),
            false,
            None,
        );
    }

    #[test]
    fn crop_preview_matches_apply_for_transformed_and_locked_layers() {
        assert_rotated_offset_multilayer_crop_preview_matches_the_executed_apply_output(
            0.0, 1.0, 1.0, None, true, None,
        );
    }

    #[test]
    fn crop_preview_landmarks_match_the_modern_apply_region() {
        let (content, input) = crop_preview_fixture();
        let frame = [0.0, 0.0, 2.0, 2.0];
        let offset = (-1.0, 0.0);
        let apply_region = crate::crop_geometry::crop_apply_region(
            crate::crop_geometry::CropInteraction::Modern,
            frame,
            offset,
            crate::crop_geometry::CropViewport {
                pan_x: 0.0,
                pan_y: 0.0,
                zoom: 1.0,
            },
        );
        assert_eq!(apply_region, [1.0, 0.0, 2.0, 2.0]);

        let camera = Camera::new(0.0, 0.0, 1.0).expect("valid camera");
        let preview = crop_preview_pixels(
            &content,
            &input,
            (3.0, 3.0),
            1.0,
            camera,
            crate::crop_geometry::CropSnapshot {
                frame,
                offset,
                rotation: 0.0,
            },
        )
        .expect("offset preview");
        for y in 0..2_u32 {
            for x in 0..2_u32 {
                let preview_index = (y as usize * preview.0 as usize + x as usize) * 4;
                let source_index =
                    (((apply_region[1] as u32 + y) * 3 + apply_region[0] as u32 + x) * 4) as usize;
                assert_eq!(
                    &preview.2[preview_index..preview_index + 4],
                    &input.layers[0].pixels[source_index..source_index + 4],
                    "preview landmark ({x}, {y}) must come from the applied crop region"
                );
            }
        }
    }

    #[test]
    fn crop_preview_raster_size_tracks_the_window_scale() {
        let (content, input) = crop_preview_fixture();
        let camera = Camera::new(0.0, 0.0, 1.0).expect("valid camera");
        let preview = crop_preview_pixels(
            &content,
            &input,
            (3.0, 3.0),
            2.0,
            camera,
            crate::crop_geometry::CropSnapshot {
                frame: [0.0, 0.0, 3.0, 3.0],
                offset: (0.0, 0.0),
                rotation: 0.0,
            },
        )
        .expect("high-DPI preview");
        assert_eq!((preview.0, preview.1), (6, 6));
        let red = (0..preview.0 as usize)
            .map(|x| preview.2[x * 4])
            .collect::<Vec<_>>();
        assert_eq!(red, [1, 1, 2, 2, 3, 3]);
    }

    #[test]
    fn crop_preview_samples_the_unscaled_composite_at_fractional_zoom_and_pan() {
        let (content, mut input) = crop_preview_fixture();
        input.layers[0].pixels = Arc::new(
            (0..9)
                .flat_map(|index| [u8::from(index % 3 == 1) * 255, 0, 0, 255])
                .collect(),
        );
        let camera = Camera::new(2.25, 0.75, 1.5).expect("valid camera");
        let preview = crop_preview_pixels(
            &content,
            &input,
            (8.0, 6.0),
            1.0,
            camera,
            crate::crop_geometry::CropSnapshot {
                frame: [0.0, 0.0, 2.0, 2.0],
                offset: (0.25, 0.75),
                rotation: 90.0,
            },
        )
        .expect("fractional transform preview");
        let pixel = (2 * preview.0 as usize + 3) * 4;
        assert_eq!(
            &preview.2[pixel..pixel + 4],
            &[43, 0, 0, 255],
            "the source composite is filtered once at the transformed screen sample"
        );
        assert_eq!((content.width, content.height), (3.0, 3.0));
        assert_eq!(content.layers[0].transform.scale_x, 1.0);
    }

    #[test]
    fn crop_preview_rounds_negative_half_screen_edges_like_the_oracle() {
        let (content, mut input) = crop_preview_fixture();
        input.layers[0].pixels = Arc::new(
            (0..9)
                .flat_map(|index| [u8::from(index % 3 == 1) * 255, 0, 0, 255])
                .collect(),
        );
        let camera = Camera::new(0.0, 0.0, 1.0).expect("valid camera");
        let preview = crop_preview_pixels(
            &content,
            &input,
            (3.0, 3.0),
            1.0,
            camera,
            crate::crop_geometry::CropSnapshot {
                frame: [-0.5, 0.0, 2.5, 2.0],
                offset: (0.0, 0.0),
                rotation: 90.0,
            },
        )
        .expect("negative half-pixel frame preview");
        let pixel = (preview.0 as usize + 1) * 4;
        assert_eq!(
            &preview.2[pixel..pixel + 4],
            &[128, 0, 0, 255],
            "Math.round maps -0.5 to zero before finding the image pivot"
        );
    }

    #[test]
    fn crop_preview_composites_transparent_layers_without_mutating_content() {
        let (mut content, mut input) = crop_preview_fixture();
        let transparent_red_and_opaque_red = (0..9)
            .flat_map(|index| match index {
                4 => [80, 10, 20, 255],
                _ => [255, 0, 0, 0],
            })
            .collect::<Vec<_>>();
        input.layers[0].pixels = Arc::new(transparent_red_and_opaque_red);

        let mut upper = content.layers[0].clone();
        upper.id = "preview-upper".to_owned();
        upper.name = "Preview upper".to_owned();
        let mut upper_pixels = vec![0; 3 * 3 * 4];
        upper_pixels[4 * 4..5 * 4].copy_from_slice(&[10, 20, 240, 128]);
        content.layers.insert(0, upper);
        input.layers.push(ProjectionLayer {
            id: "preview-upper".to_owned(),
            width: 3,
            height: 3,
            epoch: 2,
            pixels: Arc::new(upper_pixels),
        });

        let content_before = content.clone();
        let input_before = input
            .layers
            .iter()
            .map(|layer| Arc::clone(&layer.pixels))
            .collect::<Vec<_>>();
        let camera = Camera::new(0.0, 0.0, 1.0).expect("valid camera");
        let preview = crop_preview_pixels(
            &content,
            &input,
            (3.0, 3.0),
            1.0,
            camera,
            crate::crop_geometry::CropSnapshot {
                frame: [0.0, 0.0, 3.0, 3.0],
                offset: (0.0, 0.0),
                rotation: 0.0,
            },
        )
        .expect("layered preview");

        let transparent_pixel = 0;
        let composited_pixel = (preview.0 as usize + 1) * 4;
        assert_eq!(
            &preview.2[transparent_pixel..transparent_pixel + 4],
            &[0, 0, 0, 0]
        );
        assert_eq!(
            &preview.2[composited_pixel..composited_pixel + 4],
            &[45, 15, 130, 255]
        );
        assert_eq!(content, content_before);
        assert!(
            input
                .layers
                .iter()
                .zip(input_before)
                .all(|(layer, before)| layer.pixels.as_slice() == before.as_slice())
        );
    }

    #[test]
    fn crop_preview_uses_the_oracle_rounded_frame_for_its_rotation_pivot() {
        let (content, mut input) = crop_preview_fixture();
        input.layers[0].pixels = Arc::new(
            (0..9)
                .flat_map(|index| [u8::from(index % 3 == 1) * 255, 0, 0, 255])
                .collect(),
        );
        let camera = Camera::new(2.25, 0.75, 1.5).expect("valid camera");
        let preview = crop_preview_pixels(
            &content,
            &input,
            (8.0, 6.0),
            1.0,
            camera,
            crate::crop_geometry::CropSnapshot {
                frame: [0.2, 0.2, 1.2, 1.2],
                offset: (0.0, 0.0),
                rotation: 90.0,
            },
        )
        .expect("fractional frame preview");
        let pixel = (2 * preview.0 as usize + 4) * 4;
        assert_eq!(
            &preview.2[pixel..pixel + 4],
            &[255, 0, 0, 255],
            "the image pivot follows the frame rounded into screen pixels"
        );
    }

    #[test]
    #[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
    fn crop_preview_low_zoom_memory_and_performance() {
        const DOC_EDGE: u32 = 4096;
        const VIEWPORT: (f32, f32) = (800.0, 600.0);
        const SOURCE_REGION: (u32, u32) = (803, 603);
        const SAMPLES: usize = 9;

        let (mut content, mut input) = crop_preview_fixture();
        content.width = f64::from(DOC_EDGE);
        content.height = f64::from(DOC_EDGE);
        content.layers[0].width = f64::from(DOC_EDGE);
        content.layers[0].height = f64::from(DOC_EDGE);
        let layer_bytes = rgba_byte_len(DOC_EDGE, DOC_EDGE).expect("fixture byte length");
        input.layers[0].width = DOC_EDGE;
        input.layers[0].height = DOC_EDGE;
        input.layers[0].pixels = Arc::new(vec![255; layer_bytes]);

        let camera = Camera::centred_at(
            VIEWPORT.0,
            VIEWPORT.1,
            DOC_EDGE as f32,
            DOC_EDGE as f32,
            0.05,
        )
        .expect("minimum zoom camera");
        let offset = (0.5, -0.25);
        let inverse = |screen: f32, origin: f32, delta: f64| {
            (f64::from(screen) - f64::from(origin) - delta) / f64::from(camera.zoom)
        };
        assert!(inverse(0.0, camera.x, offset.0) <= 0.0);
        assert!(inverse(VIEWPORT.0, camera.x, offset.0) >= f64::from(DOC_EDGE));
        assert!(inverse(0.0, camera.y, offset.1) <= 0.0);
        assert!(inverse(VIEWPORT.1, camera.y, offset.1) >= f64::from(DOC_EDGE));

        let crop = crate::crop_geometry::CropSnapshot {
            frame: [1024.0, 1024.0, 2048.0, 2048.0],
            offset,
            rotation: 0.0,
        };
        let composite_bytes =
            rgba_byte_len(SOURCE_REGION.0, SOURCE_REGION.1).expect("source byte length");
        let output_bytes =
            rgba_byte_len(VIEWPORT.0 as u32, VIEWPORT.1 as u32).expect("viewport byte length");
        let known_rgba_peak = layer_bytes + composite_bytes + output_bytes;
        let render = || {
            let (width, height, pixels) =
                crop_preview_pixels(&content, &input, VIEWPORT, 1.0, camera, crop)
                    .expect("low-zoom crop preview");
            assert_eq!((width, height), (800, 600));
            let center = ((height as usize / 2 * width as usize) + width as usize / 2) * 4;
            assert_eq!(&pixels[center..center + 4], &[255; 4]);
            image_from_rgba(width, height, pixels).expect("preview image")
        };

        std::hint::black_box(render());
        let mut samples = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let start = std::time::Instant::now();
            std::hint::black_box(render());
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(f64::total_cmp);
        let p50 = sorted[sorted.len() / 2];
        let p95 = sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)];
        println!(
            "CROP_LOW_ZOOM doc={DOC_EDGE}x{DOC_EDGE} layers=1 viewport={}x{} scale=1.0 zoom={:.2} source_region={}x{} input_rgba={:.2}MiB composite_rgba={:.2}MiB output_rgba={:.2}MiB known_rgba_peak={:.2}MiB samples_ms={samples:?} p50={p50:.3} p95={p95:.3}",
            VIEWPORT.0 as u32,
            VIEWPORT.1 as u32,
            camera.zoom,
            SOURCE_REGION.0,
            SOURCE_REGION.1,
            layer_bytes as f64 / (1024.0 * 1024.0),
            composite_bytes as f64 / (1024.0 * 1024.0),
            output_bytes as f64 / (1024.0 * 1024.0),
            known_rgba_peak as f64 / (1024.0 * 1024.0),
        );
    }
}
