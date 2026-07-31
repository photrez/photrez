// SPDX-License-Identifier: AGPL-3.0-or-later
// Windows GDI printing — renders image pixels through the printer driver
// (not RAW spooling). This produces correct output on all printers, unlike
// the `printers` crate's print_file() which sends raw PNG bytes that most
// printer drivers cannot interpret.
//
// Paper dimensions (`paper_width_mm`, `paper_height_mm`) are supplied by the
// frontend (from PrintOptions) rather than queried via GetDeviceCaps, because
// some printer drivers return 0 for PHYSICALWIDTH/PHYSICALHEIGHT.
//
// Paper size synchronisation: the frontend passes a paper preset name (e.g.
// "A4", "Letter") or "Custom". On Windows we use DocumentPropertiesW + DEVMODE
// to tell the printer driver exactly what paper size to expect, so the printer
// does not default to whatever paper is currently selected in its settings.

use std::path::Path;
// Selectively import windows-sys GDI types and constants, but NOT functions
// (we define those locally so we can use our own DEVMODEW with CreateDCW).
use std::ffi::c_void;
use windows_sys::Win32::Graphics::Gdi::{
    DeleteDC, GetDeviceCaps, SetBrushOrgEx, SetStretchBltMode, StretchDIBits, BITMAPINFO,
    BITMAPINFOHEADER, HDC, HORZRES, LOGPIXELSX, LOGPIXELSY, PHYSICALHEIGHT, PHYSICALOFFSETX,
    PHYSICALOFFSETY, PHYSICALWIDTH, VERTRES,
};

/// Capture the last Win32 error code for diagnostic messages.
/// Must be called immediately after a failed Win32 API call.
fn win32_err() -> String {
    let err = std::io::Error::last_os_error();
    format!(" (OS: {})", err)
}

// ── Direct FFI for GDI printing functions ──────────────────────────
// CreateDCW is defined locally (not via windows-sys) so we can pass our
// own DEVMODEW pointer as *const c_void without type conflicts.
#[link(name = "gdi32")]
#[allow(non_snake_case)]
unsafe extern "system" {
    fn CreateDCW(
        lpszDriver: *const u16,
        lpszDevice: *const u16,
        lpszOutput: *const u16,
        lpInitData: *const c_void,
    ) -> HDC;
    fn StartDocW(hdc: HDC, lpdi: *const DOCINFOW) -> i32;
    fn EndDoc(hdc: HDC) -> i32;
    fn StartPage(hdc: HDC) -> i32;
    fn EndPage(hdc: HDC) -> i32;
    fn AbortDoc(hdc: HDC) -> i32;
}

#[repr(C)]
#[allow(non_snake_case)]
struct DOCINFOW {
    cbSize: i32,
    lpszDocName: *const u16,
    lpszOutput: *const u16,
    lpszDatatype: *const u16,
    fwType: u32,
}

// ── Direct FFI for winspool.drv (DEVMODE manipulation + capabilities) ─
#[link(name = "winspool")]
#[allow(non_snake_case)]
unsafe extern "system" {
    fn OpenPrinterW(
        pPrinterName: *const u16,
        phPrinter: *mut isize,
        pDefault: *const c_void,
    ) -> i32;
    fn ClosePrinter(hPrinter: isize) -> i32;
    fn DocumentPropertiesW(
        hWnd: isize,
        hPrinter: isize,
        pDeviceName: *const u16,
        pDevModeOutput: *mut c_void,
        pDevModeInput: *mut c_void,
        fMode: u32,
    ) -> i32;
    fn DeviceCapabilitiesW(
        pDevice: *const u16,
        pPort: *const u16,
        fwCapability: u16,
        pOutput: *mut c_void,
        pDevMode: *const c_void,
    ) -> i32;
}

// ── POINT for DC_PAPERSIZE output ────────────────────────────────
#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}

