use super::canvas_input::{
    drag_doc, history_cursor, page, tab_to_canvas, unlock_background_transform,
};
use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;

fn new_doc(content: &Entity<App>, cx: &mut gpui_kit::App, width: u32, height: u32) {
    content.update(cx, |app, cx| {
        app.create_document(width, height, cx)
            .expect("blank document");
        app.state_for_test()
            .update(cx, |state, _| state.active_tool = ToolId::Move);
    });
    // A blank document starts on the pinned Background; transform tests need
    // a movable, document-sized layer.
    unlock_background_transform(content, cx);
}

fn transform(content: &Entity<App>, cx: &mut gpui_kit::App) -> (f64, f64, f64, f64, f64) {
    content
        .read(cx)
        .selected_transform_for_test(cx)
        .expect("selected layer")
}

#[gpui_kit::test]
fn dragging_the_se_corner_scales_the_layer_proportionally(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 400, 300);
        window.render_frame(cx);
        let before = transform(&content, cx);
        drag_doc(window, (400, 300), (400.0, 300.0), (440.0, 330.0), cx);
        let after = transform(&content, cx);
        assert!(
            after.2 > before.2 + 0.05 && after.3 > before.3 + 0.05,
            "resize must grow both axes: {before:?} -> {after:?}"
        );
        assert!(
            (after.2 - after.3).abs() < 1e-3,
            "the default aspect lock keeps the ratio: {after:?}"
        );
        assert_eq!(after.4, before.4, "resize must not rotate");
    })
    .expect("test window remains open");
}

/// A corner drag paints, in the same render, a preview that covers the frame
/// the user sees, and never changes the canvas size. A queued or stale
/// preview would show the layer at another size for a frame, which reads as
/// the layer jumping bigger and smaller while the pointer moves.
#[gpui_kit::test]
fn corner_drag_preview_matches_the_frame_it_is_drawn_with(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        // Wider than the viewport, so the preview runs at display scale.
        new_doc(&content, cx, 1600, 1200);
        window.render_frame(cx);
        let (bounds, placement) = page(window, (1600, 1200));
        let corner = (1600.0, 1200.0);
        let start =
            super::canvas_input::window_point(bounds, placement, (1600, 1200), corner.0, corner.1);
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        // The reported gesture: a straight NE/SW run through the SE corner.
        let mut last = start;
        for step in 0..5 {
            let d = match step {
                0 => 0.0,
                1 => 30.0,
                2 => 60.0,
                3 => 30.0,
                _ => 0.0,
            };
            let position = super::canvas_input::window_point(
                bounds,
                placement,
                (1600, 1200),
                corner.0 + d * 0.5,
                corner.1 - d * 0.5,
            );
            last = position;
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                    position,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            let app = content.read(cx);
            let active = app.active_document.as_ref().expect("active document");
            assert_eq!(
                active.canvas.dimensions(),
                (1600, 1200),
                "a live preview must never resize the canvas"
            );
            let patch = active
                .canvas
                .preview_patch_rects()
                .into_iter()
                .next()
                .expect("the rendered frame carries the preview pixels");
            let drag = app.transform_drag_for_test().expect("live drag");
            let (fx, fy, fw, fh) =
                super::transform_geometry::layer_aabb(&drag.preview, drag.width, drag.height);
            // The patch covers the frame, clipped to the document.
            let left = (fx.floor() - 1.0).max(0.0);
            let top = (fy.floor() - 1.0).max(0.0);
            let right = (fx + fw).ceil().min(1600.0);
            let bottom = (fy + fh).ceil().min(1200.0);
            assert!(
                f64::from(patch.0) <= left
                    && f64::from(patch.1) <= top
                    && f64::from(patch.0 + patch.2) >= right - 1.0
                    && f64::from(patch.1 + patch.3) >= bottom - 1.0,
                "step {step}: preview {patch:?} must cover the frame ({fx}, {fy}, {fw}, {fh})"
            );
        }
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: last,
                ..Default::default()
            }),
            cx,
        );
    })
    .expect("test window remains open");
}

