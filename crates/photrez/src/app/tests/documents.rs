use super::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};

pub(super) fn open_second_document(
    app: &mut App,
    cx: &mut gpui_kit::Context<App>,
    directory: &TestDirectory,
) {
    let path = directory.path().join("second.ptz");
    std::fs::write(&path, super::commands::multi_layer_archive_with(false))
        .expect("second fixture is written");
    let file_io = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
        .expect("file boundary opens");
    file_io
        .approve_path(&path)
        .expect("second fixture is approved");
    app.open_startup_document_for_test(&path, &file_io, cx)
        .expect("the second document opens in a new tab");
}

#[gpui_kit::test]
fn opening_a_second_document_adds_a_tab_and_keeps_the_first(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            open_second_document(app, cx, &directory);
            assert_eq!(
                app.open_tab_ids_for_test(),
                vec!["parity-doc".to_string(), "multi-doc".to_string()]
            );
            assert_eq!(app.active_tab_id_for_test().as_deref(), Some("multi-doc"));
            assert_eq!(app.active_core_layer_count_for_test(), Some(2));
        });
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.tabs.len(), 2);
            assert_eq!(state.active_tab.as_deref(), Some("multi-doc"));
        }

        content.update(cx, |app, cx| {
            app.activate_document("parity-doc", cx)
                .expect("the first tab activates");
            assert_eq!(app.active_tab_id_for_test().as_deref(), Some("parity-doc"));
            assert_eq!(app.active_core_layer_count_for_test(), Some(1));
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn switching_tabs_restores_the_per_document_camera(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            app.set_camera(
                crate::render::canvas_bridge::Camera::new(11., 22., 2.).unwrap(),
                cx,
            );
            open_second_document(app, cx, &directory);
            assert!(
                app.camera_for_test().is_none(),
                "the new tab starts in fit mode"
            );
            app.activate_document("parity-doc", cx)
                .expect("switch back");
            assert_eq!(
                app.camera_for_test(),
                crate::render::canvas_bridge::Camera::new(11., 22., 2.)
            );
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn closing_a_clean_tab_activates_the_neighbour_and_dirty_tabs_prompt(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            open_second_document(app, cx, &directory);
            assert!(app.close_document_tab("multi-doc", cx));
            assert_eq!(app.open_tab_ids_for_test(), vec!["parity-doc"]);
            assert_eq!(app.active_tab_id_for_test().as_deref(), Some("parity-doc"));

            open_second_document(app, cx, &directory);
            app.mark_active_document_dirty_for_test()
                .expect("the active session accepts an edit");
            assert!(
                !app.close_document_tab("multi-doc", cx),
                "a dirty tab asks before closing"
            );
            assert_eq!(app.open_tab_ids_for_test().len(), 2);
            assert_eq!(
                app.state_for_test().read(cx).active_dialog,
                Some(crate::state::DialogId::DirtyClose)
            );
            app.dirty_close_cancel(cx);
            assert_eq!(app.state_for_test().read(cx).active_dialog, None);
            assert_eq!(app.open_tab_ids_for_test().len(), 2, "cancel keeps the tab");

            assert!(!app.close_document_tab("multi-doc", cx));
            app.dirty_close_discard(cx);
            assert_eq!(
                app.open_tab_ids_for_test().len(),
                1,
                "discard closes the tab"
            );
            assert_eq!(app.active_tab_id_for_test().as_deref(), Some("parity-doc"));

            open_second_document(app, cx, &directory);
            app.mark_active_document_dirty_for_test()
                .expect("the active session accepts an edit");
            assert!(!app.close_document_tab("multi-doc", cx));
            app.dirty_close_save(cx);
            assert_eq!(
                app.open_tab_ids_for_test().len(),
                1,
                "saving closes the tab"
            );
        });
    })
    .expect("test window remains open");
}

fn press_key(app: &mut App, cx: &mut gpui_kit::Context<App>, key: &str) {
    press_char(app, cx, key, key);
}

