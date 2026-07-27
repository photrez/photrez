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
}
