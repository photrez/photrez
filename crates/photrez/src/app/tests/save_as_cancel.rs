use super::*;

/// Cancelling Save As must leave the dirty document open and untouched.
#[gpui_kit::test]
fn ctrl_shift_s_cancel_preserves_the_dirty_document(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            assert_eq!(
                app.mark_active_document_dirty_for_test()
                    .expect("active session accepts an edit"),
                1
            );
        });
        window.press("ctrl-shift-s", cx);
    })
    .expect("test window remains open");
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();

    assert!(source.is_file(), "the source archive must survive a cancel");
    cx.update_window(window.into(), |_, _, cx| {
        content.update(cx, |app, cx| {
            assert_eq!(
                app.active_document_id_for_test().as_deref(),
                Some("bounded-raster-doc"),
                "cancelling Save As must not close the document"
            );
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((1, 0, true)),
                "cancelling Save As must leave the document dirty"
            );
            assert!(
                app.state_for_test().read(cx).status_error.is_none(),
                "a cancelled dialog is not an error"
            );
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn dirty_tab_save_as_stays_bound_to_the_tab_that_requested_close(
    cx: &mut gpui_kit::TestAppContext,
) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let first_path = directory.path().join("first.png");
    let second_path = directory.path().join("second.png");
    let first_png =
        photrez_core::pixel::export::encode_image(2, 1, &[1, 2, 3, 255, 4, 5, 6, 255], "png", 100)
            .expect("first test image encodes");
    let second_png = photrez_core::pixel::export::encode_image(
        2,
        1,
        &[7, 8, 9, 255, 10, 11, 12, 255],
        "png",
        100,
    )
    .expect("second test image encodes");
    std::fs::write(&first_path, first_png).expect("first image is written");
    std::fs::write(&second_path, second_png).expect("second image is written");
    let file_io = Arc::new(
        LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("local file boundary opens"),
    );
    let (window, content) = mount_app(cx, None, Some(file_io));

    let (first_doc_id, second_doc_id) = content.update(cx, |app, cx| {
        let file_io = app.file_io.clone().expect("file boundary is installed");
        file_io
            .approve_path(&first_path)
            .expect("first image path is approved");
        file_io
            .approve_path(&second_path)
            .expect("second image path is approved");
        app.open_path_file(&first_path, cx)
            .expect("first document opens");
        let first_doc_id = app
            .active_document_id_for_test()
            .expect("first document becomes active");
        app.open_path_file(&second_path, cx)
            .expect("second document opens");
        let second_doc_id = app
            .active_document_id_for_test()
            .expect("second document becomes active");
        (first_doc_id, second_doc_id)
    });
    assert_ne!(first_doc_id, second_doc_id);

    content.update(cx, |app, cx| {
        app.activate_document(&first_doc_id, cx)
            .expect("first tab activates");
        app.mark_active_document_dirty_for_test()
            .expect("first session accepts an edit");
        assert!(!app.close_document_tab(&first_doc_id, cx));
    });
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("dirty-close-save", cx);
    })
    .expect("test window remains open");
    assert!(cx.did_prompt_for_new_path());

    content.update(cx, |app, cx| {
        app.activate_document(&second_doc_id, cx)
            .expect("second tab can activate while the path prompt is pending");
    });

    let saved_path = directory.path().join("first-document.ptz");
    cx.simulate_new_path_selection({
        let saved_path = saved_path.clone();
        move |_| Some(saved_path)
    });
    cx.run_until_parked();

    let file_io = content.update(cx, |app, _| {
        app.file_io
            .clone()
            .expect("file boundary remains installed")
    });
    let saved = services::document::load_document_candidate(file_io.as_ref(), &saved_path)
        .expect("Save As writes a readable project");
    assert_eq!(saved.document_id(), first_doc_id);
    content.update(cx, |app, cx| {
        assert_eq!(
            app.active_document_id_for_test().as_deref(),
            Some(second_doc_id.as_str()),
            "completing Save As must preserve the unrelated active tab"
        );
        assert!(app
            .state_for_test()
            .read(cx)
            .tabs
            .iter()
            .all(|tab| { tab.doc_id != first_doc_id }));
    });
}
