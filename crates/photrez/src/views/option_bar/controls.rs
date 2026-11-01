use super::*;

// ── Shared parts ───────────────────────────────────────────────────────────────

/// A pill button: an icon plus a label, at the control height.
pub(super) fn tool_pill(icon: &'static [u8], label: String) -> impl IntoElement {
    h_flex()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(ZINC)
        .child(
            Icon::default()
                .data(icon)
                .size(px(ICON_SM))
                .text_color(ZINC),
        )
        .child(div().text_color(color::WHITE).child(label))
}

/// A thin vertical divider between control groups.
pub(super) fn divider() -> impl IntoElement {
    div()
        .h(px(DIV_H))
        .w(px(DIV_W))
        .flex_shrink_0()
        .bg(DIVIDER_LINE)
}

/// `OptionCheckbox` in the checked state — the Move bar's three are all checked.
/// A checkbox drawn from an explicit `checked` flag.
pub(super) fn option_checkbox_action(
    id: &'static str,
    label: &str,
    checked: bool,
    focus: Option<gpui_kit::FocusHandle>,
    cx: &Context<crate::app::App>,
    on_click: impl Fn(&mut crate::app::App, &mut Context<crate::app::App>) + 'static,
) -> impl IntoElement {
    let mut el = div()
        .id(id)
        .test_support()
        .h_flex()
        .h(px(CTRL_H))
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .px(px(FIELD_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(500.))
        .cursor_pointer()
        .hover(|s| s.bg(FIELD_60))
        .on_click(cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| on_click(app, cx)))
        .child(if checked {
            checked_box()
        } else {
            unchecked_box()
        })
        .child(if checked {
            div()
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(label.to_string())
        } else {
            div().text_color(ZINC).child(label.to_string())
        });
    if let Some(handle) = focus {
        el = el
            .track_focus(&handle)
            .focusable()
            .focus(|style| style.border_color(color::ACCENT));
    }
    el
}

/// The checked square: accent fill, 1px shadow, tick at stroke 3.5.
pub(super) fn checked_box() -> Div {
    div()
        .h_flex()
        .size(px(ICON_MD))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(RADIUS_SM))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::ACCENT)
        .shadow(vec![
            BoxShadow::new(px(0.), px(1.), SHADOW_XS).blur_radius(px(2.))
        ])
        .child(
            Icon::default()
                .data(icons::with_stroke(icons::CHECK, STROKE_CHECK))
                .size(px(ICON_SM))
                .text_color(color::WHITE),
        )
}

/// The unchecked square: field fill, no tick.
pub(super) fn unchecked_box() -> Div {
    div()
        .h_flex()
        .size(px(ICON_MD))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(RADIUS_SM))
        .border_1()
        .border_color(color::BORDER)
        .bg(FIELD_80)
}

/// An editable numeric field drawn at rest.
pub(super) fn editable_field(
    id: &'static str,
    label: &str,
    width: f32,
    value: &'static str,
    suffix: &'static str,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .h_flex()
        .h(px(CTRL_H))
        .w(px(width))
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .hover(|s| s.border_color(HOVER_BORDER_LOCAL))
        .when(show_field_labels(), |el| el.child(dim_span(label)))
        .child(div().flex_1().min_w(px(0.)).child(value_text(value)))
        .child(suffix_span(suffix))
}

/// A read-only W/H field: value and suffix packed left.
pub(super) fn num_field(
    label: &str,
    width: f32,
    value: &'static str,
    suffix: &'static str,
) -> impl IntoElement {
    h_flex()
        .h(px(CTRL_H))
        .w(px(width))
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .when(show_field_labels(), |el| el.child(dim_span(label)))
        .child(value_text(value))
        .child(suffix_span(suffix))
}

/// Value run: `font-mono text-[11px] font-semibold text-white`.
pub(super) fn value_text(value: &'static str) -> impl IntoElement {
    div()
        .font_family(ui::FONT_MONO)
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(color::WHITE)
        .child(value)
}

/// Caption run: `text-[10px] font-medium text-[#A1A1AA]`.
pub(super) fn dim_span(text: &str) -> impl IntoElement {
    div()
        .text_size(px(TEXT_XS))
        .font_weight(FontWeight(500.))
        .text_color(ZINC)
        .child(text.to_string())
}

