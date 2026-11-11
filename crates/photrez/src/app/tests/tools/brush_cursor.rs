//! The brush ring as the pointer: it follows every move, requests a hidden
//! system pointer, and yields to overlays and locked layers.

use super::*;

fn mount_brush(
    cx: &mut gpui_kit::TestAppContext,
) -> (
    gpui_kit::WindowHandle<gpui_kit::base::Root>,
    Entity<App>,
    TestDirectory,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    (window, content, directory)
}

fn move_pointer(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
    position: gpui_kit::Point<gpui_kit::Pixels>,
) {
    let _ = window.dispatch_event(
        PlatformInput::MouseMove(MouseMoveEvent {
            position,
            ..Default::default()
        }),
        cx,
    );
}

#[gpui_kit::test]
fn brush_hover_hides_the_pointer_and_tracks_every_move(cx: &mut gpui_kit::TestAppContext) {
    let (window, content, _source) = mount_brush(cx);
    cx.update_window(window.into(), |_, window, cx| {
        let (bounds, placement) = page(window, (400, 200));
        let first = window_point(bounds, placement, (400, 200), 100.0, 50.0);
        let second = window_point(bounds, placement, (400, 200), 260.0, 140.0);
        let pointer_hidden = true;

        let _ = crate::app::cursor::take_brush_hidden_request_for_test();
        let _ = crate::app::cursor::take_brush_hidden_calls_for_test();
        move_pointer(window, cx, first);
        assert_eq!(
            crate::app::cursor::take_brush_hidden_request_for_test(),
            Some(pointer_hidden),
            "the brush hover requests the platform pointer state"
        );
        assert_eq!(
            crate::app::cursor::take_brush_hidden_calls_for_test(),
            1,
            "the hover runs the branch that requests its frame"
        );
        window.render_frame(cx);
        let ring = content
            .update(cx, |app, cx| app.brush_cursor_for_test(cx))
            .expect("the brush paints its ring");
        assert!(
            (ring.center[0] - 100.0).abs() < 0.5 && (ring.center[1] - 50.0).abs() < 0.5,
            "the first ring centre follows the pointer: {:?}",
            ring.center
        );

        move_pointer(window, cx, second);
        assert_eq!(
            crate::app::cursor::take_brush_hidden_request_for_test(),
            Some(pointer_hidden),
            "the next move keeps the same platform pointer state"
        );
        assert_eq!(
            crate::app::cursor::take_brush_hidden_calls_for_test(),
            1,
            "every move runs the branch that requests its frame"
        );
        window.render_frame(cx);
        let ring = content
            .update(cx, |app, cx| app.brush_cursor_for_test(cx))
            .expect("the brush paints its ring");
        assert!(
            (ring.center[0] - 260.0).abs() < 0.5 && (ring.center[1] - 140.0).abs() < 0.5,
            "the ring centre follows the second move: {:?}",
            ring.center
        );

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::File);
            });
        });
        let _ = crate::app::cursor::take_brush_hidden_request_for_test();
        window.render_frame(cx);
        assert_eq!(
            crate::app::cursor::take_brush_hidden_request_for_test(),
            Some(false),
            "an overlay releases the pointer"
        );
        assert!(
            content
                .update(cx, |app, cx| app.brush_cursor_for_test(cx))
                .is_none(),
            "the overlay hides the ring"
        );

        let _ = crate::app::cursor::take_brush_hidden_request_for_test();
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = None;
            });
            app.canvas_hover_end(cx);
        });
        assert_eq!(
            crate::app::cursor::take_brush_hidden_request_for_test(),
            Some(false),
            "leaving the canvas releases the pointer"
        );
        window.render_frame(cx);
        assert!(
            content
                .update(cx, |app, cx| app.brush_cursor_for_test(cx))
                .is_none(),
            "leaving the canvas erases the ring"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn a_locked_layer_keeps_its_not_allowed_cursor(cx: &mut gpui_kit::TestAppContext) {
    let (window, content, _source) = mount_brush(cx);
    cx.update_window(window.into(), |_, window, cx| {
        let core_id = content
            .read(cx)
            .selected_layer_for_cursor(cx)
            .expect("the fixture selects a layer")
            .1
            .id;
        content.update(cx, |app, cx| app.toggle_layer_locked(core_id, cx));
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 200));
        let position = window_point(bounds, placement, (400, 200), 100.0, 50.0);

        let _ = crate::app::cursor::take_brush_hidden_request_for_test();
        let _ = crate::app::cursor::take_brush_hidden_calls_for_test();
        move_pointer(window, cx, position);
        assert_eq!(
            crate::app::cursor::take_brush_hidden_request_for_test(),
            Some(false),
            "the not-allowed cursor stays visible on a locked layer"
        );
        assert_eq!(
            content.update(cx, |app, cx| app.canvas_cursor_policy(cx)),
            crate::input_policy::CursorPolicy::NotAllowed
        );
    })
    .expect("test window remains open");
}
