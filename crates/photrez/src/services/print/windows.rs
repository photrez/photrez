mod ffi;

use super::geometry::{fit_dimensions, printable_rect};
use super::{PaperSize, PrintCancellation, PrintError, PrintRequest};
use ffi::{
    AbortDoc, ClosePrinter, CreateDCW, DocumentPropertiesW, EndDoc, EndPage, GetDefaultPrinterW,
    OpenPrinterW, StartDocW, StartPage, DEVMODEW, DMORIENT_LANDSCAPE, DMORIENT_PORTRAIT,
    DM_IN_BUFFER, DM_ORIENTATION, DM_OUT_BUFFER, DM_PAPERLENGTH, DM_PAPERSIZE, DM_PAPERWIDTH,
    DM_PAPER_INDEX_USER, DOCINFOW,
};
use std::ffi::c_void;
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Graphics::Gdi::{
    DeleteDC, GetDeviceCaps, SetBrushOrgEx, SetStretchBltMode, StretchDIBits, BITMAPINFO,
    BITMAPINFOHEADER, LOGPIXELSX, LOGPIXELSY, PHYSICALOFFSETX, PHYSICALOFFSETY,
};
use windows_sys::Win32::Graphics::Printing::{
    EnumPrintersW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_INFO_4W,
};
use windows_sys::Win32::Storage::Xps::{
    DeviceCapabilitiesW, DC_PAPERNAMES, DC_PAPERS, DC_PAPERSIZE,
};

const MAX_DEVMODE_BYTES: usize = 16 * 1024 * 1024;
const MAX_PRINTER_COUNT: usize = 4096;
const MAX_PAPER_COUNT: usize = 4096;
const PAPER_NAME_UNITS: usize = 64;

pub(super) fn print_rgba(
    request: &PrintRequest<'_>,
    cancellation: &PrintCancellation,
) -> Result<(), PrintError> {
    if cancellation.is_cancelled() {
        return Err(PrintError::Cancelled);
    }

    let printer = match request
        .settings
        .selected_printer
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        Some(name) => name.to_owned(),
        None => default_printer()?,
    };

    let mut bgra = Vec::with_capacity(request.pixels.len());
    for (index, rgba) in request.pixels.chunks_exact(4).enumerate() {
        if index & 0x0fff == 0 && cancellation.is_cancelled() {
            return Err(PrintError::Cancelled);
        }
        bgra.extend_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
    }

    let (paper_width_mm, paper_height_mm) = request.settings.oriented_dimensions();
    let printer_wide = wide_nul(&printer);
    let hdc = create_printer_dc_with_paper_size(
        &printer_wide,
        request.settings.paper_index,
        paper_width_mm,
        paper_height_mm,
        request.settings.orientation,
    )?;
    let mut document = PrintDocument::new(hdc);

    let dpi_x = unsafe { GetDeviceCaps(hdc, LOGPIXELSX as i32) } as f64;
    let dpi_y = unsafe { GetDeviceCaps(hdc, LOGPIXELSY as i32) } as f64;
    let offset_x = unsafe { GetDeviceCaps(hdc, PHYSICALOFFSETX as i32) };
    let offset_y = unsafe { GetDeviceCaps(hdc, PHYSICALOFFSETY as i32) };
    let area = printable_rect(
        paper_width_mm,
        paper_height_mm,
        dpi_x,
        dpi_y,
        offset_x,
        offset_y,
        [
            request.settings.margin_left_mm,
            request.settings.margin_right_mm,
            request.settings.margin_top_mm,
            request.settings.margin_bottom_mm,
        ],
    )?;
    let (dest_width, dest_height) =
        fit_dimensions(request.width, request.height, area.width, area.height)?;

    let mut bitmap: BITMAPINFO = unsafe { std::mem::zeroed() };
    bitmap.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bitmap.bmiHeader.biWidth = request.width as i32;
    bitmap.bmiHeader.biHeight = -(request.height as i32);
    bitmap.bmiHeader.biPlanes = 1;
    bitmap.bmiHeader.biBitCount = 32;

    if cancellation.is_cancelled() {
        return Err(PrintError::Cancelled);
    }
    let document_name = if request.document_name.is_empty() {
        "Untitled"
    } else {
        request.document_name
    };
    let document_name_wide = wide_nul(document_name);
    let output_path_wide = request
        .output_path
        .filter(|path| !path.is_empty())
        .map(wide_nul);
    let info = DOCINFOW {
        cbSize: std::mem::size_of::<DOCINFOW>() as i32,
        lpszDocName: document_name_wide.as_ptr(),
        lpszOutput: output_path_wide
            .as_ref()
            .map_or(std::ptr::null(), |path| path.as_ptr()),
        lpszDatatype: std::ptr::null(),
        fwType: 0,
    };
    document.start(&info)?;

    let needs_scale = dest_width != request.width as i32 || dest_height != request.height as i32;
    if needs_scale {
        unsafe {
            SetStretchBltMode(hdc, 4);
            SetBrushOrgEx(hdc, 0, 0, std::ptr::null_mut());
        }
    }

    for _ in 0..request.settings.copies.max(1) {
        if cancellation.is_cancelled() {
            return Err(PrintError::Cancelled);
        }
        if unsafe { StartPage(hdc) } <= 0 {
            return Err(printer_error("Failed to start print page"));
        }
        let lines = unsafe {
            StretchDIBits(
                hdc,
                area.x,
                area.y,
                dest_width,
                dest_height,
                0,
                0,
                request.width as i32,
                request.height as i32,
                bgra.as_ptr() as *const c_void,
                &bitmap,
                0,
                0x00cc0020,
            )
        };
        if lines <= 0 {
            return Err(printer_error("StretchDIBits failed"));
        }
        if cancellation.is_cancelled() {
            return Err(PrintError::Cancelled);
        }
        if unsafe { EndPage(hdc) } <= 0 {
            return Err(printer_error("Failed to end print page"));
        }
    }

    document.finish()
}

