// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shape rasterizer: `ShapeParams` -> RGBA bitmap.
//!
//! Draws ten shape kinds with a solid fill, a centered stroke, round caps and
//! joins, and anti-aliasing. `tiny-skia` is the backend, so the fill/stroke
//! pipeline produces Skia-compatible geometry.

#![cfg_attr(not(test), allow(dead_code))] // the production document render caller lands with the open workflow.

use std::f64::consts::PI;

use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform,
};

use photrez_document::layer_params::{ShapeFillKind, ShapeKind, ShapeParams};

const MIN_STROKE_WIDTH: f64 = 1.0;
const MAX_SHAPE_DIM: f64 = 16_384.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RasterizedShape {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShapeError {
    InvalidDimensions,
    InvalidColor,
    EmptyPath,
    Allocation,
}

pub(crate) fn shape_render_margin(params: &ShapeParams) -> f64 {
    if !params.stroke.enabled {
        return 0.0;
    }
    let stroke_width = params.stroke.width.max(MIN_STROKE_WIDTH);
    if params.kind != ShapeKind::Line {
        return stroke_width;
    }
    let cap = stroke_width;
    if !params.arrow_head {
        return cap;
    }
    let head = (stroke_width * 3.0).max(8.0);
    cap.max(head)
}

pub(crate) fn rasterize_shape(params: &ShapeParams) -> Result<RasterizedShape, ShapeError> {
    if !params.width.is_finite()
        || !params.height.is_finite()
        || !params.radius.is_finite()
        || !params.stroke.width.is_finite()
    {
        return Err(ShapeError::InvalidDimensions);
    }

    let is_line = params.kind == ShapeKind::Line;
    let valid = if is_line {
        params.width > 0.0 || params.height > 0.0
    } else {
        params.width > 0.0 && params.height > 0.0
    };
    if !valid {
        return Err(ShapeError::InvalidDimensions);
    }

    let margin = shape_render_margin(params);
    let width = bitmap_dimension(params.width + margin * 2.0);
    let height = bitmap_dimension(params.height + margin * 2.0);

    let stroke_enabled = if is_line { true } else { params.stroke.enabled };
    let stroke_width = if stroke_enabled {
        let default = if is_line { 2.0 } else { MIN_STROKE_WIDTH };
        let requested = if params.stroke.width == 0.0 {
            default
        } else {
            params.stroke.width
        };
        requested.max(MIN_STROKE_WIDTH)
    } else {
        0.0
    };

    let stroke_color =
        if is_line && !params.stroke.enabled && params.fill.kind == ShapeFillKind::Solid {
            parse_hex_color(&params.fill.color).ok_or(ShapeError::InvalidColor)?
        } else {
            parse_hex_color(&params.stroke.color).ok_or(ShapeError::InvalidColor)?
        };

    let fill_color = if is_line {
        if params.arrow_head {
            if params.fill.kind == ShapeFillKind::Solid {
                Some(parse_hex_color(&params.fill.color).ok_or(ShapeError::InvalidColor)?)
            } else {
                Some(stroke_color)
            }
        } else {
            None
        }
    } else if params.fill.kind == ShapeFillKind::Solid {
        Some(parse_hex_color(&params.fill.color).ok_or(ShapeError::InvalidColor)?)
    } else {
        None
    };

    let path = build_path(params, params.width, params.height, stroke_width)
        .ok_or(ShapeError::EmptyPath)?;
    // The reference compositor uses analytic coverage AA. tiny-skia's direct AA differs by up
    // to ~100 alpha units on 1px strokes; a small 4x box supersample brings the
    // coverage closer without changing the geometry. Large shapes skip it.
    let supersample: u32 = if (width as u64) * (height as u64) <= 65_536 {
        4
    } else {
        1
    };
    let render_width = width * supersample;
    let render_height = height * supersample;
    let mut pixmap = Pixmap::new(render_width, render_height).ok_or(ShapeError::Allocation)?;
    let scale = supersample as f32;
    let transform = Transform::from_scale(scale, scale)
        .post_translate(margin as f32 * scale, margin as f32 * scale);

    if let Some(color) = fill_color {
        let mut paint = Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
        paint.force_hq_pipeline = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
    }
    if stroke_width > 0.0 {
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            stroke_color[0],
            stroke_color[1],
            stroke_color[2],
            stroke_color[3],
        );
        paint.force_hq_pipeline = true;
        // The reference compositor leaves the outer 90-degree corners of a plain rect
        // uncovered; bevel is the closest tiny-skia join. Curved and other
        // shapes keep the reference's round join.
        let line_join = if params.kind == ShapeKind::Rect && params.radius <= 0.0 {
            LineJoin::Bevel
        } else {
            LineJoin::Round
        };
        let stroke = Stroke {
            width: stroke_width as f32,
            line_cap: LineCap::Round,
            line_join,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, transform, None);
    }

    let pixels = downsample(&pixmap, width, height, supersample);
    Ok(RasterizedShape {
        width,
        height,
        pixels,
    })
}