/// The Move rotate hover paints the dynamic rotate sprite at the pointer,
/// and Space wins over every handle cursor with the grab cursor.
#[gpui_kit::test]
fn move_rotate_hover_paints_the_sprite_and_space_wins(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 400, 300);
        content.update(cx, |app, cx| {
            // Half-size layer so the rotate zone sits inside the page.
            app.set_transform_field("num-w", 200.0, cx)
                .expect("width edit applies");
            app.set_transform_field("num-h", 150.0, cx)
                .expect("height edit applies");
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 300));
        let outside = super::canvas_input::window_point(bounds, placement, (400, 300), 300.0, 75.0);
        let corner = super::canvas_input::window_point(bounds, placement, (400, 300), 200.0, 150.0);
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: outside,
                ..Default::default()
            }),
            cx,
        );
        let sprite = content
            .update(cx, |app, cx| app.rotate_cursor_for_test(cx))
            .expect("the rotate hover paints the sprite");
        assert_eq!(
            (sprite.position[0], sprite.position[1]),
            (f32::from(outside.x), f32::from(outside.y)),
            "the sprite anchors at the pointer"
        );
        assert_eq!(
            content.update(cx, |app, cx| app.canvas_cursor_policy(cx)),
            crate::input_policy::CursorPolicy::Crosshair,
            "the rotate hover keeps the crosshair under the sprite"
        );

        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: corner,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.update(cx, |app, cx| app.canvas_cursor_policy(cx)),
            crate::input_policy::CursorPolicy::ResizeNwse,
            "the SE corner hovers as a resize cursor before Space"
        );
        super::canvas_input::tab_to_canvas(window, cx);
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
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
        assert_eq!(
            content.update(cx, |app, cx| app.canvas_cursor_policy(cx)),
            crate::input_policy::CursorPolicy::OpenHand,
            "Space wins over the handle cursor with the grab cursor"
        );
        assert!(
            content
                .update(cx, |app, cx| app.rotate_cursor_for_test(cx))
                .is_none(),
            "Space paints no rotate sprite"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn dragging_outside_a_corner_rotates_the_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 400, 300);
        window.render_frame(cx);
        let before = transform(&content, cx);
        assert_eq!(before.4, 0.0);
        drag_doc(window, (400, 300), (200.0, -30.0), (350.0, 150.0), cx);
        let after = transform(&content, cx);
        assert!(
            (after.4 - 90.0).abs() < 1e-3,
            "the rotation drag must land at 90 degrees: {after:?}"
        );
        assert!((after.2 - before.2).abs() < 1e-6);
        assert!((after.3 - before.3).abs() < 1e-6);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn transform_drag_previews_before_commit_and_undo_restores(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    let mut before = (0.0, 0.0, 0.0, 0.0, 0.0);
    let mut preview = photrez_core::document::content::Transform2D {
        x: 0.0,
        y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
    };
    let mut moved = gpui_kit::Point::default();
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 200, 150);
        window.render_frame(cx);
        before = transform(&content, cx);
        let (bounds, placement) = page(window, (200, 150));
        let start = super::canvas_input::window_point(bounds, placement, (200, 150), 60.0, 60.0);
        moved = super::canvas_input::window_point(bounds, placement, (200, 150), 100.0, 90.0);
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        preview = content
            .read(cx)
            .transform_drag_for_test()
            .expect("drag is live")
            .preview
            .clone();
        assert!(
            (preview.x - (before.0 + 40.0)).abs() < 1e-3
                && (preview.y - (before.1 + 30.0)).abs() < 1e-3,
            "preview follows the pointer: {preview:?}"
        );
    })
    .expect("test window remains open");
    // The pixel composite runs in the background; park until it installs.
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        let (has_live_preview, expected) = {
            let app = content.read(cx);
            let active = app.active_document.as_ref().expect("active document");
            let live_preview = matches!(
                active.canvas.read_region_rgba(0, 0, 200, 150),
                Err(crate::display::canvas_bridge::FrameError::PendingPreviews)
            );
            let loaded = app
                .sessions
                .get(&active.doc_id)
                .expect("active session")
                .loaded_document();
            let mut expected_content = loaded
                .content_snapshot()
                .expect("canonical document content")
                .clone();
            let drag = app.transform_drag_for_test().expect("live drag");
            let layer = expected_content
                .layers
                .iter_mut()
                .find(|layer| layer.id == drag.layer_id)
                .expect("dragged layer");
            assert!(matches!(
                &layer.layer_type,
                photrez_core::document::content::LayerType::Raster
            ));
            layer.transform = preview.clone();
            let expected = photrez_core::render::compositor::composite_layers_with_font_data(
                &expected_content,
                &loaded.registry,
                Some(crate::INTER),
            )
            .expect("composite canonical preview");
            (live_preview, expected.pixels)
        };
        assert!(has_live_preview, "the drag uses a live display overlay");
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        let committed = transform(&content, cx);
        assert!((committed.0 - preview.x).abs() < 1e-6);
        assert!((committed.1 - preview.y).abs() < 1e-6);
        let actual = content
            .read(cx)
            .active_document
            .as_ref()
            .expect("active document")
            .canvas
            .read_region_rgba(0, 0, 200, 150)
            .expect("read committed canvas pixels");
        assert_eq!(actual, expected, "commit preserves the displayed preview");
        content.update(cx, |app, cx| {
            app.step_history(true, cx).expect("undo");
        });
        let undone = transform(&content, cx);
        assert!((undone.0 - before.0).abs() < 1e-9);
        assert!((undone.1 - before.1).abs() < 1e-9);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn alt_drag_duplicates_the_layer_and_moves_the_copy(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 200, 150);
        window.render_frame(cx);
        assert_eq!(content.read(cx).active_core_layer_count_for_test(), Some(1));
        let (bounds, placement) = page(window, (200, 150));
        let start = super::canvas_input::window_point(bounds, placement, (200, 150), 60.0, 60.0);
        let moved = super::canvas_input::window_point(bounds, placement, (200, 150), 100.0, 90.0);
        let alt = gpui_kit::Modifiers {
            alt: true,
            ..Default::default()
        };
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: start,
                modifiers: alt,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: moved,
                modifiers: alt,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: moved,
                modifiers: alt,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content.read(cx).active_core_layer_count_for_test(),
            Some(2),
            "Alt+drag leaves the original and adds a copy"
        );
        let ids = content.read(cx).active_core_layer_ids_for_test();
        let copy_id = ids.first().expect("copy is first");
        let original_id = ids.get(1).expect("original is below");
        let copy = content
            .read(cx)
            .core_layer_pixels_for_test(copy_id)
            .expect("copy pixels");
        let original = content
            .read(cx)
            .core_layer_pixels_for_test(original_id)
            .expect("original pixels");
        assert_eq!(copy.2, original.2, "the copy inherits the pixels");
        let copy_transform = content.update(cx, |app, cx| {
            app.selected_transform_for_test(cx).expect("copy selected")
        });
        assert!((copy_transform.0 - 40.0).abs() < 1e-3);
        assert!((copy_transform.1 - 30.0).abs() < 1e-3);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn arrow_keys_nudge_the_selected_layer_in_move_mode(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 200, 150);
        window.render_frame(cx);
        super::canvas_input::tab_to_canvas(window, cx);
        let before = transform(&content, cx);
        window.press("right", cx);
        let after = transform(&content, cx);
        assert!((after.0 - (before.0 + 1.0)).abs() < 1e-6);
        window.press("shift-down", cx);
        let after_shift = transform(&content, cx);
        assert!((after_shift.1 - (after.1 + 10.0)).abs() < 1e-6);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn move_snaps_to_the_canvas_edge_and_shows_a_guide(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 200, 150);
        window.render_frame(cx);
        let (bounds, placement) = page(window, (200, 150));
        let start = super::canvas_input::window_point(bounds, placement, (200, 150), 60.0, 60.0);
        let moved = super::canvas_input::window_point(bounds, placement, (200, 150), 63.0, 60.0);
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        let drag = content.read(cx).transform_drag_for_test().expect("drag");
        assert!(
            drag.preview.x.abs() < 0.01,
            "the move snaps back to the canvas left edge: {}",
            drag.preview.x
        );
        assert!(
            content
                .read(cx)
                .snap_guides_for_test()
                .iter()
                .any(|(vertical, position)| *vertical && position.abs() < 1e-6),
            "a vertical guide is published at x=0"
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn multi_selection_drag_moves_every_selected_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 400, 300);
        content.update(cx, |app, cx| {
            app.state_for_test()
                .update(cx, |state, _| state.active_tool = ToolId::Shape);
        });
        window.render_frame(cx);
        drag_doc(window, (400, 300), (50.0, 50.0), (250.0, 200.0), cx);
        content.update(cx, |app, cx| {
            app.select_all_layers(cx);
            app.state_for_test()
                .update(cx, |state, _| state.active_tool = ToolId::Move);
        });
        window.render_frame(cx);
        let before: Vec<(String, f64, f64)> = content.update(cx, |app, _| {
            let doc_id = app.active_doc_id().unwrap();
            let content = app
                .sessions
                .get(&doc_id)
                .unwrap()
                .loaded_document()
                .content_snapshot()
                .unwrap()
                .clone();
            content
                .layers
                .iter()
                .map(|layer| (layer.id.clone(), layer.transform.x, layer.transform.y))
                .collect()
        });
        drag_doc(window, (400, 300), (200.0, 150.0), (240.0, 180.0), cx);
        content.update(cx, |app, _| {
            let doc_id = app.active_doc_id().unwrap();
            let content = app
                .sessions
                .get(&doc_id)
                .unwrap()
                .loaded_document()
                .content_snapshot()
                .unwrap()
                .clone();
            assert_eq!(content.layers.len(), 2);
            for (id, x, y) in before {
                let layer = content.layers.iter().find(|layer| layer.id == id).unwrap();
                assert!(
                    (layer.transform.x - (x + 40.0)).abs() < 1e-3
                        && (layer.transform.y - (y + 30.0)).abs() < 1e-3,
                    "{id} moved to {:?} from {:?}",
                    (layer.transform.x, layer.transform.y),
                    (x, y)
                );
            }
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn transform_drag_publishes_and_clears_the_hud(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 400, 300);
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 300));
        let start = super::canvas_input::window_point(bounds, placement, (400, 300), 400.0, 300.0);
        let moved = super::canvas_input::window_point(bounds, placement, (400, 300), 440.0, 320.0);
        let modifiers = gpui_kit::Modifiers::default();

        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: start,
                modifiers,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: moved,
                pressed_button: Some(gpui_kit::MouseButton::Left),
                modifiers,
            }),
            cx,
        );
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            let hud = state.hud.expect("resize HUD");
            assert_eq!(hud.mode, crate::state::HudMode::Resize);
            assert!(hud.a > 400.0 && hud.b > 300.0, "W/H follow the preview");
        }
        assert!(window.try_find("transform-hud").is_some());

        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: moved,
                modifiers,
                click_count: 1,
            }),
            cx,
        );
        window.render_frame(cx);
        assert!(
            content.read(cx).state_for_test().read(cx).hud.is_none(),
            "release clears the HUD"
        );
        assert!(window.try_find("transform-hud").is_none());
    })
    .expect("test window remains open");
}

