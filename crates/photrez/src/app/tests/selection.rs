use super::input::{history_cursor, page, pixels, set_tool, tab_to_canvas, window_point};
use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};
use photrez_core::ptz::content::{SelectionShape, SelectionState};

fn dispatch(window: &mut gpui_kit::Window, event: PlatformInput, cx: &mut gpui_kit::App) {
    let _ = window.dispatch_event(event, cx);
    window.render_frame(cx);
}

fn set_selection(
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) {
    content.update(cx, |app, cx| {
        app.set_core_selection_for_test(
            SelectionState {
                x,
                y,
                width,
                height,
                angle: 0.0,
                shape: Some(SelectionShape::Rect),
                inverted: None,
            },
            cx,
        )
        .expect("core accepts the selection");
    });
}

fn history_rows(content: &Entity<App>, cx: &mut gpui_kit::App) -> (Vec<String>, Option<usize>) {
    let state = content.read(cx).state_for_test().read(cx);
    (
        state.history.iter().map(|row| row.label.clone()).collect(),
        state.history.iter().position(|row| row.current),
    )
}

fn status_error(content: &Entity<App>, cx: &mut gpui_kit::App) -> Option<String> {
    content
        .read(cx)
        .state_for_test()
        .read(cx)
        .status_error
        .clone()
}

