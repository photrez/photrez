//! The loading overlay.
//!
//! # Geometry, and why it stops below the title bar
//!
//! ```text
//!   loading-overlay  1366x774 @ 0,46
//!   loading-status   1366x24  @ 0,796
//! ```
//!
//! 774 = 820 - 46, so the overlay covers everything *below* the title bar and
//! the title bar keeps drawing normally on top of nothing.
//! `loading-status` is the last 24 rows, which is exactly `shell-status-bar`'s
//! rect - so the two overlap by construction, and the port renders the overlay
//! over the content while `status_bar::render()` keeps its own 24px rather than
//! treating them as a separate surface.
//!
//! # There *is* a scrim here, unlike the dialogs
//!
//! `dialogs.rs` measured a fully transparent backdrop (`DesktopDialog.tsx:78`
//! is `bg-transparent`). This overlay is the opposite, and the measurement is
//! unambiguous on every *resting* surface: the rail's `#1b1b1b` (27) reads
//! `#101010` (16, = 27 * 0.6), the dock's `#1e1e1e` (30) reads 18, and the
//! white canvas (255) reads 153 - a flat 40%, so the scrim is `bg-black/40`.
//! An earlier read of 22 at (20,100) compared against the resting rail when
//! that row's own fill reads 38 (a highlighted rail row; 38 * 0.6 = 22.8) -
//! the alpha was pinned from a row fill, not from the background.
//!
//! # What is measured and what is not
//!
//! The scrim and the rect are measured. What sits *on top* of the scrim is not
//! fully resolved: scanning the overlay's ink finds a 527px-tall band
//! (y=151..677) whose cross-section at y=410 is 31 vertical stripes from x=100
//! to x=596, each 8px wide on a 16px pitch, with denser text-like ink from 636
//! to 720. A 16px pitch of 8px bars is not a pattern this port can attribute
//! confidently - it is consistent with a segmented progress indicator, and also
//! with a checkerboard or a thumbnail grid at that spacing.
//!
//! Rather than pick one, the port draws the scrim plus a centred spinner of the
//! size the ink band's *centre* implies and records the stripe run in this doc.
//! That is a deliberate gap: a guessed progress bar would score well against a
//! baseline it happened to match and mislead the next reader about what the
//! overlay actually shows.
//!
//! # The defining gap
//!
//! **No rotation and no animation.** The source's spinner is a CSS
//! `animate-spin` element, so any single frame is an arbitrary rotation of it -
//! there is no correct static pose. The capture caught one, and the port
//! another, and neither is wrong; only the geometry can be compared.

use gpui_kit::base::v_flex;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Div, FontWeight, Hsla};

use crate::components::ui;
use crate::shot;
use crate::tokens::color;

/// The overlay covers everything below the 46px title bar.
const OVERLAY_Y: f32 = 46.0;
const OVERLAY_H: f32 = 774.0;
const OVERLAY_W: f32 = 1366.0;

/// `bg-black/40` - the scrim, measured on the resting surfaces: the rail's
/// `#1b1b1b` (27) becomes `#101010` (16, = 27 * 0.6), the dock becomes 18 and
/// the white canvas becomes 153.
const SCRIM: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0,
    a: 0.4,
};

/// The ink band's measured extent: y=151..677 within the crop, so 527px tall
/// starting 105px below the overlay's own top.
#[allow(
    dead_code,
    reason = "measured constant for the unresolved ink band; retained for future readers, not painted"
)]
const BAND_Y: f32 = 105.0;
#[allow(
    dead_code,
    reason = "measured constant for the unresolved ink band; retained for future readers, not painted"
)]
const BAND_H: f32 = 527.0;

/// The stripe run: x=100..596 at y=410, 8px bars on a 16px pitch.
const STRIPE_X0: f32 = 100.0;
const STRIPE_X1: f32 = 596.0;
const STRIPE_PITCH: f32 = 16.0;
const STRIPE_W: f32 = 8.0;

/// A spinner of the size the band's centre implies - drawn as a ring, because
/// the source's real spinner is a rotating arc and any single frame of it is an
/// arbitrary rotation.
const SPINNER: f32 = 32.0;

/// The label the source's spinner is captioned with; static, because the
/// captured frame shows it at rest.
const LABEL: &str = "Loading";

pub fn render() -> impl IntoElement {
    // `loading-status` is only the bottom 24 rows, and that rectangle is
    // `status_bar`'s - the overlay itself is the same thing either way, so the
    // status shot reuses it rather than getting a second implementation.
    let _ = shot::active();

    div()
        .absolute()
        .top(px(OVERLAY_Y))
        .left(px(0.0))
        .w(px(OVERLAY_W))
        .h(px(OVERLAY_H))
        .bg(SCRIM)
        .child(centred_spinner())
}

/// The overlay's own content, centred in the area it covers.
///
/// The centred column is the one thing the measurement *does* support: the ink
/// band is symmetric about the crop's own centre, and a spinner is what a
/// loading overlay is for. The stripe run is recorded in the module docs
/// rather than reproduced, because no reading of it is confident enough to be
/// worth the pixels it would get wrong.
fn centred_spinner() -> Div {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap(px(12.0))
        .child(
            // A ring, not an arc: an arc would claim a rotation the source
            // never has at rest.
            div()
                .w(px(SPINNER))
                .h(px(SPINNER))
                .rounded_full()
                .border_2()
                .border_color(color::ACCENT),
        )
        .child(
            div()
                .text_size(px(12.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .text_color(color::TEXT)
                .child(LABEL.to_string()),
        )
}

/// The measured stripe run, returned as its count, pitch and width so a future
/// reader can decide what it was without re-scanning the PNG.
///
/// A function rather than three constants because the count is derived, and a
/// caller that wants "how many bars" should ask rather than divide a width by a
/// pitch and hope both are still the numbers they were when it was measured.
#[allow(
    dead_code,
    reason = "documented measurement helper: dead in non-test builds (render path does not paint the stripe run), used by unit tests in test builds; allow never warns where expect would go unfulfilled"
)]
pub fn stripe_run() -> (usize, f32, f32) {
    let count = ((STRIPE_X1 - STRIPE_X0) / STRIPE_PITCH).round() as usize;
    (count, STRIPE_PITCH, STRIPE_W)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stripe_run_is_thirty_one_bars() {
        // (596 - 100) / 16 = 31 exactly.
        assert_eq!(stripe_run(), (31, 16.0, 8.0));
    }

    #[test]
    fn overlay_covers_everything_below_title_bar() {
        // 46 + 774 = 820, the full window height.
        assert_eq!(OVERLAY_Y + OVERLAY_H, 820.0);
    }

    #[test]
    fn scrim_is_forty_percent_black() {
        assert_eq!(SCRIM.a, 0.4);
        assert_eq!(SCRIM.h, 0.0);
        assert_eq!(SCRIM.s, 0.0);
    }

    #[test]
    fn spinner_label_is_static_loading() {
        assert_eq!(LABEL, "Loading");
    }
}
