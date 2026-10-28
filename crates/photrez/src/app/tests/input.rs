use super::*;
use crate::input::PendingGesture;
use crate::render::canvas_bridge::fit_placement;
use crate::state::{SelectionShape, ToolId};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    point, px, Bounds, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    PlatformInput, Point,
};
use photrez_core::ptz::content::LayerType;

pub(super) fn set_tool(content: &Entity<App>, cx: &mut gpui_kit::App, tool: ToolId, size: f64) {
    content.update(cx, |app, cx| {
        app.state_for_test().update(cx, |state, _| {
            state.active_tool = tool;
            state.brush_size = size;
            state.fg_color = [0, 0, 0, 255];
            state.selection_shape = SelectionShape::Rect;
        });
    });
}

pub(super) fn pixels(content: &Entity<App>, cx: &mut gpui_kit::App) -> (u32, u32, Vec<u8>) {
    content
        .read(cx)
        .active_core_pixels_for_test()
        .expect("fixture has a raster layer")
}

pub(super) fn page(
    window: &mut gpui_kit::Window,
    doc: (u32, u32),
) -> (Bounds<Pixels>, (f32, f32, f32, f32)) {
    let bounds = window.find("canvas-hitbox").bounds();
    let page = fit_placement(
        bounds.size.width.into(),
        bounds.size.height.into(),
        doc.0 as f32,
        doc.1 as f32,
    )
    .expect("the canvas has a page placement");
    (bounds, page)
}

pub(super) fn window_point(
    bounds: Bounds<Pixels>,
    page: (f32, f32, f32, f32),
    doc: (u32, u32),
    x: f32,
    y: f32,
) -> Point<Pixels> {
    let (page_x, page_y, page_w, page_h) = page;
    let local_x = page_x + x / doc.0 as f32 * page_w;
    let local_y = page_y + y / doc.1 as f32 * page_h;
    bounds.origin + point(px(local_x), px(local_y))
}

fn local_point(page: (f32, f32, f32, f32), doc: (u32, u32), x: f32, y: f32) -> Point<Pixels> {
    let (page_x, page_y, page_w, page_h) = page;
    point(
        px(page_x + x / doc.0 as f32 * page_w),
        px(page_y + y / doc.1 as f32 * page_h),
    )
}

pub(super) fn drag_doc(
    window: &mut gpui_kit::Window,
    doc: (u32, u32),
    from: (f32, f32),
    to: (f32, f32),
    cx: &mut gpui_kit::App,
) {
    let (bounds, page) = page(window, doc);
    window.drag(
        window_point(bounds, page, doc, from.0, from.1),
        window_point(bounds, page, doc, to.0, to.1),
        cx,
    );
    window.render_frame(cx);
}

fn click_doc(
    window: &mut gpui_kit::Window,
    doc: (u32, u32),
    at: (f32, f32),
    cx: &mut gpui_kit::App,
) {
    let (_, page) = page(window, doc);
    window.click_at("canvas-hitbox", local_point(page, doc, at.0, at.1), cx);
    window.render_frame(cx);
}

fn history_version(content: &Entity<App>, cx: &mut gpui_kit::App) -> u64 {
    content
        .read(cx)
        .active_core_history_version_for_test()
        .expect("fixture has a history version")
}

fn dispatch_pointer_event(
    window: &mut gpui_kit::Window,
    event: PlatformInput,
    cx: &mut gpui_kit::App,
) {
    let _ = window.dispatch_event(event, cx);
}

pub(super) fn tab_to_canvas(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
    window.focus_next(cx);
    window.render_frame(cx);
    for _ in 0..64 {
        window.press("tab", cx);
        if window.find("canvas-hitbox").focused() == Some(true) {
            return;
        }
    }
    panic!("Tab traversal did not focus the canvas");
}

fn assert_pending_escape_left_core_unchanged(
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
    before_pixels: &[u8],
    before_layers: usize,
    before_transform: Option<(f64, f64)>,
    before_history: u64,
) {
    assert_eq!(pixels(content, cx).2.as_slice(), before_pixels);
    assert_eq!(
        content
            .read(cx)
            .active_core_layer_count_for_test()
            .expect("fixture has a layer count"),
        before_layers
    );
    assert_eq!(
        content.read(cx).active_core_transform_for_test(),
        before_transform
    );
    assert_eq!(history_version(content, cx), before_history);
    assert!(content.read(cx).active_core_selection_for_test().is_none());
    content.update(cx, |app, cx| {
        assert!(app.state_for_test().read(cx).crop_rect.is_none());
    });
}