fn press_char(app: &mut App, cx: &mut gpui_kit::Context<App>, key: &str, text: &str) {
    let event = gpui_kit::KeyDownEvent {
        keystroke: gpui_kit::Keystroke {
            modifiers: gpui_kit::Modifiers::default(),
            key: key.to_string(),
            key_char: Some(text.to_string()),
        },
        is_held: false,
        prefer_character_input: false,
    };
    app.new_document_key(&event, cx);
}

fn press_backspace(app: &mut App, cx: &mut gpui_kit::Context<App>) {
    let event = gpui_kit::KeyDownEvent {
        keystroke: gpui_kit::Keystroke {
            modifiers: gpui_kit::Modifiers::default(),
            key: "backspace".to_string(),
            key_char: None,
        },
        is_held: false,
        prefer_character_input: false,
    };
    app.new_document_key(&event, cx);
}

#[gpui_kit::test]
fn new_document_dialog_creates_a_dirty_tab_with_the_chosen_preset(
    cx: &mut gpui_kit::TestAppContext,
) {
    use crate::state::NewDocCategory;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);
        window.click("new-doc-tab-1", cx);
        window.render_frame(cx);
        {
            let form = content
                .read(cx)
                .new_document_form_for_test(cx)
                .expect("the dialog owns a form");
            assert_eq!(form.category, NewDocCategory::SocialMedia);
            assert_eq!(form.category.presets()[0].name, "Facebook Post");
        }
        window.click("new-doc-preset-0", cx);
        window.render_frame(cx);
        {
            let form = content
                .read(cx)
                .new_document_form_for_test(cx)
                .expect("the dialog owns a form");
            assert_eq!(form.name, "Facebook Post", "a preset sets the name");
            assert_eq!((form.width_px, form.height_px), (1200, 1200));
        }
        window.click("new-doc-create", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.active_dialog, None, "Create closes the dialog");
        assert!(state.new_document.is_none(), "the form is discarded");
        assert_eq!(state.tabs.len(), 2, "the new document opens in a tab");
        assert!(state.tabs[1].dirty, "a created document starts dirty");
        assert_eq!(
            state.tabs[1].name, "Facebook Post",
            "the tab carries the form's name"
        );
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((1200, 1200))
        );
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test(),
            Some(1),
            "the app's new document is one Background layer"
        );
        assert!(content
            .read(cx)
            .active_document_id_for_test()
            .as_deref()
            .is_some_and(|id| id.starts_with("native-doc-")));
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn new_document_form_types_the_name_and_size_and_converts_units(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);

        window.click("new-doc-name", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            press_char(app, cx, "space", " ");
            for key in ["A", "r", "t"] {
                press_key(app, cx, key);
            }
        });
        window.render_frame(cx);
        {
            let form = content
                .read(cx)
                .new_document_form_for_test(cx)
                .expect("the dialog owns a form");
            assert_eq!(form.name, "New Project Art");
        }

        window.click("new-doc-width", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            for _ in 0..4 {
                press_backspace(app, cx);
            }
            for key in ["5", "0", "0"] {
                press_key(app, cx, key);
            }
        });
        window.click("new-doc-height", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            for _ in 0..4 {
                press_backspace(app, cx);
            }
            for key in ["2", "0", "0"] {
                press_key(app, cx, key);
            }
        });
        window.render_frame(cx);

        window.click("new-doc-unit", cx);
        window.render_frame(cx);
        window.click("new-doc-unit-cm", cx);
        window.render_frame(cx);
        {
            let form = content
                .read(cx)
                .new_document_form_for_test(cx)
                .expect("the dialog owns a form");
            assert_eq!((form.width_px, form.height_px), (500, 200));
            assert_eq!(form.width_text, "13.23", "500 px formats as 13.23 cm");
            assert_eq!(form.height_text, "5.29", "200 px formats as 5.29 cm");
        }

        window.click("new-doc-create", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((500, 200))
        );
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.tabs[1].name, "New Project Art");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn new_document_transparent_background_starts_with_an_empty_layer(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);
        window.click("new-doc-background", cx);
        window.render_frame(cx);
        window.click("new-doc-background-transparent", cx);
        window.render_frame(cx);
        window.click("new-doc-create", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_composite_pixel_for_test(0, 0),
            Some([0, 0, 0, 0]),
            "a transparent background leaves the layer empty"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn a_second_press_on_a_preset_creates_the_document(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);
        window.click("new-doc-preset-1", cx);
        window.render_frame(cx);
        window.click("new-doc-preset-1", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.active_dialog, None,
            "a double-click creates the document immediately"
        );
        assert_eq!(state.tabs.len(), 2);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((2480, 3508)),
            "the second press uses the A4 preset"
        );
    })
    .expect("test window remains open");
}

