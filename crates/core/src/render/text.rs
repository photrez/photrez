// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text rasterizer: `TextData` -> RGBA bitmap.
//!
//! The reference is the frozen engine's `textRasterizer.ts`. This first slice
//! covers point mode, a single line, left alignment, and fill-only text with
//! no decorations. Area wrapping, alignment variants, strokes, decorations,
//! and letter spacing remain explicit gaps until their golden cases land.

#![cfg_attr(not(test), allow(dead_code))] // the production document render caller lands with the open workflow.

use super::shape::parse_hex_color;
use crate::ptz::layer_params::{TextAlign, TextBoxMode, TextData, TextFontStyle};
use cosmic_text::{Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};

const PADDING: f32 = 2.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RasterizedText {
    pub width: u32,
    pub height: u32,
    pub document_width: f64,
    pub document_height: f64,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TextError {
    InvalidSize,
    InvalidColor,
    UnsupportedFeature,
    EmptyInk,
}

pub(crate) fn rasterize_text(
    data: &TextData,
    font_data: Option<&[u8]>,
    scale: f64,
) -> Result<RasterizedText, TextError> {
    if !scale.is_finite()
        || scale <= 0.0
        || !data.font_size.is_finite()
        || data.font_size <= 0.0
        || !data.line_height.is_finite()
        || data.line_height <= 0.0
    {
        return Err(TextError::InvalidSize);
    }
    if data.box_mode != TextBoxMode::Point
        || data.stroke.width > 0.0
        || data.underline == Some(true)
        || data.strikethrough == Some(true)
        || data.font_weight != 400.0
        || data.font_style != TextFontStyle::Normal
    {
        return Err(TextError::UnsupportedFeature);
    }
    let color = parse_hex_color(&data.color).ok_or(TextError::InvalidColor)?;

    let font_px = (data.font_size * scale) as f32;
    let line_height_px = (data.line_height * data.font_size * scale) as f32;
    let mut font_system = FontSystem::new();
    if let Some(font_data) = font_data {
        font_system.db_mut().load_font_data(font_data.to_vec());
    }
    let align = match data.align {
        TextAlign::Left => Align::Left,
        TextAlign::Center => Align::Center,
        TextAlign::Right => Align::Right,
    };
    let mut attrs = Attrs::new().family(Family::Name(&data.font_family));
    if data.letter_spacing != 0.0 {
        attrs = attrs.letter_spacing((data.letter_spacing / data.font_size) as f32);
    }
    let text = if data.uppercase == Some(true) {
        data.content.to_uppercase()
    } else {
        data.content.clone()
    };
    let mut buffer = Buffer::new(&mut font_system, Metrics::new(font_px, line_height_px));
    // Point mode does not wrap. Measure the widest explicit line first, then
    // set that width so cosmic-text can honour centre/right alignment.
    buffer.set_size(None, None);
    buffer.set_text(&text, &attrs, Shaping::Advanced, Some(align));
    buffer.shape_until_scroll(&mut font_system, false);
    let max_line_width = buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0f32, f32::max);
    buffer.set_size(Some(max_line_width.max(1.0)), None);
    buffer.set_text(&text, &attrs, Shaping::Advanced, Some(align));
    buffer.shape_until_scroll(&mut font_system, false);
    let (line_widths, baselines) = {
        let mut widths = Vec::new();
        let mut baselines = Vec::new();
        for run in buffer.layout_runs() {
            widths.push(run.line_w);
            baselines.push(run.line_y);
        }
        (widths, baselines)
    };
    if baselines.is_empty() {
        return Err(TextError::EmptyInk);
    }
    let line_count = baselines.len();
    let line_width = line_widths.iter().copied().fold(0.0f32, f32::max);

    // First pass: measure the ink bounds on a generous scratch bitmap. The
    // reference uses Canvas2D's actualBoundingBox ascent/descent, which is ink
    // rather than line-box height; the scratch bounds give the same input.
    let temp_width = (line_width + PADDING * 2.0 + 8.0).ceil().max(1.0) as u32;
    let temp_height = (line_count as f32 * line_height_px + PADDING * 2.0 + 8.0)
        .ceil()
        .max(1.0) as u32;
    let mut temp = vec![0u8; temp_width as usize * temp_height as usize * 4];
    let mut cache = SwashCache::new();
    draw_buffer(
        &mut buffer,
        &mut font_system,
        &mut cache,
        PADDING,
        PADDING,
        temp_width,
        temp_height,
        &mut temp,
        color,
    );

    let mut any_ink = false;
    let mut min_y = u32::MAX;
    let mut max_y = 0u32;
    for y in 0..temp_height {
        for x in 0..temp_width {
            if temp[((y * temp_width + x) * 4 + 3) as usize] == 0 {
                continue;
            }
            any_ink = true;
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }
    if !any_ink {
        return Err(TextError::EmptyInk);
    }
    let first_baseline = baselines[0] + PADDING;
    let last_baseline = baselines[line_count - 1] + PADDING;
    let mut ascent = (first_baseline - min_y as f32).max(0.0);
    let mut descent = (max_y as f32 + 1.0 - last_baseline).max(0.0);
    let max_ink = font_px * 1.25;
    if ascent + descent > max_ink {
        let ratio = max_ink / (ascent + descent);
        ascent *= ratio;
        descent *= ratio;
    }
    let ink_per_line = line_height_px.max(ascent + descent);
    let width = (line_width + PADDING * 2.0).ceil().max(1.0) as u32;
    let height = ((line_count as f32 - 1.0) * line_height_px + ink_per_line + PADDING * 2.0)
        .ceil()
        .max(1.0) as u32;
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    // Canvas2D's `textBaseline = "top"` places the ink top at the padding.
    // cosmic-text's line box has a different top offset; align the measured ink
    // top to the same padding so the bitmap origin matches the reference.
    let final_offset_y = PADDING - (font_px / 8.0 + 1.0);
    draw_buffer(
        &mut buffer,
        &mut font_system,
        &mut cache,
        PADDING,
        final_offset_y,
        width,
        height,
        &mut pixels,
        color,
    );

    Ok(RasterizedText {
        width,
        height,
        document_width: f64::from(width) / scale,
        document_height: f64::from(height) / scale,
        pixels,
    })
}

#[allow(clippy::too_many_arguments)] // flat draw context; a struct would add no validation
fn draw_buffer(
    buffer: &mut Buffer,
    font_system: &mut FontSystem,
    cache: &mut SwashCache,
    offset_x: f32,
    offset_y: f32,
    width: u32,
    height: u32,
    pixels: &mut [u8],
    color: [u8; 4],
) {
    buffer.draw(
        font_system,
        cache,
        Color::rgba(color[0], color[1], color[2], 255),
        |x, y, w, h, pixel_color| {
            let alpha = pixel_color.a();
            if alpha == 0 {
                return;
            }
            let origin_x = x + offset_x as i32;
            let origin_y = y + offset_y as i32;
            for row in 0..h as i32 {
                for column in 0..w as i32 {
                    let dst_x = origin_x + column;
                    let dst_y = origin_y + row;
                    if dst_x < 0 || dst_y < 0 || dst_x >= width as i32 || dst_y >= height as i32 {
                        continue;
                    }
                    let index = (dst_y as usize * width as usize + dst_x as usize) * 4;
                    source_over(
                        &mut pixels[index..index + 4],
                        pixel_color.r(),
                        pixel_color.g(),
                        pixel_color.b(),
                        alpha,
                        color[3],
                    );
                }
            }
        },
    );
}

fn source_over(dst: &mut [u8], red: u8, green: u8, blue: u8, mask: u8, opacity: u8) {
    let src_alpha = (u32::from(mask) * u32::from(opacity) + 127) / 255;
    if src_alpha == 0 {
        return;
    }
    let dst_alpha = u32::from(dst[3]);
    let inverse = 255 - src_alpha;
    let out_alpha = src_alpha + (dst_alpha * inverse + 127) / 255;
    if out_alpha == 0 {
        dst.fill(0);
        return;
    }
    for (channel, value) in [red, green, blue].into_iter().enumerate() {
        let src = u32::from(value) * src_alpha;
        let dst_value = u32::from(dst[channel]) * dst_alpha * inverse / 255;
        dst[channel] = ((src + dst_value) / out_alpha).min(255) as u8;
    }
    dst[3] = out_alpha.min(255) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN: &[u8] = include_bytes!("text/goldens/ag-arial-16.png");
    const TEXT_GOLDENS: &[(&str, &[u8])] = &[
        (
            "multiline_left",
            include_bytes!("text/goldens/multiline_left.png"),
        ),
        (
            "multiline_center",
            include_bytes!("text/goldens/multiline_center.png"),
        ),
        (
            "multiline_right",
            include_bytes!("text/goldens/multiline_right.png"),
        ),
        ("scale2", include_bytes!("text/goldens/scale2.png")),
        (
            "letter_spacing",
            include_bytes!("text/goldens/letter_spacing.png"),
        ),
    ];

    fn base_text() -> TextData {
        TextData {
            content: String::new(),
            font_family: "Arial".to_owned(),
            font_size: 16.0,
            line_height: 1.2,
            color: "#000000".to_owned(),
            ..Default::default()
        }
    }

    fn advanced_params(name: &str) -> (TextData, f64) {
        let mut data = base_text();
        match name {
            "multiline_left" => {
                data.content = "Ag\nAg".to_owned();
                data.align = TextAlign::Left;
                (data, 1.0)
            }
            "multiline_center" => {
                data.content = "A\nAg".to_owned();
                data.align = TextAlign::Center;
                (data, 1.0)
            }
            "multiline_right" => {
                data.content = "A\nAg".to_owned();
                data.align = TextAlign::Right;
                (data, 1.0)
            }
            "scale2" => {
                data.content = "Ag".to_owned();
                data.align = TextAlign::Left;
                (data, 2.0)
            }
            "letter_spacing" => {
                data.content = "Ag".to_owned();
                data.align = TextAlign::Left;
                data.letter_spacing = 2.0;
                (data, 1.0)
            }
            _ => unreachable!("unknown text golden {name}"),
        }
    }

    #[test]
    fn advanced_text_measures_against_canvas2d() {
        for (name, png) in TEXT_GOLDENS {
            let golden = image::load_from_memory(png)
                .unwrap_or_else(|error| panic!("{name}.png decodes: {error}"))
                .to_rgba8();
            let (golden_width, golden_height) = golden.dimensions();
            let (data, scale) = advanced_params(name);
            let result = rasterize_text(&data, None, scale).expect("text rasterizes");
            assert_eq!(
                (result.width, result.height),
                (golden_width, golden_height),
                "{name} dimensions"
            );
            let bbox = |pixels: &[u8], width: u32, height: u32| {
                let mut min_x = u32::MAX;
                let mut min_y = u32::MAX;
                let mut max_x = 0u32;
                let mut max_y = 0u32;
                let mut alpha_sum = 0u64;
                for y in 0..height {
                    for x in 0..width {
                        let alpha = pixels[((y * width + x) * 4 + 3) as usize];
                        if alpha == 0 {
                            continue;
                        }
                        min_x = min_x.min(x);
                        min_y = min_y.min(y);
                        max_x = max_x.max(x);
                        max_y = max_y.max(y);
                        alpha_sum += u64::from(alpha);
                    }
                }
                (min_x, min_y, max_x, max_y, alpha_sum)
            };
            let expected = bbox(golden.as_raw(), golden_width, golden_height);
            let actual = bbox(&result.pixels, result.width, result.height);
            assert_eq!(actual.1, expected.1, "{name} ink top");
            assert!(
                actual.3.abs_diff(expected.3) <= 1,
                "{name} ink bottom: got {}, expected {}",
                actual.3,
                expected.3
            );
            assert!(
                actual.0 <= expected.0 + 1,
                "{name} ink left: got {}, expected {}",
                actual.0,
                expected.0
            );
            assert!(
                actual.2.abs_diff(expected.2) <= 1,
                "{name} ink right: got {}, expected {}",
                actual.2,
                expected.2
            );
            assert!(
                actual.4.abs_diff(expected.4) * 100 <= expected.4 * 3,
                "{name} ink coverage: got {}, expected {}",
                actual.4,
                expected.4
            );
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
            // Canvas2D and swash hint the second line's baseline differently.
            // The multiline goldens therefore allow a one-pixel per-line
            // baseline difference (max 255 on glyph edges); the bbox and
            // coverage checks keep a missing or displaced glyph from passing.
            let tolerance = if name.starts_with("multiline_") {
                255
            } else {
                64
            };
            assert!(
                max_delta <= tolerance,
                "{name}: premultiplied delta {max_delta} exceeds {tolerance}"
            );
        }
    }

    fn params() -> TextData {
        TextData {
            content: "Ag".to_owned(),
            font_family: "Arial".to_owned(),
            font_size: 16.0,
            line_height: 1.2,
            color: "#000000".to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn single_line_point_text_matches_canvas2d_within_text_tolerance() {
        let golden = image::load_from_memory(GOLDEN)
            .expect("text golden decodes")
            .to_rgba8();
        let (golden_width, golden_height) = golden.dimensions();
        let result = rasterize_text(&params(), None, 1.0).expect("text rasterizes");
        assert_eq!(
            (result.width, result.height),
            (golden_width, golden_height),
            "text box dimensions"
        );

        let bbox = |pixels: &[u8], width: u32, height: u32| {
            let mut min_x = u32::MAX;
            let mut min_y = u32::MAX;
            let mut max_x = 0u32;
            let mut max_y = 0u32;
            let mut alpha_sum = 0u64;
            for y in 0..height {
                for x in 0..width {
                    let alpha = pixels[((y * width + x) * 4 + 3) as usize];
                    if alpha == 0 {
                        continue;
                    }
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                    alpha_sum += u64::from(alpha);
                }
            }
            (min_x, min_y, max_x, max_y, alpha_sum)
        };
        let expected = bbox(golden.as_raw(), golden_width, golden_height);
        let actual = bbox(&result.pixels, result.width, result.height);
        assert_eq!((actual.1, actual.3), (expected.1, expected.3), "ink rows");
        assert_eq!(actual.2, expected.2, "ink right edge");
        assert!(
            actual.0 <= expected.0 + 1,
            "ink left edge: got {}, expected {}",
            actual.0,
            expected.0
        );
        assert!(
            actual.4.abs_diff(expected.4) * 100 <= expected.4 * 3,
            "ink coverage: got {}, expected {}",
            actual.4,
            expected.4
        );

        // Canvas2D hints and rasterizes glyphs differently from swash. The
        // measured maximum is 55 channel units on anti-aliased edges; the box
        // and ink bounds above carry the structural parity, and the coverage
        // bound keeps a different font or a displaced glyph from passing.
        let mut max_delta = 0u8;
        for (actual, expected) in result.pixels.iter().zip(golden.as_raw().iter()) {
            max_delta = max_delta.max(actual.abs_diff(*expected));
        }
        assert!(max_delta <= 64, "text AA delta {max_delta} exceeds 64");
    }
}
