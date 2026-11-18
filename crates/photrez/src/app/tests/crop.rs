use super::canvas_input::{drag_doc, page, pixels, set_tool, tab_to_canvas, window_point};
use super::*;
use crate::app::canvas_input::SetCanvasTool;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};

fn dispatch(window: &mut gpui_kit::Window, event: PlatformInput, cx: &mut gpui_kit::App) {
    let _ = window.dispatch_event(event, cx);
    window.render_frame(cx);
}

fn crop_pointer_drag(
    window: &mut gpui_kit::Window,
    start: gpui_kit::Point<gpui_kit::Pixels>,
    end: gpui_kit::Point<gpui_kit::Pixels>,
    cx: &mut gpui_kit::App,
) {
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
            pressed_button: Some(MouseButton::Left),
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
}

fn crop_pasteboard_point(
    window: &mut gpui_kit::Window,
    doc: (u32, u32),
) -> gpui_kit::Point<gpui_kit::Pixels> {
    let (bounds, (page_x, page_y, page_w, page_h)) = page(window, doc);
    let viewport_width: f32 = bounds.size.width.into();
    let viewport_height: f32 = bounds.size.height.into();
    let (x, y) = if page_y > 2.0 {
        (page_x + page_w / 2.0, page_y / 2.0)
    } else {
        (page_x / 2.0, page_y + page_h / 2.0)
    };
    assert!(x >= 0.0 && x < viewport_width && y >= 0.0 && y < viewport_height);
    assert!(x < page_x || x >= page_x + page_w || y < page_y || y >= page_y + page_h);
    bounds.origin + gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y))
}

fn double_click_canvas_at(
    window: &mut gpui_kit::Window,
    position: gpui_kit::Point<gpui_kit::Pixels>,
    cx: &mut gpui_kit::App,
) {
    for click_count in [1, 2] {
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                click_count,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position,
                ..Default::default()
            }),
            cx,
        );
    }
}

