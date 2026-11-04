use super::*;

/// The typography branch of the single-layer inspector: font, size/weight,
/// style/align, box mode, spacing and colour/stroke, in the panel's order.
/// Values come from the projected text layer;
/// the inline actions edit that layer through the same app handlers the option
/// bar uses.
pub(super) fn typography_section(
    state: &gpui_kit::Entity<crate::state::AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    use crate::state::TextAlignTool;

    let (
        font_size,
        font_weight,
        italic,
        underline,
        align,
        line_height,
        letter_spacing,
        color,
        stroke_on,
    ) = {
        let s = state.read(cx);
        (
            s.selected_text_font_size,
            s.selected_text_font_weight,
            s.selected_text_italic,
            s.selected_text_underline,
            s.selected_text_align,
            s.selected_text_line_height,
            s.selected_text_letter_spacing,
            s.selected_text_color.unwrap_or(s.fg_color),
            s.selected_text_stroke_width > 0.0,
        )
    };
    let weight_label = if font_weight >= 700.0 {
        "Bold"
    } else if font_weight >= 600.0 {
        "Semibold"
    } else if font_weight >= 500.0 {
        "Medium"
    } else {
        "Regular"
    };
    let align_button = |id: &'static str,
                        icon: &'static [u8],
                        target: TextAlignTool,
                        active: bool,
                        cx: &Context<crate::app::App>| {
        div()
            .id(id)
            .test_support()
            .h(px(24.0))
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(R3))
            .border(px(1.0))
            .border_color(if active {
                color::ACCENT
            } else {
                color::FIELD_BORDER
            })
            .bg(color::FIELD)
            .cursor_pointer()
            .on_click(
                cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                    let _ = app.set_text_align(target, cx);
                }),
            )
            .child(
                Icon::default()
                    .data(icon)
                    .size(px(14.0))
                    .text_color(if active {
                        color::ACCENT
                    } else {
                        color::TEXT_DIM
                    }),
            )
    };
    v_flex()
        .id("properties-typography")
        .test_support()
        .w_full()
        .flex_shrink_0()
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .border_b(px(1.0))
        .border_color(color::DIVIDER)
        .child(section_header(icons::TYPE, &t("properties.typography")))
        .child(
            v_flex().mt(px(HEADER_GAP)).gap(px(8.0)).w_full().children([
                ui::prop_row(
                    &t("properties.font"),
                    ui::select_field("Arial", None), // i18n-exempt: technical token
                )
                .into_any_element(),
                ui::prop_row(
                    &t("properties.sizeAndWeight"),
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(ui::num_field(
                            None,
                            Some(70.0),
                            &format!("{font_size:.0}"),
                            Some("px"),
                        ))
                        .child(ui::select_field(weight_label, None)),
                )
                .into_any_element(),
                ui::prop_row(
                    &t("properties.styleAndAlign"),
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(
                            div()
                                .id("prop-text-italic")
                                .test_support()
                                .h(px(24.0))
                                .w(px(28.0))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(R3))
                                .border(px(1.0))
                                .border_color(if italic {
                                    color::ACCENT
                                } else {
                                    color::FIELD_BORDER
                                })
                                .bg(color::FIELD)
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    |app, _: &gpui_kit::ClickEvent, _window, cx| {
                                        let _ = app.toggle_text_italic(cx);
                                    },
                                ))
                                .child(
                                    div()
                                        .italic()
                                        .font_family("Georgia")
                                        .text_size(px(11.0))
                                        .text_color(color::TEXT)
                                        .child("I"), // i18n-exempt: style glyph
                                ),
                        )
                        .child(
                            div()
                                .id("prop-text-underline")
                                .test_support()
                                .h(px(24.0))
                                .w(px(28.0))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(R3))
                                .border(px(1.0))
                                .border_color(if underline {
                                    color::ACCENT
                                } else {
                                    color::FIELD_BORDER
                                })
                                .bg(color::FIELD)
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    |app, _: &gpui_kit::ClickEvent, _window, cx| {
                                        let _ = app.toggle_text_underline(cx);
                                    },
                                ))
                                .child(
                                    div()
                                        .underline()
                                        .font_family(ui::FONT_UI)
                                        .text_size(px(11.0))
                                        .text_color(color::TEXT)
                                        .child("U"), // i18n-exempt: style glyph
                                ),
                        )
                        .child(align_button(
                            "prop-text-align-left",
                            icons::ALIGN_TEXT_LEFT,
                            TextAlignTool::Left,
                            align == TextAlignTool::Left,
                            cx,
                        ))
                        .child(align_button(
                            "prop-text-align-center",
                            icons::ALIGN_TEXT_CENTER,
                            TextAlignTool::Center,
                            align == TextAlignTool::Center,
                            cx,
                        ))
                        .child(align_button(
                            "prop-text-align-right",
                            icons::ALIGN_TEXT_RIGHT,
                            TextAlignTool::Right,
                            align == TextAlignTool::Right,
                            cx,
                        )),
                )
                .into_any_element(),
                ui::prop_row(
                    &t("properties.boxMode"),
                    ui::select_field("Auto Width (Point)", None), // i18n-exempt: technical token
                )
                .into_any_element(),
                ui::prop_row(
                    &t("properties.spacing"),
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(ui::num_field(
                            Some("Line H"),
                            None,
                            &format!("{line_height:.1}"),
                            None,
                        ))
                        .child(ui::num_field(
                            Some("Letter"),
                            None,
                            &format!("{letter_spacing:.0}"),
                            Some("px"),
                        )),
                )
                .into_any_element(),
                ui::prop_row(
                    &t("properties.colorAndStroke"),
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(
                            div()
                                .id("prop-text-color")
                                .test_support()
                                .size(px(24.0))
                                .flex_shrink_0()
                                .rounded(px(R3))
                                .border(px(1.0))
                                .border_color(color::FIELD_BORDER)
                                .bg(crate::palette::hsla(color))
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    |app, _: &gpui_kit::ClickEvent, window, cx| {
                                        app.open_color_picker(
                                            crate::state::ColorTarget::Text,
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                        )
                        .child(
                            div()
                                .id("prop-text-stroke")
                                .test_support()
                                .h(px(24.0))
                                .flex_1()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(R3))
                                .border(px(1.0))
                                .border_color(if stroke_on {
                                    color::ACCENT
                                } else {
                                    color::FIELD_BORDER
                                })
                                .bg(color::FIELD)
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    |app, _: &gpui_kit::ClickEvent, _window, cx| {
                                        app.toggle_text_stroke(cx);
                                    },
                                ))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .font_weight(FontWeight(700.))
                                        .text_color(if stroke_on {
                                            color::TEXT
                                        } else {
                                            color::TEXT_DIM
                                        })
                                        .child(t("properties.stroke")),
                                ),
                        ),
                )
                .into_any_element(),
            ]),
        )
}
