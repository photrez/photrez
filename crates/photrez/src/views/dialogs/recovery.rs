//! The startup recovery prompt: one row per autosave entry and three answers -
//! restore the entries, leave them in the store for another day, or delete them.

use super::*;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::Sizable;
use gpui_kit::StatefulInteractiveElement;

pub(super) fn recovery_body(
    state: &Entity<AppState>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> Div {
    let entries = state.read(cx).recovery_entries.clone();
    let mut list = v_flex()
        .id("recovery-list")
        .w_full()
        .max_h(px(156.0))
        .gap(px(4.0))
        .overflow_y_scroll();
    for entry in entries {
        let already_open = entry.already_open;
        let mut row = h_flex()
            .w_full()
            .min_h(px(28.0))
            .items_center()
            .gap(px(8.0))
            .text_size(px(12.0))
            .text_color(color::TEXT)
            .child(entry.name);
        if already_open {
            row = row.child(
                div()
                    .text_color(color::TEXT_DIM)
                    .child(t("dialogs.recovery.alreadyOpen").to_string()),
            );
        }
        list = list.child(row);
    }
    v_flex()
        .w_full()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(17.0))
                .text_color(color::TEXT_DIM)
                .child(t("dialogs.recovery.description")),
        )
        .child(list)
}

pub(super) fn recovery_actions(
    cx: &gpui_kit::Context<crate::app::App>,
) -> Vec<gpui_kit::AnyElement> {
    let discard = recovery_button("recovery-discard", &t("common.discard"), false, cx)
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            app.discard_autosaves(cx);
        }))
        .into_any_element();
    let decline = recovery_button(
        "recovery-decline",
        &t("dialogs.recovery.decline"),
        false,
        cx,
    )
    .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
        cx.stop_propagation();
        app.decline_autosaves(cx);
    }))
    .into_any_element();
    let restore = recovery_button("recovery-restore", &t("dialogs.recovery.restore"), true, cx)
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            app.restore_autosaves(cx);
        }))
        .into_any_element();
    vec![discard, decline, restore]
}

fn recovery_button(id: &'static str, label: &str, primary: bool, cx: &gpui_kit::App) -> Button {
    let background = if primary { color::ACCENT } else { color::FIELD };
    let foreground = if primary { color::BG } else { color::TEXT };
    Button::new(id)
        .xsmall()
        .custom(
            ButtonCustomVariant::new(cx)
                .color(background)
                .foreground(foreground)
                .hover(background.opacity(0.9))
                .active(background.opacity(0.8)),
        )
        // The custom variant tints its normal fill; keep the application fill opaque.
        .bg(background)
        .h(px(BTN_H))
        .min_w(px(BTN_MIN_W))
        .px(px(BAR_PX))
        .rounded(px(BTN_R))
        .border_1()
        .border_color(if primary {
            color::ACCENT
        } else {
            color::FIELD_BORDER
        })
        .accessibility_label(label.to_string())
        .child(
            div()
                .font_family(ui::FONT_UI)
                .text_size(px(BTN_TEXT))
                .font_weight(FontWeight(if primary { 600.0 } else { 500.0 }))
                .child(label.to_string()),
        )
}
