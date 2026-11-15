//! The shell menus: each top-level trigger opens its own measured popup, and the menu commands open the shell dialogs.

use super::*;

/// Each top-level trigger opens its own measured popup; an outside click closes it.
#[gpui_kit::test]
fn menu_triggers_open_measured_popups(cx: &mut TestAppContext) {
    let (handle, _content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let menus = [
            ("menus.file", "menu-file-popup", 58.0),
            ("menus.edit", "menu-edit-popup", 99.0),
            ("menus.image", "menu-image-popup", 142.0),
            ("menus.layer", "menu-layer-popup", 198.0),
            ("menus.view", "menu-view-popup", 249.0),
            ("menus.window", "menu-window-popup", 298.0),
            ("menus.help", "menu-help-popup", 367.0),
            ("menus.settings", "menu-settings-popup", 415.0),
        ];
        for (trigger, popup, x) in menus {
            window.click(trigger, cx);
            window.render_frame(cx);
            let bounds = window.find(popup).bounds();
            assert_eq!(bounds.size.width, px(224.), "{popup} width");
            assert_eq!(bounds.origin.y, px(36.), "{popup} top edge");
            assert_eq!(bounds.origin.x, px(x), "{popup} trigger origin");
            assert!(
                window.try_find("menu-overlay").is_some(),
                "the open menu must own the dismissal overlay"
            );

            window.click_at(
                "menu-overlay",
                Point {
                    x: px(1200.),
                    y: px(700.),
                },
                cx,
            );
            window.render_frame(cx);
            assert!(
                window.try_find(popup).is_none(),
                "an outside click must close {popup}"
            );
        }
    })
    .unwrap();
}

/// Menu commands open the three shell dialogs through the same event path.
#[gpui_kit::test]
fn menu_commands_open_the_shell_dialogs(cx: &mut TestAppContext) {
    let (handle, content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let steps = [
            ("menus.file", "menu-new-document", "dialog-new-document"),
            ("menus.help", "menu-about", "dialog-about"),
            ("menus.settings", "menu-preferences", "dialog-settings"),
        ];
        for (trigger, row, dialog_id) in steps {
            window.click(trigger, cx);
            window.render_frame(cx);
            window.click(row, cx);
            window.render_frame(cx);
            assert!(window.find(dialog_id).visible(), "{dialog_id} must mount");
            if dialog_id == "dialog-new-document" {
                window.click("new-doc-tab-1", cx);
                window.render_frame(cx);
                assert_eq!(
                    content
                        .read(cx)
                        .new_document_form_for_test(cx)
                        .expect("File > New must create a form session")
                        .category,
                    crate::state::NewDocCategory::SocialMedia
                );
            }
            assert!(
                window.try_find("menu-overlay").is_none(),
                "the menu must close when its command runs"
            );
            // The overlay fades in over 200ms; the test clock must pass it
            // before the overlay counts as visible for a click.
            cx.background_executor()
                .advance_clock(std::time::Duration::from_millis(250));
            window.render_frame(cx);

            window.click_at(
                "dialog-overlay",
                Point {
                    x: px(10.),
                    y: px(700.),
                },
                cx,
            );
            window.render_frame(cx);
            assert!(
                window.find(dialog_id).visible(),
                "{dialog_id} must stay open after an outside click"
            );
            window.click(
                match dialog_id {
                    "dialog-new-document" => "new-doc-cancel",
                    "dialog-about" => "about-close",
                    _ => "settings-ok",
                },
                cx,
            );
            window.render_frame(cx);
            assert!(
                window.try_find(dialog_id).is_none(),
                "{dialog_id} must close from its own control"
            );
        }
    })
    .unwrap();
}
