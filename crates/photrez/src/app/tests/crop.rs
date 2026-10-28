use super::input::{drag_doc, page, pixels, set_tool, tab_to_canvas, window_point};
use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};

fn dispatch(window: &mut gpui_kit::Window, event: PlatformInput, cx: &mut gpui_kit::App) {
    let _ = window.dispatch_event(event, cx);
    window.render_frame(cx);
}

#[gpui_kit::test]
fn crop_drag_shows_a_preview_and_escape_clears_it(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.1, 0.1);
        let moved = window_point(bounds, placement, doc, 0.9, 0.9);
        let _ = window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let preview = content
            .read(cx)
            .crop_rect_for_test(cx)
            .expect("the drag updates the crop preview");
        assert!(preview[2] > 0. && preview[3] > 0., "preview={preview:?}");
        assert!(
            content.read(cx).active_core_selection_for_test().is_none(),
            "a crop drag must not create a pixel selection"
        );

        tab_to_canvas(window, cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_none(),
            "Escape must discard the pending crop"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn enter_applies_the_crop_and_undo_restores_the_document_size(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((2, 1))
        );
        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_some(),
            "the drag sets the crop frame"
        );

        tab_to_canvas(window, cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((1, 1)),
            "Enter must apply the crop"
        );
        assert!(content.read(cx).crop_rect_for_test(cx).is_none());
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Move,
            "applying a crop returns to Move"
        );
        assert!(
            content.read(cx).active_core_selection_for_test().is_none(),
            "the crop clears the pixel selection"
        );

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((2, 1)),
            "undo restores the pre-crop document size"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn arrow_keys_nudge_the_crop_frame_within_the_document(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.1, 0.1);
        let end = window_point(bounds, placement, doc, 0.9, 0.9);
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
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            }),
            cx,
        );
        let before = content
            .read(cx)
            .crop_rect_for_test(cx)
            .expect("the crop frame exists");
        tab_to_canvas(window, cx);
        window.press("right", cx);
        window.render_frame(cx);
        let nudged = content
            .read(cx)
            .crop_rect_for_test(cx)
            .expect("the crop frame survives the nudge");
        assert_eq!(nudged[0], (before[0] + 1.0).min(doc.0 as f64 - before[2]));
        assert_eq!(nudged[1], before[1]);
        assert_eq!(nudged[2], before[2]);
        assert_eq!(nudged[3], before[3]);
        // The 1-pixel-wide document clamps a left nudge at zero.
        window.press("left", cx);
        window.press("left", cx);
        window.render_frame(cx);
        assert_eq!(
            content
                .read(cx)
                .crop_rect_for_test(cx)
                .expect("frame exists")[0],
            0.0
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_without_a_frame_is_refused_without_history(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let cursor_before = content
            .read(cx)
            .active_core_history_cursor_for_test()
            .unwrap();
        content.update(cx, |app, cx| {
            let error = app.apply_crop(cx).expect_err("no frame is refused");
            assert!(error.contains("no crop rectangle"), "{error}");
        });
        assert_eq!(
            content
                .read(cx)
                .active_core_history_cursor_for_test()
                .unwrap(),
            cursor_before
        );
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((2, 1))
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn ctrl_z_undoes_crop_frame_steps_before_document_history(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);
        let frame = content.read(cx).crop_rect_for_test(cx).expect("frame");
        tab_to_canvas(window, cx);
        window.press("right", cx);
        window.render_frame(cx);
        let nudged = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert_ne!(nudged[0], frame[0]);
        let history_before = content
            .read(cx)
            .active_core_history_cursor_for_test()
            .unwrap();

        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx).expect("frame")[0],
            frame[0],
            "Ctrl+Z restores the previous crop frame"
        );
        assert_eq!(
            content
                .read(cx)
                .active_core_history_cursor_for_test()
                .unwrap(),
            history_before,
            "crop undo does not step document history"
        );

        window.press("ctrl-shift-z", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx).expect("frame")[0],
            nudged[0],
            "Ctrl+Shift+Z redoes the crop step"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_frame_moves_from_inside_and_resizes_from_a_corner(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test()
                .update(cx, |state, _| state.active_tool = ToolId::Crop);
        });
        window.render_frame(cx);
        drag_doc(window, (400, 300), (50.0, 50.0), (250.0, 200.0), cx);
        let frame = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert!((frame[2] - 200.0).abs() < 3.0, "frame={frame:?}");

        // Drag from inside the frame: move, size unchanged.
        drag_doc(window, (400, 300), (150.0, 120.0), (200.0, 150.0), cx);
        let moved = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert!(
            (moved[0] - (frame[0] + 50.0)).abs() < 3.0,
            "moved={moved:?}"
        );
        assert!((moved[1] - (frame[1] + 30.0)).abs() < 3.0);
        assert!((moved[2] - frame[2]).abs() < 1.0);
        assert!((moved[3] - frame[3]).abs() < 1.0);

        // Drag the top-left corner outward: resize.
        let (left, top, width, height) = (moved[0], moved[1], moved[2], moved[3]);
        drag_doc(
            window,
            (400, 300),
            (left as f32, top as f32),
            ((left + 20.0) as f32, (top + 10.0) as f32),
            cx,
        );
        let resized = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert!(
            resized[2] < width - 10.0 && resized[3] < height - 5.0,
            "resized={resized:?} from {moved:?}"
        );
    })
    .expect("test window remains open");
}
