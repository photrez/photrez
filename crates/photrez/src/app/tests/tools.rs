use super::input::{
    drag_doc, layer_rows, mount_archive_app, page, pixels, select_layer, set_tool, tab_to_canvas,
    window_point,
};
use super::*;
use crate::state::{ShapeTool, ToolId};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};
use photrez_core::ptz::content::{LayerType, SelectionShape, SelectionState};
use photrez_core::ptz::layer_params::{ShapeFillKind, ShapeKind};

fn set_state(
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
    update: impl FnOnce(&mut crate::state::AppState),
) {
    content.update(cx, |app, cx| {
        app.state_for_test().update(cx, |state, _| update(state));
    });
}

#[gpui_kit::test]
fn brush_cursor_ring_follows_size_hardness_and_tool(cx: &mut gpui_kit::TestAppContext) {
    use photrez_core::brush::tip::get_brush_cursor_radius_scale;
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Brush, 1.0);
        set_state(&content, cx, |state| {
            state.brush_size = 20.0;
            state.brush_hardness = 1.0;
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 200));
        let over_page = window_point(bounds, placement, (400, 200), 100.0, 50.0);
        let _ = window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: over_page,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        let ring = content
            .update(cx, |app, cx| app.brush_cursor_for_test(cx))
            .expect("the brush paints its cursor ring");
        assert!(
            (ring.center[0] - 100.0).abs() < 0.5 && (ring.center[1] - 50.0).abs() < 0.5,
            "ring centre follows the pointer: {:?}",
            ring.center
        );
        assert!(
            (ring.radii[0] - 10.0).abs() < 1e-6 && (ring.radii[1] - 10.0).abs() < 1e-6,
            "a 20 px hard brush has a 10 px nominal radius: {:?}",
            ring.radii
        );
        assert!(ring.crosshair, "the brush keeps the centre crosshair");

        // Soft edges map the ring to the 20% alpha contour, not the nominal radius.
        set_state(&content, cx, |state| state.brush_hardness = 0.5);
        window.render_frame(cx);
        let soft = content
            .update(cx, |app, cx| app.brush_cursor_for_test(cx))
            .expect("soft brush ring");
        let expected = 10.0 * get_brush_cursor_radius_scale(0.5);
        assert!(
            (soft.radii[0] - expected).abs() < 1e-6,
            "soft ring={:?} expected={expected}",
            soft.radii
        );

        // The eraser owns its own size and drops the crosshair.
        set_tool(&content, cx, ToolId::Eraser, 1.0);
        set_state(&content, cx, |state| {
            state.eraser_size = 32.0;
            state.eraser_hardness = 0.0;
        });
        window.render_frame(cx);
        let eraser = content
            .update(cx, |app, cx| app.brush_cursor_for_test(cx))
            .expect("the eraser paints its cursor ring");
        let expected = 16.0 * get_brush_cursor_radius_scale(0.0);
        assert!(
            (eraser.radii[0] - expected).abs() < 1e-6,
            "eraser ring={:?} expected={expected}",
            eraser.radii
        );
        assert!(!eraser.crosshair, "the eraser paints no crosshair");

        // Other tools never paint the ring.
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        assert!(
            content
                .update(cx, |app, cx| app.brush_cursor_for_test(cx))
                .is_none(),
            "the Move tool paints no brush ring"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn a_new_shape_layer_becomes_the_active_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Shape, 1.0);
        drag_doc(window, (400, 200), (50.0, 50.0), (150.0, 120.0), cx);
        window.render_frame(cx);
        let selected_name = {
            let state = content.read(cx).state_for_test().read(cx);
            state
                .layers
                .iter()
                .find(|row| row.id == state.selected_layer)
                .map(|row| row.name.clone())
        };
        assert_eq!(
            selected_name.as_deref(),
            Some("Shape"),
            "shapeTool.ts selects the layer it creates"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn the_shape_tool_keeps_the_selection_frame_on_the_shape_rect(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Shape, 1.0);
        drag_doc(window, (400, 200), (50.0, 50.0), (150.0, 120.0), cx);
        window.render_frame(cx);

        let frame = content
            .update(cx, |app, cx| app.transform_frame_for_test(cx))
            .expect("the selected shape keeps its frame under the Shape tool");
        let near = |point: [f64; 2], x: f64, y: f64| {
            (point[0] - x).abs() < 1.0 && (point[1] - y).abs() < 1.0
        };
        assert!(
            frame.corners.iter().any(|point| near(*point, 50.0, 50.0))
                && frame.corners.iter().any(|point| near(*point, 150.0, 50.0))
                && frame.corners.iter().any(|point| near(*point, 150.0, 120.0))
                && frame.corners.iter().any(|point| near(*point, 50.0, 120.0)),
            "the frame follows the drawn rectangle, not the 400x200 canvas: {:?}",
            frame.corners
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn gradient_drag_paints_the_overlay_line(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Gradient, 1.0);
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 200));
        let from = window_point(bounds, placement, (400, 200), 50.0, 50.0);
        let to = window_point(bounds, placement, (400, 200), 250.0, 150.0);
        let _ = window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: to,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let line = content
            .update(cx, |app, cx| app.gradient_line_for_test(cx))
            .expect("the gradient drag paints its guide line");
        assert!(
            (line.start[0] - 50.0).abs() < 0.5
                && (line.start[1] - 50.0).abs() < 0.5
                && (line.end[0] - 250.0).abs() < 0.5
                && (line.end[1] - 150.0).abs() < 0.5,
            "line={line:?}"
        );
        assert!(!line.radial, "the default gradient is linear");
        let badge = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .gradient_badge
            .expect("the drag paints the distance/angle badge");
        assert_eq!(badge.distance, 224.0, "distance={}", badge.distance);
        assert!((badge.angle - 26.6).abs() < 0.05, "angle={}", badge.angle);
        let _ = window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert!(
            content
                .update(cx, |app, cx| app.gradient_line_for_test(cx))
                .is_none(),
            "the committed gradient stops painting the guide line"
        );
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .gradient_badge
                .is_none(),
            "the committed gradient clears the badge"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn brush_opacity_scales_the_stroke_alpha(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        set_state(&content, cx, |state| state.brush_opacity = 0.5);
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (1.5, 0.5), cx);
        let (_, _, after) = pixels(&content, cx);
        assert_ne!(after, before, "the half-opacity stroke still paints");
        // Black at 50% over the opaque fixture leaves a lighter pixel than a
        // full-opacity stroke would (0 instead of the channel value).
        assert!(
            after[0] > 0 && after[0] < before[0],
            "opacity 0.5 must blend, before={before:?} after={after:?}"
        );
        assert_eq!(after[3], 255, "the opaque fixture keeps its alpha");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn bucket_tolerance_and_contiguity_choose_the_filled_region(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);

        // Exact tolerance, non-contiguous: only the equal-coloured pixel 0.
        set_state(&content, cx, |state| {
            state.fill_tolerance = 0;
            state.fill_contiguous = false;
        });
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        let (_, _, exact) = pixels(&content, cx);
        assert_eq!(&exact[0..4], &[0, 0, 0, 255], "pixel 0 is filled");
        assert_eq!(&exact[4..8], &before[4..8], "pixel 1 keeps its colour");

        content.update(cx, |app, cx| {
            app.step_history(true, cx)
                .expect("undo restores the fixture");
        });
        assert_eq!(pixels(&content, cx).2, before);

        // Full tolerance, non-contiguous: every pixel is in range.
        set_state(&content, cx, |state| {
            state.fill_tolerance = 255;
            state.fill_contiguous = false;
        });
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        let (_, _, all) = pixels(&content, cx);
        assert_eq!(&all[4..8], &[0, 0, 0, 255], "pixel 1 joins the fill");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn bucket_honours_the_pixel_selection_mask(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        set_state(&content, cx, |state| state.fill_tolerance = 255);
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
        window.render_frame(cx);
        let (w, h, before) = pixels(&content, cx);
        // Clicking outside the mask must not spill into pixel 1.
        drag_doc(window, (w, h), (1.5, 0.5), (1.5, 0.5), cx);
        let (_, _, after) = pixels(&content, cx);
        assert_eq!(
            &after[4..8],
            &before[4..8],
            "the mask keeps the unselected pixel"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shape_kind_option_creates_an_ellipse_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        set_state(&content, cx, |state| state.shape_kind = ShapeTool::Ellipse);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);
        assert_eq!(
            content.read(cx).last_shape_kind_for_test(),
            Some(ShapeKind::Ellipse),
            "the shape option must reach the created layer"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn eyedropper_samples_the_composite_not_the_selected_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (rows, _) = super::input::layer_rows(&content, cx);
        let bottom = rows
            .iter()
            .find(|row| row.core_id.as_deref() == Some("bottom"))
            .expect("the bottom row exists")
            .id;
        select_layer(&content, cx, bottom);
        set_tool(&content, cx, ToolId::Eyedropper, 1.0);
        window.render_frame(cx);
        // Pixel (0,0) is blue on the top layer, red on the selected bottom.
        drag_doc(window, (2, 1), (0.5, 0.5), (0.5, 0.5), cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).fg_color,
            [0, 0, 255, 255],
            "the eyedropper reads the visible composite"
        );
    })
    .expect("test window remains open");
}

