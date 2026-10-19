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

use std::path::{Path, PathBuf};

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
// resolve_exe
// ---------------------------------------------------------------------------

/// Search the same mingw `bin` candidates as `dev.psl`: msys2, Git for Windows,
/// then scoop's msys2. Returns the first directory containing `gcc.exe`.
#[cfg(windows)]
fn find_mingw_bin() -> Result<PathBuf, String> {
    let home = std::env::var("USERPROFILE")
        .map_err(|_| "USERPROFILE is not set in the environment".to_string())?;
    let candidates = [
        PathBuf::from(r"C:\msys64\mingw64\bin"),
        PathBuf::from(r"C:\Program Files\Git\mingw64\bin"),
        PathBuf::from(home)
            .join("scoop")
            .join("apps")
            .join("msys2")
            .join("current")
            .join("mingw64")
            .join("bin"),
    ];
    for c in &candidates {
        if c.join("gcc.exe").is_file() {
            return Ok(c.clone());
        }
    }
    Err("No GNU toolchain found. Tried C:\\msys64\\mingw64\\bin, \
         C:\\Program Files\\Git\\mingw64\\bin, and \
         <USERPROFILE>\\scoop\\apps\\msys2\\current\\mingw64\\bin. \
         Install MSYS2, then: pacman -S mingw-w64-x86_64-gcc"
        .to_string())
}