fn bitmap_dimension(value: f64) -> u32 {
    value.min(MAX_SHAPE_DIM).round().max(1.0) as u32
}

fn downsample(pixmap: &Pixmap, width: u32, height: u32, factor: u32) -> Vec<u8> {
    let factor = factor as usize;
    let render_width = pixmap.width() as usize;
    let source = pixmap.pixels();
    let count = (factor * factor) as u64;
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let mut red = 0u64;
            let mut green = 0u64;
            let mut blue = 0u64;
            let mut alpha = 0u64;
            for row in 0..factor {
                let base = (y * factor + row) * render_width + x * factor;
                for column in 0..factor {
                    let pixel = source[base + column];
                    red += u64::from(pixel.red());
                    green += u64::from(pixel.green());
                    blue += u64::from(pixel.blue());
                    alpha += u64::from(pixel.alpha());
                }
            }
            let out_alpha = (alpha + count / 2) / count;
            if out_alpha == 0 {
                pixels.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                pixels.push(((red * 255 + alpha / 2) / alpha) as u8);
                pixels.push(((green * 255 + alpha / 2) / alpha) as u8);
                pixels.push(((blue * 255 + alpha / 2) / alpha) as u8);
                pixels.push(out_alpha as u8);
            }
        }
    }
    pixels
}

pub(super) fn parse_hex_color(value: &str) -> Option<[u8; 4]> {
    let hex = value.strip_prefix('#')?;
    if !hex.is_ascii() {
        return None;
    }
    let channel = |text: &str| u8::from_str_radix(text, 16).ok();
    match hex.len() {
        3 => {
            let red = channel(&hex[0..1])?;
            let green = channel(&hex[1..2])?;
            let blue = channel(&hex[2..3])?;
            Some([red * 17, green * 17, blue * 17, 255])
        }
        4 => {
            let red = channel(&hex[0..1])?;
            let green = channel(&hex[1..2])?;
            let blue = channel(&hex[2..3])?;
            let alpha = channel(&hex[3..4])?;
            Some([red * 17, green * 17, blue * 17, alpha * 17])
        }
        6 => Some([
            channel(&hex[0..2])?,
            channel(&hex[2..4])?,
            channel(&hex[4..6])?,
            255,
        ]),
        8 => Some([
            channel(&hex[0..2])?,
            channel(&hex[2..4])?,
            channel(&hex[4..6])?,
            channel(&hex[6..8])?,
        ]),
        _ => None,
    }
}

