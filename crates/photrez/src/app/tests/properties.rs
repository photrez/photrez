//! Mounted coverage for the Properties panel's transform rows. They describe the
//! selected layer rather than a specimen, and a typed value commits through the
//! same core command the Move bar's fields use.

use super::*;
use gpui_kit::test::TestWindowExt as _;
use photrez_core::protocol::LockKind;

fn panel_readout(content: &Entity<App>, cx: &gpui_kit::App) -> crate::state::LayerTransformReadout {
    content
        .read(cx)
        .transform_readout(cx)
        .expect("the fixture document has a selected layer")
}

fn session(content: &Entity<App>, cx: &gpui_kit::App) -> Option<crate::state::NumFieldSession> {
    content.read(cx).state_for_test().read(cx).num_field.clone()
}

#[gpui_kit::test]
fn the_properties_transform_rows_describe_and_edit_the_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Properties;
            });
        });
        window.render_frame(cx);

        window.click("prop-w", cx);
        window.render_frame(cx);
        let opened = session(&content, cx).expect("the size row opens a number session");
        assert_eq!(
            (opened.id, opened.buffer.as_str()),
            ("prop-w", "2"),
            "the row seeds the fixture's own 2 px width, not a painted constant"
        );

        window.press("4", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let held = panel_readout(&content, cx);
        assert_eq!(held.width, 4.0, "the W row scales the layer");
        assert_eq!(
            held.height, 2.0,
            "the held ratio moves the other axis with it"
        );
        assert_eq!(held.x, 0.0, "a size edit leaves the position alone");

        window.click("prop-constrain-ratio", cx);
        window.render_frame(cx);
        assert!(
            !content.read(cx).state_for_test().read(cx).constrain_ratio,
            "the link frees the ratio"
        );

        window.click("prop-w", cx);
        window.press("8", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let free = panel_readout(&content, cx);
        assert_eq!(free.width, 8.0, "the W row commits again");
        assert_eq!(free.height, 2.0, "with the ratio free the other axis stays");

        window.click("prop-x", cx);
        window.press("3", cx);
        window.press("5", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let moved = panel_readout(&content, cx);
        assert_eq!(moved.x, 35.0, "the X row submits the position");
        assert_eq!(
            moved.width, 8.0,
            "a position edit leaves the frame size alone"
        );

        window.click("prop-r", cx);
        window.press("3", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let turned = panel_readout(&content, cx);
        assert_eq!(turned.rotation, 30.0, "the R row submits the rotation");
        assert_eq!(turned.x, 35.0, "a rotation edit leaves the position alone");
    })
    .expect("test window remains open");
}

/// Each axis carries its own guard flag: the position lock stops X
/// and Y only, and the rotation row keeps answering.
/// The opacity row follows the layer the same way the transform rows do: the
/// Layers panel's step buttons move the projected value the row prints.
#[gpui_kit::test]
fn the_properties_opacity_row_follows_the_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Properties;
            });
        });
        window.render_frame(cx);
        assert!(
            window.try_find("prop-opacity-value").is_some(),
            "the panel carries its opacity readout"
        );

        content.update(cx, |app, cx| app.adjust_selected_opacity(-0.4, cx));
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_opacity,
            0.6,
            "the row's source value moves with the layer"
        );
    })
    .expect("test window remains open");
}

