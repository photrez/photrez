//! The brush ring window. Windows owns a small layered popup that follows the
//! pointer, so a brush hover does not repaint the shell. The ring is
//! rasterised here from the same screen-space shape the painted fallback uses.
#![allow(unsafe_code)] // SAFETY: Win32 window, DIB and layered-window calls only.
#![cfg_attr(not(windows), allow(dead_code))] // The rasteriser runs on every platform; its window does not.

use crate::display::canvas_bridge::BrushRing;

/// A ring larger than this many physical pixels stays on the painted path:
/// a very large layered bitmap costs more to upload than it saves.
#[cfg_attr(not(windows), allow(dead_code))]
const MAX_RING_PX: i32 = 2048;

pub(crate) fn show(ring: BrushRing, scale: f32) -> bool {
    imp::show(ring, scale)
}

pub(crate) fn hide() {
    imp::hide();
}

pub(crate) fn visible() -> bool {
    imp::visible()
}

/// The ring shape quantised to the values that change its bitmap. Position is
/// deliberately absent: moving the window moves the existing bitmap.
#[derive(Clone, Copy, Debug, PartialEq)]
struct RingKey {
    rx: f32,
    ry: f32,
    rotation: f32,
    scale: f32,
    crosshair: bool,
}

fn quantize(ring: BrushRing, scale: f32) -> RingKey {
    let quantise = |value: f32, step: f32| (value / step).round() * step;
    RingKey {
        rx: quantise(ring.radii[0].abs() * scale, 0.25).max(0.25),
        ry: quantise(ring.radii[1].abs() * scale, 0.25).max(0.25),
        rotation: quantise(ring.rotation, 1.0 / 1024.0),
        scale: quantise(scale, 1.0 / 16.0).max(1.0 / 16.0),
        crosshair: ring.crosshair,
    }
}

impl RingKey {
    fn bitmap_size(self) -> Option<i32> {
        let half = self.rx.max(self.ry).max(4.0 * self.scale) + 0.75 * self.scale + 1.0;
        let size = (half.ceil() as i32).checked_mul(2)?;
        (8..=MAX_RING_PX).contains(&size).then_some(size)
    }
}

/// Rasterise an antialiased black-under-white ring and crosshair into
/// premultiplied BGRA. The white stroke is the visible line; the black stroke
/// supplies contrast on light documents.
fn rasterize(key: RingKey) -> Option<Vec<u8>> {
    let size = key.bitmap_size()?;
    let pixels_len = (size as usize).checked_mul(size as usize)?.checked_mul(4)?;
    if key.rx <= 0.0 || key.ry <= 0.0 {
        return None;
    }
    let center = size as f32 / 2.0;
    let black_half = 0.75 * key.scale;
    let white_half = 0.5 * key.scale;
    let cross_half = 4.0 * key.scale;
    let cross_black_half = 1.5 * key.scale;
    let cross_white_half = 0.5 * key.scale;
    let (sin, cos) = key.rotation.sin_cos();
    let inv_rx = 1.0 / key.rx;
    let inv_ry = 1.0 / key.ry;
    let mut pixels = vec![0u8; pixels_len];
    for y in 0..size {
        let screen_y = y as f32 + 0.5 - center;
        for x in 0..size {
            let screen_x = x as f32 + 0.5 - center;
            let local_x = screen_x * cos + screen_y * sin;
            let local_y = -screen_x * sin + screen_y * cos;
            let normalised = ((local_x * inv_rx).powi(2) + (local_y * inv_ry).powi(2)).sqrt();
            let gradient =
                ((local_x * inv_rx * inv_rx).powi(2) + (local_y * inv_ry * inv_ry).powi(2)).sqrt();
            let distance = if gradient > 1e-6 {
                (normalised - 1.0) / gradient
            } else {
                normalised - 1.0
            };
            let mut black = band_coverage(distance, black_half);
            let mut white = band_coverage(distance, white_half);
            if key.crosshair {
                black = black.max(cross_coverage(
                    screen_x,
                    screen_y,
                    cross_half,
                    cross_black_half,
                ));
                white = white.max(cross_coverage(
                    screen_x,
                    screen_y,
                    cross_half,
                    cross_white_half,
                ));
            }
            let black_alpha = black * 0.5;
            let alpha = white + black_alpha * (1.0 - white);
            if alpha <= 0.0 {
                continue;
            }
            // White over black: the black stroke only contributes alpha.
            let color = (white * 255.0).round().clamp(0.0, 255.0) as u8;
            let alpha = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
            let index = ((y as usize * size as usize) + x as usize) * 4;
            pixels[index] = color;
            pixels[index + 1] = color;
            pixels[index + 2] = color;
            pixels[index + 3] = alpha;
        }
    }
    Some(pixels)
}

