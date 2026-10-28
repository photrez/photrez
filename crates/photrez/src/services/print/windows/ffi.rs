use std::ffi::c_void;
pub(super) use windows_sys::Win32::Graphics::Gdi::HDC;

#[link(name = "gdi32")]
#[allow(non_snake_case)]
unsafe extern "system" {
    pub(super) fn CreateDCW(
        driver: *const u16,
        device: *const u16,
        output: *const u16,
        init_data: *const c_void,
    ) -> HDC;
    pub(super) fn StartDocW(hdc: HDC, info: *const DOCINFOW) -> i32;
    pub(super) fn EndDoc(hdc: HDC) -> i32;
    pub(super) fn StartPage(hdc: HDC) -> i32;
    pub(super) fn EndPage(hdc: HDC) -> i32;
    pub(super) fn AbortDoc(hdc: HDC) -> i32;
}

#[link(name = "winspool")]
#[allow(non_snake_case)]
unsafe extern "system" {
    pub(super) fn OpenPrinterW(
        name: *const u16,
        handle: *mut isize,
        defaults: *const c_void,
    ) -> i32;
    pub(super) fn ClosePrinter(handle: isize) -> i32;
    pub(super) fn DocumentPropertiesW(
        window: isize,
        printer: isize,
        name: *const u16,
        output: *mut c_void,
        input: *mut c_void,
        mode: u32,
    ) -> i32;
    pub(super) fn GetDefaultPrinterW(name: *mut u16, characters: *mut u32) -> i32;
}

#[repr(C)]
#[allow(non_snake_case)]
#[allow(clippy::upper_case_acronyms)] // the struct mirrors the Win32 type name it binds
pub(super) struct DOCINFOW {
    pub(super) cbSize: i32,
    pub(super) lpszDocName: *const u16,
    pub(super) lpszOutput: *const u16,
    pub(super) lpszDatatype: *const u16,
    pub(super) fwType: u32,
}

#[repr(C)]
#[allow(dead_code, non_snake_case)] // DEVMODEW fields preserve the OS ABI; most are owned by the driver.
#[allow(clippy::upper_case_acronyms)] // the struct mirrors the Win32 type name it binds
pub(super) struct DEVMODEW {
    dmDeviceName: [u16; 32],
    dmSpecVersion: u16,
    dmDriverVersion: u16,
    dmSize: u16,
    dmDriverExtra: u16,
    pub(super) dmFields: u32,
    pub(super) dmOrientation: i16,
    pub(super) dmPaperSize: i16,
    pub(super) dmPaperLength: i16,
    pub(super) dmPaperWidth: i16,
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

pub(super) const DM_OUT_BUFFER: u32 = 2;
pub(super) const DM_IN_BUFFER: u32 = 8;
pub(super) const DM_PAPERSIZE: u32 = 0x0002;
pub(super) const DM_PAPERLENGTH: u32 = 0x0004;
pub(super) const DM_PAPERWIDTH: u32 = 0x0008;
pub(super) const DM_ORIENTATION: u32 = 0x0001;
pub(super) const DMORIENT_LANDSCAPE: i16 = 2;
pub(super) const DMORIENT_PORTRAIT: i16 = 1;
pub(super) const DM_PAPER_INDEX_USER: i16 = 256;