#[gpui_kit::test]
fn crop_drag_shows_a_preview_and_escape_clears_it(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.bg_color = [24, 48, 72, 255];
                state.crop_fill_background = true;
            });
        });
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
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_none(),
            "a Modern drag preview must not commit the crop frame"
        );
        let overlay = content.update(cx, |app, cx| app.canvas_overlay(cx));
        let preview = overlay
            .crop_create_preview
            .expect("the overlay draws the Modern drag preview");
        assert!(preview.2 > 0. && preview.3 > 0., "preview={preview:?}");
        assert!(
            overlay.crop.is_none(),
            "the committed frame chrome is hidden"
        );
        assert_eq!(
            overlay.crop_fill_color,
            Some([24, 48, 72, 255]),
            "Fill BG follows the live Modern creation frame"
        );
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
        assert!(
            content
                .update(cx, |app, cx| app.canvas_overlay(cx))
                .crop_create_preview
                .is_none(),
            "Escape clears the Modern drag preview"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_creation_snaps_the_preview_and_commits_its_geometry(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (800, 600);
    let camera =
        crate::display::canvas_bridge::Camera::new(0.0, 0.0, 1.0).expect("valid test camera");
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.set_camera(camera, cx);
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_mode = crate::crop_aspect::CropMode::Free;
                state.snap_enabled = true;
            });
        });
        window.render_frame(cx);

        let bounds = window.find("canvas-hitbox").bounds();
        let start = bounds.origin + gpui_kit::point(gpui_kit::px(8.0), gpui_kit::px(8.0));
        let end = bounds.origin + gpui_kit::point(gpui_kit::px(160.0), gpui_kit::px(120.0));
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
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        let overlay = content.update(cx, |app, cx| app.canvas_overlay(cx));
        let preview = overlay
            .crop_create_preview
            .expect("the Modern drag preview is visible");
        let preview = [
            preview.0 - f64::from(bounds.origin.x),
            preview.1 - f64::from(bounds.origin.y),
            preview.2,
            preview.3,
        ];
        let expected_preview = [0.0, 0.0, 160.0, 120.0];
        assert!(
            preview
                .into_iter()
                .zip(expected_preview)
                .all(|(actual, expected)| (actual - expected).abs() < 0.01),
            "preview={preview:?}, expected={expected_preview:?}"
        );
        assert!(
            content
                .read(cx)
                .snap_guides_for_test()
                .iter()
                .any(|(vertical, position)| *vertical && position.abs() < 1e-6),
            "the crop publishes its vertical snap guide"
        );
        assert!(
            content
                .read(cx)
                .snap_guides_for_test()
                .iter()
                .any(|(vertical, position)| !vertical && position.abs() < 1e-6),
            "the crop publishes its horizontal snap guide"
        );

        let camera = content.update(cx, |app, _| app.effective_camera().expect("camera"));
        let viewport = (f64::from(bounds.size.width), f64::from(bounds.size.height));
        let expected =
            crate::crop_geometry::modern_crop_create(crate::crop_geometry::ModernCropCreateInput {
                start: (0.0, 0.0),
                end: (160.0, 120.0),
                viewport,
                camera: crate::crop_geometry::CropViewport {
                    pan_x: f64::from(camera.x),
                    pan_y: f64::from(camera.y),
                    zoom: f64::from(camera.zoom),
                },
                mode: crate::crop_aspect::CropMode::Free,
                aspect: None,
                size_target: None,
                shift: false,
                rotation: 0.0,
            });
        let _ = window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            }),
            cx,
        );
        let state = content.read(cx).state_for_test();
        let actual = state.read(cx).crop_rect.expect("created crop frame");
        assert!(
            actual
                .into_iter()
                .zip(expected.frame)
                .all(|(actual, expected)| (actual - expected).abs() < 0.001),
            "actual={actual:?}, expected={:?}",
            expected.frame
        );
        let actual_offset = state.read(cx).crop_offset;
        assert!(
            (actual_offset.0 - expected.offset.0).abs() < 0.001
                && (actual_offset.1 - expected.offset.1).abs() < 0.001,
            "actual={actual_offset:?}, expected={:?}",
            expected.offset
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_creation_snaps_before_removing_camera_pan_at_fractional_zoom(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (800, 600);
    let camera =
        crate::display::canvas_bridge::Camera::new(30.0, 20.0, 1.25).expect("valid test camera");
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.set_camera(camera, cx);
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_mode = crate::crop_aspect::CropMode::Free;
                state.snap_enabled = true;
            });
        });
        window.render_frame(cx);
        let bounds = window.find("canvas-hitbox").bounds();
        let point = |x, y| bounds.origin + gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y));
        let start = point(475.0, 350.0);
        let end = point(525.0, 400.0);
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
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );

        let preview = content
            .update(cx, |app, cx| app.canvas_overlay(cx))
            .crop_create_preview
            .expect("Modern drag preview");
        let preview = [
            preview.0 - f64::from(bounds.origin.x),
            preview.1 - f64::from(bounds.origin.y),
            preview.2,
            preview.3,
        ];
        let expected_preview = [475.0, 350.0, 50.0, 50.0];
        assert!(
            preview
                .into_iter()
                .zip(expected_preview)
                .all(|(actual, expected)| (actual - expected).abs() < 0.01),
            "preview={preview:?}, expected={expected_preview:?}"
        );
        let guides = content.read(cx).snap_guides_for_test();
        assert!(guides.contains(&(true, 400.0)));
        assert!(guides.contains(&(false, 300.0)));

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            }),
            cx,
        );
        let state = content.read(cx).state_for_test();
        let state = state.read(cx);
        let viewport_width = f64::from(bounds.size.width);
        let viewport_height = f64::from(bounds.size.height);
        let expected_frame = [
            ((viewport_width / 2.0 - f64::from(camera.x)) / f64::from(camera.zoom) - 49.5).floor(),
            ((viewport_height / 2.0 - f64::from(camera.y)) / f64::from(camera.zoom) - 49.5).floor(),
            100.0,
            100.0,
        ];
        assert_eq!(state.crop_rect, Some(expected_frame));
        assert_eq!(
            state.crop_offset,
            (viewport_width / 2.0 - 500.0, viewport_height / 2.0 - 375.0)
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_move_snaps_to_document_targets(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (400, 300);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some([130.0, 100.0, 100.0, 80.0]);
                state.snap_enabled = true;
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 150.0, 140.0);
        let end = window_point(bounds, placement, doc, 153.0, 143.0);
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
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );

        let frame = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert!(
            (frame[0] - doc.0 as f64 / 3.0).abs() < 0.1,
            "frame={frame:?}"
        );
        assert!(
            (frame[1] - doc.1 as f64 / 3.0).abs() < 0.1,
            "frame={frame:?}"
        );
        assert!(
            content
                .read(cx)
                .snap_guides_for_test()
                .iter()
                .any(|(vertical, position)| *vertical
                    && (*position - doc.0 as f64 / 3.0).abs() < 0.1)
        );
        assert!(
            content
                .read(cx)
                .snap_guides_for_test()
                .iter()
                .any(|(vertical, position)| !*vertical
                    && (*position - doc.1 as f64 / 3.0).abs() < 0.1)
        );

        let release = window_point(bounds, placement, doc, 158.0, 148.0);
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: release,
                ..Default::default()
            }),
            cx,
        );
        let committed = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert!(
            committed
                .into_iter()
                .zip(frame)
                .all(|(committed, moved)| (committed - moved).abs() < 0.1),
            "release must retain the last move sample: {committed:?} vs {frame:?}"
        );
        assert!(content.read(cx).snap_guides_for_test().is_empty());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_snap_targets_use_shape_bitmap_bounds(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (400, 200);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
        });
        set_tool(&content, cx, ToolId::Shape, 1.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.shape_kind = crate::state::ShapeTool::Line;
                state.shape_stroke = true;
                state.shape_stroke_width = 3.25;
                state.shape_arrow_head = true;
            });
        });
        drag_doc(window, doc, (50.0, 40.0), (150.0, 40.0), cx);

        let params = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .selected_shape
            .clone()
            .expect("created shape params");
        let (width, height, _) =
            photrez_core::render::rasterize_shape_preview(&params).expect("shape bitmap");
        let (doc_id, layer_id) = {
            let app = content.read(cx);
            let doc_id = app.active_doc_id().expect("active document");
            let layer_id = app
                .sessions
                .get(&doc_id)
                .expect("active session")
                .loaded_document()
                .content_snapshot()
                .expect("document content")
                .layers
                .iter()
                .find(|layer| layer.layer_type == photrez_core::document::content::LayerType::Shape)
                .map(|layer| layer.id.clone())
                .expect("created shape layer");
            (doc_id, layer_id)
        };
        let transform = photrez_core::document::content::Transform2D {
            x: 37.0,
            y: 27.0,
            scale_x: 1.25,
            scale_y: 0.8,
            rotation: 23.0,
            flip_h: false,
            flip_v: false,
        };
        content.update(cx, |app, cx| {
            app.apply_core_command(
                &doc_id,
                photrez_core::protocol::Command::TransformLayer {
                    id: layer_id.clone(),
                    transform: photrez_core::protocol::TransformPatch {
                        x: transform.x,
                        y: transform.y,
                        scale_x: transform.scale_x,
                        scale_y: transform.scale_y,
                        rotation: transform.rotation,
                        flip_h: Some(transform.flip_h),
                        flip_v: Some(transform.flip_v),
                    },
                },
                true,
                cx,
            )
            .expect("transform shape layer");
        });
        let (x, y, aabb_width, aabb_height) = crate::app::transform_geometry::layer_aabb(
            &transform,
            f64::from(width),
            f64::from(height),
        );
        let targets = content
            .update(cx, |app, _| crate::app::crop::crop_snap_targets(app))
            .expect("visible shape snap targets");
        assert!(params.arrow_head);
        assert!(width > params.width as u32);
        assert!(height > params.height as u32);
        assert!(targets.x.contains(&x));
        assert!(targets.x.contains(&(x + aabb_width / 2.0)));
        assert!(targets.x.contains(&(x + aabb_width)));
        assert!(targets.y.contains(&y));
        assert!(targets.y.contains(&(y + aabb_height / 2.0)));
        assert!(targets.y.contains(&(y + aabb_height)));

        content.update(cx, |app, cx| {
            app.apply_core_command(
                &doc_id,
                photrez_core::protocol::Command::SetVisible {
                    id: layer_id,
                    visible: false,
                },
                true,
                cx,
            )
            .expect("hide shape layer");
        });
        let hidden_targets = content
            .update(cx, |app, _| crate::app::crop::crop_snap_targets(app))
            .expect("document snap targets");
        assert!(!hidden_targets.x.contains(&(x + aabb_width)));
        assert!(!hidden_targets.y.contains(&(y + aabb_height)));
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_resize_snaps_the_dragged_edges(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (400, 300);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some([130.0, 100.0, 68.0, 50.0]);
                state.snap_enabled = true;
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 198.0, 150.0);
        let end = window_point(bounds, placement, doc, 202.0, 154.0);
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
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );

        let frame = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert!((frame[0] - 130.0).abs() < 0.1, "frame={frame:?}");
        assert!((frame[1] - 100.0).abs() < 0.1, "frame={frame:?}");
        assert!((frame[0] + frame[2] - 200.0).abs() < 0.1, "frame={frame:?}");
        assert!((frame[1] + frame[3] - 150.0).abs() < 0.1, "frame={frame:?}");

        let release = window_point(bounds, placement, doc, 208.0, 160.0);
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: release,
                ..Default::default()
            }),
            cx,
        );
        let committed = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert!(
            committed
                .into_iter()
                .zip(frame)
                .all(|(committed, moved)| (committed - moved).abs() < 0.1),
            "release must retain the last move sample: {committed:?} vs {frame:?}"
        );
        assert!(content.read(cx).snap_guides_for_test().is_empty());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_move_bypasses_snapping_for_alt_and_disabled_setting(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (400, 300);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some([130.0, 100.0, 100.0, 80.0]);
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, doc);
        for (alt, snap_enabled) in [(true, true), (false, false)] {
            content.update(cx, |app, cx| {
                app.state_for_test().update(cx, |state, _| {
                    state.crop_rect = Some([130.0, 100.0, 100.0, 80.0]);
                    state.snap_enabled = snap_enabled;
                });
            });
            let start = window_point(bounds, placement, doc, 150.0, 140.0);
            let end = window_point(bounds, placement, doc, 153.0, 143.0);
            let modifiers = gpui_kit::Modifiers {
                alt,
                ..Default::default()
            };
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
                    pressed_button: Some(MouseButton::Left),
                    modifiers,
                }),
                cx,
            );

            let frame = content.read(cx).crop_rect_for_test(cx).expect("frame");
            assert!((frame[0] - 133.0).abs() < 0.1, "frame={frame:?}, alt={alt}");
            assert!((frame[1] - 103.0).abs() < 0.1, "frame={frame:?}, alt={alt}");
            assert!(content.read(cx).snap_guides_for_test().is_empty());

            let release = window_point(bounds, placement, doc, 158.0, 148.0);
            dispatch(
                window,
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: release,
                    ..Default::default()
                }),
                cx,
            );
        }
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_snapping_tracks_alt_and_setting_changes_during_drag(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (400, 300);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some([130.0, 100.0, 100.0, 80.0]);
                state.snap_enabled = true;
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 150.0, 140.0);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );

        for (x, y, modifiers) in [
            (153.0, 143.0, gpui_kit::Modifiers::default()),
            (
                158.0,
                148.0,
                gpui_kit::Modifiers {
                    alt: true,
                    ..Default::default()
                },
            ),
        ] {
            let position = window_point(bounds, placement, doc, x, y);
            dispatch(
                window,
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    pressed_button: Some(MouseButton::Left),
                    modifiers,
                }),
                cx,
            );
            let frame = content.read(cx).crop_rect_for_test(cx).expect("frame");
            if modifiers.alt {
                assert!((frame[0] - 138.0).abs() < 0.1, "frame={frame:?}");
                assert!((frame[1] - 108.0).abs() < 0.1, "frame={frame:?}");
                assert!(content.read(cx).snap_guides_for_test().is_empty());
            } else {
                assert!(!content.read(cx).snap_guides_for_test().is_empty());
            }
        }

        let position = window_point(bounds, placement, doc, 159.0, 149.0);
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        let frame = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert_ne!(frame, [139.0, 109.0, 100.0, 80.0]);
        assert!(!content.read(cx).snap_guides_for_test().is_empty());

        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.snap_enabled = false);
        });
        let position = window_point(bounds, placement, doc, 161.0, 151.0);
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        let frame = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert!((frame[0] - 141.0).abs() < 0.1, "frame={frame:?}");
        assert!((frame[1] - 111.0).abs() < 0.1, "frame={frame:?}");
        assert!(content.read(cx).snap_guides_for_test().is_empty());

        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.snap_enabled = true);
        });
        let position = window_point(bounds, placement, doc, 165.0, 150.0);
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        let frame = content.read(cx).crop_rect_for_test(cx).expect("frame");
        assert!((frame[0] - 150.0).abs() < 0.1, "frame={frame:?}");
        assert!((frame[1] - 110.0).abs() < 0.1, "frame={frame:?}");
        assert!(
            content
                .read(cx)
                .snap_guides_for_test()
                .contains(&(true, 200.0))
        );

        let release = window_point(bounds, placement, doc, 180.0, 170.0);
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: release,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some([150.0, 110.0, 100.0, 80.0])
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_resize_bypasses_snapping_for_alt_and_disabled_setting(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (400, 300);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, doc);
        for (alt, snap_enabled) in [(true, true), (false, false)] {
            content.update(cx, |app, cx| {
                app.state_for_test().update(cx, |state, _| {
                    state.crop_rect = Some([130.0, 100.0, 68.0, 50.0]);
                    state.snap_enabled = snap_enabled;
                });
            });
            let start = window_point(bounds, placement, doc, 198.0, 150.0);
            let end = window_point(bounds, placement, doc, 202.0, 154.0);
            let modifiers = gpui_kit::Modifiers {
                alt,
                ..Default::default()
            };
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
                    pressed_button: Some(MouseButton::Left),
                    modifiers,
                }),
                cx,
            );
            let expected = if alt {
                [126.0, 96.0, 76.0, 58.0]
            } else {
                [130.0, 100.0, 72.0, 54.0]
            };
            assert_eq!(content.read(cx).crop_rect_for_test(cx), Some(expected));
            assert!(content.read(cx).snap_guides_for_test().is_empty());

            dispatch(
                window,
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: window_point(bounds, placement, doc, 208.0, 160.0),
                    ..Default::default()
                }),
                cx,
            );
        }
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_cancels_a_snapped_classic_crop_drag_and_clears_guides(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (400, 300);
    let start_frame = [130.0, 100.0, 100.0, 80.0];
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some(start_frame);
                state.snap_enabled = true;
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 150.0, 140.0);
        let end = window_point(bounds, placement, doc, 153.0, 143.0);
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
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        assert!(!content.read(cx).snap_guides_for_test().is_empty());

        window.press("escape", cx);
        window.render_frame(cx);
        assert_eq!(content.read(cx).crop_rect_for_test(cx), Some(start_frame));
        assert!(content.read(cx).snap_guides_for_test().is_empty());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_idle_escape_discards_the_hidden_preview(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let hidden = crate::crop_geometry::CropSnapshot {
        frame: [20.0, 10.0, 80.0, 40.0],
        offset: (0.0, 0.0),
        rotation: 12.0,
    };
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 100, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_hidden_preview = Some(hidden);
                state.crop_rotation = 12.0;
            });
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("escape", cx);
        window.render_frame(cx);

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).active_tool, ToolId::Crop);
        assert_eq!(state.read(cx).crop_rect, None);
        assert_eq!(state.read(cx).crop_hidden_preview, None);
        assert_eq!(state.read(cx).crop_rotation, 0.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_idle_escape_resets_preview_transform_and_history(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let entry = crate::crop_geometry::CropSnapshot {
        frame: [10.0, 10.0, 80.0, 40.0],
        offset: (2.0, 3.0),
        rotation: 4.0,
    };
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 100, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_rect = Some([20.0, 10.0, 80.0, 40.0]);
                state.crop_hidden_preview = Some(entry);
                state.crop_offset = (7.0, -3.0);
                state.crop_rotation = 12.0;
                state.crop_history = vec![entry];
                state.crop_redo = vec![entry];
            });
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("escape", cx);
        window.render_frame(cx);

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).active_tool, ToolId::Crop);
        assert_eq!(state.read(cx).crop_rect, None);
        assert_eq!(state.read(cx).crop_hidden_preview, None);
        assert_eq!(state.read(cx).crop_offset, (0.0, 0.0));
        assert_eq!(state.read(cx).crop_rotation, 0.0);
        assert!(state.read(cx).crop_history.is_empty());
        assert!(state.read(cx).crop_redo.is_empty());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_handle_drag_is_cancelled_by_tool_shortcut(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.set_canvas_tool_action(&SetCanvasTool(ToolId::Crop), cx);
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);

        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.1, 0.1);
        let moved = window_point(bounds, placement, doc, 0.7, 0.7);
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
                position: moved,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        assert!(content.read(cx).crop_rect_for_test(cx).is_some());
        content.update(cx, |app, _| assert!(app.crop_drag.is_some()));

        window.press("v", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Move
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: window_point(bounds, placement, doc, 0.9, 0.8),
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_none(),
            "a crop drag must not restore its frame after the tool changes"
        );

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, cx| {
            assert!(app.crop_drag.is_none());
            assert!(app.state_for_test().read(cx).crop_history.is_empty());
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_handle_drag_ignores_the_stale_release_after_tool_switch(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (400, 300);
    let start_frame = [130.0, 100.0, 68.0, 50.0];
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some(start_frame);
                state.snap_enabled = false;
            });
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 198.0, 150.0);
        let moved = window_point(bounds, placement, doc, 202.0, 154.0);
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
                position: moved,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        assert_ne!(content.read(cx).crop_rect_for_test(cx), Some(start_frame));
        content.update(cx, |app, _| assert!(app.crop_drag.is_some()));

        window.press("v", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Move
        );
        assert!(content.read(cx).crop_rect_for_test(cx).is_none());
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: window_point(bounds, placement, doc, 208.0, 160.0),
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        assert!(content.read(cx).crop_rect_for_test(cx).is_none());
        content.update(cx, |app, cx| {
            assert!(app.crop_drag.is_none());
            assert!(app.pending_gesture.is_none());
            assert!(app.state_for_test().read(cx).crop_history.is_empty());
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_handle_drag_ignores_the_stale_release_after_same_document_reload(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        let (width, height, _) = pixels(&content, cx);
        let doc = (width, height);
        let start_frame = [
            f64::from(width) * 0.2,
            f64::from(height) * 0.2,
            f64::from(width) * 0.4,
            f64::from(height) * 0.4,
        ];
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some(start_frame);
                state.crop_history.clear();
                state.crop_redo.clear();
                state.snap_enabled = false;
            });
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);

        let (bounds, placement) = page(window, doc);
        let start_doc = (width as f32 * 0.6, height as f32 * 0.6);
        let moved_doc = (width as f32 * 0.64, height as f32 * 0.64);
        let start = window_point(bounds, placement, doc, start_doc.0, start_doc.1);
        let moved = window_point(bounds, placement, doc, moved_doc.0, moved_doc.1);
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
                position: moved,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        assert_ne!(content.read(cx).crop_rect_for_test(cx), Some(start_frame));
        content.update(cx, |app, _| assert!(app.crop_drag.is_some()));

        let original_id = content
            .read(cx)
            .active_doc_id()
            .expect("active document id");
        content.update(cx, |app, cx| {
            let file_io = app.file_io.clone().expect("file boundary is open");
            app.open_startup_document_for_test(&source, file_io.as_ref(), cx)
                .expect("the active document reloads");
            assert_eq!(app.active_doc_id().as_deref(), Ok(original_id.as_str()));
            assert!(app.crop_drag.is_none());
        });
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some([0.0, 0.0, f64::from(width), f64::from(height)])
        );

        let stale_move = window_point(
            bounds,
            placement,
            doc,
            width as f32 * 0.68,
            height as f32 * 0.68,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: stale_move,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some([0.0, 0.0, f64::from(width), f64::from(height)])
        );
        content.update(cx, |app, cx| {
            assert!(app.crop_drag.is_none());
            assert!(app.pending_gesture.is_none());
            assert!(app.state_for_test().read(cx).crop_history.is_empty());
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_cancels_an_active_crop_handle_drag(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.set_canvas_tool_action(&SetCanvasTool(ToolId::Crop), cx);
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let start_crop = content
            .read(cx)
            .crop_rect_for_test(cx)
            .expect("default crop frame");
        let history_entry = crate::crop_geometry::CropSnapshot {
            frame: [0.0, 0.0, 1.0, 1.0],
            offset: (1.0, 2.0),
            rotation: 3.0,
        };
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_offset = (7.0, 11.0);
                state.crop_rotation = 12.0;
                state.crop_history = vec![history_entry];
                state.crop_redo = vec![history_entry];
            });
        });
        window.render_frame(cx);

        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.1, 0.1);
        let moved = window_point(bounds, placement, doc, 0.7, 0.7);
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
                position: moved,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, _| assert!(app.crop_drag.is_some()));

        window.press("escape", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert!(app.crop_drag.is_none());
            let state = app.state_for_test();
            assert_eq!(state.read(cx).crop_rect, Some(start_crop));
            assert_eq!(state.read(cx).crop_offset, (7.0, 11.0));
            assert_eq!(state.read(cx).crop_rotation, 12.0);
            assert_eq!(state.read(cx).crop_history, vec![history_entry]);
            assert_eq!(state.read(cx).crop_redo, vec![history_entry]);
        });

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, cx| {
            assert!(app.crop_drag.is_none());
            let state = app.state_for_test();
            assert_eq!(state.read(cx).crop_rect, Some(start_crop));
            assert_eq!(state.read(cx).crop_offset, (7.0, 11.0));
            assert_eq!(state.read(cx).crop_rotation, 12.0);
            assert_eq!(state.read(cx).crop_history, vec![history_entry]);
            assert_eq!(state.read(cx).crop_redo, vec![history_entry]);
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_creation_bypasses_snapping_for_alt_and_disabled_setting(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (800, 600);
    let camera =
        crate::display::canvas_bridge::Camera::new(0.0, 0.0, 1.0).expect("valid test camera");
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.set_camera(camera, cx);
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_mode = crate::crop_aspect::CropMode::Free;
                state.crop_rect = None;
            });
        });
        window.render_frame(cx);
        let bounds = window.find("canvas-hitbox").bounds();
        let point = |x, y| bounds.origin + gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y));
        let start = point(8.0, 8.0);
        let end = point(160.0, 120.0);
        let resume = point(161.0, 121.0);

        for (alt, snap_enabled) in [(true, true), (false, false)] {
            content.update(cx, |app, cx| {
                app.state_for_test().update(cx, |state, _| {
                    state.crop_rect = None;
                    state.crop_hidden_preview = None;
                    state.crop_offset = (0.0, 0.0);
                    state.crop_rotation = 0.0;
                    state.crop_history.clear();
                    state.crop_redo.clear();
                    state.snap_enabled = snap_enabled;
                });
            });
            let modifiers = gpui_kit::Modifiers {
                alt,
                ..Default::default()
            };
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
                    pressed_button: Some(MouseButton::Left),
                    modifiers,
                }),
                cx,
            );

            let preview = content
                .update(cx, |app, cx| app.canvas_overlay(cx))
                .crop_create_preview
                .expect("Modern preview");
            let preview_left = preview.0 - f64::from(bounds.origin.x);
            let preview_top = preview.1 - f64::from(bounds.origin.y);
            assert!(
                (preview_left - 8.0).abs() < 0.1 && (preview_top - 8.0).abs() < 0.1,
                "the pointer sample stays unsnapped: {preview_left}, {preview_top}; alt={alt}"
            );
            assert!(content.read(cx).snap_guides_for_test().is_empty());

            if !alt {
                content.update(cx, |app, cx| {
                    app.state_for_test()
                        .update(cx, |state, _| state.snap_enabled = true);
                });
            }
            dispatch(
                window,
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: resume,
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                }),
                cx,
            );
            let guides = content.read(cx).snap_guides_for_test().to_vec();
            assert!(
                !guides.is_empty(),
                "snapping resumes after bypass; alt={alt}"
            );
            let preview = content
                .update(cx, |app, cx| app.canvas_overlay(cx))
                .crop_create_preview
                .expect("Modern preview after snap is enabled");
            let preview_left = preview.0 - f64::from(bounds.origin.x);
            let preview_top = preview.1 - f64::from(bounds.origin.y);
            assert!(guides.contains(&(true, 0.0)) && guides.contains(&(false, 0.0)));
            assert!(
                preview_left.abs() < 0.1 && preview_top.abs() < 0.1,
                "snapped preview starts at the viewport origin: {preview:?}; alt={alt}"
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
            let frame = content
                .read(cx)
                .crop_rect_for_test(cx)
                .expect("committed frame");
            let expected_frame = [
                (f64::from(bounds.size.width) / 2.0 - 80.0).floor(),
                (f64::from(bounds.size.height) / 2.0 - 60.0).floor(),
                161.0,
                121.0,
            ];
            assert_eq!(
                frame, expected_frame,
                "commit keeps the last move sample after release; alt={alt}"
            );
            assert!(content.read(cx).snap_guides_for_test().is_empty());
        }
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_cancels_a_snapped_modern_crop_creation_and_clears_guides(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (800, 600);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_mode = crate::crop_aspect::CropMode::Free;
                state.crop_rect = None;
                state.snap_enabled = true;
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, doc);
        let start = window_point(
            bounds,
            placement,
            doc,
            8.0 * doc.0 as f32 / placement.2,
            8.0 * doc.1 as f32 / placement.3,
        );
        let end = window_point(bounds, placement, doc, 160.0, 120.0);
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
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        assert!(!content.read(cx).snap_guides_for_test().is_empty());

        tab_to_canvas(window, cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(content.read(cx).crop_rect_for_test(cx).is_none());
        assert!(
            content
                .update(cx, |app, cx| app.canvas_overlay(cx))
                .crop_create_preview
                .is_none()
        );
        assert!(content.read(cx).snap_guides_for_test().is_empty());
        content.update(cx, |app, _| assert!(app.pending_gesture.is_none()));
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_creation_ignores_stale_release_after_tool_switch(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(800, 600, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_rect = None;
                state.snap_enabled = false;
            });
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);

        let bounds = window.find("canvas-hitbox").bounds();
        let start = bounds.origin + gpui_kit::point(gpui_kit::px(8.0), gpui_kit::px(8.0));
        let end = start + gpui_kit::point(gpui_kit::px(40.0), gpui_kit::px(40.0));
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
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, cx| {
            assert!(app.pending_gesture.as_ref().is_some_and(|pending| {
                pending.crop_modern_create && pending.crop_modern_drag_started
            }));
            assert!(app.canvas_overlay(cx).crop_create_preview.is_some());
            assert!(app.crop_rect_for_test(cx).is_none());
        });

        window.press("v", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Move
        );
        content.update(cx, |app, cx| {
            assert!(app.pending_gesture.is_none());
            assert!(app.crop_rect_for_test(cx).is_none());
            assert!(app.state_for_test().read(cx).crop_history.is_empty());
            assert!(!app.canvas_overlay(cx).crop_preview_active);
        });

        let stale_move = end + gpui_kit::point(gpui_kit::px(20.0), gpui_kit::px(20.0));
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: stale_move,
                pressed_button: Some(MouseButton::Left),
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
        content.update(cx, |app, cx| {
            assert!(app.pending_gesture.is_none());
            assert!(app.crop_rect_for_test(cx).is_none());
            assert!(app.state_for_test().read(cx).crop_history.is_empty());
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_restores_the_crop_frame_after_a_new_crop_drag(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let start = crate::crop_geometry::CropSnapshot {
        frame: [100.0, 80.0, 100.0, 80.0],
        offset: (5.0, 6.0),
        rotation: 15.0,
    };
    let history_entry = crate::crop_geometry::CropSnapshot {
        frame: [20.0, 30.0, 40.0, 50.0],
        offset: (1.0, 2.0),
        rotation: 3.0,
    };
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some(start.frame);
                state.crop_offset = start.offset;
                state.crop_rotation = start.rotation;
                state.crop_history = vec![history_entry];
                state.crop_redo = vec![history_entry];
            });
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);

        let doc = (400, 300);
        let (bounds, placement) = page(window, doc);
        let from = window_point(bounds, placement, doc, 20.0, 20.0);
        let to = window_point(bounds, placement, doc, 60.0, 50.0);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: to,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, cx| {
            assert!(app.pending_gesture.is_some());
            assert_eq!(app.state_for_test().read(cx).crop_rect, Some(start.frame));
            assert!(app.canvas_overlay(cx).crop_create_preview.is_some());
        });

        window.press("escape", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert!(app.pending_gesture.is_none());
            let state = app.state_for_test();
            assert_eq!(state.read(cx).crop_rect, Some(start.frame));
            assert_eq!(state.read(cx).crop_offset, start.offset);
            assert_eq!(state.read(cx).crop_rotation, start.rotation);
            assert_eq!(state.read(cx).crop_history, vec![history_entry]);
            assert_eq!(state.read(cx).crop_redo, vec![history_entry]);
        });

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, cx| {
            assert!(app.pending_gesture.is_none());
            assert_eq!(app.state_for_test().read(cx).crop_rect, Some(start.frame));
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn canvas_focus_loss_cancels_an_active_crop_handle_drag(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, _| window.activate_window())
        .expect("test window remains open");
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        assert!(window.is_window_active());
        content.update(cx, |app, cx| {
            app.set_canvas_tool_action(&SetCanvasTool(ToolId::Crop), cx);
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        assert!(
            content
                .read(cx)
                .canvas_focus_handle
                .contains_focused(window, cx)
        );

        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let start_frame = content
            .read(cx)
            .crop_rect_for_test(cx)
            .expect("Crop starts with a frame");
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.1, 0.1);
        let moved = window_point(bounds, placement, doc, 0.7, 0.7);
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
                position: moved,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, _| assert!(app.crop_drag.is_some()));
        assert!(
            window.captured_hitbox().is_some(),
            "crop handle drag must hold pointer capture before focus leaves"
        );

        window.press("tab", cx);
        window.render_frame(cx);
        assert!(
            !content
                .read(cx)
                .canvas_focus_handle
                .contains_focused(window, cx),
            "Tab must leave the canvas focus path"
        );
        content.update(cx, |app, cx| {
            assert!(
                app.crop_drag.is_none(),
                "canvas focus loss must cancel the active gesture"
            );
            assert_eq!(app.state_for_test().read(cx).crop_rect, Some(start_frame));
            assert!(app.state_for_test().read(cx).crop_history.is_empty());
        });
        assert!(
            window.captured_hitbox().is_none(),
            "canvas focus loss must release the captured pointer"
        );

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, cx| {
            assert!(app.crop_drag.is_none());
            assert_eq!(app.state_for_test().read(cx).crop_rect, Some(start_frame));
            assert!(app.state_for_test().read(cx).crop_history.is_empty());
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn canvas_focus_loss_cancels_a_snapped_crop_drag_and_clears_guides(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, _| window.activate_window())
        .expect("test window remains open");
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.set_canvas_tool_action(&SetCanvasTool(ToolId::Crop), cx);
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let (width, height, _) = pixels(&content, cx);
        let doc = (width, height);
        let frame = [
            f64::from(width) / 4.0,
            f64::from(height) / 4.0,
            f64::from(width) / 2.0,
            f64::from(height) / 2.0,
        ];
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some(frame);
                state.snap_enabled = true;
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, frame[0] as f32, frame[1] as f32);
        let moved = window_point(
            bounds,
            placement,
            doc,
            width as f32 / 3.0,
            height as f32 / 3.0,
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
                position: moved,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, _| assert!(app.crop_drag.is_some()));
        assert!(!content.read(cx).snap_guides_for_test().is_empty());
        assert!(window.captured_hitbox().is_some());

        window.press("tab", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert!(app.crop_drag.is_none());
            assert_eq!(app.state_for_test().read(cx).crop_rect, Some(frame));
            assert!(app.state_for_test().read(cx).crop_history.is_empty());
            assert!(app.snap_guides.is_empty());
        });
        assert!(window.captured_hitbox().is_none());

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, cx| {
            assert!(app.crop_drag.is_none());
            assert_eq!(app.state_for_test().read(cx).crop_rect, Some(frame));
            assert!(app.snap_guides.is_empty());
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn opening_a_modal_during_crop_handle_drag_cancels_before_pointer_up(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.set_canvas_tool_action(&SetCanvasTool(ToolId::Crop), cx);
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);

        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 0.1, 0.1);
        let moved = window_point(bounds, placement, doc, 0.7, 0.7);
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
                position: moved,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, _| assert!(app.crop_drag.is_some()));

        window.press("ctrl-n", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::NewDocument)
        );

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, cx| {
            assert!(app.crop_drag.is_none());
            assert!(app.state_for_test().read(cx).crop_rect.is_none());
            assert!(app.state_for_test().read(cx).crop_history.is_empty());
        });
    })
    .expect("test window remains open");
}

