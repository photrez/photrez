//! The startup recovery prompt: one row per autosave entry and three answers -
//! restore the entries, leave them in the store for another day, or delete them.

use super::*;

pub(super) fn recovery_body(
    state: &Entity<AppState>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> Div {
    let entries = state.read(cx).recovery_entries.clone();
    let mut body = v_flex().p(px(16.0)).gap(px(6.0)).w_full();
    body = body.child(
        div()
            .text_size(px(11.5))
            .text_color(color::TEXT_DIM)
            .child("Unsaved documents found in the autosave store:".to_string()), // i18n-exempt: native-only copy; no catalogue equivalent
    );
    for entry in entries {
        let already_open = entry.already_open;
        let mut row = div()
            .h(px(24.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(11.5))
            .text_color(color::TEXT)
            .child(entry.name);
        if already_open {
            row = row.child(
                div()
                    .text_color(color::TEXT_DIM)
                    .child(t("dialogs.recovery.alreadyOpen").to_string()),
            );
        }
        body = body.child(row);
    }
    body.child(
        div()
            .mt(px(6.0))
            .flex()
            .gap(px(8.0))
            .child(
                div()
                    .id("recovery-discard")
                    .test_support()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(color::BORDER)
                    .text_size(px(11.5))
                    .text_color(color::TEXT_DIM)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            app.discard_autosaves(cx);
                        }),
                    )
                    .child(t("common.discard").to_string()),
            )
            .child(
                div()
                    .id("recovery-decline")
                    .test_support()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(color::BORDER)
                    .text_size(px(11.5))
                    .text_color(color::TEXT)
                    .cursor_pointer()
                    .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        app.decline_autosaves(cx);
                    }))
                    .child("Don't restore".to_string()), // i18n-exempt: native-only copy; no catalogue equivalent
            )
            .child(
                div()
                    .id("recovery-restore")
                    .test_support()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .bg(color::ACCENT)
                    .text_size(px(11.5))
                    .text_color(color::BG)
                    .cursor_pointer()
                    .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        app.restore_autosaves(cx);
                    }))
                    .child("Restore".to_string()), // i18n-exempt: native-only copy; no catalogue equivalent
            ),
    )
}
