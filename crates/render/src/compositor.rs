// SPDX-License-Identifier: AGPL-3.0-or-later
//! CPU compositor for the document layer stack.
//!
//! Covers raster, shape, and text layers, content order (first = top),
//! visibility, opacity, all twelve blend modes, and the existing transforms.
//! A visible unsupported layer is rejected rather than silently skipped.
//!
//! The reference is the previous host engine's compositor: source-over
//! compositing, drawing the layer array from last to first. The
//! goldens in the tests were captured from that reference compositor with the
//! same draw order before this code existed, so they are independent of this
//! implementation.

use super::{shape, text};
use crate::input::RenderInput;
use photrez_document::content::{BlendMode, LayerType, PtzDocumentContent, PtzLayer, Transform2D};
use photrez_raster::kernel::apply_basic_adjustment;
use std::borrow::Cow;
use std::fmt;

/// A composited document image in row-major RGBA order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositedRgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// A document-space rectangle requested from the CPU compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositeRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// A row-major RGBA composite clipped to `region` in document coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositedRegionRgba {
    pub document_width: u32,
    pub document_height: u32,
    pub region: CompositeRegion,
    pub pixels: Vec<u8>,
}

/// Why the supported layer stack could not be composited. Every failure names
/// the layer it came from so the host can report which content is unsupported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositeError {
    ZeroDocumentDimension,
    DocumentSizeOverflow,
    UnsupportedLayerType { layer_id: String },
    UnsupportedTransform { layer_id: String },
    UnsupportedLayerDimension { layer_id: String },
    InvalidOpacity { layer_id: String },
    MissingPixels { layer_id: String },
    PixelDimensionMismatch { layer_id: String },
    InvalidShape { layer_id: String },
    InvalidText { layer_id: String },
}

impl fmt::Display for CompositeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDocumentDimension => {
                write!(formatter, "document dimensions must be positive integers")
            }
            Self::DocumentSizeOverflow => write!(formatter, "document size overflows memory"),
            Self::UnsupportedLayerType { layer_id } => {
                write!(formatter, "layer {layer_id} has an unsupported type")
            }
            Self::UnsupportedTransform { layer_id } => {
                write!(formatter, "layer {layer_id} has a non-finite transform")
            }
            Self::UnsupportedLayerDimension { layer_id } => {
                write!(formatter, "layer {layer_id} has an invalid size")
            }
            Self::InvalidOpacity { layer_id } => {
                write!(formatter, "layer {layer_id} has a non-finite opacity")
            }
            Self::MissingPixels { layer_id } => {
                write!(formatter, "layer {layer_id} has no canonical pixels")
            }
            Self::PixelDimensionMismatch { layer_id } => {
                write!(formatter, "layer {layer_id} pixels do not match its size")
            }
            Self::InvalidShape { layer_id } => {
                write!(formatter, "layer {layer_id} has invalid shape data")
            }
            Self::InvalidText { layer_id } => {
                write!(formatter, "layer {layer_id} has invalid text data")
            }
        }
    }
}

impl std::error::Error for CompositeError {}

/// Composite with the host's bundled font registered for text layers.
///
/// Text layers name the face the application embeds; without this data the
/// lookup falls back to whatever the machine has installed.
pub fn composite_layers_with_font_data(
    content: &PtzDocumentContent,
    pixels: &dyn RenderInput,
    font_data: Option<&[u8]>,
) -> Result<CompositedRgba, CompositeError> {
    let (width, height) = document_dimensions(content)?;
    let output = composite_layers_in_region_with_font_data(
        content,
        pixels,
        font_data,
        CompositeRegion {
            x: 0,
            y: 0,
            width,
            height,
        },
    )?;
    Ok(CompositedRgba {
        width,
        height,
        pixels: output.pixels,
    })
}

/// Composite with the system font stack only.
pub fn composite_layers(
    content: &PtzDocumentContent,
    pixels: &dyn RenderInput,
) -> Result<CompositedRgba, CompositeError> {
    composite_layers_with_font_data(content, pixels, None)
}