/// Arming Crop opens the default frame over the whole page, and
/// leaving the tool drops it.
#[gpui_kit::test]
fn selecting_crop_opens_the_frame_over_the_whole_page(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let (doc_w, doc_h) = content
            .read(cx)
            .active_document_size()
            .expect("fixture document");
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_none(),
            "no frame before the tool is armed"
        );
        content.update(cx, |app, cx| {
            app.set_canvas_tool_action(&SetCanvasTool(ToolId::Crop), cx);
        });
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some([0.0, 0.0, doc_w, doc_h]),
            "arming Crop frames the whole page"
        );
        content.update(cx, |app, cx| {
            app.set_canvas_tool_action(&SetCanvasTool(ToolId::Move), cx);
        });
        window.render_frame(cx);
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_none(),
            "leaving Crop drops the pending frame"
        );
    })
    .expect("test window remains open");
}

/// Perf: arming Crop seeds the full-page frame and repaints. The reset to
/// Move between samples is host-side and outside the measured span.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn crop_frame_open_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let arm = |content: &gpui_kit::Entity<App>, cx: &mut gpui_kit::App, tool: ToolId| {
            content.update(cx, |app, cx| {
                app.set_canvas_tool_action(&SetCanvasTool(tool), cx);
            });
        };
        arm(&content, cx, ToolId::Move);
        window.render_frame(cx);
        arm(&content, cx, ToolId::Crop);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            arm(&content, cx, ToolId::Move);
            window.render_frame(cx);
            let start = std::time::Instant::now();
            arm(&content, cx, ToolId::Crop);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "crop_frame_open p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "crop frame open p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn enter_applies_the_crop_and_undo_restores_the_document_size(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx)
                .expect("crop fixture document");
        });
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((400, 300))
        );
        drag_doc(window, (w, h), (40.0, 30.0), (360.0, 270.0), cx);
        let frame = content.read(cx).crop_rect_for_test(cx).expect("crop frame");
        assert!((frame[2] - 320.0).abs() < 0.01);
        assert!((frame[3] - 240.0).abs() < 0.01);
        assert!(
            frame[2] < 400.0 && frame[3] < 300.0,
            "the crop frame is smaller than the document: {frame:?}"
        );

        tab_to_canvas(window, cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((320, 240)),
            "Enter must apply the crop; frame={:?}, error={:?}",
            content.read(cx).crop_rect_for_test(cx),
            content.read(cx).state_for_test().read(cx).status_error
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
            Some((400, 300)),
            "undo restores the pre-crop document size"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn enter_during_modern_crop_creation_does_not_apply_the_existing_frame(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let camera =
        crate::display::canvas_bridge::Camera::new(0.0, 0.0, 1.0).expect("valid test camera");
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(800, 600, cx)
                .expect("crop fixture document");
            app.set_camera(camera, cx);
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_mode = crate::crop_aspect::CropMode::Free;
                state.crop_rect = Some([200.0, 150.0, 200.0, 150.0]);
                state.crop_offset = (0.0, 0.0);
                state.crop_rotation = 0.0;
            });
        });
        let crop_history_before = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .crop_history
            .len();
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let bounds = window.find("canvas-hitbox").bounds();
        let point = |x, y| bounds.origin + gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y));
        let start = point(8.0, 8.0);
        let end = point(40.0, 40.0);
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
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        assert!(
            content
                .update(cx, |app, cx| app.canvas_overlay(cx).crop_create_preview)
                .is_some(),
            "Modern creation preview is active"
        );
        content.update(cx, |app, cx| {
            let state = app.state_for_test();
            let state = state.read(cx);
            assert_eq!(state.crop_rect, Some([200.0, 150.0, 200.0, 150.0]));
            assert_eq!(state.crop_history.len(), crop_history_before);
            assert!(app.pending_gesture.as_ref().is_some_and(|pending| {
                pending.crop_modern_create
                    && pending.crop_modern_drag_started
                    && pending.crop_start.is_some()
            }));
        });

        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((800, 600)),
            "Enter must not apply the stored frame during Modern creation"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Crop
        );
        content.update(cx, |app, cx| {
            let state = app.state_for_test();
            let state = state.read(cx);
            assert_eq!(state.crop_rect, Some([200.0, 150.0, 200.0, 150.0]));
            assert_eq!(state.crop_history.len(), crop_history_before);
            assert!(app.pending_gesture.as_ref().is_some_and(|pending| {
                pending.crop_modern_create
                    && pending.crop_modern_drag_started
                    && pending.crop_start.is_some()
            }));
        });

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((800, 600))
        );
        content.update(cx, |app, _| assert!(app.pending_gesture.is_none()));
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn enter_during_modern_crop_creation_without_a_frame_does_not_report_an_error(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let camera =
        crate::display::canvas_bridge::Camera::new(0.0, 0.0, 1.0).expect("valid test camera");
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(800, 600, cx)
                .expect("crop fixture document");
            app.set_camera(camera, cx);
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_mode = crate::crop_aspect::CropMode::Free;
                state.crop_rect = None;
            });
        });
        let crop_history_before = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .crop_history
            .len();
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let bounds = window.find("canvas-hitbox").bounds();
        let point = bounds.origin + gpui_kit::point(gpui_kit::px(8.0), gpui_kit::px(8.0));
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: point,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.status_error = Some("keyboard delivery marker".to_string());
            });
        });

        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((800, 600))
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).status_error,
            None,
            "Enter reaches Crop while a no-frame click is pending"
        );
        assert!(content.read(cx).crop_rect_for_test(cx).is_none());
        content.update(cx, |app, cx| {
            assert_eq!(
                app.state_for_test().read(cx).crop_history.len(),
                crop_history_before
            );
            assert!(app.pending_gesture.as_ref().is_some_and(|pending| {
                pending.crop_modern_create
                    && !pending.crop_modern_drag_started
                    && pending.crop_start.is_none()
            }));
        });

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: point,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((800, 600))
        );
        content.update(cx, |app, _| assert!(app.pending_gesture.is_none()));
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn enter_before_modern_crop_creation_threshold_applies_the_existing_frame(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let camera =
        crate::display::canvas_bridge::Camera::new(0.0, 0.0, 1.0).expect("valid test camera");
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(800, 600, cx)
                .expect("crop fixture document");
            app.set_camera(camera, cx);
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_mode = crate::crop_aspect::CropMode::Free;
                state.crop_rect = Some([200.0, 150.0, 200.0, 150.0]);
            });
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let bounds = window.find("canvas-hitbox").bounds();
        let point = bounds.origin + gpui_kit::point(gpui_kit::px(8.0), gpui_kit::px(8.0));
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: point,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, _| {
            assert!(app.pending_gesture.as_ref().is_some_and(|pending| {
                pending.crop_modern_create
                    && !pending.crop_modern_drag_started
                    && pending.crop_start.is_some()
            }));
        });

        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((200, 150))
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Move
        );
        assert!(content.read(cx).crop_rect_for_test(cx).is_none());
        let history_after_apply = content
            .read(cx)
            .active_core_history_version_for_test()
            .expect("history version");

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: point,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((200, 150))
        );
        assert_eq!(
            content
                .read(cx)
                .active_core_history_version_for_test()
                .expect("history version"),
            history_after_apply,
            "the stale release must not apply the crop a second time"
        );
        assert!(content.read(cx).crop_rect_for_test(cx).is_none());
        content.update(cx, |app, _| assert!(app.pending_gesture.is_none()));
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
fn crop_bar_apply_and_cancel_are_wired(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx)
                .expect("crop fixture document");
        });
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("option-live-crop-apply").is_none(),
            "the Crop option bar offers no live strip"
        );
        let (w, h, _) = pixels(&content, cx);

        drag_doc(window, (w, h), (40.0, 30.0), (360.0, 270.0), cx);
        assert!(content.read(cx).crop_rect_for_test(cx).is_some());
        window.click("opt-cancel", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_none(),
            "Cancel clears the frame"
        );

        drag_doc(window, (w, h), (40.0, 30.0), (360.0, 270.0), cx);
        let before = content
            .read(cx)
            .active_document_dims_for_test()
            .expect("document dims");
        window.click("opt-apply", cx);
        window.render_frame(cx);
        assert!(content.read(cx).crop_rect_for_test(cx).is_none());
        let after = content
            .read(cx)
            .active_document_dims_for_test()
            .expect("document dims");
        assert!(
            after.0 <= before.0 && after.1 <= before.1 && after != before,
            "Apply committed the crop: {before:?} -> {after:?}"
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
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.snap_enabled = false;
            });
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
        assert!(
            (moved[1] - (frame[1] + 30.0)).abs() < 3.0,
            "moved={moved:?} from {frame:?}"
        );
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