fn draw_two_shapes(
    window: &mut gpui_kit::Window,
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
) -> (u64, u64) {
    content.update(cx, |app, cx| {
        app.create_document(200, 150, cx).expect("blank document");
    });
    super::canvas_input::set_tool(content, cx, ToolId::Shape, 1.0);
    window.render_frame(cx);
    drag_doc(window, (200, 150), (10.0, 10.0), (80.0, 80.0), cx);
    // The second press starts outside the selected shape's adaptive rotate
    // ring (31 px for a 70 px frame), so the Shape tool draws again instead
    // of starting a rotation.
    drag_doc(window, (200, 150), (120.0, 10.0), (190.0, 80.0), cx);
    super::canvas_input::set_tool(content, cx, ToolId::Move, 1.0);
    window.render_frame(cx);
    // Newest-first projection: the second drag is row 0, the first is row 1.
    let row = |index: usize| {
        content
            .read(cx)
            .state_for_test()
            .read(cx)
            .layers
            .iter()
            .filter(|row| row.name == "Shape")
            .nth(index)
            .map(|row| row.id)
            .unwrap_or_else(|| panic!("shape row {index}"))
    };
    (row(1), row(0))
}

fn selected_rows(content: &Entity<App>, cx: &mut gpui_kit::App) -> Vec<u64> {
    content
        .read(cx)
        .state_for_test()
        .read(cx)
        .selected_layers
        .clone()
}

