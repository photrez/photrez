// SPDX-License-Identifier: AGPL-3.0-or-later
// --- Native OS Cursor Override ---
//
// On Windows, WebView2 overrides the CSS cursor by handling WM_SETCURSOR
// in its own child window.  Tauri's built-in setCursorIcon calls SetCursor
// via tao, but WebView2's child window intercepts WM_SETCURSOR before
// the parent sees it.
//
// This module uses the Win32 SetCursor call from the main thread to
// forcibly set the cursor AFTER WebView2 has set its own.  SetCursor
// affects the calling thread's message queue, so calling it from the
// Tauri main thread (which owns the window) overrides the webview cursor.
//
// Drag-drop cursors (copy/move) are NOT standard LoadCursor system cursors.
// The authentic native ones are the OLE drag-drop cursors compiled into
// ole32.dll as undocumented cursor resources:
//   - resource #2 = MOVE  (arrow + document rectangle)
//   - resource #3 = COPY  (arrow + document rectangle + plus sign)
// These are exactly the cursors Windows Explorer shows during a file drag.
// ole32.dll is always loaded in a GUI/WebView2 process (COM), so we load
// them directly via LoadCursorW.  Their IDs are undocumented ("subject to
// change"), so we keep the .cur files as a final fallback.

#[cfg(not(windows))]
use tauri::Manager;
use tauri::{AppHandle, Runtime};

/// Tauri IPC command: override the native cursor to the given OS icon.
///
/// Call with icon = "copy" | "move" | "default" to switch the cursor.
/// Only affects Windows; no-op on macOS/Linux (those platforms respect
/// CSS cursor natively).
#[tauri::command]
pub(crate) fn set_native_cursor<R: Runtime>(
    _app: AppHandle<R>,
    icon: String,
) -> Result<(), String> {
    // IPC boundary validation: the frontend only sends known icons, but a
    // stale or malformed webview could send anything. Reject unknowns
    // instead of silently falling back to the default cursor (review #33).
    if !is_valid_cursor_icon(&icon) {
        return Err(format!("Invalid cursor icon: {}", icon));
    }

    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::SetCursor;

        // SAFETY (review #33): the handle comes from LoadCursorW /
        // LoadCursorFromFileW (system-owned cursors valid for the process
        // lifetime) or IDC_ARROW; never a freed or user-owned handle.
        // SetCursor affects only the calling thread's message queue; this
        // command runs on the Tauri main thread, which owns the window.
        #[cfg(debug_assertions)]
        {
            use std::sync::Once;
            static SCAN_ONCE: Once = Once::new();
            SCAN_ONCE.call_once(|| {
                debug_scan_cursor_resources();
            });
        }

        let hcursor = load_drag_cursor_win(&icon);
        SetCursor(hcursor);
    }

    #[cfg(not(windows))]
    {
        // Non-Windows: attempt Tauri's built-in setCursorIcon.
        if let Some(w) = _app.get_webview_window("main") {
            let _ = w.set_cursor_icon(match icon.as_str() {
                "copy" => tauri::cursor::CursorIcon::Copy,
                "move" => tauri::cursor::CursorIcon::Move,
                _ => tauri::cursor::CursorIcon::Default,
            });
        }
    }

    Ok(())
}

#[cfg(windows)]
unsafe fn load_drag_cursor_win(icon: &str) -> *mut core::ffi::c_void {
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::HMODULE;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        LoadCursorFromFileW, LoadCursorW, IDC_ARROW,
    };

    let res_id: u16 = match icon {
        "copy" => 3, // OLE drag-drop COPY: arrow + document + plus
        "move" => 2, // OLE drag-drop MOVE: arrow + document
        _ => return LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
    };

    // ole32.dll is always loaded in a GUI/WebView2 process (COM). Cache its
    // handle once per process. The cursor IDs are undocumented, so a failure
    // here falls through to the .cur files below instead of breaking.
    // HMODULE is a raw pointer and not Sync, so cache it as usize.
    static OLE32: OnceLock<usize> = OnceLock::new();
    let hmod = *OLE32.get_or_init(|| {
        let wide: Vec<u16> = "ole32.dll"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        GetModuleHandleW(wide.as_ptr()) as usize
    }) as HMODULE;
    if !hmod.is_null() {
        let h = LoadCursorW(hmod, res_id as *const u16); // MAKEINTRESOURCE(res_id)
        if !h.is_null() {
            return h;
        }
    }

    // Fallback: closest system .cur files (approximate drag cursors).
    let filename = match icon {
        "copy" => r"C:\Windows\Cursors\aero_link.cur",
        "move" => r"C:\Windows\Cursors\aero_move.cur",
        _ => return LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
    };
    let wide: Vec<u16> = filename.encode_utf16().chain(std::iter::once(0)).collect();
    let h = LoadCursorFromFileW(wide.as_ptr());
    if h.is_null() {
        LoadCursorW(std::ptr::null_mut(), IDC_ARROW)
    } else {
        h
    }
}

/// Only the three documented icons are accepted (review #33).
fn is_valid_cursor_icon(icon: &str) -> bool {
    matches!(icon, "copy" | "move" | "default")
}

/// Debug-only: scan system DLLs for cursor resources to find the
/// real drag-drop cursor IDs (copy/move are composited by Explorer,
/// but the raw cursors may live in imageres.dll / comctl32.dll).
#[cfg(windows)]
unsafe fn debug_scan_cursor_resources() {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        LoadImageW, IMAGE_CURSOR, LR_DEFAULTSIZE, LR_SHARED,
    };

    let dlls: &[&str] = &["imageres.dll", "shell32.dll", "comctl32.dll"];
    for dll in dlls {
        let wide: Vec<u16> = dll.encode_utf16().chain(std::iter::once(0)).collect();
        let hmod = GetModuleHandleW(wide.as_ptr());
        if hmod.is_null() {
            println!("[cursor_rs] scan: {} not loaded", dll);
            continue;
        }
        for id in 1..400u16 {
            let h = LoadImageW(
                hmod,
                id as *const u16,
                IMAGE_CURSOR,
                0,
                0,
                LR_DEFAULTSIZE | LR_SHARED,
            );
            if !h.is_null() {
                println!("[cursor_rs] scan: {}.{} is a CURSOR ({:p})", dll, id, h);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_valid_cursor_icon;

    #[test]
    fn accepts_documented_icons() {
        assert!(is_valid_cursor_icon("copy"));
        assert!(is_valid_cursor_icon("move"));
        assert!(is_valid_cursor_icon("default"));
    }

    #[test]
    fn rejects_unknown_icons() {
        assert!(!is_valid_cursor_icon(""));
        assert!(!is_valid_cursor_icon("Copy")); // case-sensitive
        assert!(!is_valid_cursor_icon("hand"));
        assert!(!is_valid_cursor_icon("default; DROP TABLE"));
    }
}