/// Preset press -> repaint. Ignored by default because parallel test load
/// inflates the percentiles; run with `--ignored --exact`.
#[gpui_kit::test]
#[ignore = "perf measurement; run serially with --ignored --exact"]
fn new_document_preset_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, index: usize| {
            let position = window
                .find(format!("new-doc-preset-{index}"))
                .bounds()
                .center();
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
        // One unmeasured warm-up press.
        press(window, cx, 1);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let index = sample % 2;
            let expected = if index == 0 { "A3" } else { "A4" };
            let start = std::time::Instant::now();
            press(window, cx, index);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(
                content
                    .read(cx)
                    .new_document_form_for_test(cx)
                    .map(|form| form.name),
                Some(expected.to_string()),
                "sample {sample} applies the preset"
            );
        }
        let mut sorted = samples.clone();
        sorted.sort_by(f64::total_cmp);
        let p50 = sorted[sorted.len() / 2];
        let p95 = sorted[(sorted.len() as f64 * 0.95).ceil() as usize - 1];
        eprintln!("S018_NEW_DOC_MS samples={samples:?} p50={p50:.3} p95={p95:.3}");
        assert!(
            p95 <= 33.3,
            "preset press p95 {p95:.3} ms exceeds the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_closes_an_open_dropdown_before_the_dialog(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);
        window.click("new-doc-unit", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .new_document_form_for_test(cx)
                .is_some_and(|form| form.unit_open),
            "the unit select opens"
        );

        window.press("escape", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(
                state.active_dialog,
                Some(crate::state::DialogId::NewDocument),
                "the first Escape only closes the dropdown"
            );
            assert_eq!(
                state.new_document.as_ref().map(|form| form.unit_open),
                Some(false)
            );
        }

        window.press("escape", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.active_dialog, None,
            "the second Escape closes the dialog"
        );
        assert!(state.new_document.is_none());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn edit_menu_rows_run_real_commands(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::Edit);
            });
        });
        window.render_frame(cx);
        window.click("menu-edit-select-all", cx);
        window.render_frame(cx);
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("Select All from the menu sets the core selection");
        assert_eq!((selection.width, selection.height), (2.0, 1.0));
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.open_menu, None, "the command closes the menu");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn export_dialog_chooses_format_and_quality(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.export_document_action(&crate::app::ExportDocument, cx);
            assert_eq!(
                app.state_for_test().read(cx).active_dialog,
                Some(crate::state::DialogId::Export)
            );
        });
        window.render_frame(cx);
        window.click("export-format-trigger", cx);
        window.render_frame(cx);
        window.click("export-format-jpg", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.export_format, crate::state::ExportFormat::Jpeg);
            assert_eq!(state.export_quality, 90);
            assert_eq!(state.active_dialog, Some(crate::state::DialogId::Export));
        }

        window.click("export-cancel", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            None
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn print_dialog_controls_paper_orientation_and_copies(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.print_document_action(&crate::app::PrintDocument, cx);
            assert_eq!(
                app.state_for_test().read(cx).active_dialog,
                Some(crate::state::DialogId::Print)
            );
        });
        window.render_frame(cx);
        // The paper list is inline behind its select, like the dialog's.
        window.click("print-paper", cx);
        window.render_frame(cx);
        window.click("print-paper-1", cx);
        window.render_frame(cx);
        window.click("print-orientation-landscape", cx);
        window.render_frame(cx);
        window.click("print-copies-inc", cx);
        window.render_frame(cx);
        window.click("print-unit", cx);
        window.render_frame(cx);
        window.click("print-unit-cm", cx);
        window.render_frame(cx);
        window.click("print-center", cx);
        window.render_frame(cx);
        window.click("print-scale-fit", cx);
        window.render_frame(cx);
        window.click("print-paper-white", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.print_paper, 1);
            assert!(state.print_landscape);
            assert_eq!(state.print_copies, 2);
            assert_eq!(state.print_unit, "cm");
            assert!(!state.print_center_on_page, "Center on Page toggles");
            assert!(!state.print_scale_to_fit, "Scale to Fit toggles");
            assert!(!state.print_show_paper_white, "Show Paper White toggles");
        }
        window.click("print-cancel", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            None
        );
    })
    .expect("test window remains open");
}