#[gpui_kit::test]
fn selection_drag_previews_then_commits_to_core(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.1, 0.1);
        let moved = window_point(bounds, placement, doc, 0.9, 0.9);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        let preview = content
            .read(cx)
            .selection_drag_for_test(cx)
            .expect("the drag publishes a marquee preview");
        assert!(preview[2] > 0. && preview[3] > 0.);
        assert!(
            content.read(cx).active_core_selection_for_test().is_none(),
            "the core selection commits on pointer-up"
        );

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        assert!(
            content.read(cx).selection_drag_for_test(cx).is_none(),
            "the preview clears when the selection commits"
        );
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("pointer-up commits the core selection");
        assert!(selection.width >= 1.0 && selection.height >= 1.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shift_constrains_and_alt_centres_a_pixel_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.1, 0.1);
        let end = window_point(bounds, placement, doc, 0.5, 0.9);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers: gpui_kit::Modifiers {
                    shift: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                ..Default::default()
            }),
            cx,
        );
        let square = content
            .read(cx)
            .selection_drag_for_test(cx)
            .expect("preview exists");
        assert!(
            (square[2] - square[3]).abs() < 1e-3,
            "Shift must produce a square: {square:?}"
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            }),
            cx,
        );

        let centre = window_point(bounds, placement, doc, 0.5, 0.5);
        let edge = window_point(bounds, placement, doc, 1.5, 0.9);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: centre,
                modifiers: gpui_kit::Modifiers {
                    alt: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: edge,
                ..Default::default()
            }),
            cx,
        );
        let centred = content
            .read(cx)
            .selection_drag_for_test(cx)
            .expect("preview exists");
        assert!(
            centred[0] < 0.5 && centred[1] < 0.5,
            "Alt anchors both axes at the centre: {centred:?}"
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: edge,
                ..Default::default()
            }),
            cx,
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn selection_shortcuts_toggle_core_selection_without_history(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let cursor_before = history_cursor(&content, cx);

        window.press("ctrl-a", cx);
        window.render_frame(cx);
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("Ctrl+A selects the whole canvas");
        assert_eq!(
            (
                selection.x,
                selection.y,
                selection.width,
                selection.height,
                selection.inverted
            ),
            (0.0, 0.0, 2.0, 1.0, None)
        );

        window.press("ctrl-shift-i", cx);
        window.render_frame(cx);
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("the selection survives the invert");
        assert_eq!(selection.inverted, Some(true));

        window.press("ctrl-d", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).active_core_selection_for_test().is_none(),
            "Ctrl+D clears the selection"
        );

        // Inverting with nothing selected falls back to select-all, matching
        // the core command's host contract.
        window.press("ctrl-shift-i", cx);
        window.render_frame(cx);
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("invert without a selection selects all");
        assert_eq!((selection.width, selection.height), (2.0, 1.0));
        assert_eq!(selection.inverted, None);

        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "selection changes are not history entries"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn copy_and_paste_round_trips_selected_pixels(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (_, _, before) = pixels(&content, cx);
        set_selection(&content, cx, 0.0, 0.0, 1.0, 1.0);
        let cursor_before = history_cursor(&content, cx);
        let layers_before = content.read(cx).active_core_layer_count_for_test().unwrap();

        content.update(cx, |app, cx| {
            app.copy_selection(cx).expect("copy succeeds");
        });
        assert_eq!(
            content.read(cx).clipboard_for_test(),
            Some((1, 1, before[0..4].to_vec()))
        );
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "copy must not change the layer"
        );
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "copy is not a history entry"
        );

        content.update(cx, |app, cx| {
            app.paste_selection(cx).expect("paste succeeds");
        });
        let layers_after = content.read(cx).active_core_layer_count_for_test().unwrap();
        assert_eq!(layers_after, layers_before + 1);
        let ids = content.read(cx).active_core_layer_ids_for_test();
        let pasted_id = ids
            .iter()
            .find(|id| id.starts_with("native-layer-"))
            .expect("the pasted layer has a generated id")
            .clone();
        let (width, height, pasted) = content
            .read(cx)
            .core_layer_pixels_for_test(&pasted_id)
            .expect("the pasted layer owns pixels");
        assert_eq!((width, height), (1, 1));
        assert_eq!(pasted, before[0..4].to_vec());
        assert_eq!(history_cursor(&content, cx), cursor_before + 1);
        let (labels, current) = history_rows(&content, cx);
        assert_eq!(labels.last().map(String::as_str), Some("Paste"));
        assert_eq!(current, Some(labels.len() - 1));

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test().unwrap(),
            layers_before,
            "undo removes the pasted layer"
        );
        content.update(cx, |app, cx| {
            app.step_history(false, cx).expect("redo succeeds");
        });
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test().unwrap(),
            layers_before + 1,
            "redo restores the pasted layer"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn cut_clears_mask_and_undo_restores_pixels(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (_, _, before) = pixels(&content, cx);
        set_selection(&content, cx, 0.0, 0.0, 1.0, 1.0);
        let cursor_before = history_cursor(&content, cx);

        content.update(cx, |app, cx| {
            app.cut_selection(cx).expect("cut succeeds");
        });
        let (_, _, cut) = pixels(&content, cx);
        assert_eq!(&cut[0..4], &[0, 0, 0, 0], "the selected pixel is cleared");
        assert_eq!(
            &cut[4..8],
            &before[4..8],
            "pixels outside the selection are untouched"
        );
        assert_eq!(
            content.read(cx).clipboard_for_test(),
            Some((1, 1, before[0..4].to_vec()))
        );
        assert_eq!(history_cursor(&content, cx), cursor_before + 1);
        let (labels, _) = history_rows(&content, cx);
        assert_eq!(labels.last().map(String::as_str), Some("Cut"));

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "undo restores the cut pixels exactly"
        );
        content.update(cx, |app, cx| {
            app.step_history(false, cx).expect("redo succeeds");
        });
        assert_eq!(
            pixels(&content, cx).2,
            cut,
            "redo re-applies the cut exactly"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn delete_requires_a_selection_and_clears_only_masked_pixels(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (_, _, before) = pixels(&content, cx);
        let cursor_before = history_cursor(&content, cx);

        content.update(cx, |app, cx| {
            let error = app
                .delete_selection(cx)
                .expect_err("delete without a selection is refused");
            assert!(error.contains("no active selection"), "{error}");
        });
        assert_eq!(pixels(&content, cx).2, before);
        assert_eq!(history_cursor(&content, cx), cursor_before);

        set_selection(&content, cx, 0.0, 0.0, 1.0, 1.0);
        content.update(cx, |app, cx| {
            app.delete_selection(cx).expect("delete succeeds");
        });
        let (_, _, deleted) = pixels(&content, cx);
        assert_eq!(&deleted[0..4], &[0, 0, 0, 0]);
        assert_eq!(&deleted[4..8], &before[4..8]);
        assert_eq!(
            content.read(cx).clipboard_for_test(),
            None,
            "delete must not fill the clipboard"
        );
        let (labels, _) = history_rows(&content, cx);
        assert_eq!(labels.last().map(String::as_str), Some("Delete Selection"));
        assert_eq!(history_cursor(&content, cx), cursor_before + 1);
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
