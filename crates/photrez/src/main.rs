//! # photrez - the native application entry point
//!
//! This file does four things and nothing else:
//!
//!   1. `#![windows_subsystem = "windows"]` unconditionally - a debug build
//!      that flashes a console window on top of the app is a defect that
//!      hides itself in release, and the whole port is developed in debug.
//!   2. `gpui_kit::application().run(...)` -> `gpui_kit::init(cx)` before any
//!      control is constructed.
//!   3. Register Inter, the same font the web build loads.
//!   4. Open the main window on [`app::App`].
//!
//! It must not own: document state (`app/` + `photrez-core`), canvas painting
//! (`render/`), or input policy (`input/`).

#![windows_subsystem = "windows"]

mod app;
mod components;
mod i18n;
mod input;
mod render;
mod services;
mod views;

use gpui_kit::{px, size, App, AppContext, TitlebarOptions, WindowBounds, WindowOptions};

/// Inter, the same file the web build loads through
/// `apps/desktop/public/fonts/fonts.css` - as TTF, because DirectWrite's
/// `CreateInMemoryFontFileReference` rejects the woff2 container with
/// `0x88985000` (see the appearance reference's `main.rs` and
/// `tools/woff2-to-ttf.py`).
pub(crate) const INTER: &[u8] = include_bytes!("../assets/inter-latin.ttf");

/// Client-area size to open at: the baseline viewport of the parity harness.
const CLIENT_W: f32 = 1366.;
const CLIENT_H: f32 = 820.;

fn main() {
    gpui_kit::application().run(|cx: &mut App| {
        gpui_kit::init(cx);

        app::load_fonts(cx);

        // `appears_transparent` hands the whole top edge to our own title bar.
        // Without it the window shows two title bars: the OS caption plus the
        // ported 46px one, including an inert second set of window buttons.
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Photrez".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::centered(size(px(CLIENT_W), px(CLIENT_H)), cx)),
            is_resizable: true,
            ..Default::default()
        };

        gpui_kit::open_window(options, cx, |_window, cx| cx.new(app::App::new))
            .expect("failed to open window");
    });
}
