use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, relative, Div, FontWeight};

use crate::components::ui;
use crate::i18n::t;
use crate::icons;
use crate::tokens::color;

use super::about_body;
use super::chrome::dialog_button;
use super::{
    BODY_TEXT, DIALOG_FIELD_H, DIALOG_FIELD_TEXT, FIELD_PAD_DIALOG, GAP_2, R4, RESIZE_GAP,
    RESIZE_LABEL_W, RESIZE_ROW_GAP, RESIZE_UNIT_W, SWATCH_CANVAS, SWATCH_H, SWATCH_PAGE, SWATCH_W,
    SWATCH_X, SWATCH_Y,
};
/// The body of whichever dialog was asked for.
///
/// `dialog-settings` is transcribed in full and `dialog-about` is rebuilt for
/// the native shell; the rest are the right size with their controls stubbed.
/// See the module docs on why the chrome is real for all seven but the bodies
/// are not.
pub(super) fn dialog_body(id: &str, _w: f32, _h: f32) -> Div {
    match id {
        "dialog-about" => about_body(),
        "dialog-settings" => settings_body(),
        "dialog-new-document" => v_flex()
            .w_full()
            .h_full()
            .bg(color::PANEL)
            .child(new_document_swatch(id)),
        "dialog-resize" => resize_body(),
        _ => div().w_full().h_full().bg(color::PANEL),
    }
}

/// The Resize Canvas body. Two dimension rows, a unit picker, and an
/// aspect-lock button.
///
/// The description is 11px dim text with a 4px bottom margin, and the whole
/// column stacks with 10px gaps. Each row is centred on one row with an
/// 8px gap, holding a 52px label, an input that stretches and may shrink
/// below its content, and a never-squeezed 60px unit picker -- the same
/// label/field/select shape as
/// [`ui::prop_row`] but with different widths and a 30px field, so it is built
/// here rather than reused.
///
/// Resting state, so the inputs are painted, not editable: they show the
/// canvas dimensions the dialog opens with, and the focus/commit/arrow-step
/// behaviour a native number field carries is out of scope exactly as it
/// is for [`ui::editable_num_field`].
fn resize_body() -> Div {
    v_flex()
        .w_full()
        .h_full()
        .gap(px(RESIZE_GAP))
        .child(ui::clip(
            div()
                .mb(px(4.0))
                .text_size(px(11.0))
                .font_family(ui::FONT_UI)
                .line_height(relative(1.35))
                .text_color(color::TEXT_DIM)
                .child(t("dialogs.resizeCanvas.description")),
        ))
        .child(resize_row(&t("dialogs.resizeCanvas.width"), "800"))
        .child(resize_row(&t("dialogs.resizeCanvas.height"), "600"))
        .child(
            // 24px tall, centred on one row, 52px left padding - indented to line up with the
            // inputs, not the labels.
            h_flex()
                .w_full()
                .h(px(24.0))
                .items_center()
                .pl(px(RESIZE_LABEL_W))
                .child(
                    h_flex()
                        .h(px(24.0))
                        .items_center()
                        .gap(px(6.0))
                        .rounded(px(6.0))
                        .px(px(6.0))
                        .bg(color::FIELD)
                        .shadow(vec![ui::accent_well()])
                        .child(
                            Icon::default()
                                // icons::LINK, the source's Link icon --
                                // dialogs.rs had no chain glyph of its own.
                                .data(icons::LINK)
                                .size(px(14.0))
                                .text_color(color::ACCENT),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_family(ui::FONT_UI)
                                .text_color(color::TEXT)
                                .child(t("dialogs.resizeCanvas.lockAspect")),
                        ),
                ),
        )
}

