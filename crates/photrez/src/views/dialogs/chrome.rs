use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{
    div, hsla, px, BoxShadow, Div, FocusHandle, FontWeight, Hsla, Stateful, TestSupportExt,
};

use crate::components::ui;
use crate::icons;
use crate::tokens::color;

use super::{
    ACTIONS_H, BAR_PX, BODY_PX, BODY_PY, BRAND, BTN_H, BTN_MIN_W, BTN_R, BTN_TEXT, CLOSE_BOX,
    CLOSE_ICON, FOOTER_FILL, GAP_2, SHELL_R, TITLEBAR_H, TITLE_TEXT,
};
/// The dialog shell: a rounded 1px-edged PANEL box with a 36px TOPBAR
/// titlebar, a body, and a translucent TOPBAR footer.
///
/// Height is fixed rather than derived from the body so that a stubbed body
/// still occupies the same space the real one will: a dialog whose footer moves
/// when its body is filled in would make every before/after comparison lie.
pub(super) fn dialog_shell(title: &str, w: f32, h: f32, body: Div) -> Div {
    dialog_shell_with_actions(title, w, h, body, Vec::new())
}

/// Content-sized shell with live footer buttons and a live titlebar close.
pub(super) fn dialog_shell_auto_actions(
    title: &str,
    w: f32,
    body: Div,
    buttons: Vec<gpui_kit::AnyElement>,
    close: impl IntoElement,
) -> Div {
    v_flex()
        .w(px(w))
        .rounded(px(SHELL_R))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::PANEL)
        .overflow_hidden()
        .shadow(vec![BoxShadow::new(
            px(0.0),
            px(18.0),
            hsla(0.0, 0.0, 0.0, 0.55),
        )
        .blur_radius(px(50.0))])
        .child(titlebar_with(title, close))
        .child(div().w_full().px(px(BODY_PX)).py(px(BODY_PY)).child(body))
        .child(actions(buttons))
}

/// The shell with live footer buttons for a production dialog.
pub(super) fn dialog_shell_with_actions(
    title: &str,
    w: f32,
    h: f32,
    body: Div,
    buttons: Vec<gpui_kit::AnyElement>,
) -> Div {
    let inner_h = h - TITLEBAR_H - ACTIONS_H;
    v_flex()
        .w(px(w))
        .h(px(h))
        .rounded(px(SHELL_R))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::PANEL)
        .overflow_hidden()
        .shadow(vec![BoxShadow::new(
            px(0.0),
            px(18.0),
            hsla(0.0, 0.0, 0.0, 0.55),
        )
        .blur_radius(px(50.0))])
        .child(titlebar(title))
        .child(
            div()
                .w_full()
                .h(px(inner_h))
                .overflow_hidden()
                .px(px(BODY_PX))
                .py(px(BODY_PY))
                .child(body),
        )
        .child(actions(buttons))
}

/// 36px tall, centred on one row with the ends pushed apart, 1px bottom
/// DIVIDER border, TOPBAR fill and 12px side inset \u{2014} brand mark, title,
/// close button.
pub(super) fn titlebar(title: &str) -> Div {
    titlebar_with(title, close_box())
}

/// The titlebar with a caller-supplied close button. Static previews use the
/// inert affordance; a live dialog passes `dialog_close_button` so the same
/// 20x20 hit area dismisses it.
fn titlebar_with(title: &str, close: impl IntoElement) -> Div {
    h_flex()
        .w_full()
        .h(px(TITLEBAR_H))
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .px(px(BAR_PX))
        .border_b_1()
        .border_color(color::DIVIDER)
        .bg(color::TOPBAR)
        .child(
            h_flex()
                .items_center()
                .gap(px(GAP_2))
                .min_w(px(0.0))
                // This chrome draws a flat accent slot rather than the brand
                // mark; `title_bar.rs` owns the importable LOGO. The mark needs
                // no mask or filter, so a rasteriser cannot refuse it.
                .child(
                    div()
                        .w(px(BRAND))
                        .h(px(BRAND))
                        .flex_shrink_0()
                        .rounded(px(4.0))
                        .bg(color::ACCENT),
                )
                .child(
                    div()
                        .text_size(px(TITLE_TEXT))
                        .font_family(ui::FONT_UI)
                        .font_weight(FontWeight(500.))
                        .text_color(color::TEXT)
                        .child(title.to_string()),
                ),
        )
        .child(close)
}