// ── DEVMODEW (public portion only — enough for paper size fields) ─
#[repr(C)]
#[allow(non_snake_case)]
struct DEVMODEW {
    dmDeviceName: [u16; 32],
    dmSpecVersion: u16,
    dmDriverVersion: u16,
    dmSize: u16,
    dmDriverExtra: u16,
    dmFields: u32,
    dmOrientation: i16,
    dmPaperSize: i16,
    dmPaperLength: i16,
    dmPaperWidth: i16,
    dmScale: i16,
    dmCopies: i16,
    dmDefaultSource: i16,
    dmPrintQuality: i16,
    dmColor: i16,
    dmDuplex: i16,
    dmYResolution: i16,
    dmTTOption: i16,
    dmCollate: i16,
    dmFormName: [u16; 32],
    dmLogPixels: u16,
    dmBitsPerPel: u32,
    dmPelsWidth: u32,
    dmPelsHeight: u32,
    dmDisplayFlags: u32,
    dmDisplayFrequency: u32,
    dmICMMethod: u32,
    dmICMIntent: u32,
    dmMediaType: u32,
    dmDitherType: u32,
    dmReserved1: u32,
    dmReserved2: u32,
    dmPanningWidth: u32,
    dmPanningHeight: u32,
}

// ── DEVMODE constants ──────────────────────────────────────────────
const DM_OUT_BUFFER: u32 = 2;
const DM_IN_BUFFER: u32 = 8;
const DM_IN_PROMPT: u32 = 4;
const DM_PAPERSIZE: u32 = 0x0002;
const DM_PAPERLENGTH: u32 = 0x0004;
const DM_PAPERWIDTH: u32 = 0x0008;

const DMORIENT_LANDSCAPE: i16 = 2;
const DMORIENT_PORTRAIT: i16 = 1;
const DM_ORIENTATION: u32 = 0x0001;

const DC_PAPERSIZE: u16 = 3;
const DC_PAPERNAMES: u16 = 16;
const DC_PAPERS: u16 = 2;

/// Map a DMPAPER_ constant back to a human-readable preset name.
/// DEPRECATED: use dmFormName from DEVMODE instead.
/// Kept temporarily for macOS/Linux compatibility — will be removed when
/// paper_index is fully adopted across all platforms.
const DMPAPER_USER: i16 = 256;

/// Set the paper size in the printer's DEVMODE structure via
/// DocumentPropertiesW, then return a DC created with that DEVMODE.
///
/// This is critical for correct paper-handling: without it the printer DC
/// inherits whatever paper is currently selected in the driver's defaults,
/// which may not match the paper size the user chose in the print dialog.
/// Fallback: open printer DC without DEVMODE when DEVMODE manipulation fails.
/// This lets the printer use its driver-default paper size.
unsafe fn fallback_create_dc(printer_wide: &[u16], error_msg: &str) -> Result<HDC, String> {
    let winspool: Vec<u16> = "WINSPOOL\0".encode_utf16().collect();
    let hdc = CreateDCW(
        winspool.as_ptr(),
        printer_wide.as_ptr(),
        std::ptr::null(),
        std::ptr::null(),
    );
    if hdc.is_null() {
        return Err(format!("{}{}", error_msg, win32_err()));
    }
    Ok(hdc)
}

