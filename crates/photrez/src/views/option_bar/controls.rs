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
            .focus_visible(|mut style| {
                style.box_shadow = Some(vec![ui::focus_band(color::ACCENT.opacity(0.7))]);
                style
            });
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

/// The unchecked square: field fill, recessed by the same inset well the
/// reference gives it, no tick.
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
        .shadow(vec![ui::inset_well()])
}

/// An editable numeric field drawn at rest.
pub(super) fn editable_field(
    id: &'static str,
    label: &str,
    width: f32,
    value: &'static str,
    suffix: &'static str,
) -> impl IntoElement {
    editable_field_text(id, label, width, value.to_string(), suffix)
}

/// The inert field shell behind [`editable_field`]: same box, live value text.
pub(super) fn editable_field_text(
    id: &'static str,
    label: &str,
    width: f32,
    value: String,
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
        // No hover edge: the reference's unlayered `* { border-color }` rule beats its own
        // `hover:border-*` utility, so these fields keep their rest border under the pointer.
        .when(show_field_labels(), |el| el.child(dim_span(label)))
        .child(div().flex_1().min_w(px(0.)).child(value_text(value)))
        .child(suffix_span(suffix))
}

/// A read-only W/H field: value and suffix packed left.
pub(super) fn num_field(
    label: &str,
    width: f32,
    value: String,
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

/// Value run: 11px monospace, semibold, white.
pub(super) fn value_text(value: String) -> impl IntoElement {
    div()
        .font_family(ui::FONT_MONO)
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(color::WHITE)
        .child(value)
}

/// Caption run: 10px medium label in `#A1A1AA`.
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

/// The group shown at >=880 px: divider, align, divider, flip, divider, reset.
/// A locked selection takes the reference's inert treatment: the groups dim
/// and the handlers refuse the edit.
pub(super) fn main_controls(locked: bool, cx: &Context<crate::app::App>) -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP))
        .child(divider())
        .child(align_group(locked, cx))
        .child(divider())
        .child(flip_group(locked, cx))
        .child(divider())
        .child(reset_button(locked, cx))
}

pub(super) fn align_group(locked: bool, cx: &Context<crate::app::App>) -> impl IntoElement {
    let mut group = h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ICON_GAP))
        .when(show_field_labels(), |el| {
            el.child(group_label(&t("properties.align")))
        })
        .child(square_icon_button(
            "Align left",
            icons::ALIGN_LEFT,
            cx,
            align_clicked(0),
        ))
        .child(square_icon_button(
            "Align center horizontal",
            icons::ALIGN_H,
            cx,
            align_clicked(1),
        ))
        .child(square_icon_button(
            "Align right",
            icons::ALIGN_RIGHT,
            cx,
            align_clicked(2),
        ))
        .child(square_icon_button(
            "Align top",
            icons::ALIGN_TOP,
            cx,
            align_clicked(3),
        ))
        .child(square_icon_button(
            "Align center vertical",
            icons::ALIGN_V,
            cx,
            align_clicked(4),
        ))
        .child(square_icon_button(
            "Align bottom",
            icons::ALIGN_BOTTOM,
            cx,
            align_clicked(5),
        ));
    if locked {
        group = group.opacity(0.3);
    }
    group
}

pub(super) fn flip_group(locked: bool, cx: &Context<crate::app::App>) -> impl IntoElement {
    let mut group = h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ICON_GAP))
        .when(show_field_labels(), |el| {
            el.child(group_label(&t("properties.flip")))
        })
        .child(square_icon_button(
            "Flip horizontal",
            icons::FLIP_H,
            cx,
            flip_clicked(true),
        ))
        .child(square_icon_button(
            "Flip vertical",
            icons::FLIP_V,
            cx,
            flip_clicked(false),
        ));
    if locked {
        group = group.opacity(0.3);
    }
    group
}

/// A 24px icon button with a live click handler — the Move bar's inline
/// align/flip group.
pub(super) fn square_icon_button(
    id: &'static str,
    icon: &'static [u8],
    cx: &Context<crate::app::App>,
    on_click: impl Fn(&mut crate::app::App, &mut Context<crate::app::App>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .h_flex()
        .size(px(BTN))
        .items_center()
        .justify_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(FIELD_40)
        .cursor_pointer()
        .hover(|s| s.bg(color::FIELD).text_color(color::WHITE))
        .on_click(cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| on_click(app, cx)))
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

/// The inline Reset button: locked layers keep the reference's disabled look
/// (no border tint, no pointer affordance).
pub(super) fn reset_button(locked: bool, cx: &Context<crate::app::App>) -> impl IntoElement {
    let mut button = div()
        .id("opt-reset")
        .test_support()
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
        .text_color(ZINC)
        .child(t("common.reset"));
    if locked {
        button = button.opacity(0.3);
    } else {
        button = button
            .cursor_pointer()
            .hover(|s| s.bg(color::FIELD).text_color(color::WHITE))
            .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                reset_transform_clicked(app, cx)
            }));
    }
    button
}

