//! Tab reach for the inline numeric fields: which rows a keyboard user can
//! land on, and what the row they left keeps behind.

use super::*;
use gpui_kit::test::TestWindowExt as _;

/// Tabs until the named element reports keyboard focus. A numeric row with no
/// tab stop never enters the traversal, so the caller's assertion names the row
/// that stayed out.
fn tab_until_focused(
    window: &mut gpui_kit::Window,
    id: &'static str,
    cx: &mut gpui_kit::App,
) -> bool {
    for _ in 0..64 {
        window.press("tab", cx);
        window.render_frame(cx);
        if window.find(id).focused() == Some(true) {
            return true;
        }
    }
    false
}

/// The open inline edit, so a test can name the row a keystroke opened and the
/// buffer it holds.
fn session(content: &Entity<App>, cx: &gpui_kit::App) -> Option<crate::state::NumFieldSession> {
    content.read(cx).state_for_test().read(cx).num_field.clone()
}

fn opened_row(content: &Entity<App>, cx: &gpui_kit::App) -> Option<(&'static str, String)> {
    session(content, cx).map(|field| (field.id, field.buffer))
}

#[gpui_kit::test]
fn tab_reaches_the_properties_position_rows_and_types_into_the_focused_one(
    cx: &mut gpui_kit::TestAppContext,
) {
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

        window.click("prop-x", cx);
        window.render_frame(cx);
        window.press("3", cx);
        window.render_frame(cx);
        assert_eq!(
            opened_row(&content, cx),
            Some(("prop-x", "3".to_string())),
            "the click opens the row and the digit replaces its seed",
        );

        assert!(
            tab_until_focused(window, "prop-y", cx),
            "Tab must reach the Y row of the Properties panel"
        );
        window.press("4", cx);
        window.render_frame(cx);
        assert_eq!(
            opened_row(&content, cx),
            Some(("prop-y", "4".to_string())),
            "the digit opens the row Tab focused, not the row it left",
        );

        let held = content
            .read(cx)
            .transform_readout(cx)
            .expect("the fixture document has a selected layer");
        assert_eq!(
            held.x, 3.0,
            "the row the user left keeps the value it typed"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn tab_reaches_the_brush_size_field_and_types_into_the_focused_one(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        crate::app::tests::canvas_input::set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        crate::app::tests::canvas_input::tab_to_canvas(window, cx);

        assert!(
            tab_until_focused(window, "opt-brush-size", cx),
            "Tab must reach the brush Size field of the option bar"
        );
        window.press("9", cx);
        window.render_frame(cx);
        assert_eq!(
            opened_row(&content, cx),
            Some(("opt-brush-size", "9".to_string())),
            "the digit lands in the focused size field",
        );

        assert!(
            tab_until_focused(window, "opt-brush-hardness", cx),
            "Tab must reach the brush Hardness field of the option bar"
        );
        window.press("5", cx);
        window.render_frame(cx);
        assert_eq!(
            opened_row(&content, cx),
            Some(("opt-brush-hardness", "5".to_string())),
            "the digit opens the field Tab focused",
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).brush_size,
            9.0,
            "the field the user left keeps its committed size"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn tab_reaches_the_print_copies_field_and_types_into_the_focused_one(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        crate::app::tests::print::open_print_dialog(&content, window, cx);

        window.click("print-copies", cx);
        window.render_frame(cx);
        window.press("3", cx);
        window.render_frame(cx);
        assert_eq!(
            opened_row(&content, cx),
            Some(("print-copies", "3".to_string())),
            "the click opens the copies field and the digit replaces its seed",
        );

        assert!(
            tab_until_focused(window, "print-width", cx),
            "Tab must reach the Width field of the open Print dialog"
        );
        window.press("2", cx);
        window.render_frame(cx);
        assert_eq!(
            opened_row(&content, cx),
            Some(("print-width", "2".to_string())),
            "the digit opens the field Tab focused through the click's own unit basis",
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).print_copies,
            3,
            "the field the user left keeps its committed copy count"
        );
    })
    .expect("test window remains open");
}
