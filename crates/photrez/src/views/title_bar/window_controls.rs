use gpui_kit::base::h_flex;
use gpui_kit::base::StyledExt;
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Decorations, Window, WindowControlArea};

use crate::icons;
use crate::tokens::{color, title};

/// `<div class="flex shrink-0 items-center pr-0 text-editor-icon">` with three
/// `h-[46px] w-11` children, where controls render.
///
/// Each 46px button overhangs the 45px content box by 0.5px; `overflow-hidden`
/// clips it. `manifest.json`: Minimize at `y = -0.5`, height 46.
///
/// The original paints the same buttons everywhere because Tauri's
/// `decorations: false` removes window chrome; native toolkits own controls
/// differently, so this follows each platform's convention:
///
/// | platform | what owns the controls |
/// |---|---|
/// | Windows | these three, but driven by the OS via hit-test codes |
/// | macOS | AppKit's traffic lights — this renders an empty cell |
/// | Linux, client-decorated | these three, driven by imperative `Window` calls |
/// | Linux, server-decorated | the window manager — empty cell again |
///
/// A compositor may deny client decorations or fall back to server-side; the WM
/// then draws its controls. Keep this cell empty unless `window_decorations()`
/// reports `Client`.
///
/// # Why Windows alone uses `window_control_area`
///
/// `window_control_area` registers the cursor region, not a click handler;
/// `WM_NCHITTEST` maps it to `HTMINBUTTON` / `HTMAXBUTTON` / `HTCLOSE`, so
/// Windows performs the action and supplies hover, snap-layouts and accessibility
/// semantics that GPUI `on_click` cannot reproduce. It is the only supported
/// platform: `on_hit_test_window_control` is empty in `gpui-pre-macos` and
/// `gpui-pre-linux/x11` / `gpui-pre-linux/wayland`, which use explicit calls.
/// Registration does not paint, preserving Windows pixels.
pub(super) fn window_controls(window: &mut Window) -> impl IntoElement {
    let cell = h_flex()
        .id("window-controls")
        .flex_shrink_0()
        .items_center()
        .text_color(color::ICON);

    // GPUI hides the title and sets `titlebarAppearsTransparent` but leaves
    // `NSTitledWindowMask`; drawing controls would duplicate AppKit's traffic lights.
    if cfg!(target_os = "macos") {
        return cell;
    }

    // On Linux, the WM already drew server-side controls. The `&&` avoids
    // querying `window_decorations()` on Windows, where it reports `Server`.
    if cfg!(target_os = "linux") && matches!(window.window_decorations(), Decorations::Server) {
        return cell;
    }

    cell.child(window_button(
        "minimize-window",
        icons::MINUS,
        title::ICON_15,
        WindowControlArea::Min,
    ))
    .child(window_button(
        "maximize-window",
        icons::SQUARE,
        title::ICON_12,
        WindowControlArea::Max,
    ))
    .child(close_button())
}

/// Issue a window-management action imperatively.
/// Only the Linux branch calls this. Windows answers it inside
/// `WM_NCHITTEST` and macOS inside AppKit, so neither needs a handler.
fn run_window_action(area: WindowControlArea, window: &mut Window) {
    match area {
        WindowControlArea::Min => window.minimize_window(),
        WindowControlArea::Max => window.zoom_window(),
        WindowControlArea::Close => window.remove_window(),
        WindowControlArea::Drag => window.start_window_move(),
    }
}

/// `flex h-[46px] w-11 items-center justify-center
///  hover:bg-white/[0.055] hover:text-editor-text`
fn window_button(
    id: &'static str,
    icon: &'static [u8],
    icon_size: f32,
    area: WindowControlArea,
) -> impl IntoElement {
    div()
        .id(id)
        .h_flex()
        .h(px(title::WINDOW_BTN_H))
        .w(px(title::WINDOW_BTN_W))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .hover(|s| s.bg(color::WHITE_55).text_color(color::TEXT))
        .when(cfg!(target_os = "windows"), move |this| {
            this.window_control_area(area)
        })
        .when(cfg!(target_os = "linux"), move |this| {
            this.on_click(move |_, window, _| run_window_action(area, window))
        })
        .child(
            Icon::default()
                .data(icon)
                .size(px(icon_size))
                .text_color(color::ICON),
        )
}

/// `flex h-[46px] w-11 items-center justify-center hover:bg-red-500/85
///  hover:text-white`
fn close_button() -> impl IntoElement {
    div()
        .id("close-window")
        .h_flex()
        .h(px(title::WINDOW_BTN_H))
        .w(px(title::WINDOW_BTN_W))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .hover(|s| s.bg(color::RED_500_85).text_color(color::TEXT))
        .when(cfg!(target_os = "windows"), |this| {
            this.window_control_area(WindowControlArea::Close)
        })
        .when(cfg!(target_os = "linux"), |this| {
            this.on_click(|_, window, _| run_window_action(WindowControlArea::Close, window))
        })
        .child(
            Icon::default()
                .data(icons::X)
                .size(px(title::ICON_16))
                .text_color(color::ICON),
        )
}