/// Enumerate locally installed and connected printers.
pub(super) fn list_printers() -> Result<Vec<String>, PrintError> {
    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
    let mut needed = 0u32;
    let mut returned = 0u32;
    unsafe {
        EnumPrintersW(
            flags,
            std::ptr::null(),
            4,
            std::ptr::null_mut(),
            0,
            &mut needed,
            &mut returned,
        );
    }
    if needed == 0 {
        return Ok(Vec::new());
    }
    let needed_bytes = usize::try_from(needed)
        .map_err(|_| PrintError::resource_limit("Printer enumeration size overflow"))?;
    // PRINTER_INFO_4W contains pointers; allocate usize-aligned storage.
    let mut buffer = vec![0usize; needed_bytes.div_ceil(std::mem::size_of::<usize>())];
    let mut returned = 0u32;
    let buffer_bytes = u32::try_from(needed_bytes)
        .map_err(|_| PrintError::resource_limit("Printer enumeration size overflow"))?;
    let ok = unsafe {
        EnumPrintersW(
            flags,
            std::ptr::null(),
            4,
            buffer.as_mut_ptr().cast::<u8>(),
            buffer_bytes,
            &mut needed,
            &mut returned,
        )
    };
    if ok == 0 {
        return Err(printer_error("Failed to enumerate printers"));
    }
    let count = usize::try_from(returned)
        .map_err(|_| PrintError::resource_limit("Printer count overflow"))?;
    if count > MAX_PRINTER_COUNT {
        return Err(PrintError::resource_limit("Printer count exceeds limit"));
    }
    let infos =
        unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<PRINTER_INFO_4W>(), count) };
    let mut printers = Vec::with_capacity(count);
    for info in infos {
        printers.push(utf16_pointer_to_string(info.pPrinterName)?);
    }
    Ok(printers)
}

