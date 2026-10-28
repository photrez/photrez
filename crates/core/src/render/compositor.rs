// SPDX-License-Identifier: AGPL-3.0-or-later
//! CPU compositor for the document layer stack.
//!
//! Covers raster, shape, and text layers, content order (first = top),
//! visibility, opacity, all twelve blend modes, and the existing transforms.
//! A visible unsupported layer is rejected rather than silently skipped.
//!
//! The reference is the frozen engine's `compositeAllLayers`, which draws the
//! layer array from last to first with Canvas2D `source-over`. The goldens in
//! the tests were produced by browser Canvas2D with the same draw order, so
//! they are independent of this implementation.

use super::{shape, text};
use crate::pixel::kernel::apply_basic_adjustment;
use crate::pixel::store::PixelStoreRegistry;
use crate::ptz::content::{BlendMode, LayerType, PtzDocumentContent, PtzLayer, Transform2D};
use std::fmt;

/// A composited document image in row-major RGBA order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositedRgba {
    pub width: u32,
    pub height: u32,
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
/// The frozen editor rendered text with the app's loaded face; without this
/// data a layer asking for that family falls back to whatever the machine has.
pub fn composite_layers_with_font_data(
    content: &PtzDocumentContent,
    pixels: &PixelStoreRegistry,
    font_data: Option<&[u8]>,
) -> Result<CompositedRgba, CompositeError> {
    let (width, height) = document_dimensions(content)?;
    if width == 0 || height == 0 {
        return Ok(CompositedRgba {
            width,
            height,
            pixels: Vec::new(),
        });
    }
    let byte_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|length| length.checked_mul(4))
        .ok_or(CompositeError::DocumentSizeOverflow)?;
    let mut out = vec![0u8; byte_len];

    for layer in content.layers.iter().rev() {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
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
        let shape_bitmap = if matches!(layer.layer_type, LayerType::Shape) {
            let params =
                layer
                    .shape_params
                    .as_ref()
                    .ok_or_else(|| CompositeError::InvalidShape {
                        layer_id: layer.id.clone(),
                    })?;
            Some(
                shape::rasterize_shape(params).map_err(|_| CompositeError::InvalidShape {
                    layer_id: layer.id.clone(),
                })?,
            )
        } else {
            None
        };
        let text_bitmap = if matches!(layer.layer_type, LayerType::Text) {
            let data = layer
                .text_data
                .as_ref()
                .ok_or_else(|| CompositeError::InvalidText {
                    layer_id: layer.id.clone(),
                })?;
            Some(text::rasterize_text(data, font_data, 1.0).map_err(|_| {
                CompositeError::InvalidText {
                    layer_id: layer.id.clone(),
                }
            })?)
        } else {
            None
        };
        let (layer_width, layer_height) = if let Some(bitmap) = &shape_bitmap {
            (bitmap.width, bitmap.height)
        } else if let Some(bitmap) = &text_bitmap {
            (bitmap.width, bitmap.height)
        } else {
            layer_dimensions(layer)?
        };
        if layer_width == 0 && layer_height == 0 {
            continue;
        }
        let base_pixels: &[u8] = if let Some(bitmap) = &shape_bitmap {
            &bitmap.pixels
        } else if let Some(bitmap) = &text_bitmap {
            &bitmap.pixels
        } else {
            let source = pixels.get_layer(&content.id, &layer.id).ok_or_else(|| {
                CompositeError::MissingPixels {
                    layer_id: layer.id.clone(),
                }
            })?;
            if source.width != layer_width || source.height != layer_height {
                return Err(CompositeError::PixelDimensionMismatch {
                    layer_id: layer.id.clone(),
                });
            }
            let expected_len = (layer_width as usize)
                .checked_mul(layer_height as usize)
                .and_then(|length| length.checked_mul(4))
                .ok_or(CompositeError::DocumentSizeOverflow)?;
            if source.pixels.len() != expected_len {
                return Err(CompositeError::PixelDimensionMismatch {
                    layer_id: layer.id.clone(),
                });
            }
            &source.pixels
        };
        let adjusted_pixels = layer.basic_adjustment.as_ref().map(|adjustment| {
            apply_basic_adjustment(
                base_pixels,
                adjustment.brightness,
                adjustment.contrast,
                adjustment.saturation,
            )
        });
        let source_pixels: &[u8] = adjusted_pixels.as_deref().unwrap_or(base_pixels);

        let opacity = opacity_alpha(layer.opacity);
        if let Some((offset_x, offset_y)) = exact_offsets {
            let flip_h = layer.transform.flip_h;
            let flip_v = layer.transform.flip_v;
            for sy in 0..layer_height as usize {
                let src_row = sy * layer_width as usize * 4;
                for sx in 0..layer_width as usize {
                    let source_x = if flip_h {
                        layer_width as i64 - 1 - sx as i64
                    } else {
                        sx as i64
                    };
                    let source_y = if flip_v {
                        layer_height as i64 - 1 - sy as i64
                    } else {
                        sy as i64
                    };
                    let x = offset_x + source_x;
                    let y = offset_y + source_y;
                    if x < 0 || y < 0 || x >= width as i64 || y >= height as i64 {
                        continue;
                    }
                    let dst = (y as usize * width as usize + x as usize) * 4;
                    let src = src_row + sx * 4;
                    let mut dst_pixel: [u8; 4] = out[dst..dst + 4]
                        .try_into()
                        .expect("destination pixel is four bytes");
                    let src_pixel: [u8; 4] = source_pixels[src..src + 4]
                        .try_into()
                        .expect("source pixel is four bytes");
                    source_over(&mut dst_pixel, src_pixel, opacity, &layer.blend_mode);
                    out[dst..dst + 4].copy_from_slice(&dst_pixel);
                }
            }
        } else {
            composite_transformed(
                &mut out,
                width as usize,
                height as usize,
                source_pixels,
                layer_width as usize,
                layer_height as usize,
                &layer.transform,
                opacity,
                &layer.blend_mode,
            );
        }
    }

    Ok(CompositedRgba {
        width,
        height,
        pixels: out,
    })
}