/// One dimension row, centred on one row with an 8px gap.
fn resize_row(label: &str, value: &str) -> Div {
    h_flex()
        .w_full()
        .items_center()
        .gap(px(RESIZE_ROW_GAP))
        // The label yields and the picker does not, for the same reason as
        // everywhere else here: min-width defaults to the content's
        // intrinsic width, so a fixed-width column that cannot shrink pushes
        // whatever follows it past the dialog edge.
        .child(ui::clip(
            div()
                .w(px(RESIZE_LABEL_W))
                .flex_none()
                .text_size(px(11.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .text_color(color::TEXT_DIM)
                .child(label.to_string()),
        ))
        .child(ui::clip(dialog_field(value)))
        .child(ui::pin(unit_picker("px")))
}

/// The shared dialog field chrome with a value in it: 30px tall, 4px radius, a
/// FIELD_BORDER edge on a FIELD fill recessed by [`ui::field_well`], 10px side
/// padding, right-aligned so the digits sit where a number input puts them.
fn dialog_field(value: &str) -> Div {
    h_flex()
        .h(px(DIALOG_FIELD_H))
        .flex_1()
        .min_w(px(0.0))
        .overflow_hidden()
        .items_center()
        .justify_end()
        .rounded(px(R4))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .shadow(vec![ui::field_well()])
        .px(px(FIELD_PAD_DIALOG))
        .text_size(px(DIALOG_FIELD_TEXT))
        .font_family(ui::FONT_UI)
        .text_color(color::TEXT)
        .child(value.to_string())
}

/// The never-squeezed 60px unit picker: the shared dialog field chrome with
/// centred text, so the value is centred rather than right-aligned like the
/// number field.
///
/// A native picker has no visible arrow, so this
/// draws CHEVRON_DOWN beside the value instead, the same stand-in
/// [`ui::select_field`] documents.
fn unit_picker(unit: &str) -> Div {
    h_flex()
        .w(px(RESIZE_UNIT_W))
        .flex_none()
        .h(px(DIALOG_FIELD_H))
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .rounded(px(R4))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .shadow(vec![ui::field_well()])
        .px(px(FIELD_PAD_DIALOG))
        .text_size(px(DIALOG_FIELD_TEXT))
        .font_family(ui::FONT_UI)
        .text_color(color::TEXT)
        .child(unit.to_string())
        .child(
            Icon::default()
                .data(icons::CHEVRON_DOWN)
                .size(px(12.0))
                .text_color(color::FIELD_CAPTION),
        )
}

/// The canvas-preview swatch - the *only* thing that differs between
/// `dialog-new-document` and `empty-new-document`.
///
/// Measured, not assumed: the two PNGs differ in 21 501 pixels, of which
/// 20 949 sit inside a single 154x144 block at crop x 190..343, y 102..244,
/// and only 19 pixels in the whole image exceed delta 60. The block is
/// `#191919` when the dialog opened with a document (`needsDoc: true`) and
/// `#272727` when it did not - the empty state previews the *page*, the dialog
/// previews the *canvas*.
///
/// Drawn at the crop coordinates relative to the dialog's own top-left, which
/// puts it 66px below the titlebar: the body's 16px vertical padding plus the
/// source's own
/// header row. Both fills are written as `Hsla` rather than borrowed from a
/// token because neither is PANEL, which is the whole point of measuring them.
fn new_document_swatch(id: &str) -> Div {
    let fill = if id == "empty-new-document" {
        SWATCH_PAGE
    } else {
        SWATCH_CANVAS
    };
    div()
        .absolute()
        .left(px(SWATCH_X))
        .top(px(SWATCH_Y))
        .w(px(SWATCH_W))
        .h(px(SWATCH_H))
        .bg(fill)
}

/// `SettingsDialog` is the shortest dialog (420x171), and its body is a single
/// labelled row plus the footer's buttons, so the body is one row rather than a
/// stack.
fn settings_body() -> Div {
    v_flex()
        .w_full()
        .h_full()
        .justify_center()
        .gap(px(GAP_2))
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .gap(px(GAP_2))
                .child(
                    div()
                        .text_size(px(BODY_TEXT))
                        .font_family(ui::FONT_UI)
                        .text_color(color::TEXT)
                        .child(t("settings.theme")),
                )
                .child(dialog_button(&t("settings.themeSystem"), false)),
        )
        .child(
            div()
                .text_size(px(10.0))
                .font_family(ui::FONT_UI)
                .text_color(color::TEXT_DIM)
                .child("Changes apply immediately.".to_string()), // i18n-exempt: not in catalog
        )
}
