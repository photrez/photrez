//! What the Print dialog says about the printer and the resolution a page will
//! actually land at.

use super::*;

/// What the printer cell reports. A failed enumeration says why, because an
/// unreadable spool and a machine with no printers otherwise look identical;
/// an empty list says there is nothing to print to; a populated list says
/// nothing.
pub(in crate::views::dialogs) fn printer_notice(
    error: Option<&str>,
    printer_count: usize,
) -> Option<String> {
    match error {
        Some(message) => Some(message.to_string()),
        None if printer_count == 0 => Some(t("dialogs.print.noPrinters").to_string()),
        None => None,
    }
}

/// The resolution a document actually lands at once it is laid out on paper:
/// pixels per inch on each axis, and the worse of the two is what the eye
/// judges. A size that has not been worked out yet reads as zero rather than
/// as an infinitely good print.
pub(in crate::views::dialogs) fn effective_ppi(doc_px: (f64, f64), print_mm: (f64, f64)) -> u32 {
    const MM_PER_INCH: f64 = 25.4;
    if print_mm.0 <= 0.0 || print_mm.1 <= 0.0 {
        return 0;
    }
    let per_axis = |px: f64, mm: f64| px / (mm / MM_PER_INCH);
    per_axis(doc_px.0, print_mm.0)
        .min(per_axis(doc_px.1, print_mm.1))
        .round() as u32
}

/// The badge tier for an effective resolution, cut where the design cuts it.
/// The copy is the reference's own literal, which it never runs through the
/// catalog.
pub(in crate::views::dialogs) fn ppi_quality(ppi: u32) -> (&'static str, Hsla, Hsla) {
    if ppi >= 300 {
        ("Optimal (300+ PPI)", color::EMERALD_400, color::EMERALD_950)
    } else if ppi >= 150 {
        (
            "Acceptable (150-299 PPI)",
            color::AMBER_400,
            color::AMBER_950,
        )
    } else {
        (
            "Low resolution (min 150 PPI recommended)",
            color::ROSE_400,
            color::ROSE_950,
        )
    }
}
