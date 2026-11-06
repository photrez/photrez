use super::canvas_input::{
    drag_doc, layer_rows, mount_archive_app, page, pixels, select_layer, set_tool, tab_to_canvas,
    window_point,
};
use super::*;
use crate::state::{ShapeTool, ToolId};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};
use photrez_core::document::content::{LayerType, SelectionShape, SelectionState};
use photrez_core::document::layer_params::{ShapeFillKind, ShapeKind};

mod brush_preview_and_move;
mod latency;
mod layer_edits;
mod option_bar_live;
mod text_and_shape_persistence;

pub(super) fn set_state(
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
    use photrez_core::raster::brush::tip::get_brush_cursor_radius_scale;
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
            "the shape tool selects the layer it creates"
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
        let (rows, _) = super::canvas_input::layer_rows(&content, cx);
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

/// Opening the selection shape menu (dispatch + render).
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
            "SELECT_SHAPE_MS samples={samples:?} p50={:.3} p95={:.3}",
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

/// The Snap toggle's tooltip (hover -> panel paint).
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
            "SNAP_TOOLTIP_MS samples={samples:?} p50={:.3} p95={:.3}",
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
            photrez_core::document::layer_params::TextAlign::Center
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

/// The 0.1 build gates its rest transform frame on a multi-layer selection,
/// so one selected layer under Move paints no frame at all. Measured: the
/// 0.1 Move frame holds 0 accent pixels inside the canvas box; this
/// build measured 2,653 before that gate landed.
#[gpui_kit::test]
fn move_with_one_selected_layer_paints_no_rest_transform_frame(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 1.0);
        window.render_frame(cx);
        let selected = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .selected_layers
            .len();
        assert_eq!(selected, 1, "the fixture selects exactly one layer");
        assert!(
            content
                .update(cx, |app, cx| app.transform_frame_for_test(cx))
                .is_none(),
            "Move at rest must not draw the frame the 0.1 build reserves for a group"
        );
    })
    .expect("test window remains open");
}