fn create_printer_dc_with_paper_size(
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

/// Open a printer, get its DEVMODE, and read back the default paper size from the driver's current settings.
/// This is how the frontend learns what paper the printer is actually configured to use.
pub(crate) fn get_default_paper_size_win(
    printer_system_name: &str,
) -> Result<(String, f64, f64), String> {
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

    // ── Allocate and read current DEVMODE ───────────────────────
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

    // ── Read paper size from DEVMODE ────────────────────────────
    unsafe {
        let dm_paper_size = (*p_devmode).dmPaperSize;
        let dm_paper_width = (*p_devmode).dmPaperWidth; // tenths of mm
        let dm_paper_length = (*p_devmode).dmPaperLength; // tenths of mm
                                                          // Read dmFormName for the paper name (driver's name, no hardcoded map)
        let paper_name = {
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
        ClosePrinter(h_printer);

        // If dmPaperSize is a standard constant, prefer dmFormName.
        // If dmFormName is empty (some drivers like EPSON L1110 leave it blank),
        // look up the canonical name via DeviceCapabilitiesW(DC_PAPERNAMES).
        if dm_paper_size > 0 && dm_paper_size != DMPAPER_USER {
            if let Some(name) = paper_name {
                // Try to fill in dimensions from printer driver (some drivers like EPSON L1110
                // report dmFormName but leave dmPaperWidth/dmPaperLength as 0)
                if let Some((w, h)) = query_paper_size_by_index(printer_system_name, dm_paper_size)
                {
                    return Ok((name, w, h));
                }
                return Ok((name, 0.0, 0.0));
            }
            // dmFormName was empty — try looking up by DMPAPER index
            if let Some(looked_up) = lookup_paper_name_by_index(printer_system_name, dm_paper_size)
            {
                if let Some((w, h)) = query_paper_size_by_index(printer_system_name, dm_paper_size)
                {
                    return Ok((looked_up, w, h));
                }
                return Ok((looked_up, 0.0, 0.0));
            }
            // Last resort: return "Custom" with dimensions from DEVMODE
            if dm_paper_width > 0 && dm_paper_length > 0 {
                return Ok((
                    "Custom".to_string(),
                    dm_paper_width as f64 / 10.0,
                    dm_paper_length as f64 / 10.0,
                ));
            }
        }
        // Custom size: return dimensions from DEVMODE (in mm)
        if dm_paper_width > 0 && dm_paper_length > 0 {
            return Ok((
                paper_name.unwrap_or_else(|| "Custom".to_string()),
                dm_paper_width as f64 / 10.0,
                dm_paper_length as f64 / 10.0,
            ));
        }
    }

    Err("No paper size found in DEVMODE".into())
}

/// Query the printer's hardware margins (unprintable area) via GDI GetDeviceCaps.
/// Returns (left_mm, top_mm, right_mm, bottom_mm) or an error.
///
/// On Windows GDI, the printable area is smaller than the physical paper due to
/// mechanical constraints (paper feed rollers, print head path). The unprintable
/// margins are reported via PHYSICALOFFSETX/Y (left/top) and computed from
/// PHYSICALWIDTH - HORZRES - PHYSICALOFFSETX (right) and
/// PHYSICALHEIGHT - VERTRES - PHYSICALOFFSETY (bottom).
///
/// These values define the unprintable area — users cannot place content inside.
pub(crate) fn get_printer_margins_win(
    printer_system_name: &str,
) -> Result<(f64, f64, f64, f64), String> {
    let printer_wide: Vec<u16> = printer_system_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let winspool: Vec<u16> = "WINSPOOL\0".encode_utf16().collect();

    // Open a printer DC (without DEVMODE — margins don't need paper-size-specific DEVMODE)
    let hdc = unsafe {
        CreateDCW(
            winspool.as_ptr(),
            printer_wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if hdc.is_null() {
        return Err(format!("Failed to open printer DC{}", win32_err()));
    }

    unsafe {
        let phys_width = GetDeviceCaps(hdc, PHYSICALWIDTH as i32);
        let phys_height = GetDeviceCaps(hdc, PHYSICALHEIGHT as i32);
        let horz_res = GetDeviceCaps(hdc, HORZRES as i32);
        let vert_res = GetDeviceCaps(hdc, VERTRES as i32);
        let offset_x = GetDeviceCaps(hdc, PHYSICALOFFSETX as i32);
        let offset_y = GetDeviceCaps(hdc, PHYSICALOFFSETY as i32);
        let dpi_x = GetDeviceCaps(hdc, LOGPIXELSX as i32) as f64;
        let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY as i32) as f64;
        DeleteDC(hdc);

        if dpi_x <= 0.0 || dpi_y <= 0.0 {
            return Err("Failed to query printer DPI".into());
        }

        // Convert device units to mm, clamping negatives to 0
        // Some printer drivers (PCL/PostScript) may report PHYSICALWIDTH < HORZRES
        // when borderless mode is off, causing negative right/bottom margins.
        let left_mm = (offset_x as f64 / dpi_x * 25.4).max(0.0);
        let top_mm = (offset_y as f64 / dpi_y * 25.4).max(0.0);
        let right_mm = ((phys_width - horz_res - offset_x) as f64 / dpi_x * 25.4).max(0.0);
        let bottom_mm = ((phys_height - vert_res - offset_y) as f64 / dpi_y * 25.4).max(0.0);

        // Round to 1 decimal place and clamp negatives (some drivers report 0)
        Ok((
            (left_mm * 10.0).round() / 10.0,
            (top_mm * 10.0).round() / 10.0,
            (right_mm * 10.0).round() / 10.0,
            (bottom_mm * 10.0).round() / 10.0,
        ))
    }
}

/// Query the printer for all supported paper sizes using DeviceCapabilitiesW.
/// Returns a vector of (name, width_mm, height_mm, dm_paper_index).
pub(crate) fn get_printer_paper_sizes_win(
    printer_system_name: &str,
) -> Result<Vec<(String, f64, f64, i16)>, String> {
    let printer_wide: Vec<u16> = printer_system_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // ── First, get the number of paper sizes via DC_PAPERSIZE ────
    let count = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERSIZE,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    if count <= 0 {
        // Printer doesn't support capability query or has no paper
        return Ok(Vec::new());
    }

    // ── Allocate buffer for POINT array (each entry = 8 bytes) ──
    let mut points_buf = vec![0u8; (count as usize) * std::mem::size_of::<POINT>()];
    let p_points = points_buf.as_mut_ptr() as *mut POINT;

    let ret = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERSIZE,
            p_points as *mut c_void,
            std::ptr::null(),
        )
    };
    if ret <= 0 {
        return Ok(Vec::new());
    }
    let actual_count = ret as usize;

    // ── Also get DMPAPER index for each size via DC_PAPERS ────────
    let indices_count = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERS,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    let mut indices: Vec<i16> = Vec::new();
    if indices_count > 0 {
        let mut index_buf = vec![0i16; indices_count as usize];
        let ret = unsafe {
            DeviceCapabilitiesW(
                printer_wide.as_ptr(),
                std::ptr::null(),
                DC_PAPERS,
                index_buf.as_mut_ptr() as *mut c_void,
                std::ptr::null(),
            )
        };
        if ret > 0 {
            indices = index_buf[..(ret as usize).min(indices_count as usize)].to_vec();
        }
    }

    // ── Get paper names (64-char wide strings each) ────────────────
    let names_count = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERNAMES,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    let mut names: Vec<String> = Vec::new();
    if names_count > 0 {
        let _name_buf_size = (names_count as usize) * 64 * 2; // 64 WCHAR per entry (size in bytes)
        let mut name_buf = vec![0u16; (names_count as usize) * 64];
        let ret = unsafe {
            DeviceCapabilitiesW(
                printer_wide.as_ptr(),
                std::ptr::null(),
                DC_PAPERNAMES,
                name_buf.as_mut_ptr() as *mut c_void,
                std::ptr::null(),
            )
        };
        if ret > 0 {
            for i in 0..(ret as usize).min(names_count as usize) {
                let start = i * 64;
                // Find null terminator within the 64-char block
                let end = name_buf[start..start + 64]
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(64);
                let name_str = String::from_utf16_lossy(&name_buf[start..start + end]);
                names.push(name_str.trim().to_string());
            }
        }
    }

    // ── Build result from points + indices ──────────────────────
    let mut result: Vec<(String, f64, f64, i16)> = Vec::new();
    let points = unsafe { std::slice::from_raw_parts(p_points, actual_count) };
    for (i, pt) in points.iter().enumerate() {
        let width_mm = pt.x as f64 / 10.0;
        let height_mm = pt.y as f64 / 10.0;
        let name = if i < names.len() {
            names[i].clone()
        } else {
            format!("{:.1}×{:.1} mm", width_mm, height_mm)
        };
        let dm_index = indices.get(i).copied().unwrap_or(DMPAPER_USER);
        result.push((name, width_mm, height_mm, dm_index));
    }

    Ok(result)
}

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

/// Look up the paper name by DMPAPER index using DeviceCapabilitiesW(DC_PAPERNAMES / DC_PAPERS).
/// Returns None when the name cannot be determined (driver doesn't populate the name list,
/// index not found, or count mismatch).
///
/// Motivation: Some printer drivers (e.g. EPSON L1110) leave dmFormName empty in DEVMODE
/// for standard paper sizes, causing the paper name to fall back to "Custom". This helper
/// queries the driver's paper name list directly to get the canonical name.
fn lookup_paper_name_by_index(printer_system_name: &str, target_index: i16) -> Option<String> {
    let printer_wide: Vec<u16> = printer_system_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // Get paper names via DC_PAPERNAMES
    let names_count = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERNAMES,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    if names_count <= 0 {
        return None;
    }

    let mut name_buf = vec![0u16; (names_count as usize) * 64];
    let names_ret = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERNAMES,
            name_buf.as_mut_ptr() as *mut c_void,
            std::ptr::null(),
        )
    };
    if names_ret <= 0 {
        return None;
    }
    let actual_names = names_ret as usize;

    // Get DMPAPER indices via DC_PAPERS
    let indices_count = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERS,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    let mut indices: Vec<i16> = Vec::new();
    if indices_count > 0 {
        let mut index_buf = vec![0i16; indices_count as usize];
        let indices_ret = unsafe {
            DeviceCapabilitiesW(
                printer_wide.as_ptr(),
                std::ptr::null(),
                DC_PAPERS,
                index_buf.as_mut_ptr() as *mut c_void,
                std::ptr::null(),
            )
        };
        if indices_ret > 0 {
            indices = index_buf[..(indices_ret as usize).min(indices_count as usize)].to_vec();
        }
    }

    // Match target_index to find corresponding name.
    // DC_PAPERNAMES and DC_PAPERS must have matching counts; if they don't,
    // the name at a given position may not correspond to the index at the same position,
    // so we return None for safety.
    if actual_names != indices.len() {
        return None;
    }

    for (i, name_start) in (0..actual_names).map(|i| i * 64).enumerate() {
        let dm_index = indices.get(i).copied().unwrap_or(DMPAPER_USER);
        if dm_index == target_index {
            let end = name_buf[name_start..name_start + 64]
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(64);
            let name = String::from_utf16_lossy(&name_buf[name_start..name_start + end]);
            let trimmed = name.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
            return None;
        }
    }
    None
}

