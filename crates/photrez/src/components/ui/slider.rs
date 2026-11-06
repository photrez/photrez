use gpui_kit::prelude::*;
use gpui_kit::{
    checkerboard, div, hsla, linear_color_stop, linear_gradient, px, relative, Background,
    BoxShadow, Div, Hsla,
};

use crate::tokens::color;

use super::{SliderKind, ICON_SM};

/// The track fill and thumb for a slider, painted per [`SliderKind`].
///
/// `percent` is the 0..100 fill; `value` is the raw value used by the
/// centre-origin kinds, which sit at 50% when it is 0 and reach the ends only
/// at ±100. Both are passed because they play different roles: `percent`
/// positions the thumb for every kind, while `value` decides *where* the fill
/// starts for the four centre-origin ones.
///
/// `accent: false` swaps the left-aligned fill from the accent to
/// [`color::TEXT_55`], the 55%-alpha text fill.
pub fn slider(percent: f32, value: Option<f32>, kind: SliderKind, accent: bool) -> Div {
    let is_wedge = kind == SliderKind::BrushSize;
    let track_h = if is_wedge { 8.0 } else { 4.0 };

    let mut track = div().absolute().inset(px(0.)).overflow_hidden();

    if !is_wedge {
        track = track.rounded_full();
    }

    // Track background: a gradient/checkerboard for the kinds that specify
    // one, and the field-border colour otherwise.
    match kind {
        SliderKind::Alpha => {
            // A left-to-right fade from full transparency to the accent over
            // an 8px checkerboard. GPUI has
            // no compositing of two fills on one element, so the pattern goes
            // on a child underneath and the accent fade on another above it —
            // same two layers, same order.
            track = track
                .bg(color::FIELD_BORDER)
                .child(
                    div()
                        .absolute()
                        .inset(px(0.))
                        .bg(checkerboard(color::CHECK_DARK, 8.0)),
                )
                .child(div().absolute().inset(px(0.)).bg(linear_gradient(
                    90.0,
                    linear_color_stop(color::ACCENT.opacity(0.0), 0.0),
                    linear_color_stop(color::ACCENT, 1.0),
                )));
        }
        SliderKind::Brightness | SliderKind::Contrast | SliderKind::Saturation => {
            track = track.children(gradient_segments(&kind.track_stops()));
        }
        SliderKind::BrushHardness => {
            track = track.bg(linear_gradient(
                90.0,
                linear_color_stop(color::ACCENT.opacity(0.15), 0.0),
                linear_color_stop(color::ACCENT, 1.0),
            ));
        }
        SliderKind::BrushSize => {
            // The upstream shape is a clip polygon 0% 65%, 100% 15%, 100% 85%,
            // 0% 65% over the field-border colour, with no corner radius, so
            // the track is a sharp-ended bar rather than a pill.
            //
            // GPUI cannot clip a box to a polygon, and this is the one gap in the
            // Slider that is *visible* at rest rather than only under interaction:
            // the brush-size track is a rectangle here, not the wedge that grows
            // to the right. Worth knowing before the brush panel lands: the shape
            // would have to be a filled path, not a clipped box, and `gpui_kit`
            // exposes no such helper today.
            track = track.bg(color::FIELD_BORDER);
        }
        _ => {
            track = track.bg(color::FIELD_BORDER);
        }
    }

    // The fill, centred or left-aligned.
    if kind.is_centre_origin() {
        if let Some(raw) = value {
            let (left, width) = centre_fill(raw);
            let fill: Background = match kind {
                SliderKind::Brightness => color::ZINC_100.into(),
                SliderKind::Contrast => color::ZINC_300.into(),
                SliderKind::Straighten => color::ACCENT.into(),
                // Pink to rose — a two-stop
                // gradient, which `linear_gradient` happens to support
                // natively, so it is not split like the wider track fills are.
                _ => linear_gradient(
                    90.0,
                    linear_color_stop(color::PINK_500, 0.0),
                    linear_color_stop(color::ROSE_500, 1.0),
                ),
            };
            let glow = match kind {
                SliderKind::Brightness => hsla(0.0, 0.0, 1.0, 0.45),
                SliderKind::Contrast => hsla(0.666667, 0.048780, 0.839216, 0.35),
                SliderKind::Straighten => hsla(0.0553, 0.8145, 0.4863, 0.45),
                _ => hsla(0.971455, 0.891626, 0.601961, 0.45),
            };

            track = track.child(
                div()
                    .absolute()
                    .left(relative(left / 100.0))
                    .w(relative(width / 100.0))
                    .top(px(0.))
                    .bottom(px(0.))
                    .rounded_full()
                    .bg(fill)
                    .shadow(vec![
                        BoxShadow::new(px(0.), px(0.), glow).blur_radius(px(8.))
                    ]),
            );
            // The neutral centre tick — a 1px white bar at 45%, centred. The
            // `_PCT` matters: `WHITE_45` is 4.5% and this is 45%.
            track = track.child(
                div()
                    .absolute()
                    .left(relative(0.5))
                    .w(px(1.0))
                    .top(px(0.))
                    .bottom(px(0.0))
                    .bg(color::WHITE_45_PCT),
            );
        }
    } else if kind != SliderKind::Alpha
        && kind != SliderKind::BrushHardness
        && kind != SliderKind::BrushSize
    {
        track = track.child(
            div()
                .absolute()
                .left(px(0.))
                .top(px(0.))
                .bottom(px(0.))
                .w(relative(percent / 100.0))
                .rounded_full()
                .bg(if accent {
                    color::ACCENT
                } else {
                    color::TEXT_55
                }),
        );
    }

    // A 12px dot centred on the fill edge: GPUI has no
    // transform, so the half-box is subtracted as a negative margin instead,
    // which lands the same pixels — the GPUI left setter positions the margin
    // edge for an absolutely positioned child, so a -6px margin shifts it back
    // half its own width.
    let thumb = div()
        .absolute()
        .left(relative(percent / 100.0))
        .top(relative(0.5))
        .ml(px(-(ICON_SM / 2.0)))
        .mt(px(-(ICON_SM / 2.0)))
        .size(px(ICON_SM))
        .rounded_full()
        .border_1()
        .border_color(color::BLACK_55)
        .bg(color::SLIDER_THUMB)
        .shadow(vec![
            BoxShadow::new(px(0.0), px(1.0), hsla(0.0, 0.0, 0.0, 0.65)).blur_radius(px(2.0)),
            BoxShadow::new(px(0.0), px(0.0), hsla(0.0, 0.0, 1.0, 0.18)).spread_radius(px(1.0)),
        ]);

    div()
        .relative()
        .flex_1()
        .h(px(track_h))
        .child(track)
        .child(thumb)
}

