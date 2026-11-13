//! Win32 helpers for the measurement binary's paint-prove and machine line.
//!
//! The pointer-injection half of the earlier standalone measurement is removed:
//! the native shell has no production pointer path yet, so this bench measures
//! document-update-to-present, not pointer-to-brush. What remains is the evidence
//! the display gate asks for - the composited window really changed - plus the
//! GPU adapter string the baseline table records.
//!
//! Every signature was verified against windows-sys 0.52.0, the version the
//! earlier measurement pinned and this crate's lock already carries.

use std::sync::atomic::{AtomicIsize, Ordering};

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    EnumDisplayDevicesW, GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
    DIB_RGB_COLORS, DISPLAY_DEVICEW, SRCCOPY,
};
use windows_sys::Win32::Storage::Xps::PrintWindow;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetWindowThreadProcessId, IsWindowVisible, PW_RENDERFULLCONTENT,
    WNDENUMPROC,
};

/// Holder for the EnumWindows callback (EnumWindows has no closure form).
static SELF_HWND: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" fn enum_self(hwnd: HWND, _lparam: isize) -> i32 {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    if pid == unsafe { GetCurrentProcessId() } && unsafe { IsWindowVisible(hwnd) } != 0 {
        SELF_HWND.store(hwnd, Ordering::SeqCst);
        return 0; // stop
    }
    1 // continue
}

/// HWND of this process's first visible window.
pub fn self_hwnd() -> Option<HWND> {
    let callback: WNDENUMPROC = Some(enum_self);
    unsafe {
        EnumWindows(callback, 0);
    }
    let hwnd = SELF_HWND.load(Ordering::SeqCst);
    if hwnd == 0 {
        None
    } else {
        Some(hwnd)
    }
}

/// Client area size of the window, in pixels.
fn client_size(hwnd: HWND) -> (i32, i32) {
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    unsafe {
        GetClientRect(hwnd, &mut rect);
    }
    (rect.right - rect.left, rect.bottom - rect.top)
}

fn read_dib(memdc: isize, bmp: isize, old: isize, width: i32, height: i32) -> Option<Vec<u8>> {
    // The bitmap must not be selected when GetDIBits reads it.
    unsafe {
        SelectObject(memdc, old);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height; // top-down
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        let mut buffer = vec![0u8; (width as usize) * (height as usize) * 4];
        let got = GetDIBits(
            memdc,
            bmp,
            0,
            height as u32,
            buffer.as_mut_ptr() as *mut std::ffi::c_void,
            &mut info,
            DIB_RGB_COLORS,
        );
        if got == height {
            Some(buffer)
        } else {
            None
        }
    }
}

/// True when the buffer contains more than one pixel value (a real capture,
/// not a solid-black fill from an accelerated surface).
fn has_variance(buffer: &[u8]) -> bool {
    let pixels = buffer.len() / 4;
    if pixels < 2 {
        return false;
    }
    let first = &buffer[0..4];
    buffer
        .as_chunks::<4>()
        .0
        .iter()
        .take(64)
        .any(|pixel| pixel.as_slice() != first)
        && buffer
            .as_chunks::<4>()
            .0
            .iter()
            .skip(64)
            .any(|pixel| pixel.as_slice() != first)
}

/// Capture the composited window into a `width * height` top-down BGRA buffer.
/// Prefers a BitBlt of the window DC; falls back to PrintWindow with
/// `PW_RENDERFULLCONTENT` when that returns a uniform surface.
pub fn capture(hwnd: HWND) -> Option<(u32, u32, Vec<u8>)> {
    let (width, height) = client_size(hwnd);
    if width <= 0 || height <= 0 {
        return None;
    }
    unsafe {
        let source = GetDC(hwnd);
        if source == 0 {
            return None;
        }
        let memdc = CreateCompatibleDC(source);
        let bitmap = CreateCompatibleBitmap(source, width, height);
        if memdc == 0 || bitmap == 0 {
            if bitmap != 0 {
                DeleteObject(bitmap);
            }
            if memdc != 0 {
                DeleteDC(memdc);
            }
            ReleaseDC(hwnd, source);
            return None;
        }
        let old = SelectObject(memdc, bitmap);

        BitBlt(memdc, 0, 0, width, height, source, 0, 0, SRCCOPY);
        let mut buffer = read_dib(memdc, bitmap, old, width, height);

        let uniform = match &buffer {
            Some(bytes) => !has_variance(bytes),
            None => true,
        };
        if uniform {
            // read_dib left the bitmap deselected; re-select before PrintWindow.
            SelectObject(memdc, bitmap);
            PrintWindow(hwnd, memdc, PW_RENDERFULLCONTENT);
            buffer = read_dib(memdc, bitmap, old, width, height);
        }

        DeleteObject(bitmap);
        DeleteDC(memdc);
        ReleaseDC(hwnd, source);
        buffer.map(|bytes| (width as u32, height as u32, bytes))
    }
}

/// GPU adapter description for the JSON row.
pub fn adapter_string() -> String {
    let mut device: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
    device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
    let ok = unsafe { EnumDisplayDevicesW(std::ptr::null(), 0, &mut device, 0) };
    if ok == 0 {
        return "unknown adapter".to_string();
    }
    let end = device
        .DeviceString
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(device.DeviceString.len());
    String::from_utf16_lossy(&device.DeviceString[..end])
}

/// Two equal-size BGRA captures -> (changed pixel count, changed fraction).
pub fn changed_pixels(before: &[u8], after: &[u8]) -> (usize, f64) {
    let changed = before
        .as_chunks::<4>()
        .0
        .iter()
        .zip(after.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a[0] != b[0] || a[1] != b[1] || a[2] != b[2])
        .count();
    let pixels = before.len() / 4;
    (changed, changed as f64 / pixels.max(1) as f64)
}