/// Composite with the system font stack only.
pub fn composite_layers(
    content: &PtzDocumentContent,
    pixels: &PixelStoreRegistry,
) -> Result<CompositedRgba, CompositeError> {
    composite_layers_with_font_data(content, pixels, None)
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

#[allow(clippy::too_many_arguments)] // flat pixel-loop context; a struct would add no validation
fn composite_transformed(
    out: &mut [u8],
    doc_width: usize,
    doc_height: usize,
    source: &[u8],
    layer_width: usize,
    layer_height: usize,
    transform: &Transform2D,
    opacity: u32,
    blend_mode: &BlendMode,
) {
    let w = layer_width as f64;
    let h = layer_height as f64;
    let sx = transform.scale_x * if transform.flip_h { -1.0 } else { 1.0 };
    let sy = transform.scale_y * if transform.flip_v { -1.0 } else { 1.0 };
    if sx == 0.0 || sy == 0.0 {
        return;
    }
    let cx = transform.x + (w * transform.scale_x.abs()) / 2.0;
    let cy = transform.y + (h * transform.scale_y.abs()) / 2.0;
    let theta = transform.rotation.to_radians();
    let cos = theta.cos();
    let sin = theta.sin();

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (corner_x, corner_y) in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)] {
        let local_x = (corner_x - w / 2.0) * sx;
        let local_y = (corner_y - h / 2.0) * sy;
        let x = cx + local_x * cos - local_y * sin;
        let y = cy + local_x * sin + local_y * cos;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let x0 = (min_x.floor() - 1.0).max(0.0) as usize;
    let y0 = (min_y.floor() - 1.0).max(0.0) as usize;
    let x1 = (max_x.ceil() + 1.0).min(doc_width as f64) as usize;
    let y1 = (max_y.ceil() + 1.0).min(doc_height as f64) as usize;

    for y in y0..y1 {
        for x in x0..x1 {
            let rx = x as f64 + 0.5 - cx;
            let ry = y as f64 + 0.5 - cy;
            let local_x = rx * cos + ry * sin;
            let local_y = -rx * sin + ry * cos;
            let u = local_x / sx + w / 2.0;
            let v = local_y / sy + h / 2.0;
            let covered = coverage(x, y, w, h, cx, cy, cos, sin, sx, sy);
            if covered <= 0.0 {
                continue;
            }
            let mut src_pixel = sample_bilinear(source, layer_width, layer_height, u, v);
            let alpha = (src_pixel[3] as f64 * covered).round().clamp(0.0, 255.0) as u8;
            if alpha == 0 {
                continue;
            }
            src_pixel[3] = alpha;
            let dst = (y * doc_width + x) * 4;
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
    // Canvas2D's edge alpha follows the smaller of the two per-axis local
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

/// Straight-alpha source-over with Canvas2D's premultiplied 8-bit rounding.
///
/// The browser oracle returns 127/128 for 50% blue over opaque red; keeping the
/// premultiplied intermediate and rounding at each step reproduces that split.
fn source_over(dst: &mut [u8; 4], src: [u8; 4], opacity: u32, mode: &BlendMode) {
    let sa = div255(src[3] as u32 * opacity);
    if sa == 0 {
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
/// Canvas2D performs these in a premultiplied 8-bit pipeline, so a blend case
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
#[path = "compositor/tests.rs"]
mod tests;
