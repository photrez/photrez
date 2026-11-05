//! Semantic pixel classifier - ported byte-for-byte from the frozen
//! `shape-diff.ts` / `shell-diff.ts` classifier, whose bands are tuned for the
//! shell palette (`#141414` chrome, `#1B1B1B` rail, `#2E2E2E` rules,
//! `#7A7A7A`/`#CACACA` text, `#E15A17` accent, white swatch). Do not retune:
//! the recorded `HARD MISMATCH` numbers are only comparable under this exact
//! classification.

use serde::Serialize;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Klass {
    Bg,
    Active,
    Stroke,
    Orange,
    White,
}

/// Report order of the histogram rows (matches `CLASSES` in the TS source).
pub const CLASS_ORDER: [Klass; 5] = [
    Klass::Bg,
    Klass::Active,
    Klass::Stroke,
    Klass::Orange,
    Klass::White,
];

impl Klass {
    pub fn as_str(self) -> &'static str {
        match self {
            Klass::Bg => "bg",
            Klass::Active => "active",
            Klass::Stroke => "stroke",
            Klass::Orange => "orange",
            Klass::White => "white",
        }
    }
}

/// Classify one pixel. Semantics identical to the TS original:
///
/// ```text
/// r - b > 70 && r > 130        -> orange
/// luma > 215 && max - min < 30 -> white
/// max - min < 40               -> stroke if luma >= 75, active if >= 34, else bg
/// otherwise                    -> stroke if luma >= 75, active if >= 34, else bg
/// ```
pub fn classify(r: u8, g: u8, b: u8) -> Klass {
    let ri = i32::from(r);
    let gi = i32::from(g);
    let bi = i32::from(b);
    let luma = 0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b);
    let spread = ri.max(gi).max(bi) - ri.min(gi).min(bi);

    if ri - bi > 70 && ri > 130 {
        return Klass::Orange;
    }
    if luma > 215.0 && spread < 30 {
        return Klass::White;
    }
    if luma >= 75.0 {
        Klass::Stroke
    } else if luma >= 34.0 {
        Klass::Active
    } else {
        Klass::Bg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Palette anchors from the TS header comment; if the bands move, these
    // catch it.
    #[test]
    fn palette_anchors_classify_as_recorded() {
        assert_eq!(classify(0x1B, 0x1B, 0x1B), Klass::Bg); // rail background
        assert_eq!(classify(0x2E, 0x2E, 0x2E), Klass::Active); // active row / rule
        assert_eq!(classify(0x7A, 0x7A, 0x7A), Klass::Stroke); // dim text
        assert_eq!(classify(0xCA, 0xCA, 0xCA), Klass::Stroke); // text
        assert_eq!(classify(0xE1, 0x5A, 0x17), Klass::Orange); // accent swatch
        assert_eq!(classify(0xFF, 0xFF, 0xFF), Klass::White); // background swatch
    }

    #[test]
    fn boundaries_follow_the_ts_rules() {
        // r - b = 70 exactly: not orange (strict >).
        assert_ne!(classify(200, 100, 130), Klass::Orange);
        // Achromatic luma ramp across the two thresholds.
        assert_eq!(classify(33, 33, 33), Klass::Bg);
        assert_eq!(classify(34, 34, 34), Klass::Active);
        assert_eq!(classify(74, 74, 74), Klass::Active);
        assert_eq!(classify(75, 75, 75), Klass::Stroke);
    }
}
