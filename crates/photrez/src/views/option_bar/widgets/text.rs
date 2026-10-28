use super::*;

// ── Text ───────────────────────────────────────────────────────────────────────

/// The text bar's font-family trigger.
pub(in super::super) fn font_family_button() -> Stateful<Div> {
    div()
        .id("opt-font-family")
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(500.))
        .child(
            div()
                .text_size(px(TEXT_13))
                .font_weight(FontWeight(700.))
                .text_color(color::ACCENT)
                .font_family("Arial") // i18n-exempt: technical token
                .child("Aa"), // i18n-exempt: glyph or symbol
        )
        .child(
            div()
                .font_family("Arial") // i18n-exempt: technical token
                .text_color(color::WHITE)
                .child("Arial"), // i18n-exempt: technical token
        )
        .child(ml_label(chevron_175()))
}

/// The text bar's font-size field, resting at 48.
pub(in super::super) fn font_size_field() -> Div {
    h_flex()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .child(
            div()
                .w(px(SIZE_INPUT_W))
                .flex_none()
                .text_center()
                .child(value_text("48")),
        )
        .child(dim_span("px")) // i18n-exempt: technical token (unit)
        .child(ml_label(chevron_175()))
}

/// The text bar's weight trigger, resting at "Regular".
pub(in super::super) fn weight_button() -> Stateful<Div> {
    div()
        .id("opt-weight")
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .child(
            div()
                .w(px(WEIGHT_W))
                .flex_none()
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(WEIGHT_LABEL),
        )
        .child(ml_label(chevron_175()))
}

/// A text bar toggle button, resting off.
pub(in super::super) fn text_toggle_button(id: &'static str, glyph: Div) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(BTN))
        .w(px(BTN))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(FIELD_40)
        .text_color(ZINC)
        .child(glyph)
}

/// A text-align icon button, active or inactive.
pub(in super::super) fn text_icon_button(
    id: &'static str,
    icon: &'static [u8],
    active: bool,
) -> Stateful<Div> {
    let btn = div()
        .id(id)
        .h(px(BTN))
        .w(px(BTN))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .text_size(px(TEXT_SM));

    let btn = if active {
        btn.bg(ACCENT_20)
            .font_weight(FontWeight(700.))
            .text_color(color::WHITE)
            .shadow(vec![
                BoxShadow::new(px(0.), px(1.), SHADOW_XS).blur_radius(px(2.))
            ])
    } else {
        btn.bg(FIELD_40).text_color(ZINC)
    };

    btn.child(
        Icon::default()
            .data(icons::with_stroke(icon, STROKE_DROPDOWN))
            .size(px(ICON_MD))
            .text_color(if active { color::ACCENT } else { ZINC }),
    )
}

/// The text bar's box-mode trigger, resting at "Auto Width".
pub(in super::super) fn box_mode_button() -> Stateful<Div> {
    div()
        .id("opt-box-mode")
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_between()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(500.))
        .child(div().text_color(color::WHITE).child(BOX_MODE))
        .child(chevron_175())
}

/// The text bar's colour swatch.
pub(in super::super) fn text_color_swatch() -> Stateful<Div> {
    div()
        .id("opt-text-color")
        .size(px(BTN))
        .flex_shrink_0()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(FG)
        .shadow(vec![ring_white_20()])
}
