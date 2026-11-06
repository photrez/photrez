use super::*;

/// The Selection tool rotates the committed
/// marquee from its pin. The drag previews without touching core, and the
/// release commits one `SetSelection` carrying the angle.
#[gpui_kit::test]
fn dragging_the_selection_rotate_pin_commits_an_angle(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        set_selection(&content, cx, 100.0, 80.0, 200.0, 80.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("ctrl-t", cx);
        window.render_frame(cx);
        assert!(
            content.read(cx).selection_edit_mode_for_test(),
            "Ctrl+T arms the transform chrome before the pin drag"
        );

        let (bounds, page) = page(window, (400, 200));
        let scale = f64::from(page.2) / 400.0;
        let pin_doc = (200.0, 80.0 - 24.0 / scale);
        let centre = (200.0, 120.0);
        let radius = centre.1 - pin_doc.1;
        let from = window_point(bounds, page, (400, 200), pin_doc.0 as f32, pin_doc.1 as f32);
        let target = (-60.0_f64).to_radians();
        let to_doc = (
            centre.0 + radius * target.cos(),
            centre.1 + radius * target.sin(),
        );
        let to = window_point(bounds, page, (400, 200), to_doc.0 as f32, to_doc.1 as f32);

        // Press on the pin: the drag starts but core keeps the old angle.
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                ..Default::default()
            }),
            cx,
        );
        assert!(
            content
                .read(cx)
                .selection_rotation_preview_for_test()
                .is_some(),
            "the press on the pin starts a rotation drag"
        );
        assert_eq!(
            content
                .read(cx)
                .core_selection_for_test()
                .expect("selection")
                .angle,
            0.0,
            "a press alone does not rotate the committed selection"
        );
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: to,
                ..Default::default()
            }),
            cx,
        );
        assert_eq!(
            content
                .read(cx)
                .core_selection_for_test()
                .expect("selection")
                .angle,
            0.0,
            "the live rotation stays display-only until the release"
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                ..Default::default()
            }),
            cx,
        );
        let angle = content
            .read(cx)
            .core_selection_for_test()
            .expect("selection")
            .angle;
        assert!(
            (angle - 30.0).abs() < 1.0,
            "releasing after a 30 degree drag commits 30 degrees, got {angle}"
        );
    })
    .expect("test window remains open");
}

/// The rotation applies the mouse-up coordinate before the commit (the
/// transform drag's latest-sample rule): a press on the pin whose FIRST
/// sample is the release position must still commit the rotated angle.
#[gpui_kit::test]
fn selection_rotation_applies_the_mouse_up_coordinate(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        set_selection(&content, cx, 100.0, 80.0, 200.0, 80.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("ctrl-t", cx);
        window.render_frame(cx);

        let (bounds, page) = page(window, (400, 200));
        let scale = f64::from(page.2) / 400.0;
        let pin_doc = (200.0, 80.0 - 24.0 / scale);
        let centre = (200.0, 120.0);
        let radius = centre.1 - pin_doc.1;
        let from = window_point(bounds, page, (400, 200), pin_doc.0 as f32, pin_doc.1 as f32);
        let target = (-60.0_f64).to_radians();
        let to_doc = (
            centre.0 + radius * target.cos(),
            centre.1 + radius * target.sin(),
        );
        let to = window_point(bounds, page, (400, 200), to_doc.0 as f32, to_doc.1 as f32);

        // Press the pin and release at the rotated position with no move in
        // between: the release coordinate is the only rotation sample.
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
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: to,
                ..Default::default()
            }),
            cx,
        );
        let angle = content
            .read(cx)
            .core_selection_for_test()
            .expect("selection")
            .angle;
        assert!(
            (angle - 30.0).abs() < 1.0,
            "the mouse-up coordinate commits the rotated angle, got {angle}"
        );
    })
    .expect("test window remains open");
}

/// The rotate sprite paints at the pointer on the pin
/// hover in edit mode and follows the pointer during a pin rotation; away
/// from the pin nothing paints.
#[gpui_kit::test]
fn rotate_cursor_sprite_follows_the_pointer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(400, 200, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Selection, 1.0);
        set_selection(&content, cx, 100.0, 80.0, 200.0, 80.0);
        window.render_frame(cx);
        tab_to_canvas(window, cx);
        window.press("ctrl-t", cx);
        window.render_frame(cx);

        let (bounds, page) = page(window, (400, 200));
        let scale = f64::from(page.2) / 400.0;
        let pin_doc = (200.0, 80.0 - 24.0 / scale);
        let pin = window_point(bounds, page, (400, 200), pin_doc.0 as f32, pin_doc.1 as f32);
        let away = window_point(bounds, page, (400, 200), 40.0, 180.0);

        // Hovering the pin paints the sprite at the pointer.
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: pin,
                ..Default::default()
            }),
            cx,
        );
        let sprite = content
            .update(cx, |app, cx| app.rotate_cursor_for_test(cx))
            .expect("the pin hover paints the rotate sprite");
        assert_eq!(
            (sprite.position[0], sprite.position[1]),
            (f32::from(pin.x), f32::from(pin.y)),
            "the sprite anchors at the pointer"
        );
        // The pin sits straight above the selection centre: the pointer angle
        // is -90 degrees, plus the sprite's 45-degree base tilt.
        assert!(
            (f64::from(sprite.angle) + 45.0).abs() < 1.0,
            "the sprite angle follows the pointer about the centre, got {}",
            sprite.angle
        );

        // Away from the pin nothing paints.
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: away,
                ..Default::default()
            }),
            cx,
        );
        assert!(
            content
                .update(cx, |app, cx| app.rotate_cursor_for_test(cx))
                .is_none(),
            "a hover away from the pin paints no sprite"
        );

        // During the pin drag the sprite keeps following the pointer.
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: pin,
                ..Default::default()
            }),
            cx,
        );
        dispatch(
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: pin,
                ..Default::default()
            }),
            cx,
        );
        let dragged = window_point(bounds, page, (400, 200), 120.0, 30.0);
        dispatch(
            window,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: dragged,
                ..Default::default()
            }),
            cx,
        );
        let sprite = content
            .update(cx, |app, cx| app.rotate_cursor_for_test(cx))
            .expect("the rotation drag keeps the sprite at the pointer");
        assert_eq!(
            (sprite.position[0], sprite.position[1]),
            (f32::from(dragged.x), f32::from(dragged.y)),
            "the drag sprite follows the pointer"
        );
        dispatch(
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: dragged,
                ..Default::default()
            }),
            cx,
        );
        assert!(
            content
                .update(cx, |app, cx| app.rotate_cursor_for_test(cx))
                .is_none(),
            "releasing away from the pin clears the sprite"
        );
    })
    .expect("test window remains open");
}
