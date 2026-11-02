//! Autosave and startup recovery: the periodic sweep writes every dirty session,
//! and the prompt at the next start can restore, keep or delete the archives.

use super::*;
use gpui_kit::test::TestWindowExt as _;

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

/// One document whose snapshot fails must not freeze the rest of the sweep.
#[gpui_kit::test]
fn autosave_writes_the_other_dirty_sessions_when_one_snapshot_fails(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let cache_root = directory.path().join("cache");
    let autosave_dir = cache_root.join("autosave");
    std::fs::create_dir_all(&autosave_dir).expect("autosave directory");
    // An existing directory sits where "multi-doc"'s archive must be written,
    // so that session's snapshot fails. Its ID sorts first, so it is the
    // session the sweep visits before the mounted fixture document.
    std::fs::create_dir_all(autosave_dir.join("multi-doc.ptz"))
        .expect("the poisoned archive target exists");

    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            app.mark_active_document_dirty_for_test()
                .expect("the first session accepts an edit");
            documents::open_second_document(app, cx, &directory);
            app.mark_active_document_dirty_for_test()
                .expect("the second session accepts an edit");
            assert!(
                app.autosave_dirty_sessions(&cache_root).is_err(),
                "the failed snapshot is reported"
            );
        });
    })
    .expect("test window remains open");

    assert!(
        autosave_dir.join("parity-doc.ptz").is_file(),
        "a sibling document still gets its snapshot"
    );
    let entries = crate::services::autosave::AutosaveStore::new(&cache_root)
        .list()
        .expect("the manifest reads");
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.doc_id())
            .collect::<Vec<_>>(),
        vec!["parity-doc"],
        "only the session that wrote an archive is listed"
    );
}

#[gpui_kit::test]
fn recovery_of_one_broken_archive_still_restores_the_others(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let cache = directory.path().join("recovery-partial");
    let autosave_dir = cache.join("autosave");
    std::fs::create_dir_all(&autosave_dir).expect("autosave directory");
    std::fs::write(
        autosave_dir.join("broken-doc.ptz"),
        b"not an archive at all",
    )
    .expect("broken recovery archive is written");
    std::fs::write(
        autosave_dir.join("multi-doc.ptz"),
        super::commands::multi_layer_archive_with(false),
    )
    .expect("good recovery archive is written");
    std::fs::write(
        autosave_dir.join("manifest.json"),
        br#"{"broken-doc":"Broken archive|1700000000000","multi-doc":"Multi layer|1700000000000"}"#,
    )
    .expect("recovery manifest is written");

    let file_io = Arc::new(
        LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("file boundary opens"),
    );
    let (window, content) = mount_app(cx, None, Some(file_io));
    let broken_path = autosave_dir.join("broken-doc.ptz");
    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            app.prepare_recovery_from(&cache, cx);
            assert_eq!(
                app.state_for_test().read(cx).recovery_entries.len(),
                2,
                "both archives are offered"
            );

            app.restore_autosaves_from(&cache, cx);
            assert_eq!(
                app.open_tab_ids_for_test(),
                vec!["multi-doc"],
                "a sibling archive must not block the readable one"
            );
            assert_eq!(
                app.state_for_test().read(cx).active_dialog,
                None,
                "the prompt closes once it is answered"
            );
            let status = app
                .state_for_test()
                .read(cx)
                .status_error
                .clone()
                .expect("the failed entry is reported");
            assert!(
                status.contains("1 of 2 restored") && status.contains("Broken archive"),
                "the status names what was restored and what failed: {status}"
            );
        });
    })
    .expect("test window remains open");

    assert!(
        !autosave_dir.join("multi-doc.ptz").exists(),
        "the restored archive is removed"
    );
    assert!(
        broken_path.exists(),
        "the archive that could not be opened is kept for the next start"
    );
    let remaining = crate::services::autosave::AutosaveStore::new(&cache)
        .list()
        .expect("the manifest reads");
    assert_eq!(
        remaining
            .iter()
            .map(|entry| entry.doc_id())
            .collect::<Vec<_>>(),
        vec!["broken-doc"],
        "only the failed entry stays in the manifest"
    );
}

/// Declining the offer answers the prompt without destroying any backup.
#[gpui_kit::test]
fn declining_recovery_keeps_every_autosave(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let cache = directory.path().join("recovery-decline");
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

    let (window, content) = mount_app(cx, None, None::<Arc<dyn FileIo>>);
    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            // The production decline reads the app's own cache root, so the
            // prompt is exercised with that root pointed at this test store.
            app.use_autosave_cache_root_for_test(cache.clone());
            app.prepare_recovery_from(&cache, cx);
            app.decline_autosaves(cx);
            let state = app.state_for_test().read(cx);
            assert_eq!(state.active_dialog, None, "the prompt closes");
            assert!(
                state.recovery_entries.is_empty(),
                "the answered offer is no longer listed"
            );
        });
    })
    .expect("test window remains open");

    assert!(
        autosave_dir.join("multi-doc.ptz").exists(),
        "declining keeps the backup for the next start"
    );
    let remaining = crate::services::autosave::AutosaveStore::new(&cache)
        .list()
        .expect("the manifest reads");
    assert_eq!(
        remaining.len(),
        1,
        "declining leaves the manifest untouched"
    );
}

fn seed_recovery_store(directory: &TestDirectory, name: &str) -> std::path::PathBuf {
    let cache = directory.path().join(name);
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
    cache
}

/// Each button in the recovery prompt must reach the action it names.
#[gpui_kit::test]
fn recovery_prompt_buttons_reach_their_actions(cx: &mut gpui_kit::TestAppContext) {
    struct Case {
        button: &'static str,
        expect_open: usize,
        expect_archive: bool,
    }
    let cases = [
        Case {
            button: "recovery-restore",
            expect_open: 1,
            expect_archive: false,
        },
        Case {
            button: "recovery-decline",
            expect_open: 0,
            expect_archive: true,
        },
        Case {
            button: "recovery-discard",
            expect_open: 0,
            expect_archive: false,
        },
    ];
    for case in cases {
        let directory = TestDirectory::new();
        let cache = seed_recovery_store(&directory, "recovery-click");
        let archive = cache.join("autosave").join("multi-doc.ptz");
        let file_io = Arc::new(
            LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
                .expect("file boundary opens"),
        );
        let (window, content) = mount_app(cx, None, Some(file_io));
        cx.update_window(window.into(), |_, window, cx| {
            content.update(cx, |app, cx| {
                app.use_autosave_cache_root_for_test(cache.clone());
                app.prepare_recovery_from(&cache, cx);
            });
            window.render_frame(cx);
            window.click(case.button, cx);
            window.render_frame(cx);
            content.update(cx, |app, cx| {
                let state = app.state_for_test().read(cx);
                assert_eq!(
                    state.active_dialog, None,
                    "{} answers the prompt",
                    case.button
                );
                assert_eq!(
                    app.open_tab_ids_for_test().len(),
                    case.expect_open,
                    "{} opens {} document(s)",
                    case.button,
                    case.expect_open
                );
                assert!(
                    state.status_error.is_none(),
                    "{} reports no failure: {:?}",
                    case.button,
                    state.status_error
                );
            });
        })
        .expect("test window remains open");
        assert_eq!(
            archive.is_file(),
            case.expect_archive,
            "{} leaves the archive present: {}",
            case.button,
            case.expect_archive
        );
    }
}