/// The close button: a centred 20px square, 4px corners, icon colour, with an
/// 8%-white hover fill and full-brightness text. The 20 px box is the whole
/// hit area, not just the 10 px glyph.
///
/// Inert by itself so a static preview draws the same chrome without a
/// handler; `dialog_close_button` adds the dismissal.
fn close_box() -> Stateful<Div> {
    div()
        .id("dialog-close")
        .w(px(CLOSE_BOX))
        .h(px(CLOSE_BOX))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .rounded(px(4.0))
        .cursor_pointer()
        // No token holds 8 % white; the alpha blends over TOPBAR as the CSS does.
        .hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.08)))
        .child(
            Icon::default()
                .data(icons::X)
                .size(px(CLOSE_ICON))
                .text_color(color::ICON),
        )
}

/// The titlebar close for a live dialog: it runs the dialog's
/// dismissal callback, which is the same path Escape and the backdrop take.
pub(super) fn dialog_close_button(
    listener: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
) -> impl IntoElement {
    close_box().test_support().on_click(listener)
}

/// Centred on one row, buttons pushed right, 45px tall including the 1px top
/// DIVIDER border, TOPBAR fill at 65%, 8px gap, 12px side and 8px vertical
/// inset \u{2014} the footer, and the buttons in it.
fn actions(buttons: Vec<gpui_kit::AnyElement>) -> Div {
    h_flex()
        .w_full()
        .min_h(px(ACTIONS_H))
        .flex_shrink_0()
        .items_center()
        .justify_end()
        .gap(px(GAP_2))
        .px(px(BAR_PX))
        .py(px(8.0))
        .border_t_1()
        .border_color(color::DIVIDER)
        .bg(FOOTER_FILL)
        .children(buttons)
}

/// A dialog footer button: 28px tall, 64px min-width, 6px radius,
/// 1px border, 12px side inset, 11px medium text.
///
/// `primary` is an accent border and fill with black text at 90%; the
/// black text is the one place a dialog inverts the palette, and it is why
/// `TEXT_BLACK` is spelled as a literal rather than borrowed from a token.
pub(super) fn dialog_button(label: &str, primary: bool) -> Div {
    div()
        .h(px(BTN_H))
        .min_w(px(BTN_MIN_W))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(BTN_R))
        .border_1()
        .border_color(if primary {
            color::ACCENT
        } else {
            color::FIELD_BORDER
        })
        .bg(if primary { color::ACCENT } else { color::FIELD })
        .px(px(BAR_PX))
        .child(
            div()
                .text_size(px(BTN_TEXT))
                .font_family(ui::FONT_UI)
                .font_weight(if primary {
                    FontWeight(600.)
                } else {
                    FontWeight(500.)
                })
                .text_color(if primary {
                    Hsla {
                        h: 0.0,
                        s: 0.0,
                        l: 0.0,
                        a: 0.9,
                    }
                } else {
                    color::TEXT
                })
                .child(label.to_string()),
        )
}

/// A footer button with an id and click handler for a production prompt.
pub(super) fn dialog_action_button(
    label: &str,
    primary: bool,
    id: &'static str,
    listener: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
) -> impl IntoElement {
    dialog_button(label, primary)
        .id(id)
        .test_support()
        .on_click(listener)
}

/// Makes a dialog text field a real tab stop.
///
/// The handle carries the flag, so `Tab` lands on the field itself and the app
/// reads focus instead of cycling a session index. The band while the field has
/// keyboard focus is the shared inset one the slider tracks use, not a new
/// treatment. A missing handle leaves the field as it was, which is how a
/// `--shot` view with no App construction stays safe.
pub(super) fn tabbable_field<E>(field: E, focus: &Option<FocusHandle>) -> E
where
    E: InteractiveElement + StatefulInteractiveElement,
{
    let Some(handle) = focus.as_ref() else {
        return field;
    };
    ui::tabbable_focus(field, handle)
}
