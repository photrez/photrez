use super::*;

/// Cancelling Save As must leave the dirty document open and untouched.
///
/// RED proof: route the cancelled dialog to `save_active_document_to` and the
/// target is written; route it to a close and the document disappears.
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
