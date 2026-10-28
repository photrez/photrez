use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, BoxShadow, Div, FontWeight, Hsla, TestSupportExt};

use crate::components::ui;
use crate::icons;
use crate::tokens::color;

use super::{
    ACTIONS_H, BAR_PX, BODY_PX, BODY_PY, BRAND, BTN_H, BTN_MIN_W, BTN_R, BTN_TEXT, CLOSE_BOX,
    CLOSE_ICON, FOOTER_FILL, GAP_2, SHELL_R, TITLEBAR_H, TITLE_TEXT,
};
/// `DesktopDialog`'s shell: a rounded 1px-edged PANEL box with a 36px TOPBAR
/// titlebar, a body, and a translucent TOPBAR footer.
///
/// Height is fixed rather than derived from the body so that a stubbed body
/// still occupies the same space the real one will: a dialog whose footer moves
/// when its body is filled in would make every before/after comparison lie.
pub(super) fn dialog_shell(title: &str, w: f32, h: f32, body: Div) -> Div {
    dialog_shell_with_actions(title, w, h, body, Vec::new())
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

/// `flex h-9 items-center justify-between border-b border-editor-divider
/// bg-editor-topbar px-3` \u{2014} brand mark, title, close button.
fn titlebar(title: &str) -> Div {
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
                // The brand mark is an inline `<svg>` with two gradients, a
                // mask and a drop-shadow filter. `assets/icons.rs` is generated
                // from lucide paths and cannot carry any of that, so the port
                // draws the slot rather than the mark; `title_bar.rs` owns the
                // importable LOGO for the same reason.
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
        .child(
            div()
                .w(px(CLOSE_BOX))
                .h(px(CLOSE_BOX))
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .rounded(px(4.0))
                .child(
                    Icon::default()
                        .data(icons::X)
                        .size(px(CLOSE_ICON))
                        .text_color(color::ICON),
                ),
        )
}

/// `flex min-h-11 items-center justify-end gap-2 border-t border-editor-divider
/// bg-editor-topbar/65 px-3 py-2` \u{2014} the footer, and the buttons in it.
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

/// `DesktopDialogButton`: `h-7 min-w-16 rounded-[6px] border px-3 text-[11px]
/// font-medium`.
///
/// `primary` is `border-editor-accent bg-editor-accent text-black/90`; the
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