fn build_path(params: &ShapeParams, width: f64, height: f64, stroke_width: f64) -> Option<Path> {
    let mut builder = PathBuilder::new();
    let w = width as f32;
    let h = height as f32;
    match params.kind {
        ShapeKind::Rect => {
            let radius = params.radius.max(0.0).min(width.min(height) / 2.0) as f32;
            if radius > 0.0 {
                push_rounded_rect(&mut builder, w, h, radius);
            } else {
                builder.push_rect(Rect::from_xywh(0.0, 0.0, w, h)?);
            }
        }
        ShapeKind::Ellipse => {
            builder.push_oval(Rect::from_xywh(0.0, 0.0, w, h)?);
        }
        ShapeKind::Triangle => {
            builder.move_to(w / 2.0, 0.0);
            builder.line_to(w, h);
            builder.line_to(0.0, h);
            builder.close();
        }
        ShapeKind::Star => {
            let radius_x = width * 0.525_731;
            let radius_y = height * 0.552_786;
            let center_x = width / 2.0;
            let center_y = radius_y;
            let inner_ratio = 0.382;
            for index in 0..10 {
                let scale = if index % 2 == 0 { 1.0 } else { inner_ratio };
                let angle = (index as f64) * PI / 5.0 - PI / 2.0;
                let x = center_x + radius_x * scale * angle.cos();
                let y = center_y + radius_y * scale * angle.sin();
                if index == 0 {
                    builder.move_to(x as f32, y as f32);
                } else {
                    builder.line_to(x as f32, y as f32);
                }
            }
            builder.close();
        }
        ShapeKind::BlockArrow => {
            let head_width = width * 0.4;
            let stem_top = height * 0.25;
            let stem_bottom = height * 0.75;
            builder.move_to(0.0, stem_top as f32);
            builder.line_to((width - head_width) as f32, stem_top as f32);
            builder.line_to((width - head_width) as f32, 0.0);
            builder.line_to(w, h / 2.0);
            builder.line_to((width - head_width) as f32, h);
            builder.line_to((width - head_width) as f32, stem_bottom as f32);
            builder.line_to(0.0, stem_bottom as f32);
            builder.close();
        }
        ShapeKind::Heart => {
            builder.move_to((width * 0.5) as f32, (height * 0.28) as f32);
            builder.cubic_to(
                (width * 0.32) as f32,
                (height * 0.02) as f32,
                0.0,
                (height * 0.12) as f32,
                0.0,
                (height * 0.4) as f32,
            );
            builder.cubic_to(
                0.0,
                (height * 0.65) as f32,
                (width * 0.28) as f32,
                (height * 0.82) as f32,
                (width * 0.5) as f32,
                h,
            );
            builder.cubic_to(
                (width * 0.72) as f32,
                (height * 0.82) as f32,
                w,
                (height * 0.65) as f32,
                w,
                (height * 0.4) as f32,
            );
            builder.cubic_to(
                w,
                (height * 0.12) as f32,
                (width * 0.68) as f32,
                (height * 0.02) as f32,
                (width * 0.5) as f32,
                (height * 0.28) as f32,
            );
            builder.close();
        }
        ShapeKind::Diamond => {
            builder.move_to(w / 2.0, 0.0);
            builder.line_to(w, h / 2.0);
            builder.line_to(w / 2.0, h);
            builder.line_to(0.0, h / 2.0);
            builder.close();
        }
        ShapeKind::SpeechBubble => {
            let body_height = height * 0.8;
            let radius = 12.0_f64.min(body_height / 4.0);
            builder.move_to(radius as f32, 0.0);
            builder.line_to((width - radius) as f32, 0.0);
            builder.quad_to(w, 0.0, w, radius as f32);
            builder.line_to(w, (body_height - radius) as f32);
            builder.quad_to(
                w,
                body_height as f32,
                (width - radius) as f32,
                body_height as f32,
            );
            builder.line_to((width * 0.35) as f32, body_height as f32);
            builder.line_to((width * 0.1) as f32, h);
            builder.line_to((width * 0.2) as f32, body_height as f32);
            builder.line_to(radius as f32, body_height as f32);
            builder.quad_to(0.0, body_height as f32, 0.0, (body_height - radius) as f32);
            builder.line_to(0.0, radius as f32);
            builder.quad_to(0.0, 0.0, radius as f32, 0.0);
            builder.close();
        }
        ShapeKind::Hexagon => {
            builder.move_to((width * 0.25) as f32, 0.0);
            builder.line_to((width * 0.75) as f32, 0.0);
            builder.line_to(w, h / 2.0);
            builder.line_to((width * 0.75) as f32, h);
            builder.line_to((width * 0.25) as f32, h);
            builder.line_to(0.0, h / 2.0);
            builder.close();
        }
        ShapeKind::Line => {
            builder.move_to(0.0, 0.0);
            builder.line_to(w, h);
            if params.arrow_head {
                let angle = height.atan2(width);
                let spread = PI / 6.0;
                let head_length = 12.0_f64.max(stroke_width * 4.0);
                builder.move_to(w, h);
                builder.line_to(
                    (width - head_length * (angle - spread).cos()) as f32,
                    (height - head_length * (angle - spread).sin()) as f32,
                );
                builder.line_to(
                    (width - head_length * (angle + spread).cos()) as f32,
                    (height - head_length * (angle + spread).sin()) as f32,
                );
                builder.close();
            }
        }
    }
    builder.finish()
}

