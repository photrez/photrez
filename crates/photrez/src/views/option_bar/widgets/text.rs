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

/// The text bar's font-size field. Clicking starts an inline edit; valid
/// keystrokes commit live, Enter ends, Escape restores the value.
pub(in super::super) fn font_size_field(
    size: f64,
    editing: Option<String>,
    focus: Option<gpui_kit::FocusHandle>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let active = editing.is_some();
    let text = match editing {
        Some(buffer) if buffer.is_empty() => "|".to_string(),
        Some(buffer) => format!("{buffer}|"),
        None => format!("{size:.0}"),
    };
    let mut field = h_flex()
        .id("opt-text-size")
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(if active { color::ACCENT } else { color::BORDER })
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
            app.begin_text_size_field(window, cx);
        }))
        .child(
            div()
                .w(px(SIZE_INPUT_W))
                .flex_none()
                .text_center()
                .font_family(ui::FONT_MONO)
                .text_size(px(TEXT_SM))
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(text),
        )
        .child(dim_span("px")) // i18n-exempt: technical token (unit)
        .child(ml_label(chevron_175()));
    if let Some(handle) = focus {
        field = field.track_focus(&handle).on_key_down(cx.listener(
            |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                app.num_field_key(event, cx);
            },
        ));
    }
    field
}

/// A text bar toggle button with live state and a click handler.
pub(in super::super) fn text_toggle_action(
    id: &'static str,
    glyph: Div,
    active: bool,
    cx: &gpui_kit::Context<crate::app::App>,
    on_click: impl Fn(&mut crate::app::App, &mut gpui_kit::Context<crate::app::App>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .h(px(BTN))
        .w(px(BTN))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(if active { ACCENT_20 } else { FIELD_40 })
        .text_color(if active { color::WHITE } else { ZINC })
        .cursor_pointer()
        .on_click(cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| on_click(app, cx)))
        .child(glyph)
}

/// A text-align icon button with live state and a click handler.
pub(in super::super) fn text_icon_action(
    id: &'static str,
    icon: &'static [u8],
    active: bool,
    cx: &gpui_kit::Context<crate::app::App>,
    on_click: impl Fn(&mut crate::app::App, &mut gpui_kit::Context<crate::app::App>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .h(px(BTN))
        .w(px(BTN))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(if active { ACCENT_20 } else { FIELD_40 })
        .cursor_pointer()
        .on_click(cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| on_click(app, cx)))
        .child(
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

/// The text bar's colour swatch: the selected text layer's colour in edit
/// mode, the paint foreground in draw mode. Clicking opens the picker on the
/// colour it shows.
pub(in super::super) fn text_color_swatch(
    state: &gpui_kit::Entity<AppState>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let shown = {
        let s = state.read(cx);
        s.selected_text_color.unwrap_or(s.fg_color)
    };
    div()
        .id("opt-text-color")
        .test_support()
        .size(px(BTN))
        .flex_shrink_0()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(crate::palette::hsla(shown))
        .shadow(vec![ring_white_20()])
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
            app.open_color_picker(crate::state::ColorTarget::Text, window, cx);
        }))
}
