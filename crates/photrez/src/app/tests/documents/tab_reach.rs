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

/// The ids outside an open dialog that a keyboard user must not reach while it
/// is up. Each is a real tab stop in the shell, so an escaping Tab lands on one.
const SHELL_STOPS: [&str; 4] = ["canvas-hitbox", "num-x", "prop-x", "menus.file"];

/// Tab wraps at the dialog's last control and Shift+Tab at its
/// first, so focus never leaves the open dialog.
#[gpui_kit::test]
fn tab_stays_inside_the_open_dialog_and_wraps(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        crate::app::tests::canvas_input::tab_to_canvas(window, cx);
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);

        let mut name_hits = 0;
        for _ in 0..30 {
            window.press("tab", cx);
            window.render_frame(cx);
            for stop in SHELL_STOPS {
                assert_ne!(
                    window.find(stop).focused(),
                    Some(true),
                    "Tab escaped the open dialog to `{stop}`"
                );
            }
            if window.find("new-doc-name").focused() == Some(true) {
                name_hits += 1;
            }
        }
        assert!(
            name_hits >= 2,
            "the traversal never came back around to the Name field, so it did not wrap \
             (hits: {name_hits})"
        );
    })
    .expect("test window remains open");
}

/// Closing returns focus to whatever held it before the dialog opened.
#[gpui_kit::test]
fn closing_a_dialog_returns_focus_to_what_had_it(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        crate::app::tests::canvas_input::tab_to_canvas(window, cx);
        assert_eq!(
            window.find("canvas-hitbox").focused(),
            Some(true),
            "the canvas holds focus before the dialog opens"
        );
        content.update(cx, |app, cx| app.open_new_document(cx));
        window.render_frame(cx);

        window.press("escape", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("canvas-hitbox").focused(),
            Some(true),
            "Escape must give focus back to the element that had it before the dialog"
        );
    })
    .expect("test window remains open");
}

/// One Space press confirms an open New Document dialog with no prior focus
/// step, which is what makes the missing focus-on-open a cosmetic gap rather
/// than a blocked action. The dialog is asserted open first, so the press
/// cannot pass by never having reached a dialog.
#[gpui_kit::test]
fn space_confirms_an_open_new_document_dialog_with_no_prior_focus(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.press("ctrl-n", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert_eq!(
                app.state_for_test().read(cx).active_dialog,
                Some(crate::state::DialogId::NewDocument),
                "the dialog has to be open before the press means anything"
            );
        });
        window.press("space", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            assert_eq!(
                state.active_dialog, None,
                "Space must confirm the dialog the keyboard was sent to"
            );
        });
    })
    .expect("test window remains open");
}
