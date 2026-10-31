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
fn cross_document_paste_adds_the_clipboard_to_the_active_document(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (_, _, source_pixels) = pixels(&content, cx);
        set_selection(&content, cx, 0.0, 0.0, 1.0, 1.0);
        content.update(cx, |app, cx| {
            app.copy_selection(cx).expect("copy in the source document");
        });
        assert_eq!(
            content.read(cx).clipboard_for_test(),
            Some((1, 1, source_pixels[0..4].to_vec()))
        );

        content.update(cx, |app, cx| {
            app.create_document(4, 4, cx)
                .expect("blank target document");
        });
        window.render_frame(cx);
        let target_layers = content.read(cx).active_core_layer_count_for_test().unwrap();
        content.update(cx, |app, cx| {
            app.paste_selection(cx)
                .expect("the clipboard pastes into the target document");
        });
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test().unwrap(),
            target_layers + 1
        );
        let pasted_id = content
            .read(cx)
            .active_core_layer_ids_for_test()
            .into_iter()
            .find(|id| id.starts_with("native-layer-"))
            .expect("the cross-document paste creates a layer");
        let (width, height, pasted) = content
            .read(cx)
            .core_layer_pixels_for_test(&pasted_id)
            .expect("the pasted layer owns pixels");
        assert_eq!((width, height), (1, 1));
        assert_eq!(pasted, source_pixels[0..4].to_vec());
        let (labels, _) = history_rows(&content, cx);
        assert_eq!(labels.last().map(String::as_str), Some("Paste"));

        content.update(cx, |app, cx| {
            app.activate_document("parity-doc", cx)
                .expect("the source document is still open");
        });
        window.render_frame(cx);
        assert_eq!(
            pixels(&content, cx).2,
            source_pixels,
            "pasting into another document leaves the source untouched"
        );
        let (source_labels, _) = history_rows(&content, cx);
        assert!(
            !source_labels.iter().any(|label| label == "Paste"),
            "the paste belongs to the target document's history"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn ctrl_t_toggles_the_selection_transform_chrome(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("ctrl-t", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).selection_edit_mode_for_test(),
            "Ctrl+T without a selection does nothing"
        );

        set_selection(&content, cx, 0.0, 0.0, 1.0, 1.0);
        window.render_frame(cx);
        window.press("ctrl-t", cx);
        window.render_frame(cx);
        assert!(content.read(cx).selection_edit_mode_for_test());
        window.press("ctrl-t", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).selection_edit_mode_for_test(),
            "a second Ctrl+T closes the transform chrome"
        );
        window.press("ctrl-t", cx);
        window.render_frame(cx);
        assert!(content.read(cx).selection_edit_mode_for_test());

        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).selection_edit_mode_for_test(),
            "Escape clears the transform chrome"
        );
        window.press("ctrl-t", cx);
        window.render_frame(cx);
        assert!(content.read(cx).selection_edit_mode_for_test());
        window.press("ctrl-d", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).selection_edit_mode_for_test(),
            "deselect clears the transform chrome"
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

#[gpui_kit::test]
fn marquee_overlay_paints_live_and_committed_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.2, 0.2);
        let moved = window_point(bounds, placement, doc, 0.8, 0.7);
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
            .expect("the drag publishes a preview");
        let overlay = content.update(cx, |app, cx| app.canvas_overlay(cx));
        let live = overlay
            .selection
            .expect("the live marquee reaches the overlay");
        let (x, y, width, height, ellipse) = (live.x, live.y, live.w, live.h, live.ellipse);
        assert_eq!(
            (x, y, width, height, ellipse),
            (preview[0], preview[1], preview[2], preview[3], false)
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
        let committed = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("pointer-up commits the core selection");
        let overlay = content.update(cx, |app, cx| app.canvas_overlay(cx));
        let painted = overlay
            .selection
            .expect("the committed selection keeps painting");
        let (x, y, width, height) = (painted.x, painted.y, painted.w, painted.h);
        assert_eq!(
            (x, y, width, height),
            (committed.x, committed.y, committed.width, committed.height)
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn canvas_click_selection_covers_the_clicked_point(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let point = window_point(bounds, placement, doc, 0.4, 0.6);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: point,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: point,
                ..Default::default()
            }),
            cx,
        );
        let overlay = content.update(cx, |app, cx| app.canvas_overlay(cx));
        match content.read(cx).active_core_selection_for_test() {
            Some(selection) => {
                let click_x = 0.4 * f64::from(doc.0);
                let click_y = 0.6 * f64::from(doc.1);
                assert!(
                    click_x >= selection.x
                        && click_x <= selection.x + selection.width
                        && click_y >= selection.y
                        && click_y <= selection.y + selection.height,
                    "a click-created selection must cover the clicked point: \
                     selection {selection:?} does not contain ({click_x}, {click_y})"
                );
            }
            None => assert!(
                overlay.selection.is_none(),
                "no committed selection must not paint a marquee"
            ),
        }
    })
    .expect("test window remains open");
}

