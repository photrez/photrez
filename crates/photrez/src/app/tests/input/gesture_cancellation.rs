//! In-flight gesture lifecycle: a stroke that is still open is cancelled by a tool
//! switch or by Escape, and the canvas's own focus and shortcut rules ride with it.

use super::*;

#[gpui_kit::test]
fn real_brush_tool_roundtrip_cancels_old_stroke_and_commits_fresh_stroke_once(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let (w, h, before) = pixels(&content, cx);
        let doc = (w, h);
        let history_before = history_version(&content, cx);
        let save_before = content
            .read(cx)
            .active_document_save_state_for_test()
            .expect("fixture has an active session");
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.5, 0.5);
        let end = window_point(bounds, placement, doc, 1.5, 0.5);

        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input::GesturePhase::Action
        );
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "unreleased brush is not committed"
        );
        assert_eq!(history_version(&content, cx), history_before);
        assert_eq!(
            content
                .read(cx)
                .active_document_save_state_for_test()
                .expect("fixture has an active session"),
            save_before
        );

        window.press("e", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert_eq!(app.state_for_test().read(cx).active_tool, ToolId::Eraser);
            assert_eq!(app.input_phase_for_test(), crate::input::GesturePhase::Idle);
            assert!(app.pending_gesture.is_none());
        });
        window.press("b", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert_eq!(app.state_for_test().read(cx).active_tool, ToolId::Brush);
            assert_eq!(app.input_phase_for_test(), crate::input::GesturePhase::Idle);
            assert!(app.pending_gesture.is_none());
        });

        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "stale release must not commit"
        );
        assert_eq!(history_version(&content, cx), history_before);
        assert_eq!(
            content
                .read(cx)
                .active_document_save_state_for_test()
                .expect("fixture has an active session"),
            save_before
        );
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input::GesturePhase::Idle
        );

        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.5, 0.5);
        let end = window_point(bounds, placement, doc, 1.5, 0.5);
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        assert_ne!(
            pixels(&content, cx).2,
            before,
            "fresh stroke must change canonical pixels"
        );
        assert_eq!(history_version(&content, cx), history_before + 1);
        let save_after = content
            .read(cx)
            .active_document_save_state_for_test()
            .expect("fixture has an active session");
        assert_eq!(
            save_after.0,
            save_before.0 + 1,
            "one stroke adds one revision"
        );
        assert_eq!(save_after.1, save_before.1);
        assert!(save_after.2);
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input::GesturePhase::Idle
        );
        assert!(content.read(cx).pending_gesture.is_none());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn tool_switch_cancels_an_inflight_gesture(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        for (tool, id) in [
            (ToolId::Move, "move"),
            (ToolId::Selection, "selection"),
            (ToolId::Crop, "crop"),
            (ToolId::Eyedropper, "eyedropper"),
            (ToolId::Brush, "brush"),
            (ToolId::Eraser, "eraser"),
            (ToolId::PaintBucket, "paintBucket"),
            (ToolId::Gradient, "gradient"),
            (ToolId::Shape, "shape"),
            (ToolId::Text, "text"),
        ] {
            content.update(cx, |app, _| {
                app.pending_gesture = Some(PendingGesture {
                    tool: ToolId::Move,
                    start: (0.0, 0.0),
                    last: (1.0, 1.0),
                    shift: false,
                    alt: false,
                });
            });
            window.click(id, cx);
            window.render_frame(cx);
            content.update(cx, |app, cx| {
                assert_eq!(app.state_for_test().read(cx).active_tool, tool);
                assert!(app.pending_gesture.is_none());
                assert_eq!(app.input_phase_for_test(), crate::input::GesturePhase::Idle);
            });
        }
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn canvas_tool_shortcuts_require_canvas_focus_and_variants(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 2.0);
        window.render_frame(cx);

        window.press("t", cx);
        content.update(cx, |app, cx| {
            assert_eq!(app.state_for_test().read(cx).active_tool, ToolId::Move);
        });

        tab_to_canvas(window, cx);
        assert_eq!(window.find("canvas-hitbox").focused(), Some(true));

        for (key, tool) in [
            ("v", ToolId::Move),
            ("m", ToolId::Selection),
            ("c", ToolId::Crop),
            ("i", ToolId::Eyedropper),
            ("b", ToolId::Brush),
            ("e", ToolId::Eraser),
            ("u", ToolId::Shape),
            ("t", ToolId::Text),
        ] {
            window.press(key, cx);
            content.update(cx, |app, cx| {
                assert_eq!(app.state_for_test().read(cx).active_tool, tool);
            });
        }

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.selection_shape = SelectionShape::Ellipse
            });
        });
        window.press("m", cx);
        content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            assert_eq!(state.active_tool, ToolId::Selection);
            assert_eq!(state.selection_shape, SelectionShape::Rect);
        });
        window.press("shift-m", cx);
        content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            assert_eq!(state.active_tool, ToolId::Selection);
            assert_eq!(state.selection_shape, SelectionShape::Ellipse);
        });

        window.press("c", cx);
        window.press("shift-m", cx);
        content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            assert_eq!(state.active_tool, ToolId::Crop);
            assert_eq!(state.selection_shape, SelectionShape::Rect);
        });

        for (key, tool) in [
            ("g", ToolId::PaintBucket),
            ("shift-g", ToolId::Gradient),
            ("g", ToolId::PaintBucket),
        ] {
            window.press(key, cx);
            content.update(cx, |app, cx| {
                assert_eq!(app.state_for_test().read(cx).active_tool, tool);
            });
        }
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn canvas_hitbox_is_tab_focusable_without_changing_bounds(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, _content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let bounds_before = window.find("canvas-hitbox").bounds();

        tab_to_canvas(window, cx);

        assert_eq!(window.find("canvas-hitbox").focused(), Some(true));
        assert_eq!(window.find("canvas-hitbox").bounds(), bounds_before);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_cancels_an_inflight_brush_without_commit(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let (w, h, before) = pixels(&content, cx);
        let doc = (w, h);
        let history_before = history_version(&content, cx);
        let save_before = content
            .read(cx)
            .active_document_save_state_for_test()
            .expect("fixture has an active session");
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.5, 0.5);
        let end = window_point(bounds, placement, doc, 1.5, 0.5);

        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input::GesturePhase::Action
        );
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "unreleased brush is not committed"
        );

        window.press("escape", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input::GesturePhase::Idle,
            "Escape must end the gesture"
        );
        assert!(content.read(cx).pending_gesture.is_none());
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "Escape must not commit pixels"
        );
        assert_eq!(
            history_version(&content, cx),
            history_before,
            "Escape must not add history"
        );
        assert_eq!(
            content
                .read(cx)
                .active_document_save_state_for_test()
                .expect("fixture has an active session"),
            save_before
        );

        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.5, 0.5);
        let end = window_point(bounds, placement, doc, 1.5, 0.5);
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert_ne!(
            pixels(&content, cx).2,
            before,
            "a fresh stroke after Escape must commit"
        );
        assert_eq!(history_version(&content, cx), history_before + 1);
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input::GesturePhase::Idle
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_cancels_a_pending_selection_and_stale_release_does_nothing(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.5, 0.5);
        let end = window_point(bounds, placement, doc, 2.5, 2.5);

        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert!(
            content.read(cx).active_core_selection_for_test().is_none(),
            "selection is pending until pointer-up"
        );

        window.press("escape", cx);
        window.render_frame(cx);
        assert!(content.read(cx).pending_gesture.is_none());
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input::GesturePhase::Idle
        );

        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert!(
            content.read(cx).active_core_selection_for_test().is_none(),
            "a stale release after Escape must not apply a selection"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn tool_switch_cancels_a_real_pending_selection_gesture(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.5, 0.5);
        let end = window_point(bounds, placement, doc, 2.5, 2.5);

        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert!(
            content.read(cx).active_core_selection_for_test().is_none(),
            "selection is pending until pointer-up"
        );
        content.update(cx, |app, _| {
            assert!(app.pending_gesture.is_some());
        });

        window.click("brush", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert_eq!(app.state_for_test().read(cx).active_tool, ToolId::Brush);
            assert!(app.pending_gesture.is_none());
            assert_eq!(app.input_phase_for_test(), crate::input::GesturePhase::Idle);
        });

        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert!(
            content.read(cx).active_core_selection_for_test().is_none(),
            "a stale release after a tool switch must not apply the old gesture"
        );
    })
    .expect("test window remains open");
}
