use super::input::tab_to_canvas;
use super::*;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    point, px, Bounds, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PlatformInput, Point, ScrollDelta, ScrollWheelEvent,
};
use photrez_core::ptz::content::{SelectionShape, SelectionState};

use crate::render::canvas_bridge::Camera;

fn set_camera(content: &Entity<App>, cx: &mut gpui_kit::App, camera: Camera) {
    content.update(cx, |app, cx| app.set_camera(camera, cx));
}

fn camera(content: &Entity<App>, cx: &mut gpui_kit::App) -> Camera {
    content
        .read(cx)
        .camera_for_test()
        .expect("the test set a concrete camera")
}

fn canvas_bounds(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
) -> Bounds<gpui_kit::Pixels> {
    window.render_frame(cx);
    window.find("canvas-hitbox").bounds()
}

fn centre(bounds: Bounds<gpui_kit::Pixels>) -> Point<gpui_kit::Pixels> {
    point(
        px(f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.),
        px(f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.),
    )
}

fn dispatch(window: &mut gpui_kit::Window, event: PlatformInput, cx: &mut gpui_kit::App) {
    let _ = window.dispatch_event(event, cx);
    window.render_frame(cx);
}

#[gpui_kit::test]
fn ctrl_wheel_zooms_around_the_cursor_and_plain_wheel_pans(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_camera(&content, cx, Camera::new(100., 100., 1.).unwrap());
        let bounds = canvas_bounds(window, cx);
        let anchor = point(
            px(f32::from(bounds.origin.x) + 101.),
            px(f32::from(bounds.origin.y) + 100.),
        );
        let before = camera(&content, cx);
        let doc_x = (101. - before.x) / before.zoom;
        let doc_y = (100. - before.y) / before.zoom;

        dispatch(
            window,
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position: anchor,
                delta: ScrollDelta::Pixels(point(px(0.), px(-1.))),
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            cx,
        );
        let after = camera(&content, cx);
        assert!((after.zoom - 1.25).abs() < 1e-5, "zoom={}", after.zoom);
        let after_doc_x = (101. - after.x) / after.zoom;
        let after_doc_y = (100. - after.y) / after.zoom;
        assert!((after_doc_x - doc_x).abs() < 1e-4, "anchor X moved");
        assert!((after_doc_y - doc_y).abs() < 1e-4, "anchor Y moved");

        dispatch(
            window,
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position: anchor,
                delta: ScrollDelta::Pixels(point(px(0.), px(40.))),
                ..Default::default()
            }),
            cx,
        );
        let panned = camera(&content, cx);
        assert!((panned.x - after.x).abs() < 1e-4);
        assert!(
            (panned.y - (after.y - 40.)).abs() < 1e-4,
            "plain wheel must pan by -delta: {} vs {}",
            panned.y,
            after.y - 40.
        );
        assert!((panned.zoom - after.zoom).abs() < 1e-6);
    })
    .expect("test window remains open");
}