/// S047 `selectionRotation.ts`: the Selection tool rotates the committed
/// marquee from its pin. The drag previews without touching core, and the
/// release commits one `SetSelection` carrying the angle.
#[gpui_kit::test]
fn dragging_the_selection_rotate_pin_commits_an_angle(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        set_selection(&content, cx, 100.0, 80.0, 200.0, 80.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("ctrl-t", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).selection_edit_mode_for_test(),
            "Ctrl+T arms the transform chrome before the pin drag"
        );

        let (bounds, page) = page(window, (400, 200));
        let scale = f64::from(page.2) / 400.0;
        let pin_doc = (200.0, 80.0 - 24.0 / scale);
        let centre = (200.0, 120.0);
        let radius = centre.1 - pin_doc.1;
        let from = window_point(bounds, page, (400, 200), pin_doc.0 as f32, pin_doc.1 as f32);
        let target = (-60.0_f64).to_radians();
        let to_doc = (
            centre.0 + radius * target.cos(),
            centre.1 + radius * target.sin(),
        );
        let to = window_point(bounds, page, (400, 200), to_doc.0 as f32, to_doc.1 as f32);

        // Press on the pin: the drag starts but core keeps the old angle.
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                ..Default::default()
            }),
            cx,
        );
        assert!(
            content
                .read(cx)
                .selection_rotation_preview_for_test()
                .is_some(),
            "the press on the pin starts a rotation drag"
        );
        assert_eq!(
            content
                .read(cx)
                .core_selection_for_test()
                .expect("selection")
                .angle,
            0.0,
            "a press alone does not rotate the committed selection"
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: to,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content
                .read(cx)
                .core_selection_for_test()
                .expect("selection")
                .angle,
            0.0,
            "the live rotation stays display-only until the release"
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                ..Default::default()
            }),
            cx,
        );
        let angle = content
            .read(cx)
            .core_selection_for_test()
            .expect("selection")
            .angle;
        assert!(
            (angle - 30.0).abs() < 1.0,
            "releasing after a 30 degree drag commits 30 degrees, got {angle}"
        );
    })
    .expect("test window remains open");
}

/// The rotation applies the mouse-up coordinate before the commit (the
/// transform drag's latest-sample rule): a press on the pin whose FIRST
/// sample is the release position must still commit the rotated angle.
#[gpui_kit::test]
fn selection_rotation_applies_the_mouse_up_coordinate(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        set_selection(&content, cx, 100.0, 80.0, 200.0, 80.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("ctrl-t", cx);
        window.render_frame(cx);

        let (bounds, page) = page(window, (400, 200));
        let scale = f64::from(page.2) / 400.0;
        let pin_doc = (200.0, 80.0 - 24.0 / scale);
        let centre = (200.0, 120.0);
        let radius = centre.1 - pin_doc.1;
        let from = window_point(bounds, page, (400, 200), pin_doc.0 as f32, pin_doc.1 as f32);
        let target = (-60.0_f64).to_radians();
        let to_doc = (
            centre.0 + radius * target.cos(),
            centre.1 + radius * target.sin(),
        );
        let to = window_point(bounds, page, (400, 200), to_doc.0 as f32, to_doc.1 as f32);

        // Press the pin and release at the rotated position with no move in
        // between: the release coordinate is the only rotation sample.
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                ..Default::default()
            }),
            cx,
        );
        let angle = content
            .read(cx)
            .core_selection_for_test()
            .expect("selection")
            .angle;
        assert!(
            (angle - 30.0).abs() < 1.0,
            "the mouse-up coordinate commits the rotated angle, got {angle}"
        );
    })
    .expect("test window remains open");
}

