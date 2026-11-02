//! The colour values the picker's channel column prints.
//!
//! There is no channel-model switch to click: the picker lists hue, saturation and
//! brightness above red, green and blue at the same time (see `column` above these
//! tests), so a click cannot move one model's numbers into the other's fields. What
//! can go wrong is the number itself - a caption bound to the wrong channel, a value
//! read from the colour the picker opened on instead of the live one, or truncation
//! where the readout says it rounds - and that is what these tests pin, for colours
//! chosen so the two models disagree unless both are correct.
//!
//! These tests fail when the derivation in `channel_readouts` is wrong. Proven by
//! temporarily: swapping the saturation line and the brightness line, which leaves
//! the captions and the full-saturation pure hue untouched - both are 100 there -
//! and breaks the mid grey, the low-saturation colour, the rounding case, the
//! cross-model sweep and both mounted tests; listing the blue channel first in the
//! group, which breaks the caption order and every value list; and deriving from the
//! colour the picker opened on instead of the live one, which prints 9/9/9 here and
//! leaves the mounted hue drag showing the colour the picker started from.

use super::{channel_readouts, Readout};
use crate::palette::{hsv_to_rgb, rgb_to_hsv, to_hex, Hsv};
use crate::state::{ColorPickerSession, ColorTarget};

/// The channel column exactly as the picker renders it for a live colour. The
/// original colour and the hex buffer deliberately name different colours from the
/// live one, so a field that read either of them instead of the live colour cannot
/// pass unnoticed.
fn fields(hsv: Hsv) -> [Readout; 6] {
    channel_readouts(&ColorPickerSession {
        target: ColorTarget::Foreground,
        hsv,
        original: [9, 9, 9, 255],
        hex: "#123456".to_string(),
    })
}

fn labels(readouts: &[Readout]) -> Vec<&'static str> {
    readouts.iter().map(|readout| readout.label).collect()
}

fn values(readouts: &[Readout]) -> Vec<&str> {
    readouts
        .iter()
        .map(|readout| readout.value.as_str())
        .collect()
}

fn units(readouts: &[Readout]) -> Vec<&'static str> {
    readouts.iter().map(|readout| readout.suffix).collect()
}

/// A printed channel as the byte it claims to be.
fn printed_byte(text: &str) -> u8 {
    text.parse().unwrap_or_else(|error| {
        panic!("printed channel {text:?} is not a byte: {error}");
    })
}

#[test]
fn both_models_are_listed_in_display_order() {
    let readouts = fields(Hsv {
        h: 0.0,
        s: 1.0,
        v: 1.0,
    });
    assert_eq!(labels(&readouts), ["H:", "S:", "B:", "R:", "G:", "B:"]);
    assert_eq!(units(&readouts), ["°", "%", "%", "", "", ""]);
}

#[test]
fn a_pure_hue_prints_the_angle_and_the_channels() {
    // Full-saturation green: hue 120 degrees, no grey, and the red channel empty.
    let readouts = fields(Hsv {
        h: 120.0,
        s: 1.0,
        v: 1.0,
    });
    assert_eq!(values(&readouts), ["120", "100", "100", "0", "255", "0"]);
}

#[test]
fn a_mid_grey_prints_no_saturation_but_keeps_its_brightness() {
    // 128/128/128. Its hue is undefined, so the only defensible reading is the
    // achromatic one: zero saturation and a brightness equal to the grey level.
    let readouts = fields(rgb_to_hsv([128, 128, 128, 255]));
    assert_eq!(values(&readouts), ["0", "0", "50", "128", "128", "128"]);
}

#[test]
fn a_low_saturation_colour_prints_a_small_saturation() {
    // Warm putty at 176/168/160: hue 30 degrees, saturation under 10%, brightness
    // 69%. A field reading saturation from a channel would print far more.
    let readouts = fields(rgb_to_hsv([176, 168, 160, 255]));
    assert_eq!(values(&readouts), ["30", "9", "69", "176", "168", "160"]);
}

#[test]
fn values_round_rather_than_truncate() {
    // #E15A17 is 19.9 degrees and 88.24% brightness. Printing the integer part would
    // read 19 and 88; the picker rounds, so the hue is 20.
    let readouts = fields(rgb_to_hsv([225, 90, 23, 255]));
    assert_eq!(values(&readouts), ["20", "90", "88", "225", "90", "23"]);
}

