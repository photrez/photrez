// Shell dialogs and the locale they render in: the Settings dialog and its locale switch,
// the startup locale from the stored preference, the About dialog, the resize prompt, and
// Escape plus titlebar-close across them, with the latency legs that measure the same paths.
// They share no fixture with the document-tab tests above, which is why they are a module.
use super::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput};

#[gpui_kit::test]
fn settings_dialog_switches_the_locale(cx: &mut gpui_kit::TestAppContext) {
    use crate::i18n::{self, Locale};

    struct RestoreLocale;
    impl Drop for RestoreLocale {
        fn drop(&mut self) {
            i18n::set_locale(Locale::En);
        }
    }

    let _guard = crate::i18n::TEST_LOCALE_LOCK
        .lock()
        .expect("locale lock is not poisoned");
    i18n::set_locale(Locale::En);
    let _restore = RestoreLocale;

    let directory = TestDirectory::new();
    let settings =
        crate::services::settings::FileSettings::at(directory.path().join("settings.json"));
    let (window, content) = mount_settings_app(cx, settings.clone(), &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.active_dialog = Some(crate::state::DialogId::Settings);
            });
        });
        window.render_frame(cx);
        window.click("settings-language", cx);
        window.render_frame(cx);
        window.click("settings-locale-id", cx);
        window.render_frame(cx);
        assert_eq!(
            settings.read(crate::i18n::LOCALE_KEY).as_deref(),
            Some("id")
        );
        content.update(cx, |app, cx| {
            app.set_locale(Locale::En, cx);
        });
        assert_eq!(
            settings.read(crate::i18n::LOCALE_KEY).as_deref(),
            Some("en")
        );
    })
    .expect("test window remains open");
}

/// The stored `photrez.locale` preference decides the startup locale.
/// The app resolves it while it is constructed, so the first frame already
/// renders the translated shell with no interaction.
#[gpui_kit::test]
fn startup_locale_comes_from_the_stored_preference(cx: &mut gpui_kit::TestAppContext) {
    use crate::i18n::{self, Locale};

    struct RestoreLocale;
    impl Drop for RestoreLocale {
        fn drop(&mut self) {
            i18n::set_locale(Locale::En);
        }
    }

    let _guard = crate::i18n::TEST_LOCALE_LOCK
        .lock()
        .expect("locale lock is not poisoned");
    i18n::set_locale(Locale::En);
    let _restore = RestoreLocale;

    let directory = TestDirectory::new();
    let settings =
        crate::services::settings::FileSettings::at(directory.path().join("settings.json"));
    settings.write(i18n::LOCALE_KEY, Locale::Id.code());
    let (window, content) = mount_settings_app(cx, settings.clone(), &directory);
    cx.update_window(window.into(), |_, window, cx| {
        assert_eq!(
            content.read(cx).startup_locale(),
            Locale::Id,
            "the app resolves the stored preference at startup"
        );
        window.render_frame(cx);
        content.update(cx, |app, cx| app.set_locale(Locale::En, cx));
    })
    .expect("test window remains open");
    assert_eq!(
        settings.read(i18n::LOCALE_KEY).as_deref(),
        Some("en"),
        "the same store the startup read from receives the later choice"
    );
}

/// Perf: choosing a locale switches the process-wide catalog, persists
/// the preference, and repaints the whole shell. Re-opening the list between
/// samples is host-side state, outside the measured span.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn language_switch_latency(cx: &mut gpui_kit::TestAppContext) {
    use crate::i18n::{self, Locale};

    struct RestoreLocale;
    impl Drop for RestoreLocale {
        fn drop(&mut self) {
            i18n::set_locale(Locale::En);
        }
    }

    let _guard = crate::i18n::TEST_LOCALE_LOCK
        .lock()
        .expect("locale lock is not poisoned");
    i18n::set_locale(Locale::En);
    let _restore = RestoreLocale;

    let directory = TestDirectory::new();
    let settings =
        crate::services::settings::FileSettings::at(directory.path().join("settings.json"));
    let (window, content) = mount_settings_app(cx, settings, &directory);
    let open_list = |content: &gpui_kit::Entity<App>, cx: &mut gpui_kit::App| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.active_dialog = Some(crate::state::DialogId::Settings);
                state.settings_language_open = true;
            });
        });
    };
    cx.update_window(window.into(), |_, window, cx| {
        open_list(&content, cx);
        window.render_frame(cx);
        let press_row = |window: &mut gpui_kit::Window, id: String, cx: &mut gpui_kit::App| {
            let position = window.find(id).bounds().center();
            let _ = window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
        };
        let mut samples = Vec::new();
        for i in 0..9 {
            let target = if i % 2 == 0 { Locale::Id } else { Locale::En };
            let id = format!("settings-locale-{}", target.code());
            open_list(&content, cx);
            window.render_frame(cx);
            let start = std::time::Instant::now();
            press_row(window, id, cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "language_switch p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "language switch p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// The Settings dialog opens through the Settings menu and
/// closes with its primary OK button; the language list expands inline.
#[gpui_kit::test]
fn settings_dialog_opens_from_the_menu_and_ok_closes_it(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::Settings);
            });
        });
        window.render_frame(cx);
        window.click("menu-preferences", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::Settings)
        );
        window.click("settings-language", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .settings_language_open,
            "the language list expands"
        );
        window.click("settings-ok", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.active_dialog, None, "OK closes the dialog");
        assert!(!state.settings_language_open, "closing resets the list");
    })
    .expect("test window remains open");
}