/// The Print dialog's Scale track writes the percentage (10..=400) and drops
/// Scale to Fit, matching the dialog's range input (min 10, max 400).
#[gpui_kit::test]
fn print_scale_track_writes_the_percentage(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.print_document_action(&crate::app::PrintDocument, cx);
        });
        window.render_frame(cx);
        let bounds = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .print_scale_slider_bounds
            .get()
            .expect("the Scale track is laid out");
        let x = bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * 0.5);
        let y = bounds.origin.y + bounds.size.height / 2.0;
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: gpui_kit::point(x, y),
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: gpui_kit::point(x, y),
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let state = content.read(cx).state_for_test();
        let scale = state.read(cx).print_scale_percent;
        assert!(
            (200..=205).contains(&scale),
            "halfway is about 205 %, got {scale}"
        );
        assert!(
            !state.read(cx).print_scale_to_fit,
            "dragging the Scale track clears Scale to Fit"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn print_dialog_latency(cx: &mut gpui_kit::TestAppContext) {
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
                app.print_document_action(&crate::app::PrintDocument, cx);
            });
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "print_dialog p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "print dialog p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn print_scale_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.print_document_action(&crate::app::PrintDocument, cx);
        });
        window.render_frame(cx);
        let bounds = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .print_scale_slider_bounds
            .get()
            .expect("the Scale track is laid out");
        let mut samples = Vec::new();
        for index in 0..9 {
            let fraction = if index % 2 == 0 { 0.3 } else { 0.7 };
            let x = bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * fraction);
            let y = bounds.origin.y + bounds.size.height / 2.0;
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: gpui_kit::point(x, y),
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: gpui_kit::point(x, y),
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "print_scale p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "print scale p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn opening_a_project_records_it_in_recent_files(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            open_second_document(app, cx, &directory);
            let recents = app.state_for_test().read(cx).recent_files.clone();
            let expected = directory.path().join("second.ptz");
            assert_eq!(recents.first().map(String::as_str), expected.to_str());
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::File);
            });
        });
        window.render_frame(cx);
        assert!(
            window.try_find("recent-file-0").is_some(),
            "the File menu lists the recent project"
        );
    })
    .expect("test window remains open");
}

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