/// The `handleAlign` click body: a locked layer is inert (the reference's
/// `isLocked()` guard), the command writes nothing else, and a failed edit
/// surfaces as a status error instead of a silent no-op.
pub(super) fn align_clicked(
    mode: u8,
) -> impl Fn(&mut crate::app::App, &mut Context<crate::app::App>) {
    move |app, cx| {
        if selected_core_layer_is_locked(app, cx) {
            return;
        }
        if let Err(error) = app.align_selected_layers(mode, cx) {
            app.set_status_error(Some(format!("Align: {error}")), cx);
        }
    }
}

/// The `handleFlip` click body, same lock guard as the reference.
pub(super) fn flip_clicked(
    horizontal: bool,
) -> impl Fn(&mut crate::app::App, &mut Context<crate::app::App>) {
    move |app, cx| {
        if selected_core_layer_is_locked(app, cx) {
            return;
        }
        if let Err(error) = app.flip_selected_layer(horizontal, cx) {
            app.set_status_error(Some(format!("Flip: {error}")), cx);
        }
    }
}

/// The `handleResetTransform` click body shared by the inline button and the
/// overflow panel row.
pub(super) fn reset_transform_clicked(
    app: &mut crate::app::App,
    cx: &mut Context<crate::app::App>,
) {
    if selected_core_layer_is_locked(app, cx) {
        return;
    }
    if let Err(error) = app.reset_selected_transform(cx) {
        app.set_status_error(Some(format!("Transform: {error}")), cx);
    }
}

/// The selected layer's core lock flag, read the same way the reference's
/// `isLocked()` reads it from the engine before any transform click.
fn selected_core_layer_is_locked(app: &crate::app::App, cx: &Context<crate::app::App>) -> bool {
    app.selected_layer_for_cursor(cx)
        .map(|(_, layer)| layer.locked)
        .unwrap_or(false)
}

/// The overflow button (`MoreDropdown`) — 24px square, 3px corner radius.
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
        // The reference's only hover change here is the glyph brightening to white; its border
        // stays grey for the same reason as the fields above.
        .hover(|s| s.text_color(color::WHITE))
        .text_color(ZINC)
        .child(
            Icon::default()
                .data(icons::MORE)
                .size(px(ICON_LG))
                .text_color(ZINC),
        )
}

/// The field background at 30% opacity — the tint of the reference's
/// overflow group rows (`bg-editor-field/30`); it composites to the census
/// pixel over [`color::BAR_POPOVER`].
const FIELD_30: Hsla = Hsla {
    h: color::FIELD.h,
    s: color::FIELD.s,
    l: color::FIELD.l,
    a: 0.30,
};

/// The shared `MoreDropdown`: a trigger that toggles the bar-wide overflow
/// state and, while open, the panel painted under it.
///
/// The panel goes through `gpui_kit::deferred` + [`SelectPanelSurface`], the
/// shape every working option-bar dropdown uses: the deferred pass paints it
/// after the canvas, so its `occlude()` removes the canvas hitbox from the
/// dispatch path and a row press cannot also start a canvas gesture; and the
/// surface publishes the panel rect, which is what tells a click-away from an
/// inside-panel click. Anchored to the bar's right edge, a folded trigger near
/// the column edge keeps the whole panel inside the clipped canvas column.
pub(super) fn more_dropdown_overflow(
    open: bool,
    state: &gpui_kit::Entity<AppState>,
    cx: &Context<crate::app::App>,
    panel_body: impl IntoElement,
) -> Div {
    let trigger =
        div()
            .id("opt-more")
            .test_support()
            .flex_shrink_0()
            .cursor_pointer()
            .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
                app.toggle_more_menu(window, cx)
            }))
            .child(more_dropdown_button());
    let mut wrapper = div().relative().flex_shrink_0().child(trigger);
    if open {
        let panel = div()
            .id("opt-more-panel")
            .test_support()
            .absolute()
            .top(px(CTRL_H + 4.0))
            .right(px(0.0))
            .min_w(px(150.0))
            .flex()
            .flex_col()
            .gap(px(PILL_PX))
            .rounded(px(6.0))
            .border_1()
            .border_color(color::BORDER)
            .bg(color::BAR_POPOVER)
            .p(px(PILL_PX))
            .occlude()
            .shadow(vec![
                BoxShadow::new(px(0.), px(6.), SHADOW_XS).blur_radius(px(18.))
            ])
            .child(panel_body);
        wrapper = wrapper.child(gpui_kit::deferred(SelectPanelSurface::new(
            panel,
            state.read(cx).more_panel_bounds.clone(),
        )));
    }
    wrapper
}

/// The overflow panel's 10px bold uppercase group caption.
pub(super) fn overflow_caption(label: &str) -> Div {
    div()
        .text_size(px(TEXT_XS))
        .font_weight(FontWeight(700.))
        .text_color(color::TEXT_DIM)
        .child(label.to_uppercase())
}