#[gpui_kit::test]
fn brush_drag_mutates_core_pixels_and_history(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let doc = {
            let (w, h, _) = pixels(&content, cx);
            (w, h)
        };
        let (_, _, before) = pixels(&content, cx);
        let version_before = history_version(&content, cx);
        drag_doc(window, doc, (0.5, 0.5), (1.5, 1.5), cx);
        let (_, _, after) = pixels(&content, cx);
        assert_ne!(after, before, "the drag must change canonical pixels");
        assert!(history_version(&content, cx) > version_before);
        content.update(cx, |app, _| {
            assert_eq!(
                app.active_document_save_state_for_test(),
                Some((1, 0, true))
            );
        });
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input::GesturePhase::Idle
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn eraser_drag_changes_pixels_after_brush(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 8.0);
        window.render_frame(cx);
        let doc = {
            let (w, h, _) = pixels(&content, cx);
            (w, h)
        };
        drag_doc(window, doc, (0.5, 0.5), (2.0, 2.0), cx);
        let (_, _, painted) = pixels(&content, cx);
        set_tool(&content, cx, ToolId::Eraser, 8.0);
        drag_doc(window, doc, (0.5, 0.5), (2.0, 2.0), cx);
        let (_, _, erased) = pixels(&content, cx);
        assert_ne!(erased, painted, "the eraser must change the painted pixels");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn paint_bucket_click_fills_with_the_foreground_color(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.fg_color = [0, 255, 0, 255];
            });
        });
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        click_doc(window, (w, h), (0.5, 0.5), cx);
        let (_, _, after) = pixels(&content, cx);
        assert_ne!(after, before);
        assert!(
            after.chunks_exact(4).any(|p| p == [0, 255, 0, 255]),
            "the fill must write the foreground colour"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn gradient_drag_changes_pixels(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Gradient, 1.0);
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (w as f32 - 1.5, 0.5), cx);
        let (_, _, after) = pixels(&content, cx);
        assert_ne!(after, before, "the gradient must write pixels");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn selection_drag_sets_core_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (2.5, 2.5), cx);
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("the drag must set core selection");
        assert!(selection.width >= 1.0 && selection.height >= 1.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn move_drag_translates_the_core_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        // A 2x1 page at the test window's zoom is smaller than the 16px handle
        // box, so its centre reports a corner handle. Move needs a layer whose
        // interior is clear of the handles.
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        let before = content
            .read(cx)
            .active_core_transform_for_test()
            .expect("raster transform");
        drag_doc(window, (400, 300), (200.0, 150.0), (260.0, 190.0), cx);
        let after = content
            .read(cx)
            .active_core_transform_for_test()
            .expect("raster transform");
        assert_ne!(after, before, "the move drag must update the transform");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shape_drag_adds_a_shape_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let before = content.read(cx).active_core_layer_count_for_test().unwrap();
        drag_doc(window, (w, h), (0.5, 0.5), (2.5, 2.5), cx);
        let status = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .status_error
            .clone();
        assert!(status.is_none(), "shape status: {status:?}");
        let types = content.read(cx).active_core_layer_types_for_test().unwrap();
        assert_eq!(types.len(), before + 1);
        assert!(types.iter().any(|t| matches!(t, LayerType::Shape)));
        let origin = content
            .read(cx)
            .active_core_layer_origin_for_test(LayerType::Shape)
            .expect("shape layer has an origin");
        assert!(
            (origin.0 - 0.5).abs() < 0.01 && origin.1.abs() < 0.01,
            "the shape must land at the drag origin (the 2x1 fixture clamps y), got {origin:?}"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_click_adds_a_text_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let before = content.read(cx).active_core_layer_count_for_test().unwrap();
        click_doc(window, (w, h), (0.5, 0.5), cx);
        let status = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .status_error
            .clone();
        assert!(status.is_none(), "text status: {status:?}");
        let types = content.read(cx).active_core_layer_types_for_test().unwrap();
        assert_eq!(types.len(), before + 1);
        assert!(types.iter().any(|t| matches!(t, LayerType::Text)));
        let origin = content
            .read(cx)
            .active_core_layer_origin_for_test(LayerType::Text)
            .expect("text layer has an origin");
        assert!(
            (origin.0 - 0.5).abs() < 0.01 && origin.1.abs() < 0.01,
            "the text layer must land at the click point (the 2x1 fixture clamps y), got {origin:?}"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_drag_sets_the_pending_crop_rect(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let before = content.read(cx).active_core_layer_count_for_test().unwrap();
        drag_doc(window, (w, h), (0.5, 0.5), (2.5, 2.5), cx);
        content.update(cx, |app, cx| {
            let rect = app.state_for_test().read(cx).crop_rect;
            assert!(rect.is_some_and(|r| r[2] >= 1.0 && r[3] >= 1.0));
        });
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test().unwrap(),
            before,
            "the crop pointer only sets the pending rectangle"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn eyedropper_samples_the_non_first_pixel_without_mutation(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Eyedropper, 1.0);
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        assert_eq!((w, h), (2, 1), "fixture must contain two document pixels");
        let first: [u8; 4] = before[0..4].try_into().expect("first RGBA pixel");
        let expected: [u8; 4] = before[4..8].try_into().expect("second RGBA pixel");
        assert_ne!(first, expected, "fixture pixels must be distinct");
        let version_before = history_version(&content, cx);
        click_doc(window, (w, h), (1.5, 0.5), cx);
        content.update(cx, |app, cx| {
            assert_eq!(app.state_for_test().read(cx).fg_color, expected);
        });
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "sampling must not mutate pixels"
        );
        assert_eq!(history_version(&content, cx), version_before);
    })
    .expect("test window remains open");
}

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

