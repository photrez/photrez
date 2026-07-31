// SPDX-License-Identifier: AGPL-3.0-or-later
// Windows GDI printing - renders image pixels through the printer driver
// (not RAW spooling). This produces correct output on all printers, unlike
// the printers crate's print_file() which sends raw PNG bytes that most
// printer drivers cannot interpret.
//
// Paper dimensions (paper_width_mm, paper_height_mm) are supplied by the
// frontend (from PrintOptions) rather than queried via GetDeviceCaps, because
// some printer drivers return 0 for PHYSICALWIDTH/PHYSICALHEIGHT.
//
// Paper size synchronisation: the frontend passes a paper preset name (e.g.
// "A4", "Letter") or "Custom". On Windows we use DocumentPropertiesW + DEVMODE
// to tell the printer driver exactly what paper size to expect, so the printer
// does not default to whatever paper is currently selected in its settings.
//
// Module layout (split from the former 1250-LOC print_windows.rs, #20):
// - ffi.rs:    Win32 FFI declarations, DEVMODEW/POINT structs, constants
// - devmode.rs: DEVMODE-based printer DC creation
// - query.rs:  paper size / margin / paper-name queries
// - dialog.rs: native printer settings dialog
// - render.rs: RGBA rendering, GDI print, DPI query

mod devmode;
mod dialog;
pub(crate) mod ffi;
mod query;
mod render;

pub(crate) use dialog::show_printer_settings_dialog;
pub(crate) use query::{
    get_default_paper_size_win, get_printer_margins_win, get_printer_paper_sizes_win,
};
pub(crate) use render::{print_image_via_gdi, query_printer_dpi_win, render_rgba_to_printer};
