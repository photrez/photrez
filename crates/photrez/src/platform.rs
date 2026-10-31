#![allow(unsafe_code)] // SAFETY: user32 cursor handles only; no memory crosses the boundary.
//! Canvas cursor policy.
//!
//! The frozen canvas hides the OS pointer under the brush/eraser ring
//! (`cursor: none`). GPUI's `CursorStyle` has no hidden variant and its
//! platform hide hook is not reachable from the host, so the native shell
//! installs a custom fully transparent cursor and sets it while a paint tool
//! hovers the canvas, restoring the arrow on leave. A real cursor handle is
//! used instead of `SetCursor(NULL)` so Windows always has a well-defined
//! cursor and the restore is explicit; no global `ShowCursor` counter is
//! touched.

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
static HIDDEN: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
type Hcursor = *mut std::ffi::c_void;

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn SetCursor(cursor: Hcursor) -> Hcursor;
    fn LoadCursorW(instance: *mut std::ffi::c_void, name: *const u16) -> Hcursor;
    fn CreateCursor(
        instance: *mut std::ffi::c_void,
        x_hotspot: i32,
        y_hotspot: i32,
        width: i32,
        height: i32,
        and_plane: *const std::ffi::c_void,
        xor_plane: *const std::ffi::c_void,
    ) -> Hcursor;
}

/// A 1x1 cursor whose AND plane keeps the screen and whose XOR plane is zero,
/// i.e. fully transparent. Created once and kept for the process lifetime.
#[cfg(windows)]
fn hidden_cursor() -> Hcursor {
    use std::sync::OnceLock;
    static CURSOR: OnceLock<usize> = OnceLock::new();
    let handle = CURSOR.get_or_init(|| unsafe {
        // One scan line of a 1x1 monochrome bitmap is padded to two bytes.
        let and_plane: [u8; 2] = [0xFF, 0x00];
        let xor_plane: [u8; 2] = [0x00, 0x00];
        CreateCursor(
            std::ptr::null_mut(),
            0,
            0,
            1,
            1,
            and_plane.as_ptr().cast(),
            xor_plane.as_ptr().cast(),
        ) as usize
    });
    *handle as Hcursor
}

#[cfg(windows)]
pub(crate) fn set_brush_cursor_hidden(hidden: bool) {
    if HIDDEN.swap(hidden, Ordering::Relaxed) == hidden {
        return;
    }
    // IDC_ARROW is MAKEINTRESOURCE(32512): the integer lives in the pointer.
    const IDC_ARROW: usize = 32512;
    let cursor = if hidden {
        hidden_cursor()
    } else {
        unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW as *const u16) }
    };
    unsafe {
        SetCursor(cursor);
    }
}

#[cfg(not(windows))]
pub(crate) fn set_brush_cursor_hidden(_hidden: bool) {}