fn band_coverage(distance: f32, half_width: f32) -> f32 {
    let start = (distance - 0.5).max(-half_width);
    let end = (distance + 0.5).min(half_width);
    (end - start).clamp(0.0, 1.0)
}

fn cross_coverage(x: f32, y: f32, half_length: f32, half_width: f32) -> f32 {
    let horizontal =
        axis_coverage(x, -half_length, half_length) * axis_coverage(y, -half_width, half_width);
    let vertical =
        axis_coverage(x, -half_width, half_width) * axis_coverage(y, -half_length, half_length);
    horizontal.max(vertical)
}

fn axis_coverage(value: f32, low: f32, high: f32) -> f32 {
    ((value + 0.5).min(high) - (value - 0.5).max(low)).clamp(0.0, 1.0)
}

#[cfg(all(windows, not(test)))]
#[allow(clippy::missing_const_for_thread_local)] // Already const; this toolchain's lint misfires.
mod imp {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::sync::Once;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        BLENDFUNCTION, DIB_RGB_COLORS, RGBQUAD,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, GetWindowLongPtrW, IsWindow,
        RegisterClassW, SetWindowPos, ShowWindow, UpdateLayeredWindow, GWLP_HINSTANCE,
        HWND_TOPMOST, MA_NOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE,
        SW_SHOWNOACTIVATE, ULW_ALPHA, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_SETCURSOR, WNDCLASSW,
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
    };

    use super::{rasterize, RingKey};
    use crate::display::canvas_bridge::BrushRing;

    const CLASS_NAME: &str = "PhotrezBrushRing";

    struct State {
        hwnd: HWND,
        key: Option<RingKey>,
        visible: bool,
    }

    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }

    pub(super) fn show(ring: BrushRing, scale: f32) -> bool {
        if !scale.is_finite() || scale <= 0.0 {
            hide();
            return false;
        }
        let key = super::quantize(ring, scale);
        let Some(size) = key.bitmap_size() else {
            hide();
            return false;
        };
        let Some(owner) = crate::app::cursor::root_window_at_cursor() else {
            hide();
            return false;
        };
        let Some(hwnd) = ensure_window(owner) else {
            hide();
            return false;
        };
        let Some((cursor_x, cursor_y)) = cursor_position() else {
            hide();
            return false;
        };
        let (current_key, was_visible) = STATE.with(|state| {
            state
                .borrow()
                .as_ref()
                .map_or((None, false), |window| (window.key, window.visible))
        });
        if current_key != Some(key) {
            let Some(pixels) = rasterize(key) else {
                hide();
                return false;
            };
            if !update_layered_window(hwnd, &pixels, size, cursor_x, cursor_y) {
                hide();
                return false;
            }
            STATE.with(|state| {
                if let Some(window) = state.borrow_mut().as_mut() {
                    window.key = Some(key);
                    window.visible = true;
                }
            });
            if !was_visible {
                unsafe {
                    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
            }
            return true;
        }
        let left = cursor_x - size / 2;
        let top = cursor_y - size / 2;
        let moved = unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                left,
                top,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        };
        if moved == 0 {
            hide();
            return false;
        }
        if !was_visible {
            unsafe {
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
        }
        STATE.with(|state| {
            if let Some(window) = state.borrow_mut().as_mut() {
                window.visible = true;
            }
        });
        true
    }

    pub(super) fn hide() {
        STATE.with(|state| {
            if let Some(window) = state.borrow_mut().as_mut() {
                if window.visible {
                    unsafe {
                        ShowWindow(window.hwnd, SW_HIDE);
                    }
                    window.visible = false;
                }
            }
        });
    }

    pub(super) fn visible() -> bool {
        STATE.with(|state| state.borrow().as_ref().is_some_and(|window| window.visible))
    }

    fn cursor_position() -> Option<(i32, i32)> {
        let mut point = POINT { x: 0, y: 0 };
        (unsafe { GetCursorPos(&mut point) } != 0).then_some((point.x, point.y))
    }

    fn ensure_window(owner: HWND) -> Option<HWND> {
        let existing = STATE.with(|state| state.borrow().as_ref().map(|window| window.hwnd));
        if let Some(hwnd) = existing {
            // The ring sits above the pointer, so WindowFromPoint can name the
            // ring instead of its owner. A live window keeps the owner it was
            // created with; only a destroyed one is rebuilt.
            if unsafe { IsWindow(hwnd) } != 0 {
                return Some(hwnd);
            }
            STATE.with(|state| {
                if let Some(window) = state.borrow_mut().take() {
                    unsafe {
                        DestroyWindow(window.hwnd);
                    }
                }
            });
        }
        let instance = unsafe { GetWindowLongPtrW(owner, GWLP_HINSTANCE) };
        if instance == 0 {
            return None;
        }
        let class = wide(CLASS_NAME);
        static REGISTER: Once = Once::new();
        REGISTER.call_once(|| {
            let class = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(ring_wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: 0,
                hCursor: 0,
                hbrBackground: 0,
                lpszMenuName: std::ptr::null(),
                lpszClassName: class.as_ptr(),
            };
            unsafe {
                RegisterClassW(&class);
            }
        });
        let title = wide("photrez brush ring");
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                owner,
                0,
                instance,
                std::ptr::null(),
            )
        };
        if hwnd == 0 {
            return None;
        }
        STATE.with(|state| {
            *state.borrow_mut() = Some(State {
                hwnd,
                key: None,
                visible: false,
            });
        });
        Some(hwnd)
    }

    fn update_layered_window(
        hwnd: HWND,
        pixels: &[u8],
        size: i32,
        cursor_x: i32,
        cursor_y: i32,
    ) -> bool {
        let screen_dc = unsafe { GetDC(0) };
        if screen_dc == 0 {
            return false;
        }
        let memory_dc = unsafe { CreateCompatibleDC(screen_dc) };
        if memory_dc == 0 {
            unsafe {
                ReleaseDC(0, screen_dc);
            }
            return false;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }],
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let bitmap = unsafe { CreateDIBSection(memory_dc, &info, DIB_RGB_COLORS, &mut bits, 0, 0) };
        if bitmap == 0 || bits.is_null() {
            unsafe {
                DeleteDC(memory_dc);
                ReleaseDC(0, screen_dc);
            }
            return false;
        }
        let expected = (size as usize) * (size as usize) * 4;
        if pixels.len() != expected {
            unsafe {
                DeleteObject(bitmap);
                DeleteDC(memory_dc);
                ReleaseDC(0, screen_dc);
            }
            return false;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
        }
        let previous = unsafe { SelectObject(memory_dc, bitmap) };
        let destination = POINT {
            x: cursor_x - size / 2,
            y: cursor_y - size / 2,
        };
        let source = POINT { x: 0, y: 0 };
        let window_size = SIZE { cx: size, cy: size };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let updated = unsafe {
            UpdateLayeredWindow(
                hwnd,
                screen_dc,
                &destination,
                &window_size,
                memory_dc,
                &source,
                0,
                &blend,
                ULW_ALPHA,
            )
        };
        unsafe {
            SelectObject(memory_dc, previous);
            DeleteObject(bitmap);
            DeleteDC(memory_dc);
            ReleaseDC(0, screen_dc);
        }
        updated != 0
    }

    unsafe extern "system" fn ring_wndproc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_SETCURSOR | WM_ERASEBKGND => 1,
            WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(not(all(windows, not(test))))]