/// S013: the cross-document paste's event-to-frame latency.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn cross_document_paste_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        // Build the clipboard once: a 3x2 selection over the painted pixels.
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (bounds, page) = page(window, (400, 200));
        let at = |fx: f32, fy: f32| window_point(bounds, page, (400, 200), fx, fy);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at(60.0, 40.0),
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: at(120.0, 90.0),
                ..Default::default()
            }),
            cx,
        );
        window.press("ctrl-c", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.create_document(300, 300, cx).expect("target document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        tab_to_canvas(window, cx);
        window.render_frame(cx);

        // One untimed warm-up paste (the S050 measurement protocol).
        window.press("ctrl-v", cx);
        window.render_frame(cx);
        window.press("ctrl-z", cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for _sample in 0..9 {
            let start = std::time::Instant::now();
            window.press("ctrl-v", cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            window.press("ctrl-z", cx);
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S013_CROSS_DOC_PASTE_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "cross-document paste p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// `cursorRotate.ts`: the rotate sprite paints at the pointer on the pin
/// hover in edit mode and follows the pointer during a pin rotation; away
/// from the pin nothing paints.
#[gpui_kit::test]
fn rotate_cursor_sprite_follows_the_pointer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        set_selection(&content, cx, 100.0, 80.0, 200.0, 80.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("ctrl-t", cx);
        window.render_frame(cx);

        let (bounds, page) = page(window, (400, 200));
        let scale = f64::from(page.2) / 400.0;
        let pin_doc = (200.0, 80.0 - 24.0 / scale);
        let pin = window_point(bounds, page, (400, 200), pin_doc.0 as f32, pin_doc.1 as f32);
        let away = window_point(bounds, page, (400, 200), 40.0, 180.0);

        // Hovering the pin paints the sprite at the pointer.
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: pin,
                ..Default::default()
            }),
            cx,
        );
        let sprite = content
            .update(cx, |app, cx| app.rotate_cursor_for_test(cx))
            .expect("the pin hover paints the rotate sprite");
        assert_eq!(
            (sprite.position[0], sprite.position[1]),
            (f32::from(pin.x), f32::from(pin.y)),
            "the sprite anchors at the pointer"
        );
        // The pin sits straight above the selection centre: the pointer angle
        // is -90 degrees, plus the source's 45-degree base tilt.
        assert!(
            (f64::from(sprite.angle) + 45.0).abs() < 1.0,
            "the sprite angle follows the pointer about the centre, got {}",
            sprite.angle
        );

        // Away from the pin nothing paints.
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: away,
                ..Default::default()
            }),
            cx,
        );
        assert!(
            content
                .update(cx, |app, cx| app.rotate_cursor_for_test(cx))
                .is_none(),
            "a hover away from the pin paints no sprite"
        );

        // During the pin drag the sprite keeps following the pointer.
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: pin,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: pin,
                ..Default::default()
            }),
            cx,
        );
        let dragged = window_point(bounds, page, (400, 200), 120.0, 30.0);
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: dragged,
                ..Default::default()
            }),
            cx,
        );
        let sprite = content
            .update(cx, |app, cx| app.rotate_cursor_for_test(cx))
            .expect("the rotation drag keeps the sprite at the pointer");
        assert_eq!(
            (sprite.position[0], sprite.position[1]),
            (f32::from(dragged.x), f32::from(dragged.y)),
            "the drag sprite follows the pointer"
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: dragged,
                ..Default::default()
            }),
            cx,
        );
        assert!(
            content
                .update(cx, |app, cx| app.rotate_cursor_for_test(cx))
                .is_none(),
            "releasing away from the pin clears the sprite"
        );
    })
    .expect("test window remains open");
}