pub(super) fn suffix_span(s: &'static str) -> impl IntoElement {
    dim_span(s)
}

/// The group shown at ≥880 px: divider, align, divider, flip, divider, reset.
pub(super) fn main_controls() -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP))
        .child(divider())
        .child(align_group())
        .child(divider())
        .child(flip_group())
        .child(divider())
        .child(reset_button())
}

pub(super) fn align_group() -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ICON_GAP))
        .when(show_field_labels(), |el| {
            el.child(group_label(&t("properties.align")))
        })
        .child(square_icon_button("Align left", icons::ALIGN_LEFT))
        .child(square_icon_button(
            "Align center horizontal",
            icons::ALIGN_H,
        ))
        .child(square_icon_button("Align right", icons::ALIGN_RIGHT))
        .child(square_icon_button("Align top", icons::ALIGN_TOP))
        .child(square_icon_button("Align center vertical", icons::ALIGN_V))
        .child(square_icon_button("Align bottom", icons::ALIGN_BOTTOM))
}

pub(super) fn flip_group() -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ICON_GAP))
        .when(show_field_labels(), |el| {
            el.child(group_label(&t("properties.flip")))
        })
        .child(square_icon_button("Flip horizontal", icons::FLIP_H))
        .child(square_icon_button("Flip vertical", icons::FLIP_V))
}

pub(super) fn square_icon_button(id: &'static str, icon: &'static [u8]) -> impl IntoElement {
    div()
        .id(id)
        .h_flex()
        .size(px(BTN))
        .items_center()
        .justify_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(FIELD_40)
        .hover(|s| s.bg(color::FIELD).text_color(color::WHITE))
        .child(
            Icon::default()
                .data(icon)
                .size(px(ICON_MD))
                .text_color(ZINC),
        )
}

pub(super) fn group_label(text: &str) -> Div {
    div().mr(px(MR_LABEL)).child(dim_span(text))
}

pub(super) fn reset_button() -> impl IntoElement {
    div()
        .id("opt-reset")
        .h_flex()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(FIELD_40)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .hover(|s| s.bg(color::FIELD).text_color(color::WHITE))
        .text_color(ZINC)
        .child(t("common.reset"))
}

/// The overflow button (`MoreDropdown`) — `size-[24px] rounded-[3px]`.
pub(super) fn more_dropdown_button() -> impl IntoElement {
    div()
        .id("opt-more")
        .h_flex()
        .size(px(BTN))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(RADIUS_SM))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .hover(|s| s.border_color(HOVER_BORDER_LOCAL).text_color(color::WHITE))
        .text_color(ZINC)
        .child(
            Icon::default()
                .data(icons::MORE)
                .size(px(ICON_LG))
                .text_color(ZINC),
        )
}

/// The eraser's full-strength helper (labelled `Hard 100`):
/// hardness, strength and flow all to 100 % and the preset cleared.
pub(super) fn hard100_button(id: &'static str, cx: &Context<crate::app::App>) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(FIELD_40)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(ZINC)
        .cursor_pointer()
        .hover(|s| s.bg(color::FIELD).text_color(color::WHITE))
        .child("Hard 100") // i18n-exempt: fixed control label, not in catalog
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                app.eraser_full_strength(cx);
            }),
        )
}

/// A select dropdown in its closed state.
pub(super) fn select_dropdown(
    icon: Option<&'static [u8]>,
    prefix: Option<&str>,
    value: String,
) -> Div {
    let mut btn = div()
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
        .font_weight(FontWeight(500.));

    if let Some(data) = icon {
        btn = btn.child(
            Icon::default()
                .data(icons::with_stroke(data, STROKE_DROPDOWN))
                .size(px(ICON_MD))
                .text_color(color::ACCENT),
        );
    }

    let label = match prefix {
        Some(p) => h_flex()
            .child(div().text_color(ZINC).child(p.to_string()))
            .child(
                div()
                    .font_weight(FontWeight(600.))
                    .text_color(color::WHITE)
                    .child(value),
            ),
        None => div()
            .font_weight(FontWeight(600.))
            .text_color(color::WHITE)
            .child(value),
    };

    btn.child(label).child(
        Icon::default()
            .data(icons::with_stroke(icons::CHEVRON_DOWN, STROKE_DROPDOWN))
            .size(px(ICON_SM))
            .text_color(ZINC),
    )
}