#[gpui_kit::test]
fn classic_crop_frame_can_move_beyond_document_bounds(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(100, 100, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some([20.0, 20.0, 40.0, 40.0]);
            });
        });
        window.render_frame(cx);
        drag_doc(window, (100, 100), (40.0, 40.0), (115.0, 115.0), cx);
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some([95.0, 95.0, 40.0, 40.0])
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_pasteboard_click_hides_and_preserves_the_preview(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let original = crate::crop_geometry::CropSnapshot {
        frame: [40.0, 20.0, 160.0, 60.0],
        offset: (7.0, -3.0),
        rotation: 15.0,
    };
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 100, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some(original.frame);
                state.crop_offset = original.offset;
                state.crop_rotation = original.rotation;
            });
        });
        window.render_frame(cx);
        let position = crop_pasteboard_point(window, (400, 100));
        let moved = position + gpui_kit::point(gpui_kit::px(2.0), gpui_kit::px(0.0));
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: moved,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).crop_rect, None);
        assert_eq!(state.read(cx).crop_hidden_preview, Some(original));
        assert_eq!(state.read(cx).crop_rotation, 0.0);
        assert_eq!(state.read(cx).active_tool, ToolId::Crop);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_pasteboard_drag_replaces_the_frame_after_three_pixels(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let original = [120.0, 20.0, 180.0, 60.0];
    let hidden = crate::crop_geometry::CropSnapshot {
        frame: [24.0, 32.0, 140.0, 50.0],
        offset: (0.0, 0.0),
        rotation: -12.0,
    };
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 100, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some(original);
                state.crop_hidden_preview = Some(hidden);
                state.crop_rotation = -12.0;
            });
        });
        window.render_frame(cx);
        let start = crop_pasteboard_point(window, (400, 100));
        let end = start + gpui_kit::point(gpui_kit::px(3.0), gpui_kit::px(3.0));
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
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        let state = content.read(cx).state_for_test();
        assert_ne!(state.read(cx).crop_rect, Some(original));
        assert_eq!(state.read(cx).crop_hidden_preview, None);
        assert_eq!(state.read(cx).crop_rotation, 0.0);
        let replacement = state.read(cx).crop_rect.expect("valid replacement preview");
        let below_threshold = start + gpui_kit::point(gpui_kit::px(2.0), gpui_kit::px(2.0));
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: below_threshold,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some(replacement),
            "returning below the threshold keeps the last valid replacement frame"
        );
        let zero_area = start + gpui_kit::point(gpui_kit::px(6.0), gpui_kit::px(0.0));
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: zero_area,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some(replacement),
            "an invalid move after a valid replacement keeps the last valid frame"
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: zero_area,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some(replacement),
            "pointer-up at a zero-area point does not replace the last valid frame"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_pasteboard_drag_rejects_zero_area_replacements(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let original = [120.0, 20.0, 180.0, 60.0];
    let hidden = crate::crop_geometry::CropSnapshot {
        frame: [24.0, 32.0, 140.0, 50.0],
        offset: (0.0, 0.0),
        rotation: -12.0,
    };
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 100, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some(original);
                state.crop_hidden_preview = Some(hidden);
                state.crop_rotation = -12.0;
            });
        });
        window.render_frame(cx);
        let start = crop_pasteboard_point(window, (400, 100));
        let end = start + gpui_kit::point(gpui_kit::px(3.0), gpui_kit::px(0.0));
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
                pressed_button: Some(MouseButton::Left),
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

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).crop_rect, Some(original));
        assert_eq!(state.read(cx).crop_hidden_preview, Some(hidden));
        assert_eq!(state.read(cx).crop_rotation, -12.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_canvas_click_restores_the_hidden_preview(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let hidden = crate::crop_geometry::CropSnapshot {
        frame: [40.0, 20.0, 160.0, 60.0],
        offset: (7.0, -3.0),
        rotation: -10.0,
    };
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 100, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_hidden_preview = Some(hidden);
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 100));
        let position = window_point(bounds, placement, (400, 100), 300.0, 80.0);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position,
                ..Default::default()
            }),
            cx,
        );

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).crop_rect, Some(hidden.frame));
        assert_eq!(state.read(cx).crop_offset, hidden.offset);
        assert_eq!(state.read(cx).crop_rotation, hidden.rotation);
        assert_eq!(state.read(cx).crop_hidden_preview, None);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_canvas_click_uses_document_delta_for_restore_threshold(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let hidden = crate::crop_geometry::CropSnapshot {
        frame: [40.0, 20.0, 160.0, 60.0],
        offset: (0.0, 0.0),
        rotation: 10.0,
    };
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 100, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_hidden_preview = Some(hidden);
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 100));
        let start = window_point(bounds, placement, (400, 100), 300.0, 80.0);
        let end = window_point(bounds, placement, (400, 100), 302.0, 80.0);
        assert!(
            f32::from(end.x) - f32::from(start.x) > 3.0,
            "fixture must make two document pixels exceed the screen threshold"
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
                position: end,
                pressed_button: Some(MouseButton::Left),
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

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).crop_rect, Some(hidden.frame));
        assert_eq!(state.read(cx).crop_rotation, hidden.rotation);
        assert_eq!(state.read(cx).crop_hidden_preview, None);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_canvas_click_without_hidden_preview_uses_the_full_document(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 100, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
            });
        });
        window.render_frame(cx);
        let start_zoom = content.update(cx, |app, _| {
            app.effective_camera().expect("fitted camera").zoom
        });
        let (bounds, placement) = page(window, (400, 100));
        let position = window_point(bounds, placement, (400, 100), 300.0, 80.0);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position,
                ..Default::default()
            }),
            cx,
        );

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).crop_rect, Some([0.0, 0.0, 400.0, 100.0]));
        assert_eq!(state.read(cx).crop_hidden_preview, None);
        let camera = content.update(cx, |app, _| app.effective_camera().expect("centred camera"));
        assert_eq!(camera.zoom, start_zoom);
        let viewport_width: f32 = bounds.size.width.into();
        let viewport_height: f32 = bounds.size.height.into();
        assert!((camera.x - (viewport_width - 400.0 * start_zoom) / 2.0).abs() < 0.5);
        assert!((camera.y - (viewport_height - 100.0 * start_zoom) / 2.0).abs() < 0.5);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_drag_moves_the_image_without_moving_the_frame(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_rect = Some([50.0, 50.0, 200.0, 150.0]);
                state.crop_offset = (0.0, 0.0);
            });
        });
        window.render_frame(cx);
        drag_doc(window, (400, 300), (150.0, 125.0), (200.0, 155.0), cx);
        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).crop_rect, Some([50.0, 50.0, 200.0, 150.0]));
        assert_ne!(state.read(cx).crop_offset, (0.0, 0.0));
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_pointer_up_keeps_the_last_pointer_move(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (400, 300);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_mode = crate::crop_aspect::CropMode::Free;
                state.crop_rect = Some([50.0, 50.0, 200.0, 150.0]);
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 150.0, 125.0);
        let moved = window_point(bounds, placement, doc, 200.0, 155.0);
        let released = window_point(bounds, placement, doc, 230.0, 175.0);
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
                position: moved,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        let after_move = content.update(cx, |app, cx| {
            let state = app.state_for_test();
            let state = state.read(cx);
            (state.crop_rect, state.crop_offset, state.crop_rotation)
        });
        assert_eq!(after_move.0, Some([50.0, 50.0, 200.0, 150.0]));
        assert_ne!(after_move.1, (0.0, 0.0));

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: released,
                ..Default::default()
            }),
            cx,
        );
        let after_release = content.update(cx, |app, cx| {
            let state = app.state_for_test();
            let state = state.read(cx);
            (state.crop_rect, state.crop_offset, state.crop_rotation)
        });
        assert_eq!(
            after_release, after_move,
            "Modern pointer-up commits the latest move sample without resampling release"
        );

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_rect = Some([50.0, 50.0, 200.0, 150.0]);
                state.crop_offset = (0.0, 0.0);
                state.crop_rotation = 0.0;
                state.crop_history.clear();
                state.crop_redo.clear();
            });
        });
        window.render_frame(cx);
        let start = window_point(bounds, placement, doc, 50.0, 50.0);
        let moved = window_point(bounds, placement, doc, 40.0, 40.0);
        let released = window_point(bounds, placement, doc, 30.0, 30.0);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, _| {
            assert_eq!(
                app.crop_drag.as_ref().and_then(|drag| drag.handle),
                Some(crate::app::crop::CropHandle::Nw)
            );
        });
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: moved,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        let after_resize_move = content.update(cx, |app, cx| {
            let state = app.state_for_test();
            let state = state.read(cx);
            (state.crop_rect, state.crop_offset, state.crop_rotation)
        });
        assert_ne!(after_resize_move.0, Some([50.0, 50.0, 200.0, 150.0]));
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: released,
                ..Default::default()
            }),
            cx,
        );
        let after_resize_release = content.update(cx, |app, cx| {
            let state = app.state_for_test();
            let state = state.read(cx);
            (state.crop_rect, state.crop_offset, state.crop_rotation)
        });
        assert_eq!(
            after_resize_release, after_resize_move,
            "Modern resize commits the latest move sample"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_pointer_up_keeps_the_last_handle_move(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (400, 300);
    let start_frame = [130.0, 100.0, 68.0, 50.0];
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some(start_frame);
                state.snap_enabled = false;
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, doc);
        let start = window_point(bounds, placement, doc, 198.0, 150.0);
        let moved = window_point(bounds, placement, doc, 202.0, 154.0);
        let released = window_point(bounds, placement, doc, 208.0, 160.0);
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
                position: moved,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        let after_move = content.read(cx).crop_rect_for_test(cx);
        assert_ne!(after_move, Some(start_frame));

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: released,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            after_move,
            "Classic pointer-up commits the last handle move without resampling release"
        );
        content.update(cx, |app, cx| {
            assert!(app.crop_drag.is_none());
            assert_eq!(app.state_for_test().read(cx).crop_history.len(), 1);
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_pasteboard_drag_centers_size_frame_and_compensates_selection(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (400, 300);
    let target = (160.0, 120.0);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_mode = crate::crop_aspect::CropMode::Size;
                state.crop_size_target = Some((target.0 as u32, target.1 as u32));
                state.crop_rect = Some([300.0, 220.0, 80.0, 60.0]);
                state.crop_offset = (11.0, -9.0);
                state.crop_rotation = 17.0;
            });
        });
        window.render_frame(cx);

        let (bounds, _) = page(window, doc);
        let camera = content.update(cx, |app, _| app.effective_camera().expect("camera"));
        let viewport_width: f32 = bounds.size.width.into();
        let viewport_height: f32 = bounds.size.height.into();
        let start = crop_pasteboard_point(window, doc);
        let end = start + gpui_kit::point(gpui_kit::px(20.0), gpui_kit::px(20.0));
        assert!(bounds.contains(&end), "release remains inside the canvas");

        let center_doc = (
            (f64::from(viewport_width) / 2.0 - f64::from(camera.x)) / f64::from(camera.zoom),
            (f64::from(viewport_height) / 2.0 - f64::from(camera.y)) / f64::from(camera.zoom),
        );
        let expected_frame = [
            (center_doc.0 - target.0 / 2.0 + 0.5).floor(),
            (center_doc.1 - target.1 / 2.0 + 0.5).floor(),
            target.0,
            target.1,
        ];
        let start_local = (
            f64::from(start.x) - f64::from(bounds.origin.x),
            f64::from(start.y) - f64::from(bounds.origin.y),
        );
        let end_local = (
            f64::from(end.x) - f64::from(bounds.origin.x),
            f64::from(end.y) - f64::from(bounds.origin.y),
        );
        let expected_offset = (
            f64::from(viewport_width) / 2.0 - (start_local.0 + end_local.0) / 2.0,
            f64::from(viewport_height) / 2.0 - (start_local.1 + end_local.1) / 2.0,
        );
        crop_pointer_drag(window, start, end, cx);

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).crop_rect, Some(expected_frame));
        assert!((state.read(cx).crop_offset.0 - expected_offset.0).abs() < 0.01);
        assert!((state.read(cx).crop_offset.1 - expected_offset.1).abs() < 0.01);
        assert_eq!(state.read(cx).crop_rotation, 17.0);
        assert_eq!(state.read(cx).crop_hidden_preview, None);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_ratio_drag_uses_selection_area_and_one_hundred_pixel_minimum(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (400, 300);
    let expected_width = (10_000.0_f64 * 1.5).sqrt();
    let expected_height = 100.0;
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_mode = crate::crop_aspect::CropMode::Ratio;
                state.crop_aspect = Some((3, 2));
                state.crop_rect = Some([300.0, 220.0, 80.0, 60.0]);
            });
        });
        window.render_frame(cx);

        let (bounds, placement) = page(window, doc);
        let camera = content.update(cx, |app, _| app.effective_camera().expect("camera"));
        let viewport_width: f32 = bounds.size.width.into();
        let viewport_height: f32 = bounds.size.height.into();
        let start = window_point(bounds, placement, doc, 20.0, 20.0);
        let end = window_point(bounds, placement, doc, 40.0, 60.0);
        crop_pointer_drag(window, start, end, cx);

        let center_doc = (
            (f64::from(viewport_width) / 2.0 - f64::from(camera.x)) / f64::from(camera.zoom),
            (f64::from(viewport_height) / 2.0 - f64::from(camera.y)) / f64::from(camera.zoom),
        );
        let frame = content.read(cx).state_for_test().read(cx).crop_rect;
        let [x, y, width, height] = frame.expect("created frame");
        assert!((width - expected_width).abs() < 0.001);
        assert!((height - expected_height).abs() < 0.001);
        assert_eq!(x, (center_doc.0 - width / 2.0 + 0.5).floor());
        assert_eq!(y, (center_doc.1 - height / 2.0 + 0.5).floor());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_shift_drag_creates_a_square_frame(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (800, 600);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_mode = crate::crop_aspect::CropMode::Free;
                state.crop_rect = Some([600.0, 400.0, 80.0, 60.0]);
                state.crop_offset = (12.0, -4.0);
                state.crop_rotation = 19.0;
            });
        });
        window.render_frame(cx);

        let (bounds, placement) = page(window, doc);
        let camera = content.update(cx, |app, _| app.effective_camera().expect("camera"));
        let viewport_width: f32 = bounds.size.width.into();
        let viewport_height: f32 = bounds.size.height.into();
        let start = window_point(bounds, placement, doc, 20.0, 20.0);
        let end = window_point(bounds, placement, doc, 170.0, 100.0);
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
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                modifiers: gpui_kit::Modifiers {
                    shift: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            cx,
        );

        let center_doc = (
            (f64::from(viewport_width) / 2.0 - f64::from(camera.x)) / f64::from(camera.zoom),
            (f64::from(viewport_height) / 2.0 - f64::from(camera.y)) / f64::from(camera.zoom),
        );
        let state = content.read(cx).state_for_test();
        let [x, y, width, height] = state.read(cx).crop_rect.expect("created frame");
        assert_eq!(x, (center_doc.0 - 75.0 + 0.5).floor());
        assert_eq!(y, (center_doc.1 - 75.0 + 0.5).floor());
        assert!((width - 150.0).abs() < 0.001);
        assert!((height - 150.0).abs() < 0.001);
        assert!((width - height).abs() < 0.001);
        assert_eq!(state.read(cx).crop_rotation, 19.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_short_drag_keeps_the_existing_frame(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let original = [50.0, 50.0, 200.0, 150.0];
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(800, 600, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_rect = Some(original);
                state.crop_offset = (13.0, -7.0);
                state.crop_rotation = 10.0;
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, (800, 600));
        let start = window_point(bounds, placement, (800, 600), 700.0, 500.0);
        let end = start + gpui_kit::point(gpui_kit::px(4.0), gpui_kit::px(4.0));
        crop_pointer_drag(window, start, end, cx);

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).crop_rect, Some(original));
        assert_eq!(state.read(cx).crop_offset, (13.0, -7.0));
        assert_eq!(state.read(cx).crop_rotation, 10.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_pasteboard_click_creates_a_centered_default_frame(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let doc = (400, 300);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(doc.0, doc.1, cx)
                .expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_offset = (8.0, -6.0);
                state.crop_rotation = 22.0;
            });
        });
        window.render_frame(cx);
        let (bounds, _) = page(window, doc);
        let viewport_width: f32 = bounds.size.width.into();
        let viewport_height: f32 = bounds.size.height.into();
        let initial_camera = content.update(cx, |app, _| app.effective_camera().expect("camera"));
        let camera = crate::display::canvas_bridge::Camera::centred_at(
            viewport_width,
            viewport_height,
            doc.0 as f32,
            doc.1 as f32,
            initial_camera.zoom,
        )
        .expect("centered camera");
        let expected_frame = crate::crop_geometry::default_modern_crop_frame(
            (f64::from(viewport_width), f64::from(viewport_height)),
            (f64::from(doc.0), f64::from(doc.1)),
            f64::from(camera.zoom),
            (f64::from(camera.x), f64::from(camera.y)),
            None,
        );
        let position = crop_pasteboard_point(window, doc);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position,
                ..Default::default()
            }),
            cx,
        );

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).crop_rect, Some(expected_frame));
        assert_eq!(state.read(cx).crop_offset, (0.0, 0.0));
        assert_eq!(state.read(cx).crop_rotation, 0.0);
        let actual_camera = content.update(cx, |app, _| app.effective_camera().expect("camera"));
        assert_eq!(actual_camera.x, camera.x);
        assert_eq!(actual_camera.y, camera.y);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_rotate_band_changes_rotation_without_moving_the_crop(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_rect = Some([50.0, 50.0, 200.0, 150.0]);
                state.crop_offset = (11.0, -9.0);
                state.crop_rotation = 0.0;
            });
        });
        window.render_frame(cx);
        drag_doc(window, (400, 300), (100.0, 25.0), (275.0, 100.0), cx);

        let state = content.read(cx).state_for_test();
        assert!(
            state.read(cx).crop_rotation.abs() > 90.0,
            "rotation={}",
            state.read(cx).crop_rotation
        );
        assert_eq!(state.read(cx).crop_rect, Some([50.0, 50.0, 200.0, 150.0]));
        assert_eq!(state.read(cx).crop_offset, (11.0, -9.0));
        assert_eq!(state.read(cx).crop_history.len(), 1);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn classic_crop_rotate_band_follows_the_rotated_frame(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some([50.0, 50.0, 200.0, 150.0]);
                state.crop_offset = (11.0, -9.0);
                state.crop_rotation = 90.0;
                state.crop_delete_pixels = true;
            });
        });
        window.render_frame(cx);
        let overlay = content.update(cx, |app, cx| app.canvas_overlay(cx));
        assert_eq!(overlay.crop_rotation, 90.0);
        assert!(overlay.crop_delete_pixels);
        let (bounds, placement) = page(window, (400, 300));
        let ring = window_point(bounds, placement, (400, 300), 250.0, 75.0);
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: ring,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.update(cx, |app, cx| app.canvas_cursor_policy(cx)),
            crate::input_policy::CursorPolicy::Rotate
        );
        drag_doc(window, (400, 300), (250.0, 75.0), (200.0, 225.0), cx);

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).crop_rotation, 180.0);
        assert_eq!(state.read(cx).crop_rect, Some([50.0, 50.0, 200.0, 150.0]));
        assert_eq!(state.read(cx).crop_offset, (11.0, -9.0));
        assert_eq!(state.read(cx).crop_history.len(), 1);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_shield_clip_is_limited_to_the_canvas_viewport(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some([-5000.0, -5000.0, 10000.0, 10000.0]);
                state.crop_rotation = 23.0;
            });
        });
        let viewport = content.update(cx, |app, _| {
            app.canvas_bounds.get().expect("canvas viewport bounds")
        });
        let scale = window.scale_factor();
        for (delete_pixels, color, alpha) in [(false, 0x000000, 0.5_f32), (true, 0x161618, 0.98)] {
            content.update(cx, |app, cx| {
                app.state_for_test()
                    .update(cx, |state, _| state.crop_delete_pixels = delete_pixels);
            });
            window.render_frame(cx);
            let expected_color: gpui_kit::Background = gpui_kit::rgb(color).alpha(alpha).into();
            let scene = window.painted_scene();
            let shield = scene
                .paths
                .iter()
                .find(|path| path.color == expected_color)
                .unwrap_or_else(|| panic!("crop shield path with {expected_color:?}"));
            let mask = shield.content_mask.bounds;
            assert_eq!(
                [
                    mask.origin.x.0,
                    mask.origin.y.0,
                    mask.size.width.0,
                    mask.size.height.0,
                ],
                [
                    f32::from(viewport.origin.x) * scale,
                    f32::from(viewport.origin.y) * scale,
                    f32::from(viewport.size.width) * scale,
                    f32::from(viewport.size.height) * scale,
                ]
            );
            assert!(
                shield.bounds.origin.x.0 < mask.origin.x.0
                    || shield.bounds.origin.y.0 < mask.origin.y.0
                    || shield.bounds.bottom_right().x.0 > mask.bottom_right().x.0
                    || shield.bounds.bottom_right().y.0 > mask.bottom_right().y.0,
                "the fixture must extend the rotated crop hole past the viewport"
            );
        }
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_rotation_keeps_the_shift_snapped_pointer_move_at_release(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_rect = Some([50.0, 50.0, 200.0, 150.0]);
                state.crop_offset = (11.0, -9.0);
                state.crop_rotation = 0.0;
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 300));
        let start = window_point(bounds, placement, (400, 300), 100.0, 25.0);
        let end = window_point(bounds, placement, (400, 300), 275.0, 100.0);
        let released = window_point(bounds, placement, (400, 300), 300.0, 130.0);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, _| {
            assert!(app.crop_drag.as_ref().is_some_and(|drag| drag.rotate));
        });
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                modifiers: gpui_kit::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_rotation,
            105.0
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: released,
                ..Default::default()
            }),
            cx,
        );

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).crop_rotation, 105.0);
        assert_eq!(state.read(cx).crop_rect, Some([50.0, 50.0, 200.0, 150.0]));
        assert_eq!(state.read(cx).crop_offset, (11.0, -9.0));
        assert_eq!(state.read(cx).crop_history.len(), 1);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_restores_a_crop_rotation_drag_snapshot(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let original = crate::crop_geometry::CropSnapshot {
        frame: [50.0, 50.0, 200.0, 150.0],
        offset: (11.0, -9.0),
        rotation: 15.0,
    };
    let history_entry = crate::crop_geometry::CropSnapshot {
        frame: [10.0, 20.0, 30.0, 40.0],
        offset: (1.0, 2.0),
        rotation: 3.0,
    };
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_rect = Some(original.frame);
                state.crop_offset = original.offset;
                state.crop_rotation = original.rotation;
                state.crop_history = vec![history_entry];
                state.crop_redo = vec![history_entry];
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 300));
        let start = window_point(bounds, placement, (400, 300), 100.0, 25.0);
        let moved = window_point(bounds, placement, (400, 300), 275.0, 100.0);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: moved,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, cx| {
            assert_ne!(
                app.state_for_test().read(cx).crop_rotation,
                original.rotation
            );
        });

        window.press("escape", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert!(app.crop_drag.is_none());
            let state = app.state_for_test();
            assert_eq!(state.read(cx).crop_rect, Some(original.frame));
            assert_eq!(state.read(cx).crop_offset, original.offset);
            assert_eq!(state.read(cx).crop_rotation, original.rotation);
            assert_eq!(state.read(cx).crop_history, vec![history_entry]);
            assert_eq!(state.read(cx).crop_redo, vec![history_entry]);
        });
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, cx| {
            let state = app.state_for_test();
            assert_eq!(state.read(cx).crop_rect, Some(original.frame));
            assert_eq!(state.read(cx).crop_offset, original.offset);
            assert_eq!(state.read(cx).crop_rotation, original.rotation);
            assert_eq!(state.read(cx).crop_history, vec![history_entry]);
            assert_eq!(state.read(cx).crop_redo, vec![history_entry]);
        });
    })
    .expect("test window remains open");
}

#[test]
fn crop_resize_handle_hit_box_is_28_screen_pixels() {
    assert_eq!(
        crate::app::App::crop_handle_at((50.0, 38.0), [50.0, 50.0, 200.0, 150.0], 1.0),
        Some((crate::app::crop::CropHandle::Nw, false))
    );
}

#[test]
fn crop_rotate_band_uses_screen_width_and_yields_to_handles() {
    use crate::app::crop::CropPointerTarget as Target;
    use crate::crop_geometry::CropInteraction::{Classic, Modern};

    let frame = [40.0, 30.0, 120.0, 90.0];
    let hit =
        |point, zoom| crate::app::App::crop_pointer_target_at(point, frame, zoom, Modern, 0.0);

    assert_eq!(hit((100.0, 13.0), 1.0), Some(Target::Rotate));
    assert_eq!(
        hit((100.0, 16.0), 1.0),
        Some(Target::Resize(crate::app::crop::CropHandle::N))
    );
    assert_eq!(hit((100.0, -70.0), 1.0), Some(Target::Rotate));
    assert_eq!(hit((100.0, -70.1), 1.0), None);
    assert_eq!(hit((100.0, -20.0), 2.0), Some(Target::Rotate));
    assert_eq!(hit((100.0, -20.1), 2.0), None);
    assert_eq!(
        crate::app::App::crop_pointer_target_at((100.0, -72.0), frame, 1.0, Classic, 0.0),
        Some(Target::Rotate)
    );
    assert_eq!(
        crate::app::App::crop_pointer_target_at((-61.9, -71.9), frame, 1.0, Classic, 0.0),
        None
    );
}