/// Composite the layer stack into a clipped document-space region; the returned region and
/// row-major straight RGBA8 pixels describe the intersection with the document bounds.
pub fn composite_layers_in_region_with_font_data(
    content: &PtzDocumentContent,
    pixels: &dyn RenderInput,
    font_data: Option<&[u8]>,
    region: CompositeRegion,
) -> Result<CompositedRegionRgba, CompositeError> {
    let (document_width, document_height) = document_dimensions(content)?;
    let x = region.x.min(document_width);
    let y = region.y.min(document_height);
    let right = region.x.saturating_add(region.width).min(document_width);
    let bottom = region.y.saturating_add(region.height).min(document_height);
    let clipped = CompositeRegion {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    };
    let byte_len = (clipped.width as usize)
        .checked_mul(clipped.height as usize)
        .and_then(|length| length.checked_mul(4))
        .ok_or(CompositeError::DocumentSizeOverflow)?;
    let mut out = vec![0u8; byte_len];

    if clipped.width > 0 && clipped.height > 0 {
        let context = CompositeRegionContext {
            content_id: &content.id,
            pixels,
            font_data,
            region: clipped,
        };
        for layer in content.layers.iter().rev() {
            composite_layer_in_region(&mut out, &context, layer)?;
        }
    }

    Ok(CompositedRegionRgba {
        document_width,
        document_height,
        region: clipped,
        pixels: out,
    })
}

struct CompositeRegionContext<'a> {
    content_id: &'a str,
    pixels: &'a dyn RenderInput,
    font_data: Option<&'a [u8]>,
    region: CompositeRegion,
}

struct CompositeLayerParams<'a> {
    source: &'a [u8],
    layer_width: usize,
    layer_height: usize,
    transform: &'a Transform2D,
    opacity: u32,
    blend_mode: &'a BlendMode,
}

fn composite_layer_in_region(
    out: &mut [u8],
    context: &CompositeRegionContext<'_>,
    layer: &PtzLayer,
) -> Result<(), CompositeError> {
    if !layer.visible || layer.opacity <= 0.0 {
        return Ok(());
    }
    if !layer.opacity.is_finite() {
        return Err(CompositeError::InvalidOpacity {
            layer_id: layer.id.clone(),
        });
    }
    if !matches!(
        layer.layer_type,
        LayerType::Raster | LayerType::Shape | LayerType::Text
    ) {
        return Err(CompositeError::UnsupportedLayerType {
            layer_id: layer.id.clone(),
        });
    }
    if !transform_is_finite(&layer.transform) {
        return Err(CompositeError::UnsupportedTransform {
            layer_id: layer.id.clone(),
        });
    }

    let exact_offsets = if is_exact_axis_aligned(&layer.transform) {
        match (
            integer_offset(layer.transform.x),
            integer_offset(layer.transform.y),
        ) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        }
    } else {
        None
    };
    let (layer_width, layer_height, base_pixels) = layer_pixels(layer, context)?;
    if layer_width == 0 && layer_height == 0
        || !layer_intersects_region(
            layer_width,
            layer_height,
            &layer.transform,
            context.region,
            exact_offsets,
        )
    {
        return Ok(());
    }

    let adjusted_pixels = layer.basic_adjustment.as_ref().map(|adjustment| {
        apply_basic_adjustment(
            &base_pixels,
            adjustment.brightness,
            adjustment.contrast,
            adjustment.saturation,
        )
    });
    let composite = CompositeLayerParams {
        source: adjusted_pixels.as_deref().unwrap_or(&base_pixels),
        layer_width: layer_width as usize,
        layer_height: layer_height as usize,
        transform: &layer.transform,
        opacity: opacity_alpha(layer.opacity),
        blend_mode: &layer.blend_mode,
    };
    if exact_offsets.is_some() {
        composite_exact(out, context.region, &composite);
    } else {
        composite_transformed(out, context.region, &composite);
    }
    Ok(())
}

