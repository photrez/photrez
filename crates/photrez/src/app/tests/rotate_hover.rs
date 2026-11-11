//! The Move rotate hover and the cursor it requests.

use super::canvas_input::{page, unlock_background_transform};
use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;

fn new_doc(content: &Entity<App>, cx: &mut gpui_kit::App, width: u32, height: u32) {
    content.update(cx, |app, cx| {
        app.create_document(width, height, cx)
            .expect("blank document");
        app.state_for_test()
            .update(cx, |state, _| state.active_tool = ToolId::Move);
    });
    unlock_background_transform(content, cx);
}

/// The Move rotate hover asks for the rotate cursor, including over the
/// pasteboard outside the page; the sprite only remains where no native
/// cursor exists. Space wins over every handle cursor with the grab cursor.
#[gpui_kit::test]
fn move_rotate_hover_paints_the_sprite_and_space_wins(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 400, 300);
        content.update(cx, |app, cx| {
            // Half-size layer so the rotate band extends past the page edge.
            app.set_transform_field("num-w", 200.0, cx)
                .expect("width edit applies");
            app.set_transform_field("num-h", 150.0, cx)
                .expect("height edit applies");
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 300));
        let outside = super::canvas_input::window_point(bounds, placement, (400, 300), -10.0, 75.0);
        let corner = super::canvas_input::window_point(bounds, placement, (400, 300), 200.0, 150.0);
        let _ = crate::app::cursor::take_last_request_for_test();
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: outside,
                ..Default::default()
            }),
            cx,
        );
        #[cfg(not(windows))]
        {
            let sprite = content
                .update(cx, |app, cx| app.rotate_cursor_for_test(cx))
                .expect("the rotate hover paints the sprite");
            assert_eq!(
                (sprite.position[0], sprite.position[1]),
                (f32::from(outside.x), f32::from(outside.y)),
                "the sprite anchors at the pointer"
            );
        }
        #[cfg(windows)]
        assert!(
            content
                .update(cx, |app, cx| app.rotate_cursor_for_test(cx))
                .is_none(),
            "the native cursor replaces the sprite"
        );
        let expected_direction = content
            .update(cx, |app, cx| app.rotate_cursor_direction(cx))
            .expect("the rotate hover has a direction");
        assert_eq!(
            crate::app::cursor::take_last_request_for_test(),
            Some((true, expected_direction)),
            "the pasteboard rotate hover asks for the native cursor"
        );
        assert_eq!(
            content.update(cx, |app, cx| app.canvas_cursor_policy(cx)),
            crate::input_policy::CursorPolicy::Rotate,
            "the rotate hover keeps the crosshair policy under the native cursor"
        );

        let _ = crate::app::cursor::take_last_request_for_test();
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: corner,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.update(cx, |app, cx| app.canvas_cursor_policy(cx)),
            crate::input_policy::CursorPolicy::ResizeNwse,
            "the SE corner hovers as a resize cursor before Space"
        );
        assert_eq!(
            crate::app::cursor::take_last_request_for_test(),
            Some((false, 0)),
            "leaving the rotate band returns the policy cursor"
        );
        super::canvas_input::tab_to_canvas(window, cx);
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
                keystroke: gpui_kit::Keystroke {
                    modifiers: gpui_kit::Modifiers::default(),
                    key: "space".to_string(),
                    key_char: None,
                },
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
        assert_eq!(
            content.update(cx, |app, cx| app.canvas_cursor_policy(cx)),
            crate::input_policy::CursorPolicy::OpenHand,
            "Space wins over the handle cursor with the grab cursor"
        );
        assert!(
            content
                .update(cx, |app, cx| app.rotate_cursor_for_test(cx))
                .is_none(),
            "Space paints no rotate sprite"
        );
    })
    .expect("test window remains open");
}

/// An overlay above the canvas releases the rotate cursor even when the
/// pointer has not moved off the canvas.
#[gpui_kit::test]
fn a_menu_over_the_canvas_releases_the_rotate_cursor(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 400, 300);
        content.update(cx, |app, cx| {
            app.set_transform_field("num-w", 200.0, cx)
                .expect("width edit applies");
            app.set_transform_field("num-h", 150.0, cx)
                .expect("height edit applies");
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 300));
        let outside = super::canvas_input::window_point(bounds, placement, (400, 300), -10.0, 75.0);
        let _ = crate::app::cursor::take_last_request_for_test();
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: outside,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            crate::app::cursor::take_last_request_for_test().map(|(active, _)| active),
            Some(true),
            "the rotate hover owns the cursor before the menu opens"
        );

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::File);
            });
        });
        window.render_frame(cx);
        assert_eq!(
            crate::app::cursor::take_last_request_for_test(),
            Some((false, 0)),
            "the menu releases the canvas rotate cursor"
        );
    })
    .expect("test window remains open");
}
