//! The Rename dialog: the live buffer, Enter to commit and Escape to cancel.

use super::*;

/// The rename prompt shows the live buffer; Enter commits, Escape cancels.
pub(super) fn rename_body(
    state: &Entity<AppState>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> Div {
    let content = state
        .read(cx)
        .rename_layer
        .as_ref()
        .map(|session| session.content.clone())
        .unwrap_or_default();
    // The prompt's one text field carries its own handle, so Tab lands on it.
    let field_focus = state.read(cx).rename_field_focus.clone();
    let cancel_state = state.clone();
    v_flex()
        .p(px(16.0))
        .gap(px(8.0))
        .w_full()
        .child(chrome::tabbable_field(
            div()
                .id("rename-value")
                .test_support()
                .h(px(28.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(4.0))
                .border_1()
                .border_color(color::BORDER)
                .shadow(ui::outline_cue(true))
                .bg(color::FIELD)
                .text_size(px(12.0))
                .text_color(color::TEXT)
                .aria_label(t("layers.renameLayer").to_string())
                .child(format!("{content}|")),
            &field_focus,
        ))
        .child(
            div()
                .text_size(px(10.0))
                .text_color(color::TEXT_DIM)
                .child("Enter to rename, Escape to cancel.".to_string()), // i18n-exempt: native-only copy; no catalogue equivalent
        )
        .child(
            div()
                .mt(px(4.0))
                .flex()
                .gap(px(8.0))
                .child(
                    div()
                        .id("rename-cancel")
                        .test_support()
                        .h(px(26.0))
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
                                let _ = &cancel_state;
                                app.cancel_rename_layer(cx);
                            }),
                        )
                        .child(t("common.cancel").to_string()),
                )
                .child(
                    div()
                        .id("rename-confirm")
                        .test_support()
                        .h(px(26.0))
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
                            let result = app.commit_rename_layer(cx);
                            match result {
                                Ok(()) => app.set_status_error(None, cx),
                                Err(error) => {
                                    app.set_status_error(Some(format!("Rename: {error}")), cx)
                                }
                            }
                        }))
                        .child("Rename".to_string()), // i18n-exempt: native-only copy; no catalogue equivalent
                ),
        )
}
