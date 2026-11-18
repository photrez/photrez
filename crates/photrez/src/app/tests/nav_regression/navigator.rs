//! The Navigator: the no-document body, the collapse and zoom controls, and a modal opened during a zoom drag.

use super::*;

/// The Navigator's no-document body: the dashed no-image state replaces the
/// canvas and its pan hitbox, and the zoom row loses its handlers, so a stray
/// click leaves no error behind.
///
/// RED proof: mount the canvas body regardless of the canvas and the second
/// assert fails on the hitbox the empty branch must not install.
#[gpui_kit::test]
fn navigator_shows_the_no_image_state_without_a_document(cx: &mut TestAppContext) {
    let (handle, content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("navigator-empty").is_some(),
            "the dashed no-image state mounts without a document"
        );
        assert!(
            window.try_find("navigator-hitbox").is_none(),
            "the pan hitbox belongs to the canvas body, not the empty state"
        );
        assert!(
            window.try_find("navigator-zoom-row").is_some(),
            "the zoom row still mounts over the empty state"
        );
        for id in ["navigator-zoom-out", "navigator-zoom-in"] {
            window.click(id, cx);
            window.render_frame(cx);
            assert!(
                content
                    .read(cx)
                    .state_for_test()
                    .read(cx)
                    .status_error
                    .is_none(),
                "{id} must be inert without a document, not a rejected command"
            );
        }
    })
    .unwrap();
}

/// The collapse chevron folds the section to its header and back, and the fit
/// button plus the zoom +/- steps drive the real camera.
///
/// RED proof: wire the buttons to no-ops and the zoom asserts fail on the
/// unchanged camera.
#[gpui_kit::test]
fn navigator_collapse_and_zoom_drive_the_camera(cx: &mut TestAppContext) {
    let paths = StartupTestFiles::new();
    let source = paths.file("nav-zoom.ptz");
    let io = Arc::new(StartupFileIo::default());
    io.insert(&source, startup_archive("doc-nav", 1, 1, false, 0));
    let (handle, content) = open_app_with(cx, Some((source, io)));
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("navigator-empty").is_none(),
            "a document shows the canvas preview, not the empty state"
        );
        assert!(window.try_find("navigator-hitbox").is_some());

        window.click("navigator-fit", cx);
        window.render_frame(cx);
        let fitted = content
            .read(cx)
            .camera_for_test()
            .expect("fit stores a concrete camera")
            .zoom;

        window.click("navigator-zoom-in", cx);
        window.render_frame(cx);
        let zoomed = content
            .read(cx)
            .camera_for_test()
            .expect("camera after zoom-in")
            .zoom;
        assert!(
            zoomed > fitted,
            "the navigator + button must zoom the camera in"
        );

        window.click("navigator-zoom-out", cx);
        window.render_frame(cx);
        let stepped = content
            .read(cx)
            .camera_for_test()
            .expect("camera after zoom-out")
            .zoom;
        assert!(
            stepped < zoomed,
            "the navigator - button must zoom the camera back out"
        );

        window.click("navigator-collapse", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("navigator-hitbox").is_none(),
            "the collapsed section mounts no body"
        );
        window.click("navigator-collapse", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("navigator-hitbox").is_some(),
            "re-expanding mounts the body again"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn opening_a_modal_during_navigator_zoom_drag_stops_later_camera_updates(cx: &mut TestAppContext) {
    let paths = StartupTestFiles::new();
    let source = paths.file("nav-modal-zoom.ptz");
    let io = Arc::new(StartupFileIo::default());
    io.insert(&source, startup_archive("doc-nav-modal", 1, 1, false, 0));
    let (handle, content) = open_app_with(cx, Some((source, io)));
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("navigator-fit", cx);
        window.render_frame(cx);
        let bounds = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .nav_zoom_track_bounds
            .get()
            .expect("the zoom track has laid-out bounds");
        let y = bounds.origin.y + bounds.size.height / 2.0;
        let at = |fraction: f32| {
            gpui_kit::point(
                bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * fraction),
                y,
            )
        };
        let before = content
            .read(cx)
            .camera_for_test()
            .expect("the opened document has a camera")
            .zoom;
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: at(0.60),
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let zoom_before_modal = content
            .read(cx)
            .camera_for_test()
            .expect("the zoom track keeps a camera")
            .zoom;
        assert_ne!(zoom_before_modal, before, "the press changes zoom");

        window.press("ctrl-n", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::NewDocument)
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: at(0.25),
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let zoom_after_modal = content
            .read(cx)
            .camera_for_test()
            .expect("the zoom track keeps a camera")
            .zoom;
        assert_eq!(
            zoom_after_modal, zoom_before_modal,
            "a captured zoom move must not change the camera behind a modal"
        );

        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: at(0.25),
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
}