#[test]
fn crop_resize_cursor_follows_classic_frame_rotation_only() {
    use crate::app::crop::CropHandle::N;
    use crate::crop_geometry::CropInteraction::{Classic, Modern};
    use crate::input_policy::CursorPolicy::{ResizeNesw, ResizeNs};

    assert_eq!(
        crate::app::canvas_input::crop_cursor_for_handle(N, Classic, 45.0),
        ResizeNesw
    );
    assert_eq!(
        crate::app::canvas_input::crop_cursor_for_handle(N, Modern, 45.0),
        ResizeNs
    );
}

#[test]
fn crop_rotation_snap_matches_oracle_rounding_and_mode_order() {
    use crate::app::crop::crop_rotation_after_drag;
    use crate::crop_geometry::CropInteraction::{Classic, Modern};

    assert_eq!(crop_rotation_after_drag(-7.5, Modern, true), 0.0);
    assert_eq!(crop_rotation_after_drag(7.5, Modern, true), 15.0);
    assert_eq!(crop_rotation_after_drag(-187.5, Modern, true), 180.0);
    assert_eq!(crop_rotation_after_drag(-187.5, Classic, true), -180.0);
}

#[gpui_kit::test]
fn modern_crop_offset_and_rotation_render_a_derived_preview(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(4, 4, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_rect = Some([0.0, 0.0, 4.0, 4.0]);
                state.crop_offset = (5.0, -3.0);
                state.crop_rotation = 17.0;
            });
        });
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let overlay = content.update(cx, |app, cx| app.canvas_overlay(cx));
        assert!(
            overlay.crop_preview_active,
            "the transformed raster replaces the untransformed page"
        );
        assert_eq!(overlay.crop_rotation, 0.0);
    })
    .expect("test window remains open");

    let original_scale = cx
        .update_window(window.into(), |_, window, _| window.scale_factor())
        .expect("test window remains open");
    let initial_size = content
        .update(cx, |app, _| app.crop_preview_size_for_test())
        .expect("crop preview raster");
    let history_version = content
        .update(cx, |app, _| app.active_core_history_version_for_test())
        .expect("core history version");
    let next_scale = if original_scale == 1.5 { 2.0 } else { 1.5 };
    cx.simulate_window_scale_factor_change(window.into(), next_scale);
    cx.simulate_window_resize(
        window.into(),
        gpui_kit::size(gpui_kit::px(1367.5), gpui_kit::px(820.5)),
    );
    cx.update_window(window.into(), |_, window, cx| {
        assert_eq!(window.scale_factor(), next_scale);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let actual_size = content
            .update(cx, |app, _| app.crop_preview_size_for_test())
            .expect("rescaled crop preview raster");
        let viewport = content
            .update(cx, |app, _| {
                app.canvas_bounds
                    .get()
                    .map(|bounds| (f32::from(bounds.size.width), f32::from(bounds.size.height)))
            })
            .expect("canvas bounds");
        assert!(
            viewport.0.fract() != 0.0 || viewport.1.fract() != 0.0,
            "the resized canvas must keep a fractional logical edge: {viewport:?}"
        );
        assert_eq!(
            actual_size,
            (
                (viewport.0 * next_scale).ceil() as u32,
                (viewport.1 * next_scale).ceil() as u32,
            )
        );
        assert!(content.update(cx, |app, cx| app.canvas_overlay(cx).crop_preview_active));
        assert_eq!(
            content.update(cx, |app, _| app.active_core_history_version_for_test()),
            Some(history_version)
        );
        assert_ne!(actual_size, initial_size);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_preview_rejects_stale_work_and_clears_when_inactive(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(4, 4, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_rect = Some([0.0, 0.0, 4.0, 4.0]);
                state.crop_offset = (5.0, -3.0);
                state.crop_rotation = 17.0;
            });
        });
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(content.update(cx, |app, cx| app.canvas_overlay(cx).crop_preview_active));
    })
    .expect("test window remains open");
    let (size_a, displayed_a) = content.update(cx, |app, _| {
        (
            app.crop_preview_size_for_test()
                .expect("initial preview raster"),
            app.crop_preview_displayed_key
                .clone()
                .expect("initial preview key"),
        )
    });

    let (release_b, gate_b) = futures::channel::oneshot::channel();
    content.update(cx, |app, _| app.crop_preview_test_gate = Some(gate_b));
    cx.simulate_window_resize(
        window.into(),
        gpui_kit::size(gpui_kit::px(1201.5), gpui_kit::px(760.5)),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    let (size_b, key_b) = content.update(cx, |app, cx| {
        let bounds = app.canvas_bounds.get().expect("resized canvas bounds");
        let size = (
            (f32::from(bounds.size.width) * app.window_scale).ceil() as u32,
            (f32::from(bounds.size.height) * app.window_scale).ceil() as u32,
        );
        let running = app
            .crop_preview_running
            .as_ref()
            .expect("replacement preview is running")
            .clone();
        assert_ne!(running, displayed_a);
        assert_ne!(size, size_a);
        assert_eq!(app.crop_preview_displayed_key.as_ref(), Some(&displayed_a));
        assert_eq!(app.crop_preview_size_for_test(), Some(size_a));
        assert!(!app.canvas_overlay(cx).crop_preview_active);
        (size, running)
    });

    let (release_c, gate_c) = futures::channel::oneshot::channel();
    content.update(cx, |app, _| app.crop_preview_test_gate = Some(gate_c));
    cx.simulate_window_resize(
        window.into(),
        gpui_kit::size(gpui_kit::px(1321.5), gpui_kit::px(820.5)),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    let size_c = content.update(cx, |app, cx| {
        let bounds = app.canvas_bounds.get().expect("latest canvas bounds");
        let size = (
            (f32::from(bounds.size.width) * app.window_scale).ceil() as u32,
            (f32::from(bounds.size.height) * app.window_scale).ceil() as u32,
        );
        assert_ne!(size, size_a);
        assert_ne!(size, size_b);
        assert_eq!(app.crop_preview_running.as_ref(), Some(&key_b));
        assert_eq!(app.crop_preview_displayed_key.as_ref(), Some(&displayed_a));
        assert_eq!(app.crop_preview_size_for_test(), Some(size_a));
        assert!(!app.canvas_overlay(cx).crop_preview_active);
        size
    });

    release_b.send(()).expect("replacement job is still gated");
    cx.run_until_parked();
    let key_c = content.update(cx, |app, cx| {
        let running = app
            .crop_preview_running
            .as_ref()
            .expect("latest preview is queued behind the stale result")
            .clone();
        assert_ne!(running, key_b);
        assert_eq!(app.crop_preview_displayed_key.as_ref(), Some(&displayed_a));
        assert_eq!(app.crop_preview_size_for_test(), Some(size_a));
        assert!(!app.canvas_overlay(cx).crop_preview_active);
        running
    });

    release_c.send(()).expect("latest job is still gated");
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert_eq!(app.crop_preview_running, None);
            assert_eq!(app.crop_preview_displayed_key.as_ref(), Some(&key_c));
            assert_eq!(app.crop_preview_size_for_test(), Some(size_c));
            assert!(app.canvas_overlay(cx).crop_preview_active);
        });
    })
    .expect("test window remains open");

    let (release_d, gate_d) = futures::channel::oneshot::channel();
    content.update(cx, |app, _| app.crop_preview_test_gate = Some(gate_d));
    cx.simulate_window_resize(
        window.into(),
        gpui_kit::size(gpui_kit::px(1401.5), gpui_kit::px(860.5)),
    );
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    let key_d = content.update(cx, |app, cx| {
        let running = app
            .crop_preview_running
            .as_ref()
            .expect("inactive preview work is running")
            .clone();
        assert_ne!(running, key_c);
        assert!(!app.canvas_overlay(cx).crop_preview_active);
        running
    });
    content.update(cx, |app, cx| {
        app.state_for_test().update(cx, |state, _| {
            state.crop_offset = (0.0, 0.0);
            state.crop_rotation = 0.0;
        });
        cx.notify();
    });
    cx.update_window(window.into(), |_, window, cx| window.render_frame(cx))
        .expect("test window remains open");
    content.update(cx, |app, _| {
        assert_eq!(app.crop_preview_running.as_ref(), Some(&key_d));
        assert_eq!(app.crop_preview_displayed_key, None);
        assert_eq!(app.crop_preview_size_for_test(), None);
    });

    release_d.send(()).expect("inactive job is still gated");
    cx.run_until_parked();
    content.update(cx, |app, _| {
        assert_eq!(app.crop_preview_running, None);
        assert_eq!(app.crop_preview_displayed_key, None);
        assert_eq!(app.crop_preview_size_for_test(), None);
    });
}

#[gpui_kit::test]
fn modern_crop_preview_tracks_size_target_key_and_ignores_non_size_stale_target(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let (release_size_a, gate_size_a) = futures::channel::oneshot::channel();
    let (release_size_b, gate_size_b) = futures::channel::oneshot::channel();

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(4, 4, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_mode = crate::crop_aspect::CropMode::Size;
                state.crop_size_target = Some((4, 4));
                state.crop_rect = Some([0.0, 0.0, 4.0, 4.0]);
                state.crop_offset = (0.0, 0.0);
                state.crop_rotation = 0.0;
            });
            app.crop_preview_test_gate = Some(gate_size_a);
        });
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    cx.run_until_parked();

    let key_size_a = content.update(cx, |app, _| {
        assert!(app.crop_preview_displayed_key.is_none());
        assert!(app.crop_preview_test_gate.is_none());
        app.crop_preview_running
            .clone()
            .expect("identity Size transform schedules preview")
    });
    content.update(cx, |app, cx| {
        app.state_for_test()
            .update(cx, |state, _| state.crop_size_target = Some((3, 2)));
        app.crop_preview_test_gate = Some(gate_size_b);
        cx.notify();
    });
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    content.update(cx, |app, _| {
        assert_eq!(app.crop_preview_running.as_ref(), Some(&key_size_a));
        assert!(app.crop_preview_test_gate.is_some());
        assert!(app.crop_preview_displayed_key.is_none());
    });

    release_size_a
        .send(())
        .expect("first Size preview remains gated");
    cx.run_until_parked();
    let key_size_b = content.update(cx, |app, _| {
        let running = app
            .crop_preview_running
            .clone()
            .expect("changed Size target schedules a replacement");
        assert_ne!(running, key_size_a);
        assert!(app.crop_preview_test_gate.is_none());
        assert!(app.crop_preview_displayed_key.is_none());
        running
    });
    release_size_b
        .send(())
        .expect("replacement Size preview remains gated");
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert_eq!(app.crop_preview_running, None);
            assert_eq!(app.crop_preview_displayed_key.as_ref(), Some(&key_size_b));
            assert!(app.canvas_overlay(cx).crop_preview_active);
        });
    })
    .expect("test window remains open");

    let (release_ratio, gate_ratio) = futures::channel::oneshot::channel();
    let (_release_unused, gate_unused) = futures::channel::oneshot::channel();
    content.update(cx, |app, cx| {
        app.crop_preview_test_gate = Some(gate_ratio);
        app.state_for_test().update(cx, |state, _| {
            state.crop_mode = crate::crop_aspect::CropMode::Ratio;
            state.crop_offset = (5.0, -3.0);
            state.crop_rotation = 17.0;
        });
        cx.notify();
    });
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    cx.run_until_parked();
    let ratio_key = content.update(cx, |app, cx| {
        let running = app
            .crop_preview_running
            .clone()
            .expect("non-Size crop transform schedules preview");
        assert_ne!(running, key_size_b);
        assert_eq!(app.crop_preview_displayed_key.as_ref(), Some(&key_size_b));
        assert!(!app.canvas_overlay(cx).crop_preview_active);
        running
    });

    content.update(cx, |app, cx| {
        app.crop_preview_test_gate = Some(gate_unused);
        app.state_for_test()
            .update(cx, |state, _| state.crop_size_target = Some((6, 7)));
        cx.notify();
    });
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    content.update(cx, |app, _| {
        assert_eq!(app.crop_preview_running.as_ref(), Some(&ratio_key));
        assert!(app.crop_preview_test_gate.is_some());
    });

    release_ratio
        .send(())
        .expect("non-Size preview remains gated");
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert_eq!(app.crop_preview_running, None);
            assert_eq!(app.crop_preview_displayed_key.as_ref(), Some(&ratio_key));
            assert!(app.crop_preview_test_gate.is_some());
            assert!(app.canvas_overlay(cx).crop_preview_active);
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn modern_crop_preview_does_not_retry_a_failed_key(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let (release_failure, gate_failure) = futures::channel::oneshot::channel();
    let (_release_retry, gate_retry) = futures::channel::oneshot::channel();

    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(4, 4, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_rect = Some([0.0, 0.0, 0.0, 4.0]);
                state.crop_offset = (5.0, -3.0);
                state.crop_rotation = 17.0;
            });
            app.crop_preview_test_gate = Some(gate_failure);
        });
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    cx.run_until_parked();

    let failed_key = content.update(cx, |app, _| {
        app.crop_preview_running
            .clone()
            .expect("invalid frame preview is running")
    });
    content.update(cx, |app, _| app.crop_preview_test_gate = Some(gate_retry));
    release_failure
        .send(())
        .expect("failed preview remains gated");
    cx.run_until_parked();

    content.update(cx, |app, _| {
        assert_eq!(app.crop_preview_failed.as_ref(), Some(&failed_key));
        assert_eq!(app.crop_preview_running, None);
        assert!(app.crop_preview_test_gate.is_some());
    });
    cx.update_window(window.into(), |_, window, cx| window.render_frame(cx))
        .expect("test window remains open");
    content.update(cx, |app, _| {
        assert_eq!(app.crop_preview_failed.as_ref(), Some(&failed_key));
        assert_eq!(app.crop_preview_running, None);
        assert!(app.crop_preview_test_gate.is_some());
    });
}