impl SliderKind {
    /// `brightness`, `contrast`, `saturation` and `straighten` grow from the
    /// centre rather than the left.
    ///
    /// For those, the fill's left edge sits at 50% at rest and its width is
    /// `|value| / 2` % — so ±100 reaches an end and 0 sits at the tick.
    fn is_centre_origin(self) -> bool {
        matches!(
            self,
            SliderKind::Brightness
                | SliderKind::Contrast
                | SliderKind::Saturation
                | SliderKind::Straighten
        )
    }

    /// The gradient stops of the track, as `(colour, 0..1 position)` pairs.
    ///
    /// The measured gradient stops for this kind, in order.
    fn track_stops(self) -> Vec<(Hsla, f32)> {
        match self {
            SliderKind::Brightness => vec![
                (color::SLIDER_DARK, 0.0),
                (color::SLIDER_MID_BRIGHT, 0.5),
                (color::WHITE, 1.0),
            ],
            SliderKind::Contrast => vec![
                (color::SLIDER_DARK, 0.0),
                (color::SLIDER_MID_CONTRAST, 0.5),
                (color::WHITE, 1.0),
            ],
            SliderKind::Saturation => vec![
                (color::SLIDER_MID_BRIGHT, 0.0),
                (color::SLIDER_MID_CONTRAST, 0.35),
                (color::RED_500, 0.6),
                (color::YELLOW_500, 0.8),
                (color::BLUE_500, 1.0),
            ],
            _ => vec![],
        }
    }
}

/// The GPUI left/width of the centre-origin fill, in percent, for a raw value.
///
/// The rule is piecewise: at or above zero the fill runs from the centre to
/// the right by `value / 2`; below zero it starts at `(value + 100) / 2` and
/// stops at the centre, so the bar is always `|value| / 2` wide and its free
/// end moves with the sign.
pub fn centre_fill(raw: f32) -> (f32, f32) {
    if raw >= 0.0 {
        (50.0, raw / 2.0)
    } else {
        let start = (raw + 100.0) / 2.0;
        (start, 50.0 - start)
    }
}

/// Lay a multi-stop gradient out as one `linear_gradient` per gap between
/// stops.
///
/// GPUI's `linear_gradient` takes exactly **two** stops
/// (`gpui::linear_gradient(angle, from, to)`), where CSS's takes a list. The
/// straightforward fix is one element per segment rather than a cleverer
/// encoding: a segment spanning `p0..p1` is an absolutely positioned box at
/// `p0`% from the left, `(p1 - p0)`% wide, carrying its own two-stop gradient
/// across its own box — the same interpolation CSS would have performed.
///
/// Splitting also sidesteps whether a two-stop gradient clamps outside its
/// stops — each segment's stops sit at its own 0 and 1, so there is nothing
/// to clamp.
pub fn gradient_segments(stops: &[(Hsla, f32)]) -> Vec<Div> {
    stops
        .windows(2)
        .map(|pair| {
            let (c0, p0) = pair[0];
            let (c1, p1) = pair[1];
            div()
                .absolute()
                .left(relative(p0))
                .w(relative(p1 - p0))
                .top(px(0.0))
                .bottom(px(0.0))
                .bg(linear_gradient(
                    90.0, // left to right: 0deg is top, clockwise from there
                    linear_color_stop(c0, 0.0),
                    linear_color_stop(c1, 1.0),
                ))
        })
        .collect()
}
