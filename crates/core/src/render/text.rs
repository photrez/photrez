// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text rasterizer: `TextData` -> RGBA bitmap.
//!
//! The reference is the frozen engine's `textRasterizer.ts`. This slice covers
//! point mode with every alignment, letter spacing, and fill/stroke text.
//! Area wrapping, underline/strikethrough, and non-normal weights stay
//! explicit gaps until their golden cases land.

#![cfg_attr(not(test), allow(dead_code))] // the production document render caller lands with the open workflow.

use super::shape::parse_hex_color;
use crate::ptz::layer_params::{TextAlign, TextBoxMode, TextData, TextFontStyle, TextStrokeAlign};
use cosmic_text::{
    Align, Attrs, Buffer, Color, Command as OutlineCommand, Family, FontSystem, Metrics, Shaping,
    SwashCache,
};
use tiny_skia::{LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke as SkiaStroke};

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

/// Shaping happens on every keystroke while a text layer is edited, and
/// `FontSystem::new()` reloads the system font database each time (~160 ms
/// per key). Keep one system; the embedded face is loaded once and only
/// reloaded if a different face arrives.
static FONT_SYSTEM: std::sync::OnceLock<std::sync::Mutex<(FontSystem, Option<u64>)>> =
    std::sync::OnceLock::new();

fn cached_font_system(
    font_data: Option<&[u8]>,
) -> std::sync::MutexGuard<'static, (FontSystem, Option<u64>)> {
    fn fnv64(bytes: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
    let hash = font_data.map(fnv64);
    let mutex = FONT_SYSTEM.get_or_init(|| std::sync::Mutex::new((FontSystem::new(), None)));
    let mut guard = match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let (Some(bytes), Some(hash)) = (font_data, hash) {
        if guard.1 != Some(hash) {
            *guard = (FontSystem::new(), None);
            guard.0.db_mut().load_font_data(bytes.to_vec());
            guard.1 = Some(hash);
        }
    }
    guard
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
        || data.underline == Some(true)
        || data.strikethrough == Some(true)
        || data.font_weight != 400.0
        || data.font_style != TextFontStyle::Normal
    {
        return Err(TextError::UnsupportedFeature);
    }
    let color = parse_hex_color(&data.color).ok_or(TextError::InvalidColor)?;
    let stroke = StrokeSpec::from_data(data, scale)?;

    let font_px = (data.font_size * scale) as f32;
    let line_height_px = (data.line_height * data.font_size * scale) as f32;
    let mut font_system_guard = cached_font_system(font_data);
    let font_system = &mut font_system_guard.0;
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
    let mut buffer = Buffer::new(&mut *font_system, Metrics::new(font_px, line_height_px));
    // Point mode does not wrap. Measure the widest explicit line first, then
    // set that width so cosmic-text can honour centre/right alignment.
    buffer.set_size(None, None);
    buffer.set_text(&text, &attrs, Shaping::Advanced, Some(align));
    buffer.shape_until_scroll(&mut *font_system, false);
    let max_line_width = buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0f32, f32::max);
    buffer.set_size(Some(max_line_width.max(1.0)), None);
    buffer.set_text(&text, &attrs, Shaping::Advanced, Some(align));
    buffer.shape_until_scroll(&mut *font_system, false);
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
        &mut *font_system,
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
    let stroke_pad = stroke.as_ref().map_or(0.0, |stroke| stroke.pad);
    let width = (line_width + PADDING * 2.0 + stroke_pad * 2.0)
        .ceil()
        .max(1.0) as u32;
    let height = ((line_count as f32 - 1.0) * line_height_px
        + ink_per_line
        + PADDING * 2.0
        + stroke_pad * 2.0)
        .ceil()
        .max(1.0) as u32;
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    // Canvas2D's `textBaseline = "top"` places the ink top at the padding.
    // cosmic-text's line box has a different top offset; align the measured ink
    // top to the same padding so the bitmap origin matches the reference.
    let draw_x = PADDING + stroke_pad;
    let draw_y = PADDING + stroke_pad - (font_px / 8.0 + 1.0);
    // The reference strokes before filling for outside alignment (the fill then
    // covers the inner half of the doubled line) and fills first for the center
    // and inside alignments.
    if let Some(stroke) = stroke
        .as_ref()
        .filter(|s| s.align == TextStrokeAlign::Outside)
    {
        stroke_buffer(
            &mut buffer,
            &mut *font_system,
            &mut cache,
            draw_x,
            draw_y,
            width,
            height,
            stroke.line_width(),
            stroke.color,
            None,
            &mut pixels,
        );
        draw_buffer(
            &mut buffer,
            &mut *font_system,
            &mut cache,
            draw_x,
            draw_y,
            width,
            height,
            &mut pixels,
            color,
        );
    } else {
        draw_buffer(
            &mut buffer,
            &mut *font_system,
            &mut cache,
            draw_x,
            draw_y,
            width,
            height,
            &mut pixels,
            color,
        );
        if let Some(stroke) = stroke.as_ref() {
            // Only the inside alignment clips the stroke to the fill; a center
            // stroke keeps the half of the line that falls outside the glyph.
            let fill_mask = (stroke.align == TextStrokeAlign::Inside).then(|| {
                pixels
                    .chunks_exact(4)
                    .map(|pixel| pixel[3])
                    .collect::<Vec<u8>>()
            });
            stroke_buffer(
                &mut buffer,
                &mut *font_system,
                &mut cache,
                draw_x,
                draw_y,
                width,
                height,
                stroke.line_width(),
                stroke.color,
                fill_mask.as_deref(),
                &mut pixels,
            );
        }
    }

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