#[gpui_kit::test]
fn modern_crop_without_a_frame_keeps_its_offset_preview_during_creation(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let camera =
        crate::display::canvas_bridge::Camera::new(0.0, 0.0, 1.0).expect("valid test camera");
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(4, 4, cx).expect("blank document");
            app.set_camera(camera, cx);
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_rect = None;
                state.crop_offset = (5.0, -3.0);
                state.crop_rotation = 17.0;
            });
        });
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            content.update(cx, |app, cx| app.canvas_overlay(cx).crop_preview_active),
            "Modern Crop displays its offset raster without a frame"
        );
        assert!(
            content
                .update(cx, |app, _| app.crop_preview_size_for_test())
                .is_some()
        );

        tab_to_canvas(window, cx);
        let bounds = window.find("canvas-hitbox").bounds();
        let point = |x, y| bounds.origin + gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y));
        let start = point(8.0, 8.0);
        let end = point(40.0, 40.0);
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
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );

        content.update(cx, |app, cx| {
            let state = app.state_for_test();
            let state = state.read(cx);
            assert!(state.crop_rect.is_none());
            assert_eq!(state.crop_offset, (5.0, -3.0));
            assert_eq!(state.crop_rotation, 17.0);
            assert!(app.pending_gesture.as_ref().is_some_and(|pending| {
                pending.crop_modern_create && pending.crop_modern_drag_started
            }));
            assert!(app.canvas_overlay(cx).crop_create_preview.is_some());
            assert!(app.canvas_overlay(cx).crop_preview_active);
        });

        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, _| assert!(app.pending_gesture.is_none()));
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn switching_away_from_modern_crop_clears_state_and_hides_the_preview(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(4, 4, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
                state.crop_rect = Some([0.0, 0.0, 4.0, 4.0]);
                state.crop_offset = (5.0, -3.0);
                state.crop_rotation = 17.0;
                state.crop_history = vec![crate::crop_geometry::CropSnapshot {
                    frame: [1.0, 1.0, 2.0, 2.0],
                    offset: (2.0, 3.0),
                    rotation: 10.0,
                }];
                state.crop_redo = vec![crate::crop_geometry::CropSnapshot {
                    frame: [0.0, 0.0, 1.0, 1.0],
                    offset: (-1.0, 1.0),
                    rotation: -5.0,
                }];
            });
        });
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            content.update(cx, |app, cx| app.canvas_overlay(cx).crop_preview_active),
            "the transformed preview is displayed before the tool switch"
        );
        tab_to_canvas(window, cx);
        window.press("v", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.active_tool, ToolId::Move);
        assert!(state.crop_rect.is_none());
        assert_eq!(state.crop_offset, (0.0, 0.0));
        assert_eq!(state.crop_rotation, 0.0);
        assert!(state.crop_history.is_empty() && state.crop_redo.is_empty());
        assert!(
            !content.update(cx, |app, cx| app.canvas_overlay(cx).crop_preview_active),
            "the cached crop raster is hidden outside Crop"
        );
        assert_eq!(
            content.update(cx, |app, _| app.crop_preview_size_for_test()),
            None,
            "the inactive derived raster is released"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_bar_checkboxes_toggle_real_state(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test();
        assert!(state.read(cx).crop_delete_pixels, "Delete ships checked");
        assert!(state.read(cx).crop_fill_background, "Fill BG ships checked");
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.bg_color = [24, 48, 72, 128];
                state.crop_rect = Some([0.0, 0.0, 2.0, 1.0]);
            });
            assert_eq!(
                app.canvas_overlay(cx).crop_fill_color,
                Some([24, 48, 72, 128])
            );
            app.state.update(cx, |state, _| {
                state.bg_color = [96, 48, 24, 255];
            });
            assert_eq!(
                app.canvas_overlay(cx).crop_fill_color,
                Some([96, 48, 24, 255])
            );
        });

        window.click("opt-crop-delete", cx);
        window.render_frame(cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .crop_delete_pixels,
            "clicking Delete clears the flag"
        );
        window.click("opt-crop-fill", cx);
        window.render_frame(cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .crop_fill_background,
            "clicking Fill BG clears the flag"
        );
        content.update(cx, |app, cx| {
            assert_eq!(app.canvas_overlay(cx).crop_fill_color, None);
        });
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.crop_fill_background = true;
                state.active_tool = ToolId::Move;
            });
            assert_eq!(app.canvas_overlay(cx).crop_fill_color, None);
            app.state.update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = None;
            });
            assert_eq!(app.canvas_overlay(cx).crop_fill_color, None);
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_delete_and_fill_are_keyboard_reachable_and_operable(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        content.update(cx, |app, cx| {
            app.state
                .update(cx, |state, _| state.crop_fill_use_background = false);
        });
        window.render_frame(cx);
        assert_eq!(window.find("opt-crop-delete").label(), Some("Delete"));
        assert_eq!(window.find("opt-crop-delete").checked(), Some(true));
        assert_eq!(window.find("opt-crop-fill").label(), Some("Fill BG"));
        assert_eq!(window.find("opt-crop-fill").checked(), Some(true));
        window.focus_next(cx);
        window.render_frame(cx);

        let mut delete_focused = false;
        for _ in 0..64 {
            if window.find("opt-crop-delete").focused() == Some(true) {
                delete_focused = true;
                break;
            }
            window.press("tab", cx);
            window.render_frame(cx);
        }
        assert!(delete_focused, "Tab reaches Delete");
        window.press("space", cx);
        window.render_frame(cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .crop_delete_pixels,
            "Space toggles Delete"
        );
        assert_eq!(window.find("opt-crop-delete").checked(), Some(false));

        let mut fill_focused = false;
        for _ in 0..64 {
            if window.find("opt-crop-fill").focused() == Some(true) {
                fill_focused = true;
                break;
            }
            window.press("tab", cx);
            window.render_frame(cx);
        }
        assert!(fill_focused, "Tab reaches Fill");
        window.press("space", cx);
        window.render_frame(cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .crop_fill_background,
            "Space toggles Fill"
        );
        assert_eq!(window.find("opt-crop-fill").checked(), Some(false));

        window.press("space", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .crop_fill_background,
            "Space enables Fill"
        );

        let mut use_bg_focused = false;
        for _ in 0..64 {
            if window.find("opt-crop-fill-use-bg").focused() == Some(true) {
                use_bg_focused = true;
                break;
            }
            window.press("tab", cx);
            window.render_frame(cx);
        }
        assert!(use_bg_focused, "Tab reaches Use BG");
        window.press("space", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .crop_fill_use_background,
            "Space selects Use BG"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_fill_picker_preserves_source_on_cancel_and_apply(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        let original_fg = content.read(cx).state_for_test().read(cx).fg_color;
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.bg_color = [24, 48, 72, 255];
                state.crop_fill_color = [96, 48, 24, 255];
                state.crop_fill_use_background = true;
                state.crop_rect = Some([0.0, 0.0, 2.0, 1.0]);
            });
        });
        window.render_frame(cx);
        assert!(window.find("opt-crop-fill-color").visible());
        assert!(window.find("opt-crop-fill-use-bg").visible());
        assert!(window.find("opt-crop-fill-color").label().is_some());
        assert!(window.find("opt-crop-fill-use-bg").label().is_some());

        window.click("opt-crop-fill-color", cx);
        window.render_frame(cx);
        window.click("color-picker-sv", cx);
        window.render_frame(cx);
        window.press("escape", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert!(state.crop_fill_use_background);
            assert_eq!(state.crop_fill_color, [96, 48, 24, 255]);
        }
        assert_eq!(
            content.update(cx, |app, cx| app.canvas_overlay(cx).crop_fill_color),
            Some([24, 48, 72, 255])
        );

        window.click("opt-crop-fill-color", cx);
        window.render_frame(cx);
        window.input("00FF00", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert!(!state.crop_fill_use_background);
            assert_eq!(state.crop_fill_color, [0, 255, 0, 255]);
            assert_eq!(state.fg_color, original_fg);
        }
        content.update(cx, |app, cx| {
            app.state
                .update(cx, |state, _| state.bg_color = [8, 16, 32, 255]);
            assert_eq!(
                app.canvas_overlay(cx).crop_fill_color,
                Some([0, 255, 0, 255])
            );
        });

        window.click("opt-crop-fill-use-bg", cx);
        window.render_frame(cx);
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .crop_fill_use_background
        );
        assert_eq!(
            content.update(cx, |app, cx| app.canvas_overlay(cx).crop_fill_color),
            Some([8, 16, 32, 255])
        );

        window.click("opt-crop-fill", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-crop-fill-color").is_none(),
            "the fill color controls hide when Fill BG is off"
        );
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert!(state.crop_fill_use_background);
            assert_eq!(state.crop_fill_color, [0, 255, 0, 255]);
        }
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_fill_picker_dismissal_routes_focus_to_the_expected_target(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        let canvas_focus = content.read(cx).canvas_focus_handle.clone();
        window.render_frame(cx);
        window.focus_next(cx);
        window.render_frame(cx);

        let mut trigger_focused = false;
        for _ in 0..64 {
            if window.find("opt-crop-fill-color").focused() == Some(true) {
                trigger_focused = true;
                break;
            }
            window.press("tab", cx);
            window.render_frame(cx);
        }
        assert!(trigger_focused, "Tab reaches the Crop Fill color button");
        assert_eq!(
            window.find("opt-crop-fill-color").label(),
            Some("Crop fill color")
        );
        let trigger_focus = window.focused(cx).expect("the color button has focus");

        window.press("enter", cx);
        window.render_frame(cx);
        assert!(window.find("color-picker-layer").visible());
        assert_eq!(
            window.find("color-picker-layer").focused(),
            Some(true),
            "opening the picker transfers focus to its surface"
        );

        window.press("escape", cx);
        window.render_frame(cx);
        assert_eq!(
            window.focused(cx),
            Some(trigger_focus.clone()),
            "dismissing the picker restores focus to its trigger"
        );

        window.press("enter", cx);
        window.render_frame(cx);
        assert!(window.find("color-picker-layer").visible());
        window.press("enter", cx);
        window.render_frame(cx);
        assert!(window.try_find("color-picker").is_none());
        assert_eq!(
            window.focused(cx),
            Some(trigger_focus.clone()),
            "committing with Enter restores focus to the Crop Fill button"
        );

        window.press("enter", cx);
        window.render_frame(cx);
        window.click("color-picker-ok", cx);
        window.render_frame(cx);
        assert!(window.try_find("color-picker").is_none());
        assert_eq!(
            window.focused(cx),
            Some(trigger_focus.clone()),
            "committing with OK restores focus to the Crop Fill button"
        );

        window.press("enter", cx);
        window.render_frame(cx);
        window.click_at(
            "color-picker-layer",
            Point {
                x: gpui_kit::px(40.),
                y: gpui_kit::px(700.),
            },
            cx,
        );
        window.render_frame(cx);
        assert!(window.try_find("color-picker").is_none());
        assert_eq!(
            window.focused(cx),
            Some(canvas_focus),
            "an outside click keeps focus on the clicked canvas"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_fill_picker_restores_source_on_document_change(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        let previous_doc_id = content
            .read(cx)
            .active_document
            .as_ref()
            .expect("the fixture document is active")
            .doc_id
            .clone();
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.bg_color = [24, 48, 72, 255];
                state.crop_fill_color = [96, 48, 24, 255];
                state.crop_fill_use_background = true;
                state.crop_rect = Some([0.0, 0.0, 2.0, 1.0]);
            });
        });
        window.render_frame(cx);
        window.click("opt-crop-fill-color", cx);
        window.render_frame(cx);
        window.click("color-picker-sv", cx);
        window.render_frame(cx);

        window.press("ctrl-n", cx);
        window.render_frame(cx);
        window.click("new-doc-create", cx);
        window.render_frame(cx);

        let new_doc_id = content
            .read(cx)
            .active_document
            .as_ref()
            .expect("the new document is active")
            .doc_id
            .clone();
        assert_ne!(new_doc_id, previous_doc_id);
        assert!(window.try_find("color-picker").is_none());
        let state = content.read(cx).state_for_test().read(cx);
        assert!(state.color_picker.is_none());
        assert!(state.crop_fill_use_background);
        assert_eq!(state.crop_fill_color, [96, 48, 24, 255]);
        assert_eq!(
            content.update(cx, |app, cx| app.canvas_overlay(cx).crop_fill_color),
            Some([24, 48, 72, 255])
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn apply_crop_passes_the_checkbox_flags_to_core(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);

        // Defaults (both checked): destructive crop plus a fill in the
        // Background colour.
        content.update(cx, |app, cx| {
            app.state
                .update(cx, |state, _| state.crop_rect = Some([0.0, 0.0, 2.0, 1.0]));
            app.apply_crop(cx).expect("crop applies");
        });
        let bg = content.read(cx).state_for_test().read(cx).bg_color;
        assert_eq!(
            content.read(cx).state_for_test().read(cx).last_crop_flags,
            Some((true, Some(bg))),
            "checked boxes send delete + fill"
        );

        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let custom_fill = [12, 34, 56, 255];
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.crop_fill_background = true;
                state.crop_fill_use_background = false;
                state.crop_fill_color = custom_fill;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some([-2.0, 0.0, 4.0, 1.0]);
            });
            app.apply_crop(cx).expect("custom fill crop applies");
        });
        assert_eq!(
            content.read(cx).state_for_test().read(cx).last_crop_flags,
            Some((true, Some(custom_fill))),
            "Apply sends the selected custom fill colour"
        );
        let applied_fill: [u8; 4] = content.update(cx, |app, _| {
            let doc_id = app
                .active_document
                .as_ref()
                .expect("the fixture document remains active")
                .doc_id
                .clone();
            let loaded = app
                .sessions
                .get(&doc_id)
                .expect("the fixture session remains loaded")
                .loaded_document();
            let content = loaded
                .content_snapshot()
                .expect("the applied document has content");
            let background = content
                .layers
                .iter()
                .find(|layer| layer.is_background == Some(true))
                .expect("Fill BG keeps a Background layer");
            let pixels = loaded
                .registry
                .get_layer(&doc_id, &background.id)
                .expect("the Background pixels remain available");
            pixels.pixels[..4]
                .try_into()
                .expect("one RGBA pixel has four channels")
        });
        assert_eq!(
            applied_fill, custom_fill,
            "the area beyond the page uses the selected custom fill colour"
        );

        // Unchecking both must reach core as (false, None). Apply returns to
        // Move, so re-select Crop first.
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.crop_delete_pixels = false;
                state.crop_fill_background = false;
                state.crop_rect = Some([0.0, 0.0, 2.0, 1.0]);
            });
            app.apply_crop(cx).expect("crop applies");
        });
        assert_eq!(
            content.read(cx).state_for_test().read(cx).last_crop_flags,
            Some((false, None)),
            "unchecked boxes send non-destructive, no fill"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn apply_crop_clears_layer_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        assert!(
            !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .selected_layers
                .is_empty()
        );

        content.update(cx, |app, cx| {
            app.state
                .update(cx, |state, _| state.crop_rect = Some([0.0, 0.0, 2.0, 1.0]));
            app.apply_crop(cx).expect("crop applies");
        });
        window.render_frame(cx);

        let state = content.read(cx).state_for_test();
        let state = state.read(cx);
        assert!(state.selected_layers.is_empty());
        assert_eq!(state.selected_layer, 0);
        assert!(state.multi_bounds.is_none());

        assert!(window.try_find("canvas-size").is_some());
        assert!(window.try_find("prop-x").is_none());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_classic_checkbox_switches_interaction_mode(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1536.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        assert_eq!(
            window.find("opt-more").label(),
            Some("More Options"),
            "the More button has an accessible name"
        );
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_offset = (7.0, 11.0);
                state.crop_rotation = 12.0;
            });
        });
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_interaction,
            crate::crop_geometry::CropInteraction::Modern
        );
        assert!(window.try_find("opt-more").is_some());
        assert!(window.try_find("crop-classic").is_none());
        window.click("opt-more", cx);
        window.render_frame(cx);
        assert!(window.try_find("crop-classic").is_some());

        window.click("crop-classic", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_interaction,
            crate::crop_geometry::CropInteraction::Classic
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_offset,
            (7.0, 11.0)
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_rotation,
            12.0
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_more_and_classic_toggle_are_keyboard_reachable(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1536.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_offset = (7.0, 11.0);
                state.crop_rotation = 12.0;
            });
        });
        window.focus_next(cx);
        window.render_frame(cx);

        let mut more_focused = false;
        for _ in 0..64 {
            if window.find("opt-more").focused() == Some(true) {
                more_focused = true;
                break;
            }
            window.press("tab", cx);
        }
        assert!(more_focused, "Tab reaches the More button");

        window.press("space", cx);
        window.render_frame(cx);
        assert!(window.try_find("opt-crop-rotate-cw").is_some());
        assert_eq!(
            window.find("opt-crop-rotate-cw").label(),
            Some("Rotate 90° CW"),
            "the clockwise rotation button has an accessible name"
        );
        let mut rotate_focused = false;
        for _ in 0..64 {
            if window.find("opt-crop-rotate-cw").focused() == Some(true) {
                rotate_focused = true;
                break;
            }
            window.press("tab", cx);
        }
        assert!(rotate_focused, "Tab reaches the clockwise rotation button");
        window.press("space", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_rotation,
            102.0,
            "Space activates the focused rotation button"
        );

        assert!(window.try_find("crop-classic").is_some());
        assert_eq!(
            window.find("crop-classic").label(),
            Some("Classic Crop"),
            "the mode checkbox has an accessible name"
        );
        let mut classic_focused = false;
        for _ in 0..64 {
            if window.find("crop-classic").focused() == Some(true) {
                classic_focused = true;
                break;
            }
            window.press("tab", cx);
        }
        assert!(classic_focused, "Tab reaches the Classic crop checkbox");

        window.press("space", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_interaction,
            crate::crop_geometry::CropInteraction::Classic
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_offset,
            (7.0, 11.0)
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_rotation,
            102.0
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn size_mode_applies_the_requested_output_dimensions(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let assert_no_pending = |cx: &mut gpui_kit::App, stage: &str| {
            content.update(cx, |app, _| {
                let pending = app.pending_gesture.as_ref().map(|pending| {
                    (
                        pending.crop_modern_create,
                        pending.crop_modern_drag_started,
                        pending.crop_start.is_some(),
                        pending.start,
                    )
                });
                assert!(pending.is_none(), "{stage}: {pending:?}");
            });
        };
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.click("crop-ratio", cx);
        assert_no_pending(cx, "opening the ratio menu must not start a canvas gesture");
        window.render_frame(cx);
        window.render_frame(cx);
        let size_item_bounds = window.find("crop-ratio-size").bounds();
        let canvas_bounds = window.find("canvas-hitbox").bounds();
        let size_center = size_item_bounds.center();
        let size_overlaps_canvas = size_center.x >= canvas_bounds.origin.x
            && size_center.x < canvas_bounds.origin.x + canvas_bounds.size.width
            && size_center.y >= canvas_bounds.origin.y
            && size_center.y < canvas_bounds.origin.y + canvas_bounds.size.height;
        window.click("crop-ratio-size", cx);
        assert_no_pending(
            cx,
            &format!(
                "selecting Size must not start a canvas gesture: item={size_item_bounds:?}, \
                 canvas={canvas_bounds:?}, center_overlaps={size_overlaps_canvas}"
            ),
        );
        window.render_frame(cx);
        window.click("crop-size-w", cx);
        assert_no_pending(
            cx,
            "opening the width field must not start a canvas gesture",
        );
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        assert_no_pending(cx, "focusing the canvas must not start a pointer gesture");
        window.press("3", cx);
        assert_no_pending(cx, "typing the width must not start a pointer gesture");
        window.render_frame(cx);
        window.click("crop-size-h", cx);
        assert_no_pending(
            cx,
            "opening the height field must not start a canvas gesture",
        );
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        assert_no_pending(cx, "refocusing the canvas must not start a pointer gesture");
        window.press("2", cx);
        assert_no_pending(cx, "typing the height must not start a pointer gesture");
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.crop_delete_pixels = false;
                state.crop_fill_background = false;
                state.crop_rect = Some([0.0, 0.0, 2.0, 1.0]);
            });
            assert_eq!(
                app.state.read(cx).crop_size_target,
                Some((3, 2)),
                "the Size controls set the pixel target"
            );
        });
        content.update(cx, |app, cx| {
            assert!(app.pending_gesture.is_none());
            let state = app.state.read(cx);
            assert_eq!(state.crop_mode, crate::crop_aspect::CropMode::Size);
            assert_eq!(state.crop_rect, Some([0.0, 0.0, 2.0, 1.0]));
        });
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Move,
            "Enter must dispatch Crop Apply"
        );
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((3, 2))
        );

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo succeeds");
        });
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((2, 1))
        );
        content.update(cx, |app, cx| {
            app.step_history(false, cx).expect("redo succeeds");
        });
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((3, 2))
        );
        window.press("ctrl-s", cx);
        window.render_frame(cx);
    })
    .expect("test window remains open");

    let file_io = crate::services::file_io::LocalFileIo::new(
        directory.path(),
        directory.path().join("approvals.json"),
    )
    .expect("local file boundary opens");
    file_io
        .approve_path(&source)
        .expect("saved path is approved");
    let saved = crate::services::document::load_document_candidate(&file_io, &source)
        .expect("the saved crop reloads");
    let saved = saved.content_snapshot().expect("saved content is present");
    assert_eq!((saved.width, saved.height), (3.0, 2.0));
}

