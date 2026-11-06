//! Undo and history bindings: the documented shortcuts, the bottom-of-stack no-op,
//! undo stepping on an in-flight stroke, and the order two strokes restore in.

use super::*;

#[gpui_kit::test]
fn undo_redo_restores_brush_pixels_with_all_bindings(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        let doc = (w, h);
        let cursor_before = history_cursor(&content, cx);
        drag_doc(window, doc, (0.5, 0.5), (1.5, 1.5), cx);
        let (_, _, painted) = pixels(&content, cx);
        assert_ne!(painted, before, "the stroke must change pixels");
        let cursor_after = history_cursor(&content, cx);
        assert_eq!(cursor_after, cursor_before + 1, "one stroke = one entry");

        tab_to_canvas(window, cx);
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "Ctrl+Z must restore the pre-stroke pixels exactly"
        );
        assert_eq!(history_cursor(&content, cx), cursor_before);

        window.press("ctrl-shift-z", cx);
        window.render_frame(cx);
        assert_eq!(
            pixels(&content, cx).2,
            painted,
            "Ctrl+Shift+Z must re-apply the stroke exactly"
        );
        assert_eq!(history_cursor(&content, cx), cursor_after);

        window.press("ctrl-z", cx);
        window.press("ctrl-y", cx);
        window.render_frame(cx);
        assert_eq!(pixels(&content, cx).2, painted, "Ctrl+Y is the redo alias");
        assert_eq!(history_cursor(&content, cx), cursor_after);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn undo_at_the_bottom_is_a_no_op(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let (_, _, before) = pixels(&content, cx);
        let cursor_before = history_cursor(&content, cx);
        for _ in 0..3 {
            window.press("ctrl-z", cx);
            window.render_frame(cx);
        }
        let (_, _, after) = pixels(&content, cx);
        assert_eq!(after.len(), before.len());
        assert_eq!(history_cursor(&content, cx), cursor_before);
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input_policy::GesturePhase::Idle
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn undo_cancels_an_inflight_brush_before_stepping_history(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let (w, h, before) = pixels(&content, cx);
        let doc = (w, h);
        let cursor_before = history_cursor(&content, cx);
        drag_doc(window, doc, (0.5, 0.5), (1.5, 1.5), cx);
        assert_eq!(history_cursor(&content, cx), cursor_before + 1);

        let (bounds, placement) = page(window, doc);
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: window_point(bounds, placement, doc, 1.5, 0.5),
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: window_point(bounds, placement, doc, 0.5, 1.5),
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input_policy::GesturePhase::Action
        );

        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input_policy::GesturePhase::Idle,
            "undo must drop the in-flight stroke"
        );
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "the in-flight stroke must not commit and the first stroke undoes"
        );
        assert_eq!(history_cursor(&content, cx), cursor_before);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn undo_restores_a_move_transform_through_the_metadata_branch(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Move, 2.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let transform_before = content
            .read(cx)
            .active_core_transform_for_test()
            .expect("fixture has a transform");
        let cursor_before = history_cursor(&content, cx);
        drag_doc(window, (400, 300), (200.0, 150.0), (240.0, 180.0), cx);
        let transform_after = content
            .read(cx)
            .active_core_transform_for_test()
            .expect("fixture has a transform");
        assert_ne!(transform_after, transform_before);
        assert_eq!(history_cursor(&content, cx), cursor_before + 1);

        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_transform_for_test(),
            Some(transform_before),
            "undo must restore the metadata transform"
        );
        window.press("ctrl-y", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_transform_for_test(),
            Some(transform_after)
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn history_steps_two_brush_strokes_in_order(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (w, h, blank) = pixels(&content, cx);
        let doc = (w, h);
        let cursor_blank = history_cursor(&content, cx);

        drag_doc(window, doc, (0.2, 0.2), (0.8, 0.8), cx);
        let (_, _, first) = pixels(&content, cx);
        let cursor_first = history_cursor(&content, cx);
        assert_eq!(cursor_first, cursor_blank + 1);

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.fg_color = [255, 0, 0, 255];
            });
        });
        drag_doc(window, doc, (1.2, 0.2), (1.8, 0.8), cx);
        let (_, _, second) = pixels(&content, cx);
        assert_ne!(first, second, "the second stroke must paint");
        let cursor_second = history_cursor(&content, cx);
        assert_eq!(cursor_second, cursor_first + 1, "one entry per stroke");

        tab_to_canvas(window, cx);
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(pixels(&content, cx).2, first, "undo the second stroke");
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(pixels(&content, cx).2, blank, "undo the first stroke");
        assert_eq!(history_cursor(&content, cx), cursor_blank);

        window.press("ctrl-shift-z", cx);
        window.render_frame(cx);
        assert_eq!(pixels(&content, cx).2, first);
        window.press("ctrl-y", cx);
        window.render_frame(cx);
        assert_eq!(pixels(&content, cx).2, second);
        assert_eq!(history_cursor(&content, cx), cursor_second);
    })
    .expect("test window remains open");
}
