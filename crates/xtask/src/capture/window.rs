//! Generic window-capture mode (`xtask capture --window <title|:pid>`).
//!
//! Attaches to an already-running window instead of launching the app, so the
//! same harness can capture any application - the native Photrez window and
//! the 2026-08-16 oracle running in its Tauri shell alike. Same DPI-aware GDI
//! client-area path as the launch mode; the environment assertion is not
//! applied here because the comparison environment deliberately runs at 125%
//! scaling, which the launch mode refuses for the frozen baseline size.

use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    SetForegroundWindow, SetProcessDPIAware, ShowWindow, SW_MAXIMIZE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

use super::{capture_client, park_point};

pub struct WindowSpec {
    /// Title substring (case-insensitive), or `:1234` to attach by PID.
    pub target: String,
    /// Maximize before capturing.
    pub maximize: bool,
    /// Milliseconds to wait for the window to appear.
    pub wait_ms: u64,
    pub out: std::path::PathBuf,
}

struct TitleCtx {
    needle: String,
    hwnd: Option<HWND>,
    pid: Option<u32>,
}

unsafe extern "system" fn enum_title_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let ctx = &mut *(lparam as *mut TitleCtx);
    if IsWindowVisible(hwnd) == 0 {
        return 1;
    }
    let mut buf = [0u16; 512];
    let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 512);
    let title = String::from_utf16_lossy(&buf[..len.max(0) as usize]);
    if title.to_lowercase().contains(&ctx.needle) {
        if let Some(pid) = ctx.pid {
            let mut window_pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut window_pid);
            if window_pid != pid {
                return 1;
            }
        }
        ctx.hwnd = Some(hwnd);
        return 0;
    }
    1
}

fn find_window(spec: &WindowSpec) -> Result<HWND, String> {
    let (needle, pid) = if let Some(rest) = spec.target.strip_prefix(':') {
        let pid: u32 = rest
            .parse()
            .map_err(|_| format!("`:pid` target must be numeric, got `{}`", rest))?;
        (String::new(), Some(pid))
    } else {
        (spec.target.to_lowercase(), None)
    };
    let deadline = Instant::now() + Duration::from_millis(spec.wait_ms);
    loop {
        let mut ctx = TitleCtx {
            needle: needle.clone(),
            hwnd: None,
            pid,
        };
        unsafe {
            EnumWindows(Some(enum_title_proc), &mut ctx as *mut _ as LPARAM);
        }
        if let Some(hwnd) = ctx.hwnd {
            return Ok(hwnd);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no visible window matching `{}` within {}ms",
                spec.target, spec.wait_ms
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Attach to the window, optionally maximize, settle, and BitBlt the client
/// area to a PNG. Prints the manifest facts (resolution, DPI, window class of
/// evidence) on success.
#[cfg(windows)]
pub fn capture_window(spec: &WindowSpec) -> Result<std::path::PathBuf, String> {
    // Make this process DPI-aware BEFORE measuring anything: without it,
    // GetClientRect returns virtualized (scaled-down) dimensions on a scaled
    // display and the BitBlt captures the wrong region at the wrong size.
    unsafe { SetProcessDPIAware() };
    let hwnd = find_window(spec)?;

    if spec.maximize {
        unsafe { ShowWindow(hwnd, SW_MAXIMIZE) };
        std::thread::sleep(Duration::from_millis(600));
    }
    // Settle: let fonts/first paint finish after maximize or attach.
    std::thread::sleep(Duration::from_millis(750));

    let mut client_rect: RECT = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut client_rect) } == 0 {
        return Err("GetClientRect returned false".into());
    }
    let w = client_rect.right - client_rect.left;
    let h = client_rect.bottom - client_rect.top;
    if w <= 0 || h <= 0 {
        return Err("client rect is zero".into());
    }
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let mut origin: POINT = POINT { x: 0, y: 0 };
    if unsafe { windows_sys::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut origin) } == 0 {
        return Err("ClientToScreen returned false".into());
    }

    unsafe { SetForegroundWindow(hwnd) };
    std::thread::sleep(Duration::from_millis(300));

    let mut prev: POINT = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut prev) } == 0 {
        return Err("GetCursorPos returned false".into());
    }
    let park = park_point(origin.x, origin.y, w, h);
    unsafe { SetCursorPos(park.x, park.y) };
    std::thread::sleep(Duration::from_millis(500));

    let rgba = unsafe { capture_client(origin.x, origin.y, w, h)? };
    let _ = unsafe { SetCursorPos(prev.x, prev.y) };

    let img = crate::parity::pngio::RgbaImage::from_rgba(w as u32, h as u32, rgba)?;
    img.save(spec.out.as_path())?;
    println!(
        "manifest: client={}x{} dpi={} origin=({},{}) out={}",
        w,
        h,
        dpi,
        origin.x,
        origin.y,
        spec.out.display()
    );
    Ok(spec.out.clone())
}

#[cfg(not(windows))]
pub fn capture_window(_spec: &WindowSpec) -> Result<std::path::PathBuf, String> {
    Err("window capture is Windows-only".into())
}