fn select_rows(content: &Entity<App>, cx: &mut gpui_kit::App, rows: Vec<u64>) {
    let primary = rows.last().copied().unwrap_or(0);
    content.update(cx, |app, cx| {
        app.state_for_test().update(cx, |state, _| {
            state.selected_layer = primary;
            state.selected_layers = rows;
        });
    });
}

#[gpui_kit::test]
fn move_drag_on_empty_canvas_marquees_the_overlapping_layers(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let (a, b) = draw_two_shapes(window, &content, cx);
        drag_doc(window, (200, 150), (5.0, 5.0), (85.0, 85.0), cx);
        assert_eq!(
            selected_rows(&content, cx),
            vec![a],
            "the marquee replaces the selection with the layers it overlaps"
        );
        assert!(
            !selected_rows(&content, cx).contains(&b),
            "the right-hand box is outside the marquee"
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selected_layer,
            b,
            "the marquee does not move the primary row"
        );
        assert!(
            content.read(cx).move_marquee_for_test().is_none(),
            "release ends the session"
        );
        drag_doc(window, (200, 150), (5.0, 5.0), (190.0, 90.0), cx);
        let selected = selected_rows(&content, cx);
        assert_eq!(selected.len(), 2, "a wider marquee covers both boxes");
        assert!(selected.contains(&a) && selected.contains(&b));
        assert!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .multi_bounds
                .is_some(),
            "a multi selection projects the union bounds"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn move_empty_click_clears_the_layer_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        draw_two_shapes(window, &content, cx);
        let (bounds, placement) = page(window, (200, 150));
        let empty = super::canvas_input::window_point(bounds, placement, (200, 150), 5.0, 140.0);
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: empty,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: empty,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert!(
            selected_rows(&content, cx).is_empty(),
            "a sub-threshold, non-additive press deselects every row"
        );
        let state = content.read(cx).state_for_test();
        assert_eq!(
            state.read(cx).selected_layer,
            0,
            "native has no null primary"
        );
        assert!(state.read(cx).multi_bounds.is_none());
    })
    .expect("test window remains open");
}