fn layer_pixels<'a>(
    layer: &PtzLayer,
    context: &CompositeRegionContext<'a>,
) -> Result<(u32, u32, Cow<'a, [u8]>), CompositeError> {
    match layer.layer_type {
        LayerType::Raster => {
            let (width, height) = layer_dimensions(layer)?;
            if width == 0 && height == 0 {
                return Ok((width, height, Cow::Borrowed(&[])));
            }
            let source = context
                .pixels
                .layer_pixels(context.content_id, &layer.id)
                .ok_or_else(|| CompositeError::MissingPixels {
                    layer_id: layer.id.clone(),
                })?;
            if source.width != width || source.height != height {
                return Err(CompositeError::PixelDimensionMismatch {
                    layer_id: layer.id.clone(),
                });
            }
            let expected_len = (width as usize)
                .checked_mul(height as usize)
                .and_then(|length| length.checked_mul(4))
                .ok_or(CompositeError::DocumentSizeOverflow)?;
            if source.pixels.len() != expected_len {
                return Err(CompositeError::PixelDimensionMismatch {
                    layer_id: layer.id.clone(),
                });
            }
            Ok((width, height, Cow::Borrowed(source.pixels)))
        }
        LayerType::Shape => {
            let params =
                layer
                    .shape_params
                    .as_ref()
                    .ok_or_else(|| CompositeError::InvalidShape {
                        layer_id: layer.id.clone(),
                    })?;
            let bitmap =
                shape::rasterize_shape(params).map_err(|_| CompositeError::InvalidShape {
                    layer_id: layer.id.clone(),
                })?;
            Ok((bitmap.width, bitmap.height, Cow::Owned(bitmap.pixels)))
        }
        LayerType::Text => {
            let data = layer
                .text_data
                .as_ref()
                .ok_or_else(|| CompositeError::InvalidText {
                    layer_id: layer.id.clone(),
                })?;
            // A fresh text session starts with empty `content`; the layer
            // paints nothing until the first glyph arrives.
            if data.content.is_empty() {
                return Ok((1, 1, Cow::Owned(vec![0, 0, 0, 0])));
            }
            let bitmap = text::rasterize_text(data, context.font_data, 1.0).map_err(|_| {
                CompositeError::InvalidText {
                    layer_id: layer.id.clone(),
                }
            })?;
            Ok((bitmap.width, bitmap.height, Cow::Owned(bitmap.pixels)))
        }
        _ => Err(CompositeError::UnsupportedLayerType {
            layer_id: layer.id.clone(),
        }),
    }
}

fn document_dimensions(content: &PtzDocumentContent) -> Result<(u32, u32), CompositeError> {
    let width = integer_dimension(content.width).ok_or(CompositeError::ZeroDocumentDimension)?;
    let height = integer_dimension(content.height).ok_or(CompositeError::ZeroDocumentDimension)?;
    if (width == 0) != (height == 0) {
        return Err(CompositeError::ZeroDocumentDimension);
    }
    Ok((width, height))
}

fn layer_dimensions(layer: &PtzLayer) -> Result<(u32, u32), CompositeError> {
    let width = integer_dimension(layer.width).ok_or_else(|| {
        CompositeError::UnsupportedLayerDimension {
            layer_id: layer.id.clone(),
        }
    })?;
    let height = integer_dimension(layer.height).ok_or_else(|| {
        CompositeError::UnsupportedLayerDimension {
            layer_id: layer.id.clone(),
        }
    })?;
    if (width == 0) != (height == 0) {
        return Err(CompositeError::UnsupportedLayerDimension {
            layer_id: layer.id.clone(),
        });
    }
    Ok((width, height))
}

fn integer_dimension(value: f64) -> Option<u32> {
    if value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value <= u32::MAX as f64 {
        Some(value as u32)
    } else {
        None
    }
}

fn is_exact_axis_aligned(transform: &Transform2D) -> bool {
    transform.scale_x == 1.0 && transform.scale_y == 1.0 && transform.rotation == 0.0
}

fn transform_is_finite(transform: &Transform2D) -> bool {
    transform.x.is_finite()
        && transform.y.is_finite()
        && transform.scale_x.is_finite()
        && transform.scale_y.is_finite()
        && transform.rotation.is_finite()
}

fn integer_offset(value: f64) -> Option<i64> {
    if value.is_finite()
        && value.fract() == 0.0
        && value >= i64::MIN as f64
        && value <= i64::MAX as f64
    {
        Some(value as i64)
    } else {
        None
    }
}

fn layer_intersects_region(
    layer_width: u32,
    layer_height: u32,
    transform: &Transform2D,
    region: CompositeRegion,
    exact_offsets: Option<(i64, i64)>,
) -> bool {
    if let Some((offset_x, offset_y)) = exact_offsets {
        let Some(right) = offset_x.checked_add(layer_width as i64) else {
            return false;
        };
        let Some(bottom) = offset_y.checked_add(layer_height as i64) else {
            return false;
        };
        let region_right = region.x.saturating_add(region.width) as i64;
        let region_bottom = region.y.saturating_add(region.height) as i64;
        return offset_x < region_right
            && offset_y < region_bottom
            && right > region.x as i64
            && bottom > region.y as i64;
    }

    let Some((min_x, min_y, max_x, max_y)) =
        transformed_bounds(layer_width as usize, layer_height as usize, transform)
    else {
        return false;
    };
    let left = (min_x.floor() - 1.0).max(0.0);
    let top = (min_y.floor() - 1.0).max(0.0);
    let right = max_x.ceil() + 1.0;
    let bottom = max_y.ceil() + 1.0;
    let region_right = region.x.saturating_add(region.width) as f64;
    let region_bottom = region.y.saturating_add(region.height) as f64;
    left < region_right
        && top < region_bottom
        && right > region.x as f64
        && bottom > region.y as f64
}