/// Locate the photrez debug executable, building it if necessary.
///
/// Order: the explicit `--exe` path; else `crates/photrez/target/debug/photrez.exe`
/// relative to the repository root (found by walking up for the root
/// `Cargo.toml`); else `cargo build` with the crate's own manifest (its
/// `.cargo/config.toml` decides the target dir and linker) and the executable
/// read out of `--message-format=json` compiler-artifact rows. The mingw `bin`
/// directory (msys64 / Git for Windows / scoop) is put on PATH for the build -
/// the same candidates the dev scripts search, found the same way.
pub fn resolve_exe(explicit: Option<&Path>) -> Result<PathBuf, String> {
    // 1. Explicit path - trust but verify it exists.
    if let Some(p) = explicit {
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        return Err(format!("explicit exe not found: {}", p.display()));
    }

    // 2. Default debug artefact location relative to repo root.
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "cannot resolve xtask parent dir (CARGO_MANIFEST_DIR)".to_string())?
        .parent()
        .ok_or_else(|| "cannot resolve repository root from xtask location".to_string())?;
    let exe_name = if std::env::consts::EXE_EXTENSION.is_empty() {
        "photrez".to_string()
    } else {
        format!("photrez.{}", std::env::consts::EXE_EXTENSION)
    };
    let default_exe = repo_root
        .join("crates")
        .join("photrez")
        .join("target")
        .join("debug")
        .join(&exe_name);
    if default_exe.exists() {
        return Ok(default_exe);
    }

    // 3. Fresh build via cargo build --message-format=json.
    let photrez_manifest = repo_root.join("crates").join("photrez").join("Cargo.toml");
    let mut cmd = std::process::Command::new("cargo");
    #[cfg(windows)]
    {
        let mingw = find_mingw_bin()?;
        let old_path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{};{}", mingw.display(), old_path));
    }
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(&photrez_manifest)
        .arg("--message-format=json");
    let output = cmd
        .output()
        .map_err(|e| format!("failed to spawn cargo build: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cargo build failed:\n{stderr}"));
    }

    // Parse compiler-artifact JSON rows to find the photrez binary path.
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if msg.get("reason").and_then(|v| v.as_str()) != Some("compiler-artifact") {
            continue;
        }
        let target = match msg.get("target").and_then(|v| v.as_object()) {
            Some(t) => t,
            None => continue,
        };
        let is_bin = target
            .get("kind")
            .and_then(|v| v.as_array())
            .map(|kinds| kinds.iter().any(|k| k.as_str() == Some("bin")))
            .unwrap_or(false);
        if !is_bin {
            continue;
        }
        if msg.get("package").and_then(|v| v.as_str()) != Some("photrez") {
            continue;
        }
        if let Some(exe) = msg.get("executable").and_then(|v| v.as_str()) {
            let p = PathBuf::from(exe);
            if p.exists() {
                return Ok(p);
            }
        }
    }
    Err("cargo build succeeded but no photrez executable was found in the artifact output".into())
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
unsafe fn capture_client(src_x: i32, src_y: i32, w: i32, h: i32) -> Result<Vec<u8>, String> {
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
    kill_stray_photrez()?;
    let exe = resolve_exe(spec.exe.as_deref())?;

    let mut cmd = std::process::Command::new(&exe);
    cmd.stdout(std::process::Stdio::null())
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

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::assert_capture_environment;
    use super::bgra_to_rgba_top_down;

    /// 2x2 bottom-up BGRA buffer:
    ///
    ///   screen top-left  = Red   BGRA [  0,   0, 255, 255]
    ///   screen top-right = Green BGRA [  0, 255,   0, 255]
    ///   screen bot-left  = Blue  BGRA [255,   0,   0, 255]
    ///   screen bot-right = White BGRA [255, 255, 255, 255]
    ///
    /// Memory layout (bottom-up, stride 8):
    ///   row 0 (bottom on screen): Blue, White
    ///   row 1 (top on screen):    Red, Green
    #[test]
    fn bgra_2x2_flip_and_swap() {
        let bottom: [u8; 8] = [255, 0, 0, 255, 255, 255, 255, 255];
        let top: [u8; 8] = [0, 0, 255, 255, 0, 255, 0, 255];
        let src: Vec<u8> = bottom.iter().chain(top.iter()).copied().collect();

        let out = bgra_to_rgba_top_down(&src, 2, 2, 8).expect("2x2 conversion");

        // Expected top-down RGBA:
        //   row 0 (top):    Red -> [255, 0, 0, 255]
        //                   Green -> [  0, 255, 0, 255]
        //   row 1 (bottom): Blue -> [  0,   0, 255, 255]
        //                   White -> [255, 255, 255, 255]
        let expected: [u8; 16] = [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        assert_eq!(out.as_slice(), expected.as_slice());
    }

    #[test]
    fn zero_dims_rejected() {
        let r = bgra_to_rgba_top_down(&[], 0, 0, 0);
        assert!(r.is_err());
    }

    #[test]
    fn stride_too_small_rejected() {
        // 2 px wide but stride of only 4 bytes (need 8).
        let r = bgra_to_rgba_top_down(&[0; 8], 2, 1, 4);
        assert!(r.is_err());
    }

    #[test]
    fn buffer_too_short_rejected() {
        // stride 8, 2 rows need 16 bytes, give only 8.
        let r = bgra_to_rgba_top_down(&[0; 8], 2, 2, 8);
        assert!(r.is_err());
    }

    #[test]
    fn single_row_identity() {
        // 1x1, bottom-up == top-down for a single row; BGRA -> RGBA.
        let src: [u8; 4] = [10, 20, 30, 40]; // B=10 G=20 R=30 A=40
        let out = bgra_to_rgba_top_down(&src, 1, 1, 4).expect("1x1");
        assert_eq!(out, vec![30, 20, 10, 40]); // R=30 G=20 B=10 A=40
    }

    /// The reference environment: 100% scale, exact 1366x820 client.
    #[cfg(windows)]
    #[test]
    fn capture_environment_accepts_reference_conditions() {
        assert!(assert_capture_environment(1366, 820, 96).is_ok());
    }

    /// A scaled display (120 dpi = 125%) must be rejected with the scale named.
    #[cfg(windows)]
    #[test]
    fn capture_environment_rejects_scaled_display() {
        let err = assert_capture_environment(1366, 820, 120).expect_err("125% must be rejected");
        assert!(err.contains("125%"), "message names the scale: {err}");
    }

    /// A client clamped by the work area (e.g. 816 high) must be rejected with
    /// the clamp named, not silently captured as a cropped page.
    #[cfg(windows)]
    #[test]
    fn capture_environment_rejects_clamped_client() {
        let err =
            assert_capture_environment(1366, 816, 96).expect_err("clamped client must be rejected");
        assert!(err.contains("clamped"), "message names the cause: {err}");
    }
}