/// A left press and release on the empty canvas page clears the
/// selection and repaints. The measured span is the dispatch pair plus one
/// frame; reseeding the selection between samples is host-side.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn canvas_click_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let (a, b) = draw_two_shapes(window, &content, cx);
        let (bounds, placement) = page(window, (200, 150));
        let empty = super::canvas_input::window_point(bounds, placement, (200, 150), 5.0, 140.0);
        let press = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: empty,
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: empty,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        };
        select_rows(&content, cx, vec![a, b]);
        window.render_frame(cx);
        press(window, cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            select_rows(&content, cx, vec![a, b]);
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
            "canvas_click p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "canvas click p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shift_drag_marquee_adds_layers_to_the_existing_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let (a, b) = draw_two_shapes(window, &content, cx);
        // The primary sits far from the press so the transform rotate band
        // (ROTATE_THRESHOLD_PX) does not claim the empty-canvas press first.
        select_rows(&content, cx, vec![b]);
        window.render_frame(cx);
        let (bounds, placement) = page(window, (200, 150));
        let start = super::canvas_input::window_point(bounds, placement, (200, 150), 5.0, 5.0);
        let moved = super::canvas_input::window_point(bounds, placement, (200, 150), 185.0, 85.0);
        let modifiers = gpui_kit::Modifiers {
            shift: true,
            ..Default::default()
        };
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: start,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: moved,
                pressed_button: Some(gpui_kit::MouseButton::Left),
                modifiers,
            }),
            cx,
        );
        let selected = selected_rows(&content, cx);
        assert_eq!(selected.len(), 2, "an additive marquee unions both ids");
        assert_eq!(
            selected[0], b,
            "an additive marquee keeps the initial ids ahead of the hits"
        );
        assert!(selected.contains(&a));
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: moved,
                modifiers,
                click_count: 1,
            }),
            cx,
        );
        assert_eq!(
            selected_rows(&content, cx),
            selected,
            "releasing an additive session keeps the selection"
        );
        let empty = super::canvas_input::window_point(bounds, placement, (200, 150), 5.0, 140.0);
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: empty,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: empty,
                modifiers,
                click_count: 1,
            }),
            cx,
        );
        assert_eq!(
            selected_rows(&content, cx),
            selected,
            "an additive press never clears the selection"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn cancelling_a_move_marquee_restores_the_initial_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let (a, b) = draw_two_shapes(window, &content, cx);
        // Same rotate-band guard as the additive test: the primary is the
        // far box so the press starts the marquee, not a handle drag.
        select_rows(&content, cx, vec![b]);
        window.render_frame(cx);
        let (bounds, placement) = page(window, (200, 150));
        let start = super::canvas_input::window_point(bounds, placement, (200, 150), 5.0, 5.0);
        let moved = super::canvas_input::window_point(bounds, placement, (200, 150), 190.0, 90.0);
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: moved,
                pressed_button: Some(gpui_kit::MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        let selected = selected_rows(&content, cx);
        assert!(
            selected.len() == 2 && selected.contains(&a) && selected.contains(&b),
            "the drag selects both boxes before cancel: {selected:?}"
        );
        let rect = content
            .read(cx)
            .move_marquee_for_test()
            .expect("the live marquee paints past the drag threshold");
        assert!(
            (rect.0 - 5.0).abs() < 0.5
                && (rect.1 - 5.0).abs() < 0.5
                && (rect.2 - 185.0).abs() < 0.5
                && (rect.3 - 85.0).abs() < 0.5,
            "the overlay rect is the document-space drag box: {rect:?}"
        );
        content.update(cx, |app, cx| app.cancel_canvas_gesture(cx));
        assert_eq!(
            selected_rows(&content, cx),
            vec![b],
            "cancel restores the pre-drag selection"
        );
        assert!(content.read(cx).move_marquee_for_test().is_none());
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: moved,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            selected_rows(&content, cx),
            vec![b],
            "the cancelled capture's release must not clear the selection"
        );
    })
    .expect("test window remains open");
}

