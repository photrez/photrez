//! Escape and the previews it owns: pending tools, move and group previews, selection
//! reprojection, the copy cursor, the crop resize cursors and popover dismissal.

use super::*;

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
            let layer_selection = content
                .read(cx)
                .state_for_test()
                .read(cx)
                .selected_layers
                .clone();
            if tool == ToolId::Selection {
                content.update(cx, |app, _| app.selection_edit_mode = true);
            }
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
            assert_eq!(
                content.read(cx).state_for_test().read(cx).selected_layers,
                layer_selection,
                "{tool:?} Escape must preserve layer selection"
            );
            if tool == ToolId::Selection {
                content.update(cx, |app, _| {
                    assert!(!app.selection_edit_mode);
                });
            }

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
fn idle_move_escape_clears_single_and_multi_layer_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 2.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                let primary = state.selected_layer;
                state.selected_layers = vec![primary];
                state.multi_bounds = None;
            });
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);

        let before_pixels = pixels(&content, cx).2;
        let before_history = history_version(&content, cx);
        window.press("escape", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            assert!(state.selected_layers.is_empty());
            assert_eq!(state.selected_layer, 0);
            assert!(state.multi_bounds.is_none());
        });
        assert_eq!(pixels(&content, cx).2, before_pixels);
        assert_eq!(history_version(&content, cx), before_history);

        window.press("escape", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            assert!(state.selected_layers.is_empty());
            assert_eq!(state.selected_layer, 0);
            assert!(state.multi_bounds.is_none());
        });
        assert_eq!(pixels(&content, cx).2, before_pixels);
        assert_eq!(history_version(&content, cx), before_history);

        content.update(cx, |app, cx| app.add_empty_layer(cx));
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                assert!(state.layers.len() >= 2, "the fixture has multiple rows");
                state.selected_layer = state.layers[0].id;
                state.selected_layers = state.layers.iter().map(|row| row.id).collect();
                state.multi_bounds = Some((1.0, 2.0, 3.0, 4.0));
            });
        });
        let before_pixels = pixels(&content, cx).2;
        let before_history = history_version(&content, cx);
        window.press("escape", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            assert!(state.selected_layers.is_empty());
            assert_eq!(state.selected_layer, 0);
            assert!(state.multi_bounds.is_none());
        });
        assert_eq!(pixels(&content, cx).2, before_pixels);
        assert_eq!(history_version(&content, cx), before_history);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn layer_reprojection_preserves_explicit_deselection_and_allows_reselection(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let initial = content.read(cx).state_for_test().read(cx).selected_layer;
        assert_eq!(initial, 0);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selected_layers,
            vec![initial],
            "the initial first layer remains selected"
        );
        let first_core_id = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .layers
            .iter()
            .find(|row| row.id == initial)
            .and_then(|row| row.core_id.clone())
            .expect("the initial row is projected");

        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.selected_layers.clear();
                state.selected_layer = 0;
                state.multi_bounds = None;
            });
            app.project_layers_with_selection(None, cx);
        });
        assert!(content
            .read(cx)
            .state_for_test()
            .read(cx)
            .selected_layers
            .is_empty());

        content.update(cx, |app, cx| {
            app.project_layers_with_selection(Some(&first_core_id), cx);
        });
        content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            assert_eq!(state.selected_layers, vec![state.selected_layer]);
            assert!(state.selected_layers.contains(&state.selected_layer));
        });
        window.render_frame(cx);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn empty_projected_selection_does_not_resolve_or_transform_the_first_layer(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Move, 2.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.show_transform_controls = true;
                state.selected_layers.clear();
                state.selected_layer = 0;
                state.multi_bounds = None;
            });
        });
        window.render_frame(cx);
        let (w, h, _) = pixels(&content, cx);
        let doc = (w, h);
        let (bounds, placement) = page(window, doc);
        let corner = window_point(bounds, placement, doc, 0.0, 0.0);
        dispatch_pointer_event(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: corner,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        content.update(cx, |app, cx| {
            assert!(app.selected_layer(cx).is_err());
            assert!(app.selected_layer_for_cursor(cx).is_err());
            assert!(!app.begin_transform_drag((0.0, 0.0), 1.0, cx));
            let overlay = app.canvas_overlay(cx);
            assert!(overlay.transform.is_none());
            assert!(!overlay.transform_controls);
            assert_eq!(
                app.canvas_cursor_policy(cx),
                crate::input::CursorPolicy::SystemDefault
            );
            app.state_for_test().update(cx, |state, _| {
                for row in &mut state.layers {
                    row.core_id = None;
                }
            });
            assert!(matches!(
                app.selected_layer(cx).map(|(_, layer)| layer.layer_type),
                Ok(LayerType::Raster)
            ));
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_cancels_move_transform_preview_and_preserves_selection(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
            app.state_for_test()
                .update(cx, |state, _| state.active_tool = ToolId::Move);
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let before_pixels = pixels(&content, cx).2;
        let before_transform = content.read(cx).active_core_transform_for_test();
        let before_history = history_version(&content, cx);
        let before_selection = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .selected_layers
            .clone();
        let (bounds, placement) = page(window, (200, 150));
        let start = window_point(bounds, placement, (200, 150), 60.0, 60.0);
        let moved = window_point(bounds, placement, (200, 150), 100.0, 90.0);
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
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        content.update(cx, |app, _| {
            let drag = app.transform_drag_for_test().expect("Move drag is live");
            assert_ne!(drag.preview.x, before_transform.expect("transform").0);
        });

        window.press("escape", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert!(app.transform_drag_for_test().is_none());
            assert_eq!(
                app.state_for_test().read(cx).selected_layers,
                before_selection
            );
        });
        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(pixels(&content, cx).2, before_pixels);
        assert_eq!(
            content.read(cx).active_core_transform_for_test(),
            before_transform
        );
        assert_eq!(history_version(&content, cx), before_history);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selected_layers,
            before_selection
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn alt_during_a_live_move_drag_shows_the_copy_cursor(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::{Modifiers, MouseMoveEvent, PlatformInput};

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
            app.state_for_test()
                .update(cx, |state, _| state.active_tool = ToolId::Move);
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, (200, 150));
        let start = window_point(bounds, placement, (200, 150), 60.0, 60.0);
        let moved = window_point(bounds, placement, (200, 150), 100.0, 90.0);
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                ..Default::default()
            }),
            cx,
        );
        content.update(cx, |app, cx| {
            assert!(
                app.transform_drag_for_test().is_some(),
                "the press started the move drag"
            );
            // The grip layer answers first wherever the pointer sits on a
            // transform handle, so the drag-layer rule is unreadable with the
            // controls shown.
            app.state_for_test()
                .update(cx, |state, _| state.show_transform_controls = false);
        });

        let move_with = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, alt: bool| {
            dispatch_pointer_event(
                window,
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: moved,
                    modifiers: Modifiers {
                        alt,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
        };

        move_with(window, cx, true);
        content.update(cx, |app, cx| {
            assert_eq!(
                app.canvas_cursor_policy(cx),
                crate::input::CursorPolicy::Cloning,
                "Alt during the drag is the copy cursor"
            );
        });

        move_with(window, cx, false);
        content.update(cx, |app, cx| {
            assert_eq!(
                app.canvas_cursor_policy(cx),
                crate::input::CursorPolicy::Grabbing,
                "releasing Alt returns to the grabbing cursor"
            );
        });

        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn hovering_the_crop_frame_paints_the_direction_resize_cursors(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::{Modifiers, MouseMoveEvent, PlatformInput};

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
            app.state_for_test().update(cx, |state, _| {
                state.active_tool = ToolId::Crop;
                state.crop_rect = Some([40.0, 30.0, 120.0, 90.0]);
            });
        });
        window.render_frame(cx);
        let (bounds, placement) = page(window, (200, 150));

        let hover = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App, doc: (f32, f32)| {
            dispatch_pointer_event(
                window,
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: window_point(bounds, placement, (200, 150), doc.0, doc.1),
                    modifiers: Modifiers::default(),
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
        };

        for (doc, expected, note) in [
            (
                (40.0, 30.0),
                crate::input::CursorPolicy::ResizeNwse,
                "NW corner",
            ),
            (
                (160.0, 30.0),
                crate::input::CursorPolicy::ResizeNesw,
                "NE corner",
            ),
            (
                (40.0, 120.0),
                crate::input::CursorPolicy::ResizeNesw,
                "SW corner",
            ),
            (
                (160.0, 120.0),
                crate::input::CursorPolicy::ResizeNwse,
                "SE corner",
            ),
            (
                (40.0, 75.0),
                crate::input::CursorPolicy::ResizeEw,
                "west edge",
            ),
            (
                (160.0, 75.0),
                crate::input::CursorPolicy::ResizeEw,
                "east edge",
            ),
            (
                (100.0, 30.0),
                crate::input::CursorPolicy::ResizeNs,
                "north edge",
            ),
            (
                (100.0, 120.0),
                crate::input::CursorPolicy::ResizeNs,
                "south edge",
            ),
            (
                (100.0, 75.0),
                crate::input::CursorPolicy::SystemDefault,
                "frame interior",
            ),
        ] {
            hover(window, cx, doc);
            content.update(cx, |app, cx| {
                assert_eq!(
                    app.canvas_cursor_policy(cx),
                    expected,
                    "the pointer at the {note} asks the matching resize cursor"
                );
            });
        }
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_cancels_group_move_preview_and_preserves_selection(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
            app.state_for_test()
                .update(cx, |state, _| state.active_tool = ToolId::Shape);
        });
        window.render_frame(cx);
        drag_doc(window, (200, 150), (20.0, 20.0), (60.0, 60.0), cx);
        drag_doc(window, (200, 150), (100.0, 60.0), (140.0, 100.0), cx);
        content.update(cx, |app, cx| {
            app.select_all_layers(cx);
            app.state_for_test().update(cx, |state, _| {
                assert!(state.selected_layers.len() >= 2);
                state.active_tool = ToolId::Move;
                state.show_transform_controls = true;
            });
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);

        let before_selection = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .selected_layers
            .clone();
        let before_transform = content.read(cx).active_core_transform_for_test();
        let before_history = history_version(&content, cx);
        let (bounds, placement) = page(window, (200, 150));
        let start = window_point(bounds, placement, (200, 150), 100.0, 75.0);
        let moved = window_point(bounds, placement, (200, 150), 130.0, 95.0);
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
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        content.update(cx, |app, _| {
            assert!(
                app.group_transform_drag.is_some(),
                "group Move drag is live"
            );
        });

        window.press("escape", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert!(app.group_transform_drag.is_none());
            assert_eq!(
                app.state_for_test().read(cx).selected_layers,
                before_selection
            );
        });
        dispatch_pointer_event(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).active_core_transform_for_test(),
            before_transform
        );
        assert_eq!(history_version(&content, cx), before_history);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).selected_layers,
            before_selection
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn canvas_context_menu_escape_does_not_cancel_underlying_move_drag(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
            app.state_for_test()
                .update(cx, |state, _| state.active_tool = ToolId::Move);
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        let before_selection = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .selected_layers
            .clone();
        let (bounds, placement) = page(window, (200, 150));
        let start = window_point(bounds, placement, (200, 150), 60.0, 60.0);
        let moved = window_point(bounds, placement, (200, 150), 100.0, 90.0);
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
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        dispatch_pointer_event(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Right,
                position: moved,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        assert!(content
            .read(cx)
            .state_for_test()
            .read(cx)
            .context_menu
            .is_some());
        content.update(cx, |app, _| {
            assert!(app.transform_drag_for_test().is_some());
        });
        assert_eq!(window.find("canvas-hitbox").focused(), Some(true));

        window.press("escape", cx);
        window.render_frame(cx);
        assert!(content
            .read(cx)
            .state_for_test()
            .read(cx)
            .context_menu
            .is_none());
        content.update(cx, |app, cx| {
            assert!(app.transform_drag_for_test().is_some());
            assert_eq!(
                app.state_for_test().read(cx).selected_layers,
                before_selection
            );
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn escape_closes_the_topmost_shape_popover_before_its_dropdown(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Shape, 2.0);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_select = Some("opt-shape-kind");
                state.stroke_popover_open = true;
            });
        });
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            assert_eq!(state.open_select, Some("opt-shape-kind"));
            assert!(state.stroke_popover_open);
        });

        window.press("escape", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            assert_eq!(state.open_select, Some("opt-shape-kind"));
            assert!(!state.stroke_popover_open);
        });
    })
    .expect("test window remains open");
}
