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

/// The reference guards each axis with its own flag: the position lock stops X
/// and Y only, and the rotation row keeps answering.
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
