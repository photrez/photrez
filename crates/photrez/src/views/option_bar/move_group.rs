use super::*;

/// The group shown at >=880 px: divider, align, divider, flip, divider, reset.
/// A locked selection takes the inert treatment: the groups dim
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

/// The inline Reset button: locked layers keep the disabled look
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
            .hover(|s| s.bg(color::FIELD).text_color(color::WHITE))
            .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                reset_transform_clicked(app, cx)
            }));
    }
    button
}

/// The align click body: a locked layer is inert (the lock guard
/// below), the command writes nothing else, and a failed edit
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

/// The flip click body, with the same lock guard as the align body.
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

/// The reset-transform click body shared by the inline button and the
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

/// The selected layer's core lock flag, read from the engine before any
/// transform click.
fn selected_core_layer_is_locked(app: &crate::app::App, cx: &Context<crate::app::App>) -> bool {
    app.selected_layer_for_cursor(cx)
        .map(|(_, layer)| layer.locked)
        .unwrap_or(false)
}