/// S030: the Move bar's toggle response (dispatch + one render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn move_bar_toggle_click_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, _content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            let position = window.find("opt-snap").bounds().center();
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
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S030_MOVE_BAR_TOGGLE_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "Move bar toggle p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// S050: the inspector dock tab switch (dispatch + one render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn dock_tab_switch_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        // Warm the panel body once; the first switch pays the mount cost.
        window.click("adjustments", cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        let mut next = "properties";
        for _ in 0..9 {
            let position = window.find(next).bounds().center();
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
            let tab = content.read(cx).state_for_test().read(cx).inspector_tab;
            if next == "adjustments" {
                assert_eq!(tab, crate::state::InspectorTab::Adjustments);
                next = "properties";
            } else {
                assert_eq!(tab, crate::state::InspectorTab::Properties);
                next = "adjustments";
            }
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S050_DOCK_TAB_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "dock tab switch p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// S031: opening the brush preset menu from the overflow (dispatch + render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn brush_overflow_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 20.0);
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, id: &'static str| {
            let position = window.find(id).bounds().center();
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
        };
        // Warm once on the overflow toggle, then leave it closed.
        press(window, cx, "opt-more");
        window.render_frame(cx);
        assert!(window.try_find("opt-more-panel").is_some());
        press(window, cx, "opt-more");
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            press(window, cx, "opt-more");
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(
                window.try_find("opt-more-panel").is_some(),
                "sample {sample} shows the overflow panel"
            );
            press(window, cx, "opt-more");
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S031_BRUSH_OVERFLOW_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "brush overflow p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// S038: opening the shape kind menu (dispatch + render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn shape_kind_menu_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, id: &'static str| {
            let position = window.find(id).bounds().center();
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
        };
        // Warm once, then close the menu again.
        press(window, cx, "opt-shape-kind");
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            Some("opt-shape-kind")
        );
        window.click("canvas-hitbox", cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            press(window, cx, "opt-shape-kind");
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(
                content.read(cx).state_for_test().read(cx).open_select,
                Some("opt-shape-kind"),
                "sample {sample} opens the kind menu"
            );
            window.click("canvas-hitbox", cx);
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S038_SHAPE_KIND_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "shape kind menu p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// S048: the shape tool's release -> committed rectangle frame. The drag's
/// move frames are warm-up: the measured event is the one that commits the
/// layer and paints the result, like the other tool-commit budgets.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn shape_draw_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let mut samples = Vec::new();
        for _ in 0..9 {
            content.update(cx, |app, cx| {
                app.create_document(800, 600, cx).expect("blank document");
            });
            set_tool(&content, cx, ToolId::Shape, 1.0);
            window.render_frame(cx);
            let (bounds, page_bounds) = page(window, (800, 600));
            let from = window_point(bounds, page_bounds, (800, 600), 10.0, 10.0);
            let to = window_point(bounds, page_bounds, (800, 600), 100.0, 100.0);
            let _ = window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: from,
                    ..Default::default()
                }),
                cx,
            );
            for step in 1..=8 {
                let fraction = step as f32 / 8.;
                let position = gpui_kit::point(
                    from.x + (to.x - from.x) * fraction,
                    from.y + (to.y - from.y) * fraction,
                );
                let _ = window.dispatch_event(
                    PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                        position,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Default::default(),
                    }),
                    cx,
                );
            }
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: to,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(
                content.read(cx).active_core_layer_count_for_test(),
                Some(2),
                "the release commits one Shape layer over the Background"
            );
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "shape_draw p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "shape draw p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// S006: one pointer move over the page with the brush armed, from the event
/// dispatch through the frame that paints the cursor ring. Measured on a
/// fresh 800x600 document per sample, like the other overlay latencies.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn brush_cursor_hover_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let mut samples = Vec::new();
        for _ in 0..9 {
            content.update(cx, |app, cx| {
                app.create_document(800, 600, cx).expect("blank document");
            });
            set_tool(&content, cx, ToolId::Brush, 1.0);
            window.render_frame(cx);
            let (bounds, page_bounds) = page(window, (800, 600));
            let position = window_point(bounds, page_bounds, (800, 600), 400.0, 300.0);
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(
                content
                    .update(cx, |app, cx| app.brush_cursor_for_test(cx))
                    .is_some(),
                "the hover frame paints the ring"
            );
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "brush_cursor_hover p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "brush cursor hover p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// S023: the gradient tool's release -> committed gradient frame, measured on
/// a fresh 800x600 document like the shape commit above.
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn gradient_draw_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let mut samples = Vec::new();
        for _ in 0..9 {
            content.update(cx, |app, cx| {
                app.create_document(800, 600, cx).expect("blank document");
            });
            set_tool(&content, cx, ToolId::Gradient, 1.0);
            window.render_frame(cx);
            let before = content
                .read(cx)
                .active_core_history_version_for_test()
                .expect("history version");
            let (bounds, page_bounds) = page(window, (800, 600));
            let from = window_point(bounds, page_bounds, (800, 600), 10.0, 10.0);
            let to = window_point(bounds, page_bounds, (800, 600), 100.0, 100.0);
            let _ = window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: from,
                    ..Default::default()
                }),
                cx,
            );
            for step in 1..=8 {
                let fraction = step as f32 / 8.;
                let position = gpui_kit::point(
                    from.x + (to.x - from.x) * fraction,
                    from.y + (to.y - from.y) * fraction,
                );
                let _ = window.dispatch_event(
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Default::default(),
                    }),
                    cx,
                );
            }
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: to,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_ne!(
                content
                    .read(cx)
                    .active_core_history_version_for_test()
                    .expect("history version"),
                before,
                "the release commits the gradient through core history"
            );
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "gradient_draw p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "gradient draw p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// S032: picking a bucket tolerance preset (dispatch + render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn bucket_tolerance_preset_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, id: &'static str| {
            let position = window.find(id).bounds().center();
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
        };
        // Warm once, then return to the 32 preset.
        press(window, cx, "opt-tol-64");
        window.render_frame(cx);
        press(window, cx, "opt-tol-32");
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            press(window, cx, "opt-tol-64");
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(
                content.read(cx).state_for_test().read(cx).fill_tolerance == 64,
                "sample {sample} selects the 64 preset"
            );
            press(window, cx, "opt-tol-32");
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S032_BUCKET_TOL_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "bucket tolerance preset p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// S034: hovering the eyedropper HEX badge (hover -> tooltip paint).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn eyedropper_hex_tooltip_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Eyedropper, 1.0);
        window.render_frame(cx);
        // Warm once, then leave the pointer off the badge.
        window.hover("opt-copy-hex", cx);
        window.render_frame(cx);
        assert!(window.try_find("opt-copy-hex-tooltip").is_some());
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            window.hover("opt-copy-hex", cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(
                window.try_find("opt-copy-hex-tooltip").is_some(),
                "sample {sample} shows the copy tooltip"
            );
            window.hover("canvas-hitbox", cx);
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S034_HEX_TOOLTIP_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "eyedropper hex tooltip p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// S035: opening the gradient type menu (dispatch + render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn gradient_type_menu_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Gradient, 1.0);
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, id: &'static str| {
            let position = window.find(id).bounds().center();
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
        };
        // Warm once, then close the menu again.
        press(window, cx, "opt-gradient-type");
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            Some("opt-gradient-type")
        );
        window.click("canvas-hitbox", cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            press(window, cx, "opt-gradient-type");
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(
                content.read(cx).state_for_test().read(cx).open_select,
                Some("opt-gradient-type"),
                "sample {sample} opens the type menu"
            );
            window.click("canvas-hitbox", cx);
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S035_GRAD_TYPE_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "gradient type menu p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

/// S037: opening the selection shape menu (dispatch + render).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn selection_shape_menu_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, id: &'static str| {
            let position = window.find(id).bounds().center();
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
        };
        press(window, cx, "opt-select-shape");
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            Some("opt-select-shape")
        );
        window.click("canvas-hitbox", cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            press(window, cx, "opt-select-shape");
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(
                content.read(cx).state_for_test().read(cx).open_select,
                Some("opt-select-shape"),
                "sample {sample} opens the shape menu"
            );
            window.click("canvas-hitbox", cx);
            window.render_frame(cx);
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S037_SELECT_SHAPE_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "selection shape menu p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn option_bar_steppers_change_the_live_tool_settings(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        window.click("opt-brush-opacity", cx);
        window.press("9", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let opacity = content.read(cx).state_for_test().read(cx).brush_opacity;
        assert!((opacity - 0.9).abs() < 1e-6, "opacity={opacity}");
        window.click("opt-more", cx);
        window.render_frame(cx);
        window.click("opt-brush-smoothing", cx);
        window.press("5", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).brush_smoothing,
            5.0
        );

        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        window.click("opt-tol-64", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).fill_tolerance,
            64
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_click_opens_an_editor_and_typing_commits_to_the_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        assert_eq!(
            content.read(cx).text_edit_content_for_test(cx).as_deref(),
            Some(""),
            "the text click opens an empty editor, as textTool.ts does"
        );
        assert!(
            window.try_find("text-editor").is_some(),
            "the in-place editor overlay is mounted"
        );
        assert!(
            window.try_find("text-editor-hint").is_some(),
            "the overlay shows the key legend"
        );

        tab_to_canvas(window, cx);
        window.press("h", cx);
        window.press("i", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).text_edit_content_for_test(cx).as_deref(),
            Some("hi")
        );
        window.press("ctrl-enter", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).text_edit_content_for_test(cx).is_none(),
            "Ctrl+Enter closes the editor"
        );
        let text_id = content
            .read(cx)
            .text_layer_id_for_test()
            .expect("the text layer exists");
        assert_eq!(
            content
                .read(cx)
                .text_layer_content_for_test(&text_id)
                .as_deref(),
            Some("hi"),
            "the typed text is committed to the layer"
        );
        assert!(content
            .read(cx)
            .active_document_save_state_for_test()
            .is_some_and(|(_, _, dirty)| dirty));
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_escape_cancels_the_session_and_removes_the_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let before = content.read(cx).active_core_layer_count_for_test().unwrap();
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("x", cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).text_edit_content_for_test(cx).is_none(),
            "Escape closes the editor"
        );
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test().unwrap(),
            before,
            "Escape removes the temporary text layer"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn mounted_shape_saves_and_reloads_as_a_shape_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);
        let saved = directory.path().join("shape-saved.ptz");
        content
            .update(cx, |app, cx| app.save_to_path_for_test(&saved, cx))
            .expect("shape document saves");
        let file_io = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("file boundary");
        file_io.approve_path(&saved).expect("saved path approved");
        content
            .update(cx, |app, cx| {
                app.open_startup_document_for_test(&saved, &file_io, cx)
            })
            .expect("saved document reopens");
        let types = content
            .read(cx)
            .active_core_layer_types_for_test()
            .expect("layers exist");
        assert!(
            types
                .iter()
                .any(|kind| matches!(kind, photrez_core::ptz::content::LayerType::Shape)),
            "the shape layer survives save/reload: {types:?}"
        );
        let _ = source;
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn mounted_text_renders_into_the_exported_png(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(160, 96, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Text;
                state.fg_color = [0, 0, 0, 255];
            });
        });
        window.render_frame(cx);
        drag_doc(window, (160, 96), (12.0, 12.0), (12.0, 12.0), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        window.press("g", cx);
        window.press("ctrl-enter", cx);
        window.render_frame(cx);

        let exported = directory.path().join("text-export.png");
        content
            .update(cx, |app, cx| app.export_to_path_for_test(&exported, cx))
            .expect("text document exports");
        let image = ::image::open(&exported)
            .expect("exported PNG opens")
            .to_rgba8();
        let ink = image
            .pixels()
            .filter(|pixel| pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100)
            .count();
        assert!(ink > 0, "the exported PNG contains text ink");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn mounted_shape_renders_into_the_exported_png(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(96, 96, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Shape;
                state.fg_color = [0, 0, 0, 255];
                state.shape_kind = crate::state::ShapeTool::Rect;
            });
        });
        window.render_frame(cx);
        drag_doc(window, (96, 96), (16.0, 16.0), (80.0, 80.0), cx);
        let exported = directory.path().join("shape-export.png");
        content
            .update(cx, |app, cx| app.export_to_path_for_test(&exported, cx))
            .expect("shape document exports");
        let image = ::image::open(&exported)
            .expect("exported PNG opens")
            .to_rgba8();
        let ink = image
            .pixels()
            .filter(|pixel| pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100)
            .count();
        assert!(ink > 100, "the exported PNG contains the shape");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn brush_preset_button_cycles_the_frozen_presets(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        // The fixture bar is 781 px: the preset control lives in the overflow.
        window.click("opt-more", cx);
        window.render_frame(cx);
        window.click("opt-brush-preset", cx);
        window.render_frame(cx);
        window.click("opt-brush-preset-option-1", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.brush_size, 40.0);
            assert!((state.brush_hardness - 0.15).abs() < 1e-6);
        }
        window.click("opt-brush-preset", cx);
        window.render_frame(cx);
        window.click("opt-brush-preset-option-2", cx);
        window.render_frame(cx);
        assert_eq!(content.read(cx).state_for_test().read(cx).brush_size, 5.0);

        set_tool(&content, cx, ToolId::Eraser, 1.0);
        window.render_frame(cx);
        window.click("opt-brush-preset", cx);
        window.render_frame(cx);
        window.click("opt-brush-preset-option-4", cx);
        window.render_frame(cx);
        assert_eq!(content.read(cx).state_for_test().read(cx).eraser_size, 30.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn brush_pointer_moves_push_preview_patches_without_recompositing(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 20.0);
        window.render_frame(cx);
        let before = content.read(cx).full_composites_for_test();
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.2, 0.5), (0.8, 0.5), cx);
        let after = content.read(cx).full_composites_for_test();
        assert_eq!(
            after, before,
            "a brush gesture never recomposites the document; moves and commit both patch"
        );
        content.update(cx, |app, cx| {
            app.refresh_active_canvas(cx).expect("composite runs");
        });
        assert_eq!(
            content.read(cx).full_composites_for_test(),
            after + 1,
            "the counter observes a real full composite"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn multi_layer_brush_moves_push_preview_patches_without_recompositing(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 2.0);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test(),
            Some(2),
            "the fixture has two raster layers"
        );
        let composites_before = content.read(cx).full_composites_for_test();
        let previews_before = content.read(cx).preview_pushes_for_test();
        drag_doc(window, (2, 1), (0.5, 0.5), (1.5, 0.5), cx);
        assert!(
            content.read(cx).preview_pushes_for_test() > previews_before,
            "a move on a multi-layer document pushes a display preview"
        );
        assert_eq!(
            content.read(cx).full_composites_for_test(),
            composites_before + 1,
            "only the pointer-up commit recomposites; the moves patch"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn move_toggles_are_tab_reachable_and_space_operates_them(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);

        let ids = ["opt-snap", "opt-auto-select", "opt-transform"];
        let mut seen = [false; 3];
        for _ in 0..12 {
            window.press("tab", cx);
            window.render_frame(cx);
            for (index, id) in ids.iter().enumerate() {
                if window.find(*id).focused() == Some(true) {
                    seen[index] = true;
                }
            }
            if seen.iter().all(|reached| *reached) {
                break;
            }
        }
        assert!(
            seen.iter().all(|reached| *reached),
            "Tab must reach every Move toggle: {seen:?}"
        );

        for _ in 0..8 {
            if window.find("opt-snap").focused() == Some(true) {
                break;
            }
            window.press("tab", cx);
            window.render_frame(cx);
        }
        assert_eq!(
            window.find("opt-snap").focused(),
            Some(true),
            "the snap toggle is focused before Space"
        );
        let before = content.read(cx).state_for_test().read(cx).snap_enabled;
        window.press("space", cx);
        let after_press = content.read(cx).state_for_test().read(cx).snap_enabled;
        assert_eq!(after_press, !before, "Space toggles before the next frame");
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).snap_enabled,
            !before,
            "Space operates the focused toggle"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn move_snap_toggle_updates_snap_state(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).snap_enabled,
            "snapping defaults to on"
        );
        window.click("opt-snap", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).state_for_test().read(cx).snap_enabled,
            "clicking the snap toggle turns snapping off"
        );
        window.click("opt-snap", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).snap_enabled,
            "clicking again turns snapping back on"
        );
        window.click("opt-auto-select", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).state_for_test().read(cx).move_auto_select,
            "the auto-select toggle turns auto-select off"
        );
        window.click("opt-auto-select", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).move_auto_select,
            "clicking again turns auto-select back on"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn transform_controls_toggle_gates_direct_manipulation(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test()
                .update(cx, |state, _| state.active_tool = ToolId::Move);
        });
        window.render_frame(cx);
        let center = (200.0_f32, 150.0_f32);
        let moved = (center.0 + 40.0, center.1 + 30.0);

        window.click("opt-transform", cx);
        window.render_frame(cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .show_transform_controls
        );
        let before = content.read(cx).selected_transform_for_test(cx);
        drag_doc(window, (400, 300), center, moved, cx);
        let after = content.read(cx).selected_transform_for_test(cx);
        assert_eq!(
            after, before,
            "transform controls off blocks direct manipulation"
        );

        window.click("opt-transform", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .show_transform_controls
        );
        drag_doc(window, (400, 300), center, moved, cx);
        let after_on = content.read(cx).selected_transform_for_test(cx);
        assert_ne!(
            after_on, before,
            "transform controls on restores drag-to-move"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn move_auto_select_click_picks_the_layer_under_the_pointer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        let (rows, selected) = layer_rows(&content, cx);
        assert_eq!(rows.len(), 2, "the fixture has two layers");
        assert_eq!(selected, rows[0].id, "the top layer starts selected");

        // The top layer is 1x1 at the document origin, so the right half only
        // intersects the bottom layer.
        drag_doc(window, (2, 1), (1.5, 0.5), (1.5, 0.5), cx);
        let (_, selected) = layer_rows(&content, cx);
        assert_eq!(
            selected, rows[1].id,
            "the click selects the layer under the pointer"
        );

        drag_doc(window, (2, 1), (0.5, 0.5), (0.5, 0.5), cx);
        let (_, selected) = layer_rows(&content, cx);
        assert_eq!(
            selected, rows[0].id,
            "the click selects the top layer again"
        );

        window.click("opt-auto-select", cx);
        window.render_frame(cx);
        assert!(!content.read(cx).state_for_test().read(cx).move_auto_select);
        drag_doc(window, (2, 1), (1.5, 0.5), (1.5, 0.5), cx);
        let (_, selected) = layer_rows(&content, cx);
        assert_eq!(
            selected, rows[0].id,
            "auto-select off keeps the current selection"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn tolerance_slider_sets_the_value_on_click_and_drag(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        let bounds = window.find("opt-tolerance-slider").bounds();
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        assert!(width > 0.0 && height > 0.0, "the track is laid out");

        window.click_at(
            "opt-tolerance-slider",
            gpui_kit::point(gpui_kit::px(width * 0.75), gpui_kit::px(height * 0.5)),
            cx,
        );
        window.render_frame(cx);
        let value = content.read(cx).state_for_test().read(cx).fill_tolerance;
        assert!(
            (i64::from(value) - 191).abs() <= 2,
            "click at 75% maps to 191, got {value}"
        );

        window.drag(
            bounds.origin + gpui_kit::point(gpui_kit::px(width * 0.25), gpui_kit::px(height * 0.5)),
            bounds.origin + gpui_kit::point(gpui_kit::px(width * 0.5), gpui_kit::px(height * 0.5)),
            cx,
        );
        window.render_frame(cx);
        let value = content.read(cx).state_for_test().read(cx).fill_tolerance;
        assert!(
            (i64::from(value) - 128).abs() <= 3,
            "drag to 50% maps to 128, got {value}"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shape_bar_kind_select_is_live(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        window.click("opt-shape-kind", cx);
        window.render_frame(cx);
        window.click("opt-shape-kind-option-1", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).shape_kind,
            ShapeTool::Ellipse,
            "choosing Ellipse sets the shape kind"
        );
        assert_eq!(content.read(cx).state_for_test().read(cx).open_select, None);

        window.click("opt-shape-kind", cx);
        window.render_frame(cx);
        window.press("escape", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            None,
            "Escape closes the shape dropdown"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).shape_kind,
            ShapeTool::Ellipse
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shape_fill_stroke_radius_and_arrow_are_live(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);

        // The pill opens the popover; its header checkbox toggles the outline,
        // and the oracle's last-paint guard keeps fill or stroke on.
        window.click("opt-fill", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).shape_fill,
            "fill stays on while stroke is off"
        );
        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .stroke_popover_open,
            "the pill opens the stroke popover"
        );
        window.click("opt-stroke-enable", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).shape_stroke,
            "the popover checkbox turns the outline on"
        );
        window.click("opt-fill", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).state_for_test().read(cx).shape_fill,
            "fill can turn off once stroke is on"
        );
        window.click("opt-stroke-enable", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).shape_stroke,
            "stroke stays on while fill is off"
        );

        window.click("opt-shape-radius", cx);
        window.press("4", cx);
        window.press("2", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).shape_radius,
            42.0,
            "radius commits live from the field"
        );
        window.click("opt-shape-radius", cx);
        window.press("9", cx);
        window.press("9", cx);
        window.press("9", cx);
        window.press("enter", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).shape_radius,
            100.0,
            "radius clamps to the oracle ceiling"
        );

        window.click("opt-shape-kind", cx);
        window.render_frame(cx);
        window.click("opt-shape-kind-option-2", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).shape_kind,
            ShapeTool::Line
        );
        window.click("opt-shape-arrow", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).shape_arrow_head,
            "the line shape reveals the live arrow toggle"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shape_bar_edits_the_selected_shape_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);

        content.update(cx, |app, cx| {
            let row = {
                let state = app.state_for_test().read(cx);
                state
                    .layers
                    .iter()
                    .find(|row| row.name == "Shape")
                    .map(|row| row.id)
                    .expect("shape row")
            };
            app.state_for_test().update(cx, |state, _| {
                state.selected_layer = row;
                state.selected_layers = vec![row];
            });
            app.project_layers(cx);
        });
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .selected_shape
                .is_some(),
            "a selected shape puts the bar into edit mode"
        );
        let cursor_start = content.read(cx).active_core_history_cursor_for_test();

        // Enabling the outline through the popover edits the layer, not just
        // the next-shape defaults.
        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        window.click("opt-stroke-enable", cx);
        window.render_frame(cx);
        let params = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .selected_shape
            .clone()
            .expect("shape parameters");
        assert!(
            params.stroke.enabled,
            "the popover checkbox reaches the layer"
        );
        assert_eq!(
            content.read(cx).active_core_history_cursor_for_test(),
            cursor_start.map(|cursor| cursor + 1),
            "the layer edit is one core history entry"
        );

        window.click("opt-stroke-color", cx);
        window.render_frame(cx);
        for digit in ["1", "2", "3", "4", "5", "6"] {
            window.press(digit, cx);
        }
        window.press("enter", cx);
        window.render_frame(cx);
        let params = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .selected_shape
            .clone()
            .expect("shape parameters");
        assert_eq!(
            params.stroke.color, "#123456",
            "the picker writes the layer's outline colour"
        );
        assert!(params.stroke.enabled);

        // Fill off is legal while the outline is on, and undo restores it.
        window.click("opt-fill", cx);
        window.render_frame(cx);
        let params = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .selected_shape
            .clone()
            .expect("shape parameters");
        assert_eq!(
            params.fill.kind,
            ShapeFillKind::None,
            "turning Fill off edits the layer"
        );
        content.update(cx, |app, cx| {
            assert!(app.step_history(true, cx).expect("undo fill"));
            assert_eq!(
                app.state_for_test()
                    .read(cx)
                    .selected_shape
                    .as_ref()
                    .map(|params| params.fill.kind.clone()),
                Some(ShapeFillKind::Solid),
                "undo restores the layer's fill"
            );
        });

        // The radius field commits to the layer and keeps the next-shape
        // default in step.
        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        window.click("opt-shape-radius", cx);
        window.press("4", cx);
        window.press("2", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(
                state.selected_shape.as_ref().map(|params| params.radius),
                Some(42.0),
                "the radius field edits the selected layer"
            );
            assert_eq!(
                state.shape_radius, 42.0,
                "the edit syncs the session default"
            );
        }
        content.update(cx, |app, cx| {
            // The NumField commits live per keystroke, so "4" and "42" are
            // separate core entries.
            assert!(app.step_history(true, cx).expect("undo the 42"));
            assert_eq!(
                app.state_for_test()
                    .read(cx)
                    .selected_shape
                    .as_ref()
                    .map(|params| params.radius),
                Some(4.0),
                "undo steps back through the live radius commit"
            );
            assert!(app.step_history(true, cx).expect("undo the 4"));
            assert_eq!(
                app.state_for_test()
                    .read(cx)
                    .selected_shape
                    .as_ref()
                    .map(|params| params.radius),
                Some(0.0),
                "undo restores the layer's created radius"
            );
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shape_stroke_popover_edits_color_and_width(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-stroke-popover").is_none(),
            "the popover starts closed"
        );
        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-stroke-popover").is_some(),
            "the pill mounts the popover"
        );

        window.click("opt-stroke-enable", cx);
        window.render_frame(cx);
        assert!(content.read(cx).state_for_test().read(cx).shape_stroke);

        window.click("opt-stroke-width", cx);
        window.press("1", cx);
        window.press("2", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .shape_stroke_width,
            12.0,
            "width commits live from the popover field"
        );
        window.click("opt-stroke-width", cx);
        window.press("9", cx);
        window.press("9", cx);
        window.press("9", cx);
        window.press("enter", cx);
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .shape_stroke_width,
            200.0,
            "width clamps to the oracle ceiling"
        );

        window.click("opt-stroke-color", cx);
        window.render_frame(cx);
        assert!(
            window.find("color-picker").visible(),
            "the colour button opens the picker"
        );
        content.update(cx, |app, cx| {
            assert_eq!(
                app.state_for_test()
                    .read(cx)
                    .color_picker
                    .as_ref()
                    .map(|session| session.target),
                Some(crate::state::ColorTarget::ShapeStroke),
                "the picker edits the stroke target"
            );
        });
        for digit in ["1", "2", "3", "4", "5", "6"] {
            window.press(digit, cx);
        }
        window.press("enter", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(
                state.shape_stroke_color,
                [0x12, 0x34, 0x56, 255],
                "the picked colour reaches the stroke setting"
            );
            assert!(state.shape_stroke, "choosing a colour enables the outline");
        }

        // Close the popover, then draw: the new layer carries the popover's
        // width and colour through `apply_shape`.
        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.1, 0.1), (0.9, 0.9), cx);
        assert_eq!(
            content.read(cx).last_shape_stroke_for_test(),
            Some((true, "#123456".to_string(), 200.0)),
            "the created layer uses the popover's stroke"
        );

        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        window.press("escape", cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .stroke_popover_open,
            "Escape closes the popover"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn gradient_bar_selects_and_reverse_are_live(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Gradient, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("option-live-gradient-radial").is_none(),
            "the Gradient live strip is retired"
        );

        window.click("opt-gradient-type", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            Some("opt-gradient-type")
        );
        window.click("opt-gradient-type-option-1", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).gradient_radial,
            "choosing Radial sets the gradient type"
        );
        assert_eq!(content.read(cx).state_for_test().read(cx).open_select, None);

        window.click("opt-gradient-preset", cx);
        window.render_frame(cx);
        window.click("opt-gradient-preset-option-1", cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .gradient_fg_transparent,
            "choosing FG to transparent sets the preset"
        );

        assert!(!content.read(cx).state_for_test().read(cx).gradient_reverse);
        window.click("opt-reverse", cx);
        assert!(
            content.read(cx).state_for_test().read(cx).gradient_reverse,
            "the chrome Reverse button drives the setting"
        );

        window.click("opt-gradient-type", cx);
        window.render_frame(cx);
        window.press("escape", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            None,
            "Escape closes the dropdown without choosing"
        );
        assert!(content.read(cx).state_for_test().read(cx).gradient_radial);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn tolerance_slider_arrow_keys_adjust_the_value(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        let bounds = window.find("opt-tolerance-slider").bounds();
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        window.click_at(
            "opt-tolerance-slider",
            gpui_kit::point(gpui_kit::px(width * 0.5), gpui_kit::px(height * 0.5)),
            cx,
        );
        let start = i64::from(content.read(cx).state_for_test().read(cx).fill_tolerance);
        window.press("right", cx);
        window.press("right", cx);
        window.press("up", cx);
        assert_eq!(
            i64::from(content.read(cx).state_for_test().read(cx).fill_tolerance),
            start + 3,
            "arrows step the value"
        );
        window.press("home", cx);
        assert_eq!(content.read(cx).state_for_test().read(cx).fill_tolerance, 0);
        window.press("end", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).fill_tolerance,
            255
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn paint_bucket_bar_presets_field_and_contiguous_are_live(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::PaintBucket, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("option-live-fill-tolerance").is_none(),
            "the Paint Bucket live strip is retired"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).fill_tolerance,
            32
        );

        window.click("opt-tol-64", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).fill_tolerance,
            64
        );

        window.click("opt-tolerance", cx);
        window.press("1", cx);
        window.press("0", cx);
        window.press("0", cx);
        window.press("enter", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).fill_tolerance,
            100
        );

        assert!(content.read(cx).state_for_test().read(cx).fill_contiguous);
        window.click("opt-contiguous", cx);
        assert!(!content.read(cx).state_for_test().read(cx).fill_contiguous);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn eyedropper_bar_checkbox_toggles_auto_copy(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Eyedropper, 1.0);
        window.render_frame(cx);
        assert!(
            window
                .try_find("option-live-eyedropper-auto-copy")
                .is_none(),
            "the Eyedropper live strip is retired"
        );
        assert!(!content.read(cx).state_for_test().read(cx).auto_copy_hex);
        window.click("opt-auto-copy", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).auto_copy_hex,
            "the chrome checkbox drives the setting"
        );
        window.click("opt-auto-copy", cx);
        assert!(!content.read(cx).state_for_test().read(cx).auto_copy_hex);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn eyedropper_hex_badge_shows_copy_tooltip(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Eyedropper, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-copy-hex-tooltip").is_none(),
            "the HEX badge is tooltip-free until it is hovered"
        );
        window.hover("opt-copy-hex", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-copy-hex-tooltip").is_some(),
            "the HEX badge carries the oracle click-to-copy tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-copy-hex-tooltip").is_none(),
            "leaving the badge hides the tooltip"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_size_num_field_commits_live_and_escape_reverts(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);

        window.click("opt-text-size", cx);
        window.press("7", cx);
        window.press("2", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).text_font_size,
            72.0,
            "valid keystrokes commit live"
        );

        window.click("opt-text-size", cx);
        window.press("9", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).text_font_size,
            9.0
        );
        window.press("escape", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).text_font_size,
            72.0,
            "Escape restores the value the edit started from"
        );

        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.font_size, 72.0, "the created layer inherits the value");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_option_controls_reach_the_created_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        window.click("opt-text-size", cx);
        window.press("5", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.click("opt-weight", cx);
        window.render_frame(cx);
        window.click("opt-weight-option-5", cx);
        window.click("opt-italic", cx);
        window.click("opt-align-center", cx);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.font_size, 50.0);
        assert_eq!(data.font_weight, 700.0);
        assert_eq!(
            data.font_style,
            photrez_core::ptz::layer_params::TextFontStyle::Italic
        );
        assert_eq!(
            data.align,
            photrez_core::ptz::layer_params::TextAlign::Center
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn adjustment_sliders_brighten_the_composite_and_undo(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Adjustments;
            });
        });
        window.render_frame(cx);
        let before = content
            .read(cx)
            .active_composite_pixel_for_test(0, 0)
            .expect("composite pixel");
        let track = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .adjustment_slider_bounds[0]
            .get()
            .expect("the Bright track is laid out");
        let mut press = |fraction: f32, cx: &mut gpui_kit::App| {
            let x = track.origin.x + gpui_kit::px(f32::from(track.size.width) * fraction);
            let y = track.origin.y + track.size.height / 2.0;
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: gpui_kit::point(x, y),
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: gpui_kit::point(x, y),
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        };
        press(0.55, cx);
        let cursor_one = content.read(cx).active_core_history_cursor_for_test();
        press(0.60, cx);
        let cursor_two = content.read(cx).active_core_history_cursor_for_test();
        window.render_frame(cx);
        assert_eq!((cursor_one, cursor_two), (Some(1), Some(2)));
        let after = content
            .read(cx)
            .active_composite_pixel_for_test(0, 0)
            .expect("composite pixel");
        assert!(
            after[0] > before[0] && after[1] > before[1] && after[2] > before[2],
            "brightness must lighten the composite: {before:?} -> {after:?}"
        );
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .adjustment_brightness,
            20.0
        );
        content.update(cx, |app, cx| {
            let stepped = app.step_history(true, cx).expect("undo one");
            assert!(stepped);
            assert_eq!(app.active_core_history_cursor_for_test(), Some(1));
            let (_, layer) = app.selected_layer_for_cursor(cx).unwrap();
            assert_eq!(
                layer
                    .basic_adjustment
                    .as_ref()
                    .map(|adjustment| adjustment.brightness)
                    .unwrap_or(0.0),
                10.0,
                "one undo restores +10"
            );
            let stepped = app.step_history(true, cx).expect("undo two");
            assert!(stepped);
            assert_eq!(app.active_core_history_cursor_for_test(), Some(0));
            let (_, layer) = app.selected_layer_for_cursor(cx).unwrap();
            assert_eq!(
                layer
                    .basic_adjustment
                    .as_ref()
                    .map(|adjustment| adjustment.brightness),
                None,
                "second undo clears the adjustment"
            );
        });
        assert_eq!(
            content
                .read(cx)
                .active_composite_pixel_for_test(0, 0)
                .expect("composite pixel"),
            before
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn properties_panel_actions_reach_the_selected_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("prop-flip-h").is_some(),
            "the properties panel mounts its live actions"
        );

        let flip = |cx: &mut gpui_kit::App| {
            content
                .update(cx, |app, cx| {
                    app.selected_layer_for_cursor(cx)
                        .map(|(_, layer)| layer.transform.flip_h)
                })
                .expect("selected layer")
        };
        assert!(!flip(cx));
        window.click("prop-flip-h", cx);
        assert!(flip(cx), "flip H toggles the layer transform");
        window.click("prop-flip-h", cx);
        assert!(!flip(cx), "flip H toggles back");

        content.update(cx, |app, cx| {
            app.nudge_selected_layer(12.0, -7.0, cx)
                .expect("nudge applies");
        });
        window.render_frame(cx);
        assert_eq!(
            content
                .read(cx)
                .selected_transform_for_test(cx)
                .map(|transform| (transform.0, transform.1)),
            Some((12.0, -7.0))
        );
        window.click("prop-reset", cx);
        assert_eq!(
            content
                .read(cx)
                .selected_transform_for_test(cx)
                .map(|transform| (transform.0, transform.1, transform.4)),
            Some((0.0, 0.0, 0.0)),
            "reset returns the layer to its origin"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_option_bar_edits_the_selected_layer_and_syncs_defaults(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        window.render_frame(cx);
        assert!(
            window.try_find("opt-text-size").is_some(),
            "a text selection exposes the typography controls"
        );
        assert!(
            window.try_find("opt-text-rasterize").is_some(),
            "layer-only actions appear in edit mode"
        );

        window.click("opt-text-size", cx);
        window.press("5", cx);
        window.press("0", cx);
        window.press("enter", cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.font_size, 50.0, "typed 50 replaces the resting 48");
        assert_eq!(
            content.read(cx).state_for_test().read(cx).text_font_size,
            50.0,
            "editing the layer syncs the next-text default"
        );

        window.click("opt-weight", cx);
        window.render_frame(cx);
        window.click("opt-weight-option-5", cx);
        window.click("opt-italic", cx);
        window.click("opt-underline", cx);
        window.click("opt-strikethrough", cx);
        window.click("opt-uppercase", cx);
        window.click("opt-align-center", cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.font_weight, 700.0);
        assert_eq!(
            data.font_style,
            photrez_core::ptz::layer_params::TextFontStyle::Italic
        );
        assert_eq!(
            data.align,
            photrez_core::ptz::layer_params::TextAlign::Center
        );
        assert_eq!(data.underline, Some(true));
        assert_eq!(data.strikethrough, Some(true));
        assert_eq!(data.uppercase, Some(true));
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.text_font_weight, 700.0);
            assert!(state.text_italic);
            assert_eq!(state.text_align, crate::state::TextAlignTool::Center);
            assert!(state.text_underline);
            assert!(state.text_strikethrough);
            assert!(state.text_uppercase);
        }

        // U11: the host renders the text bitmap and hands it to RasterizeLayer,
        // which materializes it as the rasterized layer's canonical buffer.
        // A layer whose features the core rasterizer cannot render (the
        // decorated case: underline/strikethrough/uppercase/weight) refuses
        // the rasterize with a typed error instead of producing a Raster
        // layer the compositor cannot paint. (Pre-fix RED receipt: both
        // kinds rasterized metadata-only and failed with MissingPixels —
        // .slim/deepwork/78-evidence/gates-u10-u11/defect-receipts.log.)
        content.update(cx, |app, cx| {
            let error = app
                .rasterize_selected_layer(cx)
                .expect_err("a decorated text layer refuses the rasterize");
            assert!(
                error.contains("cannot be rendered"),
                "typed refusal, got {error}"
            );
        });
        window.render_frame(cx);
        let layer_type = content
            .update(cx, |app, cx| {
                app.selected_layer_for_cursor(cx)
                    .map(|(_, layer)| layer.layer_type)
            })
            .expect("selected layer");
        assert_eq!(
            layer_type,
            photrez_core::ptz::content::LayerType::Text,
            "the refused rasterize leaves the layer parametric"
        );
        assert!(
            window.try_find("option-live-text-rasterize").is_none(),
            "layer-only actions disappear with the text selection"
        );
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .status_error
                .is_some(),
            "the typed refusal surfaces in the status bar"
        );
    })
    .expect("test window remains open");
}