fn push_rounded_rect(builder: &mut PathBuilder, width: f32, height: f32, radius: f32) {
    let radius = radius.min(width.min(height) / 2.0);
    let kappa = 0.552_284_8 * radius;
    builder.move_to(radius, 0.0);
    builder.line_to(width - radius, 0.0);
    builder.cubic_to(
        width - radius + kappa,
        0.0,
        width,
        radius - kappa,
        width,
        radius,
    );
    builder.line_to(width, height - radius);
    builder.cubic_to(
        width,
        height - radius + kappa,
        width - radius + kappa,
        height,
        width - radius,
        height,
    );
    builder.line_to(radius, height);
    builder.cubic_to(
        radius - kappa,
        height,
        0.0,
        height - radius + kappa,
        0.0,
        height - radius,
    );
    builder.line_to(0.0, radius);
    builder.cubic_to(0.0, radius - kappa, radius - kappa, 0.0, radius, 0.0);
    builder.close();
}

#[cfg(test)]
mod tests {
    use super::*;
    use photrez_document::layer_params::{ShapeFill, ShapeStroke};

    fn params(kind: ShapeKind, width: f64, height: f64) -> ShapeParams {
        ShapeParams {
            kind,
            width,
            height,
            radius: 0.0,
            fill: ShapeFill {
                kind: ShapeFillKind::Solid,
                color: "#ff0000".to_owned(),
            },
            stroke: ShapeStroke {
                enabled: false,
                color: "#000000".to_owned(),
                width: 1.0,
            },
            arrow_head: false,
        }
    }

    const SHAPE_GOLDENS: &[(&str, &[u8])] = &[
        (
            "triangle",
            include_bytes!("../tests/goldens/shape/triangle.png"),
        ),
        ("star", include_bytes!("../tests/goldens/shape/star.png")),
        (
            "block_arrow",
            include_bytes!("../tests/goldens/shape/block_arrow.png"),
        ),
        ("heart", include_bytes!("../tests/goldens/shape/heart.png")),
        (
            "diamond",
            include_bytes!("../tests/goldens/shape/diamond.png"),
        ),
        (
            "speech_bubble",
            include_bytes!("../tests/goldens/shape/speech_bubble.png"),
        ),
        (
            "hexagon",
            include_bytes!("../tests/goldens/shape/hexagon.png"),
        ),
        (
            "rect_rounded",
            include_bytes!("../tests/goldens/shape/rect_rounded.png"),
        ),
        ("line", include_bytes!("../tests/goldens/shape/line.png")),
        ("arrow", include_bytes!("../tests/goldens/shape/arrow.png")),
    ];

    fn golden_params(name: &str) -> ShapeParams {
        match name {
            "triangle" => params(ShapeKind::Triangle, 6.0, 6.0),
            "star" => params(ShapeKind::Star, 6.0, 6.0),
            "block_arrow" => params(ShapeKind::BlockArrow, 6.0, 6.0),
            "heart" => params(ShapeKind::Heart, 6.0, 6.0),
            "diamond" => params(ShapeKind::Diamond, 6.0, 6.0),
            "speech_bubble" => params(ShapeKind::SpeechBubble, 6.0, 6.0),
            "hexagon" => params(ShapeKind::Hexagon, 6.0, 6.0),
            "rect_rounded" => {
                let mut shape = params(ShapeKind::Rect, 6.0, 6.0);
                shape.radius = 1.0;
                shape
            }
            "line" => {
                let mut shape = params(ShapeKind::Line, 4.0, 4.0);
                shape.fill.kind = ShapeFillKind::None;
                shape.stroke = ShapeStroke {
                    enabled: true,
                    color: "#000000".to_owned(),
                    width: 1.0,
                };
                shape
            }
            "arrow" => {
                let mut shape = golden_params("line");
                shape.arrow_head = true;
                shape
            }
            _ => unreachable!("unknown shape golden {name}"),
        }
    }