/// Zoom shortcut -> status bar readout refresh (press + one render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn status_zoom_readout_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        tab_to_canvas(window, cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        let mut last: Option<f32> = None;
        let control = gpui_kit::Modifiers {
            control: true,
            ..Default::default()
        };
        for i in 0..9 {
            let key = if i % 2 == 0 { "=" } else { "-" };
            // Dispatch the binding without gpui-kit's press helper: it renders
            // a frame before and after the key, so timing it would charge
            // three frames to one event. This measures event -> visible frame.
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
                    keystroke: gpui_kit::Keystroke {
                        modifiers: control,
                        key: key.to_string(),
                        key_char: None,
                    },
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            let zoom = camera(&content, cx).zoom;
            if let Some(previous) = last {
                assert_ne!(zoom, previous, "the zoom shortcut must move the readout");
            }
            last = Some(zoom);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S054_STATUS_ZOOM_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "status readout refresh p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn zoom_shortcuts_fit_actual_size_and_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let bounds = canvas_bounds(window, cx);
        let viewport_w: f32 = bounds.size.width.into();
        let viewport_h: f32 = bounds.size.height.into();
        // The 2x1 fixture fits at the 10x ceiling.
        tab_to_canvas(window, cx);
        window.press("ctrl-=", cx);
        window.render_frame(cx);
        assert!(
            (camera(&content, cx).zoom - 12.5).abs() < 1e-4,
            "ctrl+= must zoom the fitted view by 1.25"
        );
        window.press("ctrl--", cx);
        window.render_frame(cx);
        assert!((camera(&content, cx).zoom - 10.).abs() < 1e-4);

        // Both "-" and "_" zoom out; "_" is
        // Ctrl+Shift+- on a US layout.
        window.press("ctrl-shift--", cx);
        window.render_frame(cx);
        assert!(
            (camera(&content, cx).zoom - 8.).abs() < 1e-4,
            "Ctrl+Shift+- (_) must zoom out like Ctrl+-"
        );
        window.press("ctrl-=", cx);
        window.render_frame(cx);
        assert!((camera(&content, cx).zoom - 10.).abs() < 1e-4);

        window.press("ctrl-1", cx);
        window.render_frame(cx);
        assert!(
            (camera(&content, cx).zoom - 1.).abs() < 1e-6,
            "actual size is 100%"
        );

        window.press("ctrl-=", cx);
        window.press("ctrl-0", cx);
        window.render_frame(cx);
        assert!(
            (camera(&content, cx).zoom - 10.).abs() < 1e-4,
            "Ctrl+0 must return to the fit zoom"
        );

        content.update(cx, |app, cx| {
            app.set_core_selection_for_test(
                SelectionState {
                    x: 0.,
                    y: 0.,
                    width: 1.,
                    height: 1.,
                    angle: 0.,
                    shape: Some(SelectionShape::Rect),
                    inverted: None,
                },
                cx,
            )
            .expect("selection is accepted");
        });
        window.press("ctrl-alt-0", cx);
        window.render_frame(cx);
        let zoomed = camera(&content, cx);
        assert!((zoomed.zoom - 10.).abs() < 1e-4, "zoom={}", zoomed.zoom);
        assert!((zoomed.x - (viewport_w - 10.) / 2.).abs() < 1e-3);
        assert!((zoomed.y - (viewport_h - 10.) / 2.).abs() < 1e-3);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn middle_drag_and_space_drag_pan_the_camera(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_camera(&content, cx, Camera::new(0., 0., 1.).unwrap());
        let bounds = canvas_bounds(window, cx);
        let start = centre(bounds);
        let end = point(px(f32::from(start.x) + 15.), px(f32::from(start.y) + 25.));
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Middle,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Middle,
                position: end,
                ..Default::default()
            }),
            cx,
        );
        let panned = camera(&content, cx);
        assert!((panned.x - 15.).abs() < 1e-3, "x={}", panned.x);
        assert!((panned.y - 25.).abs() < 1e-3, "y={}", panned.y);
        assert_eq!(
            content.read(cx).session_viewport_for_test(),
            Some((15., 25., 1.))
        );

        // Space latches the pan cursor and pans a left drag instead of the tool.
        tab_to_canvas(window, cx);
        dispatch(
            window,
            PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
                keystroke: gpui_kit::Keystroke {
                    modifiers: Modifiers::default(),
                    key: "space".to_string(),
                    key_char: None,
                },
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
        let start = centre(bounds);
        let end = point(px(f32::from(start.x) + 5.), px(f32::from(start.y) + 7.));
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
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
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
        let space_panned = camera(&content, cx);
        assert!((space_panned.x - 20.).abs() < 1e-3, "x={}", space_panned.x);
        assert!((space_panned.y - 32.).abs() < 1e-3, "y={}", space_panned.y);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn navigator_click_centres_the_camera_on_the_clicked_document_point(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_camera(&content, cx, Camera::new(0., 0., 1.).unwrap());
        let canvas = canvas_bounds(window, cx);
        let viewport_w: f32 = canvas.size.width.into();
        let viewport_h: f32 = canvas.size.height.into();
        // 2x1 document: scale 88, thumb 176x88, offset (16,0), pad (16,16).
        window.click_at(
            "navigator-hitbox",
            point(px(16. + 16. + 88.), px(16. + 44.)),
            cx,
        );
        window.render_frame(cx);
        let centred = camera(&content, cx);
        assert!((centred.x - (viewport_w / 2. - 1.)).abs() < 1e-3);
        assert!((centred.y - (viewport_h / 2. - 0.5)).abs() < 1e-3);
        assert!((centred.zoom - 1.).abs() < 1e-6);
    })
    .expect("test window remains open");
}

/// Navigator click recentre latency (dispatch + one render).
/// Measured explicitly: concurrent suite load inflates this click path.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn navigator_click_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_camera(&content, cx, Camera::new(0., 0., 1.).unwrap());
        window.render_frame(cx);
        let origin = window.find("navigator-hitbox").bounds().origin;
        let position = origin + point(px(16. + 16. + 88.), px(16. + 44.));
        let mut samples = Vec::new();
        for _ in 0..9 {
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            let centred = camera(&content, cx);
            assert!((centred.zoom - 1.).abs() < 1e-6, "the navigator keeps zoom");
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S029_NAVIGATOR_CLICK_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "navigator click p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn edge_proximity_pans_the_camera_during_a_paint_drag(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(800, 600, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = crate::state::ToolId::Brush
            });
            app.set_camera(Camera::new(10., 10., 1.).unwrap(), cx);
        });
        window.render_frame(cx);
        let bounds = canvas_bounds(window, cx);
        let start = point(
            px(f32::from(bounds.origin.x) + 10. + 20.),
            px(f32::from(bounds.origin.y) + 10. + 20.),
        );
        let near_right = point(
            px(f32::from(bounds.origin.x) + 10. + 760.),
            px(f32::from(bounds.origin.y) + 10. + 300.),
        );
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
            PlatformInput::MouseMove(MouseMoveEvent {
                position: near_right,
                ..Default::default()
            }),
            cx,
        );
        assert!(
            content.read(cx).edge_scroll_active_for_test(),
            "a paint drag inside the edge zone starts the loop"
        );
        let before = camera(&content, cx);
        let moved = content.update(cx, |app, cx| app.edge_scroll_step_for_test(0.1, cx));
        assert!(moved, "the edge tick keeps scrolling");
        let after = camera(&content, cx);
        assert!(
            after.x < before.x,
            "the right edge pans the camera left: {} -> {}",
            before.x,
            after.x
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: near_right,
                ..Default::default()
            }),
            cx,
        );
        assert!(!content.read(cx).edge_scroll_active_for_test());
    })
    .expect("test window remains open");
}