/// U11 fixed: a supported text layer rasterizes with its host-rendered
/// bitmap materialized as the canonical buffer, and the document composites.
#[gpui_kit::test]
fn text_rasterize_materializes_pixels_and_composites(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        window.render_frame(cx);

        content.update(cx, |app, cx| {
            app.rasterize_selected_layer(cx)
                .expect("a supported text layer rasterizes");
        });
        window.render_frame(cx);
        let (layer_type, store_pixels) = content.update(cx, |app, cx| {
            let (_, layer) = app.selected_layer_for_cursor(cx).expect("selected layer");
            let layer_type = layer.layer_type;
            let pixels = app
                .active_document
                .as_ref()
                .and_then(|doc| app.sessions.get(&doc.doc_id))
                .and_then(|session| {
                    let loaded = session.loaded_document();
                    loaded
                        .content_snapshot()
                        .and_then(|c| c.layers.iter().find(|l| l.id == layer.id).cloned())
                        .and_then(|l| {
                            loaded
                                .registry
                                .get_layer(&loaded.document_id(), &l.id)
                                .map(|px| (px.width, px.height, px.pixels.len(), l.width, l.height))
                        })
                });
            (layer_type, pixels)
        });
        assert_eq!(
            layer_type,
            photrez_core::ptz::content::LayerType::Raster,
            "rasterize converts the parametric layer"
        );
        let (width, height, len, layer_w, layer_h) =
            store_pixels.expect("the rasterized layer carries pixels");
        assert_eq!(
            len,
            width as usize * height as usize * 4,
            "the materialized buffer matches its dims"
        );
        assert_eq!(
            (f64::from(width), f64::from(height)),
            (layer_w, layer_h),
            "the layer frame matches the materialized bitmap"
        );
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .status_error
                .is_none(),
            "a successful rasterize leaves no status error"
        );
    })
    .expect("test window remains open");
}

