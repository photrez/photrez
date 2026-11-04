//! Tab reach for the dialog text fields: what a keyboard user can land on
//! and type into once a dialog is open.

use super::*;

/// Tabs until the named element reports keyboard focus, and says which element
/// it stopped on. A dialog field with no handle or no tab stop never appears in
/// the traversal, so the caller's assertion names the field that stayed out.
fn tab_until_focused(
    window: &mut gpui_kit::Window,
    id: &'static str,
    cx: &mut gpui_kit::App,
) -> bool {
    for _ in 0..40 {
        window.press("tab", cx);
        window.render_frame(cx);
        if window.find(id).focused() == Some(true) {
            return true;
        }
    }
    false
}

#[gpui_kit::test]
fn tab_reaches_the_new_document_fields_and_types_into_the_focused_one(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);

        assert!(
            tab_until_focused(window, "new-doc-name", cx),
            "Tab must reach the Name field of the open dialog"
        );
        window.press("tab", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("new-doc-width").focused(),
            Some(true),
            "the next Tab moves focus to the Width field"
        );

        // The width buffer holds the preset's 1080. Clearing it and typing one
        // digit proves the keystroke followed real focus, not a stored index.
        for _ in 0..4 {
            window.press("backspace", cx);
        }
        window.press("7", cx);
        window.render_frame(cx);
        let form = content
            .read(cx)
            .new_document_form_for_test(cx)
            .expect("the dialog owns a form");
        assert_eq!(form.width_text, "7", "the digit lands in the focused field");
        assert_eq!(form.height_text, "1080", "the unfocused field is untouched");
        assert_eq!(form.name, "New Project", "the name field is untouched");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn tab_reaches_the_resize_fields_and_types_into_the_focused_one(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.open_menu = Some(crate::state::MenuId::Image);
            });
        });
        window.render_frame(cx);
        window.click("menu-resize-canvas", cx);
        window.render_frame(cx);
        // Unlock the aspect pair so one field's input cannot rewrite the other.
        window.click("resize-aspect", cx);
        window.render_frame(cx);

        assert!(
            tab_until_focused(window, "resize-width", cx),
            "Tab must reach the Width field of the open dialog"
        );
        window.press("tab", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("resize-height").focused(),
            Some(true),
            "the next Tab moves focus to the Height field"
        );

        window.press("backspace", cx);
        window.press("6", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).resize_fields_for_test(cx),
            Some(("2".to_string(), "6".to_string())),
            "the digit lands in the focused height field"
        );
    })
    .expect("test window remains open");
}
