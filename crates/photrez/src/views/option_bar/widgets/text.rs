use super::*;

/// The box-mode list's fixed-box label.
const BOX_MODE_AREA: &str = "Fixed Box"; // i18n-exempt: not in catalog

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
        .border_color(color::BORDER)
        .shadow(ui::ring_cue(active))
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
        field = ui::tabbable_focus(field, &handle).on_key_down(cx.listener(
            |app, event: &gpui_kit::KeyDownEvent, window, cx| {
                app.num_field_key_in_row(
                    "opt-text-size",
                    event,
                    window,
                    move |app, window, cx| {
                        app.begin_text_size_field(window, cx);
                    },
                    cx,
                );
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

/// The text bar's box-mode control: the trigger plus the 0.1 build's
/// two-item list. Opening routes through the bar's
/// single-open select slot, so Escape and an outside click dismiss it like
/// the other dropdowns. The current mode is read from the live text-edit
/// session; with no session the resting point mode is shown, matching the
/// reference fallback.
pub(in super::super) fn box_mode_control(
    state: &gpui_kit::Entity<AppState>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    use photrez_core::document::layer_params::TextBoxMode;
    let (open, area_active) = {
        let s = state.read(cx);
        (
            s.open_select == Some("opt-box-mode"),
            s.text_edit
                .as_ref()
                .is_some_and(|session| session.data.box_mode == TextBoxMode::Area),
        )
    };
    let label = if area_active { BOX_MODE_AREA } else { BOX_MODE };
    let trigger = div()
        .id("opt-box-mode")
        .test_support()
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
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, window, cx| {
                app.toggle_select("opt-box-mode", usize::from(area_active), 2, window, cx);
            }),
        )
        .child(div().text_color(color::WHITE).child(label))
        .child(chevron_175());
    let mut wrapper = div().relative().flex_shrink_0().child(trigger);
    if open {
        wrapper = wrapper.child(gpui_kit::deferred(SelectPanelSurface::new(
            box_mode_list(area_active, state, cx),
            state.read(cx).select_panel_bounds.clone(),
        )));
    }
    wrapper
}

/// The list itself: the 0.1 build's `#1D2026` popover with Auto Width and
/// Fixed Box; the current mode carries the accent tint and check.
fn box_mode_list(
    area_active: bool,
    state: &gpui_kit::Entity<AppState>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    gpui_kit::base::v_flex()
        .id("opt-box-mode-panel")
        .test_support()
        .absolute()
        .top(px(CTRL_H + 6.0))
        .left(px(0.0))
        .w(px(112.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::BAR_POPOVER)
        .py(px(4.0))
        .occlude()
        .shadow(vec![
            BoxShadow::new(px(0.), px(6.), SHADOW_XS).blur_radius(px(18.))
        ])
        .child(box_mode_row(
            "opt-box-mode-point",
            BOX_MODE,
            !area_active,
            false,
            state,
            cx,
        ))
        .child(box_mode_row(
            "opt-box-mode-area",
            BOX_MODE_AREA,
            area_active,
            true,
            state,
            cx,
        ))
}

/// One box-mode row. The click mirrors `setBoxMode`: the selected text
/// layer and the live edit session both take the mode, then the list closes.
/// With neither present the 0.1 build writes nothing, and neither does this.
fn box_mode_row(
    id: &'static str,
    label: &'static str,
    active: bool,
    area: bool,
    state: &gpui_kit::Entity<AppState>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let state = state.clone();
    div()
        .id(id)
        .test_support()
        .w_full()
        .h_flex()
        .items_center()
        .justify_between()
        .px(px(10.0))
        .py(px(6.0))
        .text_size(px(TEXT_SM))
        .text_color(if active { color::WHITE } else { ZINC })
        .font_weight(if active {
            FontWeight(600.)
        } else {
            FontWeight(400.)
        })
        .when(active, |el| el.bg(ACCENT_20))
        .cursor_pointer()
        .hover(|s| s.bg(color::WHITE_10))
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                // Mirror `applyEdit({ boxMode, ... })`: a selected text layer
                // rewrites through core history.
                let _ = app.edit_selected_text(cx, |data| apply_box_mode(data, area));
                // Mirror `setTextEditSession`: the live editor takes the same
                // patch while a text edit is open.
                state.update(cx, |state, _| {
                    if let Some(session) = state.text_edit.as_mut() {
                        apply_box_mode(&mut session.data, area);
                    }
                });
                app.close_select(cx);
            }),
        )
        .child(div().child(label))
        .when(active, |el| {
            el.child(
                Icon::default()
                    .data(icons::CHECK)
                    .size(px(ICON_SM))
                    .text_color(color::ACCENT),
            )
        })
}

/// The 0.1 build's `setBoxMode` value patch: point mode clears the box
/// dimensions; area mode keeps a positive width and seeds the 200 px the
/// reference uses for a layer that never had one.
fn apply_box_mode(data: &mut photrez_core::document::layer_params::TextData, area: bool) {
    use photrez_core::document::layer_params::TextBoxMode;
    if area {
        data.box_mode = TextBoxMode::Area;
        if data.box_width <= 0.0 {
            data.box_width = 200.0;
        }
    } else {
        data.box_mode = TextBoxMode::Point;
        data.box_width = 0.0;
        data.box_height = 0.0;
    }
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
