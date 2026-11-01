// SPDX-License-Identifier: AGPL-3.0-or-later
//! Colour conversions for the paint-colour picker.
//!
//! The picker edits HSV and paints through gpui's `Hsla`, while the document
//! tools consume sRGBA bytes. Keeping the three conversions here (and tested)
//! stops the picker, the swatches and the tools from each rolling their own.

use gpui_kit::Hsla;

/// HSV with `h` in degrees `0..360` and `s`/`v` in `0..1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

/// sRGBA (alpha 255) to HSV. Grey has no meaningful hue, so it reports 0.
pub fn rgb_to_hsv([r, g, b, _]: [u8; 4]) -> Hsv {
    let r = f32::from(r) / 255.0;
    let g = f32::from(g) / 255.0;
    let b = f32::from(b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    Hsv {
        h: if h < 0.0 { h + 360.0 } else { h },
        s: if max == 0.0 { 0.0 } else { delta / max },
        v: max,
    }
}

/// HSV to sRGBA (alpha 255).
pub fn hsv_to_rgb(hsv: Hsv) -> [u8; 4] {
    let h = hsv.h.rem_euclid(360.0);
    let s = hsv.s.clamp(0.0, 1.0);
    let v = hsv.v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h {
        h if h < 60.0 => (c, x, 0.0),
        h if h < 120.0 => (x, c, 0.0),
        h if h < 180.0 => (0.0, c, x),
        h if h < 240.0 => (0.0, x, c),
        h if h < 300.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [byte(r), byte(g), byte(b), 255]
}

/// sRGBA to gpui's `Hsla` (`h` normalised to `0..1`, paint ignores alpha).
pub fn hsla([r, g, b, a]: [u8; 4]) -> Hsla {
    let hsv = rgb_to_hsv([r, g, b, a]);
    let max = f32::from(r.max(g).max(b)) / 255.0;
    let min = f32::from(r.min(g).min(b)) / 255.0;
    let l = (max + min) / 2.0;
    let s = if max == min {
        0.0
    } else {
        let d = max - min;
        if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        }
    };
    Hsla {
        h: hsv.h / 360.0,
        s,
        l,
        a: f32::from(a) / 255.0,
    }
}

/// `#RRGGBB`, the form both the picker field and the reference palette use.
pub fn to_hex([r, g, b, _]: [u8; 4]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// Parse `#RGB` / `#RRGGBB` (the `#` optional, case-insensitive).
pub fn parse_hex(text: &str) -> Option<[u8; 4]> {
    let digits = text.strip_prefix('#').unwrap_or(text);
    let expanded: String = match digits.len() {
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 => digits.to_string(),
        _ => return None,
    };
    if !expanded.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&expanded[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?, 255])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn primaries_keep_their_hue_through_both_directions() {
        for (rgb, hue) in [
            ([255, 0, 0, 255], 0.0),
            ([0, 255, 0, 255], 120.0),
            ([0, 0, 255, 255], 240.0),
            ([255, 255, 0, 255], 60.0),
            ([0, 255, 255, 255], 180.0),
            ([255, 0, 255, 255], 300.0),
        ] {
            let hsv = rgb_to_hsv(rgb);
            assert!(close(hsv.h, hue), "{rgb:?} -> {hsv:?}");
            assert!(close(hsv.s, 1.0));
            assert!(close(hsv.v, 1.0));
            assert_eq!(hsv_to_rgb(hsv), rgb);
        }
    }

    #[test]
    fn black_white_and_grey_round_trip_without_hue() {
        for rgb in [[0, 0, 0, 255], [255, 255, 255, 255], [128, 128, 128, 255]] {
            let hsv = rgb_to_hsv(rgb);
            assert_eq!(hsv.h, 0.0, "{rgb:?} has no hue");
            assert_eq!(hsv_to_rgb(hsv), rgb);
        }
    }

    #[test]
    fn a_saturated_hue_at_full_value_is_the_pure_colour() {
        let rgb = hsv_to_rgb(Hsv {
            h: 19.9,
            s: 1.0,
            v: 1.0,
        });
        assert_eq!(rgb[0], 255);
        assert_eq!(rgb[1], 85);
        assert_eq!(rgb[2], 0);
    }

    #[test]
    fn hex_parses_both_lengths_and_rejects_garbage() {
        assert_eq!(parse_hex("#E15A17"), Some([225, 90, 23, 255]));
        assert_eq!(parse_hex("e15a17"), Some([225, 90, 23, 255]));
        assert_eq!(parse_hex("#fff"), Some([255, 255, 255, 255]));
        assert_eq!(parse_hex("#12"), None);
        assert_eq!(parse_hex("#GGGGGG"), None);
        assert_eq!(to_hex([225, 90, 23, 255]), "#E15A17");
    }
}