#[gpui_kit::test]
fn escape_cancels_each_pending_tool_without_applying_it(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);

        for tool in [
            ToolId::Move,
            ToolId::Selection,
            ToolId::Crop,
            ToolId::Shape,
            ToolId::Gradient,
            ToolId::Text,
        ] {
            set_tool(&content, cx, tool, 2.0);
            window.render_frame(cx);
            let before_pixels = pixels(&content, cx).2;
            let before_layers = content
                .read(cx)
                .active_core_layer_count_for_test()
                .expect("fixture has a layer count");
            let before_transform = content.read(cx).active_core_transform_for_test();
            let before_history = history_version(&content, cx);
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
            content.update(cx, |app, _| {
                assert!(
                    app.pending_gesture.is_some() || app.transform_drag_for_test().is_some(),
                    "{tool:?} must arm a gesture"
                );
            });
            assert_pending_escape_left_core_unchanged(
                &content,
                cx,
                &before_pixels,
                before_layers,
                before_transform,
                before_history,
            );

            window.press("escape", cx);
            window.render_frame(cx);
            content.update(cx, |app, _| {
                assert!(
                    app.pending_gesture.is_none() && app.transform_drag_for_test().is_none(),
                    "{tool:?} Escape must clear the armed gesture"
                );
            });
            assert_eq!(
                content.read(cx).input_phase_for_test(),
                crate::input::GesturePhase::Idle
            );
            assert_pending_escape_left_core_unchanged(
                &content,
                cx,
                &before_pixels,
                before_layers,
                before_transform,
                before_history,
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
            assert_pending_escape_left_core_unchanged(
                &content,
                cx,
                &before_pixels,
                before_layers,
                before_transform,
                before_history,
            );
        }
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn zero_size_selection_and_crop_normalize_to_one_pixel(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);

        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        click_doc(window, doc, (0.5, 0.5), cx);
        let selection = content
            .read(cx)
            .active_core_selection_for_test()
            .expect("a zero-size click still sets a selection");
        assert!(
            selection.width >= 1.0 && selection.height >= 1.0,
            "zero-size selection must normalize to one pixel"
        );

        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        click_doc(window, doc, (0.5, 0.5), cx);
        content.update(cx, |app, cx| {
            let rect = app
                .state_for_test()
                .read(cx)
                .crop_rect
                .expect("a zero-size click still sets a crop rect");
            assert!(
                rect[2] >= 1.0 && rect[3] >= 1.0,
                "zero-size crop must normalize to one pixel"
            );
        });
        assert_eq!(
            content
                .read(cx)
                .active_core_layer_count_for_test()
                .expect("fixture has a layer count"),
            1,
            "the crop pointer only sets the pending rectangle"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn click_outside_the_page_does_not_mutate(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        let doc = (w, h);
        let history_before = history_version(&content, cx);
        let (_, placement) = page(window, doc);

        // Above the page, still inside the hitbox for the wide test fixture.
        window.click_at("canvas-hitbox", local_point(placement, doc, 0.5, -0.1), cx);
        window.render_frame(cx);
        assert_eq!(
            pixels(&content, cx).2,
            before,
            "a click outside the page must not paint"
        );
        assert_eq!(
            history_version(&content, cx),
            history_before,
            "a click outside the page must not add history"
        );
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input::GesturePhase::Idle
        );
    })
    .expect("test window remains open");
}