/// Query printer driver for paper dimensions by DMPAPER index.
/// Used as fallback (BUG-09) when DEVMODE returns 0 for dmPaperWidth/dmPaperLength
/// (some drivers set dmPaperSize but not dmPaperWidth/dmPaperLength for standard sizes).
fn query_paper_size_by_index(printer_system_name: &str, target_index: i16) -> Option<(f64, f64)> {
    let printer_wide: Vec<u16> = printer_system_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // Get number of paper sizes via DC_PAPERSIZE
    let count = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERSIZE,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    if count <= 0 {
        return None;
    }

    // Allocate buffer for POINT array (each entry = 8 bytes)
    let mut points_buf = vec![0u8; (count as usize) * std::mem::size_of::<POINT>()];
    let p_points = points_buf.as_mut_ptr() as *mut POINT;

    let ret = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERSIZE,
            p_points as *mut c_void,
            std::ptr::null(),
        )
    };
    if ret <= 0 {
        return None;
    }
    let actual_count = ret as usize;

    // Get DMPAPER indices via DC_PAPERS
    let indices_count = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERS,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    let mut indices: Vec<i16> = Vec::new();
    if indices_count > 0 {
        let mut index_buf = vec![0i16; indices_count as usize];
        let ret = unsafe {
            DeviceCapabilitiesW(
                printer_wide.as_ptr(),
                std::ptr::null(),
                DC_PAPERS,
                index_buf.as_mut_ptr() as *mut c_void,
                std::ptr::null(),
            )
        };
        if ret > 0 {
            indices = index_buf[..(ret as usize).min(indices_count as usize)].to_vec();
        }
    }

    // Match target_index to find corresponding dimensions
    let points = unsafe { std::slice::from_raw_parts(p_points, actual_count) };
    for (i, pt) in points.iter().enumerate() {
        let dm_index = indices.get(i).copied().unwrap_or(DMPAPER_USER);
        if dm_index == target_index {
            return Some((pt.x as f64 / 10.0, pt.y as f64 / 10.0));
        }
    }
    None
}

