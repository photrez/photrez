//! # photrez - the native application library
//!
//! Owns the application composition shared by every entry point:
//!
//!   1. `gpui_kit::application().run(...)` -> `gpui_kit::init(cx)` before any
//!      control is constructed.
//!   2. Settle the startup locale: stored preference, then the OS locale,
//!      then `en` (`i18n::initial_locale`).
//!   3. Register Inter, the font the reference captures were shaped with.
//!   4. Open the main window on [`app::App`].
//!
//! [`run`] is the single startup path. `src/main.rs` is a thin binary over it;
//! integration tests and measurement binaries link this library instead of
//! re-implementing startup.
//!
//! The startup path (`run`) must not own: document state (`app/` +
//! `photrez-core`), canvas painting (`render/`), or input policy (`input/`).

pub mod app;
pub mod components;
pub(crate) mod crop_aspect;
pub mod i18n;
pub mod icons;
pub mod input;
pub mod palette;
pub mod render;
pub mod services;
pub mod shot;
pub mod state;
pub mod tokens;
pub mod units;
pub mod views;

#[cfg(test)]
mod nav_regression_tests;

use gpui_kit::{px, size, App, AppContext, TitlebarOptions, WindowBounds, WindowOptions};
use std::ffi::OsString;
use std::path::PathBuf;

use services::settings::SettingsStore as _;

/// Inter, the face the design's typography specifies - as TTF, because
/// DirectWrite's
/// `CreateInMemoryFontFileReference` rejects the woff2 container with
/// `0x88985000`, so the bundled file is converted to TTF ahead of time.
pub(crate) const INTER: &[u8] = include_bytes!("../assets/inter-latin.ttf");

/// Client-area size to open at: the baseline viewport of the capture harness.
const CLIENT_W: f32 = 1366.;
const CLIENT_H: f32 = 820.;

struct StartupArgs {
    shot: Option<String>,
    open_path: Option<PathBuf>,
}

fn parse_startup_args(args: impl IntoIterator<Item = OsString>) -> StartupArgs {
    let mut args = args.into_iter().peekable();
    let mut shot = None;
    let mut open_path = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--shot")
                if args
                    .peek()
                    .is_some_and(|value| !value.to_string_lossy().starts_with("--")) =>
            {
                shot = args.next().and_then(|value| value.into_string().ok())
            }
            Some("--open")
                if args
                    .peek()
                    .is_some_and(|value| !value.to_string_lossy().starts_with("--")) =>
            {
                open_path = args.next().map(PathBuf::from)
            }
            _ => {}
        }
    }
    let has_shot = shot.is_some();
    StartupArgs {
        shot,
        open_path: if has_shot { None } else { open_path },
    }
}

/// Runs the application: toolkit init, locale, fonts, and the main window.
///
/// This is the only startup path. `main.rs` calls it; tests and measurement
/// binaries link the same library so they exercise production composition.
pub fn run() {
    let startup_args = parse_startup_args(std::env::args_os().skip(1));
    if let Some(id) = startup_args.shot {
        shot::set_active(id);
    }
    let open_path = startup_args.open_path;
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

        // Restore the geometry the last session left behind, when it still
        // lands on a display; otherwise open centered at the baseline size.
        let displays = cx.displays();
        let layout: Vec<_> = displays
            .iter()
            .map(|display| display.visible_bounds())
            .collect();
        let restored = settings
            .as_ref()
            .and_then(|store| store.read(services::window_state::KEY))
            .and_then(|text| services::window_state::decode(&text))
            .and_then(|geometry| services::window_state::restore(geometry, &layout));
        let (window_bounds, display_id) = match restored {
            // The record names the monitor it belongs to. Creating the window
            // against that monitor (instead of the primary) is what lets the
            // platform validate a placement on a second screen.
            Some((index, geometry)) => (
                // A maximized record carries its restore bounds, so the window
                // opens maximized without losing the size behind it.
                if geometry.maximized {
                    WindowBounds::Maximized(geometry.bounds())
                } else {
                    WindowBounds::Windowed(geometry.bounds())
                },
                displays.get(index).map(|display| display.id()),
            ),
            None => (
                WindowBounds::centered(size(px(CLIENT_W), px(CLIENT_H)), cx),
                None,
            ),
        };

        // `appears_transparent` hands the whole top edge to our own title bar.
        // Without it the window shows two title bars: the OS caption plus the
        // the native 46px one, including an inert second set of window buttons.
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Photrez".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            window_bounds: Some(window_bounds),
            display_id,
            is_resizable: true,
            ..Default::default()
        };

        #[allow(clippy::expect_used)]
        // startup cannot proceed without a window; the message is the diagnostic
        gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| app::App::new(window, settings, open_path, cx))
        })
        .expect("failed to open window");
    });
}

#[cfg(test)]
mod startup_args_tests {
    use super::*;
    use std::ffi::OsString;

    fn parse(args: &[&str]) -> StartupArgs {
        parse_startup_args(args.iter().map(OsString::from))
    }

    #[test]
    fn explicit_open_path_is_parsed() {
        assert_eq!(
            parse(&["--open", "image.ptz"]).open_path,
            Some("image.ptz".into())
        );
    }

    #[test]
    fn open_without_value_is_ignored() {
        assert_eq!(parse(&["--open"]).open_path, None);
    }

    #[test]
    fn open_without_value_does_not_consume_a_following_shot() {
        let args = parse(&["--open", "--shot", "dock-closed"]);
        assert_eq!(args.shot.as_deref(), Some("dock-closed"));
        assert_eq!(args.open_path, None);
    }

    #[test]
    fn shot_without_value_does_not_consume_a_following_open() {
        let args = parse(&["--shot", "--open", "image.ptz"]);
        assert_eq!(args.shot, None);
        assert_eq!(args.open_path, Some("image.ptz".into()));
    }

    #[test]
    fn unknown_positional_arguments_are_ignored() {
        assert_eq!(parse(&["image.ptz", "--unknown"]).open_path, None);
    }

    #[test]
    fn shot_takes_precedence_when_open_comes_first() {
        let args = parse(&["--open", "image.ptz", "--shot", "dock-closed"]);
        assert_eq!(args.shot.as_deref(), Some("dock-closed"));
        assert_eq!(args.open_path, None);
    }

    #[test]
    fn shot_takes_precedence_when_open_comes_last() {
        let args = parse(&["--shot", "dock-closed", "--open", "image.ptz"]);
        assert_eq!(args.shot.as_deref(), Some("dock-closed"));
        assert_eq!(args.open_path, None);
    }
}
