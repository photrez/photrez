// SPDX-License-Identifier: AGPL-3.0-or-later
use super::ffi::*;
use std::ffi::c_void;
use windows_sys::Win32::Graphics::Gdi::HDC;

pub(crate) fn create_printer_dc_with_paper_size(
    printer_wide: &[u16],
    paper_index: i16,
    paper_width_mm: f64,
    paper_height_mm: f64,
    orientation: &str,
) -> Result<HDC, String> {
    // ── Open printer ─────────────────────────────────────────────
    let mut h_printer: isize = 0;
    let ret = unsafe { OpenPrinterW(printer_wide.as_ptr(), &mut h_printer, std::ptr::null()) };
    if ret == 0 || h_printer == 0 {
        // Fall back to CreateDCW without DEVMODE
        let err = win32_err();
        return unsafe {
            fallback_create_dc(
                printer_wide,
                &format!("Failed to open printer (no DEVMODE fallback){}", err),
            )
        };
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
        return unsafe {
            fallback_create_dc(
                printer_wide,
                &format!("Failed to get DEVMODE size (no DEVMODE fallback){}", err),
            )
        };
    }

    // ── Allocate and initialise DEVMODE ─────────────────────────
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
        return unsafe {
            fallback_create_dc(
                printer_wide,
                &format!("Failed to initialise DEVMODE (no DEVMODE fallback){}", err),
            )
        };
    }

    // ── Set paper size + orientation fields ──────────────────────
    // dmOrientation must be set so the printer driver knows the page
    // orientation — without it the DC defaults to portrait regardless
    // of the paper dimensions. The frontend sends EFFECTIVE dimensions
    // (swapped for landscape), so dmPaperWidth = landscape width (long
    // edge), dmPaperLength = landscape height (short edge).
    unsafe {
        (*p_devmode).dmFields |= DM_ORIENTATION;
        (*p_devmode).dmOrientation = if orientation == "landscape" {
            DMORIENT_LANDSCAPE
        } else {
            DMORIENT_PORTRAIT
        };
        (*p_devmode).dmFields |= DM_PAPERSIZE;
        (*p_devmode).dmPaperSize = paper_index;
        if paper_index == DMPAPER_USER {
            // dmPaperWidth and dmPaperLength are in TENTHS of a millimeter (i16).
            // Clamp to i16 range to prevent overflow on custom sizes > 3276mm.
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

    // ── Merge changes with driver ────────────────────────────────
    let ret = unsafe {
        DocumentPropertiesW(
            0,
            h_printer,
            printer_wide.as_ptr(),
            p_devmode as *mut c_void,
            p_devmode as *mut c_void,
            DM_IN_BUFFER | DM_OUT_BUFFER,
        )
    };
    if ret <= 0 {
        // Merge failed — proceed with default DEVMODE
        let err = win32_err();
        unsafe { ClosePrinter(h_printer) };
        return unsafe {
            fallback_create_dc(
                printer_wide,
                &format!("Failed to merge DEVMODE (no DEVMODE fallback){}", err),
            )
        };
    }

    // ── Create printer DC with the modified DEVMODE ─────────────
    let winspool: Vec<u16> = "WINSPOOL\0".encode_utf16().collect();
    let hdc = unsafe {
        CreateDCW(
            winspool.as_ptr(),
            printer_wide.as_ptr(),
            std::ptr::null(),
            p_devmode as *const c_void,
        )
    };
    unsafe { ClosePrinter(h_printer) };

    if hdc.is_null() {
        return Err(format!(
            "Failed to open printer DC with DEVMODE{}",
            win32_err()
        ));
    }
    Ok(hdc)
}