mod imp {
    use crate::display::canvas_bridge::BrushRing;

    pub(super) fn show(_ring: BrushRing, _scale: f32) -> bool {
        false
    }

    pub(super) fn hide() {}

    pub(super) fn visible() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(rx: f32, ry: f32, rotation: f32, scale: f32, crosshair: bool) -> RingKey {
        RingKey {
            rx,
            ry,
            rotation,
            scale,
            crosshair,
        }
    }

    #[test]
    fn bitmap_size_covers_the_ring_and_crosshair() {
        let ring = key(20.0, 20.0, 0.0, 1.0, true);
        assert_eq!(ring.bitmap_size(), Some(44));
        let tiny = key(1.0, 1.0, 0.0, 1.0, true);
        assert_eq!(tiny.bitmap_size(), Some(12));
        let larger = key(100.0, 60.0, 0.0, 1.0, false);
        assert_eq!(larger.bitmap_size(), Some(204));
    }

    #[test]
    fn a_very_large_ring_stays_on_the_painted_path() {
        assert_eq!(key(2000.0, 2000.0, 0.0, 1.0, true).bitmap_size(), None);
    }

    #[test]
    fn the_ring_has_white_pixels_outer_alpha_and_a_clear_centre() {
        let ring = key(20.0, 20.0, 0.0, 1.0, true);
        let pixels = rasterize(ring).expect("the ring rasterises");
        let size = ring.bitmap_size().expect("sized") as usize;
        let mut strongest = 0u8;
        let mut soft = false;
        for pixel in pixels.as_chunks::<4>().0.iter() {
            strongest = strongest.max(pixel[3]);
            if pixel[3] > 0 && pixel[3] < 200 {
                soft = true;
            }
        }
        assert!(
            strongest >= 128,
            "the white stroke reaches a visible alpha: {strongest}"
        );
        assert!(soft, "the black underlay keeps a soft outer edge");
        let centre = (size / 2 * size + size / 2) * 4;
        assert!(pixels[centre + 3] > 0, "the brush centre crosshair paints");
        let eraser = key(20.0, 20.0, 0.0, 1.0, false);
        let pixels = rasterize(eraser).expect("the eraser rasterises");
        assert_eq!(pixels[centre + 3], 0, "the eraser leaves the centre clear");
    }

