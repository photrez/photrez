// SPDX-License-Identifier: AGPL-3.0-or-later
use super::ffi::*;
use std::ffi::c_void;
use windows_sys::Win32::Graphics::Gdi::{
    DeleteDC, GetDeviceCaps, HORZRES, LOGPIXELSX, LOGPIXELSY, PHYSICALHEIGHT, PHYSICALOFFSETX,
    PHYSICALOFFSETY, PHYSICALWIDTH, VERTRES,
};

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
/// index not found, or count mismatch).
///
/// Motivation: Some printer drivers (e.g. EPSON L1110) leave dmFormName empty in DEVMODE
/// for standard paper sizes, causing the paper name to fall back to "Custom". This helper
/// queries the driver's paper name list directly to get the canonical name.
pub(crate) fn lookup_paper_name_by_index(
    printer_system_name: &str,
    target_index: i16,
) -> Option<String> {
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
pub(crate) fn query_paper_size_by_index(
    printer_system_name: &str,
    target_index: i16,
) -> Option<(f64, f64)> {
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
