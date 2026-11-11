//! The loading overlay.
//!
//! The scrim is measured: black at 40% over the resting surfaces, covering
//! everything below the 46px title bar. The owner redesigned the captured
//! content as a compact progress bar: one caption above one track. The overlay
//! renders during a live document open and for `--shot loading-*`. Project
//! loads report per-layer progress; a raster decode stays indeterminate.
//! `None` draws the indeterminate segment rather than a fabricated percentage.

use gpui_kit::base::v_flex;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Div, FontWeight, Hsla};

use crate::components::ui;
use crate::tokens::color;

/// The overlay covers everything below the 46px title bar.
const OVERLAY_Y: f32 = 46.0;

/// Black at 40% - the scrim, measured on the resting surfaces: the rail's
/// `#1b1b1b` (27) becomes `#101010` (16, = 27 * 0.6), the dock becomes 18 and
/// the white canvas becomes 153.
const SCRIM: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0,
    a: 0.4,
};

/// The compact frame: a 280px track under a one-line caption.
const BAR_W: f32 = 280.0;
const BAR_H: f32 = 6.0;
const CAPTION_GAP: f32 = 8.0;

/// The fraction drawn when no caller has real progress to report.
const INDETERMINATE_FRACTION: f32 = 0.35;

pub fn render(progress: Option<f32>) -> impl IntoElement {
    let fill_width = BAR_W * progress_fraction(progress);
    div()
        .absolute()
        .top(px(OVERLAY_Y))
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .occlude()
        .bg(SCRIM)
        .child(centred_progress(fill_width))
}

fn centred_progress(fill_width: f32) -> Div {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap(px(CAPTION_GAP))
        .child(
            div()
                .text_size(px(12.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .text_color(color::TEXT)
                .child(crate::i18n::t("common.loading")),
        )
        .child(
            div()
                .w(px(BAR_W))
                .h(px(BAR_H))
                .rounded_full()
                .bg(color::BORDER)
                .child(
                    div()
                        .w(px(fill_width))
                        .h_full()
                        .rounded_full()
                        .bg(color::ACCENT),
                ),
        )
}

/// `None` means no real progress is known; the bar shows its indeterminate
/// segment instead of inventing a percentage.
fn progress_fraction(progress: Option<f32>) -> f32 {
    match progress {
        None => INDETERMINATE_FRACTION,
        Some(value) if value.is_finite() => value.clamp(0.0, 1.0),
        Some(_) => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determinate_progress_clamps_to_the_track() {
        assert_eq!(progress_fraction(Some(0.0)), 0.0);
        assert_eq!(progress_fraction(Some(0.5)), 0.5);
        assert_eq!(progress_fraction(Some(1.0)), 1.0);
        assert_eq!(progress_fraction(Some(1.7)), 1.0);
        assert_eq!(progress_fraction(Some(-0.2)), 0.0);
    }

    #[test]
    fn missing_progress_draws_the_indeterminate_segment() {
        assert_eq!(progress_fraction(None), INDETERMINATE_FRACTION);
        assert_eq!(progress_fraction(Some(f32::NAN)), 0.0);
    }
}