/// Enumerate the selected printer's paper sizes in millimetres.
pub(super) fn paper_sizes(printer: &str) -> Result<Vec<PaperSize>, PrintError> {
    let printer_wide = wide_nul(printer);
    let count = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERS,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    if count <= 0 {
        return Err(printer_error("Failed to enumerate printer papers"));
    }
    let count =
        usize::try_from(count).map_err(|_| PrintError::resource_limit("Paper count overflow"))?;
    if count > MAX_PAPER_COUNT {
        return Err(PrintError::resource_limit("Paper count exceeds limit"));
    }

    let mut indices = vec![0u16; count];
    let mut names = vec![0u16; count * PAPER_NAME_UNITS];
    let mut sizes = vec![POINT { x: 0, y: 0 }; count];
    let paper_count = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERS,
            indices.as_mut_ptr(),
            std::ptr::null(),
        )
    };
    let name_count = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERNAMES,
            names.as_mut_ptr(),
            std::ptr::null(),
        )
    };
    let size_count = unsafe {
        DeviceCapabilitiesW(
            printer_wide.as_ptr(),
            std::ptr::null(),
            DC_PAPERSIZE,
            sizes.as_mut_ptr().cast::<u16>(),
            std::ptr::null(),
        )
    };
    if paper_count != i32::try_from(count).unwrap_or(i32::MAX)
        || name_count != i32::try_from(count).unwrap_or(i32::MAX)
        || size_count != i32::try_from(count).unwrap_or(i32::MAX)
    {
        return Err(PrintError::Printer(
            "Printer paper list changed while enumerating".to_owned(),
        ));
    }

    let mut papers = Vec::with_capacity(count);
    for index in 0..count {
        let name_start = index * PAPER_NAME_UNITS;
        let name_units = &names[name_start..name_start + PAPER_NAME_UNITS];
        let name_end = name_units.iter().position(|unit| *unit == 0).unwrap_or(0);
        if name_end == 0 {
            continue;
        }
        let name = String::from_utf16(&name_units[..name_end])
            .map_err(|error| PrintError::Printer(format!("Invalid paper name: {error}")))?;
        papers.push(PaperSize {
            name,
            index: i16::try_from(indices[index]).map_err(|_| {
                PrintError::Printer("Paper index exceeds the DEVMODE range".to_owned())
            })?,
            width_mm: f64::from(sizes[index].x) / 10.0,
            height_mm: f64::from(sizes[index].y) / 10.0,
        });
    }
    Ok(papers)
}

fn utf16_pointer_to_string(pointer: *const u16) -> Result<String, PrintError> {
    if pointer.is_null() {
        return Err(PrintError::Printer("Printer name is null".to_owned()));
    }
    let mut length = 0usize;
    while length < 32_768 && unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    if length == 32_768 {
        return Err(PrintError::Printer(
            "Printer name is not terminated".to_owned(),
        ));
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(pointer, length) })
        .map_err(|error| PrintError::Printer(format!("Invalid printer name: {error}")))
}

fn default_printer() -> Result<String, PrintError> {
    let mut length = 0u32;
    unsafe { GetDefaultPrinterW(std::ptr::null_mut(), &mut length) };
    if length == 0 || length as usize > 32_768 {
        return Err(PrintError::Printer(format!(
            "No default printer is available ({})",
            std::io::Error::last_os_error()
        )));
    }
    let mut name = vec![0u16; length as usize];
    if unsafe { GetDefaultPrinterW(name.as_mut_ptr(), &mut length) } == 0 {
        return Err(printer_error("Failed to query the default printer"));
    }
    let end = name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(name.len());
    String::from_utf16(&name[..end])
        .map_err(|error| PrintError::Printer(format!("Default printer name is invalid: {error}")))
}