/// S039 perf: one keystroke in the Text bar's size field commits live and
/// paints a frame. The key is dispatched directly; gpui-kit's press helper
/// renders a frame before and after and would charge three frames.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn text_bar_size_commit_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        window.render_frame(cx);
        let mut samples = Vec::new();
        let mut last: Option<f64> = None;
        for i in 0..9 {
            window.click("opt-text-size", cx);
            window.render_frame(cx);
            let digit = if i % 2 == 0 { "6" } else { "7" };
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
                    keystroke: gpui_kit::Keystroke {
                        modifiers: Default::default(),
                        key: digit.to_string(),
                        key_char: Some(digit.to_string()),
                    },
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            window.press("enter", cx);
            let size = content
                .read(cx)
                .text_layer_data_for_test()
                .expect("text data")
                .font_size;
            if let Some(previous) = last {
                assert_ne!(size, previous, "each typed digit commits a new size");
            }
            last = Some(size);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "text_bar_size p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "text bar size commit p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn text_stroke_popover_edits_the_layer_and_the_next_text_defaults(
    cx: &mut gpui_kit::TestAppContext,
) {
    use photrez_core::ptz::layer_params::TextStrokeAlign;

    let directory = TestDirectory::new();
    // The 1920x1080 client at 125% scaling is 1536 CSS px: the only width at
    // which the frozen option-bar geometry lays every text control in reach.
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1536.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);

        // Draw mode: the popover drives the next-text defaults.
        assert!(
            window.try_find("opt-text-stroke").is_some(),
            "the text bar exposes the stroke pill"
        );
        assert!(
            window.try_find("opt-text-stroke-popover").is_none(),
            "the popover starts closed"
        );
        window.click("opt-text-stroke", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .text_stroke_popover_open,
            "the pill opens the popover"
        );
        assert!(
            window.try_find("opt-text-stroke-popover").is_some(),
            "the pill mounts the popover"
        );
        window.click("opt-text-stroke-toggle", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.text_stroke_width, 4.0, "the switch enables at 4");
            assert_eq!(
                state.text_stroke_align,
                TextStrokeAlign::Outside,
                "position starts outside"
            );
        }
        window.click("opt-text-stroke", cx);
        window.render_frame(cx);

        // The created layer inherits the popover's defaults. The session starts
        // empty, so type one character before committing.
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        content.update(cx, |app, cx| {
            let _ = app.commit_text_edit(cx);
        });
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.stroke.width, 4.0, "the new layer carries the width");
        assert_eq!(data.stroke.color, "#000000", "and the default colour");
        assert_eq!(data.stroke.align, Some(TextStrokeAlign::Outside));

        // Edit mode: the popover edits the selected layer.
        window.click("opt-text-stroke", cx);
        window.render_frame(cx);
        window.click("opt-text-stroke-width", cx);
        for digit in ["9", "9", "9"] {
            window.press(digit, cx);
        }
        window.press("enter", cx);
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.stroke.width, 100.0, "width clamps to 100");
        assert_eq!(
            content.read(cx).state_for_test().read(cx).text_stroke_width,
            100.0,
            "editing the layer syncs the next-text width"
        );

        window.click("opt-text-stroke-width", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.stroke.width, 1.0, "the field floors at 1, not 0");

        window.click("opt-text-stroke-inc", cx);
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.stroke.width, 2.0, "the stepper adds one");

        let bounds = window.find("opt-text-stroke-slider").bounds();
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        assert!(width > 0.0 && height > 0.0, "the track is laid out");
        window.drag(
            bounds.origin + gpui_kit::point(gpui_kit::px(width * 0.25), gpui_kit::px(height * 0.5)),
            bounds.origin + gpui_kit::point(gpui_kit::px(width * 0.75), gpui_kit::px(height * 0.5)),
            cx,
        );
        window.render_frame(cx);
        let value = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists")
            .stroke
            .width;
        assert!(
            (value - 75.25).abs() <= 3.0,
            "dragging to 75% maps to the 1..100 track, got {value}"
        );

        window.click("opt-text-stroke-color", cx);
        window.render_frame(cx);
        assert!(
            window.find("color-picker").visible(),
            "the colour row opens the picker"
        );
        content.update(cx, |app, cx| {
            assert_eq!(
                app.state_for_test()
                    .read(cx)
                    .color_picker
                    .as_ref()
                    .map(|session| session.target),
                Some(crate::state::ColorTarget::TextStroke),
                "the picker edits the text stroke target"
            );
        });
        for digit in ["1", "2", "3", "4", "5", "6"] {
            window.press(digit, cx);
        }
        window.press("enter", cx);
        window.render_frame(cx);
        {
            let data = content
                .read(cx)
                .text_layer_data_for_test()
                .expect("text data exists");
            assert_eq!(data.stroke.color, "#123456");
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.text_stroke_color, [0x12, 0x34, 0x56, 255]);
            assert_eq!(
                state.selected_text_stroke_color,
                Some([0x12, 0x34, 0x56, 255])
            );
        }

        window.click("opt-text-stroke-inside", cx);
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.stroke.align, Some(TextStrokeAlign::Inside));
        assert_eq!(
            content.read(cx).state_for_test().read(cx).text_stroke_align,
            TextStrokeAlign::Inside
        );

        content.update(cx, |app, cx| {
            assert!(app.step_history(true, cx).expect("undo the position"));
        });
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(
            data.stroke.align,
            Some(TextStrokeAlign::Outside),
            "undo restores the previous position"
        );

        window.press("escape", cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .text_stroke_popover_open,
            "Escape closes the popover"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn properties_panel_shows_no_image_placeholder_without_a_document(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let doc_id = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .document_id
            .clone()
            .expect("fixture document id");
        window.click(format!("close-tab-{doc_id}"), cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).state_for_test().read(cx).has_document,
            "closing the last tab leaves no document"
        );
        assert!(
            window.try_find("properties-no-image").is_some(),
            "the Properties panel shows the no-image placeholder"
        );
        assert!(
            window.try_find("prop-flip-h").is_none(),
            "no layer actions without a document"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn properties_panel_falls_back_to_canvas_properties(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(120, 80, cx).expect("blank document");
            app.delete_selected_layer(cx);
        });
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layers.len(),
            0,
            "the last layer is gone"
        );
        assert!(
            window.try_find("canvas-size").is_some(),
            "the panel falls back to Canvas Properties"
        );
        window.click("canvas-size", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::ResizeCanvas),
            "the size field opens the Resize Canvas dialog"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn properties_panel_multi_selection_distributes_and_aligns(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(100, 100, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        drag_doc(window, (100, 100), (0.0, 10.0), (50.0, 60.0), cx);
        drag_doc(window, (100, 100), (30.0, 10.0), (80.0, 60.0), cx);
        drag_doc(window, (100, 100), (70.0, 10.0), (100.0, 60.0), cx);

        content.update(cx, |app, cx| {
            // Shape layers carry the document size at creation, so widen the
            // canvas afterwards to give align/distribute room to move.
            app.state_for_test().update(cx, |state, _| {
                state.resize_canvas = Some(crate::state::ResizeSession {
                    width_px: 500,
                    height_px: 100,
                    width: "500".to_string(),
                    height: "100".to_string(),
                    field: 0,
                    unit: crate::units::DimensionUnit::Px,
                    aspect: 1.0,
                    aspect_locked: false,
                    edited: Some(0),
                });
            });
            app.apply_resize_canvas(cx).expect("canvas resizes");
            // Layers project newest-first, so the first drag (0,10) is the
            // third shape row and the last drag (70,10) is the first.
            for (index, delta) in [(2usize, 0.0), (1, 90.0), (0, 230.0)] {
                let row = {
                    let state = app.state_for_test().read(cx);
                    state
                        .layers
                        .iter()
                        .filter(|row| row.name == "Shape")
                        .nth(index)
                        .map(|row| row.id)
                        .expect("shape row")
                };
                app.state_for_test().update(cx, |state, _| {
                    state.selected_layer = row;
                    state.selected_layers = vec![row];
                });
                app.project_layers(cx);
                if delta != 0.0 {
                    app.nudge_selected_layer(delta, 0.0, cx)
                        .expect("nudge applies");
                }
            }
            let rows: Vec<u64> = {
                let state = app.state_for_test().read(cx);
                state
                    .layers
                    .iter()
                    .filter(|row| row.name == "Shape")
                    .map(|row| row.id)
                    .collect()
            };
            assert_eq!(rows.len(), 3);
            app.state_for_test().update(cx, |state, _| {
                state.selected_layer = rows[0];
                state.selected_layers = rows;
            });
            app.project_layers(cx);
        });
        window.render_frame(cx);
        assert!(
            window.try_find("prop-align-left").is_some(),
            "the multi-selection inspector mounts"
        );

        window.click("prop-distribute-h", cx);
        let mut xs: Vec<f64> = content
            .read(cx)
            .core_layer_origins_for_test(LayerType::Shape)
            .into_iter()
            .map(|origin| origin.0)
            .collect();
        xs.sort_by(f64::total_cmp);
        for (actual, expected) in xs.iter().zip([0.0, 150.0, 300.0]) {
            assert!(
                (actual - expected).abs() < 1e-2,
                "free space is split evenly: {xs:?}"
            );
        }

        window.click("prop-align-center-h", cx);
        let xs = content
            .read(cx)
            .core_layer_origins_for_test(LayerType::Shape);
        assert!(
            xs.iter().all(|origin| (origin.0 - 200.0).abs() < 1e-6),
            "centre alignment lands every shape at (500 - 100) / 2: {xs:?}"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn selection_bar_shape_select_is_live(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Selection, 1.0);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selection_shape,
            crate::state::SelectionShape::Rect
        );
        window.click("opt-select-shape", cx);
        window.render_frame(cx);
        window.click("opt-select-shape-option-1", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selection_shape,
            crate::state::SelectionShape::Ellipse,
            "choosing Elliptical sets the marquee shape"
        );
        assert_eq!(content.read(cx).state_for_test().read(cx).open_select, None);

        window.click("opt-select-shape", cx);
        window.render_frame(cx);
        window.press("escape", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            None,
            "Escape closes the shape dropdown"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn brush_bar_numeric_fields_are_live(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        window.click("opt-brush-size", cx);
        window.press("8", cx);
        window.press("0", cx);
        window.press("enter", cx);
        assert_eq!(content.read(cx).state_for_test().read(cx).brush_size, 80.0);
        window.click("opt-brush-hardness", cx);
        window.press("2", cx);
        window.press("5", cx);
        window.press("enter", cx);
        assert!((content.read(cx).state_for_test().read(cx).brush_hardness - 0.25).abs() < 1e-6);
        window.click("opt-more", cx);
        window.render_frame(cx);
        window.click("opt-brush-flow", cx);
        window.press("5", cx);
        window.press("0", cx);
        window.press("enter", cx);
        assert!((content.read(cx).state_for_test().read(cx).brush_flow - 0.5).abs() < 1e-6);
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .brush_preset_index,
            usize::MAX,
            "manual edits clear the preset"
        );

        set_tool(&content, cx, ToolId::Eraser, 1.0);
        window.render_frame(cx);
        // The overflow panel stays open across the tool switch.
        window.click("opt-brush-smoothing", cx);
        window.press("3", cx);
        window.press("0", cx);
        window.press("enter", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).eraser_smoothing,
            30.0
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn select_supports_arrow_navigation_and_click_away(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        window.click("opt-more", cx);
        window.render_frame(cx);
        window.click("opt-brush-preset", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            Some("opt-brush-preset")
        );
        window.press("down", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).select_cursor,
            1,
            "ArrowDown moves the highlight"
        );
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(content.read(cx).state_for_test().read(cx).open_select, None);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).brush_size,
            40.0,
            "Enter commits the highlighted preset"
        );

        window.click("opt-brush-preset", cx);
        window.render_frame(cx);
        window.click("canvas-hitbox", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).open_select,
            None,
            "clicking the canvas closes the dropdown"
        );
        assert!(
            !content.read(cx).state_for_test().read(cx).more_open,
            "clicking the canvas closes the overflow panel too"
        );
    })
    .expect("test window remains open");
}

