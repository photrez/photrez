//! Mounted contracts for the colour picker's channel column: the numbers on
//! screen are derived from the colour the picker actually holds, in both
//! models, and every surface re-derives them instead of leaving the previous
//! colour's text behind.
//!
//! The picker lists hue/saturation/brightness and red/green/blue at the same
//! time, so these tests drive the live session and read the column through the
//! view's own derivation. The colours are chosen so the two models print
//! different numbers: a pure hue, a mid grey and a low-saturation colour.
//!
//! These tests fail when the column stops describing the live colour. Each was
//! proved by mutating the code it guards, running, and restoring that file
//! byte-identically. Assertions are named by their message, because line numbers
//! move as the file grows:
//! * printing the saturation percentage from `hsv.v` and the brightness percentage
//!   from `hsv.s` leaves the captions and the pure-hue case alone - both are 100
//!   there - and swaps the pair in the grey and low-saturation cases. That fails
//!   `for_each_live_colour_the_channel_fields_print_both_models` at
//!   "the column prints {typed} in both models", and
//!   `a_hue_strip_drag_rederives_both_models_and_the_hex_field` at both
//!   "the column opens on the paint colour, rounding its angle and percentages"
//!   and "the strip re-derives the angle and all three channels";
//! * deriving the column from `session.original`, the colour the picker opened on,
//!   instead of the live one passes the opening assertion and fails
//!   `a_hue_strip_drag_rederives_both_models_and_the_hex_field` at "the strip
//!   re-derives the angle and all three channels": after the strip has moved the
//!   colour, the column still prints the one it started from;
//! * listing the blue channel first in the group fails both tests' value lists at
//!   those same assertions, and the caption order in the picker's own unit tests;
//! * dropping the hex sync a strip click performs fails
//!   `a_hue_strip_drag_rederives_both_models_and_the_hex_field` at
//!   "the hex field follows the strip" with `#E15A17` where `#17E1E1` is expected.
//!
//! The hex field is state the panel holds, so it is read from that state; the
//! channel column is derived, and the one function that derives it is
//! `crate::views::color_picker::channel_readouts`, whose only argument is the live
//! session the panel renders from. Reading the column through that function is the
//! closest a headless window can come to reading the numbers off the screen.

use super::*;
use crate::views::color_picker::channel_readouts;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{px, Point};

/// The six numbers the open picker prints, in display order.
fn printed_channels(content: &Entity<App>, cx: &gpui_kit::App) -> Vec<String> {
    let session = content
        .read(cx)
        .state_for_test()
        .read(cx)
        .color_picker
        .clone()
        .expect("the picker stays open");
    channel_readouts(&session)
        .iter()
        .map(|readout| readout.value.clone())
        .collect()
}

fn printed_hex(content: &Entity<App>, cx: &gpui_kit::App) -> String {
    content
        .read(cx)
        .state_for_test()
        .read(cx)
        .color_picker
        .clone()
        .expect("the picker stays open")
        .hex
}

/// Commit one colour through the picker, then reopen it and read the column
/// back: what the user typed, what the tools paint, and what the fields print
/// must be one colour.
#[gpui_kit::test]
fn for_each_live_colour_the_channel_fields_print_both_models(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    // A pure hue, a mid grey and a low-saturation colour: hue angle,
    // saturation, brightness, then red, green, blue.
    const CASES: [(&str, [u8; 4], [&str; 6]); 3] = [
        (
            "00FF00",
            [0, 255, 0, 255],
            ["120", "100", "100", "0", "255", "0"],
        ),
        (
            "808080",
            [128, 128, 128, 255],
            ["0", "0", "50", "128", "128", "128"],
        ),
        (
            "B0A8A0",
            [176, 168, 160, 255],
            ["30", "9", "69", "176", "168", "160"],
        ),
    ];

    cx.update_window(window.into(), |_, window, cx| {
        for (typed, colour, expected) in CASES {
            window.click("fg-swatch", cx);
            window.render_frame(cx);
            window.input(typed, cx);
            window.press("enter", cx);
            window.render_frame(cx);
            assert!(
                window.try_find("color-picker").is_none(),
                "{typed} commits and closes the picker"
            );
            content.update(cx, |app, cx| {
                assert_eq!(
                    app.state_for_test().read(cx).fg_color,
                    colour,
                    "{typed} becomes the paint colour"
                );
            });

            window.click("fg-swatch", cx);
            window.render_frame(cx);
            assert_eq!(
                printed_channels(&content, cx),
                expected.to_vec(),
                "the column prints {typed} in both models"
            );
            assert_eq!(
                printed_hex(&content, cx),
                format!("#{:02X}{:02X}{:02X}", colour[0], colour[1], colour[2]),
                "the hex field and the channel column describe {colour:?}"
            );

            window.press("escape", cx);
            window.render_frame(cx);
        }
    })
    .expect("test window remains open");
}

/// Half a turn on the hue strip and a drag in the saturation/brightness box
/// each re-derive both models and the hex field; only the fields the touched
/// surface leaves alone keep their text.
#[gpui_kit::test]
fn a_hue_strip_drag_rederives_both_models_and_the_hex_field(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        window.click("fg-swatch", cx);
        window.render_frame(cx);
        assert_eq!(
            printed_channels(&content, cx),
            ["20", "90", "88", "225", "90", "23"].to_vec(),
            "the column opens on the paint colour, rounding its angle and percentages"
        );

        // The strip is 18x200, so its mid-point is half a colour wheel.
        window.click_at(
            "color-picker-hue",
            Point {
                x: px(9.0),
                y: px(100.0),
            },
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            printed_channels(&content, cx),
            ["180", "90", "88", "23", "225", "225"].to_vec(),
            "the strip re-derives the angle and all three channels"
        );
        assert_eq!(
            printed_hex(&content, cx),
            "#17E1E1",
            "the hex field follows the strip"
        );
        content.update(cx, |app, cx| {
            assert_eq!(
                app.state_for_test().read(cx).fg_color,
                [23, 225, 225, 255],
                "the paint colour matches the printed channels"
            );
        });

        // The box mid-point is half saturation and half brightness.
        window.click_at(
            "color-picker-sv",
            Point {
                x: px(100.0),
                y: px(100.0),
            },
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            printed_channels(&content, cx),
            ["180", "50", "50", "64", "128", "128"].to_vec(),
            "the box keeps the angle and re-derives everything else"
        );
        assert_eq!(
            printed_hex(&content, cx),
            "#408080",
            "the hex field follows the box"
        );
        assert!(
            window.find("color-picker").visible(),
            "both surfaces stay inside one open picker"
        );
    })
    .expect("test window remains open");
}
