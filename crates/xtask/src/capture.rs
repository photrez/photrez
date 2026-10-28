//! Product-side window capture driver - the Rust replacement for the original
//! PowerShell capture scripts (one process per shot, client-area screen
//! capture, pointer parked outside the box).
//!
//! Semantics copied deliberately from the PowerShell original, because each
//! one exists for a recorded reason:
//!
//! - window found by PROCESS ID (the child we spawned - never by title), kept
//!   only when visible with a non-zero client rect;
//! - a generous wait for font registration + first paint, then fixed settles
//!   (750 ms after the window exists, 300 ms after focus, 500 ms with the
//!   pointer parked - the PS original's numbers);
//! - pointer parked OUTSIDE the client box for the captured frame (photrez
//!   paints hover states; a stale hover diffs as a regression) and restored
//!   afterwards;
//! - capture is the client area only (excludes title bar and drop shadow),
//!   read from the screen at the window's origin - the same region the
//!   baselines were cropped from;
//! - any previously running `photrez` process is killed before launch (a live
//!   instance holds the executable open and would fail the next link).
//!
//! The PowerShell original was DPI-unaware at deviceScaleFactor 1 and so is
//! this driver; parity is defined at 1366x820 client pixels.
//!
//! `unsafe_code` is allowed for THIS MODULE ONLY (root lints opt in elsewhere):
//! the module is a thin, reviewed wrapper over `windows-sys` screen-capture
//! calls, which are `unsafe` by nature. Everything else in the crate stays
//! deny.

#![allow(unsafe_code)] // reason: thin windows-sys screen-capture wrapper; the rest of the crate stays deny.

use std::path::PathBuf;

mod executable;
#[cfg(test)]
mod tests;
pub mod window;

pub use executable::resolve_exe;

#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
use windows_sys::Win32::Foundation::{FALSE, HWND, LPARAM, POINT, RECT, TRUE};
#[cfg(windows)]
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, SRCCOPY,
};
#[cfg(windows)]
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetCursorPos, GetSystemMetrics, GetWindowThreadProcessId,
    IsWindowVisible, SetCursorPos, SetForegroundWindow, HWND_DESKTOP, SM_CYSCREEN,
};

/// One capture request: which shot the app should render, which executable to
/// run (default: locate the photrez debug build), and where the PNG goes.
pub struct CaptureSpec {
    /// `--shot <id>` forwarded to the app; `None` renders the default state.
    pub shot: Option<String>,
    /// Explicit photrez executable; `None` triggers build + locate.
    pub exe: Option<PathBuf>,
    /// Output PNG path (parent directories are created).
    pub out: PathBuf,
    /// Bounds for the window to appear plus first paint, in milliseconds.
    pub wait_ms: u64,
}

// ---------------------------------------------------------------------------
// capture environment guard
// ---------------------------------------------------------------------------

/// The client size the baselines were captured at - parity is defined only at
/// this size, and only at 100% display scale.
#[cfg(windows)]
const EXPECTED_CLIENT: (i32, i32) = (1366, 820);