/// Below the oracle's 880 px container breakpoint the brush bar folds Flow,
/// Smoothing, Presets and the eraser's full-strength helper into `MoreDropdown`;
/// at 1366x820 the bar resolves to 781 px, so this is the live layout.
#[gpui_kit::test]
fn brush_overflow_folds_into_the_more_dropdown_below_880(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more").is_some(),
            "the overflow button mounts"
        );
        for id in [
            "opt-brush-flow",
            "opt-brush-smoothing",
            "opt-brush-preset",
            "opt-brush-hard100",
        ] {
            assert!(
                window.try_find(id).is_none(),
                "{id} must stay in the overflow at 781 px"
            );
        }

        window.click("opt-more", cx);
        window.render_frame(cx);
        for id in [
            "opt-more-panel",
            "opt-brush-flow",
            "opt-brush-smoothing",
            "opt-brush-preset",
        ] {
            assert!(
                window.try_find(id).is_some(),
                "the overflow panel carries {id}"
            );
        }
        assert!(
            window.try_find("opt-more-hard100").is_none(),
            "the full-strength helper is eraser only"
        );
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more-panel").is_none(),
            "Escape closes the overflow panel"
        );

        set_tool(&content, cx, ToolId::Eraser, 1.0);
        window.render_frame(cx);
        window.click("opt-more", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more-hard100").is_some(),
            "the eraser overflow carries Hard 100"
        );
        set_state(&content, cx, |state| {
            state.eraser_hardness = 0.2;
            state.eraser_opacity = 0.3;
            state.eraser_flow = 0.4;
            state.brush_preset_index = 2;
        });
        window.render_frame(cx);
        window.click("opt-more-hard100", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.eraser_hardness, 1.0, "Hard 100 sets full hardness");
        assert_eq!(state.eraser_opacity, 1.0, "Hard 100 sets full strength");
        assert_eq!(state.eraser_flow, 1.0, "Hard 100 sets full flow");
        assert_eq!(
            state.brush_preset_index,
            usize::MAX,
            "Hard 100 clears the preset to Custom"
        );
    })
    .expect("test window remains open");
}