pub(super) fn history_cursor(content: &Entity<App>, cx: &mut gpui_kit::App) -> usize {
    content
        .read(cx)
        .active_core_history_cursor_for_test()
        .expect("fixture has a history cursor")
}

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
            crate::input::GesturePhase::Idle
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
            crate::input::GesturePhase::Action
        );

        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input::GesturePhase::Idle,
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
            crate::input::GesturePhase::Action
        );
        content.update(cx, |app, cx| {
            // The window-deactivation observer runs the same pair.
            app.release_canvas_pointer(window);
            app.cancel_canvas_gesture(cx);
        });
        assert!(window.captured_hitbox().is_none());
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input::GesturePhase::Idle
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
    // G9 decision: no app-side event accumulator, so both a single dispatch
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

#[gpui_kit::test]
fn all_ten_tools_round_trip_in_both_directions(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        // `g` selects the bucket and `Shift+G` the gradient, matching the
        // frozen canvas handler; the ring can therefore run in reverse.
        let ring = [
            ("v", ToolId::Move),
            ("m", ToolId::Selection),
            ("c", ToolId::Crop),
            ("i", ToolId::Eyedropper),
            ("b", ToolId::Brush),
            ("e", ToolId::Eraser),
            ("g", ToolId::PaintBucket),
            ("shift-g", ToolId::Gradient),
            ("u", ToolId::Shape),
            ("t", ToolId::Text),
        ];
        for (key, tool) in ring {
            window.press(key, cx);
            content.update(cx, |app, cx| {
                assert_eq!(app.state_for_test().read(cx).active_tool, tool, "key {key}");
                assert_eq!(
                    app.input_phase_for_test(),
                    crate::input::GesturePhase::Idle,
                    "key {key} must not leave a gesture"
                );
            });
        }
        let reverse = [
            ("t", ToolId::Text),
            ("u", ToolId::Shape),
            ("shift-g", ToolId::Gradient),
            ("g", ToolId::PaintBucket),
            ("e", ToolId::Eraser),
            ("b", ToolId::Brush),
            ("i", ToolId::Eyedropper),
            ("c", ToolId::Crop),
            ("m", ToolId::Selection),
            ("v", ToolId::Move),
        ];
        for (key, tool) in reverse {
            window.press(key, cx);
            content.update(cx, |app, cx| {
                assert_eq!(app.state_for_test().read(cx).active_tool, tool, "key {key}");
            });
        }
        // A->B->A for every adjacent pair has now run twice; the canvas still
        // paints after the round trip.
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Brush;
                state.brush_size = 2.0;
            });
        });
        let (w, h, before) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (1.5, 0.5), cx);
        assert_ne!(
            pixels(&content, cx).2,
            before,
            "painting must still work after the full tool round trip"
        );
    })
    .expect("test window remains open");
}

pub(super) fn mount_archive_app(
    cx: &mut gpui_kit::TestAppContext,
    directory: &TestDirectory,
    bytes: Vec<u8>,
) -> (
    gpui_kit::WindowHandle<gpui_kit::base::Root>,
    Entity<App>,
    std::path::PathBuf,
) {
    let source = directory.path().join("archive.ptz");
    std::fs::write(&source, bytes).expect("archive fixture is written");
    let file_io = Arc::new(
        LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("local file boundary opens"),
    );
    file_io.approve_path(&source).expect("fixture is approved");
    let (window, content) = mount_app(cx, Some(source.clone()), Some(file_io));
    (window, content, source)
}

