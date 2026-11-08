//! Modal input isolation: while a dialog is open, the surfaces behind it
//! answer nothing - pressing the dialog's titlebar lets the canvas capture
//! nothing, and a wheel over the canvas neither zooms nor pans. The dialog
//! drag and the canvas gesture must never run as one combined action.
//!
//! The paint path is not covered here on purpose: the input policy already
//! refuses brush targets behind a modal, so a paint assertion could not be
//! shown red. The pointer capture and the camera are the observable leaks.
//!
//! RED proofs: against the unguarded input path the capture test fails on the
//! canvas owning the pointer, and the wheel test on the camera it moved.

use super::*;
use gpui_kit::test::TestWindowExt as _;

/// Press at `from`, drag by `by`, release: three dispatched events with a
/// rendered frame after each, the way a real drag reaches the app.
fn press_drag_release(
    window: &mut gpui_kit::Window,
    from: gpui_kit::Point<gpui_kit::Pixels>,
    by: (f32, f32),
    cx: &mut gpui_kit::App,
) {
    let to = gpui_kit::point(from.x + gpui_kit::px(by.0), from.y + gpui_kit::px(by.1));
    let _ = window.dispatch_event(
        gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
            button: gpui_kit::MouseButton::Left,
            position: from,
            click_count: 1,
            ..Default::default()
        }),
        cx,
    );
    window.render_frame(cx);
    let _ = window.dispatch_event(
        gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
            position: to,
            ..Default::default()
        }),
        cx,
    );
    window.render_frame(cx);
    let _ = window.dispatch_event(
        gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
            button: gpui_kit::MouseButton::Left,
            position: to,
            click_count: 1,
            ..Default::default()
        }),
        cx,
    );
    window.render_frame(cx);
}

#[gpui_kit::test]
fn dragging_a_dialog_titlebar_captures_no_canvas_pointer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);

        let handle = window.find("dialog-drag-handle").bounds().center();
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: handle,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        assert!(
            window.captured_hitbox().is_none(),
            "pressing a dialog's titlebar must not let the canvas behind it \
             capture the pointer"
        );

        // The drag itself still moves the dialog: a follow-up press-move-up
        // keeps the session alive without the canvas ever owning the pointer.
        press_drag_release(window, handle, (120., 80.), cx);
        assert!(
            window.captured_hitbox().is_none(),
            "the dialog drag must not hand the pointer to the canvas"
        );
        assert!(
            window.try_find("dialog-drag-handle").is_some(),
            "the dialog stays open through its own drag"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn wheel_over_the_canvas_behind_a_modal_moves_no_camera(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);

        let before = content.read(cx).camera_for_test();
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::Scroll(gpui_kit::ScrollEvent {
                position: gpui_kit::point(gpui_kit::px(683.), gpui_kit::px(700.)),
                delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
                    gpui_kit::px(0.),
                    gpui_kit::px(-80.),
                )),
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        assert_eq!(
            content.read(cx).camera_for_test(),
            before,
            "a wheel behind an open modal must not pan or zoom the canvas"
        );
    })
    .expect("test window remains open");
}