fn transformed_bounds(
    layer_width: usize,
    layer_height: usize,
    transform: &Transform2D,
) -> Option<(f64, f64, f64, f64)> {
    let width = layer_width as f64;
    let height = layer_height as f64;
    let sx = transform.scale_x * if transform.flip_h { -1.0 } else { 1.0 };
    let sy = transform.scale_y * if transform.flip_v { -1.0 } else { 1.0 };
    if sx == 0.0 || sy == 0.0 {
        return None;
    }
    let cx = transform.x + (width * transform.scale_x.abs()) / 2.0;
    let cy = transform.y + (height * transform.scale_y.abs()) / 2.0;
    let theta = transform.rotation.to_radians();
    let cos = theta.cos();
    let sin = theta.sin();
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (corner_x, corner_y) in [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)] {
        let local_x = (corner_x - width / 2.0) * sx;
        let local_y = (corner_y - height / 2.0) * sy;
        let x = cx + local_x * cos - local_y * sin;
        let y = cy + local_x * sin + local_y * cos;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    [min_x, min_y, max_x, max_y]
        .into_iter()
        .all(f64::is_finite)
        .then_some((min_x, min_y, max_x, max_y))
}

fn composite_exact(out: &mut [u8], region: CompositeRegion, composite: &CompositeLayerParams<'_>) {
    let source = composite.source;
    let layer_width = composite.layer_width;
    let layer_height = composite.layer_height;
    let transform = composite.transform;
    let opacity = composite.opacity;
    let blend_mode = composite.blend_mode;
    let (Some(offset_x), Some(offset_y)) =
        (integer_offset(transform.x), integer_offset(transform.y))
    else {
        return;
    };
    let Some(layer_right) = offset_x.checked_add(layer_width as i64) else {
        return;
    };
    let Some(layer_bottom) = offset_y.checked_add(layer_height as i64) else {
        return;
    };
    let left = offset_x.max(region.x as i64).max(0);
    let top = offset_y.max(region.y as i64).max(0);
    let right = layer_right.min(region.x.saturating_add(region.width) as i64);
    let bottom = layer_bottom.min(region.y.saturating_add(region.height) as i64);
    if right <= left || bottom <= top {
        return;
    }

    for y in top..bottom {
        let layer_y = (y - offset_y) as usize;
        let source_y = if transform.flip_v {
            layer_height - 1 - layer_y
        } else {
            layer_y
        };
        for x in left..right {
            let layer_x = (x - offset_x) as usize;
            let source_x = if transform.flip_h {
                layer_width - 1 - layer_x
            } else {
                layer_x
            };
            let src = (source_y * layer_width + source_x) * 4;
            let dst = (((y - region.y as i64) as usize * region.width as usize)
                + (x - region.x as i64) as usize)
                * 4;
            let mut dst_pixel: [u8; 4] = out[dst..dst + 4]
                .try_into()
                .expect("destination pixel is four bytes");
            let src_pixel: [u8; 4] = source[src..src + 4]
                .try_into()
                .expect("source pixel is four bytes");
            source_over(&mut dst_pixel, src_pixel, opacity, blend_mode);
            out[dst..dst + 4].copy_from_slice(&dst_pixel);
        }
    }
}