/// S046: the marquee release commit's event-to-frame latency. Each sample
/// dispatches down/move/up through the production canvas path and times one
/// render, then verifies the selection actually committed.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn selection_marquee_commit_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (bounds, page) = page(window, (400, 200));
        let start_at = window_point(bounds, page, (400, 200), 80.0, 40.0);
        let end_at = window_point(bounds, page, (400, 200), 320.0, 160.0);

        // One untimed warm-up commit so the first timed sample is not the
        // cold shader/allocator path (the S050 measurement protocol).
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start_at,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end_at,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            window.press("ctrl-d", cx);
            window.render_frame(cx);
            assert!(
                content.read(cx).committed_selection().is_none(),
                "sample {sample} starts deselected"
            );
            let start = std::time::Instant::now();
            dispatch(
                window,
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: start_at,
                    ..Default::default()
                }),
                cx,
            );
            dispatch(
                window,
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: end_at,
                    ..Default::default()
                }),
                cx,
            );
            dispatch(
                window,
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: end_at,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            let selection = content
                .read(cx)
                .committed_selection()
                .expect("the marquee commits each sample");
            assert!(
                selection.width > 100.0,
                "sample {sample} committed the drag rect"
            );
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S046_MARQUEE_COMMIT_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "selection marquee commit p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// S047: the rotate-pin release commit's event-to-frame latency.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn selection_rotation_commit_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        set_selection(&content, cx, 100.0, 80.0, 200.0, 80.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("ctrl-t", cx);
        window.render_frame(cx);

        let (bounds, page) = page(window, (400, 200));
        let scale = f64::from(page.2) / 400.0;
        let pin_doc = (200.0, 80.0 - 24.0 / scale);
        let centre = (200.0, 120.0);
        let radius = centre.1 - pin_doc.1;
        let from = window_point(bounds, page, (400, 200), pin_doc.0 as f32, pin_doc.1 as f32);
        let target = (-60.0_f64).to_radians();
        let to = window_point(
            bounds,
            page,
            (400, 200),
            (centre.0 + radius * target.cos()) as f32,
            (centre.1 + radius * target.sin()) as f32,
        );

        // One untimed warm-up rotation (the S050 measurement protocol).
        set_selection(&content, cx, 100.0, 80.0, 200.0, 80.0);
        window.render_frame(cx);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        let mut samples = Vec::new();
        for _sample in 0..9 {
            // The pin travels with the rotation; restore the unrotated
            // selection so every sample presses the same pin.
            set_selection(&content, cx, 100.0, 80.0, 200.0, 80.0);
            window.render_frame(cx);
            let start = std::time::Instant::now();
            dispatch(
                window,
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: from,
                    ..Default::default()
                }),
                cx,
            );
            dispatch(
                window,
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: to,
                    ..Default::default()
                }),
                cx,
            );
            dispatch(
                window,
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: to,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            let angle = content
                .read(cx)
                .core_selection_for_test()
                .expect("selection")
                .angle;
            assert!(
                (angle - 30.0).abs() < 1.0,
                "the sample rotates back to 30 degrees, got {angle}"
            );
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S047_ROTATE_COMMIT_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "selection rotation commit p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// `SelectionOptionBar.tsx`: the X/Y/W/H/R fields read the committed
/// selection, their commits rewrite one dimension through core history, and
/// without a selection the bar keeps the oracle's disabled state.
#[gpui_kit::test]
fn selection_bar_fields_read_and_rewrite_the_committed_selection(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    // The bar needs the full comparison width: at 1536 CSS px the 2xl dock
    // (598 px) leaves 886 px and clips the R field the oracle shows at 1920.
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1920.0);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);

        // Disabled state: the static field accepts no edit and nothing
        // starts a num-field session.
        window.click("sel-w", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).committed_selection().is_none(),
            "the disabled field must not create a selection"
        );
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .num_field
                .is_none(),
            "the disabled field must not start an edit session"
        );

        set_selection(&content, cx, 40.0, 30.0, 120.0, 80.0);
        window.render_frame(cx);

        // W rewrites only the width through core history.
        window.click("sel-w", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .num_field
                .is_some(),
            "clicking the W field starts an edit session"
        );
        window.press("2", cx);
        window.press("0", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let selection = content
            .read(cx)
            .committed_selection()
            .expect("the selection survives the field edit");
        assert_eq!(
            selection.width,
            200.0,
            "the W field commits its parsed value (status {:?}, field {:?})",
            status_error(&content, cx),
            content.read(cx).state_for_test().read(cx).num_field
        );
        assert_eq!(
            (selection.x, selection.y, selection.height),
            (40.0, 30.0, 80.0),
            "the other dimensions survive the rewrite"
        );

        // X moves the box.
        window.click("sel-x", cx);
        window.press("6", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).committed_selection().expect("selection").x,
            60.0,
            "the X field moves the selection"
        );

        // The oracle's n > 0 guard: a non-positive width edit is ignored.
        window.click("sel-w", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content
                .read(cx)
                .committed_selection()
                .expect("selection")
                .width,
            200.0,
            "a non-positive width edit is ignored"
        );

        // R rotates the selection.
        window.click("sel-r", cx);
        window.press("3", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content
                .read(cx)
                .committed_selection()
                .expect("selection")
                .angle,
            30.0,
            "the R field commits the angle"
        );
    })
    .expect("test window remains open");
}