    #[test]
    fn rotation_changes_the_ring_bitmap() {
        let upright = rasterize(key(40.0, 20.0, 0.0, 1.0, false)).expect("upright");
        let turned = rasterize(key(40.0, 20.0, 0.6, 1.0, false)).expect("turned");
        assert_ne!(upright, turned);
    }

    #[test]
    fn quantize_matches_logical_radii_at_the_window_scale() {
        let ring = BrushRing {
            center: [10.0, 20.0],
            radii: [20.0, 10.0],
            rotation: 0.0,
            crosshair: true,
        };
        let key = quantize(ring, 1.5);
        assert_eq!(key.rx, 30.0);
        assert_eq!(key.ry, 15.0);
        assert_eq!(key.scale, 1.5);
        assert!(key.crosshair);
    }

    #[test]
    fn the_shape_key_tracks_every_bitmap_input() {
        let base = key(20.0, 20.0, 0.0, 1.0, false);
        assert_ne!(base, key(21.0, 20.0, 0.0, 1.0, false));
        assert_ne!(base, key(20.0, 21.0, 0.0, 1.0, false));
        assert_ne!(base, key(20.0, 20.0, 0.1, 1.0, false));
        assert_ne!(base, key(20.0, 20.0, 0.0, 1.5, false));
        assert_ne!(base, key(20.0, 20.0, 0.0, 1.0, true));
    }

    #[test]
    #[ignore = "performance measurement; run explicitly"]
    fn ring_rasterize_latency() {
        let ring = key(200.0, 120.0, 0.4, 1.5, true);
        let mut samples = Vec::new();
        for _ in 0..12 {
            let start = std::time::Instant::now();
            let pixels = rasterize(ring).expect("the ring rasterises");
            assert!(!pixels.is_empty());
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        println!(
            "ring_rasterize p50={p50:.3} p95={p95:.3} ms n={}",
            samples.len()
        );
        assert!(p95 < 50.0, "ring rasterisation p95 {p95:.3} ms");
    }
}