fn composite_transformed(
    out: &mut [u8],
    region: CompositeRegion,
    composite: &CompositeLayerParams<'_>,
) {
    let source = composite.source;
    let layer_width = composite.layer_width;
    let layer_height = composite.layer_height;
    let transform = composite.transform;
    let opacity = composite.opacity;
    let blend_mode = composite.blend_mode;
    let width = layer_width as f64;
    let height = layer_height as f64;
    let sx = transform.scale_x * if transform.flip_h { -1.0 } else { 1.0 };
    let sy = transform.scale_y * if transform.flip_v { -1.0 } else { 1.0 };
    if sx == 0.0 || sy == 0.0 {
        return;
    }
    let cx = transform.x + (width * transform.scale_x.abs()) / 2.0;
    let cy = transform.y + (height * transform.scale_y.abs()) / 2.0;
    let theta = transform.rotation.to_radians();
    let cos = theta.cos();
    let sin = theta.sin();
    let Some((min_x, min_y, max_x, max_y)) =
        transformed_bounds(layer_width, layer_height, transform)
    else {
        return;
    };
    let region_right = region.x.saturating_add(region.width) as f64;
    let region_bottom = region.y.saturating_add(region.height) as f64;
    let x0 = (min_x.floor() - 1.0).max(region.x as f64).max(0.0) as usize;
    let y0 = (min_y.floor() - 1.0).max(region.y as f64).max(0.0) as usize;
    let x1 = (max_x.ceil() + 1.0).min(region_right) as usize;
    let y1 = (max_y.ceil() + 1.0).min(region_bottom) as usize;

    for y in y0..y1 {
        for x in x0..x1 {
            let rx = x as f64 + 0.5 - cx;
            let ry = y as f64 + 0.5 - cy;
            let local_x = rx * cos + ry * sin;
            let local_y = -rx * sin + ry * cos;
            let u = local_x / sx + width / 2.0;
            let v = local_y / sy + height / 2.0;
            let covered = coverage(x, y, width, height, cx, cy, cos, sin, sx, sy);
            if covered <= 0.0 {
                continue;
            }
            let mut src_pixel = sample_bilinear(source, layer_width, layer_height, u, v);
            let alpha = (src_pixel[3] as f64 * covered).round().clamp(0.0, 255.0) as u8;
            if alpha == 0 {
                continue;
            }
            src_pixel[3] = alpha;
            let dst =
                (((y - region.y as usize) * region.width as usize) + (x - region.x as usize)) * 4;
            let mut dst_pixel: [u8; 4] = out[dst..dst + 4]
                .try_into()
                .expect("destination pixel is four bytes");
            source_over(&mut dst_pixel, src_pixel, opacity, blend_mode);
            out[dst..dst + 4].copy_from_slice(&dst_pixel);
        }
    }
}
fn sample_bilinear(source: &[u8], width: usize, height: usize, u: f64, v: f64) -> [u8; 4] {
    let x = (u - 0.5).clamp(0.0, (width - 1) as f64);
    let y = (v - 0.5).clamp(0.0, (height - 1) as f64);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let fx = x - x0 as f64;
    let fy = y - y0 as f64;
    let mut alpha = 0.0;
    let mut red = 0.0;
    let mut green = 0.0;
    let mut blue = 0.0;
    for (sample_x, sample_y, weight) in [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x1, y0, fx * (1.0 - fy)),
        (x0, y1, (1.0 - fx) * fy),
        (x1, y1, fx * fy),
    ] {
        if weight <= 0.0 {
            continue;
        }
        let index = (sample_y * width + sample_x) * 4;
        let sample_alpha = source[index + 3] as f64;
        alpha += weight * sample_alpha;
        red += weight * source[index] as f64 * sample_alpha;
        green += weight * source[index + 1] as f64 * sample_alpha;
        blue += weight * source[index + 2] as f64 * sample_alpha;
    }
    if alpha <= 0.0 {
        return [0, 0, 0, 0];
    }
    [
        (red / alpha).round().clamp(0.0, 255.0) as u8,
        (green / alpha).round().clamp(0.0, 255.0) as u8,
        (blue / alpha).round().clamp(0.0, 255.0) as u8,
        alpha.round().clamp(0.0, 255.0) as u8,
    ]
}

#[allow(clippy::too_many_arguments)] // flat inverse-mapping context; a struct would add no validation
fn coverage(
    x: usize,
    y: usize,
    w: f64,
    h: f64,
    cx: f64,
    cy: f64,
    cos: f64,
    sin: f64,
    sx: f64,
    sy: f64,
) -> f64 {
    // The reference compositor's edge alpha follows the smaller of the two per-axis local
    // coverages, not the 2D overlap area. A destination pixel becomes a rotated
    // parallelogram in layer space; its local extents give the two ratios.
    let mut min_u = f64::INFINITY;
    let mut max_u = f64::NEG_INFINITY;
    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for (offset_x, offset_y) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        let rx = x as f64 + offset_x - cx;
        let ry = y as f64 + offset_y - cy;
        let local_x = rx * cos + ry * sin;
        let local_y = -rx * sin + ry * cos;
        let u = local_x / sx + w / 2.0;
        let v = local_y / sy + h / 2.0;
        min_u = min_u.min(u);
        max_u = max_u.max(u);
        min_v = min_v.min(v);
        max_v = max_v.max(v);
    }
    let span_u = (max_u - min_u).max(f64::EPSILON);
    let span_v = (max_v - min_v).max(f64::EPSILON);
    let overlap_u = (max_u.min(w) - min_u.max(0.0)).max(0.0);
    let overlap_v = (max_v.min(h) - min_v.max(0.0)).max(0.0);
    (overlap_u / span_u).min(overlap_v / span_v).clamp(0.0, 1.0)
}

