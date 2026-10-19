//! # photrez - the native application entry point
//!
//! This file does five things and nothing else:
//!
//!   1. `#![windows_subsystem = "windows"]` unconditionally - a debug build
//!      that flashes a console window on top of the app is a defect that
//!      hides itself in release, and the whole port is developed in debug.
//!   2. `gpui_kit::application().run(...)` -> `gpui_kit::init(cx)` before any
//!      control is constructed.
//!   3. Settle the startup locale: stored preference, then the OS locale,
//!      then `en` (`i18n::initial_locale`).
//!   4. Register Inter, the same font the web build loads.
//!   5. Open the main window on [`app::App`].
//!
//! It must not own: document state (`app/` + `photrez-core`), canvas painting
//! (`render/`), or input policy (`input/`).

#![windows_subsystem = "windows"]

mod app;
mod assets;
mod components;
mod i18n;
mod input;
mod render;
mod services;
mod shot;
mod state;
mod tokens;
mod views;

#[cfg(test)]
mod nav_regression_tests;

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

/// Parses `--shot <id>` from the process command-line arguments.
/// Unknown flags and a missing value are ignored silently; the app never
/// crashes on bad arguments.
fn parse_shot_arg() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1; // skip the program name
    while i < args.len() {
        if args[i] == "--shot" {
            if i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
            return None; // `--shot` with no value: ignored
        }
        i += 1;
    }
    None
}

fn main() {
    if let Some(id) = parse_shot_arg() {
        shot::set_active(id);
    }
    gpui_kit::application().run(|cx: &mut App| {
        gpui_kit::init(cx);

        // Settle the locale before any control reads a string: the whole
        // window tree renders from the catalog (`i18n/`).
        let settings = services::settings::FileSettings::user_default();
        i18n::set_locale(i18n::initial_locale(
            settings
                .as_ref()
                .map(|store| store as &dyn services::settings::SettingsStore),
        ));

        // Rasterise the logo once, before any window opens. `title_bar::init`
        // fills the `LOGO_IMAGE` cache that `title_bar::logo` reads per frame;
        // the same call the appearance reference's `main.rs` makes.
        views::title_bar::init(&cx.svg_renderer());

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

        #[allow(clippy::expect_used)]
        // startup cannot proceed without a window; the message is the diagnostic
        gpui_kit::open_window(options, cx, |_window, cx| cx.new(app::App::new))
            .expect("failed to open window");
    });
}
