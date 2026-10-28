use super::PrintError;

const MAX_CUSTOM_PAPER_MM: f64 = i16::MAX as f64 / 10.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PrintSettings {
    pub(super) selected_printer: Option<String>,
    pub(super) copies: u32,
    pub(super) paper_name: String,
    pub(super) paper_index: i16,
    paper_width_mm: f64,
    paper_height_mm: f64,
    pub(super) orientation: Orientation,
    margin_mm: f64,
    pub(super) margin_left_mm: f64,
    pub(super) margin_right_mm: f64,
    pub(super) margin_top_mm: f64,
    pub(super) margin_bottom_mm: f64,
    hardware_margin_min_mm: f64,
    scale_to_fit: bool,
    scale_percent: f64,
    center_image: bool,
    top_offset_mm: f64,
    left_offset_mm: f64,
    unit: String,
    show_paper_white: bool,
    color_handling: String,
    rendering_intent: String,
    black_point_compensation: bool,
    printer_dpi: Option<f64>,
}

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            selected_printer: None,
            copies: 1,
            paper_name: "A4".to_string(),
            paper_index: 9,
            paper_width_mm: 210.0,
            paper_height_mm: 297.0,
            orientation: Orientation::Portrait,
            margin_mm: 5.0,
            margin_left_mm: 5.0,
            margin_right_mm: 5.0,
            margin_top_mm: 5.0,
            margin_bottom_mm: 5.0,
            hardware_margin_min_mm: 0.0,
            scale_to_fit: false,
            scale_percent: 100.0,
            center_image: true,
            top_offset_mm: 0.0,
            left_offset_mm: 0.0,
            unit: "mm".to_string(),
            show_paper_white: true,
            color_handling: "printer_manages".to_string(),
            rendering_intent: "perceptual".to_string(),
            black_point_compensation: true,
            printer_dpi: Some(300.0),
        }
    }
}

impl PrintSettings {
    pub(crate) fn set_paper(
        &mut self,
        name: &str,
        index: i16,
        width_mm: f64,
        height_mm: f64,
    ) -> (f64, f64) {
        self.paper_name = name.to_owned();
        self.paper_index = index;
        self.paper_width_mm = width_mm;
        self.paper_height_mm = height_mm;
        self.canonicalize_paper()
    }

    pub(crate) fn set_orientation(&mut self, orientation: Orientation) -> (f64, f64) {
        self.orientation = orientation;
        self.canonicalize_paper()
    }

    pub(crate) fn oriented_dimensions(&self) -> (f64, f64) {
        match self.orientation {
            Orientation::Portrait => (self.paper_width_mm, self.paper_height_mm),
            Orientation::Landscape => (self.paper_height_mm, self.paper_width_mm),
        }
    }

    pub(crate) fn set_margin_mm(&mut self, margin_mm: f64) {
        if !margin_mm.is_finite() {
            self.margin_mm = margin_mm;
            return;
        }
        self.margin_mm = margin_mm.clamp(self.hardware_margin_min_mm, 100.0);
    }

    pub(crate) fn set_hardware_margin_min_mm(&mut self, minimum_mm: f64) -> Result<(), PrintError> {
        if !minimum_mm.is_finite() {
            return Err(PrintError::validation(
                "Hardware margin minimum must be finite",
            ));
        }
        self.hardware_margin_min_mm = minimum_mm.clamp(0.0, 100.0);
        self.set_margin_mm(self.margin_mm);
        Ok(())
    }

    pub(crate) fn set_per_side_margins(
        &mut self,
        left_mm: f64,
        right_mm: f64,
        top_mm: f64,
        bottom_mm: f64,
    ) {
        self.margin_left_mm = non_negative(left_mm);
        self.margin_right_mm = non_negative(right_mm);
        self.margin_top_mm = non_negative(top_mm);
        self.margin_bottom_mm = non_negative(bottom_mm);
        if [left_mm, right_mm, top_mm, bottom_mm]
            .iter()
            .any(|margin| !margin.is_finite())
        {
            self.margin_mm = left_mm.max(right_mm).max(top_mm).max(bottom_mm);
            return;
        }
        self.margin_mm = left_mm
            .max(right_mm)
            .max(top_mm)
            .max(bottom_mm)
            .max(self.hardware_margin_min_mm)
            .clamp(self.hardware_margin_min_mm, 100.0);
    }

    pub(crate) fn set_copies(&mut self, copies: u32) {
        self.copies = copies.max(1);
    }

    pub(crate) fn set_scale_to_fit(&mut self, enabled: bool) {
        self.scale_to_fit = enabled;
    }

    pub(crate) fn set_scale_percent(&mut self, percent: f64) {
        self.scale_percent = percent.clamp(1.0, 1000.0);
    }

    pub(crate) fn set_center_image(&mut self, center: bool) {
        self.center_image = center;
    }

    pub(crate) fn set_offsets_mm(&mut self, left_mm: f64, top_mm: f64) {
        self.left_offset_mm = left_mm;
        self.top_offset_mm = top_mm;
    }

    pub(crate) fn set_unit(&mut self, unit: &str) {
        self.unit = unit.to_owned();
    }

    pub(crate) fn set_show_paper_white(&mut self, show: bool) {
        self.show_paper_white = show;
    }

    pub(crate) fn set_color_handling(&mut self, handling: &str) {
        self.color_handling = handling.to_owned();
    }

    pub(crate) fn set_rendering_intent(&mut self, intent: &str) {
        self.rendering_intent = intent.to_owned();
    }

