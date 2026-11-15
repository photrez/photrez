//! The crop interaction modes: Modern, where the frame stays where it is and
//! the picture moves behind it, and Classic, where the frame moves over a
//! still picture. The session undo, the arrow keys and Escape belong here
//! too, because they answer the mode.

use super::*;

/// The display receives the picture offset, so the page slides behind the
/// frame while the frame stays where it is.
#[gpui_kit::test]
fn the_picture_offset_reaches_the_canvas_overlay(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some([100.0, 100.0, 100.0, 100.0]);
            });
        });
        window.render_frame(cx);
        let (_, placement) = page(window, (400, 300));
        let zoom = f64::from(placement.2 / 400.0);
        drag_doc(window, (400, 300), (150.0, 150.0), (170.0, 160.0), cx);
        let overlay = content
            .update(cx, |app, cx| app.crop_overlay_for_test(cx))
            .expect("the frame");
        assert_eq!(overlay.0, [100.0, 100.0, 100.0, 100.0], "the frame stays");
        assert!(
            (overlay.2.0 - 20.0 * zoom).abs() < 0.5 && (overlay.2.1 - 10.0 * zoom).abs() < 0.5,
            "the paint pass receives the picture offset: {:?}",
            overlay.2
        );
    })
    .expect("test window remains open");
}

/// Classic interaction: a drag inside the frame moves the frame and a corner
/// drag resizes it with the opposite corner put.
#[gpui_kit::test]
fn classic_interaction_moves_the_frame_and_resizes_from_a_corner(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
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

/// The arrow keys move the crop session: the picture in Modern interaction,
/// the frame in Classic interaction.
#[gpui_kit::test]
fn arrow_keys_move_the_picture_in_modic_and_the_frame_in_classic(
    cx: &mut gpui_kit::TestAppContext,
) {
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
        let state = content.read(cx).state_for_test().read(cx);
        let nudged = state.crop_rect.expect("the crop frame survives the nudge");
        assert_eq!(nudged, before, "Modern interaction keeps the frame");
        assert_eq!(
            state.crop_offset,
            (1.0, 0.0),
            "the arrow key moves the picture behind the frame"
        );

        // Classic interaction moves the frame over the still picture.
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_offset = (0.0, 0.0);
            });
        });
        window.render_frame(cx);
        window.press("right", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        let nudged = state.crop_rect.expect("the crop frame survives the nudge");
        assert_eq!(
            nudged[0],
            before[0] + 1.0,
            "Classic interaction moves the frame"
        );
        assert_eq!(nudged[1], before[1]);
        assert_eq!(nudged[2], before[2]);
        assert_eq!(nudged[3], before[3]);
        assert_eq!(state.crop_offset, (0.0, 0.0), "the picture stays put");
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
        let offset = content.read(cx).state_for_test().read(cx).crop_offset;
        assert_eq!(offset, (1.0, 0.0), "the arrow key moves the picture");
        let history_before = content
            .read(cx)
            .active_core_history_cursor_for_test()
            .unwrap();

        window.press("ctrl-z", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.crop_offset,
            (0.0, 0.0),
            "Ctrl+Z restores the previous crop session"
        );
        assert_eq!(
            state.crop_rect.expect("frame"),
            frame,
            "and the frame with it"
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
            content.read(cx).state_for_test().read(cx).crop_offset,
            offset,
            "Ctrl+Shift+Z redoes the crop step"
        );
    })
    .expect("test window remains open");
}

/// Escape clears the crop session and keeps the Crop tool armed.
#[gpui_kit::test]
fn escape_clears_the_crop_session_and_keeps_the_tool(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.set_canvas_tool_action(&SetCanvasTool(ToolId::Crop), cx);
        });
        window.render_frame(cx);
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_some(),
            "arming the crop seeds the frame"
        );
        tab_to_canvas(window, cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).crop_rect_for_test(cx).is_none(),
            "Escape discards the crop frame"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Crop,
            "Escape keeps the Crop tool armed"
        );
    })
    .expect("test window remains open");
}