/// The Export dialog's behaviour: the format list swaps
/// PNG/JPEG/WebP, JPEG reveals the quality track, PNG hides it, and a press on
/// the track writes the quality.
#[gpui_kit::test]
fn export_dialog_switches_format_and_quality(cx: &mut gpui_kit::TestAppContext) {
    use crate::state::ExportFormat;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.export_document_action(&crate::app::ExportDocument, cx);
        });
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::Export),
            "the Export action opens the dialog"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).export_format,
            ExportFormat::Png
        );
        assert!(
            window.try_find("export-quality").is_none(),
            "PNG has no quality track"
        );
        window.click("export-format-trigger", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("export-format-jpg").is_some(),
            "the format list opens under the trigger"
        );
        window.click("export-format-jpg", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).export_format,
            ExportFormat::Jpeg
        );
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .export_format_open,
            "picking a format closes the list"
        );
        assert!(
            window.try_find("export-quality").is_some(),
            "JPEG shows the quality track"
        );
        let bounds = window.find("export-quality").bounds();
        let point = gpui_kit::Point {
            x: bounds.origin.x + bounds.size.width * 0.25,
            y: bounds.origin.y + bounds.size.height / 2.0,
        };
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: point,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: point,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let quality = content.read(cx).state_for_test().read(cx).export_quality;
        assert!(
            (20..=30).contains(&quality),
            "a press a quarter into the track sets ~26%, got {quality}"
        );
        window.click("export-format-trigger", cx);
        window.render_frame(cx);
        window.click("export-format-png", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("export-quality").is_none(),
            "PNG hides the track again"
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

/// Perf: opening the export dialog and painting its body.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn export_dialog_latency(cx: &mut gpui_kit::TestAppContext) {
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
                app.export_document_action(&crate::app::ExportDocument, cx);
            });
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "export_dialog p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "export dialog open p95 {p95:.3} ms over the 33.3 ms budget"
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

#[gpui_kit::test]
fn dropped_project_paths_open_as_tabs(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let dropped = directory.path().join("dropped.ptz");
    std::fs::write(&dropped, super::commands::multi_layer_archive_with(false))
        .expect("dropped archive is written");
    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            app.file_io
                .clone()
                .expect("file boundary is open")
                .approve_path(&dropped)
                .expect("dropped path is approved");
            app.stage_dropped_paths(vec![dropped.clone()], gpui_kit::Point::default());
            app.submit_dropped_paths(cx);
            assert_eq!(
                app.open_tab_ids_for_test(),
                vec!["parity-doc".to_string(), "multi-doc".to_string()]
            );
            assert_eq!(app.active_tab_id_for_test().as_deref(), Some("multi-doc"));
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn tabs_reorder_without_changing_the_active_document(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            open_second_document(app, cx, &directory);
            assert!(app.move_document_tab("multi-doc", "parity-doc", cx));
            assert_eq!(
                app.open_tab_ids_for_test(),
                vec!["multi-doc".to_string(), "parity-doc".to_string()]
            );
            assert_eq!(
                app.active_tab_id_for_test().as_deref(),
                Some("multi-doc"),
                "reordering does not switch documents"
            );
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn resize_canvas_menu_opens_the_dialog_and_enter_resizes(cx: &mut gpui_kit::TestAppContext) {
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
        assert_eq!(
            content.read(cx).resize_fields_for_test(cx),
            Some(("2".to_string(), "1".to_string()))
        );
        // The dialog opens with `Keep proportions` on; unlock it so the two
        // fields stay independent for this case.
        window.click("resize-aspect", cx);
        window.render_frame(cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .resize_canvas
                .as_ref()
                .is_some_and(|session| session.aspect_locked),
            "the aspect lock toggles off"
        );
        window.press("backspace", cx);
        window.press("4", cx);
        window.click("resize-height", cx);
        window.render_frame(cx);
        window.press("backspace", cx);
        window.press("3", cx);
        window.render_frame(cx);
        let fields = content.read(cx).resize_fields_for_test(cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((4, 3)),
            "fields after typing: {fields:?}"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            None
        );
        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo resize");
        });
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((2, 1))
        );
    })
    .expect("test window remains open");
}

