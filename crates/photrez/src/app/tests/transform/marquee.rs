//! The move marquee: it selects the overlapping layers, and a cancelled capture restores the first.

use super::*;

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