/// The Classic switch in the advanced group turns on Classic interaction.
#[gpui_kit::test]
fn the_classic_switch_turns_on_classic_interaction(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Crop, 1.0);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_interaction,
            crate::crop_geometry::CropInteraction::Modern,
            "the crop session starts in Modern interaction"
        );
        window.click("opt-more", cx);
        window.render_frame(cx);
        window.click("opt-crop-classic", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_interaction,
            crate::crop_geometry::CropInteraction::Classic,
            "the switch turns on Classic interaction"
        );
    })
    .expect("test window remains open");
}

/// Modern interaction keeps the frame and moves the picture behind it.
#[gpui_kit::test]
fn a_modern_drag_moves_the_picture_and_keeps_the_frame(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some([100.0, 100.0, 100.0, 100.0]);
            });
        });
        window.render_frame(cx);
        let (_, placement) = page(window, (400, 300));
        let zoom = f64::from(placement.2 / 400.0);
        drag_doc(window, (400, 300), (150.0, 150.0), (170.0, 160.0), cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.crop_rect,
            Some([100.0, 100.0, 100.0, 100.0]),
            "Modern interaction keeps the frame where it is"
        );
        assert!(
            (state.crop_offset.0 - 20.0 * zoom).abs() < 0.5
                && (state.crop_offset.1 - 10.0 * zoom).abs() < 0.5,
            "the drag moves the picture behind the frame: {:?}",
            state.crop_offset
        );
    })
    .expect("test window remains open");
}

#[test]
fn modern_crop_move_converts_document_delta_to_screen_offset() {
    let frame = [10.0, 20.0, 100.0, 50.0];
    let drag = crate::app::crop::CropDrag {
        handle: None,
        start_rect: frame,
        start_doc: (20.0, 30.0),
        start_snapshot: crate::crop_geometry::CropSnapshot {
            frame,
            offset: (7.0, 11.0),
            rotation: 0.0,
        },
    };

    let update = crate::app::crop::crop_drag_apply(
        &drag,
        (30.0, 25.0),
        crate::crop_geometry::CropInteraction::Modern,
        None,
        2.0,
        false,
        400.0,
        300.0,
    );

    assert_eq!(update.offset, (27.0, 1.0));
}

#[test]
fn modern_crop_resize_scales_picture_compensation_by_zoom() {
    let frame = [10.0, 20.0, 100.0, 50.0];
    let drag = crate::app::crop::CropDrag {
        handle: Some(crate::app::crop::CropHandle::E),
        start_rect: frame,
        start_doc: (110.0, 45.0),
        start_snapshot: crate::crop_geometry::CropSnapshot {
            frame,
            offset: (4.0, 5.0),
            rotation: 0.0,
        },
    };

    let update = crate::app::crop::crop_drag_apply(
        &drag,
        (120.0, 45.0),
        crate::crop_geometry::CropInteraction::Modern,
        None,
        2.0,
        false,
        400.0,
        300.0,
    );

    assert_eq!(update.frame, [0.0, 20.0, 120.0, 50.0]);
    assert_eq!(update.offset, (-16.0, 5.0));
}