/// The unit list converts the display buffers with the dialog's own 96 DPI
/// base, the pixel value stays authoritative, and the aspect lock derives the
/// linked field from the one the user typed (default on, like the dialog).
#[gpui_kit::test]
fn resize_canvas_units_convert_and_the_aspect_lock_links_the_fields(
    cx: &mut gpui_kit::TestAppContext,
) {
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

        // Pixels reject a decimal point.
        window.press(".", cx);
        assert_eq!(
            content.read(cx).resize_fields_for_test(cx),
            Some(("2".to_string(), "1".to_string())),
            "px fields are whole numbers"
        );

        window.click("resize-unit", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).resize_unit_open,
            "the unit list expands"
        );
        window.click("resize-unit-cm", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).state_for_test().read(cx).resize_unit_open,
            "choosing a unit collapses the list"
        );
        assert_eq!(
            content.read(cx).resize_fields_for_test(cx),
            Some(("0.05".to_string(), "0.03".to_string())),
            "the buffers re-display in centimetres"
        );

        // 5 cm = 189 px; the locked 2:1 aspect derives height 95.
        window.click("resize-width", cx);
        for _ in 0..4 {
            window.press("backspace", cx);
        }
        window.press("5", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((189, 95)),
            "cm input converts to pixels and the aspect lock derives the height"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn resize_canvas_units_offer_points_and_unlocked_fields_stay_independent(
    cx: &mut gpui_kit::TestAppContext,
) {
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

        window.click("resize-unit", cx);
        window.render_frame(cx);
        window.click("resize-unit-pt", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).resize_fields_for_test(cx),
            Some(("1.50".to_string(), "0.75".to_string())),
            "points use the 96 DPI conversion"
        );

        window.click("resize-aspect", cx);
        window.render_frame(cx);
        window.click("resize-height", cx);
        for _ in 0..5 {
            window.press("backspace", cx);
        }
        window.press("1", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((2, 13)),
            "10 pt = 13 px and the unlocked width keeps its value"
        );
    })
    .expect("test window remains open");
}

/// Perf: switching the unit re-displays both fields and repaints the
/// dialog. Measured with the same down+up dispatch as the other surfaces so
/// the click helper's extra render does not inflate the samples.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn resize_unit_switch_latency(cx: &mut gpui_kit::TestAppContext) {
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
        let press = |window: &mut gpui_kit::Window, id: &'static str, cx: &mut gpui_kit::App| {
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
        // One unmeasured warm-up switch (open the list, then pick a unit).
        press(window, "resize-unit", cx);
        window.render_frame(cx);
        press(window, "resize-unit-cm", cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let id = if sample % 2 == 0 {
                "resize-unit-px"
            } else {
                "resize-unit-cm"
            };
            let start = std::time::Instant::now();
            press(window, "resize-unit", cx);
            window.render_frame(cx);
            press(window, id, cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        println!(
            "resize_unit_switch p50={p50:.3} p95={p95:.3} ms n={}",
            samples.len()
        );
        assert!(
            p95 <= 33.3,
            "resize unit switch p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// The shot shell shows its one document tab (37 px content height,
/// active underline), and a click on it stays on the same document. The
/// timed path is the same dispatch protocol as the menu surfaces.
#[gpui_kit::test]
fn document_tab_presentation_and_activation_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.seed_shot_tab());
            app.create_document(200, 150, cx).expect("blank document");
        });
        // create_document projects real tabs over the seeded row; re-seed so
        // the tab strip shows the display-only shot tab under test.
        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.seed_shot_tab());
        });
        window.render_frame(cx);
        let tab = window
            .try_find("document-tab-shot-document")
            .expect("the shot tab mounts");
        let bounds = tab.bounds();
        assert!(
            (f32::from(bounds.size.height) - 37.0).abs() < 0.5,
            "tab content height is 37 px, got {}",
            bounds.size.height
        );
        let position = bounds.center();
        let mut samples = Vec::new();
        for _ in 0..9 {
            let start = std::time::Instant::now();
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
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(
                window.try_find("document-tab-shot-document").is_some(),
                "the tab stays mounted after a click"
            );
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S055_TAB_CLICK_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn document_tab_drag_ghost_renders_during_active_drag(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.seed_shot_tab());
            app.create_document(200, 150, cx).expect("blank document");
        });
        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.seed_shot_tab());
        });
        window.render_frame(cx);
        let start = window.find("document-tab-shot-document").bounds().center();
        let moved = gpui_kit::point(start.x + gpui_kit::px(20.0), start.y);
        let _ = window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let _ = window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert!(cx.has_active_drag(), "the document-tab drag is active");
        let _ = window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
    })
    .expect("test window remains open");
}