    fn shape_tolerance(name: &str) -> u8 {
        // Measured premultiplied maxima against the reference-compositor goldens. These are
        // AA/stroke differences, not geometry drift; the line is the largest
        // because its 1px diagonal stroke lands on subpixel boundaries.
        match name {
            "triangle" | "hexagon" => 1,
            "rect_rounded" => 7,
            "diamond" => 8,
            "block_arrow" => 10,
            "star" => 17,
            "heart" => 18,
            "speech_bubble" => 26,
            "arrow" => 38,
            "line" => 66,
            _ => 0,
        }
    }

    #[test]
    fn remaining_shape_goldens_match_reference_captures_within_shape_tolerance() {
        for (name, png) in SHAPE_GOLDENS {
            let golden = image::load_from_memory(png)
                .unwrap_or_else(|error| panic!("{name}.png decodes: {error}"))
                .to_rgba8();
            let (golden_width, golden_height) = golden.dimensions();
            let result = rasterize_shape(&golden_params(name)).expect("shape rasterizes");
            assert_eq!(
                (result.width, result.height),
                (golden_width, golden_height),
                "{name} dimensions"
            );
            // Compare premultiplied RGB: transparent pixels have arbitrary RGB,
            // so a straight-channel comparison would report a false 255 at
            // alpha 0/1 boundaries.
            let mut max_delta = 0u8;
            for (actual, expected) in result
                .pixels
                .chunks_exact(4)
                .zip(golden.as_raw().chunks_exact(4))
            {
                max_delta = max_delta.max(actual[3].abs_diff(expected[3]));
                for channel in 0..3 {
                    let actual_premul = u16::from(actual[channel]) * u16::from(actual[3]) / 255;
                    let expected_premul =
                        u16::from(expected[channel]) * u16::from(expected[3]) / 255;
                    max_delta = max_delta.max(actual_premul.abs_diff(expected_premul) as u8);
                }
            }
            let tolerance = shape_tolerance(name);
            assert!(
                max_delta <= tolerance,
                "{name}: premultiplied delta {max_delta} exceeds {tolerance}"
            );
        }
    }

    const RECT_STROKE_GOLDEN: [u8; 144] = [
        0, 0, 0, 0, 0, 0, 0, 127, 0, 0, 0, 127, 0, 0, 0, 127, 0, 0, 0, 127, 0, 0, 0, 0, //
        0, 0, 0, 127, 63, 0, 0, 255, 127, 0, 0, 255, 127, 0, 0, 255, 63, 0, 0, 255, 0, 0, 0,
        128, //
        0, 0, 0, 127, 127, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 128, 0, 0, 255, 0, 0, 0,
        128, //
        0, 0, 0, 127, 127, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 128, 0, 0, 255, 0, 0, 0,
        128, //
        0, 0, 0, 127, 64, 0, 0, 255, 128, 0, 0, 255, 128, 0, 0, 255, 64, 0, 0, 255, 0, 0, 0,
        128, //
        0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 0, 128, 0, 0, 0, 128, 0, 0, 0, 128, 0, 0, 0, 0,
    ];
    const ELLIPSE_FILL_GOLDEN: [u8; 64] = [
        255, 0, 0, 86, 255, 0, 0, 216, 255, 0, 0, 216, 255, 0, 0, 86, //
        255, 0, 0, 216, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 216, //
        255, 0, 0, 216, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 216, //
        255, 0, 0, 86, 255, 0, 0, 216, 255, 0, 0, 216, 255, 0, 0, 86,
    ];