#[gpui_kit::test]
fn modern_crop_pointer_drags_keep_screen_offsets_across_zoom_levels(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some([100.0, 100.0, 100.0, 100.0]);
                state.crop_interaction = crate::crop_geometry::CropInteraction::Modern;
            });
        });
        window.render_frame(cx);
        let bounds = window.find("canvas-hitbox").bounds();

        for zoom in [0.5_f32, 1.0, 1.25, 2.0] {
            let camera = crate::display::canvas_bridge::Camera::centred_at(
                bounds.size.width.into(),
                bounds.size.height.into(),
                400.0,
                300.0,
                zoom,
            )
            .expect("the stated zoom fits in the test camera");
            content.update(cx, |app, cx| {
                app.set_camera(camera, cx);
                app.state_for_test().update(cx, |state, _| {
                    state.crop_rect = Some([100.0, 100.0, 100.0, 100.0]);
                    state.crop_offset = (0.0, 0.0);
                });
            });
            window.render_frame(cx);

            let screen_point = |x: f32, y: f32| {
                bounds.origin
                    + gpui_kit::point(
                        gpui_kit::px(camera.x + x * zoom),
                        gpui_kit::px(camera.y + y * zoom),
                    )
            };
            window.drag(
                screen_point(150.0, 150.0),
                screen_point(150.0, 150.0)
                    + gpui_kit::point(gpui_kit::px(20.0), gpui_kit::px(-10.0)),
                cx,
            );
            window.render_frame(cx);
            let moved = content.read(cx).state_for_test().read(cx).crop_offset;
            assert!(
                (moved.0 - 20.0).abs() < 0.5 && (moved.1 + 10.0).abs() < 0.5,
                "zoom={zoom}, moved={moved:?}"
            );

            content.update(cx, |app, cx| {
                app.state_for_test().update(cx, |state, _| {
                    state.crop_rect = Some([100.0, 100.0, 100.0, 100.0]);
                    state.crop_offset = (0.0, 0.0);
                });
            });
            window.render_frame(cx);
            window.drag(
                screen_point(200.0, 150.0),
                screen_point(200.0, 150.0) + gpui_kit::point(gpui_kit::px(20.0), gpui_kit::px(0.0)),
                cx,
            );
            window.render_frame(cx);
            let resized = content.read(cx).state_for_test().read(cx).crop_offset;
            assert!(
                (resized.0 + 20.0).abs() < 0.5 && resized.1.abs() < 0.5,
                "zoom={zoom}, resized={resized:?}"
            );
        }
    })
    .expect("test window remains open");
}

/// Classic interaction moves the frame over a still picture.
#[gpui_kit::test]
fn a_classic_drag_moves_the_frame(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_interaction = crate::crop_geometry::CropInteraction::Classic;
                state.crop_rect = Some([100.0, 100.0, 100.0, 100.0]);
            });
        });
        window.render_frame(cx);
        drag_doc(window, (400, 300), (150.0, 150.0), (170.0, 160.0), cx);
        let state = content.read(cx).state_for_test().read(cx);
        let frame = state.crop_rect.expect("the frame survives");
        assert!(
            (frame[0] - 120.0).abs() < 0.5 && (frame[1] - 110.0).abs() < 0.5,
            "Classic interaction moves the frame with the pointer: {frame:?}"
        );
        assert_eq!(
            (frame[2], frame[3]),
            (100.0, 100.0),
            "the size is unchanged"
        );
        assert_eq!(state.crop_offset, (0.0, 0.0), "the picture stays put");
    })
    .expect("test window remains open");
}

/// A Modern handle drag grows the frame about its centre and moves the
/// picture so the content under the opposite edge keeps its place.
#[gpui_kit::test]
fn a_modern_handle_drag_resizes_about_the_centre(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some([100.0, 100.0, 100.0, 100.0]);
            });
        });
        window.render_frame(cx);
        let (_, placement) = page(window, (400, 300));
        let zoom = f64::from(placement.2 / 400.0);
        // The east handle sits on the frame's right edge.
        drag_doc(window, (400, 300), (200.0, 150.0), (220.0, 150.0), cx);
        let state = content.read(cx).state_for_test().read(cx);
        let frame = state.crop_rect.expect("the frame survives");
        assert!(
            (frame[2] - 140.0).abs() < 2.0 && (frame[0] - 80.0).abs() < 2.0,
            "the frame grows about its centre: {frame:?}"
        );
        assert!(
            (state.crop_offset.0 + 20.0 * zoom).abs() < 2.0,
            "the picture compensates against the pointer: {:?}",
            state.crop_offset
        );
    })
    .expect("test window remains open");
}

/// The mini undo restores the whole session: the frame, the picture offset
/// and the angle.
#[gpui_kit::test]
fn the_crop_mini_undo_restores_the_whole_session(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some([100.0, 100.0, 100.0, 100.0]);
            });
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("right", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).crop_offset,
            (1.0, 0.0),
            "the arrow key moves the picture in Modern interaction"
        );
        window.press("ctrl-z", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(state.crop_offset, (0.0, 0.0), "undo restores the offset");
        assert_eq!(
            state.crop_rect,
            Some([100.0, 100.0, 100.0, 100.0]),
            "undo restores the frame"
        );
    })
    .expect("test window remains open");
}