pub(super) fn layer_rows(
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
) -> (Vec<crate::state::LayerRow>, u64) {
    let state = content.read(cx).state_for_test().read(cx);
    (state.layers.clone(), state.selected_layer)
}

pub(super) fn select_layer(content: &Entity<App>, cx: &mut gpui_kit::App, row_id: u64) {
    content.update(cx, |app, cx| {
        app.state_for_test()
            .update(cx, |state, _| state.selected_layer = row_id);
    });
}

#[gpui_kit::test]
fn paint_targets_the_selected_layer_not_the_first_raster(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (rows, selected) = layer_rows(&content, cx);
        assert_eq!(rows.len(), 2, "the panel must project both core layers");
        assert_eq!(rows[0].name, "top");
        assert_eq!(rows[0].core_id.as_deref(), Some("top"));
        assert_eq!(rows[1].name, "bottom");
        assert_eq!(rows[1].core_id.as_deref(), Some("bottom"));
        assert_eq!(selected, rows[0].id, "projection selects the first row");

        let (_, _, top_before) = content
            .read(cx)
            .core_layer_pixels_for_test("top")
            .expect("top layer has pixels");
        let (_, _, bottom_before) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .expect("bottom layer has pixels");
        let cursor_before = history_cursor(&content, cx);

        select_layer(&content, cx, rows[1].id);
        drag_doc(window, (2, 1), (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, top_after) = content.read(cx).core_layer_pixels_for_test("top").unwrap();
        let (_, _, bottom_after) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .unwrap();
        assert_eq!(
            top_after, top_before,
            "the unselected top layer must not change"
        );
        assert_ne!(
            bottom_after, bottom_before,
            "the selected bottom layer must paint"
        );
        assert_eq!(history_cursor(&content, cx), cursor_before + 1);

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        let (_, _, bottom_undone) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .unwrap();
        assert_eq!(
            bottom_undone, bottom_before,
            "undo restores the selected layer"
        );

        select_layer(&content, cx, rows[0].id);
        drag_doc(window, (2, 1), (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, top_after_top) = content.read(cx).core_layer_pixels_for_test("top").unwrap();
        let (_, _, bottom_after_top) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .unwrap();
        assert_ne!(top_after_top, top_before, "selecting top paints the top");
        assert_eq!(
            bottom_after_top, bottom_before,
            "the deselected bottom layer must not change"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn locked_selected_layer_refuses_brush_and_bucket(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(true),
    );
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (rows, selected) = layer_rows(&content, cx);
        assert_eq!(selected, rows[0].id, "the locked top row is selected");
        let (_, _, top_before) = content.read(cx).core_layer_pixels_for_test("top").unwrap();
        let (_, _, bottom_before) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .unwrap();
        let cursor_before = history_cursor(&content, cx);
        drag_doc(window, (2, 1), (0.5, 0.5), (1.5, 0.5), cx);
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        click_doc(window, (2, 1), (0.5, 0.5), cx);
        let (_, _, top_after) = content.read(cx).core_layer_pixels_for_test("top").unwrap();
        let (_, _, bottom_after) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .unwrap();
        assert_eq!(top_after, top_before, "a locked layer must refuse paint");
        assert_eq!(bottom_after, bottom_before);
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "a refused paint must add no history"
        );
        assert_eq!(
            content.read(cx).input_phase_for_test(),
            crate::input::GesturePhase::Idle
        );

        select_layer(&content, cx, rows[1].id);
        set_tool(&content, cx, ToolId::Brush, 2.0);
        drag_doc(window, (2, 1), (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, bottom_unlocked) = content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .unwrap();
        assert_ne!(
            bottom_unlocked, bottom_before,
            "an unlocked selected layer still paints"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn non_raster_selected_layer_refuses_paint(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        drag_doc(window, doc, (0.2, 0.2), (1.8, 0.8), cx);
        let (rows, _) = layer_rows(&content, cx);
        assert_eq!(rows.len(), 2, "the shape layer projects into the rows");
        assert!(
            rows[0]
                .core_id
                .as_deref()
                .unwrap_or("")
                .starts_with("native-layer-"),
            "the shape is the top row"
        );

        set_tool(&content, cx, ToolId::Brush, 2.0);
        select_layer(&content, cx, rows[0].id);
        let cursor_before = history_cursor(&content, cx);
        let raster_id = rows[1].core_id.as_deref().unwrap();
        let (_, _, raster_before) = content
            .read(cx)
            .core_layer_pixels_for_test(raster_id)
            .unwrap();
        drag_doc(window, doc, (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, raster_after) = content
            .read(cx)
            .core_layer_pixels_for_test(raster_id)
            .unwrap();
        assert_eq!(
            raster_after, raster_before,
            "painting a shape layer must not fall through to the raster below"
        );
        assert_eq!(history_cursor(&content, cx), cursor_before);

        select_layer(&content, cx, rows[1].id);
        drag_doc(window, doc, (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, raster_selected) = content
            .read(cx)
            .core_layer_pixels_for_test(raster_id)
            .unwrap();
        assert_ne!(
            raster_selected, raster_before,
            "selecting the raster row restores painting"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn layer_drag_reorders_core_and_undo_restores(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test(),
            vec!["top", "bottom"]
        );
        window.drag_to("layer-row-1", "layer-row-0", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test(),
            vec!["bottom", "top"],
            "the drag must reorder the canonical layers"
        );
        let (rows, selected) = layer_rows(&content, cx);
        assert_eq!(rows[0].name, "bottom");
        assert_eq!(selected, rows[0].id, "the dragged layer must stay selected");

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test(),
            vec!["top", "bottom"],
            "undo must restore the canonical order"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn window_close_prompts_when_dirty_and_cancel_keeps_the_document(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (1.5, 0.5), cx);
        assert!(
            content
                .read(cx)
                .active_document_save_state_for_test()
                .expect("fixture has a session")
                .2
        );
    })
    .expect("test window remains open");

    let mut visual = gpui_kit::VisualTestContext::from_window(window.into(), cx);
    assert!(
        !visual.simulate_close(),
        "a dirty close must be interrupted by the prompt"
    );
    visual.update(|window, cx| {
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::DirtyClose)
        );
        window.render_frame(cx);
        window.click("dirty-close-cancel", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            None
        );
        assert!(
            content
                .read(cx)
                .active_document_save_state_for_test()
                .expect("the document stays open")
                .2,
            "cancel must keep the document dirty"
        );
    });
}

#[gpui_kit::test]
fn window_close_discard_closes_the_document_and_drops_the_prompt(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (1.5, 0.5), cx);
    })
    .expect("test window remains open");

    let mut visual = gpui_kit::VisualTestContext::from_window(window.into(), cx);
    assert!(!visual.simulate_close(), "the dirty prompt must open");
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click("dirty-close-discard", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert!(!state.has_document, "discard closes the document");
        assert_eq!(state.active_dialog, None);
    });
}