/// One overflow group row: the reference's `bg-editor-field/30` container
/// with a field border around the icon buttons.
pub(super) fn overflow_group_row() -> Div {
    h_flex()
        .items_center()
        .gap(px(PILL_PX))
        .p(px(FIELD_PX))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(FIELD_30)
}

/// A borderless icon button inside an overflow group row (16px icon, 3px
/// radius, 2px padding — the reference's `p-0.5` treatment). Disabled rows
/// keep the reference's `pointer-events-none`: no cursor, no click.
pub(super) fn overflow_icon_button(
    id: &'static str,
    icon: &'static [u8],
    enabled: bool,
    cx: &Context<crate::app::App>,
    on_click: impl Fn(&mut crate::app::App, &mut Context<crate::app::App>) + 'static,
) -> impl IntoElement {
    let mut button = div()
        .id(id)
        .test_support()
        .h_flex()
        .p(px(2.0))
        .items_center()
        .justify_center()
        .rounded(px(RADIUS_SM))
        .text_color(ZINC)
        .child(
            Icon::default()
                .data(icons::with_stroke(icon, STROKE_ICON))
                .size(px(ICON_LG))
                .text_color(ZINC),
        );
    if enabled {
        button = button
            .cursor_pointer()
            .hover(|s| s.text_color(color::WHITE))
            .on_click(
                cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| on_click(app, cx)),
            );
    }
    button
}

/// A labelled action button inside the overflow panel (24px tall, 3px
/// radius, field border and fill). `enabled` is the reference gate: a
/// missing selection or a locked layer leaves the row inert and dimmed.
pub(super) fn overflow_action_button(
    id: &'static str,
    label: String,
    icon: Option<&'static [u8]>,
    enabled: bool,
    full_width: bool,
    cx: &Context<crate::app::App>,
    on_click: impl Fn(&mut crate::app::App, &mut Context<crate::app::App>) + 'static,
) -> impl IntoElement {
    let mut button = div()
        .id(id)
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS_SM))
        .px(px(FIELD_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(500.))
        .text_color(if enabled { color::TEXT } else { ZINC })
        .when(full_width, |el| el.w_full());
    if enabled {
        button = button
            .border_1()
            .border_color(color::BORDER)
            .bg(color::FIELD)
            .cursor_pointer()
            .hover(|s| s.bg(FIELD_80))
            .on_click(
                cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| on_click(app, cx)),
            );
    } else {
        button = button.opacity(0.3);
    }
    if let Some(data) = icon {
        button = button.child(
            Icon::default()
                .data(icons::with_stroke(data, STROKE_ICON))
                .size(px(ICON_SM))
                .text_color(if enabled { color::TEXT } else { ZINC }),
        );
    }
    button.child(div().child(label))
}

/// The overflow panel's 1px section divider (`h-px bg-editor-divider my-1`).
pub(super) fn overflow_divider() -> Div {
    div().w_full().h(px(1.0)).my(px(4.0)).bg(color::DIVIDER)
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
            .bg(color::MENU_LIST)
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

/// 4px of left spacing for the text bar's chevrons.
pub(super) fn ml_label(el: impl IntoElement) -> Div {
    div().ml(px(MR_LABEL)).child(el)
}

/// A subtle drop shadow as two layered shadows.
pub(super) fn shadow_sm_layers() -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(1.), hsla(0.0, 0.0, 0.0, 0.10)).blur_radius(px(3.)),
        BoxShadow::new(px(0.), px(1.), hsla(0.0, 0.0, 0.0, 0.10))
            .blur_radius(px(2.))
            .spread_radius(px(-1.)),
    ]
}

/// A 1px white ring at 20% opacity, as a spread shadow.
pub(super) fn ring_white_20() -> BoxShadow {
    BoxShadow::new(px(0.), px(0.), color::WHITE_20).spread_radius(px(1.))
}

// ── Brush bar ────────────────────────────────────────────────────────────────

/// A brush-bar numeric field. Clicking starts an inline edit; valid keystrokes
/// commit live, Enter ends, Escape restores the value.
///
/// The pinned reference source asks for an accent focus outline, but its
/// compiled cascade and pointer-focus behavior have not been verified.
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
        .border_color(color::BORDER)
        .shadow(ui::outline_cue(active))
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

/// A 40%-opacity wrapper for the selection bar's disabled fields.
pub(super) fn dimmed(el: impl IntoElement) -> Div {
    div().opacity(0.4).child(el)
}

/// One centred, never-squeezed row with a 4px gap, around paired fields.
pub(super) fn field_pair(first: impl IntoElement, second: impl IntoElement) -> Div {
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ITEM_GAP))
        .child(first)
        .child(second)
}

// ── Paint Bucket ───────────────────────────────────────────────────────────────