/// Refuse to capture when the environment cannot produce a parity image.
///
/// Two conditions make a capture meaningless rather than merely different,
/// and both leave the saved PNG looking plausible:
///
/// - a window at a non-96 display scale is rendered on the display's pixel
///   grid (every logical pixel lands on ~1.25 physical ones), so the PNG no
///   longer shares the baselines' grid;
/// - a client smaller than 1366x820 means the window was clamped to the
///   display's work area (its top edge even goes off-screen), so the page is
///   cropped before it is compared.
///
/// A pixel diff cannot see either condition, so the driver refuses loudly
/// here instead of producing a plausible-looking but meaningless comparison.
#[cfg(windows)]
fn assert_capture_environment(client_w: i32, client_h: i32, dpi: u32) -> Result<(), String> {
    if dpi != 96 {
        return Err(format!(
            "display scale is {}% ({dpi} dpi); parity captures require 100% display scaling \
             (Windows Settings > System > Display > Scale). The baselines assume 1366x820 logical pixels.",
            dpi * 100 / 96
        ));
    }
    let (expected_w, expected_h) = EXPECTED_CLIENT;
    if (client_w, client_h) != (expected_w, expected_h) {
        return Err(format!(
            "client is {client_w}x{client_h}, expected {expected_w}x{expected_h} - the window was clamped \
             by the work area. Ensure at least {expected_h}px of usable display height (hide or auto-hide \
             the taskbar) and re-run."
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// capture locale guard
// ---------------------------------------------------------------------------

/// The locale every parity capture must resolve: the golden-master baselines
/// were captured with the English catalog, so a diff under any other locale is
/// meaningless rather than merely different.
const CAPTURE_LOCALE: &str = "en";

/// The override the app reads before any stored preference (see
/// `photrez::i18n::initial_locale`). The driver sets it on the child it
/// spawns; a caller who already set it to something else is refused.
const LOCALE_ENV: &str = "PHOTREZ_LOCALE";

/// Pure core of the guard: nothing requested is fine; a blank request means
/// "unset"; anything but English is refused by name.
fn locale_request_ok(requested: Option<&str>) -> Result<(), String> {
    match requested {
        None => Ok(()),
        Some(tag) if tag.trim().is_empty() => Ok(()),
        Some(tag) if tag.trim().eq_ignore_ascii_case(CAPTURE_LOCALE) => Ok(()),
        Some(tag) => Err(format!(
            "{LOCALE_ENV}={tag} requests a non-English locale; parity captures \
             must resolve `{CAPTURE_LOCALE}`. Unset the variable or set it to `{CAPTURE_LOCALE}`."
        )),
    }
}

/// Read the incoming environment and refuse a non-English locale request.
pub fn require_english_locale() -> Result<(), String> {
    locale_request_ok(std::env::var(LOCALE_ENV).ok().as_deref())
}
// ---------------------------------------------------------------------------
// capture (Windows-only)
// ---------------------------------------------------------------------------

/// Kill every running `photrez.exe` process so a stale handle does not block
/// the next link. Mirrors capture-shot.ps1 / dev.psl. `taskkill` returns a
/// non-zero exit code when nothing matches - that is not an error here.
#[cfg(windows)]
fn kill_stray_photrez() -> Result<(), String> {
    let _ = std::process::Command::new("taskkill")
        .arg("/F")
        .arg("/IM")
        .arg("photrez.exe")
        .status()
        .map_err(|e| format!("failed to run taskkill: {e}"))?;
    // Give the OS a moment to tear down the old process and release the
    // executable file handle (dev.psl pauses 400 ms here).
    std::thread::sleep(Duration::from_millis(400));
    Ok(())
}

/// Force-kill the child we spawned, identified by PID. Uses `taskkill` (no
/// extra windows-sys feature needed for OpenProcess/TerminateProcess).
#[cfg(windows)]
fn kill_photrez_by_pid(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .arg("/PID")
        .arg(pid.to_string())
        .arg("/F")
        .status();
}

/// LPARAM payload for the EnumWindows callback: the child PID to match and
/// where the result lands.
#[cfg(windows)]
#[repr(C)]
struct FindCtx {
    pid: u32,
    hwnd: Option<HWND>,
    rect: Option<RECT>,
}

/// EnumWindows callback: match the child PID, require IsWindowVisible, and a
/// non-zero client rect. Return FALSE (0) to stop enumeration once found.
#[cfg(windows)]
unsafe extern "system" fn enum_find_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let ctx = &mut *(lparam as *mut FindCtx);
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == ctx.pid && IsWindowVisible(hwnd) != 0 {
        let mut rect: RECT = RECT::default();
        if GetClientRect(hwnd, &mut rect) != 0 {
            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;
            if w > 0 && h > 0 {
                ctx.hwnd = Some(hwnd);
                ctx.rect = Some(rect);
                return FALSE; // stop enumerating
            }
        }
    }
    TRUE // keep going
}

/// Poll the window table every ~250 ms for a window owned by *pid* that is
/// visible with a non-zero client rect. Bails out after `wait_ms`.
#[cfg(windows)]
fn find_window_for_pid(pid: u32, wait_ms: u64) -> Result<(HWND, RECT), String> {
    let deadline = Instant::now() + Duration::from_millis(wait_ms);
    loop {
        let mut ctx = FindCtx {
            pid,
            hwnd: None,
            rect: None,
        };
        unsafe {
            EnumWindows(Some(enum_find_proc), &mut ctx as *mut _ as LPARAM);
        }
        if let (Some(hwnd), Some(rect)) = (ctx.hwnd, ctx.rect) {
            return Ok((hwnd, rect));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no visible window with a non-zero client rect for pid {pid} within {wait_ms}ms"
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Full primary-screen height in pixels, via GetSystemMetrics(SM_CYSCREEN).
#[cfg(windows)]
fn screen_height() -> i32 {
    unsafe { GetSystemMetrics(SM_CYSCREEN) }
}

/// Compute a screen point outside the client box, mirroring capture.psl:
/// prefer the gap just below the window; fall back to outside the left edge
/// (or right edge when the window is flush against x = 0).
#[cfg(windows)]
fn park_point(box_x: i32, box_y: i32, box_w: i32, box_h: i32) -> POINT {
    let bottom = screen_height();
    let mid_y = box_y + box_h / 2;
    let mut park_x;
    let mut park_y;
    // Just past the left edge (or right edge if flush against x = 0).
    if box_x >= 3 {
        park_x = box_x - 3;
    } else {
        park_x = box_x + box_w + 3;
    }
    park_y = mid_y;
    // If there is room below the window, park there instead: that puts the
    // cursor outside the client box by the window's full height, which matters
    // because the invisible resize frame means 3 px may still be inside it.
    if box_y + box_h + 12 < bottom {
        park_x = box_x + box_w / 2;
        park_y = box_y + box_h + 12;
    }
    POINT {
        x: park_x,
        y: park_y,
    }
}

/// BitBlt the client area from the screen into a top-down RGBA8 buffer.
///
/// `src_x` / `src_y` is the screen coordinate of the client origin; `w` / `h`
/// are the client dimensions in pixels.
#[cfg(windows)]
pub(crate) unsafe fn capture_client(
    src_x: i32,
    src_y: i32,
    w: i32,
    h: i32,
) -> Result<Vec<u8>, String> {
    let hdc_screen = GetDC(HWND_DESKTOP);
    if hdc_screen.is_null() {
        return Err("GetDC(HWND_DESKTOP) returned NULL".into());
    }
    let hdc_mem = CreateCompatibleDC(hdc_screen);
    if hdc_mem.is_null() {
        ReleaseDC(HWND_DESKTOP, hdc_screen);
        return Err("CreateCompatibleDC failed".into());
    }
    let hbm = CreateCompatibleBitmap(hdc_screen, w, h);
    if hbm.is_null() {
        DeleteDC(hdc_mem);
        ReleaseDC(HWND_DESKTOP, hdc_screen);
        return Err("CreateCompatibleBitmap failed".into());
    }
    let old = SelectObject(hdc_mem, hbm);
    if BitBlt(hdc_mem, 0, 0, w, h, hdc_screen, src_x, src_y, SRCCOPY) == 0 {
        let _ = SelectObject(hdc_mem, old);
        let _ = DeleteObject(hbm);
        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(HWND_DESKTOP, hdc_screen);
        return Err("BitBlt failed".into());
    }

    // Request a 32-bpp BI_RGB DIB (bottom-up because biHeight > 0).
    let mut bmi: BITMAPINFO = BITMAPINFO::default();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = w;
    bmi.bmiHeader.biHeight = h;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;

    let row_stride = (w as usize) * 4;
    let buf_len = (h as usize) * row_stride;
    let mut raw = vec![0u8; buf_len];
    let lines = GetDIBits(
        hdc_mem,
        hbm,
        0,
        h as u32,
        raw.as_mut_ptr() as *mut std::ffi::c_void,
        &mut bmi as *mut BITMAPINFO,
        DIB_RGB_COLORS,
    );
    let _ = SelectObject(hdc_mem, old);
    let _ = DeleteObject(hbm);
    let _ = DeleteDC(hdc_mem);
    let _ = ReleaseDC(HWND_DESKTOP, hdc_screen);

    if lines == 0 {
        return Err("GetDIBits returned 0 scan lines".into());
    }
    bgra_to_rgba_top_down(&raw, w as u32, h as u32, row_stride)
}

/// Windows implementation of capture.
#[cfg(windows)]
fn capture_inner(spec: &CaptureSpec, child_pid: u32) -> Result<PathBuf, String> {
    // Find the window by child PID.
    let (hwnd, _) = find_window_for_pid(child_pid, spec.wait_ms)?;

    // Settle: 750 ms after the window exists (font registration + first paint).
    std::thread::sleep(Duration::from_millis(750));

    // Re-read the client rect + origin; the window may have painted and
    // resized since we first spotted it.
    let mut client_rect: RECT = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut client_rect) } == 0 {
        return Err("GetClientRect after settle returned false".into());
    }
    let w = client_rect.right - client_rect.left;
    let h = client_rect.bottom - client_rect.top;
    if w <= 0 || h <= 0 {
        return Err("client rect became zero after settle".into());
    }
    // Refuse to continue when the environment cannot yield a parity image
    // (scaled display or a clamped window); see `assert_capture_environment`.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    assert_capture_environment(w, h, dpi)?;
    let mut origin: POINT = POINT { x: 0, y: 0 };
    if unsafe { ClientToScreen(hwnd, &mut origin) } == 0 {
        return Err("ClientToScreen returned false".into());
    }

    // Elevate the window so its pixels are on top (CopyFromScreen reads the
    // screen, not the window directly).
    unsafe { SetForegroundWindow(hwnd) };
    std::thread::sleep(Duration::from_millis(300));

    // Park the cursor outside the client box, capture, then restore.
    let mut prev: POINT = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut prev) } == 0 {
        return Err("GetCursorPos returned false".into());
    }
    let park = park_point(origin.x, origin.y, w, h);
    if unsafe { SetCursorPos(park.x, park.y) } == 0 {
        return Err("SetCursorPos (park) returned false".into());
    }
    std::thread::sleep(Duration::from_millis(500));

    let rgba = unsafe { capture_client(origin.x, origin.y, w, h)? };

    // Restore cursor regardless of capture success.
    let _ = unsafe { SetCursorPos(prev.x, prev.y) };

    // Save the PNG.
    let img = crate::parity::pngio::RgbaImage::from_rgba(w as u32, h as u32, rgba)?;
    img.save(spec.out.as_path())?;
    Ok(spec.out.clone())
}

/// Run one capture. Returns the PNG path written.
///
/// Windows-only; on other platforms returns an explanatory error (the parity
/// comparison itself is portable, capture is not).
#[cfg(windows)]
pub fn capture(spec: &CaptureSpec) -> Result<PathBuf, String> {
    require_english_locale()?;
    kill_stray_photrez()?;
    let exe = resolve_exe(spec.exe.as_deref())?;

    let mut cmd = std::process::Command::new(&exe);
    // The baselines were captured with the English catalog: pin it on the
    // child regardless of this machine's stored preference or OS locale.
    cmd.env(LOCALE_ENV, CAPTURE_LOCALE)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(ref id) = spec.shot {
        cmd.arg("--shot").arg(id);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn photrez: {e}"))?;
    let child_pid = child.id();

    // capture_inner does the work; the child is always torn down afterwards
    // (on both success and error).
    let result = capture_inner(spec, child_pid);
    kill_photrez_by_pid(child_pid);
    let _ = child.wait();
    result
}

#[cfg(not(windows))]
pub fn capture(spec: &CaptureSpec) -> Result<PathBuf, String> {
    Err("capture is Windows-only; this platform cannot screen-capture the Photrez window".into())
}

// ---------------------------------------------------------------------------
// bgra_to_rgba_top_down
// ---------------------------------------------------------------------------

/// Convert a GDI DIB buffer (BGRA, bottom-up rows, `row_stride` possibly
/// padded) into a top-down RGBA8 buffer for PNG encoding. Pure function, so it
/// carries its own test.
pub(crate) fn bgra_to_rgba_top_down(
    src: &[u8],
    width: u32,
    height: u32,
    row_stride: usize,
) -> Result<Vec<u8>, String> {
    let width = width as usize;
    let height = height as usize;
    if width == 0 || height == 0 {
        return Err("width and height must be non-zero".into());
    }
    let pixel_bytes = 4;
    if row_stride < width * pixel_bytes {
        return Err(format!(
            "row stride {row_stride} is smaller than width {width} * 4 bytes = {}",
            width * pixel_bytes
        ));
    }
    // Verify the source buffer is large enough (guard against overflow).
    let src_needed = height.checked_mul(row_stride);
    match src_needed {
        Some(n) if src.len() >= n => {}
        Some(n) => {
            return Err(format!(
                "source buffer is {} bytes, need at least {n} for {height} rows x {row_stride} stride",
                src.len()
            ));
        }
        None => {
            return Err("image dimensions overflow usize".into());
        }
    }

    // Bottom-up => top-down: input row 0 is the bottom scan line, output row 0
    // is the top. Within each pixel, swap blue and red (BGRA => RGBA).
    let out_len = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(pixel_bytes))
        .ok_or_else(|| "output buffer size overflow".to_string())?;
    let mut out = Vec::with_capacity(out_len);
    for out_row in 0..height {
        let src_row = height - 1 - out_row;
        let src_start = src_row * row_stride;
        for px in 0..width {
            let s = src_start + px * pixel_bytes;
            out.push(src[s + 2]); // R
            out.push(src[s + 1]); // G
            out.push(src[s]); // B
            out.push(src[s + 3]); // A
        }
    }
    Ok(out)
}