#[gpui_kit::test]
fn duplicate_selected_layer_copies_pixels_and_undoes(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (_, _, before_pixels) = content.read(cx).core_layer_pixels_for_test("top").unwrap();
        content.update(cx, |app, cx| {
            app.duplicate_selected_layer(cx)
                .expect("duplicate succeeds");
        });
        let ids = content.read(cx).active_core_layer_ids_for_test();
        assert_eq!(ids.len(), 3, "the duplicate must be a new layer");
        let duplicate_id = ids
            .iter()
            .find(|id| id.starts_with("native-layer-"))
            .expect("the duplicate has a generated id")
            .clone();
        let (_, _, pixels) = content
            .read(cx)
            .core_layer_pixels_for_test(&duplicate_id)
            .expect("the duplicate has pixels");
        assert_eq!(pixels, before_pixels, "the duplicate copies the pixels");
        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test().len(),
            2,
            "undo removes the duplicate"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn merge_down_composites_pixels_and_undoes(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.merge_selected_layer_down(cx)
                .expect("merge down succeeds");
        });
        let ids = content.read(cx).active_core_layer_ids_for_test();
        assert_eq!(ids.len(), 1, "the pair becomes one layer");
        let (width, height, pixels) = content
            .read(cx)
            .core_layer_pixels_for_test(&ids[0])
            .expect("the merged layer has pixels");
        assert_eq!((width, height), (2, 1));
        assert_eq!(&pixels[0..4], &[0, 0, 255, 255], "top wins at (0,0)");
        assert_eq!(&pixels[4..8], &[255, 0, 0, 255], "bottom remains at (1,0)");

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        let ids = content.read(cx).active_core_layer_ids_for_test();
        assert_eq!(ids, vec!["top", "bottom"], "undo restores the pair");
        assert!(content.read(cx).core_layer_pixels_for_test("top").is_some());
        assert!(content
            .read(cx)
            .core_layer_pixels_for_test("bottom")
            .is_some());

        content.update(cx, |app, cx| {
            app.step_history(false, cx).expect("redo succeeds");
        });
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test().len(),
            1,
            "redo restores the merged layer"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn flatten_composites_pixels_and_undoes(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.flatten_document(cx).expect("flatten succeeds");
        });
        let ids = content.read(cx).active_core_layer_ids_for_test();
        assert_eq!(ids.len(), 1, "flatten leaves one layer");
        let (_, _, pixels) = content
            .read(cx)
            .core_layer_pixels_for_test(&ids[0])
            .expect("the flattened layer has pixels");
        assert_eq!(&pixels[0..4], &[0, 0, 255, 255]);
        assert_eq!(&pixels[4..8], &[255, 0, 0, 255]);
        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test(),
            vec!["top", "bottom"],
            "undo restores the flattened layers"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn history_panel_lists_and_jumps_core_entries(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (w, h, blank) = pixels(&content, cx);
        let doc = (w, h);
        drag_doc(window, doc, (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, first) = pixels(&content, cx);
        drag_doc(window, doc, (0.5, 0.5), (1.5, 1.0), cx);
        let (_, _, second) = pixels(&content, cx);
        assert_ne!(first, second);

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.layer_tab = crate::state::LayerTab::History
            });
        });
        window.render_frame(cx);
        let rows = content.read(cx).state_for_test().read(cx).history.clone();
        assert_eq!(rows.len(), 3, "Open plus two strokes");
        assert_eq!(rows[0].label, "Open");
        assert_eq!(rows[1].label, "Brush");
        assert_eq!(rows[2].label, "Brush");
        assert!(rows[2].current, "the newest entry is current");

        window.click("history-row-1", cx);
        window.render_frame(cx);
        assert_eq!(
            pixels(&content, cx).2,
            first,
            "clicking the first stroke restores its pixels"
        );
        assert!(content
            .read(cx)
            .state_for_test()
            .read(cx)
            .history
            .iter()
            .find(|row| row.current)
            .is_some_and(|row| row.label != "Open"));

        window.click("history-row-2", cx);
        window.render_frame(cx);
        assert_eq!(pixels(&content, cx).2, second);

        window.click("history-row-0", cx);
        window.render_frame(cx);
        assert_eq!(
            pixels(&content, cx).2,
            blank,
            "the Open row reverts every entry"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn layer_shortcuts_duplicate_merge_and_flatten(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("ctrl-j", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test().len(),
            3,
            "Ctrl+J duplicates the selected layer"
        );
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(content.read(cx).active_core_layer_ids_for_test().len(), 2);

        window.press("ctrl-e", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test().len(),
            1,
            "Ctrl+E merges the selected layer down"
        );
        window.press("ctrl-z", cx);
        window.render_frame(cx);

        window.press("ctrl-shift-e", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test().len(),
            1,
            "Ctrl+Shift+E flattens the document"
        );
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_ids_for_test(),
            vec!["top", "bottom"],
            "undo restores the flattened document"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn save_after_move_persists_the_live_transform(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    let moved = cx
        .update_window(window.into(), |_, window, cx| {
            content.update(cx, |app, cx| {
                app.create_document(400, 300, cx).expect("blank document");
            });
            set_tool(&content, cx, ToolId::Move, 2.0);
            window.render_frame(cx);
            let before = content
                .read(cx)
                .active_core_transform_for_test()
                .expect("fixture has a transform");
            drag_doc(window, (400, 300), (200.0, 150.0), (240.0, 150.0), cx);
            let moved = content
                .read(cx)
                .active_core_transform_for_test()
                .expect("fixture has a transform");
            assert_ne!(before, moved, "the move must change the transform");
            content
                .update(cx, |app, cx| app.save_to_path_for_test(&source, cx))
                .expect("the moved document saves");
            content.update(cx, |app, _cx| {
                let (saved, current, dirty) = app
                    .active_document_save_state_for_test()
                    .expect("the moved document has a save state");
                assert_eq!(
                    (saved, dirty),
                    (current, false),
                    "a metadata edit must save and clear dirty"
                );
            });
            moved
        })
        .expect("test window remains open");

    let file_io = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
        .expect("local file boundary opens");
    file_io.approve_path(&source).expect("saved path approved");
    let loaded = crate::services::document::load_document_candidate(&file_io, &source)
        .expect("the saved document reloads");
    let saved = loaded.content_snapshot().expect("saved content is present");
    assert_eq!(
        (saved.layers[0].transform.x, saved.layers[0].transform.y),
        moved,
        "the saved file must carry the live transform"
    );
}

#[gpui_kit::test]
fn alt_click_with_the_brush_samples_instead_of_painting(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        let history = history_version(&content, cx);
        let (bounds, placement) = page(window, (w, h));
        let start = window_point(bounds, placement, (w, h), 0.5, 0.5);
        let end = window_point(bounds, placement, (w, h), 1.5, 0.5);
        let modifiers = gpui_kit::Modifiers {
            alt: true,
            ..Default::default()
        };
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                modifiers,
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        assert_eq!(
            pixels(&content, cx).2,
            before,
            "Alt sampling must not paint a stroke"
        );
        assert_eq!(
            history_version(&content, cx),
            history,
            "Alt sampling must not add history"
        );
        let sampled = content
            .read(cx)
            .active_composite_pixel_for_test(1, 0)
            .expect("composite pixel under the pointer");
        let fg = content.read(cx).state_for_test().read(cx).fg_color;
        assert_eq!(fg, sampled, "the sampled colour must become the foreground");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shift_locks_a_brush_stroke_to_the_dominant_axis(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(100, 100, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        let (width, _, before) = pixels(&content, cx);

        let (bounds, placement) = page(window, (100, 100));
        let start = window_point(bounds, placement, (100, 100), 20.0, 50.0);
        let middle = window_point(bounds, placement, (100, 100), 80.0, 58.0);
        let end = window_point(bounds, placement, (100, 100), 90.0, 60.0);
        let modifiers = gpui_kit::Modifiers {
            shift: true,
            ..Default::default()
        };
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        for position in [middle, end] {
            dispatch_pointer_event(
                window,
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    pressed_button: Some(MouseButton::Left),
                    modifiers,
                }),
                cx,
            );
        }
        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        let (after_width, _, after) = pixels(&content, cx);
        assert_eq!(after_width, width);
        let pixel = |rgba: &[u8], x: usize, y: usize| {
            let at = (y * width as usize + x) * 4;
            rgba[at..at + 4].to_vec()
        };
        assert_ne!(
            pixel(&after, 70, 50),
            pixel(&before, 70, 50),
            "the stroke must paint along the locked row"
        );
        assert_eq!(
            pixel(&after, 70, 58),
            pixel(&before, 70, 58),
            "the off-axis pointer path must not paint"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn alt_right_drag_adjusts_the_brush_size_and_hardness(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 10.0);
        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.brush_hardness = 0.5);
        });
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        let history = history_version(&content, cx);
        let (bounds, placement) = page(window, (w, h));
        let start = window_point(bounds, placement, (w, h), 0.5, 0.5);
        let end = start + point(px(200.0), px(-125.0));
        let modifiers = gpui_kit::Modifiers {
            alt: true,
            ..Default::default()
        };
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Right,
                position: start,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Right),
                modifiers,
            }),
            cx,
        );
        {
            let state = content.read(cx).state_for_test().read(cx);
            let hud = state.hud.expect("brush adjust HUD");
            assert_eq!(hud.mode, crate::state::HudMode::Brush);
            assert_eq!(hud.a, 20.0);
            assert!((hud.b - 100.0).abs() < 1e-6);
        }
        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Right,
                position: end,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        {
            let state = content.read(cx).state_for_test().read(cx);
            assert!(state.hud.is_none(), "release clears the brush HUD");
            assert_eq!(state.brush_size, 20.0, "200px right doubles the size");
            assert!(
                (state.brush_hardness - 1.0).abs() < 1e-6,
                "125px up adds 0.5 hardness"
            );
            assert!(
                state.context_menu.is_none(),
                "no context menu during adjust"
            );
        }
        assert_eq!(pixels(&content, cx).2, before, "adjusting must not paint");
        assert_eq!(
            history_version(&content, cx),
            history,
            "adjusting must not add history"
        );
    })
    .expect("test window remains open");
}
