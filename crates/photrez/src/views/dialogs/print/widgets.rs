//! The Print dialog's row widgets and unit maths. The dialog's `render` reads
//! `AppState` once and lays out both columns; these are the pieces it lays with.

use super::*;

pub(super) fn label(text: &str) -> Div {
    div()
        .w(px(LABEL_W))
        .flex_none()
        .text_size(px(11.0))
        .text_color(color::TEXT_DIM)
        .child(text.to_string())
}

pub(super) fn header_row(title: &str, trailing: &str) -> Div {
    h_flex()
        .h(px(HEADER_H))
        .w_full()
        .items_center()
        .justify_between()
        .child(
            h_flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    Icon::default()
                        .data(icons::CHEVRON_DOWN)
                        .size(px(10.0))
                        .text_color(color::TEXT_DIM),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight(500.))
                        .text_color(color::TEXT)
                        .child(title.to_string()),
                ),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(color::TEXT_DIM)
                .child(trailing.to_string()),
        )
}

pub(super) fn field<F>(
    id: &str,
    value: String,
    cx: &Context<crate::app::App>,
    handler: F,
) -> impl IntoElement
where
    F: Fn(
            &mut crate::app::App,
            &gpui_kit::ClickEvent,
            &mut gpui_kit::Window,
            &mut Context<crate::app::App>,
        ) + 'static,
{
    div()
        .id(id.to_string())
        .test_support()
        .h(px(ROW_H))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .px(px(8.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .text_size(px(11.5))
        .text_color(color::TEXT)
        .cursor_pointer()
        .on_click(cx.listener(handler))
        .child(div().child(value))
        .child(
            Icon::default()
                .data(icons::CHEVRON_DOWN)
                .size(px(11.0))
                .text_color(color::TEXT_DIM),
        )
}

pub(super) fn checkbox<F>(
    id: &str,
    text: &str,
    checked: bool,
    cx: &Context<crate::app::App>,
    handler: F,
) -> impl IntoElement
where
    F: Fn(
            &mut crate::app::App,
            &gpui_kit::ClickEvent,
            &mut gpui_kit::Window,
            &mut Context<crate::app::App>,
        ) + 'static,
{
    div()
        .id(id.to_string())
        .test_support()
        .h(px(20.0))
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.0))
        .cursor_pointer()
        .child(
            div()
                .w(px(14.0))
                .h(px(14.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3.0))
                .border_1()
                .border_color(if checked {
                    color::ACCENT
                } else {
                    color::FIELD_BORDER
                })
                .bg(if checked { color::ACCENT } else { color::FIELD })
                .when(checked, |box_| {
                    box_.child(
                        Icon::default()
                            .data(icons::CHECK)
                            .size(px(10.0))
                            .text_color(color::BG),
                    )
                }),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(color::TEXT)
                .child(text.to_string()),
        )
        .on_click(cx.listener(handler))
}

pub(super) fn toggle<F>(
    id: &str,
    text: &str,
    on: bool,
    cx: &Context<crate::app::App>,
    handler: F,
) -> impl IntoElement
where
    F: Fn(
            &mut crate::app::App,
            &gpui_kit::ClickEvent,
            &mut gpui_kit::Window,
            &mut Context<crate::app::App>,
        ) + 'static,
{
    div()
        .id(id.to_string())
        .test_support()
        .h(px(28.0))
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(if on {
            color::ACCENT
        } else {
            color::FIELD_BORDER
        })
        .bg(if on { color::ACCENT } else { color::FIELD })
        .text_size(px(11.5))
        .text_color(if on { color::BG } else { color::TEXT })
        .cursor_pointer()
        .on_click(cx.listener(handler))
        .child(text.to_string())
}

pub(super) fn unit_label(unit: &str) -> String {
    match unit {
        "cm" => t("dialogs.print.centimeters"),
        "in" => t("dialogs.print.inches"),
        "px" => t("dialogs.print.pixels"),
        _ => t("dialogs.print.millimeters"),
    }
}

pub(super) fn to_unit(mm: f64, unit: &str) -> f64 {
    match unit {
        "cm" => mm / 10.0,
        "in" => mm / 25.4,
        "px" => mm / 25.4 * PRINT_PPI,
        _ => mm,
    }
}

pub(super) fn unit_suffix(unit: &str) -> &'static str {
    match unit {
        "cm" => "cm",
        "in" => "in",
        "px" => "px",
        _ => "mm",
    }
}

pub(super) fn paper_label(index: usize) -> String {
    use crate::app::PRINT_PAPERS;
    let (name, _, w, h) = PRINT_PAPERS[index.min(PRINT_PAPERS.len() - 1)];
    format!("{name} ({w:.0} \u{00d7} {h:.0} mm) (not listed on this printer)") // i18n-exempt: copy not in catalog
}