#[test]
fn the_two_models_describe_the_one_live_colour() {
    // Six ordinary colours, a grey and a near-black: enough range that a channel
    // swapped with another cannot agree for all of them.
    for (label, colour, printed) in [
        (
            "red",
            [255, 0, 0, 255],
            ["0", "100", "100", "255", "0", "0"],
        ),
        (
            "green",
            [0, 255, 0, 255],
            ["120", "100", "100", "0", "255", "0"],
        ),
        (
            "blue",
            [0, 0, 255, 255],
            ["240", "100", "100", "0", "0", "255"],
        ),
        (
            "orange",
            [255, 127, 0, 255],
            ["30", "100", "100", "255", "127", "0"],
        ),
        (
            "grey",
            [128, 128, 128, 255],
            ["0", "0", "50", "128", "128", "128"],
        ),
        (
            "putty",
            [176, 168, 160, 255],
            ["30", "9", "69", "176", "168", "160"],
        ),
        (
            "burnt orange",
            [225, 90, 23, 255],
            ["20", "90", "88", "225", "90", "23"],
        ),
        (
            "near black",
            [10, 20, 30, 255],
            ["210", "67", "12", "10", "20", "30"],
        ),
    ] {
        let readouts = fields(rgb_to_hsv(colour));
        assert_eq!(values(&readouts), printed, "{label}");
        let live = hsv_to_rgb(rgb_to_hsv(colour));
        for channel in 0..3 {
            assert_eq!(
                printed_byte(printed[3 + channel]),
                live[channel],
                "{label} prints a red/green/blue channel that is not the colour the \
                 picker holds"
            );
        }
        // The hex field and the channel fields describe the same colour, so the
        // printed channels must build the hex string the picker shows.
        let hex = format!(
            "#{:02X}{:02X}{:02X}",
            printed_byte(printed[3]),
            printed_byte(printed[4]),
            printed_byte(printed[5])
        );
        assert_eq!(hex, to_hex(live), "{label}");
        // The angle and the two percentages are enough to rebuild the colour: a
        // caption bound to the wrong channel would not survive this round trip.
        let rebuilt = Hsv {
            h: printed[0].parse().unwrap(),
            s: printed[1].parse::<f32>().unwrap() / 100.0,
            v: printed[2].parse::<f32>().unwrap() / 100.0,
        };
        let rebuilt_rgb = hsv_to_rgb(rebuilt);
        for channel in 0..3 {
            let drift = i16::from(rebuilt_rgb[channel]) - i16::from(live[channel]);
            assert!(
                drift.abs() <= 1,
                "{label}: the hue/saturation/brightness fields print {printed:?}, \
                 which rebuilds to {rebuilt_rgb:?} instead of the {live:?} the \
                 channel fields print"
            );
        }
    }
}

#[test]
fn moving_the_hue_strip_rederives_the_hue_and_the_red_channels() {
    // The hue track is the only field a drag can move without touching saturation or
    // brightness, so it isolates the re-derivation: red and blue follow the angle.
    let before = fields(Hsv {
        h: 0.0,
        s: 1.0,
        v: 1.0,
    });
    assert_eq!(values(&before), ["0", "100", "100", "255", "0", "0"]);
    let after = fields(Hsv {
        h: 180.0,
        s: 1.0,
        v: 1.0,
    });
    assert_eq!(values(&after), ["180", "100", "100", "0", "255", "255"]);
    for field in [0, 3, 4, 5] {
        assert_ne!(
            before[field].value, after[field].value,
            "field {field} kept its text after the colour changed"
        );
    }
}

#[test]
fn dragging_the_saturation_box_rederives_every_field_but_the_hue() {
    let before = fields(Hsv {
        h: 180.0,
        s: 1.0,
        v: 1.0,
    });
    assert_eq!(values(&before), ["180", "100", "100", "0", "255", "255"]);
    let after = fields(Hsv {
        h: 180.0,
        s: 0.5,
        v: 0.5,
    });
    assert_eq!(values(&after), ["180", "50", "50", "64", "128", "128"]);
    assert_eq!(before[0].value, after[0].value);
    for field in 1..6 {
        assert_ne!(
            before[field].value, after[field].value,
            "field {field} kept its text after the saturation box moved the colour"
        );
    }
}
