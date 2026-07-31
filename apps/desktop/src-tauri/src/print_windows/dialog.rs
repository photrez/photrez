// SPDX-License-Identifier: AGPL-3.0-or-later
use super::ffi::*;
use super::query::{lookup_paper_name_by_index, query_paper_size_by_index};
use std::ffi::c_void;

/// Show the printer driver's native settings dialog via DocumentPropertiesW
/// with DM_IN_PROMPT. If the user changes paper size/orientation and clicks OK,
/// the modified DEVMODE is read back and the new settings are returned.
/// Returns (paper_name, width_mm, height_mm, orientation, paper_index)
/// or None if user cancelled.
///
/// Unlike the previous implementation, this function PRE-SETS the DEVMODE
/// with the caller's paper_index and orientation BEFORE showing the dialog,
/// so the native dialog reflects Photrez's current paper selection (BUG-01).
///
/// After the dialog returns, dmFormName is read back from DEVMODE for the
/// paper name (instead of using a hardcoded preset map). If dimensions are 0
/// (some drivers don't set dmPaperWidth/dmPaperLength for standard sizes),
/// a fallback query via DeviceCapabilitiesW DC_PAPERSIZE is performed (BUG-09).
pub(crate) fn show_printer_settings_dialog(
    printer_system_name: &str,
    hwnd: isize,
    paper_index: i16,
    paper_width_mm: f64,
    paper_height_mm: f64,
    orientation: &str,
) -> Result<Option<(String, f64, f64, String, i16)>, String> {
    let printer_wide: Vec<u16> = printer_system_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // ── Open printer ─────────────────────────────────────────────
    let mut h_printer: isize = 0;
    let ret = unsafe { OpenPrinterW(printer_wide.as_ptr(), &mut h_printer, std::ptr::null()) };
    if ret == 0 || h_printer == 0 {
        return Err(format!("Failed to open printer{}", win32_err()));
    }

    // ── Get DEVMODE buffer size ──────────────────────────────────
    let dm_size = unsafe {
        DocumentPropertiesW(
            0,
            h_printer,
            printer_wide.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    if dm_size <= 0 {
        let err = win32_err();
        unsafe { ClosePrinter(h_printer) };
        return Err(format!("Failed to get DEVMODE size{}", err));
    }

    // ── Allocate and populate initial DEVMODE ───────────────────
    let mut devmode_buf = vec![0u8; dm_size as usize];
    let p_devmode = devmode_buf.as_mut_ptr() as *mut DEVMODEW;

    let ret = unsafe {
        DocumentPropertiesW(
            0,
            h_printer,
            printer_wide.as_ptr(),
            p_devmode as *mut c_void,
            std::ptr::null_mut(),
            DM_OUT_BUFFER,
        )
    };
    if ret <= 0 {
        let err = win32_err();
        unsafe { ClosePrinter(h_printer) };
        return Err(format!("Failed to read DEVMODE{}", err));
    }

    // ── Pre-set paper size + orientation in DEVMODE ────────────
    // This fixes BUG-01: the native dialog now reflects Photrez's selection.
    unsafe {
        (*p_devmode).dmFields |= DM_PAPERSIZE | DM_ORIENTATION;
        (*p_devmode).dmPaperSize = paper_index;
        (*p_devmode).dmOrientation = if orientation == "landscape" {
            DMORIENT_LANDSCAPE
        } else {
            DMORIENT_PORTRAIT
        };
        if paper_index == DMPAPER_USER {
            // Custom paper: set explicit dimensions in tenths of mm.
            // Clamp to i16 range to prevent overflow on sizes > 3276mm.
            (*p_devmode).dmFields |= DM_PAPERLENGTH | DM_PAPERWIDTH;
            (*p_devmode).dmPaperWidth = (paper_width_mm * 10.0)
                .round()
                .clamp(i16::MIN as f64, i16::MAX as f64)
                as i16;
            (*p_devmode).dmPaperLength = (paper_height_mm * 10.0)
                .round()
                .clamp(i16::MIN as f64, i16::MAX as f64)
                as i16;
        }
    }

    // ── Merge with driver (validates the DEVMODE changes) ────────
    // If merge fails, re-read fresh DEVMODE and continue without pre-set.
    let merge_ret = unsafe {
        DocumentPropertiesW(
            0,
            h_printer,
            printer_wide.as_ptr(),
            p_devmode as *mut c_void,
            p_devmode as *mut c_void,
            DM_IN_BUFFER | DM_OUT_BUFFER,
        )
    };
    if merge_ret <= 0 {
        // Merge failed — re-read fresh DEVMODE for the dialog
        let _ = unsafe {
            DocumentPropertiesW(
                0,
                h_printer,
                printer_wide.as_ptr(),
                p_devmode as *mut c_void,
                std::ptr::null_mut(),
                DM_OUT_BUFFER,
            )
        };
    }

    // ── Show the native settings dialog ─────────────────────────
    // The dialog is modal — it blocks until the user clicks OK or Cancel.
    let ret = unsafe {
        DocumentPropertiesW(
            hwnd,
            h_printer,
            printer_wide.as_ptr(),
            p_devmode as *mut c_void,
            p_devmode as *mut c_void,
            DM_IN_PROMPT | DM_IN_BUFFER | DM_OUT_BUFFER,
        )
    };

    unsafe { ClosePrinter(h_printer) };

    if ret <= 0 {
        // User cancelled or dialog failed
        return Ok(None);
    }

    // ── Read modified paper size + orientation from DEVMODE ──────
    unsafe {
        let dm_paper_size = (*p_devmode).dmPaperSize;
        let dm_paper_width = (*p_devmode).dmPaperWidth;
        let dm_paper_length = (*p_devmode).dmPaperLength;
        let dm_orientation = (*p_devmode).dmOrientation;
        let result_orientation = if dm_orientation == DMORIENT_LANDSCAPE {
            "landscape"
        } else {
            "portrait"
        };

        // Read dmFormName for the paper name (instead of hardcoded preset map)
        let form_name = {
            let name_slice = &(*p_devmode).dmFormName;
            let end = name_slice.iter().position(|&c| c == 0).unwrap_or(32);
            let name_str = String::from_utf16_lossy(&name_slice[..end]);
            let trimmed = name_str.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        };

        // BUG-09: Fallback dimension lookup when driver returns 0 for standard sizes
        let (w_mm, h_mm) = if dm_paper_width > 0 && dm_paper_length > 0 {
            (dm_paper_width as f64 / 10.0, dm_paper_length as f64 / 10.0)
        } else {
            query_paper_size_by_index(printer_system_name, dm_paper_size).unwrap_or((0.0, 0.0))
        };

        // Paper name: prefer dmFormName from driver.
        // If dmFormName is empty (some drivers like EPSON L1110 leave it blank),
        // look up the canonical name via DeviceCapabilitiesW(DC_PAPERNAMES).
        let paper_name = if let Some(name) = form_name {
            name
        } else if dm_paper_size > 0 && dm_paper_size != DMPAPER_USER {
            lookup_paper_name_by_index(printer_system_name, dm_paper_size)
                .unwrap_or_else(|| "Custom".to_string())
        } else {
            "Custom".to_string()
        };

        return Ok(Some((
            paper_name,
            w_mm,
            h_mm,
            result_orientation.to_string(),
            dm_paper_size,
        )));
    }
}