/// Perf: one Move drag's release, i.e. the transform commit plus the frame
/// that repaints the moved layer. The press and the preview frames are warm-up;
/// the measured event is release -> visible result.
#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn transform_commit_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let (a, _b) = draw_two_shapes(window, &content, cx);
        select_rows(&content, cx, vec![a]);
        window.render_frame(cx);
        let (bounds, placement) = page(window, (200, 150));
        let mut samples = Vec::new();
        for _ in 0..9 {
            let (x, y, _, _, _) = transform(&content, cx);
            let center = (x + 35.0, y + 35.0);
            let from = super::canvas_input::window_point(
                bounds,
                placement,
                (200, 150),
                center.0 as f32,
                center.1 as f32,
            );
            let to = super::canvas_input::window_point(
                bounds,
                placement,
                (200, 150),
                (center.0 + 6.0) as f32,
                (center.1 + 4.0) as f32,
            );
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: from,
                    ..Default::default()
                }),
                cx,
            );
            let start = std::time::Instant::now();
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
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "transform_commit p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "transform commit p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

fn selected_core_id(content: &Entity<App>, cx: &gpui_kit::App) -> String {
    content
        .read(cx)
        .selected_layer_for_cursor(cx)
        .expect("the fixture selects a layer")
        .1
        .id
}

/// Repeating a transform field at the value it already holds must not add a
/// history row.
#[gpui_kit::test]
fn an_unchanged_transform_field_adds_no_history_entry(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, _window, cx| {
        new_doc(&content, cx, 400, 300);
        // The new document starts on a position-locked Background, which
        // refuses a position edit before the ghost path is reached.
        content.update(cx, |app, cx| app.add_empty_layer(cx));
        let cursor_before = history_cursor(&content, cx);
        let x = transform(&content, cx).0;
        content.update(cx, |app, cx| {
            app.set_transform_field("num-x", x, cx)
                .expect("the field commits");
        });
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "a field at its current value must not add a history entry"
        );
    })
    .expect("test window remains open");
}