/// The panel's opacity track is one of the slider hosts: the pointer's x over
/// the laid-out track maps onto the layer's opacity, and the drag keeps
/// answering while the pointer travels.
#[gpui_kit::test]
fn dragging_the_properties_opacity_track_sets_the_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Properties;
            });
        });
        window.render_frame(cx);

        let bounds = window.find("prop-opacity-track").bounds();
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        assert!(width > 0.0 && height > 0.0, "the track is laid out");
        let mid_y = gpui_kit::px(height * 0.5);

        window.drag(
            bounds.origin + gpui_kit::point(gpui_kit::px(width * 0.5), mid_y),
            bounds.origin + gpui_kit::point(gpui_kit::px(width * 0.25), mid_y),
            cx,
        );
        window.render_frame(cx);
        let dropped = content.read(cx).state_for_test().read(cx).layer_opacity;
        assert!(
            (dropped - 0.25).abs() <= 0.03,
            "dragging to a quarter of the track sets 25 % opacity, got {dropped}"
        );

        window.drag(
            bounds.origin + gpui_kit::point(gpui_kit::px(width * 0.25), mid_y),
            bounds.origin + gpui_kit::point(gpui_kit::px(width * 0.9), mid_y),
            cx,
        );
        window.render_frame(cx);
        let raised = content.read(cx).state_for_test().read(cx).layer_opacity;
        assert!(
            (raised - 0.9).abs() <= 0.03,
            "a second drag still maps, got {raised}"
        );
    })
    .expect("test window remains open");
}

/// The track answers the keyboard as well: a click focuses it, arrows step one
/// percent, and Home and End go to the ends.
#[gpui_kit::test]
fn the_properties_opacity_track_answers_arrow_keys(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Properties;
            });
        });
        window.render_frame(cx);

        let bounds = window.find("prop-opacity-track").bounds();
        window.click_at(
            "prop-opacity-track",
            gpui_kit::point(
                gpui_kit::px(f32::from(bounds.size.width) * 0.5),
                gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            ),
            cx,
        );
        window.render_frame(cx);
        let middle = content.read(cx).state_for_test().read(cx).layer_opacity;
        assert!(
            (middle - 0.5).abs() <= 0.03,
            "the click that focuses the track maps to 50 %, got {middle}"
        );

        window.press("right", cx);
        window.press("up", cx);
        let raised = content.read(cx).state_for_test().read(cx).layer_opacity;
        assert!(
            raised > middle + 0.015,
            "arrows step the opacity up, got {raised}"
        );

        window.press("home", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_opacity,
            0.0,
            "Home empties the layer"
        );
        window.press("end", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).layer_opacity,
            1.0,
            "End restores full opacity"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn a_position_lock_stops_only_the_properties_position_rows(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Properties;
            });
        });
        let core_id = content
            .read(cx)
            .selected_layer_for_cursor(cx)
            .expect("the fixture layer is selected")
            .1
            .id;
        content.update(cx, |app, cx| {
            app.toggle_layer_lock(core_id, LockKind::Position, cx);
        });
        window.render_frame(cx);
        assert!(
            panel_readout(&content, cx).lock_position,
            "the readout carries the layer's own position lock"
        );

        window.click("prop-x", cx);
        window.render_frame(cx);
        assert!(
            session(&content, cx).is_none(),
            "a locked position row opens no edit"
        );

        window.click("prop-r", cx);
        window.press("4", cx);
        window.press("5", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            panel_readout(&content, cx).rotation,
            45.0,
            "the rotation row is not guarded by the position lock"
        );
    })
    .expect("test window remains open");
}

/// The track is a tab stop in its own right: Tab from the canvas focuses it, and
/// the arrows then step the layer without any pointer having touched it.
#[gpui_kit::test]
fn the_properties_opacity_track_is_reachable_by_tab(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Properties;
            });
        });
        window.render_frame(cx);
        crate::app::tests::canvas_input::tab_to_canvas(window, cx);

        let mut reached = false;
        for _ in 0..32 {
            window.press("tab", cx);
            window.render_frame(cx);
            if window.find("prop-opacity-track").focused() == Some(true) {
                reached = true;
                break;
            }
        }
        assert!(reached, "Tab from the canvas must reach the opacity track");

        let before = content.read(cx).state_for_test().read(cx).layer_opacity;
        window.press("left", cx);
        window.press("left", cx);
        let after = content.read(cx).state_for_test().read(cx).layer_opacity;
        assert!(
            after < before - 0.015,
            "arrows step the track that Tab focused, got {before} then {after}"
        );
    })
    .expect("test window remains open");
}