#[gpui_kit::test]
fn other_crop_modes_ignore_the_stored_size_target(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.crop_mode = crate::crop_aspect::CropMode::Free;
                state.crop_size_target = Some((3, 2));
                state.crop_delete_pixels = false;
                state.crop_fill_background = false;
                state.crop_rect = Some([0.0, 0.0, 2.0, 1.0]);
            });
            app.apply_crop(cx).expect("crop applies");
        });
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((2, 1))
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn size_mode_requires_a_target_without_mutating_the_document(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            app.state.update(cx, |state, _| {
                state.crop_mode = crate::crop_aspect::CropMode::Size;
                state.crop_size_target = None;
                state.crop_delete_pixels = false;
                state.crop_fill_background = false;
                state.crop_rect = Some([0.0, 0.0, 2.0, 1.0]);
            });
            assert_eq!(
                app.apply_crop(cx)
                    .expect_err("missing Size target rejects Apply"),
                "the crop Size target is missing"
            );
        });
        assert_eq!(
            content.read(cx).active_document_dims_for_test(),
            Some((2, 1))
        );
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some([0.0, 0.0, 2.0, 1.0])
        );
    })
    .expect("test window remains open");
}

/// Crop bar `Delete` toggle latency: raw MouseDown+MouseUp at the control plus
/// one `render_frame`. The kit `click` helper must not be timed (it renders
/// before dispatching). `#[ignore]`: perf probes run explicitly.
#[gpui_kit::test]
#[ignore]
fn crop_toggle_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            let position = window.find("opt-crop-delete").bounds().center();
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
        // One unmeasured warm-up.
        press(window, cx);
        window.render_frame(cx);

        let mut samples = Vec::new();
        for sample in 0..9 {
            let expected = !content
                .read(cx)
                .state_for_test()
                .read(cx)
                .crop_delete_pixels;
            let start = std::time::Instant::now();
            press(window, cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(
                content
                    .read(cx)
                    .state_for_test()
                    .read(cx)
                    .crop_delete_pixels,
                expected,
                "sample {sample} toggles the flag"
            );
        }
        let mut sorted = samples.clone();
        sorted.sort_by(f64::total_cmp);
        let p50 = sorted[sorted.len() / 2];
        let p95 = sorted[(sorted.len() as f64 * 0.95).ceil() as usize - 1];
        eprintln!("CROP_TOGGLE_MS samples={samples:?} p50={p50:.3} p95={p95:.3}");
        assert!(
            p95 <= 33.3,
            "crop toggle p95 {p95:.3} ms exceeds the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// The ratio dropdown is live. Choosing 1:1 (inside the list's 300 px
/// viewport) refits the pending frame to the largest square inside the
/// document and records the recent.
#[gpui_kit::test]
fn crop_ratio_dropdown_refits_the_pending_frame(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.click("crop-ratio", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).crop_ratio_open,
            "the ratio list expands"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_mode,
            crate::crop_aspect::CropMode::Free
        );
        let _active_row = window.find("crop-ratio-free");
        window.click("crop-ratio-preset-0", cx);
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.crop_mode,
            crate::crop_aspect::CropMode::Ratio,
            "picking a preset switches to Ratio"
        );
        assert_eq!(state.crop_aspect, Some((1, 1)));
        assert_eq!(state.recent_crop_ratios, vec![(1, 1)]);
        let rect = state.crop_rect.expect("the preset refits the frame");
        let expected =
            crate::crop_aspect::fit_crop_rect_to_aspect((1, 1), f64::from(w), f64::from(h), 0.0);
        assert_eq!(rect, expected, "1:1 fits the document height");
    })
    .expect("test window remains open");
}

/// `Lock Current Shape` is inert outside Free: the control is disabled unless
/// the mode is Free; the Size mode keeps its mode.
#[gpui_kit::test]
fn crop_lock_shape_is_inert_outside_free_mode(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.click("crop-ratio", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("crop-ratio-size", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_mode,
            crate::crop_aspect::CropMode::Size,
            "the Size row switches the mode"
        );
        window.click("crop-ratio", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("crop-ratio-lock", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.crop_mode,
            crate::crop_aspect::CropMode::Size,
            "a disabled Lock Current Shape row must not switch the mode"
        );
        assert!(
            state.crop_ratio_open,
            "an inert row must not dismiss the list either"
        );
    })
    .expect("test window remains open");
}

/// The Size mode converts entry at the crop bar's 300 PPI base,
/// keeps the pixel target authoritative across unit changes and swap.
#[gpui_kit::test]
fn crop_size_mode_converts_at_300_ppi_and_swaps(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.click("crop-ratio", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).crop_ratio_open,
            "the ratio list opens before choosing Size"
        );
        window.click("crop-ratio-size", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(
                state.crop_mode,
                crate::crop_aspect::CropMode::Size,
                "the Size row switches the mode; open={}",
                state.crop_ratio_open
            );
            assert_eq!(state.crop_size_target, Some((800, 600)));
            assert_eq!(
                (state.crop_size_w.as_str(), state.crop_size_h.as_str()),
                ("800", "600")
            );
        }

        // Switch the unit to inches: the pixel target stays 800x600.
        window.click("crop-size-unit", cx);
        window.render_frame(cx);
        window.render_frame(cx);
        window.click("crop-size-unit-in", cx);
        window.render_frame(cx);
        content.update(cx, |app, _| {
            assert!(
                app.pending_gesture.is_none() && app.crop_drag.is_none(),
                "choosing a Size unit must not leave a canvas gesture"
            );
        });
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.crop_size_unit, crate::crop_aspect::CropSizeUnit::In);
            assert_eq!(state.crop_size_target, Some((800, 600)));
            assert_eq!(
                (state.crop_size_w.as_str(), state.crop_size_h.as_str()),
                ("2.67", "2.00"),
                "800 px is 2.67 in at 300 PPI"
            );
        }

        // Type 4 into W: 4 in * 300 = 1200 px.
        window.click("crop-size-w", cx);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        // The field selects its value on focus, so the first digit replaces it.
        window.press("4", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.crop_size_target, Some((1200, 600)), "4 in at 300 PPI");
            assert_eq!(state.crop_size_w, "4");
        }

        // Swap: 1200x600 becomes 600x1200, and the buffers swap with it.
        window.click("crop-size-swap", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.crop_size_target, Some((600, 1200)));
        assert_eq!(
            (state.crop_size_w.as_str(), state.crop_size_h.as_str()),
            ("2.00", "4")
        );
    })
    .expect("test window remains open");
}

/// Perf: choosing the Size mode (`crop-ratio` -> `crop-ratio-size`)
/// repaints the bar with the W/H fields and the unit trigger.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn crop_size_mode_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        let reopen = |content: &gpui_kit::Entity<App>, cx: &mut gpui_kit::App| {
            content.update(cx, |app, cx| {
                app.state_for_test().update(cx, |state, _| {
                    state.crop_ratio_open = true;
                    state.crop_mode = crate::crop_aspect::CropMode::Free;
                });
            });
        };
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            let position = window.find("crop-ratio-size").bounds().center();
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
        reopen(&content, cx);
        window.render_frame(cx);
        window.render_frame(cx);
        press(window, cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            reopen(&content, cx);
            window.render_frame(cx);
            window.render_frame(cx);
            let start = std::time::Instant::now();
            press(window, cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "crop_size_mode p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "crop size mode p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// Perf: opening/closing the Aspect Ratio dropdown rebuilds the list and
/// repaints the option bar.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn crop_ratio_open_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        // The press helper renders around the dispatch, so the replayed pair of
        // events plus one frame is the event-to-frame metric.
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            let position = window.find("crop-ratio").bounds().center();
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
        // One unmeasured warm-up.
        press(window, cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            let start = std::time::Instant::now();
            press(window, cx);
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "crop_ratio_open p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "crop ratio open p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

/// At a 1536 px layout width the crop bar paints two narrow field-filled boxes
/// near the left. Enumerate the bar's controls at that width instead of
/// reading the answer back off pixels.
#[gpui_kit::test]
fn crop_bar_keeps_every_control_at_the_reference_width(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1536.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);

        let ids = [
            "crop-ratio",
            "opt-crop-delete",
            "opt-crop-fill",
            "opt-cancel",
            "opt-apply",
        ];
        let missing: Vec<&str> = ids
            .iter()
            .copied()
            .filter(|id| window.try_find(*id).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "the crop bar dropped controls at the reference width: {missing:?}"
        );
        for id in ids {
            let bounds = window.find(id).bounds();
            assert!(
                f32::from(bounds.size.width) > 0.0 && f32::from(bounds.size.height) > 0.0,
                "{id} is laid out with no area at the reference width"
            );
        }
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_bar_places_classic_toggle_inline_at_wide_width(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-more").is_none(),
            "the More trigger stays folded out at a wide bar width"
        );
        assert!(window.try_find("crop-classic").is_some());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn double_clicking_the_crop_interior_applies_the_crop(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some([50.0, 50.0, 300.0, 200.0]);
            });
        });
        window.render_frame(cx);

        let (bounds, placement) = page(window, (400, 300));
        let interior = window_point(bounds, placement, (400, 300), 200.0, 150.0);
        let history_before = content
            .read(cx)
            .active_core_history_cursor_for_test()
            .expect("document history cursor");
        double_click_canvas_at(window, interior, cx);

        assert_eq!(
            content
                .read(cx)
                .active_document_size()
                .expect("cropped document size"),
            (300.0, 200.0),
            "a double-click in the crop move area applies its region"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Move
        );
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_none(),
            "Apply clears the pending crop frame"
        );
        let history_after = content
            .read(cx)
            .active_core_history_cursor_for_test()
            .expect("document history cursor");
        assert_eq!(history_after, history_before + 1);

        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo crop");
        });
        assert_eq!(
            content
                .read(cx)
                .active_document_size()
                .expect("restored document size"),
            (400.0, 300.0)
        );
        assert_eq!(
            content
                .read(cx)
                .active_core_history_cursor_for_test()
                .expect("document history cursor"),
            history_before
        );
        content.update(cx, |app, cx| {
            app.step_history(false, cx).expect("redo crop");
        });
        assert_eq!(
            content
                .read(cx)
                .active_document_size()
                .expect("redone document size"),
            (300.0, 200.0)
        );
        assert_eq!(
            content
                .read(cx)
                .active_core_history_cursor_for_test()
                .expect("document history cursor"),
            history_after
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn double_clicking_the_classic_crop_interior_applies_the_crop(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some([50.0, 50.0, 300.0, 200.0]);
            });
        });
        window.render_frame(cx);

        let (bounds, placement) = page(window, (400, 300));
        let interior = window_point(bounds, placement, (400, 300), 200.0, 150.0);
        double_click_canvas_at(window, interior, cx);

        assert_eq!(
            content
                .read(cx)
                .active_document_size()
                .expect("cropped document size"),
            (300.0, 200.0)
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Move
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn double_clicking_a_crop_handle_does_not_apply_the_crop(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some([50.0, 50.0, 300.0, 200.0]);
            });
        });
        window.render_frame(cx);

        let (bounds, placement) = page(window, (400, 300));
        let handle = window_point(bounds, placement, (400, 300), 50.0, 50.0);
        double_click_canvas_at(window, handle, cx);

        assert_eq!(
            content
                .read(cx)
                .active_document_size()
                .expect("document size"),
            (400.0, 300.0)
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Crop
        );
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some([50.0, 50.0, 300.0, 200.0])
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn double_clicking_while_a_crop_drag_is_active_does_not_apply_the_crop(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some([50.0, 50.0, 300.0, 200.0]);
            });
        });
        window.render_frame(cx);

        let (bounds, placement) = page(window, (400, 300));
        let handle = window_point(bounds, placement, (400, 300), 50.0, 50.0);
        let interior = window_point(bounds, placement, (400, 300), 200.0, 150.0);
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: handle,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, _| assert!(app.crop_drag.is_some()));

        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: interior,
                click_count: 2,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content
                .read(cx)
                .active_document_size()
                .expect("document size"),
            (400.0, 300.0)
        );

        window.press("escape", cx);
        window.render_frame(cx);
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: interior,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some([50.0, 50.0, 300.0, 200.0])
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Crop
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn double_clicking_outside_the_crop_frame_does_not_apply_the_crop(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some([50.0, 50.0, 300.0, 200.0]);
            });
        });
        window.render_frame(cx);

        let (bounds, placement) = page(window, (400, 300));
        let outside = window_point(bounds, placement, (400, 300), 25.0, 25.0);
        double_click_canvas_at(window, outside, cx);

        assert_eq!(
            content
                .read(cx)
                .active_document_size()
                .expect("document size"),
            (400.0, 300.0)
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Crop
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn double_clicking_during_space_navigation_does_not_apply_the_crop(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some([50.0, 50.0, 300.0, 200.0]);
            });
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);

        let (bounds, placement) = page(window, (400, 300));
        let interior = window_point(bounds, placement, (400, 300), 200.0, 150.0);
        dispatch(
            window,
            PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
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
        double_click_canvas_at(window, interior, cx);
        dispatch(
            window,
            PlatformInput::KeyUp(gpui_kit::KeyUpEvent {
                keystroke: gpui_kit::Keystroke {
                    modifiers: gpui_kit::Modifiers::default(),
                    key: "space".to_string(),
                    key_char: None,
                },
            }),
            cx,
        );

        assert_eq!(
            content
                .read(cx)
                .active_document_size()
                .expect("document size"),
            (400.0, 300.0)
        );
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some([50.0, 50.0, 300.0, 200.0])
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Crop
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_guide_selector_is_reachable_at_wide_width(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        assert!(
            window.try_find("crop-guide").is_some(),
            "the guide selector is inline when the option bar is wide"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_guide_menu_is_keyboard_operable_and_updates_the_canvas_overlay(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1536.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.focus_next(cx);

        let mut more_focused = false;
        for _ in 0..64 {
            if window.find("opt-more").focused() == Some(true) {
                more_focused = true;
                break;
            }
            window.press("tab", cx);
        }
        assert!(more_focused, "Tab reaches the More button");
        window.press("space", cx);
        window.render_frame(cx);

        let mut guide_focused = false;
        for _ in 0..64 {
            if window
                .try_find("crop-guide")
                .is_some_and(|guide| guide.focused() == Some(true))
            {
                guide_focused = true;
                break;
            }
            window.press("tab", cx);
        }
        assert!(guide_focused, "Tab reaches the composition guide selector");
        assert_eq!(
            window.find("crop-guide").label(),
            Some("Guides: Thirds"),
            "the guide selector has an accessible name"
        );

        window.press("space", cx);
        window.render_frame(cx);
        let menu_open = window.find("popup-menu");
        assert!(
            menu_open.visible(),
            "Space opens the guide menu: {menu_open:?}"
        );
        assert_eq!(
            menu_open.focused(),
            Some(true),
            "the open guide menu receives focus: {menu_open:?}"
        );
        window.press("escape", cx);
    })
    .expect("test window remains open");
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("crop-guide").focused(),
            Some(true),
            "Escape returns focus to the guide selector"
        );
    })
    .expect("test window remains open");

    for (down_count, guide) in [
        (1, crate::crop_geometry::CropGuides::None),
        (2, crate::crop_geometry::CropGuides::Thirds),
        (3, crate::crop_geometry::CropGuides::Grid),
        (4, crate::crop_geometry::CropGuides::Diagonal),
        (5, crate::crop_geometry::CropGuides::Golden),
    ] {
        cx.update_window(window.into(), |_, window, cx| {
            assert_eq!(
                window.find("crop-guide").focused(),
                Some(true),
                "the guide selector keeps focus before opening the next menu"
            );
            window.press("space", cx);
            window.render_frame(cx);
            for _ in 0..down_count {
                window.press("down", cx);
            }
            window.press("enter", cx);
        })
        .expect("test window remains open");
        cx.run_until_parked();

        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                content.read(cx).state_for_test().read(cx).crop_guides,
                guide
            );
            assert_eq!(
                content.update(cx, |app, cx| app.canvas_overlay(cx).crop_guides),
                guide
            );
            assert_eq!(
                window.find("crop-guide").focused(),
                Some(true),
                "selecting a guide returns focus to its selector"
            );
        })
        .expect("test window remains open");
    }
}

