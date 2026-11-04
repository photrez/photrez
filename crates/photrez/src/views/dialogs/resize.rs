//! The Resize Canvas dialog: the description, the unit-aware fields, their
//! unit select and the aspect-lock pill between them.

use super::*;

/// Resize Canvas body: the description, the
/// two unit-aware fields with their own unit select, and the aspect lock
/// between them.
pub(super) fn resize_canvas_body(
    state: &Entity<AppState>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> Div {
    let (width, height, field, unit, locked, unit_open, width_focus, height_focus) = {
        let read = state.read(cx);
        match read.resize_canvas.as_ref() {
            Some(session) => (
                session.width.clone(),
                session.height.clone(),
                session.field,
                session.unit,
                session.aspect_locked,
                read.resize_unit_open,
                read.resize_field_focus(0),
                read.resize_field_focus(1),
            ),
            None => (
                String::new(),
                String::new(),
                0,
                DimensionUnit::Px,
                true,
                false,
                None,
                None,
            ),
        }
    };
    v_flex()
        .gap(px(10.0))
        .w_full()
        .child(
            div()
                .mb(px(4.0))
                .text_size(px(11.0))
                .text_color(color::TEXT_DIM)
                .child(t("dialogs.resizeCanvas.description").to_string()),
        )
        .child(resize_field(
            "resize-width",
            &t("dialogs.resizeCanvas.width"),
            &width,
            field == 0,
            0,
            unit,
            unit_open,
            &width_focus,
            cx,
        ))
        .child(resize_aspect_lock(locked, cx))
        .child(resize_field(
            "resize-height",
            &t("dialogs.resizeCanvas.height"),
            &height,
            field == 1,
            1,
            unit,
            false,
            &height_focus,
            cx,
        ))
}

#[allow(clippy::too_many_arguments)] // flat args mirror the field's props
fn resize_field(
    id: &'static str,
    label: &str,
    value: &str,
    active: bool,
    field: u8,
    unit: DimensionUnit,
    list_open: bool,
    focus: &Option<gpui_kit::FocusHandle>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> Div {
    let unit_index = DimensionUnit::ALL
        .iter()
        .position(|candidate| *candidate == unit)
        .unwrap_or(0);
    let trigger_id = if field == 0 {
        "resize-unit"
    } else {
        "resize-unit-h"
    };
    let trigger = div()
        .id(trigger_id)
        .test_support()
        .h(px(28.0))
        .w(px(60.0))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .shadow(vec![ui::field_well()])
        .text_size(px(11.0))
        .text_color(color::TEXT)
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            app.toggle_resize_unit(cx);
        }))
        .child(unit.label())
        .child(
            Icon::default()
                .data(icons::CHEVRON_DOWN)
                .size(px(9.0))
                .text_color(color::TEXT_DIM),
        );
    let row = h_flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .w(px(52.0))
                .text_size(px(11.0))
                .font_weight(FontWeight(500.))
                .text_color(color::TEXT_DIM)
                .child(label.to_string()),
        )
        .child(chrome::tabbable_field(
            div()
                .id(id)
                .test_support()
                .h(px(28.0))
                .flex_1()
                .min_w(px(0.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(6.0))
                .border_1()
                .border_color(if active {
                    color::ACCENT
                } else {
                    color::FIELD_BORDER
                })
                .bg(color::FIELD)
                .shadow(vec![ui::field_well()])
                .text_size(px(11.5))
                .text_color(color::TEXT)
                .cursor_pointer()
                .aria_label(label.to_string())
                .on_click(
                    cx.listener(move |app, _: &gpui_kit::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        app.focus_resize_field(field, window, cx);
                    }),
                )
                .child(format!("{value}|")),
            focus,
        ))
        .child(trigger);
    if !(list_open && field == 0) {
        return row;
    }
    // The list expands inline: a popup deferred inside the dialog overlay
    // never receives hits (measured on this control).
    let mut list = v_flex()
        .w(px(60.0))
        .flex()
        .flex_col()
        .rounded(px(6.0))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::TOOLBAR)
        .py(px(4.0));
    for (index, candidate) in DimensionUnit::ALL.iter().enumerate() {
        list = list.child(
            div()
                .id(format!("resize-unit-{}", candidate.label()))
                .test_support()
                .h(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(11.0))
                .text_color(if index == unit_index {
                    color::ACCENT
                } else {
                    color::TEXT
                })
                .cursor_pointer()
                .on_click(
                    cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        app.choose_select("resize-unit", index, cx);
                    }),
                )
                .child(candidate.label()),
        );
    }
    v_flex()
        .gap(px(4.0))
        .child(row)
        .child(h_flex().justify_end().child(list))
}

/// The `Keep proportions` pill's colours for one lock state, kept out of the
/// view builder so the tiers can be asserted without a window.
///
/// The locked pill asks for an accent edge at 40%, the unlocked one for a
/// transparent edge, but neither ever paints: one app-wide border colour
/// outranks the per-element border colour a component asks for, so the pill's
/// 1px edge is [`color::BORDER`] in both states. The capture of the locked
/// dialog measures rgb(51,51,51) on that row, which is what makes this visible
/// rather than a reading of the rules.
///
/// Fills keep their own token, so the locked accent/10 wash does paint and the
/// unlocked pill really is unfilled. The 1px border stays in both states, so
/// toggling the lock never reflows the pill; only its fill and text move.
pub(super) struct ResizePillColours {
    pub(super) edge: Hsla,
    pub(super) fill: Hsla,
    pub(super) text: Hsla,
}

pub(super) fn resize_pill_colours(locked: bool) -> ResizePillColours {
    let accent_at = |alpha: f32| Hsla {
        h: color::ACCENT.h,
        s: color::ACCENT.s,
        l: color::ACCENT.l,
        a: alpha,
    };
    if locked {
        ResizePillColours {
            edge: color::BORDER,
            fill: accent_at(0.10),
            text: color::ACCENT,
        }
    } else {
        ResizePillColours {
            edge: color::BORDER,
            fill: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 0.0,
            },
            text: color::TEXT_DIM,
        }
    }
}

/// `Keep proportions`: an accent pill while
/// the aspect lock is on, a dim row while it is off.
fn resize_aspect_lock(locked: bool, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    let colours = resize_pill_colours(locked);
    h_flex().h(px(24.0)).pl(px(24.0)).child(
        div()
            .id("resize-aspect")
            .test_support()
            .h(px(24.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .rounded(px(6.0))
            .border_1()
            .border_color(colours.edge)
            .bg(colours.fill)
            .when(locked, |el| el.shadow(vec![ui::accent_well()]))
            .text_size(px(11.0))
            .text_color(colours.text)
            .gap(px(4.0))
            .cursor_pointer()
            .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.toggle_resize_aspect_lock(cx);
            }))
            .child(
                Icon::default()
                    .data(if locked { icons::LINK } else { icons::UNLINK })
                    .size(px(12.0))
                    .text_color(if locked {
                        color::ACCENT
                    } else {
                        color::TEXT_DIM
                    }),
            )
            .child(t("dialogs.resizeCanvas.keepProportions").to_string()),
    )
}
