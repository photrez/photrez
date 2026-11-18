//! Startup open: the active bundle stays until the candidate is presentable.

use super::*;

#[gpui_kit::test]
fn startup_open_keeps_active_bundle_until_candidate_is_presentable(cx: &mut TestAppContext) {
    let paths = StartupTestFiles::new();
    let missing = paths.0.join("missing.ptz");
    let malformed = paths.file("malformed.ptz");
    let unsupported = paths.file("unsupported.ptz");
    let oversized = paths.file("oversized.ptz");
    let zero = paths.file("zero.ptz");
    let a = paths.file("a.ptz");
    let c = paths.file("c.ptz");
    let io = Arc::new(StartupFileIo::default());
    io.insert(&malformed, b"not a zip archive".to_vec());
    io.insert(
        &unsupported,
        startup_archive("doc-unsupported", 1, 1, true, 0),
    );
    io.insert(
        &oversized,
        startup_archive("doc-oversized", 1, 1, false, 1024 * 1024 + 1),
    );
    io.insert(&zero, startup_archive("doc-zero", 0, 0, false, 0));
    io.insert(&a, startup_archive("doc-a", 1, 1, false, 0));
    io.insert(&c, startup_archive("doc-c", 1, 1, false, 0));
    let (handle, content) = open_app_with(cx, Some((a.clone(), io.clone())));

    cx.update_window(handle.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            assert_eq!(app.active_document_id_for_test().as_deref(), Some("doc-a"));
            assert_eq!(app.active_document_pixel_lengths_for_test(), Some((4, 0)));

            let approvals = io.approvals.load(Ordering::Relaxed);
            let reads = io.reads.load(Ordering::Relaxed);
            assert!(
                app.open_startup_document_for_test(&missing, io.as_ref(), cx)
                    .is_err()
            );
            assert_eq!(io.approvals.load(Ordering::Relaxed), approvals);
            assert_eq!(io.reads.load(Ordering::Relaxed), reads);
            assert_eq!(app.active_document_id_for_test().as_deref(), Some("doc-a"));

            for path in [&malformed, &unsupported, &oversized] {
                let result = app.open_startup_document_for_test(path, io.as_ref(), cx);
                assert!(result.is_err(), "accepted {}", path.display());
                assert_eq!(app.active_document_id_for_test().as_deref(), Some("doc-a"));
            }

            assert_eq!(
                app.stage_startup_document_for_test(&c, io.as_ref())
                    .expect("C stages"),
                ("doc-c".to_owned(), 1, 1)
            );
            assert_eq!(app.active_document_id_for_test().as_deref(), Some("doc-a"));

            let approvals = io.approvals.load(Ordering::Relaxed);
            let reads = io.reads.load(Ordering::Relaxed);
            let error = app
                .open_startup_document_for_test(&zero, io.as_ref(), cx)
                .expect_err("0x0 loads but cannot be presented");
            assert!(error.starts_with("document presentation failed:"));
            assert_eq!(io.approvals.load(Ordering::Relaxed), approvals + 1);
            assert_eq!(io.reads.load(Ordering::Relaxed), reads + 1);
            assert_eq!(app.active_document_id_for_test().as_deref(), Some("doc-a"));

            app.open_startup_document_for_test(&c, io.as_ref(), cx)
                .expect("C replaces A");
            assert_eq!(app.active_document_id_for_test().as_deref(), Some("doc-c"));
            assert_eq!(app.active_document_pixel_lengths_for_test(), Some((4, 0)));
        });
    })
    .unwrap();
}