/// Print an image file to a Windows printer using GDI.
///
/// 1. Decodes the image to RGBA pixels (via `image` crate)
/// 2. Opens a printer DC via CreateDCW (with DEVMODE paper size set)
/// 3. Queries printer DPI via GetDeviceCaps(LOGPIXELSX/Y) for correct physical sizing
/// 4. Compensates for unprintable hardware margins via PHYSICALOFFSETX/Y
/// 5. Renders each copy via StretchDIBits (scaled+centered, HALFTONE smoothing)
/// Shared GDI rendering pipeline: takes already-decoded R↔B-swapped RGBA pixels
/// and renders them to the printer via GDI. Handles BITMAPINFO, printer DC setup,
/// DPI query, margin inset, StretchDIBits, document/page lifecycle,
/// and copy iteration. Called by `print_image_via_gdi` (file) and
/// `print_image_raw` (raw bytes).
///
/// The frontend composites the image within the margin-inset printable area
/// and sends raw RGBA.  This function positions the canvas at the correct
/// physical offset (hardware margin + user margin).  No additional centering
/// is applied — the frontend already handles centering or manual offset.
pub(crate) fn render_rgba_to_printer(
    pixels: &[u8],
    width: u32,
    height: u32,
    printer_system_name: &str,
    copies: u32,
    paper_width_mm: f64,
    paper_height_mm: f64,
    margin_left_mm: f64,
    margin_right_mm: f64,
    margin_top_mm: f64,
    margin_bottom_mm: f64,
    paper_index: i16,
    document_name: &str,
    orientation: &str,
) -> Result<(), String> {
    // ── 1. Build BITMAPINFO (32-bit BI_RGB, top-down) ──────────
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = width as i32;
    bmi.bmiHeader.biHeight = -(height as i32);
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;

    // ── 2. Create printer DC with DEVMODE paper size ───────────
    let printer_wide: Vec<u16> = printer_system_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let hdc = create_printer_dc_with_paper_size(
        &printer_wide,
        paper_index,
        paper_width_mm,
        paper_height_mm,
        orientation,
    )?;

    // ── 3. Query printer DPI + page dimensions ─────────────────
    const MM_PER_INCH: f64 = 25.4;
    let printer_dpi_x = unsafe { GetDeviceCaps(hdc, LOGPIXELSX as i32) } as f64;
    let printer_dpi_y = unsafe { GetDeviceCaps(hdc, LOGPIXELSY as i32) } as f64;
    let page_w = (paper_width_mm / MM_PER_INCH * printer_dpi_x).round() as i32;
    let page_h = (paper_height_mm / MM_PER_INCH * printer_dpi_y).round() as i32;

    if page_w <= 0 || page_h <= 0 {
        unsafe { DeleteDC(hdc) };
        return Err("Invalid paper dimensions".into());
    }

    // ── 4. Query hardware margins ──────────────────────────────
    let offset_x = unsafe { GetDeviceCaps(hdc, PHYSICALOFFSETX as i32) };
    let offset_y = unsafe { GetDeviceCaps(hdc, PHYSICALOFFSETY as i32) };

    // ── 5. StartDocW ───────────────────────────────────────────
    let doc_name_wide: Vec<u16> = format!("{}\0", document_name).encode_utf16().collect();
    let doc_info = DOCINFOW {
        cbSize: std::mem::size_of::<DOCINFOW>() as i32,
        lpszDocName: doc_name_wide.as_ptr(),
        lpszOutput: std::ptr::null(),
        lpszDatatype: std::ptr::null(),
        fwType: 0,
    };
    let job_id = unsafe { StartDocW(hdc, &doc_info) };
    if job_id <= 0 {
        let err = win32_err();
        unsafe { DeleteDC(hdc) };
        return Err(format!("Failed to start print document{}", err));
    }

    // ── 6. Compute per-side margins in printer pixels ──────────
    let margin_left_px = (margin_left_mm / MM_PER_INCH * printer_dpi_x).round() as i32;
    let margin_right_px = (margin_right_mm / MM_PER_INCH * printer_dpi_x).round() as i32;
    let margin_top_px = (margin_top_mm / MM_PER_INCH * printer_dpi_y).round() as i32;
    let margin_bottom_px = (margin_bottom_mm / MM_PER_INCH * printer_dpi_y).round() as i32;
    let effective_left = margin_left_px.max(0);
    let effective_right = margin_right_px.max(0);
    let effective_top = margin_top_px.max(0);
    let effective_bottom = margin_bottom_px.max(0);
    let printable_w = page_w - effective_left - effective_right;
    let printable_h = page_h - effective_top - effective_bottom;

    // ── 7. Scale source canvas to fill printable area ──────────
    // Frontend composites at target DPI to a canvas matching these printable
    // dimensions, so dimensions should match when DPIs are close.
    // When they match exactly, use 1:1 fast path (no GDI interpolation).
    let (dest_w, dest_h) = if width as i32 == printable_w && height as i32 == printable_h {
        (width as i32, height as i32)
    } else {
        let scale_x = printable_w as f64 / width as f64;
        let scale_y = printable_h as f64 / height as f64;
        let scale = scale_x.min(scale_y);
        (
            (width as f64 * scale) as i32,
            (height as f64 * scale) as i32,
        )
    };

    // Position at hardware offset + per-side margin.
    // Frontend handles centering/manual-offset within the canvas.
    let dest_x = offset_x + effective_left;
    let dest_y = offset_y + effective_top;

    // ── 8. Set HALFTONE stretch mode (only if scaling) ────────
    let needs_scale = dest_w != width as i32 || dest_h != height as i32;
    if needs_scale {
        unsafe { SetStretchBltMode(hdc, 4) };
        unsafe { SetBrushOrgEx(hdc, 0, 0, std::ptr::null_mut()) };
    }

    // ── 9. Render each copy ────────────────────────────────────
    let mut render_err: Option<String> = None;
    let count = copies.max(1);
    for _ in 0..count {
        if unsafe { StartPage(hdc) } <= 0 {
            let err = win32_err();
            unsafe { AbortDoc(hdc) };
            unsafe { DeleteDC(hdc) };
            return Err(format!("Failed to start page{}", err));
        }

        let lines = unsafe {
            StretchDIBits(
                hdc,
                dest_x,
                dest_y,
                dest_w,
                dest_h,
                0,
                0,
                width as i32,
                height as i32,
                pixels.as_ptr() as *const c_void,
                &bmi as *const BITMAPINFO,
                0,
                0x00CC0020,
            )
        };

        if lines <= 0 {
            let err = win32_err();
            unsafe { AbortDoc(hdc) };
            unsafe { DeleteDC(hdc) };
            return Err(format!("StretchDIBits failed{}", err));
        }

        if unsafe { EndPage(hdc) } <= 0 {
            let err = win32_err();
            render_err = Some(format!("Failed to end page{}", err));
            break;
        }
    }

    // ── 10. End document + cleanup ─────────────────────────────
    if render_err.is_some() {
        unsafe { AbortDoc(hdc) };
    } else {
        unsafe { EndDoc(hdc) };
    }
    unsafe { DeleteDC(hdc) };

    render_err.map(Err).unwrap_or(Ok(()))
}

