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
