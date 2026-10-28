use super::*;
use gpui_kit::test::TestWindowExt as _;

fn open_second_document(app: &mut App, cx: &mut gpui_kit::Context<App>, directory: &TestDirectory) {
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

#[gpui_kit::test]
fn new_document_dialog_creates_a_dirty_tab_with_the_chosen_preset(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.active_dialog = Some(crate::state::DialogId::NewDocument);
            });
        });
        window.render_frame(cx);
        window.click("new-doc-preset-1", cx);
        window.render_frame(cx);
        window.click("new-doc-create", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.active_dialog, None, "Create closes the dialog");
        assert_eq!(state.tabs.len(), 2, "the new document opens in a tab");
        assert!(state.tabs[1].dirty, "a created document starts dirty");
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((1920, 1080))
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
fn autosave_writes_dirty_sessions_and_reports_status(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let cache = directory.path().join("autosave-cache");
    std::fs::create_dir(&cache).expect("autosave cache directory");
    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            app.mark_active_document_dirty_for_test()
                .expect("the session is dirty");
            app.autosave_now(&cache, cx);
            assert_eq!(
                app.state_for_test().read(cx).autosave_status,
                crate::state::AutosaveStatus::Saved
            );
        });
    })
    .expect("test window remains open");

    let saved = walk_files(&cache);
    assert!(
        saved
            .iter()
            .any(|path| path.extension().and_then(|ext| ext.to_str()) == Some("ptz")),
        "autosave wrote a .ptz: {saved:?}"
    );
}

fn walk_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk_files(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[gpui_kit::test]
fn startup_recovery_restores_autosaves_as_unsaved_tabs(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let cache = directory.path().join("recovery-cache");
    let autosave_dir = cache.join("autosave");
    std::fs::create_dir_all(&autosave_dir).expect("autosave directory");
    std::fs::write(
        autosave_dir.join("multi-doc.ptz"),
        super::commands::multi_layer_archive_with(false),
    )
    .expect("recovery archive is written");
    std::fs::write(
        autosave_dir.join("manifest.json"),
        br#"{"multi-doc":"Multi layer|1700000000000"}"#,
    )
    .expect("recovery manifest is written");

    let file_io = Arc::new(
        LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("file boundary opens"),
    );
    let (window, content) = mount_app(cx, None, Some(file_io));
    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            app.prepare_recovery_from(&cache, cx);
            {
                let state = app.state_for_test().read(cx);
                assert_eq!(state.active_dialog, Some(crate::state::DialogId::Recovery));
                assert_eq!(state.recovery_entries.len(), 1);
                assert_eq!(state.recovery_entries[0].name, "Multi layer");
            }

            app.restore_autosaves_from(&cache, cx);
            assert_eq!(app.open_tab_ids_for_test(), vec!["multi-doc"]);
            assert!(app
                .active_document_id_for_test()
                .as_deref()
                .is_some_and(|id| id == "multi-doc"));
            assert_eq!(app.state_for_test().read(cx).active_dialog, None);
            assert_eq!(app.active_core_layer_count_for_test(), Some(2));
        });
    })
    .expect("test window remains open");
    assert!(
        !autosave_dir.join("multi-doc.ptz").exists(),
        "restoring removes the recovery archive"
    );
    let manifest = std::fs::read_to_string(autosave_dir.join("manifest.json"))
        .expect("the cleared manifest exists");
    assert_eq!(
        manifest.trim(),
        "{}",
        "the manifest lists no recoverable docs"
    );
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
        window.click("export-format-jpg", cx);
        window.render_frame(cx);
        window.click("export-quality-dec", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.export_format, crate::state::ExportFormat::Jpeg);
            assert_eq!(state.export_quality, 85);
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
        window.click("print-paper-1", cx);
        window.render_frame(cx);
        window.click("print-orientation", cx);
        window.render_frame(cx);
        window.click("print-copies-inc", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.print_paper, 1);
            assert!(state.print_landscape);
            assert_eq!(state.print_copies, 2);
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
            app.stage_dropped_paths(vec![dropped.clone()]);
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
