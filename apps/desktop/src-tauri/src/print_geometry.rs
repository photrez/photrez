/// Standard PPI for print calculations.
pub const PPI: f64 = 96.0;

/// Millimeters per inch.
pub const MM_PER_INCH: f64 = 25.4;

/// Convert millimeters to the specified unit.
pub fn mm_to_unit(mm: f64, unit: &str) -> f64 {
    match unit {
        "mm" => mm,
        "cm" => mm / 10.0,
        "in" => mm / MM_PER_INCH,
        "px" => mm / MM_PER_INCH * PPI,
        _ => mm,
    }
}

/// Convert from the specified unit to millimeters.
pub fn unit_to_mm(value: f64, unit: &str) -> f64 {
    match unit {
        "mm" => value,
        "cm" => value * 10.0,
        "in" => value * MM_PER_INCH,
        "px" => value / PPI * MM_PER_INCH,
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mm_to_unit_conversions() {
        assert!((mm_to_unit(25.4, "in") - 1.0).abs() < 0.001);
        assert!((mm_to_unit(10.0, "cm") - 1.0).abs() < 0.001);
        assert!((mm_to_unit(1.0, "mm") - 1.0).abs() < 0.001);
        assert!((mm_to_unit(25.4, "px") - 96.0).abs() < 0.001);
    }

    #[test]
    fn unit_to_mm_conversions() {
        assert!((unit_to_mm(1.0, "in") - 25.4).abs() < 0.001);
        assert!((unit_to_mm(1.0, "cm") - 10.0).abs() < 0.001);
        assert!((unit_to_mm(1.0, "mm") - 1.0).abs() < 0.001);
        assert!((unit_to_mm(96.0, "px") - 25.4).abs() < 0.001);
    }

    #[test]
    fn round_trip_mm_to_unit_to_mm() {
        let values = [0.0, 1.0, 25.4, 100.0, 297.0, 1000.0];
        let units = ["mm", "cm", "in", "px"];
        for &mm in &values {
            for &unit in &units {
                let converted = mm_to_unit(mm, unit);
                let back = unit_to_mm(converted, unit);
                assert!(
                    (mm - back).abs() < 0.001,
                    "Round-trip failed: {} mm → {} {} → {} mm",
                    mm,
                    converted,
                    unit,
                    back
                );
            }
        }
    }

    #[test]
    fn zero_values_idempotent() {
        assert!((mm_to_unit(0.0, "mm") - 0.0).abs() < 0.001);
        assert!((mm_to_unit(0.0, "cm") - 0.0).abs() < 0.001);
        assert!((mm_to_unit(0.0, "in") - 0.0).abs() < 0.001);
        assert!((mm_to_unit(0.0, "px") - 0.0).abs() < 0.001);
        assert!((unit_to_mm(0.0, "mm") - 0.0).abs() < 0.001);
        assert!((unit_to_mm(0.0, "cm") - 0.0).abs() < 0.001);
        assert!((unit_to_mm(0.0, "in") - 0.0).abs() < 0.001);
        assert!((unit_to_mm(0.0, "px") - 0.0).abs() < 0.001);
    }

    #[test]
    fn unknown_unit_falls_back_to_mm() {
        // unknown units return the value unchanged (identity)
        assert!((mm_to_unit(42.0, "xyz") - 42.0).abs() < 0.001);
        assert!((unit_to_mm(42.0, "xyz") - 42.0).abs() < 0.001);
    }

    #[test]
    fn large_values_no_overflow() {
        // Very large mm values should not cause overflow
        let huge = 1_000_000.0;
        let in_unit = mm_to_unit(huge, "cm");
        assert!((in_unit - huge / 10.0).abs() < 0.001);
        let back = unit_to_mm(in_unit, "cm");
        assert!((huge - back).abs() < 0.1);
    }

    #[test]
    fn negative_values_passthrough() {
        // Negative values should convert correctly (caller validates bounds)
        assert!((mm_to_unit(-25.4, "in") + 1.0).abs() < 0.001);
        assert!((unit_to_mm(-1.0, "in") + 25.4).abs() < 0.001);
    }

    #[test]
    fn px_conversion_at_custom_ppi() {
        // At 96 PPI: 1 inch = 96 px = 25.4 mm
        assert!((mm_to_unit(25.4, "px") - 96.0).abs() < 0.001);
        // At 300 PPI: 300 px should be 300/96 * 25.4 ≈ 79.375 mm
        // (unit_to_mm uses 96 PPI hardcoded)
        let px_at_300 = 300.0;
        let mm = unit_to_mm(px_at_300, "px");
        assert!((mm - 300.0 / 96.0 * 25.4).abs() < 0.001);
    }
}