/// Reset on an identity transform changes nothing, so it must not add a
/// history row.
#[gpui_kit::test]
fn resetting_an_identity_transform_adds_no_history_entry(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, _window, cx| {
        new_doc(&content, cx, 400, 300);
        let cursor_before = history_cursor(&content, cx);
        content.update(cx, |app, cx| {
            app.reset_selected_transform(cx).expect("reset commits");
        });
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "a reset with nothing to reset must not add a history entry"
        );
    })
    .expect("test window remains open");
}

/// A locked layer must not move from a canvas drag, including the background
/// position lock. The Move bar fields already refuse a locked layer.
#[gpui_kit::test]
fn a_locked_layer_cannot_be_moved_from_the_canvas(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 400, 300);
        let core_id = selected_core_id(&content, cx);
        content.update(cx, |app, cx| app.toggle_layer_locked(core_id, cx));
        window.render_frame(cx);
        let before = transform(&content, cx);
        let cursor_before = history_cursor(&content, cx);
        drag_doc(window, (400, 300), (200.0, 150.0), (240.0, 180.0), cx);
        assert_eq!(
            transform(&content, cx),
            before,
            "a locked layer must not move from a canvas drag"
        );
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "a refused drag must not add a history entry"
        );
    })
    .expect("test window remains open");
}

/// The background layer carries a position lock. A canvas drag must not move
/// it, or the Background invariant that the stacking order relies on breaks.
#[gpui_kit::test]
fn a_position_locked_layer_cannot_be_moved_from_the_canvas(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 300, cx).expect("blank document");
            app.state_for_test()
                .update(cx, |state, _| state.active_tool = ToolId::Move);
        });
        assert!(
            content
                .read(cx)
                .selected_layer_for_cursor(cx)
                .expect("the blank document selects its Background")
                .1
                .lock_position
                .unwrap_or(false),
            "the blank document's Background carries the position lock"
        );
        window.render_frame(cx);
        let before = transform(&content, cx);
        let cursor_before = history_cursor(&content, cx);
        drag_doc(window, (400, 300), (200.0, 150.0), (240.0, 180.0), cx);
        assert_eq!(
            transform(&content, cx),
            before,
            "a position-locked layer must not move from a canvas drag"
        );
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "a refused drag must not add a history entry"
        );
    })
    .expect("test window remains open");
}

/// The arrow-key nudge reads the same transform as the Move bar, so a locked
/// layer must ignore it too.
#[gpui_kit::test]
fn arrow_keys_do_not_move_a_locked_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 400, 300);
        let core_id = selected_core_id(&content, cx);
        content.update(cx, |app, cx| app.toggle_layer_locked(core_id, cx));
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let before = transform(&content, cx);
        let cursor_before = history_cursor(&content, cx);
        window.press("right", cx);
        window.render_frame(cx);
        assert_eq!(
            transform(&content, cx),
            before,
            "arrow keys must not move a locked layer"
        );
        assert_eq!(
            history_cursor(&content, cx),
            cursor_before,
            "a refused nudge must not add a history entry"
        );
    })
    .expect("test window remains open");
}

/// During a resize drag the cursor must keep describing the handle even after
/// the pointer leaves the canvas. The reference showed resize/rotate cursors
/// outside the viewport while the drag was live.
#[gpui_kit::test]
fn a_resize_drag_keeps_its_cursor_outside_the_canvas(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 400, 300);
        window.render_frame(cx);
        let (bounds, placement) = page(window, (400, 300));
        let start = super::canvas_input::window_point(bounds, placement, (400, 300), 400.0, 300.0);
        let outside = bounds.origin
            + gpui_kit::point(
                gpui_kit::px(f32::from(bounds.size.width) + 200.0),
                gpui_kit::px(f32::from(bounds.size.height) + 200.0),
            );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent {
                position: outside,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            content.update(cx, |app, cx| app.canvas_cursor_policy(cx)),
            crate::input_policy::CursorPolicy::ResizeNwse,
            "the resize cursor survives outside the canvas during the drag"
        );
    })
    .expect("test window remains open");
}
