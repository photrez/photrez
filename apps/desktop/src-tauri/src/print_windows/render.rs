// SPDX-License-Identifier: AGPL-3.0-or-later
use super::devmode::create_printer_dc_with_paper_size;
use super::ffi::*;
use std::ffi::c_void;
use std::path::Path;
use windows_sys::Win32::Graphics::Gdi::{
    DeleteDC, GetDeviceCaps, SetBrushOrgEx, SetStretchBltMode, StretchDIBits, BITMAPINFO,
    BITMAPINFOHEADER, LOGPIXELSX, LOGPIXELSY, PHYSICALOFFSETX, PHYSICALOFFSETY,
};

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
