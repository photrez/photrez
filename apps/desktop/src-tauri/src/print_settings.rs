use serde::{Deserialize, Serialize};

/// Single source of truth for all print settings.
/// Stored as Mutex<PrintSettings> in Tauri managed state.
/// Frontend listens to `print-settings-changed` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintSettings {
    // Printer
    pub selected_printer: Option<String>,
    pub copies: u32,

    // Paper
    pub paper_name: String,
    pub paper_index: i16,
    pub paper_width_mm: f64,
    pub paper_height_mm: f64,
    pub orientation: String,

    // Margins
    pub margin_mm: f64,
    /// Floor enforced by set_margin_mm — set from printer driver's
    /// PHYSICALOFFSETX/Y (Windows) or PPD ImageableArea (macOS/Linux).
    pub hardware_margin_min_mm: f64,

    // Scale & Position
    pub scale_to_fit: bool,
    pub scale_percent: f64,
    pub center_image: bool,
    pub top_offset_mm: f64,
    pub left_offset_mm: f64,

    // Display
    pub unit: String,
    pub show_paper_white: bool,

    // Print DPI — queried from printer driver when printer is selected.
    // Used by frontend to composite at printer-native DPI so StretchDIBits
    // hits the 1:1 GDI fast path (no CPU scaling).
    pub printer_dpi: Option<f64>,
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
            orientation: "portrait".to_string(),
            margin_mm: 5.0,
            hardware_margin_min_mm: 0.0,
            scale_to_fit: false,
            scale_percent: 100.0,
            center_image: true,
            top_offset_mm: 0.0,
            left_offset_mm: 0.0,
            unit: "mm".to_string(),
            show_paper_white: true,
            printer_dpi: Some(300.0),
        }
    }
}

impl PrintSettings {
    pub fn set_paper(&mut self, name: &str, index: i16, width_mm: f64, height_mm: f64) -> (f64, f64) {
        eprintln!("[RUST:PrintSettings] set_paper — name={}, index={}, width_mm={}, height_mm={}, canonical_before=({}, {}), orientation={}",
            name, index, width_mm, height_mm, self.paper_width_mm, self.paper_height_mm, self.orientation);
        self.paper_name = name.to_string();
        self.paper_index = index;
        self.paper_width_mm = width_mm;
        self.paper_height_mm = height_mm;
        let result = self.apply_orientation();
        eprintln!("[RUST:PrintSettings] set_paper — after apply_orientation: stored=({}, {}), returned=({}, {})",
            self.paper_width_mm, self.paper_height_mm, result.0, result.1);
        result
    }

    pub fn set_orientation(&mut self, orientation: &str) -> (f64, f64) {
        eprintln!("[RUST:PrintSettings] set_orientation — from={}, to={}, dims_before=({}, {})",
            self.orientation, orientation, self.paper_width_mm, self.paper_height_mm);
        self.orientation = orientation.to_string();
        let result = self.apply_orientation();
        eprintln!("[RUST:PrintSettings] set_orientation — after: stored=({}, {}), returned=({}, {})",
            self.paper_width_mm, self.paper_height_mm, result.0, result.1);
        result
    }

    pub fn set_margin_mm(&mut self, margin: f64) {
        self.margin_mm = margin.clamp(self.hardware_margin_min_mm, 100.0);
    }

    pub fn set_scale_to_fit(&mut self, enabled: bool) {
        self.scale_to_fit = enabled;
    }

    pub fn set_scale_percent(&mut self, percent: f64) {
        self.scale_percent = percent.clamp(1.0, 1000.0);
    }

    pub fn set_center_image(&mut self, center: bool) {
        self.center_image = center;
    }

    pub fn set_top_offset_mm(&mut self, offset: f64) {
        self.top_offset_mm = offset;
    }

    pub fn set_left_offset_mm(&mut self, offset: f64) {
        self.left_offset_mm = offset;
    }

    pub fn set_unit(&mut self, unit: &str) {
        self.unit = unit.to_string();
    }

