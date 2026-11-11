//! The native tool cursors: Eyedropper, Paint Bucket and Crop switch the
//! pointer only over the canvas, and the other tools keep their policy cursor.

use super::*;
use crate::app::cursor::{
    take_tool_cursor_request_for_test, TOOL_BUCKET, TOOL_CROP, TOOL_EYEDROPPER,
};

fn hover_doc(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
    bounds: gpui_kit::Bounds<gpui_kit::Pixels>,
    placement: (f32, f32, f32, f32),
    doc: (u32, u32),
    x: f32,
    y: f32,
) {
    let position = window_point(bounds, placement, doc, x, y);
    let _ = window.dispatch_event(
        PlatformInput::MouseMove(MouseMoveEvent {
            position,
            ..Default::default()
        }),
        cx,
    );
}

#[gpui_kit::test]
fn tool_cursors_follow_eyedropper_bucket_and_crop(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 200));

        let mut check = |tool: ToolId, expected: Option<u8>, x: f32, y: f32| {
            set_tool(&content, cx, tool, 1.0);
            window.render_frame(cx);
            let _ = take_tool_cursor_request_for_test();
            hover_doc(window, cx, bounds, placement, (400, 200), x, y);
            assert_eq!(
                take_tool_cursor_request_for_test(),
                Some(expected),
                "{tool:?} at ({x}, {y})"
            );
        };

        check(ToolId::Eyedropper, Some(TOOL_EYEDROPPER), 100.0, 50.0);
        check(ToolId::PaintBucket, Some(TOOL_BUCKET), 100.0, 50.0);
        check(ToolId::Crop, Some(TOOL_CROP), 200.0, 100.0);
        check(ToolId::Selection, None, 100.0, 50.0);
        check(ToolId::Move, None, 100.0, 50.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn a_locked_layer_releases_the_bucket_cursor(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        let core_id = content
            .read(cx)
            .selected_layer_for_cursor(cx)
            .expect("the fixture selects a layer")
            .1
            .id;
        content.update(cx, |app, cx| app.toggle_layer_locked(core_id, cx));
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 200));
        let _ = take_tool_cursor_request_for_test();
        hover_doc(window, cx, bounds, placement, (400, 200), 100.0, 50.0);
        assert_eq!(
            take_tool_cursor_request_for_test(),
            Some(None),
            "a locked layer keeps the not-allowed cursor"
        );
    })
    .expect("test window remains open");
}