/// Hover tooltip: the control publishes its id on hover and a
/// deferred panel paints above it; leaving clears the id.
///
/// The id is owned rather than `&'static str` so callers whose rows carry a
/// per-row id (the layer list) can key both the hover state and the element.
pub(crate) fn tooltip(
    state: &gpui_kit::Entity<AppState>,
    id: impl Into<String>,
    text: impl Into<String>,
    shortcut: Option<&'static str>,
    cx: &Context<crate::app::App>,
    child: impl IntoElement,
) -> impl IntoElement {
    let id: String = id.into();
    let shown = state.read(cx).tooltip.as_deref() == Some(id.as_str());
    let hover_id = id.clone();
    let mut wrapper = h_flex()
        .id(format!("{id}-hover"))
        .relative()
        .flex_shrink_0()
        .items_center()
        .on_hover(cx.listener(move |app, hovered: &bool, _window, cx| {
            app.set_tooltip(
                if *hovered {
                    Some(hover_id.clone())
                } else {
                    None
                },
                cx,
            );
        }))
        .child(child);
    if shown {
        let mut panel = div()
            .id(format!("{id}-tooltip"))
            .test_support()
            .absolute()
            .bottom(px(CTRL_H + 6.0))
            .left(px(0.0))
            .flex()
            .items_center()
            .gap(px(GAP))
            .rounded(px(RADIUS))
            .border_1()
            .border_color(color::BORDER)
            .bg(color::TOOLBAR)
            .px(px(PILL_PX))
            .py(px(4.0))
            .text_size(px(TEXT_SM))
            .text_color(color::WHITE)
            .shadow(vec![
                BoxShadow::new(px(0.), px(6.), SHADOW_XS).blur_radius(px(14.))
            ])
            .child(text.into());
        if let Some(shortcut) = shortcut {
            panel = panel.child(dim_span(shortcut));
        }
        wrapper = wrapper.child(gpui_kit::deferred(panel));
    }
    wrapper
}

/// A live dropdown: closed chrome plus a deferred option panel when open.
#[allow(clippy::too_many_arguments)] // flat args mirror the dropdown's option props
pub(super) fn live_select(
    id: &'static str,
    icon: Option<&'static [u8]>,
    prefix: Option<&str>,
    value: String,
    options: &'static [&'static str],
    active: usize,
    open: bool,
    panel_bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let mut trigger = div()
        .id(id)
        .test_support()
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
        .cursor_pointer();
    let label = match prefix {
        Some(p) => h_flex()
            .child(div().text_color(ZINC).child(p.to_string()))
            .child(
                div()
                    .font_weight(FontWeight(600.))
                    .text_color(color::WHITE)
                    .child(value),
            ),
        None => div()
            .font_weight(FontWeight(600.))
            .text_color(color::WHITE)
            .child(value),
    };
    if let Some(data) = icon {
        trigger = trigger.child(
            Icon::default()
                .data(icons::with_stroke(data, STROKE_DROPDOWN))
                .size(px(ICON_MD))
                .text_color(color::ACCENT),
        );
    }
    trigger = trigger.child(label).child(
        Icon::default()
            .data(icons::with_stroke(icons::CHEVRON_DOWN, STROKE_DROPDOWN))
            .size(px(ICON_SM))
            .text_color(ZINC),
    );
    let trigger = trigger.on_click(cx.listener(
        move |app, _: &gpui_kit::ClickEvent, window, cx| {
            app.toggle_select(id, active, options.len(), window, cx);
        },
    ));
    let mut wrapper = div().relative().flex_shrink_0().child(trigger);
    if open {
        let mut panel = div()
            .id(format!("{id}-panel"))
            .absolute()
            .top(px(CTRL_H + 4.0))
            .left(px(0.0))
            .min_w(px(140.0))
            .flex()
            .flex_col()
            .rounded(px(RADIUS))
            .border_1()
            .border_color(color::BORDER)
            .bg(color::TOOLBAR)
            .py(px(4.0))
            .occlude()
            .shadow(vec![
                BoxShadow::new(px(0.), px(6.), SHADOW_XS).blur_radius(px(18.))
            ]);
        for (index, option) in options.iter().enumerate() {
            panel = panel.child(
                div()
                    .id(format!("{id}-option-{index}"))
                    .test_support()
                    .h(px(24.0))
                    .px(px(FIELD_PX))
                    .flex()
                    .items_center()
                    .gap(px(GAP))
                    .text_size(px(TEXT_SM))
                    .text_color(if index == active {
                        color::ACCENT
                    } else {
                        color::TEXT
                    })
                    .cursor_pointer()
                    .hover(|s| s.bg(FIELD_60))
                    .on_click(
                        cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                            app.choose_select(id, index, cx);
                        }),
                    )
                    .child(*option)
                    .when(index == active, |el| {
                        el.child(
                            Icon::default()
                                .data(icons::CHECK)
                                .size(px(ICON_SM))
                                .text_color(color::ACCENT),
                        )
                    }),
            );
        }
        wrapper = wrapper.child(gpui_kit::deferred(SelectPanelSurface::new(
            panel,
            panel_bounds,
        )));
    }
    wrapper
}

