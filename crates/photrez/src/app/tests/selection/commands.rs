use super::*;
use crate::app::selection::DeleteSelectionAction;

/// A live transform owns the layer until pointer-up. Delete must not clear
/// pixels underneath it.
#[gpui_kit::test]
fn delete_selection_is_blocked_during_a_transform_drag(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let before = pixels(&content, cx).2;
        content.update(cx, |app, cx| {
            app.set_core_selection_for_test(
                SelectionState {
                    x: 0.0,
                    y: 0.0,
                    width: 2.0,
                    height: 1.0,
                    angle: 0.0,
                    shape: Some(SelectionShape::Rect),
                    inverted: None,
                },
                cx,
            )
            .expect("core accepts the selection");
            assert!(
                app.begin_transform_drag((1.0, 0.5), 1.0, cx),
                "the drag starts on the fixture layer"
            );
        });
        content.update(cx, |app, cx| {
            app.delete_selection_action(&DeleteSelectionAction, cx);
        });
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "Delete during a transform drag must not clear pixels"
        );
        assert!(
            content.read(cx).transform_drag_for_test().is_some(),
            "the transform drag is still live"
        );
    })
    .expect("test window remains open");
}

/// Cut is one history entry: undo restores the pixels and redo clears them
/// again.
#[gpui_kit::test]
fn cut_selection_undo_and_redo_round_trip(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let before = pixels(&content, cx).2;
        content.update(cx, |app, cx| {
            app.set_core_selection_for_test(
                SelectionState {
                    x: 0.0,
                    y: 0.0,
                    width: 2.0,
                    height: 1.0,
                    angle: 0.0,
                    shape: Some(SelectionShape::Rect),
                    inverted: None,
                },
                cx,
            )
            .expect("core accepts the selection");
            app.cut_selection(cx).expect("cut clears the selection");
        });
        let cleared = pixels(&content, cx).2;
        assert_eq!(cleared, vec![0u8; before.len()]);

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "undo restores the cut pixels"
        );

        content.update(cx, |app, cx| {
            app.step_history(false, cx).expect("redo succeeds");
        });
        assert_eq!(
            pixels(&content, cx).2,
            cleared,
            "redo clears the pixels again"
        );
    })
    .expect("test window remains open");
}

/// Paste is one history entry: undo removes the pasted layer and redo puts it
/// back.
#[gpui_kit::test]
fn paste_selection_undo_and_redo_round_trip(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let base_layers = content
            .read(cx)
            .active_core_layer_count_for_test()
            .expect("fixture layers");
        content.update(cx, |app, cx| {
            app.set_core_selection_for_test(
                SelectionState {
                    x: 0.0,
                    y: 0.0,
                    width: 2.0,
                    height: 1.0,
                    angle: 0.0,
                    shape: Some(SelectionShape::Rect),
                    inverted: None,
                },
                cx,
            )
            .expect("core accepts the selection");
            app.copy_selection(cx).expect("copy fills the clipboard");
            app.paste_selection(cx).expect("paste adds a layer");
        });
        assert_eq!(
            content
                .read(cx)
                .active_core_layer_count_for_test()
                .expect("layer count"),
            base_layers + 1
        );

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            content
                .read(cx)
                .active_core_layer_count_for_test()
                .expect("layer count"),
            base_layers,
            "undo removes the pasted layer"
        );

        content.update(cx, |app, cx| {
            app.step_history(false, cx).expect("redo succeeds");
        });
        assert_eq!(
            content
                .read(cx)
                .active_core_layer_count_for_test()
                .expect("layer count"),
            base_layers + 1,
            "redo restores the pasted layer"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn a_rotated_selection_copies_and_deletes_its_bounding_box(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (_, _, before) = pixels(&content, cx);
        content.update(cx, |app, cx| {
            app.set_core_selection_for_test(
                SelectionState {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                    angle: 45.0,
                    shape: Some(SelectionShape::Rect),
                    inverted: None,
                },
                cx,
            )
            .expect("core accepts the rotated selection");
        });
        content.update(cx, |app, cx| {
            app.copy_selection(cx)
                .expect("a rotated selection copies its bounding box");
        });
        assert_eq!(
            content.read(cx).clipboard_for_test(),
            Some((2, 1, before.clone())),
            "the 45-degree box spans both fixture pixels"
        );

        content.update(cx, |app, cx| {
            app.delete_selection(cx)
                .expect("a rotated selection clears its bounding box");
        });
        let (_, _, deleted) = pixels(&content, cx);
        assert_eq!(deleted, vec![0u8; before.len()]);
        let (labels, _) = history_rows(&content, cx);
        assert_eq!(labels.last().map(String::as_str), Some("Delete Selection"));

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "undo restores the pixels the rotated box cleared"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn copy_without_a_selection_reports_an_error_and_keeps_status_clean_after_success(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let error = app
                .copy_selection(cx)
                .expect_err("copy without a selection is refused");
            assert!(error.contains("no active selection"), "{error}");
        });
        assert!(content.read(cx).clipboard_for_test().is_none());

        tab_to_canvas(window, cx);
        window.press("ctrl-a", cx);
        window.render_frame(cx);
        // Invert select-all so the selection covers no pixel; copying must
        // report empty content instead of an empty clipboard.
        window.press("ctrl-shift-i", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let error = app
                .copy_selection(cx)
                .expect_err("an empty masked selection is refused");
            assert!(error.contains("no pixels"), "{error}");
            app.set_status_error(None, cx);
        });
        assert_eq!(status_error(&content, cx), None);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn right_click_context_menu_runs_edit_commands(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let (bounds, placement) = page(window, (w, h));
        let at = window_point(bounds, placement, (w, h), 0.5, 0.5);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Right,
                position: at,
                ..Default::default()
            }),
            cx,
        );
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .context_menu
                .is_some(),
            "right-click opens the context menu"
        );
        window.click("ctx-select-all", cx);
        window.render_frame(cx);
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("Select All from the context menu works");
        assert_eq!((selection.width, selection.height), (2.0, 1.0));
        assert_eq!(
            content.read(cx).state_for_test().read(cx).context_menu,
            None,
            "the command closes the context menu"
        );
    })
    .expect("test window remains open");
}
