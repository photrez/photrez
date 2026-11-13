//! The native hover cursor. Windows gets an OS-drawn rotate direction cursor
//! and can hide the system pointer while the painted brush ring is the
//! pointer; the other platforms keep the painted sprite because their
//! backends do not expose custom cursor images. The hover and press geometry
//! share one adaptive band, and leaving a hover releases the custom cursor at
//! once.

#![allow(clippy::missing_const_for_thread_local)] // Clippy 1.98 still flags const { ... } thread_local initializers
#![allow(unsafe_code)] // SAFETY: Win32 cursor resource creation and SetCursor only; the resource buffer is static and aligned.

#[cfg(test)]
use std::cell::Cell;

#[cfg(windows)]
mod imp {
    use std::cell::Cell;
    use std::sync::{Mutex, OnceLock};

    #[cfg(not(test))]
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
    #[cfg(not(test))]
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, DefWindowProcW, GetAncestor, GetCursorPos, SetWindowLongPtrW,
        WindowFromPoint, GA_ROOT, GWLP_WNDPROC,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateIconFromResourceEx, LoadCursorW, SetCursor, HCURSOR, HTCLIENT, IDC_ARROW,
        LR_DEFAULTCOLOR, LR_DEFAULTSIZE, WM_SETCURSOR,
    };

    /// RT_CURSOR resources: one record per 22.5-degree direction. Each record is
    /// a 4-byte LOCALHEADER (hotspot 16, 16) followed by a 32x32 32bpp DIB of
    /// `rotate_cursor_polygon(0.0)` centred on the hotspot and rotated by the
    /// record's direction. The declaration is 8-aligned because
    /// CreateIconFromResourceEx requires a DWORD-aligned buffer.
    const DIRECTIONS: usize = 16;
    const RECORD_LEN: usize = 4268;
    const RESOURCE_LEN: usize = DIRECTIONS * RECORD_LEN;
    #[repr(align(8))]
    struct Aligned([u8; RESOURCE_LEN]);
    static RESOURCE: Aligned = Aligned(*include_bytes!(
        "../../assets/cursors/rotate_directions.curres"
    ));
    const _: () = assert!(
        include_bytes!("../../assets/cursors/rotate_directions.curres").len() == RESOURCE_LEN
    );

    static CURSORS: OnceLock<[Option<HCURSOR>; DIRECTIONS]> = OnceLock::new();
    static ARROW: OnceLock<Option<HCURSOR>> = OnceLock::new();

    /// RT_CURSOR resources for eyedropper, bucket, and crop tool icons.
    /// icon rather than a crosshair. Each record is a LOCALHEADER plus a
    /// 32x32 32bpp DIB, drawn from the app's own tool glyphs.
    const TOOL_COUNT: usize = 3;
    const TOOL_RESOURCE_LEN: usize = TOOL_COUNT * RECORD_LEN;
    #[repr(align(8))]
    struct ToolAligned([u8; TOOL_RESOURCE_LEN]);
    static TOOL_RESOURCE: ToolAligned =
        ToolAligned(*include_bytes!("../../assets/cursors/tool_cursors.curres"));
    const _: () = assert!(
        include_bytes!("../../assets/cursors/tool_cursors.curres").len() == TOOL_RESOURCE_LEN
    );

    #[cfg(not(test))]
    static TOOL_CURSORS: OnceLock<[Option<HCURSOR>; TOOL_COUNT]> = OnceLock::new();

    /// Which window cursor the subclass owns. The modes are exclusive: a tool
    /// shows its rotate cursor, its tool icon, or the brush's hidden pointer.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Default,
        Rotate,
        Hidden,
        Tool(u8),
    }

    thread_local! {
        static MODE: Cell<Mode> = const { Cell::new(Mode::Default) };
        static DIRECTION: Cell<u8> = const { Cell::new(0) };
    }

    fn build_all() -> [Option<HCURSOR>; DIRECTIONS] {
        std::array::from_fn(|index| {
            let start = index * RECORD_LEN;
            let handle = unsafe {
                CreateIconFromResourceEx(
                    RESOURCE.0[start..].as_ptr(),
                    RECORD_LEN as u32,
                    0,
                    0x0003_0000,
                    0,
                    0,
                    LR_DEFAULTCOLOR | LR_DEFAULTSIZE,
                )
            };
            (handle != 0).then_some(handle as HCURSOR)
        })
    }

    #[cfg(test)]
    pub(super) fn build_for_test() -> [Option<HCURSOR>; DIRECTIONS] {
        build_all()
    }

    fn build_tools() -> [Option<HCURSOR>; TOOL_COUNT] {
        std::array::from_fn(|index| {
            let start = index * RECORD_LEN;
            let handle = unsafe {
                CreateIconFromResourceEx(
                    TOOL_RESOURCE.0[start..].as_ptr(),
                    RECORD_LEN as u32,
                    0,
                    0x0003_0000,
                    0,
                    0,
                    LR_DEFAULTCOLOR | LR_DEFAULTSIZE,
                )
            };
            (handle != 0).then_some(handle as HCURSOR)
        })
    }

    #[cfg(test)]
    pub(super) fn build_tools_for_test() -> [Option<HCURSOR>; TOOL_COUNT] {
        build_tools()
    }

    fn cursor_for(direction: u8) -> Option<HCURSOR> {
        CURSORS.get_or_init(build_all)[usize::from(direction % DIRECTIONS as u8)]
    }

    #[cfg(not(test))]
    fn tool_cursor_for(index: u8) -> Option<HCURSOR> {
        TOOL_CURSORS.get_or_init(build_tools)[usize::from(index) % TOOL_COUNT]
    }

    fn restore_arrow() {
        if let Some(arrow) = *ARROW.get_or_init(|| {
            let arrow = unsafe { LoadCursorW(0, IDC_ARROW) };
            (arrow != 0).then_some(arrow)
        }) {
            unsafe {
                SetCursor(arrow);
            }
        }
    }

    pub(super) fn set_rotate(active: bool, direction: u8) {
        let direction = direction % DIRECTIONS as u8;
        DIRECTION.with(|state| state.set(direction));
        if active {
            MODE.with(|state| state.set(Mode::Rotate));
            if let Some(cursor) = cursor_for(direction) {
                unsafe {
                    SetCursor(cursor);
                }
            }
            #[cfg(not(test))]
            ensure_subclass();
        } else {
            let was_rotate = MODE.with(|state| {
                if state.get() == Mode::Rotate {
                    state.set(Mode::Default);
                    true
                } else {
                    false
                }
            });
            if was_rotate {
                // Leave the custom cursor immediately; WM_SETCURSOR for the
                // next position would otherwise keep it until the next move.
                restore_arrow();
            }
        }
    }

    /// Hide the system pointer while the painted brush ring marks it.
    pub(super) fn set_brush_hidden(active: bool) {
        if active {
            MODE.with(|state| state.set(Mode::Hidden));
            // Tests must not hide the desktop pointer.
            #[cfg(not(test))]
            unsafe {
                SetCursor(0);
            }
            #[cfg(not(test))]
            ensure_subclass();
        } else {
            let was_hidden = MODE.with(|state| {
                if state.get() == Mode::Hidden {
                    state.set(Mode::Default);
                    true
                } else {
                    false
                }
            });
            if was_hidden {
                #[cfg(not(test))]
                restore_arrow();
            }
        }
    }

    /// Swap the pointer for a tool icon while it is over the canvas.
    pub(super) fn set_tool_cursor(index: Option<u8>) {
        match index {
            Some(index) if usize::from(index) < TOOL_COUNT => {
                MODE.with(|state| state.set(Mode::Tool(index)));
                #[cfg(not(test))]
                {
                    if let Some(cursor) = tool_cursor_for(index) {
                        unsafe {
                            SetCursor(cursor);
                        }
                    }
                    ensure_subclass();
                }
            }
            Some(_) => set_tool_cursor(None),
            None => {
                let was_tool = MODE.with(|state| {
                    if matches!(state.get(), Mode::Tool(_)) {
                        state.set(Mode::Default);
                        true
                    } else {
                        false
                    }
                });
                if was_tool {
                    #[cfg(not(test))]
                    restore_arrow();
                }
            }
        }
    }

    pub(super) fn available() -> bool {
        CURSORS.get_or_init(build_all).iter().any(Option::is_some)
    }

    pub(super) fn set_photrez_pointer_active(_active: bool) {}

    /// Whether a WM_SETCURSOR message is a hover cursor's client-area hit.
    /// The subclass claims only this message; every other message chains to
    /// GPUI's window procedure.
    pub(super) fn wants_custom(message: u32, hit_test: u32, active: bool) -> bool {
        active && message == WM_SETCURSOR && hit_test == HTCLIENT
    }

    #[cfg(not(test))]
    type WndProc = unsafe extern "system" fn(isize, u32, WPARAM, LPARAM) -> LRESULT;

    #[cfg(not(test))]
    static PREVIOUS_WNDPROC: OnceLock<WndProc> = OnceLock::new();

    pub(super) fn install_once<T>(
        installed: &OnceLock<T>,
        install_lock: &Mutex<()>,
        install: impl FnOnce() -> Option<T>,
    ) -> bool {
        if installed.get().is_some() {
            return true;
        }
        let _guard = install_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if installed.get().is_some() {
            return true;
        }
        let Some(value) = install() else {
            return false;
        };
        installed.set(value).is_ok() || installed.get().is_some()
    }

    #[cfg(not(test))]
    unsafe extern "system" fn rotate_wndproc(
        hwnd: isize,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let hit_test = (lparam as u32) & 0xFFFF;
        let mode = MODE.with(Cell::get);
        if wants_custom(message, hit_test, mode != Mode::Default) {
            match mode {
                Mode::Rotate => {
                    if let Some(cursor) = cursor_for(DIRECTION.with(Cell::get)) {
                        unsafe {
                            SetCursor(cursor);
                        }
                        return 1;
                    }
                }
                Mode::Hidden => {
                    unsafe {
                        SetCursor(0);
                    }
                    return 1;
                }
                Mode::Tool(index) => {
                    if let Some(cursor) = tool_cursor_for(index) {
                        unsafe {
                            SetCursor(cursor);
                        }
                        return 1;
                    }
                }
                Mode::Default => {}
            }
        }
        match PREVIOUS_WNDPROC.get().copied() {
            Some(previous) => unsafe {
                CallWindowProcW(Some(previous), hwnd, message, wparam, lparam)
            },
            None => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    /// The top-level window under the pointer. The cursor subclass and the
    /// ring overlay both need the window that owns the canvas.
    #[cfg(not(test))]
    pub(super) fn root_window_at_cursor() -> Option<isize> {
        let mut point = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut point) } == 0 {
            return None;
        }
        let target = unsafe { WindowFromPoint(point) };
        if target == 0 {
            return None;
        }
        let root = unsafe { GetAncestor(target, GA_ROOT) };
        (root != 0).then_some(root)
    }

    /// Replace the app window's window procedure once. The replacement claims
    /// WM_SETCURSOR while either hover cursor is active and chains every other
    /// message to GPUI, so the policy cursor never overwrites it.
    #[cfg(not(test))]
    pub(super) fn ensure_subclass() -> bool {
        static INSTALL_LOCK: Mutex<()> = Mutex::new(());
        install_once(&PREVIOUS_WNDPROC, &INSTALL_LOCK, || {
            let root = root_window_at_cursor()?;
            let previous = unsafe {
                SetWindowLongPtrW(root, GWLP_WNDPROC, rotate_wndproc as *const () as isize)
            };
            if previous == 0 {
                eprintln!("photrez: cursor subclass failed to install");
                return None;
            }
            Some(unsafe { std::mem::transmute::<isize, WndProc>(previous) })
        })
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::cell::Cell;

    thread_local! {
        static PHOTREZ_POINTER: Cell<bool> = const { Cell::new(false) };
    }

    pub(super) fn set_rotate(_active: bool, _direction: u8) {}

    pub(super) fn set_brush_hidden(_active: bool) {}

    pub(super) fn set_tool_cursor(_index: Option<u8>) {}

    pub(super) fn available() -> bool {
        false
    }

    pub(super) fn set_photrez_pointer_active(active: bool) {
        let was = PHOTREZ_POINTER.with(|state| state.replace(active));
        if active != was {
            #[cfg(not(test))]
            {
                use objc2_app_kit::NSCursor;
                if active {
                    NSCursor::hide();
                } else {
                    NSCursor::unhide();
                }
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    pub(super) fn set_rotate(_active: bool, _direction: u8) {}

    pub(super) fn set_brush_hidden(_active: bool) {}

    pub(super) fn set_tool_cursor(_index: Option<u8>) {}

    pub(super) fn available() -> bool {
        false
    }

    pub(super) fn set_photrez_pointer_active(_active: bool) {}
}

/// Indices into the tool cursor resource.
pub(crate) const TOOL_EYEDROPPER: u8 = 0;
pub(crate) const TOOL_BUCKET: u8 = 1;
pub(crate) const TOOL_CROP: u8 = 2;
#[cfg(test)]
thread_local! {
    static LAST_REQUEST: Cell<Option<(bool, u8)>> = const { Cell::new(None) };
    static BRUSH_HIDDEN_REQUEST: Cell<Option<bool>> = const { Cell::new(None) };
    static BRUSH_HIDDEN_CALLS: Cell<u32> = const { Cell::new(0) };
    static TOOL_REQUEST: Cell<Option<Option<u8>>> = const { Cell::new(None) };
}

/// Ask the platform for the rotate cursor at `direction` (0..15, 22.5-degree
/// steps), or let the policy cursor return. The request is recorded for the
/// mounted tests even where no native cursor exists.
pub(crate) fn set_rotate(active: bool, direction: u8) {
    let direction = direction % 16;
    #[cfg(test)]
    LAST_REQUEST.with(|cell| cell.set(Some((active, direction))));
    imp::set_rotate(active, direction);
}

/// Hide the system pointer while the painted brush ring is visible. The
/// painted ring is the only pointer for these tools, so the OS arrow must not
/// sit on top of it.
pub(crate) fn set_brush_hidden(active: bool) {
    #[cfg(test)]
    {
        BRUSH_HIDDEN_REQUEST.with(|cell| cell.set(Some(active)));
        BRUSH_HIDDEN_CALLS.with(|cell| cell.set(cell.get() + 1));
    }
    imp::set_brush_hidden(active);
}

/// Show a tool icon cursor, or return to the policy cursor.
pub(crate) fn set_tool_cursor(index: Option<u8>) {
    #[cfg(test)]
    TOOL_REQUEST.with(|cell| cell.set(Some(index)));
    imp::set_tool_cursor(index);
}

/// Whether this build can replace the painted sprite with an OS-drawn cursor.
pub(crate) fn native_rotate_available() -> bool {
    imp::available()
}

/// Whether this build installs OS-drawn tool icon cursors over the canvas.
pub(crate) fn native_tool_cursor_available() -> bool {
    cfg!(windows)
}
/// The tool cursor resource index for this tool and policy, if any.
pub(crate) fn tool_cursor_index(
    tool: crate::state::ToolId,
    policy: crate::input_policy::CursorPolicy,
) -> Option<u8> {
    use crate::input_policy::CursorPolicy;
    match (tool, policy) {
        (crate::state::ToolId::Eyedropper, CursorPolicy::Crosshair) => Some(TOOL_EYEDROPPER),
        (crate::state::ToolId::PaintBucket, CursorPolicy::Crosshair) => Some(TOOL_BUCKET),
        (crate::state::ToolId::Crop, CursorPolicy::Crosshair | CursorPolicy::SystemDefault) => {
            Some(TOOL_CROP)
        }
        _ => None,
    }
}

/// Hide or show the platform pointer while Photrez paints the only cursor.
pub(crate) fn set_photrez_pointer_active(active: bool) {
    imp::set_photrez_pointer_active(active);
}

/// The top-level window under the pointer, for the ring overlay's owner.
#[cfg(all(windows, not(test)))]
pub(crate) fn root_window_at_cursor() -> Option<isize> {
    imp::root_window_at_cursor()
}

/// The 22.5-degree bucket of `point` about `center`, with 0 at the right and
/// increasing clockwise. The rotate cursor uses it to face the hovered side
/// or corner.
pub(crate) fn direction_bucket(center: (f64, f64), point: (f64, f64)) -> u8 {
    let angle = (point.1 - center.1).atan2(point.0 - center.0);
    let bucket = (angle / (std::f64::consts::PI / 8.0)).round();
    bucket.rem_euclid(16.0) as u8
}

#[cfg(test)]
pub(crate) fn take_last_request_for_test() -> Option<(bool, u8)> {
    LAST_REQUEST.with(|cell| cell.take())
}

#[cfg(test)]
pub(crate) fn take_brush_hidden_request_for_test() -> Option<bool> {
    BRUSH_HIDDEN_REQUEST.with(|cell| cell.take())
}

#[cfg(test)]
pub(crate) fn take_brush_hidden_calls_for_test() -> u32 {
    BRUSH_HIDDEN_CALLS.with(|cell| cell.replace(0))
}

#[cfg(test)]
pub(crate) fn take_tool_cursor_request_for_test() -> Option<Option<u8>> {
    TOOL_REQUEST.with(|cell| cell.take())
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyCursor, HCURSOR};

    const RESOURCE: &[u8] = include_bytes!("../../assets/cursors/rotate_directions.curres");

    /// The embedded resources start with the cursor hotspot and a 32x32,
    /// 32bpp DIB whose height covers the XOR and AND masks.
    #[test]
    fn cursor_resources_are_sixteen_32px_alpha_bitmaps() {
        assert_eq!(RESOURCE.len(), 68288);
        for record in RESOURCE.as_chunks::<4268>().0.iter() {
            assert_eq!(u16::from_le_bytes([record[0], record[1]]), 16);
            assert_eq!(u16::from_le_bytes([record[2], record[3]]), 16);
            let dib = &record[4..];
            assert_eq!(u32::from_le_bytes(dib[0..4].try_into().unwrap()), 40);
            assert_eq!(i32::from_le_bytes(dib[4..8].try_into().unwrap()), 32);
            assert_eq!(i32::from_le_bytes(dib[8..12].try_into().unwrap()), 64);
            assert_eq!(u16::from_le_bytes(dib[14..16].try_into().unwrap()), 32);
        }
    }

    const TOOL_RESOURCE: &[u8] = include_bytes!("../../assets/cursors/tool_cursors.curres");

    /// The tool cursors are three LOCALHEADER plus 32x32 32bpp records with the
    /// action hotspots the tools use.
    #[test]
    fn tool_cursor_resources_are_three_32px_icons() {
        assert_eq!(TOOL_RESOURCE.len(), 12_804);
        let hotspots = [(4, 28), (24, 16), (9, 11)];
        for (record, (hotspot_x, hotspot_y)) in
            TOOL_RESOURCE.as_chunks::<4268>().0.iter().zip(hotspots)
        {
            assert_eq!(u16::from_le_bytes([record[0], record[1]]), hotspot_x);
            assert_eq!(u16::from_le_bytes([record[2], record[3]]), hotspot_y);
            let dib = &record[4..];
            assert_eq!(u32::from_le_bytes(dib[0..4].try_into().unwrap()), 40);
            assert_eq!(i32::from_le_bytes(dib[4..8].try_into().unwrap()), 32);
            assert_eq!(i32::from_le_bytes(dib[8..12].try_into().unwrap()), 64);
            assert_eq!(u16::from_le_bytes(dib[14..16].try_into().unwrap()), 32);
        }
    }

    #[cfg(windows)]
    #[test]
    fn every_tool_cursor_resource_creates_a_cursor_handle() {
        let cursors = super::imp::build_tools_for_test();
        for cursor in cursors.into_iter() {
            let cursor = cursor.expect("the tool cursor resource builds");
            unsafe {
                DestroyCursor(cursor);
            }
        }
    }

    #[test]
    fn tool_cursor_requests_are_recorded() {
        let _ = super::take_tool_cursor_request_for_test();
        super::set_tool_cursor(Some(super::TOOL_BUCKET));
        assert_eq!(
            super::take_tool_cursor_request_for_test(),
            Some(Some(super::TOOL_BUCKET))
        );
        super::set_tool_cursor(None);
        assert_eq!(super::take_tool_cursor_request_for_test(), Some(None));
    }

    #[cfg(windows)]
    #[test]
    fn every_direction_resource_creates_a_cursor_handle() {
        let cursors: [Option<HCURSOR>; 16] = super::imp::build_for_test();
        for cursor in cursors.into_iter() {
            let cursor = cursor.expect("the cursor resource builds");
            unsafe {
                DestroyCursor(cursor);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn the_subclass_claims_only_an_active_client_setcursor() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HTCLIENT, HTNOWHERE, WM_MOUSEMOVE, WM_SETCURSOR,
        };

        assert!(super::imp::wants_custom(WM_SETCURSOR, HTCLIENT, true));
        assert!(!super::imp::wants_custom(WM_SETCURSOR, HTCLIENT, false));
        assert!(!super::imp::wants_custom(WM_MOUSEMOVE, HTCLIENT, true));
        assert!(!super::imp::wants_custom(WM_SETCURSOR, HTNOWHERE, true));
    }

    #[cfg(windows)]
    #[test]
    fn failed_subclass_install_is_retried() {
        use std::{
            cell::Cell,
            sync::{Mutex, OnceLock},
        };

        let installed = OnceLock::new();
        let install_lock = Mutex::new(());
        let attempts = Cell::new(0);

        assert!(!super::imp::install_once(&installed, &install_lock, || {
            attempts.set(attempts.get() + 1);
            None::<()>
        }));
        assert!(super::imp::install_once(&installed, &install_lock, || {
            attempts.set(attempts.get() + 1);
            Some(())
        }));
        assert!(super::imp::install_once(&installed, &install_lock, || {
            panic!("an installed subclass must not install twice")
        }));
        assert_eq!(attempts.get(), 2);
    }

    #[test]
    fn direction_buckets_follow_the_hovered_side() {
        let center = (100.0, 75.0);
        assert_eq!(super::direction_bucket(center, (110.0, 75.0)), 0, "right");
        assert_eq!(
            super::direction_bucket(center, (110.0, 65.0)),
            14,
            "top-right"
        );
        assert_eq!(super::direction_bucket(center, (100.0, 65.0)), 12, "top");
        assert_eq!(super::direction_bucket(center, (90.0, 75.0)), 8, "left");
        assert_eq!(super::direction_bucket(center, (100.0, 85.0)), 4, "bottom");
    }
}
