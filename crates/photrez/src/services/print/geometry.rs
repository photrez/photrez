use super::PrintError;

pub(crate) const PPI: f64 = 96.0;
pub(crate) const MM_PER_INCH: f64 = 25.4;

pub(crate) fn mm_to_unit(mm: f64, unit: &str) -> f64 {
    match unit {
        "mm" => mm,
        "cm" => mm / 10.0,
        "in" => mm / MM_PER_INCH,
        "px" => mm / MM_PER_INCH * PPI,
        _ => mm,
    }
}

pub(crate) fn unit_to_mm(value: f64, unit: &str) -> f64 {
    match unit {
        "mm" => value,
        "cm" => value * 10.0,
        "in" => value * MM_PER_INCH,
        "px" => value / PPI * MM_PER_INCH,
        _ => value,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PrintRect {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: i32,
    pub(super) height: i32,
}

pub(super) fn printable_rect(
    paper_width_mm: f64,
    paper_height_mm: f64,
    dpi_x: f64,
    dpi_y: f64,
    hardware_offset_x: i32,
    hardware_offset_y: i32,
    margins_mm: [f64; 4],
) -> Result<PrintRect, PrintError> {
    let page_width = mm_to_pixels(paper_width_mm, dpi_x)?;
    let page_height = mm_to_pixels(paper_height_mm, dpi_y)?;
    let left = mm_to_pixels(margins_mm[0], dpi_x)?;
    let right = mm_to_pixels(margins_mm[1], dpi_x)?;
    let top = mm_to_pixels(margins_mm[2], dpi_y)?;
    let bottom = mm_to_pixels(margins_mm[3], dpi_y)?;
    let width = page_width
        .checked_sub(left)
        .and_then(|width| width.checked_sub(right))
        .ok_or_else(|| PrintError::resource_limit("Print width overflow"))?;
    let height = page_height
        .checked_sub(top)
        .and_then(|height| height.checked_sub(bottom))
        .ok_or_else(|| PrintError::resource_limit("Print height overflow"))?;

    if width <= 0 || height <= 0 {
        return Err(PrintError::validation(
            "Print margins leave no printable page area",
        ));
    }

    Ok(PrintRect {
        x: hardware_offset_x
            .checked_add(left)
            .ok_or_else(|| PrintError::resource_limit("Print X position overflow"))?,
        y: hardware_offset_y
            .checked_add(top)
            .ok_or_else(|| PrintError::resource_limit("Print Y position overflow"))?,
        width,
        height,
    })
}

pub(super) fn fit_dimensions(
    source_width: u32,
    source_height: u32,
    area_width: i32,
    area_height: i32,
) -> Result<(i32, i32), PrintError> {
    if source_width == 0 || source_height == 0 || area_width <= 0 || area_height <= 0 {
        return Err(PrintError::validation(
            "Print image dimensions must be positive",
        ));
    }

    if i64::from(source_width) == i64::from(area_width)
        && i64::from(source_height) == i64::from(area_height)
    {
        return Ok((area_width, area_height));
    }

    let scale = (area_width as f64 / f64::from(source_width))
        .min(area_height as f64 / f64::from(source_height));
    let width = (f64::from(source_width) * scale) as i32;
    let height = (f64::from(source_height) * scale) as i32;
    if width <= 0 || height <= 0 {
        return Err(PrintError::validation(
            "Print image cannot fit the printable area",
        ));
    }
    Ok((width, height))
}

fn mm_to_pixels(mm: f64, dpi: f64) -> Result<i32, PrintError> {
    if !mm.is_finite() || mm < 0.0 || !dpi.is_finite() || dpi <= 0.0 {
        return Err(PrintError::validation(
            "Invalid print dimensions or printer DPI",
        ));
    }
    let pixels = (mm / MM_PER_INCH * dpi).round();
    if pixels > f64::from(i32::MAX) {
        return Err(PrintError::resource_limit(
            "Print dimensions exceed GDI limits",
        ));
    }
    Ok(pixels as i32)
}

#[cfg(test)]
mod tests {
    use super::{fit_dimensions, mm_to_unit, printable_rect, unit_to_mm};

    #[test]
    fn physical_units_round_trip() {
        for unit in ["mm", "cm", "in", "px"] {
            let mm = 25.4;
            assert!((unit_to_mm(mm_to_unit(mm, unit), unit) - mm).abs() < 0.001);
        }
        assert_eq!(mm_to_unit(42.0, "unknown"), 42.0);
        assert_eq!(unit_to_mm(42.0, "unknown"), 42.0);
    }

    /// The print-geometry expectations, kept as an independent comparison
    /// table: exact conversion anchors, zero/negative identity, and the
    /// large-value case.
    #[test]
    fn legacy_unit_conversion_cases_match() {
        assert!((mm_to_unit(25.4, "in") - 1.0).abs() < 0.001);
        assert!((mm_to_unit(10.0, "cm") - 1.0).abs() < 0.001);
        assert!((mm_to_unit(1.0, "mm") - 1.0).abs() < 0.001);
        assert!((mm_to_unit(25.4, "px") - 96.0).abs() < 0.001);
        assert!((unit_to_mm(1.0, "in") - 25.4).abs() < 0.001);
        assert!((unit_to_mm(1.0, "cm") - 10.0).abs() < 0.001);
        assert!((unit_to_mm(96.0, "px") - 25.4).abs() < 0.001);

        for mm in [0.0, 1.0, 25.4, 100.0, 297.0, 1000.0] {
            for unit in ["mm", "cm", "in", "px"] {
                let back = unit_to_mm(mm_to_unit(mm, unit), unit);
                assert!((mm - back).abs() < 0.001, "{mm} mm via {unit}");
            }
        }

        assert_eq!(mm_to_unit(0.0, "px"), 0.0);
        assert!((mm_to_unit(-25.4, "in") + 1.0).abs() < 0.001);
        assert!((unit_to_mm(-1.0, "in") + 25.4).abs() < 0.001);

        let huge = 1_000_000.0;
        assert!((mm_to_unit(huge, "cm") - huge / 10.0).abs() < 0.001);
        assert!((unit_to_mm(mm_to_unit(huge, "cm"), "cm") - huge).abs() < 0.1);
        assert!((unit_to_mm(300.0, "px") - 300.0 / 96.0 * 25.4).abs() < 0.001);
    }

    #[test]
    fn printable_area_applies_hardware_offset_and_each_margin() {
        let rect = printable_rect(210.0, 297.0, 96.0, 96.0, 10, 20, [3.0, 4.0, 5.0, 6.0])
            .expect("valid printable area");
        assert_eq!((rect.x, rect.y), (21, 39));
        assert_eq!((rect.width, rect.height), (768, 1081));
    }

    #[test]
    fn image_is_scaled_to_fit_without_changing_aspect_ratio() {
        assert_eq!(fit_dimensions(100, 50, 200, 200).expect("fit"), (200, 100));
        assert_eq!(
            fit_dimensions(100, 50, 100, 50).expect("exact fit"),
            (100, 50)
        );
    }
}