/// A 1.75-stroke chevron used by the text bar's custom buttons.
pub(super) fn chevron_175() -> impl IntoElement {
    Icon::default()
        .data(icons::with_stroke(icons::CHEVRON_DOWN, STROKE_PILL))
        .size(px(ICON_SM))
        .text_color(ZINC)
}

/// `ml-1` spacing for the text bar's chevrons.
pub(super) fn ml_label(el: impl IntoElement) -> Div {
    div().ml(px(MR_LABEL)).child(el)
}

/// `shadow-sm` as two layered shadows.
pub(super) fn shadow_sm_layers() -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(1.), hsla(0.0, 0.0, 0.0, 0.10)).blur_radius(px(3.)),
        BoxShadow::new(px(0.), px(1.), hsla(0.0, 0.0, 0.0, 0.10))
            .blur_radius(px(2.))
            .spread_radius(px(-1.)),
    ]
}

/// `ring-1 ring-white/20` as a 1px spread shadow.
pub(super) fn ring_white_20() -> BoxShadow {
    BoxShadow::new(px(0.), px(0.), color::WHITE_20).spread_radius(px(1.))
}

// ── Brush bar ────────────────────────────────────────────────────────────────

/// A brush-bar numeric field. Clicking starts an inline edit; valid keystrokes
/// commit live, Enter ends, Escape restores the value.
#[allow(clippy::too_many_arguments)] // flat args mirror the paint field's many props
pub(super) fn brush_field(
    id: &'static str,
    label: &str,
    input_w: f32,
    value: String,
    suffix: &'static str,
    editing: Option<String>,
    focus: Option<gpui_kit::FocusHandle>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let active = editing.is_some();
    let text = match editing {
        Some(buffer) if buffer.is_empty() => "|".to_string(),
        Some(buffer) => format!("{buffer}|"),
        None => value,
    };
    let mut field = h_flex()
        .id(id)
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(if active { color::ACCENT } else { color::BORDER })
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, window, cx| {
                app.begin_brush_field(id, window, cx);
            }),
        )
        .child(dim_span(label))
        .child(
            div()
                .w(px(input_w))
                .flex_none()
                .font_family(ui::FONT_MONO)
                .text_size(px(TEXT_SM))
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(text),
        )
        .child(dim_span(suffix));
    if let Some(handle) = focus {
        field = field.track_focus(&handle).on_key_down(cx.listener(
            |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                app.num_field_key(event, cx);
            },
        ));
    }
    field
}

/// `opacity-40` wrapper for the selection bar's disabled fields.
pub(super) fn dimmed(el: impl IntoElement) -> Div {
    div().opacity(0.4).child(el)
}

/// `<div class="flex shrink-0 items-center gap-1">` around paired fields.
pub(super) fn field_pair(first: impl IntoElement, second: impl IntoElement) -> Div {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ITEM_GAP))
        .child(first)
        .child(second)
}

// ── Paint Bucket ───────────────────────────────────────────────────────────────
