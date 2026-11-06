use super::*;

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
        // The 0.1 build's only hover change here is the glyph brightening to white; its border
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

/// The field background at 30% opacity: the tint of the 0.1 build's
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

/// One overflow group row: the 0.1 build's `bg-editor-field/30` container
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
/// radius, 2px padding, the 0.1 build's `p-0.5` treatment). Disabled rows
/// keep the 0.1 build's `pointer-events-none`: no cursor, no click.
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
/// radius, field border and fill). `enabled` is the 0.1 gate: a
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
