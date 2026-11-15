//! The print path: the document reaches the backend, and a failed print reports a typed error.

use super::*;

/// Ctrl+P must hand the active document's validated pixels to the print
/// boundary with the default paper and no output file.
///
/// RED proof: remove the `ctrl-p` binding or the print handler and the
/// backend records no call.
#[gpui_kit::test]
fn ctrl_p_sends_the_active_document_to_the_print_backend(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let backend = Arc::new(RecordingPrintBackend::default());

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            app.set_print_backend_for_test(Arc::clone(&backend) as Arc<dyn PrintDispatch>);
        });
        window.press("ctrl-p", cx);
        window.render_frame(cx);
        window.click("print-confirm", cx);
        window.render_frame(cx);

        content.update(cx, |app, cx| {
            assert!(
                app.state_for_test().read(cx).status_error.is_none(),
                "a successful print must not leave a status error"
            );
        });
    })
    .expect("test window remains open");

    let calls = backend.calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "Ctrl+P must reach the print boundary once");
    let call = &calls[0];
    assert_eq!((call.width, call.height), (2, 1));
    assert_eq!(call.pixels, EXPECTED_CANONICAL_RGBA);
    assert_eq!(call.document_name, "Bounded raster document");
    assert_eq!(call.paper_mm, (210.0, 297.0));
    assert_eq!(call.output_path, None);
}

/// A two-layer 2x1 project: a blue 1x1 top raster over a red 2x1 bottom.
/// Saving or printing it must use the composited bytes, not one layer.
#[gpui_kit::test]
fn ctrl_p_prints_the_composited_multi_layer_document(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let source = directory.path().join("multi.ptz");
    std::fs::write(&source, multi_layer_archive()).expect("multi-layer fixture is written");
    let file_io = Arc::new(
        LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("local file boundary opens"),
    );
    file_io.approve_path(&source).expect("fixture is approved");
    let (window, content) = mount_app(cx, Some(source), Some(file_io));
    let backend = Arc::new(RecordingPrintBackend::default());

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            let active = app
                .active_document
                .as_ref()
                .expect("the multi-layer document is active");
            assert_eq!(active.doc_id, "multi-doc");
            assert_eq!(active.canvas.tile_count(), 1);
            app.set_print_backend_for_test(Arc::clone(&backend) as Arc<dyn PrintDispatch>);
        });
        window.press("ctrl-p", cx);
        window.render_frame(cx);
        window.click("print-confirm", cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");

    let calls = backend.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!((calls[0].width, calls[0].height), (2, 1));
    assert_eq!(
        calls[0].pixels,
        [0, 0, 255, 255, 255, 0, 0, 255],
        "print must receive the composited layer stack"
    );
}

/// Ctrl+S must preserve every layer of a multi-layer project, not just the
/// first raster.
/// A failed print must keep the document open, report the typed error, and
/// clear it on the next successful attempt.
#[gpui_kit::test]
fn ctrl_p_failure_reports_and_recovers(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let backend = Arc::new(RecordingPrintBackend::default());
    backend
        .fail
        .store(true, std::sync::atomic::Ordering::Relaxed);

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            app.set_print_backend_for_test(Arc::clone(&backend) as Arc<dyn PrintDispatch>);
        });
        window.press("ctrl-p", cx);
        window.render_frame(cx);
        window.click("print-confirm", cx);
        window.render_frame(cx);
        assert!(
            window.find("status-error").visible(),
            "a failed print must mount the status-bar error cell"
        );

        content.update(cx, |app, cx| {
            assert_eq!(
                app.active_document_id_for_test().as_deref(),
                Some("bounded-raster-doc"),
                "a failed print must keep the document open"
            );
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((0, 0, false)),
                "printing must not change the save state"
            );
            let message = app
                .state_for_test()
                .read(cx)
                .status_error
                .as_deref()
                .expect("the print failure is shown")
                .to_owned();
            assert!(message.contains("Print failed (E_PRINTER)"), "{message}");
        });

        backend
            .fail
            .store(false, std::sync::atomic::Ordering::Relaxed);
        window.press("ctrl-p", cx);
        window.render_frame(cx);
        window.click("print-confirm", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("status-error").is_none(),
            "a successful retry must unmount the error cell"
        );
        content.update(cx, |app, cx| {
            assert!(app.state_for_test().read(cx).status_error.is_none());
        });
    })
    .expect("test window remains open");

    let calls = backend.calls.lock().unwrap();
    assert_eq!(calls.len(), 2, "the retry must reach the boundary again");
}

/// Printing with no document must report a typed conflict instead of
/// silently doing nothing.
#[gpui_kit::test]
fn ctrl_p_without_a_document_reports_a_conflict(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let backend = Arc::new(RecordingPrintBackend::default());
    let (handle, content) = mount_app(cx, None, None);
    cx.update_window(handle.into(), |_, window, cx| {
        content.update(cx, |app, _| {
            app.set_print_backend_for_test(Arc::clone(&backend) as Arc<dyn PrintDispatch>);
        });
        window.press("ctrl-p", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let message = app
                .state_for_test()
                .read(cx)
                .status_error
                .as_deref()
                .expect("the conflict is shown")
                .to_owned();
            assert!(message.contains("Print failed (E_CONFLICT)"), "{message}");
        });
    })
    .expect("test window remains open");
    assert!(backend.calls.lock().unwrap().is_empty());
}