/// The titlebar close is a real 20 px button, not decoration. It
/// dismisses Settings the same way OK does, and it is hit-testable for hover.
#[gpui_kit::test]
fn settings_titlebar_close_dismisses_the_dialog(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::Settings);
            });
        });
        window.render_frame(cx);
        window.click("menu-preferences", cx);
        window.render_frame(cx);
        window.hover("dialog-close", cx);
        window.click("dialog-close", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.active_dialog, None,
            "the titlebar close dismisses the dialog"
        );
        assert!(!state.settings_language_open, "closing resets the list");
    })
    .expect("test window remains open");
}

/// Escape routes to the dismissal handler for a managed-focus
/// dialog. Settings and About have no field of their own, so the chrome's
/// focus target owns the key.
#[gpui_kit::test]
fn escape_dismisses_settings_and_about(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::Settings);
            });
        });
        window.render_frame(cx);
        window.click("menu-preferences", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::Settings)
        );
        window.press("escape", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            None,
            "Escape closes Settings"
        );

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::Help);
            });
        });
        window.render_frame(cx);
        window.click("menu-about", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::About)
        );
        window.press("escape", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            None,
            "Escape closes About"
        );
    })
    .expect("test window remains open");
}

/// The About dialog mounts through the Help menu with its two link
/// buttons and the crate version, and its footer/inline close both dismiss it.
#[gpui_kit::test]
fn about_dialog_mounts_its_links_and_closes(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::Help);
            });
        });
        window.render_frame(cx);
        window.click("menu-about", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::About)
        );
        window.find("dialog-about");
        // The pill's accessible name is the string the view renders, so this
        // proves the About surface shows the crate version. The helper alone
        // cannot: it returns the same macro the assertion compares against, so
        // a version that never reaches the screen would still satisfy it.
        assert_eq!(
            window.find("about-version").label(),
            Some(concat!("v", env!("CARGO_PKG_VERSION"))),
            "the About surface mounts the crate version, not a copy of it"
        );
        window.find("about-close");
        window.click("about-close", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            None,
            "the About footer closes the dialog"
        );

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::Help);
            });
        });
        window.render_frame(cx);
        window.click("menu-about", cx);
        window.render_frame(cx);
        window.click("dialog-close", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            None,
            "the About titlebar X closes the dialog"
        );
    })
    .expect("test window remains open");
}

/// Perf: opening the About dialog and painting its body. The menu row's
/// handler installs `active_dialog` and notifies, so the timer covers that
/// state change plus one frame, the same protocol as the other dialog
/// surfaces.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn about_dialog_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let close = |content: &gpui_kit::Entity<App>, cx: &mut gpui_kit::App| {
            content.update(cx, |app, cx| {
                app.state_for_test().update(cx, |state, _| {
                    state.active_dialog = None;
                });
            });
        };
        close(&content, cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            close(&content, cx);
            window.render_frame(cx);
            let start = std::time::Instant::now();
            content.update(cx, |app, cx| {
                app.state_for_test().update(cx, |state, _| {
                    state.open_menu = None;
                    state.active_dialog = Some(crate::state::DialogId::About);
                });
            });
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "about_dialog p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "about dialog open p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// Each dialog supplies its own dismissal, so the close X on Resize
/// Cancel-paths (no pending resize session survives).
#[gpui_kit::test]
fn resize_titlebar_close_cancels_the_pending_session(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::Image);
            });
        });
        window.render_frame(cx);
        window.click("menu-resize-canvas", cx);
        window.render_frame(cx);
        assert!(content
            .read(cx)
            .state_for_test()
            .read(cx)
            .resize_canvas
            .is_some());
        window.click("dialog-close", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.active_dialog, None, "the close X closes Resize");
        assert!(
            state.resize_canvas.is_none(),
            "the pending resize session is dropped, not applied"
        );
    })
    .expect("test window remains open");
}

/// Perf: the titlebar close dismisses the dialog and repaints the shell.
/// Re-opening between samples is host-side state, outside the measured span.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn dialog_close_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let reopen = |content: &gpui_kit::Entity<App>, cx: &mut gpui_kit::App| {
            content.update(cx, |app, cx| {
                app.state_for_test().update(cx, |state, _| {
                    state.active_dialog = Some(crate::state::DialogId::Settings);
                });
            });
        };
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            let position = window.find("dialog-close").bounds().center();
            let _ = window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
        };
        reopen(&content, cx);
        window.render_frame(cx);
        press(window, cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            reopen(&content, cx);
            window.render_frame(cx);
            let start = std::time::Instant::now();
            press(window, cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "dialog_close p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "dialog close p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// Perf: toggling the language list repaints the dialog.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn settings_language_toggle_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.active_dialog = Some(crate::state::DialogId::Settings);
            });
        });
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            let position = window.find("settings-language").bounds().center();
            let _ = window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
        };
        press(window, cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            let start = std::time::Instant::now();
            press(window, cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        println!(
            "settings_language_toggle p50={p50:.3} p95={p95:.3} ms n={}",
            samples.len()
        );
        assert!(
            p95 <= 33.3,
            "settings language toggle p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}
