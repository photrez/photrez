//! Crop aspect/size model: the preset list, the auto-fit solver, the unit
//! conversion and the mode handlers the crop bar drives.
//!
//! Three modes share one pending frame: `Free` (the frame follows the drag),
//! `Ratio` (a preset or custom W:H refits the frame to that aspect) and `Size`
//! (physical W/H at the crop bar's own 300 PPI base). The dialog fields use
//! 96 DPI instead (`units.rs`); the two bases are deliberately different in
//! the crop bar and the dialogs, so they stay separate here.

/// The crop ratio presets, in menu order.
pub(crate) const CROP_PRESETS: [((u32, u32), &str); 9] = [
    ((1, 1), "1:1"),
    ((4, 5), "4:5"),
    ((5, 4), "5:4"),
    ((2, 3), "2:3"),
    ((3, 2), "3:2"),
    ((4, 3), "4:3"),
    ((9, 16), "9:16"),
    ((16, 9), "16:9"),
    ((21, 9), "21:9"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CropMode {
    #[default]
    Free,
    Ratio,
    Size,
}

/// The units the crop Size mode offers, in picker order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CropSizeUnit {
    #[default]
    Px,
    Cm,
    Mm,
    In,
}

/// The crop bar's pixels-per-inch base. Not the dialogs' 96 DPI.
pub(crate) const CROP_PPI: f64 = 300.0;

impl CropSizeUnit {
    pub(crate) const ALL: [Self; 4] = [Self::Px, Self::Cm, Self::Mm, Self::In];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Px => "px",
            Self::Cm => "cm",
            Self::Mm => "mm",
            Self::In => "in",
        }
    }

    fn factor(self) -> f64 {
        match self {
            Self::Px => 1.0,
            Self::Cm => CROP_PPI / 2.54,
            Self::Mm => CROP_PPI / 25.4,
            Self::In => CROP_PPI,
        }
    }

    /// `toUnit` — rounds to two decimals, like the source's `+(...).toFixed(2)`.
    pub(crate) fn to_unit(self, px: f64) -> f64 {
        round2(px / self.factor())
    }

    /// `fromUnit` — rounds to two decimals, like the source's `+(...).toFixed(2)`.
    pub(crate) fn to_pixels(self, value: f64) -> f64 {
        round2(value * self.factor())
    }

    /// Only physical units accept a decimal point.
    pub(crate) fn accepts_decimal(self) -> bool {
        self != Self::Px
    }
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// Fits a crop rect to an aspect: the largest frame with
/// this aspect that fits the document, centred, honouring a rotated document.
pub(crate) fn fit_crop_rect_to_aspect(
    (aw, ah): (u32, u32),
    doc_w: f64,
    doc_h: f64,
    rotation: f64,
) -> [f64; 4] {
    let ratio = f64::from(aw) / f64::from(ah);
    let (cw, ch) = (doc_w, doc_h);

    if rotation.rem_euclid(180.0).abs() < 0.01 {
        let (w, h) = if cw / ch > ratio {
            (ch * ratio, ch)
        } else {
            (cw, cw / ratio)
        };
        return [(cw - w) / 2.0, (ch - h) / 2.0, w, h];
    }

    let rad = rotation.abs() * std::f64::consts::PI / 180.0;
    let (cos, sin) = (rad.cos(), rad.sin());
    let factor_x = (cos - sin / ratio).abs();
    let factor_y = (sin + cos / ratio).abs();
    let hw = (cw / 2.0 / factor_x).min(ch / 2.0 / factor_y);
    let w = hw * 2.0;
    let h = w / ratio;
    [(cw - w) / 2.0, (ch - h) / 2.0, w, h]
}

/// `currentRatioLabel`: Free / Size / preset label / `w:h` for a custom ratio.
pub(crate) fn ratio_label(mode: CropMode, aspect: Option<(u32, u32)>) -> String {
    match mode {
        CropMode::Free => "Free".to_string(),
        CropMode::Size => "Size".to_string(),
        CropMode::Ratio => match aspect {
            None => "Free".to_string(),
            Some(a) => CROP_PRESETS
                .iter()
                .find(|(preset, _)| *preset == a)
                .map(|(_, label)| (*label).to_string())
                .unwrap_or_else(|| format!("{}:{}", a.0, a.1)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_matches_the_source_for_landscape_and_portrait_documents() {
        // 800x600 document, 1:1 -> 600x600 centred at (100, 0).
        assert_eq!(
            fit_crop_rect_to_aspect((1, 1), 800.0, 600.0, 0.0),
            [100.0, 0.0, 600.0, 600.0]
        );
        // 16:9 in the same document -> 800x450 at y=75.
        assert_eq!(
            fit_crop_rect_to_aspect((16, 9), 800.0, 600.0, 0.0),
            [0.0, 75.0, 800.0, 450.0]
        );
        // 9:16 -> 337.5 wide, x = 231.25.
        assert_eq!(
            fit_crop_rect_to_aspect((9, 16), 800.0, 600.0, 0.0),
            [231.25, 0.0, 337.5, 600.0]
        );
    }

    #[test]
    fn fit_honours_a_rotated_document() {
        let rect = fit_crop_rect_to_aspect((16, 9), 800.0, 600.0, 30.0);
        assert!(rect[2] > 0.0 && rect[3] > 0.0);
        assert!((rect[2] / rect[3] - 16.0 / 9.0).abs() < 1e-9);
        assert!(rect[0] >= 0.0 && rect[1] >= 0.0);
        assert!(rect[0] + rect[2] <= 800.0 + 1e-9 && rect[1] + rect[3] <= 600.0 + 1e-9);
    }

    #[test]
    fn crop_units_use_the_300_ppi_base_and_two_decimals() {
        assert_eq!(CropSizeUnit::Px.to_unit(800.0), 800.0);
        assert_eq!(CropSizeUnit::Cm.to_unit(800.0), 6.77);
        assert_eq!(CropSizeUnit::Mm.to_unit(800.0), 67.73);
        assert_eq!(CropSizeUnit::In.to_unit(300.0), 1.0);
        assert_eq!(CropSizeUnit::In.to_pixels(4.0), 1200.0);
        assert_eq!(CropSizeUnit::Cm.to_pixels(2.54), 300.0);
        assert_eq!(CropSizeUnit::Mm.to_pixels(25.4), 300.0);
        assert_eq!(CropSizeUnit::Px.to_pixels(12.6), 12.6);
        assert!(CropSizeUnit::Cm.accepts_decimal() && !CropSizeUnit::Px.accepts_decimal());
    }

    #[test]
    fn ratio_labels_follow_the_mode_and_presets() {
        assert_eq!(ratio_label(CropMode::Free, None), "Free");
        assert_eq!(ratio_label(CropMode::Size, None), "Size");
        assert_eq!(ratio_label(CropMode::Ratio, Some((16, 9))), "16:9");
        assert_eq!(ratio_label(CropMode::Ratio, Some((7, 5))), "7:5");
        assert_eq!(ratio_label(CropMode::Ratio, None), "Free");
    }
}