    #[test]
    fn shape_goldens_match_reference_captures_within_documented_tolerance() {
        let mut rect = params(ShapeKind::Rect, 4.0, 4.0);
        rect.stroke = ShapeStroke {
            enabled: true,
            color: "#000000".to_owned(),
            width: 1.0,
        };
        let stroked = rasterize_shape(&rect).expect("stroked rect rasterizes");
        assert_eq!((stroked.width, stroked.height), (6, 6));
        // The reference compositor leaves the four outer bevel corners uncovered, while
        // tiny-skia paints part of them. The measured bound is 36; every other
        // channel is within one unit.
        for (index, (actual, expected)) in stroked
            .pixels
            .chunks_exact(4)
            .zip(RECT_STROKE_GOLDEN.chunks_exact(4))
            .enumerate()
        {
            let tolerance = if matches!(index, 0 | 5 | 30 | 35) {
                36
            } else {
                1
            };
            for (actual, expected) in actual.iter().zip(expected.iter()) {
                assert!(
                    actual.abs_diff(*expected) <= tolerance,
                    "rect pixel {index}: got {actual:?}, expected {expected:?}"
                );
            }
        }

        let ellipse =
            rasterize_shape(&params(ShapeKind::Ellipse, 4.0, 4.0)).expect("ellipse rasterizes");
        assert_eq!((ellipse.width, ellipse.height), (4, 4));
        // The 2x2 centre is exact; the anti-aliased rim differs by up to 13
        // alpha units between the reference compositor's ellipse path and tiny-skia.
        for (index, (actual, expected)) in ellipse
            .pixels
            .chunks_exact(4)
            .zip(ELLIPSE_FILL_GOLDEN.chunks_exact(4))
            .enumerate()
        {
            let tolerance = if matches!(index, 5 | 6 | 9 | 10) {
                0
            } else {
                13
            };
            for (actual, expected) in actual.iter().zip(expected.iter()) {
                assert!(
                    actual.abs_diff(*expected) <= tolerance,
                    "ellipse pixel {index}: got {actual:?}, expected {expected:?}"
                );
            }
        }
    }

    #[test]
    fn filled_rect_is_opaque_red() {
        let shape = rasterize_shape(&params(ShapeKind::Rect, 4.0, 4.0)).expect("rect rasterizes");
        assert_eq!((shape.width, shape.height), (4, 4));
        for pixel in shape.pixels.chunks_exact(4) {
            assert_eq!(pixel, [255, 0, 0, 255]);
        }
    }

    #[test]
    fn stroke_margin_expands_the_bitmap() {
        let mut params = params(ShapeKind::Rect, 4.0, 4.0);
        params.stroke = ShapeStroke {
            enabled: true,
            color: "#000000".to_owned(),
            width: 1.0,
        };
        let shape = rasterize_shape(&params).expect("stroked rect rasterizes");
        assert_eq!(shape_render_margin(&params), 1.0);
        assert_eq!((shape.width, shape.height), (6, 6));
    }

    #[test]
    fn invalid_dimensions_and_colors_are_rejected() {
        assert_eq!(
            rasterize_shape(&params(ShapeKind::Rect, 0.0, 4.0)).unwrap_err(),
            ShapeError::InvalidDimensions
        );
        let mut invalid_color = params(ShapeKind::Rect, 4.0, 4.0);
        invalid_color.fill.color = "not-a-color".to_owned();
        assert_eq!(
            rasterize_shape(&invalid_color).unwrap_err(),
            ShapeError::InvalidColor
        );
    }

    #[test]
    fn hex_colors_accept_short_and_alpha_forms() {
        assert_eq!(parse_hex_color("#f00"), Some([255, 0, 0, 255]));
        assert_eq!(parse_hex_color("#0f08"), Some([0, 255, 0, 136]));
        assert_eq!(parse_hex_color("#11223344"), Some([17, 34, 51, 68]));
        assert_eq!(parse_hex_color("112233"), None);
    }
}
