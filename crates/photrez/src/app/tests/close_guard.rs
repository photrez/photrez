// The dirty-close guard: `ctrl-w` on an unsaved document prompts, and each answer has its own
// contract. Cancel leaves the document open and dirty, discard closes without writing, a clean
// document closes at once, Save writes and then closes, a failed write keeps it dirty, and an
// imported document is routed through Save As first.
use super::*;

/// Cancel on the dirty-close prompt leaves the document open and dirty.
#[gpui_kit::test]
fn ctrl_w_cancel_keeps_the_document_dirty(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{px, Point};

    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            app.mark_active_document_dirty_for_test()
                .expect("active session accepts an edit");
        });
        window.press("ctrl-w", cx);
        window.render_frame(cx);
        assert!(window.find("dialog-dirty-close").visible());

        window.click_at(
            "dialog-overlay",
            Point {
                x: px(10.),
                y: px(700.),
            },
            cx,
        );
        window.render_frame(cx);
        assert!(
            window.find("dialog-dirty-close").visible(),
            "the dirty-close decision must survive an outside click"
        );

        window.click("dirty-close-cancel", cx);
        window.render_frame(cx);
        assert!(window.try_find("dialog-dirty-close").is_none());
        content.update(cx, |app, _| {
            assert_eq!(
                app.active_document_id_for_test().as_deref(),
                Some("parity-doc")
            );
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((1, 0, true))
            );
        });
    })
    .expect("test window remains open");
    assert_eq!(
        std::fs::read(&source).expect("source remains"),
        BOUNDED_RASTER,
        "cancel must not write the source"
    );
}

/// Discard closes the document without writing its source file.
#[gpui_kit::test]
fn ctrl_w_discard_closes_without_writing(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    let before = std::fs::read(&source).expect("source reads");
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            app.mark_active_document_dirty_for_test()
                .expect("active session accepts an edit");
        });
        window.press("ctrl-w", cx);
        window.render_frame(cx);
        window.click("dirty-close-discard", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert!(app.active_document_id_for_test().is_none());
            assert!(!app.state_for_test().read(cx).has_document);
            assert_eq!(app.active_document_save_state_for_test(), None);
        });
    })
    .expect("test window remains open");
    assert_eq!(
        std::fs::read(&source).expect("source remains"),
        before,
        "discard must not write the source"
    );
}

/// A clean document closes without mounting the prompt.
#[gpui_kit::test]
fn ctrl_w_clean_document_closes_immediately(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.press("ctrl-w", cx);
        window.render_frame(cx);
        assert!(window.try_find("dialog-dirty-close").is_none());
        content.update(cx, |app, cx| {
            assert!(app.active_document_id_for_test().is_none());
            assert!(!app.state_for_test().read(cx).has_document);
        });
    })
    .expect("test window remains open");
}

/// Save on the prompt writes the source and then closes the document.
#[gpui_kit::test]
fn ctrl_w_save_writes_then_closes(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            app.mark_active_document_dirty_for_test()
                .expect("active session accepts an edit");
        });
        std::fs::remove_file(&source).expect("source is removed before save");
        window.press("ctrl-w", cx);
        window.render_frame(cx);
        window.click("dirty-close-save", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert!(app.active_document_id_for_test().is_none());
            assert!(!app.state_for_test().read(cx).has_document);
        });
    })
    .expect("test window remains open");
    assert!(source.is_file(), "save must recreate the source");
    let file = std::fs::File::open(&source).expect("saved archive opens");
    zip::ZipArchive::new(file).expect("saved archive is complete");
}

/// A failed save keeps the document open and dirty.
#[gpui_kit::test]
fn ctrl_w_save_failure_keeps_dirty(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            app.mark_active_document_dirty_for_test()
                .expect("active session accepts an edit");
        });
        std::fs::remove_file(&source).expect("source is removed before save");
        std::fs::create_dir(&source).expect("a directory occupies the save target");
        window.press("ctrl-w", cx);
        window.render_frame(cx);
        window.click("dirty-close-save", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert_eq!(
                app.active_document_id_for_test().as_deref(),
                Some("parity-doc"),
                "a failed save must keep the document open"
            );
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((1, 0, true)),
                "a failed save must keep the document dirty"
            );
            let message = app
                .state_for_test()
                .read(cx)
                .status_error
                .as_deref()
                .expect("the save error is shown")
                .to_owned();
            assert!(message.contains("Save failed (E_VALIDATION)"), "{message}");
        });
        std::fs::remove_dir(&source).expect("remove the obstructing directory");
    })
    .expect("test window remains open");
}

/// Save on the prompt for an imported document asks for a project path and
/// closes after that save succeeds.
#[gpui_kit::test]
fn ctrl_w_save_on_imported_document_prompts_save_as_then_closes(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let image_path = directory.path().join("photo.png");
    let png =
        photrez_core::pixel::export::encode_image(2, 1, &[1, 2, 3, 255, 4, 5, 6, 255], "png", 100)
            .expect("test image encodes");
    std::fs::write(&image_path, png).expect("test image is written");
    let file_io = Arc::new(
        LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("local file boundary opens"),
    );
    let (window, content) = mount_app(cx, None, Some(file_io));

    cx.update_window(window.into(), |_, window, cx| {
        window.press("ctrl-o", cx);
    })
    .expect("test window remains open");
    cx.simulate_path_prompt_response(|_| Some(vec![image_path.clone()]));
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            assert!(app
                .active_document_id_for_test()
                .is_some_and(|id| id.starts_with("import-")));
            app.mark_active_document_dirty_for_test()
                .expect("active session accepts an edit");
        });
        window.press("ctrl-w", cx);
        window.render_frame(cx);
        window.click("dirty-close-save", cx);
    })
    .expect("test window remains open");
    assert!(
        cx.did_prompt_for_new_path(),
        "an imported document must ask for a project path before closing"
    );

    let saved_path = directory.path().join("saved.ptz");
    cx.simulate_new_path_selection({
        let saved_path = saved_path.clone();
        move |_| Some(saved_path)
    });
    cx.run_until_parked();

    cx.update_window(window.into(), |_, _, cx| {
        content.update(cx, |app, cx| {
            assert!(app.active_document_id_for_test().is_none());
            assert!(!app.state_for_test().read(cx).has_document);
        });
    })
    .expect("test window remains open");
    assert!(saved_path.is_file(), "Save As must write the project");
}