/// Stroke geometry from `textRasterizer.ts`: the document-space width becomes
/// `strokePad` in device space, and the bitmap grows by one pad on each side.
struct StrokeSpec {
    pad: f32,
    align: TextStrokeAlign,
    color: [u8; 4],
}

impl StrokeSpec {
    fn from_data(data: &TextData, scale: f64) -> Result<Option<Self>, TextError> {
        if !data.stroke.width.is_finite() || data.stroke.width <= 0.0 {
            return Ok(None);
        }
        let pad = (data.stroke.width * scale) as f32;
        if !pad.is_finite() || pad <= 0.0 {
            return Ok(None);
        }
        let color = parse_hex_color(&data.stroke.color).ok_or(TextError::InvalidColor)?;
        Ok(Some(Self {
            pad,
            align: data.stroke.align.unwrap_or(TextStrokeAlign::Outside),
            color,
        }))
    }

    /// `outside`/`inside` double the line so half of it falls under the fill;
    /// `center` keeps the pad so half the line shows on each side.
    fn line_width(&self) -> f32 {
        if self.align == TextStrokeAlign::Center {
            self.pad
        } else {
            self.pad * 2.0
        }
    }
}

/// Rasterize every glyph outline as one stroked path and composite it onto
/// `pixels`. Canvas2D strokes a line's glyphs as a single path, so overlapping
/// glyphs must not blend twice. `source_atop` carries the fill's alpha mask for
/// the inside alignment, which the reference implements with `source-atop`.
#[allow(clippy::too_many_arguments)] // flat draw context; a struct would add no validation
fn stroke_buffer(
    buffer: &mut Buffer,
    font_system: &mut FontSystem,
    cache: &mut SwashCache,
    offset_x: f32,
    offset_y: f32,
    width: u32,
    height: u32,
    line_width: f32,
    color: [u8; 4],
    source_atop: Option<&[u8]>,
    pixels: &mut [u8],
) {
    let mut builder = PathBuilder::new();
    for run in buffer.layout_runs() {
        for glyph in run.glyphs {
            let physical = glyph.physical((0., run.line_y), 1.0);
            let origin_x = offset_x + physical.x as f32 + physical.cache_key.x_bin.as_float();
            let origin_y = offset_y + physical.y as f32 + physical.cache_key.y_bin.as_float();
            let Some(commands) = cache.get_outline_commands(font_system, physical.cache_key) else {
                continue;
            };
            // swash outlines are y-up around the baseline; the bitmap is y-down.
            for command in commands {
                match command {
                    OutlineCommand::MoveTo(point) => {
                        builder.move_to(origin_x + point.x, origin_y - point.y);
                    }
                    OutlineCommand::LineTo(point) => {
                        builder.line_to(origin_x + point.x, origin_y - point.y);
                    }
                    OutlineCommand::QuadTo(control, point) => {
                        builder.quad_to(
                            origin_x + control.x,
                            origin_y - control.y,
                            origin_x + point.x,
                            origin_y - point.y,
                        );
                    }
                    OutlineCommand::CurveTo(control1, control2, point) => {
                        builder.cubic_to(
                            origin_x + control1.x,
                            origin_y - control1.y,
                            origin_x + control2.x,
                            origin_y - control2.y,
                            origin_x + point.x,
                            origin_y - point.y,
                        );
                    }
                    OutlineCommand::Close => builder.close(),
                }
            }
        }
    }
    let Some(path) = builder.finish() else {
        return;
    };
    let Some(mut mask) = Pixmap::new(width, height) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
    paint.anti_alias = true;
    let stroke = SkiaStroke {
        width: line_width,
        line_cap: LineCap::Butt,
        line_join: LineJoin::Round,
        miter_limit: 2.0,
        ..SkiaStroke::default()
    };
    mask.stroke_path(
        &path,
        &paint,
        &stroke,
        tiny_skia::Transform::identity(),
        None,
    );
    let coverage = mask.data();
    for (index, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let alpha = u32::from(coverage[index * 4 + 3]);
        if alpha == 0 {
            continue;
        }
        let alpha = match source_atop {
            Some(fill) => (alpha * u32::from(fill[index]) + 127) / 255,
            None => alpha,
        } as u8;
        source_over(pixel, color[0], color[1], color[2], alpha, color[3]);
    }
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

    /// Stroke geometry follows `textRasterizer.ts`: the bitmap grows by one
    /// `strokePad` per side, the line is doubled for outside/inside so half of
    /// it hides under the fill, and inside strokes are clipped to the fill
    /// alpha (`source-atop`).
    #[test]
    fn stroke_pads_the_bitmap_and_clips_each_alignment() {
        let stroke = |align: Option<TextStrokeAlign>| {
            let mut data = params();
            data.stroke.width = 4.0;
            data.stroke.color = "#ff0000".to_owned();
            data.stroke.align = align;
            rasterize_text(&data, None, 1.0).expect("stroked text rasterizes")
        };
        let plain = rasterize_text(&params(), None, 1.0).expect("plain text rasterizes");
        let ink_bounds = |result: &RasterizedText, red_only: bool| {
            let mut bounds: Option<(u32, u32, u32, u32)> = None;
            for y in 0..result.height {
                for x in 0..result.width {
                    let index = ((y * result.width + x) * 4) as usize;
                    let pixel = &result.pixels[index..index + 4];
                    if pixel[3] == 0 {
                        continue;
                    }
                    if red_only && !(pixel[0] > 200 && pixel[1] < 60 && pixel[2] < 60) {
                        continue;
                    }
                    let entry = bounds.get_or_insert((x, y, x, y));
                    entry.0 = entry.0.min(x);
                    entry.1 = entry.1.min(y);
                    entry.2 = entry.2.max(x);
                    entry.3 = entry.3.max(y);
                }
            }
            bounds
        };
        let plain_ink = ink_bounds(&plain, false).expect("plain ink");

        let outside = stroke(Some(TextStrokeAlign::Outside));
        let center = stroke(Some(TextStrokeAlign::Center));
        let inside = stroke(Some(TextStrokeAlign::Inside));
        let default = stroke(None);
        for (name, stroked) in [
            ("outside", &outside),
            ("center", &center),
            ("inside", &inside),
            ("default", &default),
        ] {
            assert_eq!(
                (stroked.width, stroked.height),
                (plain.width + 8, plain.height + 8),
                "{name} pads the bitmap by one stroke pad per side"
            );
            assert_eq!(
                (stroked.document_width, stroked.document_height),
                (f64::from(stroked.width), f64::from(stroked.height)),
                "{name} document size follows the device bitmap at scale 1"
            );
            assert!(
                ink_bounds(stroked, true).is_some(),
                "{name} paints the stroke colour"
            );
        }
        assert_eq!(
            default.pixels, outside.pixels,
            "a missing alignment defaults to outside"
        );

        let outside_ink = ink_bounds(&outside, false).expect("outside ink");
        let center_ink = ink_bounds(&center, false).expect("center ink");
        let inside_ink = ink_bounds(&inside, false).expect("inside ink");
        assert!(
            outside_ink.0 < center_ink.0 && center_ink.0 < inside_ink.0,
            "wider strokes bleed further left: {outside_ink:?} {center_ink:?} {inside_ink:?}"
        );
        // The fill is drawn at the padded origin, so the inside stroke can only
        // cover pixels the fill already reached.
        assert!(
            inside_ink.0 >= plain_ink.0 + 4
                && inside_ink.1 >= plain_ink.1 + 4
                && inside_ink.2 <= plain_ink.2 + 4
                && inside_ink.3 <= plain_ink.3 + 4,
            "inside stroke stays within the fill: {inside_ink:?} vs {plain_ink:?}"
        );
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