fn create_printer_dc_with_paper_size(
    printer_wide: &[u16],
    paper_index: i16,
    paper_width_mm: f64,
    paper_height_mm: f64,
    orientation: super::Orientation,
) -> Result<ffi::HDC, PrintError> {
    let mut handle = 0isize;
    if unsafe { OpenPrinterW(printer_wide.as_ptr(), &mut handle, std::ptr::null()) } == 0
        || handle == 0
    {
        let error = std::io::Error::last_os_error();
        return create_dc_without_devmode(
            printer_wide,
            &format!("Failed to open printer; default paper fallback: {error}"),
        );
    }
    let handle = PrinterHandle(handle);

    let size = unsafe {
        DocumentPropertiesW(
            0,
            handle.0,
            printer_wide.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    if size <= 0 {
        let error = std::io::Error::last_os_error();
        return create_dc_without_devmode(
            printer_wide,
            &format!("Failed to query printer DEVMODE; default paper fallback: {error}"),
        );
    }
    let size = size as usize;
    if size < std::mem::size_of::<DEVMODEW>() || size > MAX_DEVMODE_BYTES {
        return create_dc_without_devmode(
            printer_wide,
            "Printer returned an invalid DEVMODE size; default paper fallback",
        );
    }

    // Vec<u8> only guarantees byte alignment; DEVMODEW requires native alignment.
    let mut devmode_buffer = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
    let devmode = devmode_buffer.as_mut_ptr().cast::<DEVMODEW>();
    if unsafe {
        DocumentPropertiesW(
            0,
            handle.0,
            printer_wide.as_ptr(),
            devmode.cast::<c_void>(),
            std::ptr::null_mut(),
            DM_OUT_BUFFER,
        )
    } <= 0
    {
        let error = std::io::Error::last_os_error();
        return create_dc_without_devmode(
            printer_wide,
            &format!("Failed to initialize printer DEVMODE; default paper fallback: {error}"),
        );
    }

    unsafe {
        (*devmode).dmFields |= DM_ORIENTATION;
        (*devmode).dmOrientation = match orientation {
            super::Orientation::Portrait => DMORIENT_PORTRAIT,
            super::Orientation::Landscape => DMORIENT_LANDSCAPE,
        };
        (*devmode).dmFields |= DM_PAPERSIZE;
        (*devmode).dmPaperSize = paper_index;
        if paper_index == DM_PAPER_INDEX_USER {
            (*devmode).dmFields |= DM_PAPERLENGTH | DM_PAPERWIDTH;
            (*devmode).dmPaperWidth = (paper_width_mm * 10.0)
                .round()
                .clamp(i16::MIN as f64, i16::MAX as f64)
                as i16;
            (*devmode).dmPaperLength = (paper_height_mm * 10.0)
                .round()
                .clamp(i16::MIN as f64, i16::MAX as f64)
                as i16;
        }
    }

    if unsafe {
        DocumentPropertiesW(
            0,
            handle.0,
            printer_wide.as_ptr(),
            devmode.cast::<c_void>(),
            devmode.cast::<c_void>(),
            DM_IN_BUFFER | DM_OUT_BUFFER,
        )
    } <= 0
    {
        let error = std::io::Error::last_os_error();
        return create_dc_without_devmode(
            printer_wide,
            &format!("Failed to merge printer DEVMODE; default paper fallback: {error}"),
        );
    }

    let winspool = wide_nul("WINSPOOL");
    let hdc = unsafe {
        CreateDCW(
            winspool.as_ptr(),
            printer_wide.as_ptr(),
            std::ptr::null(),
            devmode.cast::<c_void>(),
        )
    };
    if hdc == 0 {
        return Err(printer_error("Failed to create printer DC with DEVMODE"));
    }
    Ok(hdc)
}

fn create_dc_without_devmode(printer_wide: &[u16], context: &str) -> Result<ffi::HDC, PrintError> {
    let winspool = wide_nul("WINSPOOL");
    let hdc = unsafe {
        CreateDCW(
            winspool.as_ptr(),
            printer_wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if hdc == 0 {
        return Err(printer_error(context));
    }
    Ok(hdc)
}

struct PrinterHandle(isize);

impl Drop for PrinterHandle {
    fn drop(&mut self) {
        unsafe { ClosePrinter(self.0) };
    }
}

struct PrintDocument {
    hdc: ffi::HDC,
    started: bool,
    finished: bool,
}

impl PrintDocument {
    fn new(hdc: ffi::HDC) -> Self {
        Self {
            hdc,
            started: false,
            finished: false,
        }
    }

    fn start(&mut self, info: &DOCINFOW) -> Result<(), PrintError> {
        if unsafe { StartDocW(self.hdc, info) } <= 0 {
            return Err(printer_error("Failed to start print document"));
        }
        self.started = true;
        Ok(())
    }

    fn finish(&mut self) -> Result<(), PrintError> {
        if unsafe { EndDoc(self.hdc) } <= 0 {
            return Err(printer_error("Failed to finish print document"));
        }
        self.finished = true;
        Ok(())
    }
}

impl Drop for PrintDocument {
    fn drop(&mut self) {
        if self.started && !self.finished {
            unsafe { AbortDoc(self.hdc) };
        }
        unsafe { DeleteDC(self.hdc) };
    }
}

fn printer_error(context: &str) -> PrintError {
    PrintError::Printer(format!("{context}: {}", std::io::Error::last_os_error()))
}

fn wide_nul(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
