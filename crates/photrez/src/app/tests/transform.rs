use super::input::{drag_doc, page};
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
    cx.update_window(window.into(), |_, window, cx| {
        new_doc(&content, cx, 200, 150);
        window.render_frame(cx);
        let before = transform(&content, cx);
        let (bounds, placement) = page(window, (200, 150));
        let start = super::input::window_point(bounds, placement, (200, 150), 60.0, 60.0);
        let moved = super::input::window_point(bounds, placement, (200, 150), 100.0, 90.0);
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
        let preview = content
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
        let (has_live_preview, expected) = {
            let app = content.read(cx);
            let active = app.active_document.as_ref().expect("active document");
            let live_preview = matches!(
                active.canvas.read_region_rgba(0, 0, 200, 150),
                Err(crate::render::canvas_bridge::FrameError::PendingPreviews)
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
                photrez_core::ptz::content::LayerType::Raster
            ));
            layer.transform = drag.preview.clone();
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
        let start = super::input::window_point(bounds, placement, (200, 150), 60.0, 60.0);
        let moved = super::input::window_point(bounds, placement, (200, 150), 100.0, 90.0);
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
        super::input::tab_to_canvas(window, cx);
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
        let start = super::input::window_point(bounds, placement, (200, 150), 60.0, 60.0);
        let moved = super::input::window_point(bounds, placement, (200, 150), 63.0, 60.0);
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
        let start = super::input::window_point(bounds, placement, (400, 300), 400.0, 300.0);
        let moved = super::input::window_point(bounds, placement, (400, 300), 440.0, 320.0);
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
    super::input::set_tool(content, cx, ToolId::Shape, 1.0);
    window.render_frame(cx);
    drag_doc(window, (200, 150), (10.0, 10.0), (80.0, 80.0), cx);
    // The second press starts outside the selected shape's adaptive rotate
    // ring (31 px for a 70 px frame), so the Shape tool draws again instead
    // of starting a rotation.
    drag_doc(window, (200, 150), (120.0, 10.0), (190.0, 80.0), cx);
    super::input::set_tool(content, cx, ToolId::Move, 1.0);
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
        let empty = super::input::window_point(bounds, placement, (200, 150), 5.0, 140.0);
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
        let empty = super::input::window_point(bounds, placement, (200, 150), 5.0, 140.0);
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
        let start = super::input::window_point(bounds, placement, (200, 150), 5.0, 5.0);
        let moved = super::input::window_point(bounds, placement, (200, 150), 185.0, 85.0);
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
        let empty = super::input::window_point(bounds, placement, (200, 150), 5.0, 140.0);
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
        let start = super::input::window_point(bounds, placement, (200, 150), 5.0, 5.0);
        let moved = super::input::window_point(bounds, placement, (200, 150), 190.0, 90.0);
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
            let from = super::input::window_point(
                bounds,
                placement,
                (200, 150),
                center.0 as f32,
                center.1 as f32,
            );
            let to = super::input::window_point(
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