    pub(crate) fn set_black_point_compensation(&mut self, enabled: bool) {
        self.black_point_compensation = enabled;
    }

    pub(crate) fn set_selected_printer(&mut self, printer: Option<String>) {
        self.selected_printer = printer;
    }

    pub(super) fn validate(&self) -> Result<(), PrintError> {
        if !self.paper_width_mm.is_finite()
            || !self.paper_height_mm.is_finite()
            || self.paper_width_mm <= 0.0
            || self.paper_height_mm <= 0.0
        {
            return Err(PrintError::validation(
                "Paper dimensions must be finite and positive",
            ));
        }
        if self.paper_index == 256
            && (self.paper_width_mm > MAX_CUSTOM_PAPER_MM
                || self.paper_height_mm > MAX_CUSTOM_PAPER_MM)
        {
            return Err(PrintError::resource_limit(
                "Paper dimensions exceed the printer DEVMODE limit",
            ));
        }

        let margins = [
            self.margin_mm,
            self.margin_left_mm,
            self.margin_right_mm,
            self.margin_top_mm,
            self.margin_bottom_mm,
            self.hardware_margin_min_mm,
        ];
        if margins
            .iter()
            .any(|margin| !margin.is_finite() || *margin < 0.0)
        {
            return Err(PrintError::validation(
                "Print margins must be finite and non-negative",
            ));
        }
        let (paper_width_mm, paper_height_mm) = self.oriented_dimensions();
        if self.margin_left_mm + self.margin_right_mm >= paper_width_mm
            || self.margin_top_mm + self.margin_bottom_mm >= paper_height_mm
        {
            return Err(PrintError::validation(
                "Print margins leave no printable page area",
            ));
        }
        if !self.scale_percent.is_finite()
            || !(1.0..=1000.0).contains(&self.scale_percent)
            || !self.left_offset_mm.is_finite()
            || !self.top_offset_mm.is_finite()
            || self.copies == 0
            || self
                .printer_dpi
                .is_some_and(|dpi| !dpi.is_finite() || dpi <= 0.0)
        {
            return Err(PrintError::validation("Invalid print settings"));
        }
        if self
            .selected_printer
            .as_deref()
            .is_some_and(|printer| printer.contains('\0'))
        {
            return Err(PrintError::validation(
                "Printer name contains a null character",
            ));
        }
        Ok(())
    }

    fn canonicalize_paper(&mut self) -> (f64, f64) {
        if self.paper_width_mm > self.paper_height_mm {
            std::mem::swap(&mut self.paper_width_mm, &mut self.paper_height_mm);
        }
        self.oriented_dimensions()
    }
}

fn non_negative(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::{Orientation, PrintSettings};
    use crate::services::print::PrintError;

    #[test]
    fn defaults_match_the_existing_print_settings() {
        let settings = PrintSettings::default();
        assert_eq!(settings.paper_name, "A4");
        assert_eq!(settings.paper_index, 9);
        assert_eq!(settings.oriented_dimensions(), (210.0, 297.0));
        assert_eq!(settings.copies, 1);
        assert_eq!(settings.margin_mm, 5.0);
        assert_eq!(settings.scale_percent, 100.0);
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn paper_is_canonical_and_orientation_returns_effective_dimensions() {
        let mut settings = PrintSettings::default();
        assert_eq!(
            settings.set_paper("wide", 256, 300.0, 200.0),
            (200.0, 300.0)
        );
        assert_eq!(
            settings.set_orientation(Orientation::Portrait),
            (200.0, 300.0)
        );
        assert_eq!(
            settings.set_orientation(Orientation::Landscape),
            (300.0, 200.0)
        );
    }

    #[test]
    fn margin_floor_and_setting_bounds_are_enforced() {
        let mut settings = PrintSettings::default();
        settings
            .set_hardware_margin_min_mm(3.0)
            .expect("valid hardware margin");
        settings.set_margin_mm(2.0);
        assert_eq!(settings.margin_mm, 3.0);
        settings.set_per_side_margins(1.0, 4.0, 2.0, 3.0);
        assert_eq!(settings.margin_left_mm, 1.0);
        assert_eq!(settings.margin_right_mm, 4.0);
        assert_eq!(settings.margin_mm, 4.0);
        settings.set_copies(0);
        settings.set_scale_percent(2000.0);
        assert_eq!(settings.copies, 1);
        assert_eq!(settings.scale_percent, 1000.0);
    }

    #[test]
    fn invalid_paper_and_margin_values_are_typed_validation_errors() {
        let settings = PrintSettings {
            paper_width_mm: f64::NAN,
            ..PrintSettings::default()
        };
        assert!(matches!(settings.validate(), Err(PrintError::Service(_))));

        let settings = PrintSettings {
            margin_left_mm: f64::INFINITY,
            ..PrintSettings::default()
        };
        assert!(matches!(settings.validate(), Err(PrintError::Service(_))));

        let settings = PrintSettings {
            margin_left_mm: 210.0,
            ..PrintSettings::default()
        };
        assert!(matches!(settings.validate(), Err(PrintError::Service(_))));
    }

    #[test]
    fn zero_and_non_finite_printer_dpi_are_typed_validation_errors() {
        for printer_dpi in [0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let settings = PrintSettings {
                printer_dpi: Some(printer_dpi),
                ..PrintSettings::default()
            };
            assert!(
                matches!(settings.validate(), Err(PrintError::Service(_))),
                "invalid printer DPI accepted: {printer_dpi}"
            );
        }
    }
}