fn opacity_alpha(opacity: f64) -> u32 {
    ((opacity.clamp(0.0, 1.0) * 255.0).round() as u32).min(255)
}

/// Straight-alpha source-over with the reference compositor's premultiplied 8-bit rounding.
///
/// The reference capture returns 127/128 for 50% blue over opaque red; keeping the
/// premultiplied intermediate and rounding at each step reproduces that split.
fn source_over(dst: &mut [u8; 4], src: [u8; 4], opacity: u32, mode: &BlendMode) {
    let sa = div255(src[3] as u32 * opacity);
    if sa == 0 {
        return;
    }
    if matches!(mode, BlendMode::Normal) {
        if sa == 255 {
            *dst = src;
            return;
        }
        let da = dst[3] as u32;
        let inv = 255 - sa;
        let out_a = (sa + div255(da * inv)).min(255);
        for channel in 0..3 {
            let src_premul = div255(src[channel] as u32 * sa);
            let dst_premul = div255(dst[channel] as u32 * da);
            let premul = (src_premul + div255(dst_premul * inv)).min(255);
            dst[channel] = ((premul * 255 + out_a / 2) / out_a).min(255) as u8;
        }
        dst[3] = out_a as u8;
        return;
    }
    let da = dst[3] as u32;
    let inv = 255 - sa;
    let out_a = (sa + div255(da * inv)).min(255);
    if out_a == 0 {
        *dst = [0, 0, 0, 0];
        return;
    }
    let da_ratio = da as f64 / 255.0;
    for channel in 0..3 {
        let cb = dst[channel] as f64 / 255.0;
        let cs = src[channel] as f64 / 255.0;
        let blended = blend_channel(mode, cb, cs).clamp(0.0, 1.0);
        let cs_prime = ((1.0 - da_ratio) * cs + da_ratio * blended).clamp(0.0, 1.0);
        let cs_prime = (cs_prime * 255.0).round() as u32;
        let src_premul = div255(cs_prime * sa);
        let dst_premul = div255(dst[channel] as u32 * da);
        let premul = (src_premul + div255(dst_premul * inv)).min(255);
        dst[channel] = ((premul * 255 + out_a / 2) / out_a).min(255) as u8;
    }
    dst[3] = out_a as u8;
}

/// W3C compositing blend functions over non-premultiplied channels.
///
/// The reference compositor performs these in a premultiplied 8-bit pipeline, so a blend case
/// that lands on a rounding boundary can differ by one channel unit.
fn blend_channel(mode: &BlendMode, cb: f64, cs: f64) -> f64 {
    match mode {
        BlendMode::Normal => cs,
        BlendMode::Multiply => cb * cs,
        BlendMode::Screen => cb + cs - cb * cs,
        BlendMode::Overlay => hard_light(cs, cb),
        BlendMode::Darken => cb.min(cs),
        BlendMode::Lighten => cb.max(cs),
        BlendMode::ColorDodge => {
            if cb <= 0.0 {
                0.0
            } else if cs >= 1.0 {
                1.0
            } else {
                (cb / (1.0 - cs)).min(1.0)
            }
        }
        BlendMode::ColorBurn => {
            if cb >= 1.0 {
                1.0
            } else if cs <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - cb) / cs).min(1.0)
            }
        }
        BlendMode::SoftLight => {
            if cs <= 0.5 {
                cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
            } else {
                let d = if cb <= 0.25 {
                    ((16.0 * cb - 12.0) * cb + 4.0) * cb
                } else {
                    cb.sqrt()
                };
                cb + (2.0 * cs - 1.0) * (d - cb)
            }
        }
        BlendMode::HardLight => hard_light(cb, cs),
        BlendMode::Difference => (cb - cs).abs(),
        BlendMode::Exclusion => cb + cs - 2.0 * cb * cs,
    }
}

fn hard_light(cb: f64, cs: f64) -> f64 {
    if cs <= 0.5 {
        2.0 * cb * cs
    } else {
        1.0 - 2.0 * (1.0 - cb) * (1.0 - cs)
    }
}

fn div255(value: u32) -> u32 {
    (value + 127) / 255
}

#[cfg(test)]
mod tests;