/// S061: the Snap toggle's tooltip (hover -> panel paint).
#[gpui_kit::test]
#[ignore = "performance measurement; run explicitly"]
fn snap_tooltip_hover_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        // Warm once; the first hover pays the panel mount cost.
        window.hover("opt-snap", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-snap-tooltip").is_some(),
            "the warm-up hover shows the Snap tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let start = std::time::Instant::now();
            window.hover("opt-snap", cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(
                window.try_find("opt-snap-tooltip").is_some(),
                "sample {sample} shows the Snap tooltip"
            );
            window.hover("canvas-hitbox", cx);
            window.render_frame(cx);
        }
        let _ = &content;
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "S061_SNAP_TOOLTIP_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
        assert!(
            sorted[8] <= 33.3,
            "Snap tooltip hover p95 {:.3} ms exceeds the 33.3 ms budget",
            sorted[8]
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn option_bar_controls_show_hover_tooltips(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        window.hover("opt-shape-kind", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-shape-kind-tooltip").is_some(),
            "hovering the shape control shows its tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-shape-kind-tooltip").is_none(),
            "leaving the control hides the tooltip"
        );
        assert!(
            window.try_find("opt-fill-color").is_some(),
            "Fill defaults on, so the swatch is mounted"
        );
        window.hover("opt-fill-color", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-fill-color-tooltip").is_some(),
            "the fill swatch carries the oracle fill-color tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        window.click("opt-shape-stroke", cx);
        window.render_frame(cx);
        window.click("opt-stroke-enable", cx);
        window.render_frame(cx);
        window.click("opt-fill", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-fill-color").is_none(),
            "the oracle unmounts the fill swatch once Fill is off"
        );
        window.click("opt-fill", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-fill-color").is_some(),
            "re-enabling Fill remounts the swatch"
        );
        window.hover("opt-shape-stroke", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-shape-stroke-tooltip").is_some(),
            "the stroke pill carries the oracle stroke-options tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);

        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        window.hover("opt-weight", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-weight-tooltip").is_some(),
            "the weight control carries its own tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        window.hover("opt-italic", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-italic-tooltip").is_some(),
            "the italic button carries the oracle tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        window.hover("opt-align-left", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-align-left-tooltip").is_some(),
            "the align-left button carries the oracle tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);

        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        window.click("opt-more", cx);
        window.render_frame(cx);
        window.hover("opt-brush-preset", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-brush-preset-tooltip").is_some(),
            "the brush preset control carries the oracle tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-brush-preset-tooltip").is_none(),
            "leaving the preset control hides its tooltip"
        );

        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        window.hover("opt-auto-select", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-auto-select-tooltip").is_some(),
            "the auto-select toggle carries the oracle tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-auto-select-tooltip").is_none(),
            "leaving the auto-select toggle hides its tooltip"
        );
        window.hover("opt-snap", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-snap-tooltip").is_some(),
            "the snap toggle carries the oracle tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        window.hover("opt-transform", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-transform-tooltip").is_some(),
            "the transform toggle carries the oracle tooltip"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn typography_section_edits_the_selected_text_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("h", cx);
        window.press("i", cx);
        window.press("ctrl-enter", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("properties-typography").is_some(),
            "a text selection shows the Typography section"
        );
        assert!(
            window.try_find("prop-text-italic").is_some()
                && window.try_find("prop-text-align-center").is_some()
                && window.try_find("prop-text-color").is_some()
                && window.try_find("prop-text-stroke").is_some(),
            "the typography controls are mounted"
        );

        window.click("prop-text-align-center", cx);
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(
            data.align,
            photrez_core::ptz::layer_params::TextAlign::Center
        );

        window.click("prop-text-stroke", cx);
        window.render_frame(cx);
        let data = content
            .read(cx)
            .text_layer_data_for_test()
            .expect("text data exists");
        assert_eq!(data.stroke.width, 4.0, "the stroke toggle enables at 4");

        window.click("prop-text-color", cx);
        window.render_frame(cx);
        assert!(
            window.find("color-picker").visible(),
            "the typography swatch opens the text colour picker"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn text_typing_preview_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
        tab_to_canvas(window, cx);
        window.press("a", cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for i in 0..9 {
            let letter = if i % 2 == 0 { "b" } else { "c" };
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
                    keystroke: gpui_kit::Keystroke {
                        modifiers: Default::default(),
                        key: letter.to_string(),
                        key_char: Some(letter.to_string()),
                    },
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        let typed = content
            .read(cx)
            .text_edit_content_for_test(cx)
            .expect("editor open");
        assert_eq!(
            typed.chars().count(),
            10,
            "every keystroke reached the buffer"
        );
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "text_typing_preview p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "text typing preview p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}