/// Print via GDI — decodes a PNG from file, then delegates to
/// `render_rgba_to_printer`.
pub(crate) fn print_image_via_gdi(
    path: &Path,
    printer_system_name: &str,
    copies: u32,
    paper_width_mm: f64,
    paper_height_mm: f64,
    margin_mm: f64,
    paper_index: i16,
    document_name: &str,
    orientation: &str,
) -> Result<(), String> {
    let img = image::open(path).map_err(|e| format!("Failed to decode image: {e}"))?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut pixels = rgba.into_raw();
    for px in pixels.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    render_rgba_to_printer(
        &pixels,
        width,
        height,
        printer_system_name,
        copies,
        paper_width_mm,
        paper_height_mm,
        margin_mm,
        margin_mm,
        margin_mm,
        margin_mm,
        paper_index,
        document_name,
        orientation,
    )
}

/// Query the printer's native DPI via GDI `GetDeviceCaps(LOGPIXELSX/Y)`.
///
/// Opens a temporary printer DC (without DEVMODE — DPI is a device cap,
/// not a per-job setting), reads the DPI, and closes the DC.
///
/// Returns `None` if the printer cannot be opened or the DPI is 0.
pub(crate) fn query_printer_dpi_win(printer_system_name: &str) -> Option<f64> {
    let printer_wide: Vec<u16> = printer_system_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let winspool: Vec<u16> = "WINSPOOL\0".encode_utf16().collect();

    let hdc = unsafe {
        CreateDCW(
            winspool.as_ptr(),
            printer_wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if hdc.is_null() {
        return None;
    }

    let dpi = unsafe { GetDeviceCaps(hdc, LOGPIXELSX as i32) } as f64;
    unsafe { DeleteDC(hdc) };

    if dpi > 0.0 {
        Some(dpi)
    } else {
        None
    }
}
