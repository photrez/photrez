// SPDX-License-Identifier: AGPL-3.0-or-later
// Win32 FFI layer for GDI printing. windows-sys GDI types and constants are
// imported selectively, but the functions themselves are declared locally
// (not via windows-sys) so we can pass our own DEVMODEW with CreateDCW.

// Selectively import windows-sys GDI types and constants, but NOT functions
// (we define those locally so we can use our own DEVMODEW with CreateDCW).
use std::ffi::c_void;

/// Capture the last Win32 error code for diagnostic messages.
/// Must be called immediately after a failed Win32 API call.
pub(crate) fn win32_err() -> String {
    let err = std::io::Error::last_os_error();
    format!(" (OS: {})", err)
}

use windows_sys::Win32::Graphics::Gdi::HDC;

// ── Direct FFI for GDI printing functions ──────────────────────────
// CreateDCW is defined locally (not via windows-sys) so we can pass our
// own DEVMODEW pointer as *const c_void without type conflicts.
#[link(name = "gdi32")]
#[allow(non_snake_case)]
unsafe extern "system" {
    pub(crate) fn CreateDCW(
        lpszDriver: *const u16,
        lpszDevice: *const u16,
        lpszOutput: *const u16,
        lpInitData: *const c_void,
    ) -> HDC;
    pub(crate) fn StartDocW(hdc: HDC, lpdi: *const DOCINFOW) -> i32;
    pub(crate) fn EndDoc(hdc: HDC) -> i32;
    pub(crate) fn StartPage(hdc: HDC) -> i32;
    pub(crate) fn EndPage(hdc: HDC) -> i32;
    pub(crate) fn AbortDoc(hdc: HDC) -> i32;
}

#[repr(C)]
#[allow(non_snake_case)]
pub(crate) struct DOCINFOW {
    pub(crate) cbSize: i32,
    pub(crate) lpszDocName: *const u16,
    pub(crate) lpszOutput: *const u16,
    pub(crate) lpszDatatype: *const u16,
    pub(crate) fwType: u32,
}

// ── Direct FFI for winspool.drv (DEVMODE manipulation + capabilities) ─
#[link(name = "winspool")]
#[allow(non_snake_case)]
unsafe extern "system" {
    pub(crate) fn OpenPrinterW(
        pPrinterName: *const u16,
        phPrinter: *mut isize,
        pDefault: *const c_void,
    ) -> i32;
    pub(crate) fn ClosePrinter(hPrinter: isize) -> i32;
    pub(crate) fn DocumentPropertiesW(
        hWnd: isize,
        hPrinter: isize,
        pDeviceName: *const u16,
        pDevModeOutput: *mut c_void,
        pDevModeInput: *mut c_void,
        fMode: u32,
    ) -> i32;
    pub(crate) fn DeviceCapabilitiesW(
        pDevice: *const u16,
        pPort: *const u16,
        fwCapability: u16,
        pOutput: *mut c_void,
        pDevMode: *const c_void,
    ) -> i32;
}

// ── POINT for DC_PAPERSIZE output ────────────────────────────────
#[repr(C)]
pub(crate) struct POINT {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

// ── DEVMODEW (public portion only — enough for paper size fields) ─
#[repr(C)]
#[allow(non_snake_case)]
pub(crate) struct DEVMODEW {
    pub(crate) dmDeviceName: [u16; 32],
    pub(crate) dmSpecVersion: u16,
    pub(crate) dmDriverVersion: u16,
    pub(crate) dmSize: u16,
    pub(crate) dmDriverExtra: u16,
    pub(crate) dmFields: u32,
    pub(crate) dmOrientation: i16,
    pub(crate) dmPaperSize: i16,
    pub(crate) dmPaperLength: i16,
    pub(crate) dmPaperWidth: i16,
    pub(crate) dmScale: i16,
    pub(crate) dmCopies: i16,
    pub(crate) dmDefaultSource: i16,
    pub(crate) dmPrintQuality: i16,
    pub(crate) dmColor: i16,
    pub(crate) dmDuplex: i16,
    pub(crate) dmYResolution: i16,
    pub(crate) dmTTOption: i16,
    pub(crate) dmCollate: i16,
    pub(crate) dmFormName: [u16; 32],
    pub(crate) dmLogPixels: u16,
    pub(crate) dmBitsPerPel: u32,
    pub(crate) dmPelsWidth: u32,
    pub(crate) dmPelsHeight: u32,
    pub(crate) dmDisplayFlags: u32,
    pub(crate) dmDisplayFrequency: u32,
    pub(crate) dmICMMethod: u32,
    pub(crate) dmICMIntent: u32,
    pub(crate) dmMediaType: u32,
    pub(crate) dmDitherType: u32,
    pub(crate) dmReserved1: u32,
    pub(crate) dmReserved2: u32,
    pub(crate) dmPanningWidth: u32,
    pub(crate) dmPanningHeight: u32,
}

// ── DEVMODE constants ──────────────────────────────────────────────
pub(crate) const DM_OUT_BUFFER: u32 = 2;
pub(crate) const DM_IN_BUFFER: u32 = 8;
pub(crate) const DM_IN_PROMPT: u32 = 4;
pub(crate) const DM_PAPERSIZE: u32 = 0x0002;
pub(crate) const DM_PAPERLENGTH: u32 = 0x0004;
pub(crate) const DM_PAPERWIDTH: u32 = 0x0008;

pub(crate) const DMORIENT_LANDSCAPE: i16 = 2;
pub(crate) const DMORIENT_PORTRAIT: i16 = 1;
pub(crate) const DM_ORIENTATION: u32 = 0x0001;

pub(crate) const DC_PAPERSIZE: u16 = 3;
pub(crate) const DC_PAPERNAMES: u16 = 16;
pub(crate) const DC_PAPERS: u16 = 2;

/// Map a DMPAPER_ constant back to a human-readable preset name.
/// DEPRECATED: use dmFormName from DEVMODE instead.
/// Kept temporarily for macOS/Linux compatibility — will be removed when
/// paper_index is fully adopted across all platforms.
pub(crate) const DMPAPER_USER: i16 = 256;

/// Set the paper size in the printer's DEVMODE structure via
/// DocumentPropertiesW, then return a DC created with that DEVMODE.
///
/// This is critical for correct paper-handling: without it the printer DC
/// inherits whatever paper is currently selected in the driver's defaults,
/// which may not match the paper size the user chose in the print dialog.
/// Fallback: open printer DC without DEVMODE when DEVMODE manipulation fails.
/// This lets the printer use its driver-default paper size.
pub(crate) unsafe fn fallback_create_dc(
    printer_wide: &[u16],
    error_msg: &str,
) -> Result<HDC, String> {
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