#[gpui_kit::test]
fn crop_guide_menu_accepts_pointer_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        window.click("crop-guide", cx);
        assert!(window.try_find("popup-menu").is_some());
        window.within("popup-menu").click(4usize, cx);
        window.render_frame(cx);

        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_guides,
            crate::crop_geometry::CropGuides::Golden
        );
        assert_eq!(
            content.update(cx, |app, cx| app.canvas_overlay(cx).crop_guides),
            crate::crop_geometry::CropGuides::Golden
        );
        assert_eq!(window.find("crop-guide").label(), Some("Guides: Golden"));
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_quarter_turn_buttons_update_both_modes_and_mini_history(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        let modern_frame = [20.0, 30.0, 100.0, 60.0];
        let modern_offset = (7.0, -5.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_rect = Some(modern_frame);
                state.crop_offset = modern_offset;
                state.crop_rotation = 15.0;
                state.crop_history.clear();
                state.crop_redo.clear();
            });
        });
        window.render_frame(cx);

        assert!(
            window.try_find("opt-crop-rotate-cw").is_some(),
            "the wide Crop bar exposes clockwise rotation"
        );
        assert!(
            window.try_find("opt-crop-rotate-ccw").is_some(),
            "the wide Crop bar exposes counterclockwise rotation"
        );
        window.click("opt-crop-rotate-cw", cx);
        let state = content.read(cx).state_for_test().clone();
        assert_eq!(state.read(cx).crop_rect, Some(modern_frame));
        assert_eq!(state.read(cx).crop_offset, modern_offset);
        assert_eq!(state.read(cx).crop_rotation, 105.0);
        assert_eq!(state.read(cx).crop_history.len(), 1);
        assert!(content.update(cx, |app, cx| app.crop_undo(cx)));
        assert_eq!(state.read(cx).crop_rotation, 15.0);
        assert!(content.update(cx, |app, cx| app.crop_redo(cx)));
        assert_eq!(state.read(cx).crop_rotation, 105.0);

        let classic_frame = [50.0, 40.0, 120.0, 80.0];
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some(classic_frame);
                state.crop_offset = (0.0, 0.0);
                state.crop_rotation = -10.0;
                state.crop_history.clear();
                state.crop_redo.clear();
            });
        });
        window.render_frame(cx);
        window.click("opt-crop-rotate-ccw", cx);
        let state = content.read(cx).state_for_test().clone();
        assert_eq!(state.read(cx).crop_rect, Some([70.0, 20.0, 80.0, 120.0]));
        assert_eq!(state.read(cx).crop_rotation, -100.0);
        assert!(content.update(cx, |app, cx| app.crop_undo(cx)));
        assert_eq!(state.read(cx).crop_rect, Some(classic_frame));
        assert_eq!(state.read(cx).crop_rotation, -10.0);
        assert!(content.update(cx, |app, cx| app.crop_redo(cx)));
        assert_eq!(state.read(cx).crop_rect, Some([70.0, 20.0, 80.0, 120.0]));
        assert_eq!(state.read(cx).crop_rotation, -100.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
#[ignore = "requires GPUI_A11Y_ACTIVE=1; run explicitly"]
fn crop_straighten_slider_exposes_accessible_step_actions(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);

        let tree = window
            .debug_a11y_tree_json()
            .expect("accessibility tree is active");
        let tree: serde_json::Value =
            serde_json::from_str(&tree).expect("accessibility tree is valid JSON");
        let node = tree["nodes"]
            .as_object()
            .expect("accessibility nodes are serialized")
            .values()
            .find(|node| node["aria"]["label"] == "Straighten")
            .expect("the straighten slider is present in the accessibility tree");
        assert_eq!(node["aria"]["role"], "Slider");
        assert_eq!(node["aria"]["numeric_value_step"], 0.1);
        let actions = node["aria"]["on_action"]
            .as_array()
            .expect("slider actions are exposed");
        for action in ["Increment", "Decrement"] {
            assert!(actions.iter().any(|value| value == action), "{action}");
        }
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_straighten_slider_is_keyboard_operable_and_has_one_undo_per_key(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    let frame = [20.0, 30.0, 100.0, 60.0];
    let offset = (7.0, -5.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_rect = Some(frame);
                state.crop_offset = offset;
                state.crop_rotation = 0.0;
                state.crop_history.clear();
                state.crop_redo.clear();
            });
        });
        window.render_frame(cx);
        assert!(
            window.try_find("opt-crop-straighten").is_some(),
            "the wide Crop bar exposes the straighten slider"
        );
        assert_eq!(
            window.find("opt-crop-straighten").label(),
            Some("Straighten"),
            "the slider has an accessible name"
        );

        window.focus_next(cx);
        window.render_frame(cx);
        let mut slider_focused = false;
        for _ in 0..64 {
            if window.find("opt-crop-straighten").focused() == Some(true) {
                slider_focused = true;
                break;
            }
            window.press("tab", cx);
        }
        assert!(slider_focused, "Tab reaches the straighten slider");

        window.press("right", cx);
        let state = content.read(cx).state_for_test().clone();
        assert_eq!(state.read(cx).crop_rotation, 0.1);
        assert_eq!(state.read(cx).crop_history.len(), 1);
        assert_eq!(state.read(cx).crop_history[0].rotation, 0.0);

        window.press("home", cx);
        assert_eq!(state.read(cx).crop_rotation, -45.0);
        assert_eq!(state.read(cx).crop_history.len(), 2);
        window.press("end", cx);
        assert_eq!(state.read(cx).crop_rotation, 45.0);
        assert_eq!(state.read(cx).crop_history.len(), 3);

        assert!(content.update(cx, |app, cx| app.crop_undo(cx)));
        assert_eq!(state.read(cx).crop_rotation, -45.0);
        assert!(content.update(cx, |app, cx| app.crop_redo(cx)));
        assert_eq!(state.read(cx).crop_rotation, 45.0);
        assert_eq!(state.read(cx).crop_rect, Some(frame));
        assert_eq!(state.read(cx).crop_offset, offset);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_straighten_pointer_changes_and_double_click_reset_are_undoable(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_rect = Some([20.0, 30.0, 100.0, 60.0]);
                state.crop_rotation = 0.0;
                state.crop_history.clear();
                state.crop_redo.clear();
            });
        });
        window.render_frame(cx);

        let bounds = window.find("opt-crop-straighten").bounds();
        let at = |fraction: f32| {
            gpui_kit::point(
                bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * fraction),
                bounds.origin.y + gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            )
        };
        let move_to = |window: &mut gpui_kit::Window,
                       position: gpui_kit::Point<gpui_kit::Pixels>,
                       cx: &mut gpui_kit::App| {
            dispatch(
                window,
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    ..Default::default()
                }),
                cx,
            );
        };

        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at(0.5),
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        move_to(window, at(0.6), cx);
        move_to(window, at(0.7), cx);
        let outside = at(1.2);
        move_to(window, outside, cx);
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: outside,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );

        let state = content.read(cx).state_for_test().clone();
        assert_eq!(state.read(cx).crop_rotation, 45.0);
        assert_eq!(state.read(cx).crop_history.len(), 3);
        assert_eq!(
            state
                .read(cx)
                .crop_history
                .iter()
                .map(|snapshot| snapshot.rotation)
                .collect::<Vec<_>>(),
            [0.0, 9.0, 18.0],
            "each distinct slider input records the preceding rotation"
        );

        for click_count in [1, 2] {
            let position = at(1.0);
            dispatch(
                window,
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    click_count,
                    ..Default::default()
                }),
                cx,
            );
            dispatch(
                window,
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    click_count,
                    ..Default::default()
                }),
                cx,
            );
        }
        assert_eq!(state.read(cx).crop_rotation, 0.0);
        assert_eq!(state.read(cx).crop_history.len(), 4);
        assert!(content.update(cx, |app, cx| app.crop_undo(cx)));
        assert_eq!(state.read(cx).crop_rotation, 45.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_straighten_capture_cleanup_waits_for_another_capture_and_handles_mouse_up(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_rect = Some([20.0, 30.0, 100.0, 60.0]);
                state.crop_rotation = 0.0;
                state.crop_history.clear();
                state.crop_redo.clear();
            });
        });
        window.render_frame(cx);

        let bounds = window.find("opt-crop-straighten").bounds();
        let at = |fraction: f32| {
            gpui_kit::point(
                bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * fraction),
                bounds.origin.y + gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            )
        };
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at(0.5),
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(0.65),
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_rotation,
            13.5
        );

        window.release_pointer();
        let zoom_bounds = window.find("navigator-zoom-track").bounds();
        let zoom_at = |fraction: f32| {
            gpui_kit::point(
                zoom_bounds.origin.x + gpui_kit::px(f32::from(zoom_bounds.size.width) * fraction),
                zoom_bounds.origin.y + gpui_kit::px(f32::from(zoom_bounds.size.height) * 0.5),
            )
        };
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: zoom_at(0.5),
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        assert!(window.captured_hitbox().is_some());
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .slider_capture
                .get()
                .is_some(),
            "the straighten capture remains stored while the zoom track owns the pointer"
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: zoom_at(0.75),
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_rotation,
            13.5,
            "another active capture must not end the straighten drag"
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: zoom_at(0.75),
                ..Default::default()
            }),
            cx,
        );
        window.release_pointer();
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(0.9),
                ..Default::default()
            }),
            cx,
        );
        let state = content.read(cx).state_for_test().clone();
        assert_eq!(state.read(cx).crop_rotation, 13.5);
        assert!(
            state.read(cx).slider_capture.get().is_none(),
            "lost capture clears the slider's stored hitbox"
        );
        let app = content.read(cx);
        assert!(!app.slider_dragging);
        assert!(app.crop_straighten_document_id.is_none());

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_rotation = 0.0;
                state.crop_history.clear();
                state.crop_redo.clear();
            });
        });
        window.render_frame(cx);
        let bounds = window.find("opt-crop-straighten").bounds();
        let at = |fraction: f32| {
            gpui_kit::point(
                bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * fraction),
                bounds.origin.y + gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            )
        };
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at(0.5),
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(0.65),
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_rotation,
            13.5
        );
        window.release_pointer();
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: at(0.9),
                ..Default::default()
            }),
            cx,
        );
        let state = content.read(cx).state_for_test().clone();
        assert!(state.read(cx).slider_capture.get().is_none());
        let app = content.read(cx);
        assert!(!app.slider_dragging);
        assert!(app.crop_straighten_document_id.is_none());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_straighten_drag_stops_after_same_document_reload(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_rect = Some([20.0, 30.0, 100.0, 60.0]);
                state.crop_rotation = 0.0;
                state.crop_history.clear();
                state.crop_redo.clear();
            });
        });
        window.render_frame(cx);

        let bounds = window.find("opt-crop-straighten").bounds();
        let at = |fraction: f32| {
            gpui_kit::point(
                bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * fraction),
                bounds.origin.y + gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            )
        };
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at(0.5),
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(0.65),
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_rotation,
            13.5
        );

        let original_id = content
            .read(cx)
            .active_doc_id()
            .expect("active document id");
        content.update(cx, |app, cx| {
            let file_io = app.file_io.clone().expect("file boundary is open");
            app.open_startup_document_for_test(&source, file_io.as_ref(), cx)
                .expect("the active document reloads");
            assert_eq!(app.active_doc_id().as_deref(), Ok(original_id.as_str()));
        });
        window.render_frame(cx);
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(0.9),
                ..Default::default()
            }),
            cx,
        );

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).crop_rotation, 0.0);
        let app = content.read(cx);
        assert!(!app.slider_dragging);
        assert!(app.crop_straighten_document_id.is_none());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn opening_a_modal_stops_crop_straighten_drag_before_pointer_up(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_rect = Some([20.0, 30.0, 100.0, 60.0]);
                state.crop_rotation = 0.0;
                state.crop_history.clear();
                state.crop_redo.clear();
            });
        });
        window.render_frame(cx);

        let bounds = window.find("opt-crop-straighten").bounds();
        let at = |fraction: f32| {
            gpui_kit::point(
                bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * fraction),
                bounds.origin.y + gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            )
        };
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at(0.5),
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(0.65),
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_rotation,
            13.5
        );

        window.press("ctrl-n", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::NewDocument)
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(0.9),
                ..Default::default()
            }),
            cx,
        );

        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).crop_rotation, 13.5);
        assert_eq!(
            state.read(cx).active_dialog,
            Some(crate::state::DialogId::NewDocument)
        );
        let app = content.read(cx);
        assert!(!app.slider_dragging);
        assert!(app.crop_straighten_document_id.is_none());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_straighten_drag_does_not_resume_after_switching_documents(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    cx.update_window(window.into(), |_, window, cx| {
        let (first_id, second_id) = content.update(cx, |app, cx| {
            let first_id = app.active_doc_id().expect("fixture document id");
            app.create_document(300, 200, cx).expect("second document");
            let second_id = app.active_doc_id().expect("second document id");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some([20.0, 30.0, 100.0, 60.0]);
                state.crop_rotation = 0.0;
                state.crop_history.clear();
                state.crop_redo.clear();
            });
            (first_id, second_id)
        });
        window.render_frame(cx);

        let bounds = window.find("opt-crop-straighten").bounds();
        let at = |fraction: f32| {
            gpui_kit::point(
                bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * fraction),
                bounds.origin.y + gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            )
        };
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at(0.5),
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(0.65),
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_rotation,
            13.5
        );

        content.update(cx, |app, cx| {
            app.activate_document(&first_id, cx)
                .expect("switch to first document");
            app.activate_document(&second_id, cx)
                .expect("switch back during the drag");
        });
        window.render_frame(cx);
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(0.9),
                ..Default::default()
            }),
            cx,
        );
        let state = content.read(cx).state_for_test();
        assert_eq!(
            content.read(cx).active_doc_id().as_deref(),
            Ok(second_id.as_str())
        );
        assert_eq!(state.read(cx).crop_rotation, 0.0);
        assert!(state.read(cx).crop_history.is_empty());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_straighten_drag_does_not_resume_after_tool_switch(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1600.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_rect = Some([20.0, 30.0, 100.0, 60.0]);
                state.crop_rotation = 0.0;
                state.crop_history.clear();
                state.crop_redo.clear();
            });
        });
        window.render_frame(cx);

        let bounds = window.find("opt-crop-straighten").bounds();
        let at = |fraction: f32| {
            gpui_kit::point(
                bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * fraction),
                bounds.origin.y + gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            )
        };
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at(0.5),
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(0.65),
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_rotation,
            13.5
        );

        content.update(cx, |app, cx| {
            app.select_canvas_tool(ToolId::Move, cx);
            app.select_canvas_tool(ToolId::Crop, cx);
        });
        window.render_frame(cx);
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at(0.9),
                ..Default::default()
            }),
            cx,
        );
        let state = content.read(cx).state_for_test();
        assert_eq!(state.read(cx).active_tool, ToolId::Crop);
        assert_eq!(state.read(cx).crop_rotation, 0.0);
        assert!(state.read(cx).crop_history.is_empty());
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn crop_angle_field_commits_on_enter_and_escape_discards_the_draft(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app_sized(cx, &directory, 1536.0);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_rect = Some([20.0, 30.0, 100.0, 60.0]);
                state.crop_rotation = 12.0;
                state.crop_history.clear();
                state.crop_redo.clear();
            });
        });
        window.render_frame(cx);
        window.click("opt-more", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-crop-angle").is_some(),
            "More exposes the editable angle field"
        );
        assert!(
            window.try_find("opt-crop-straighten").is_some(),
            "More exposes the straighten slider"
        );
        window.focus_next(cx);
        window.render_frame(cx);
        let mut angle_focused = false;
        for _ in 0..64 {
            if window.find("opt-crop-angle").focused() == Some(true) {
                angle_focused = true;
                break;
            }
            window.press("tab", cx);
        }
        assert!(angle_focused, "Tab reaches the angle field in More");
        window.press("8", cx);
        window.press(".", cx);
        window.press("5", cx);
        let state = content.read(cx).state_for_test().clone();
        assert_eq!(
            state.read(cx).crop_rotation,
            12.0,
            "typing edits only the angle draft"
        );
        assert_eq!(
            state
                .read(cx)
                .num_field
                .as_ref()
                .map(|field| field.buffer.as_str()),
            Some("8.5")
        );

        window.press("enter", cx);
        assert_eq!(state.read(cx).crop_rotation, 8.5);
        assert_eq!(state.read(cx).crop_history.len(), 1);
        assert_eq!(state.read(cx).crop_history[0].rotation, 12.0);
        assert!(content.update(cx, |app, cx| app.crop_undo(cx)));
        assert_eq!(state.read(cx).crop_rotation, 12.0);
        assert!(content.update(cx, |app, cx| app.crop_redo(cx)));
        assert_eq!(state.read(cx).crop_rotation, 8.5);

        window.click("opt-crop-angle", cx);
        window.press("9", cx);
        assert_eq!(state.read(cx).crop_rotation, 8.5);
        window.press("escape", cx);
        assert_eq!(state.read(cx).crop_rotation, 8.5);
        assert_eq!(state.read(cx).crop_history.len(), 1);

        window.click("opt-crop-angle", cx);
        window.press("-", cx);
        window.press("enter", cx);
        assert_eq!(state.read(cx).crop_rotation, 8.5);
        assert_eq!(state.read(cx).crop_history.len(), 1);

        content.update(cx, |app, cx| app.set_crop_rotation(f64::NAN, cx));
        assert_eq!(state.read(cx).crop_rotation, 8.5);
        assert_eq!(state.read(cx).crop_history.len(), 1);
        content.update(cx, |app, cx| app.set_crop_rotation(f64::INFINITY, cx));
        assert_eq!(state.read(cx).crop_rotation, 8.5);
        assert_eq!(state.read(cx).crop_history.len(), 1);
    })
    .expect("test window remains open");
}

/// A document change resets the crop frame and its mini-undo stacks. The
/// fresh page seeds a full-frame crop for the new document size.
#[gpui_kit::test]
fn switching_documents_resets_the_crop_frame_and_history(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("first document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some([20.0, 20.0, 200.0, 150.0]);
                state.crop_history = vec![crate::crop_geometry::CropSnapshot {
                    frame: [10.0, 10.0, 100.0, 100.0],
                    offset: (0.0, 0.0),
                    rotation: 0.0,
                }];
                state.crop_redo = vec![crate::crop_geometry::CropSnapshot {
                    frame: [5.0, 5.0, 50.0, 50.0],
                    offset: (0.0, 0.0),
                    rotation: 0.0,
                }];
            });
        });
        let first_doc = content
            .read(cx)
            .active_document_id_for_test()
            .expect("first document id");
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("second document");
        });
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some([0.0, 0.0, 200.0, 150.0]),
            "creating a document re-seeds the crop frame for its page"
        );
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert!(
                state.crop_history.is_empty() && state.crop_redo.is_empty(),
                "the previous document's crop stacks do not survive"
            );
        }

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_rect = Some([15.0, 15.0, 90.0, 80.0]);
                state.crop_history = vec![crate::crop_geometry::CropSnapshot {
                    frame: [1.0, 1.0, 2.0, 2.0],
                    offset: (0.0, 0.0),
                    rotation: 0.0,
                }];
            });
        });
        content.update(cx, |app, cx| {
            app.activate_document(&first_doc, cx)
                .expect("first document activates");
        });
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).crop_rect_for_test(cx),
            Some([0.0, 0.0, 400.0, 300.0]),
            "switching tabs re-seeds the crop frame for the target page"
        );
        let state = content.read(cx).state_for_test().read(cx);
        assert!(
            state.crop_history.is_empty() && state.crop_redo.is_empty(),
            "switching tabs clears the crop stacks"
        );
    })
    .expect("test window remains open");
}
