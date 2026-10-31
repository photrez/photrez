//! Dimension units for the dialog fields (`lib/units.ts`).
//!
//! Every field keeps the canonical value in whole pixels and converts only for
//! display. The oracle uses a 96 DPI base for these fields (not the 300 PPI of
//! the crop bar's unit conversion), so the conversions here are the same ones
//! the New Document and Resize Canvas dialogs use.

/// The units the dimension fields offer, in `UNITS` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DimensionUnit {
    #[default]
    Px,
    In,
    Cm,
    Mm,
    Pt,
}

impl DimensionUnit {
    pub const ALL: [Self; 5] = [Self::Px, Self::In, Self::Cm, Self::Mm, Self::Pt];

    pub fn label(self) -> &'static str {
        match self {
            Self::Px => "px",
            Self::In => "in",
            Self::Cm => "cm",
            Self::Mm => "mm",
            Self::Pt => "pt",
        }
    }

    /// `formatUnit` (`lib/units.ts`): whole pixels, two decimals otherwise.
    pub fn format(self, px: u32) -> String {
        let value = f64::from(px);
        let converted = match self {
            Self::Px => return format!("{}", (value.round() as u32)),
            Self::In => value / 96.0,
            Self::Cm => value / 96.0 * 2.54,
            Self::Mm => value / 96.0 * 25.4,
            Self::Pt => value / 96.0 * 72.0,
        };
        format!("{converted:.2}")
    }

    /// `unitToPx`: converts a display value back to whole pixels.
    pub fn to_px(self, value: f64) -> u32 {
        let px = match self {
            Self::Px => value,
            Self::In => value * 96.0,
            Self::Cm => value * 96.0 / 2.54,
            Self::Mm => value * 96.0 / 25.4,
            Self::Pt => value * 96.0 / 72.0,
        };
        px.round().max(0.0) as u32
    }

    /// Only physical units accept a decimal point in their fields.
    pub fn accepts_decimal(self) -> bool {
        self != Self::Px
    }
}

#[cfg(test)]
mod tests {
    use super::DimensionUnit;

    #[test]
    fn formatting_and_conversion_match_the_source() {
        assert_eq!(DimensionUnit::Px.format(800), "800");
        assert_eq!(DimensionUnit::Cm.format(800), "21.17");
        assert_eq!(DimensionUnit::Cm.format(600), "15.88");
        assert_eq!(DimensionUnit::In.format(96), "1.00");
        assert_eq!(DimensionUnit::Mm.format(96), "25.40");
        assert_eq!(DimensionUnit::Pt.format(96), "72.00");

        assert_eq!(DimensionUnit::Cm.to_px(21.17), 800);
        assert_eq!(DimensionUnit::Cm.to_px(10.0), 378);
        assert_eq!(DimensionUnit::In.to_px(1.0), 96);
        assert_eq!(DimensionUnit::Mm.to_px(25.4), 96);
        assert_eq!(DimensionUnit::Pt.to_px(10.0), 13);
        assert_eq!(DimensionUnit::Px.to_px(12.6), 13);
    }

    #[test]
    fn unit_labels_follow_the_source_order() {
        let labels: Vec<&str> = DimensionUnit::ALL.iter().map(|u| u.label()).collect();
        assert_eq!(labels, ["px", "in", "cm", "mm", "pt"]);
    }
}
