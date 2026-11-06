//! Pointer capture: a release outside the canvas still commits, a lost capture drops
//! the stroke, the bracket keys step size and hardness, and the same sequence driven
//! twice must produce the same pixels.

use super::*;

#[gpui_kit::test]
fn pointer_capture_commits_a_release_outside_the_canvas(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let (w, h, before) = pixels(&content, cx);
        let doc = (w, h);
        let cursor_before = history_cursor(&content, cx);
        let (bounds, placement) = page(window, doc);
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: window_point(bounds, placement, doc, 0.5, 0.5),
                ..Default::default()
            }),
            cx,
        );
        assert!(
            window.captured_hitbox().is_some(),
            "pointer-down must capture the canvas hitbox"
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: window_point(bounds, placement, doc, 1.5, 0.5),
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        // Release over the title bar, outside the canvas surface.
        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: point(px(5.0), px(5.0)),
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert!(
            window.captured_hitbox().is_none(),
            "pointer-up must release the capture"
        );
        assert_ne!(
            pixels(&content, cx).2,
            before,
            "a release outside the canvas must still commit the brush stroke"
        );
        assert_eq!(history_cursor(&content, cx), cursor_before + 1);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn lost_capture_drops_the_inflight_stroke_without_commit(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let (w, h, before) = pixels(&content, cx);
        let doc = (w, h);
        let history_before = history_version(&content, cx);
        let (bounds, placement) = page(window, doc);
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: window_point(bounds, placement, doc, 0.5, 0.5),
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: window_point(bounds, placement, doc, 1.5, 0.5),
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
        content.update(cx, |app, cx| {
            // The window-deactivation observer runs the same pair.
            app.release_canvas_pointer(window);
            app.cancel_canvas_gesture(cx);
        });
        assert!(window.captured_hitbox().is_none());
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input_policy::GesturePhase::Idle
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: window_point(bounds, placement, doc, 1.5, 0.5),
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "a lost capture must never commit the stroke"
        );
        assert_eq!(history_version(&content, cx), history_before);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn bracket_keys_step_size_and_hardness_only_for_paint_tools(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 10.0);
        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.brush_hardness = 0.5);
        });
        window.render_frame(cx);

        // Canvas-scoped: without canvas focus nothing changes.
        window.press("]", cx);
        content.update(cx, |app, cx| {
            assert_eq!(app.state_for_test().read(cx).brush_size, 10.0);
        });

        tab_to_canvas(window, cx);
        let brush = |content: &Entity<App>, cx: &mut gpui_kit::App| {
            let state = content.read(cx).state_for_test().read(cx);
            (state.brush_size, state.brush_hardness)
        };
        window.press("]", cx);
        assert_eq!(brush(&content, cx).0, 11.0, "one step is ~10% (min 1)");
        window.press("[", cx);
        assert_eq!(brush(&content, cx).0, 10.0);
        window.press("shift-]", cx);
        assert_eq!(brush(&content, cx).1, 0.6);
        window.press("shift-[", cx);
        assert_eq!(brush(&content, cx).1, 0.5);

        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.brush_size = 1.0);
        });
        window.press("[", cx);
        assert_eq!(brush(&content, cx).0, 1.0, "size clamps at 1");
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.brush_size = 2000.0;
                state.brush_hardness = 1.0;
            });
        });
        window.press("]", cx);
        window.press("shift-]", cx);
        assert_eq!(
            brush(&content, cx),
            (2000.0, 1.0),
            "size/hardness clamp at their maxima"
        );

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Eraser;
                state.brush_size = 10.0;
                state.eraser_size = 10.0;
            });
        });
        window.press("]", cx);
        let eraser_state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            eraser_state.eraser_size, 11.0,
            "the eraser has its own size"
        );
        assert_eq!(
            eraser_state.brush_size, 10.0,
            "the eraser step leaves the brush size alone"
        );
        let _ = eraser_state;

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Selection;
                state.brush_size = 10.0;
            });
        });
        window.press("]", cx);
        assert_eq!(
            brush(&content, cx).0,
            10.0,
            "brackets are paint-tool shortcuts only"
        );
    })
    .expect("test window remains open");
}

fn run_pointer_sequence(
    window: &mut gpui_kit::Window,
    content: &Entity<App>,
    doc: (u32, u32),
    render_between_events: bool,
    cx: &mut gpui_kit::App,
) -> Vec<u8> {
    let (bounds, placement) = page(window, doc);
    let steps = [(0.5, 0.5), (0.8, 0.5), (1.1, 0.5), (1.4, 0.5), (1.7, 0.5)];
    dispatch_pointer_event(
        window,
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position: window_point(bounds, placement, doc, steps[0].0, steps[0].1),
            ..Default::default()
        }),
        cx,
    );
    if render_between_events {
        window.render_frame(cx);
    }
    for step in &steps[1..] {
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: window_point(bounds, placement, doc, step.0, step.1),
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        if render_between_events {
            window.render_frame(cx);
        }
    }
    dispatch_pointer_event(
        window,
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position: window_point(bounds, placement, doc, steps[4].0, steps[4].1),
            ..Default::default()
        }),
        cx,
    );
    window.render_frame(cx);
    pixels(content, cx).2
}

#[gpui_kit::test]
fn identical_pointer_sequences_produce_identical_pixels(cx: &mut gpui_kit::TestAppContext) {
    // No app-side event accumulator, so both a single dispatch
    // burst and a render between every event must produce the same canonical
    // pixels (no dropped or duplicated dab).
    let first_dir = TestDirectory::new();
    let (first_window, first, _source) = mount_fixture_app(cx, &first_dir);
    let first_pixels = cx
        .update_window(first_window.into(), |_, window, cx| {
            set_tool(&first, cx, ToolId::Brush, 2.0);
            window.render_frame(cx);
            let (w, h, _) = pixels(&first, cx);
            run_pointer_sequence(window, &first, (w, h), false, cx)
        })
        .expect("first test window remains open");

    let second_dir = TestDirectory::new();
    let (second_window, second, _source) = mount_fixture_app(cx, &second_dir);
    let second_pixels = cx
        .update_window(second_window.into(), |_, window, cx| {
            set_tool(&second, cx, ToolId::Brush, 2.0);
            window.render_frame(cx);
            let (w, h, _) = pixels(&second, cx);
            run_pointer_sequence(window, &second, (w, h), true, cx)
        })
        .expect("second test window remains open");

    assert_eq!(
        first_pixels, second_pixels,
        "event batching must not change the committed pixels"
    );
}
