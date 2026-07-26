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
}
