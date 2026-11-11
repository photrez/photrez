use super::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};

mod tab_reach;

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
                vec!["bounded-raster-doc".to_string(), "multi-doc".to_string()]
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
            app.activate_document("bounded-raster-doc", cx)
                .expect("the first tab activates");
            assert_eq!(
                app.active_tab_id_for_test().as_deref(),
                Some("bounded-raster-doc")
            );
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
                crate::display::canvas_bridge::Camera::new(11., 22., 2.).unwrap(),
                cx,
            );
            open_second_document(app, cx, &directory);
            assert!(
                app.camera_for_test().is_none(),
                "the new tab starts in fit mode"
            );
            app.activate_document("bounded-raster-doc", cx)
                .expect("switch back");
            assert_eq!(
                app.camera_for_test(),
                crate::display::canvas_bridge::Camera::new(11., 22., 2.)
            );
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn reloading_active_document_discards_its_pending_projection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.active_document
                .as_mut()
                .expect("fixture document is active")
                .canvas_stale = true;
            app.maybe_schedule_viewport_projection(cx);
            assert!(app.projection_key.is_some(), "projection was scheduled");

            let file_io = app.file_io.clone().expect("file boundary is open");
            app.open_startup_document_for_test(&source, file_io.as_ref(), cx)
                .expect("clean active document reloads");
            assert_eq!(
                app.active_document_id_for_test().as_deref(),
                Some("bounded-raster-doc")
            );
            assert!(
                app.projection_key.is_none(),
                "reload invalidates the old key"
            );
            assert!(
                app.projection_cache.is_none(),
                "reload discards the old source cache"
            );
        });
        window.render_frame(cx);
    })
    .expect("test window remains open");

    cx.run_until_parked();

    cx.update_window(window.into(), |_, _window, cx| {
        assert_eq!(
            content.read(cx).projection_rect_for_test(),
            None,
            "the old completion must not cover the reloaded canvas"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn pending_projection_is_accepted_after_switching_back_to_its_document(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let generation = content.update(cx, |app, cx| {
            app.active_document
                .as_mut()
                .expect("fixture document is active")
                .canvas_stale = true;
            app.maybe_schedule_viewport_projection(cx);
            assert!(app.projection_key.is_some(), "projection was scheduled");
            let generation = app.projection_generation;

            open_second_document(app, cx, &directory);
            assert_eq!(
                app.active_document_id_for_test().as_deref(),
                Some("multi-doc")
            );
            app.activate_document("bounded-raster-doc", cx)
                .expect("the original document activates");

            generation
        });
        window.render_frame(cx);
        content.update(cx, |app, _| {
            assert_eq!(
                app.projection_generation, generation,
                "switching away and back keeps the matching projection current"
            );
        });
    })
    .expect("test window remains open");

    cx.run_until_parked();

    cx.update_window(window.into(), |_, _window, cx| {
        assert_eq!(
            content.read(cx).projection_rect_for_test(),
            Some((0, 0, 2, 1)),
            "the pending result still belongs to the active document"
        );
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
            assert_eq!(app.open_tab_ids_for_test(), vec!["bounded-raster-doc"]);
            assert_eq!(
                app.active_tab_id_for_test().as_deref(),
                Some("bounded-raster-doc")
            );

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
            assert_eq!(
                app.active_tab_id_for_test().as_deref(),
                Some("bounded-raster-doc")
            );

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

fn press_key(app: &mut App, window: &gpui_kit::Window, cx: &mut gpui_kit::Context<App>, key: &str) {
    press_char(app, window, cx, key, key);
}

fn press_char(
    app: &mut App,
    window: &gpui_kit::Window,
    cx: &mut gpui_kit::Context<App>,
    key: &str,
    text: &str,
) {
    let event = gpui_kit::KeyDownEvent {
        keystroke: gpui_kit::Keystroke {
            modifiers: gpui_kit::Modifiers::default(),
            key: key.to_string(),
            key_char: Some(text.to_string()),
        },
        is_held: false,
        prefer_character_input: false,
    };
    app.new_document_key(&event, window, cx);
}

fn press_backspace(app: &mut App, window: &gpui_kit::Window, cx: &mut gpui_kit::Context<App>) {
    let event = gpui_kit::KeyDownEvent {
        keystroke: gpui_kit::Keystroke {
            modifiers: gpui_kit::Modifiers::default(),
            key: "backspace".to_string(),
            key_char: None,
        },
        is_held: false,
        prefer_character_input: false,
    };
    app.new_document_key(&event, window, cx);
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
            press_char(app, window, cx, "space", " ");
            for key in ["A", "r", "t"] {
                press_key(app, window, cx, key);
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
                press_backspace(app, window, cx);
            }
            for key in ["5", "0", "0"] {
                press_key(app, window, cx, key);
            }
        });
        window.click("new-doc-height", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            for _ in 0..4 {
                press_backspace(app, window, cx);
            }
            for key in ["2", "0", "0"] {
                press_key(app, window, cx, key);
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
        eprintln!("NEW_DOC_MS samples={samples:?} p50={p50:.3} p95={p95:.3}");
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
                vec!["bounded-raster-doc".to_string(), "multi-doc".to_string()]
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
            assert!(app.move_document_tab("multi-doc", "bounded-raster-doc", cx));
            assert_eq!(
                app.open_tab_ids_for_test(),
                vec!["multi-doc".to_string(), "bounded-raster-doc".to_string()]
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
            content.read(cx).state_for_test().read(cx).open_select == Some("resize-unit"),
            "the unit list expands"
        );
        window.click("resize-unit-option-2", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .open_select
                .is_none(),
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
        // Points are the fifth entry of the unit dropdown's order.
        window.click("resize-unit-option-4", cx);
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
            "TAB_CLICK_MS samples={samples:?} p50={:.3} p95={:.3}",
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