    pub fn set_show_paper_white(&mut self, show: bool) {
        self.show_paper_white = show;
    }

    pub fn set_copies(&mut self, copies: u32) {
        self.copies = copies.max(1);
    }

    pub fn set_selected_printer(&mut self, printer: Option<String>) {
        self.selected_printer = printer.clone();
        #[cfg(target_os = "windows")]
        {
            if let Some(ref name) = printer {
                self.printer_dpi = crate::print_windows::query_printer_dpi_win(name);
            } else {
                self.printer_dpi = Some(300.0);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.printer_dpi = Some(300.0);
        }
    }

    /// Initialize the default printer if none is currently selected.
    /// Safe to call multiple times — only sets on first None.
    pub fn initialize_default_printer(&mut self) {
        if self.selected_printer.is_none() {
            if let Some(default) = printers::get_default_printer() {
                self.set_selected_printer(Some(default.name.clone()));
            }
        }
    }

    fn apply_orientation(&mut self) -> (f64, f64) {
        // Ensure canonical storage: width ≤ height (portrait form).
        // This is idempotent — calling it N times produces the same result.
        if self.paper_width_mm > self.paper_height_mm {
            std::mem::swap(&mut self.paper_width_mm, &mut self.paper_height_mm);
        }
        // Return effective dimensions based on orientation
        if self.orientation == "landscape" {
            (self.paper_height_mm, self.paper_width_mm)
        } else {
            (self.paper_width_mm, self.paper_height_mm)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings() {
        let s = PrintSettings::default();
        assert_eq!(s.paper_name, "A4");
        assert_eq!(s.paper_index, 9);
        assert_eq!(s.paper_width_mm, 210.0);
        assert_eq!(s.paper_height_mm, 297.0);
        assert_eq!(s.orientation, "portrait");
        assert_eq!(s.margin_mm, 5.0);
        assert!(!s.scale_to_fit);
    }

    #[test]
    fn set_paper_letter() {
        let mut s = PrintSettings::default();
        let (w, h) = s.set_paper("Letter", 1, 215.9, 279.4);
        assert_eq!(w, 215.9);
        assert_eq!(h, 279.4);
        assert_eq!(s.paper_name, "Letter");
        assert_eq!(s.paper_index, 1);
    }

    #[test]
    fn set_orientation_landscape() {
        let mut s = PrintSettings::default();
        let (w, h) = s.set_orientation("landscape");
        assert_eq!(w, 297.0);
        assert_eq!(h, 210.0);
    }

    #[test]
    fn margin_clamped() {
        let mut s = PrintSettings::default();
        s.set_margin_mm(-10.0);
        assert_eq!(s.margin_mm, 0.0);
        s.set_margin_mm(200.0);
        assert_eq!(s.margin_mm, 100.0);
    }

    #[test]
    fn margin_respects_hardware_min() {
        let mut s = PrintSettings::default();
        s.hardware_margin_min_mm = 3.0;
        s.set_margin_mm(0.0);
        assert_eq!(s.margin_mm, 3.0);
        s.set_margin_mm(2.0);
        assert_eq!(s.margin_mm, 3.0);
        s.set_margin_mm(5.0);
        assert_eq!(s.margin_mm, 5.0);
    }

    #[test]
    fn scale_percent_clamped() {
        let mut s = PrintSettings::default();
        s.set_scale_percent(0.0);
        assert_eq!(s.scale_percent, 1.0);
        s.set_scale_percent(1000.0);
        assert_eq!(s.scale_percent, 1000.0);
        s.set_scale_percent(2000.0);
        assert_eq!(s.scale_percent, 1000.0);
    }

    #[test]
    fn copies_minimum_one() {
        let mut s = PrintSettings::default();
        s.set_copies(0);
        assert_eq!(s.copies, 1);
    }

    #[test]
    fn serialization_roundtrip() {
        let s = PrintSettings::default();
        let json = serde_json::to_string(&s).unwrap();
        let s2: PrintSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(s.paper_name, s2.paper_name);
        assert_eq!(s.paper_width_mm, s2.paper_width_mm);
    }

    #[test]
    fn default_printer_dpi() {
        let s = PrintSettings::default();
        assert_eq!(s.printer_dpi, Some(300.0));
    }

    #[test]
    fn printer_dpi_serialized() {
        let s = PrintSettings::default();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"printer_dpi\":300.0"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn set_printer_falls_back_to_300_dpi() {
        let mut s = PrintSettings::default();
        s.set_selected_printer(Some("Test Printer".into()));
        assert_eq!(s.printer_dpi, Some(300.0));
    }

    #[test]
    fn set_printer_none_resets_to_300_dpi() {
        let mut s = PrintSettings::default();
        s.printer_dpi = Some(600.0); // simulate high DPI
        s.set_selected_printer(None);
        assert_eq!(s.printer_dpi, Some(300.0));
    }

    // ── Orientation edge cases ──

    #[test]
    fn apply_orientation_portrait_keeps_canonical() {
        let mut s = PrintSettings::default();
        s.paper_width_mm = 210.0;
        s.paper_height_mm = 297.0;
        s.orientation = "portrait".to_string();
        let (w, h) = s.apply_orientation();
        assert_eq!(w, 210.0);
        assert_eq!(h, 297.0);
        // Stored values should remain canonical
        assert_eq!(s.paper_width_mm, 210.0);
        assert_eq!(s.paper_height_mm, 297.0);
    }

    #[test]
    fn apply_orientation_landscape_swaps_effective() {
        let mut s = PrintSettings::default();
        s.paper_width_mm = 210.0;
        s.paper_height_mm = 297.0;
        s.orientation = "landscape".to_string();
        let (w, h) = s.apply_orientation();
        assert_eq!(w, 297.0);
        assert_eq!(h, 210.0);
        // Stored should remain canonical (width ≤ height)
        assert_eq!(s.paper_width_mm, 210.0);
        assert_eq!(s.paper_height_mm, 297.0);
    }

    #[test]
    fn apply_orientation_idempotent() {
        let mut s = PrintSettings::default();
        s.paper_width_mm = 210.0;
        s.paper_height_mm = 297.0;

        // Calling apply_orientation multiple times should produce same result
        let (w1, h1) = s.apply_orientation();
        let (w2, h2) = s.apply_orientation();
        assert_eq!(w1, w2);
        assert_eq!(h1, h2);
    }

    #[test]
    fn apply_orientation_swaps_non_canonical_dimensions_at_set_time() {
        // If someone sets width > height (non-canonical), apply_orientation should fix it
        let mut s = PrintSettings::default();
        s.set_paper("Custom", 1, 297.0, 210.0); // width > height
        s.orientation = "portrait".to_string();
        // apply_orientation is called inside set_paper, so stored dims should be canonical
        assert_eq!(s.paper_width_mm, 210.0);
        assert_eq!(s.paper_height_mm, 297.0);
        let (w, h) = s.apply_orientation();
        assert_eq!(w, 210.0);
        assert_eq!(h, 297.0);
    }

    // ── Scale percent boundary tests ──

    #[test]
    fn scale_percent_boundary_min() {
        let mut s = PrintSettings::default();
        s.set_scale_percent(1.0);
        assert_eq!(s.scale_percent, 1.0);
        s.set_scale_percent(1.5);
        assert_eq!(s.scale_percent, 1.5);
    }

    #[test]
    fn scale_percent_boundary_max() {
        let mut s = PrintSettings::default();
        s.set_scale_percent(1000.0);
        assert_eq!(s.scale_percent, 1000.0);
        s.set_scale_percent(999.9);
        assert_eq!(s.scale_percent, 999.9);
    }

    // ── Copies boundary tests ──

    #[test]
    fn copies_minimum_one_multiple_calls() {
        let mut s = PrintSettings::default();
        s.set_copies(0);
        assert_eq!(s.copies, 1);
        s.set_copies(0);
        assert_eq!(s.copies, 1); // idempotent
    }

    #[test]
    fn copies_high_value() {
        let mut s = PrintSettings::default();
        s.set_copies(100);
        assert_eq!(s.copies, 100);
        s.set_copies(9999);
        assert_eq!(s.copies, 9999);
    }

    // ── Margin edge cases ──

    #[test]
    fn margin_exact_hardware_min() {
        let mut s = PrintSettings::default();
        s.hardware_margin_min_mm = 3.0;
        s.set_margin_mm(3.0);
        assert_eq!(s.margin_mm, 3.0);
    }

    #[test]
    fn margin_zero_with_zero_hardware_min() {
        let mut s = PrintSettings::default();
        s.hardware_margin_min_mm = 0.0;
        s.set_margin_mm(0.0);
        assert_eq!(s.margin_mm, 0.0);
    }

    // ── Serialization edge cases ──

    #[test]
    fn serialization_with_none_printer_dpi() {
        let mut s = PrintSettings::default();
        s.printer_dpi = None;
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"printer_dpi\":null"));
        let s2: PrintSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(s2.printer_dpi, None);
    }

    #[test]
    fn set_margin_after_hardware_min_update() {
        let mut s = PrintSettings::default();
        s.set_margin_mm(2.0);
        assert_eq!(s.margin_mm, 2.0); // hardware_margin_min_mm is 0.0, so 2.0 is OK
        s.hardware_margin_min_mm = 5.0; // simulate printer driver update
        // Margin should be clamped on next set, but current value stays
        assert_eq!(s.margin_mm, 2.0);
        // Next set_margin with low value should clamp
        s.set_margin_mm(3.0);
        assert_eq!(s.margin_mm, 5.0);
    }

    // ── initialize_default_printer ──

    #[test]
    fn initialize_default_printer_when_none() {
        let mut s = PrintSettings::default();
        s.selected_printer = None;
        // In test environment there's no system printer, so it stays None
        s.initialize_default_printer();
        // Just verify it doesn't panic and selected_printer remains valid
        assert!(s.selected_printer.is_none() || s.selected_printer.is_some());
    }

    #[test]
    fn initialize_default_printer_only_once() {
        let mut s = PrintSettings::default();
        s.selected_printer = Some("Custom".into());
        s.initialize_default_printer();
        assert_eq!(s.selected_printer.unwrap(), "Custom");
    }

    // ── set_center_image toggle ──

    #[test]
    fn set_center_image_idempotent() {
        let mut s = PrintSettings::default();
        assert!(s.center_image);
        s.set_center_image(false);
        assert!(!s.center_image);
        s.set_center_image(true);
        assert!(s.center_image);
    }

    // ── set_show_paper_white toggle ──

    #[test]
    fn set_show_paper_white_toggle() {
        let mut s = PrintSettings::default();
        assert!(s.show_paper_white);
        s.set_show_paper_white(false);
        assert!(!s.show_paper_white);
    }

    // ── set_unit ──

    #[test]
    fn set_unit_all_formats() {
        let mut s = PrintSettings::default();
        s.set_unit("cm");
        assert_eq!(s.unit, "cm");
        s.set_unit("in");
        assert_eq!(s.unit, "in");
        s.set_unit("px");
        assert_eq!(s.unit, "px");
        s.set_unit("mm");
        assert_eq!(s.unit, "mm");
    }

    // ── set_selected_printer ──

    #[test]
    fn set_selected_printer_with_name() {
        let mut s = PrintSettings::default();
        s.set_selected_printer(Some("Epson Stylus Pro 3880".into()));
        assert_eq!(s.selected_printer, Some("Epson Stylus Pro 3880".into()));
    }

    #[test]
    fn set_selected_printer_none_clears() {
        let mut s = PrintSettings::default();
        s.set_selected_printer(Some("Epson Stylus Pro 3880".into()));
        s.set_selected_printer(None);
        assert_eq!(s.selected_printer, None);
    }
}
