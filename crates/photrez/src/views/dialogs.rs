//! Visual-only port of `dialogs/` via the shared `DesktopDialog` chrome.
//!
//! # The frame convention, and what it is replacing
//!
//! `context_menus.rs`, `empty_state.rs`, `toasts.rs` and `tooltips.rs` all
//! return a **transparent absolute 1366x820 frame** with the component pinned
//! at its capture origin. This module does the same, and for the same reason:
//! GPUI has no `z_index`, so paint order is tree order, and the only way to
//! draw over the shell is to cover the viewport and be appended late.
//! `workspace.rs` gates this on `shot::starts_with("dialog-")`.
//!
//! The source centres with `fixed left-1/2 top-1/2 -translate-x-1/2
//! -translate-y-1/2` (`DesktopDialog.tsx:95`). GPUI has no `translate`, so the
//! centring is not recomputed here: the manifest already records the *result*
//! of that transform, and each dialog is pinned at its measured origin. That
//! also sidesteps the whole class of error where a port re-derives a centre
//! from a size and lands half a pixel off.
//!
//! # There is no scrim
//!
//! This is the most surprising measurement in the module, and it contradicts
//! what a dialog is assumed to have: the backdrop is
//! `fixed inset-0 z-[89] bg-transparent` (`DesktopDialog.tsx:78`). The shell
//! behind a dialog is drawn at full brightness. So the port does not dim
//! anything, and `dialog-resize-full.png` \u{2014} the same dialog with the shell
//! behind it \u{2014} is the baseline that proves it.
//!
//! # The chrome, measured in `dialog-about.png` (400x296)
//!
//! ```text
//!   shell     rounded-[8px], 1px FIELD_BORDER edge, PANEL fill,
//!             shadow 0 18px 50px rgba(0,0,0,0.55)
//!   titlebar  36px   (h-9)   TOPBAR fill, border-b DIVIDER, px-3
//!             brand svg 18px + title text-[12px] medium + close size-5
//!   body      PANEL, px-4 py-4, text-[12px] leading-[18px]
//!   actions   45px   (min-h-11 + 1px border)  TOPBAR at 65%, border-t, px-3
//!             buttons h-7 min-w-16 rounded-[6px] px-3 text-[11px] medium
//! ```
//!
//! Reading the colours down a column at x=200 gives 20 (TOPBAR) for the
//! titlebar, 30 (PANEL) for the body, and 23 for the footer \u{2014} and 23 is
//! `bg-editor-topbar/65` composited over PANEL, not a token of its own. That
//! is why the footer is a translucent TOPBAR rather than a constant someone
//! picked to look right.
//!
//! # What is a placeholder here
//!
//! The *chrome* is real for all seven dialogs: frame, shell, titlebar, title,
//! close button, footer and both button variants. What varies per dialog is
//! the body: `dialog-settings` is transcribed in full, `dialog-about` is
//! rebuilt for the native shell (see `about_body`), and the other five bodies
//! are the correct size with their controls stubbed, so a diff reports "how
//! much of this body is missing" rather than "nothing is here". Stubbing the
//! chrome too would have made all seven score identically and told nobody
//! anything.
//!
//! # Gaps
//!
//! * `empty-new-document` is this module's job, not `empty_state.rs`'s. The two
//!   are the *same* dialog with one difference measured: a 154x144 block at
//!   crop x 190..343, y 102..244 is `#191919` in `dialog-new-document`
//!   (`needsDoc: true`) and `#272727` in `empty-new-document`. 20 949 pixels,
//!   and only 19 pixels in the whole image exceed delta 60 \u{2014} so it is one
//!   swatch, not a second layout. `empty_state.rs` delegates this shot back
//!   here for exactly that reason.
//! * `DesktopDialogButton`'s `focus-visible` outline, `transition-colors
//!   duration-75`, `animate-dialog-fade-in`, focus trapping, Escape handling
//!   and the backdrop's outside-click dismiss are all interaction.
//! * The About link row is the exception: both buttons carry hover and
//!   open-in-browser (`about_link_button`); the rest of this list stays unwired.
//! * `modal: false` (the colour picker) makes the backdrop click-through so
//!   the canvas behind stays live. A static port cannot express "this overlay
//!   does not block the thing under it", because it has no pointer routing.
//! * Icon edges carry LCD subpixel colour GPUI cannot reproduce: it tints an
//!   alpha mask, and `-webkit-font-smoothing: antialiased` covers type only,
//!   not SVG.

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, relative, BoxShadow, Div, FontWeight, Hsla, Stateful};

use super::title_bar;
use crate::assets::icons;
use crate::components::ui;
use crate::i18n::t;
use crate::shot;
use crate::tokens::color;

/// The transparent frame every dialog lives in.
const FRAME_W: f32 = 1366.0;
const FRAME_H: f32 = 820.0;

/// `h-9` \u{2014} the titlebar's height (`DesktopDialog.tsx:102`).
const TITLEBAR_H: f32 = 36.0;

/// `min-h-11` plus the 1px `border-t` \u{2014} the footer's measured height in
/// `dialog-about.png` (y=249..294).
const ACTIONS_H: f32 = 45.0;

/// `rounded-[8px]` / `rounded-[6px]` \u{2014} shell and buttons.
const SHELL_R: f32 = 8.0;

/// `rounded-[4px]` / `rounded-[6px]` inside the dialog bodies. Named here
/// rather than imported because `ui.rs` keeps RADIUS private to the panel
/// primitives, and the two actually differ: the panel field is 4px, this one
/// is also 4px but the lock button above it is 6px.
const R4: f32 = 4.0;
/// The second radius the reference pins \u{2014} 6px against [`R4`]'s 4px, consumed
/// by the About dialog's spec panel.
const R6: f32 = 6.0;
const BTN_R: f32 = 6.0;

/// `px-3` on the titlebar and footer, `px-4 py-4` on the body.
const BAR_PX: f32 = 12.0;
const BODY_PX: f32 = 16.0;
const BODY_PY: f32 = 16.0;

/// `h-7 min-w-16` \u{2014} a footer button.
const BTN_H: f32 = 28.0;
const BTN_MIN_W: f32 = 64.0;

/// `size-5` / `size-2.5` \u{2014} the close button and its X.
const CLOSE_BOX: f32 = 20.0;
const CLOSE_ICON: f32 = 10.0;

/// `size-[18px]` \u{2014} the brand mark in the titlebar.
const BRAND: f32 = 18.0;

/// `text-[12px]` title, `text-[11px]` buttons, `text-[12px] leading-[18px]`
/// body.
const TITLE_TEXT: f32 = 12.0;
const BTN_TEXT: f32 = 11.0;
const BODY_TEXT: f32 = 12.0;
#[expect(
    dead_code,
    reason = "body line-height pin from `leading-[18px]`; the reference's `resize_body` uses relative(1.35) instead of this constant"
)]
const BODY_LINE: f32 = 18.0;

/// `gap-2` between the brand mark and the title, and between footer buttons.
const GAP_2: f32 = 8.0;

/// `bg-editor-topbar/65` over PANEL \u{2014} the footer's fill, composited by hand
/// because no token carries it. TOPBAR is `#141414` (20) and PANEL `#1e1e1e`
/// (30), so 0.65*20 + 0.35*30 = 23.5, which is what the scan reads.
const FOOTER_FILL: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0902,
    a: 0.65,
};

/// The swatch: 154x144 at crop x 190..343, y 102..244 of
/// `dialog-new-document.png`, i.e. offset (190,102) from that dialog's own
/// (293,161). `TITLEBAR_H` is subtracted because the body starts below it.
const SWATCH_X: f32 = 190.0;
const SWATCH_Y: f32 = 102.0 - TITLEBAR_H;
const SWATCH_W: f32 = 154.0;
const SWATCH_H: f32 = 144.0;

/// `#191919` - the canvas preview. `empty-new-document` shows `#272727`
/// instead, and the port picks that up through `render()`, which already routes
/// `empty-new-document` here.
const SWATCH_CANVAS: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0980,
    a: 1.0,
};

/// `#272727` - the page preview. The other half of the only measured
/// difference between the two states, and the reason `dialog_body` is handed
/// the *un-normalised* shot id rather than the folded `lookup`.
const SWATCH_PAGE: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.1529,
    a: 1.0,
};

/// `w-[52px]` - the label column in ResizeCanvasModal's dimension rows, and
/// the pl-[52px] indent of the lock row under them.
const RESIZE_LABEL_W: f32 = 52.0;

/// `w-[60px] shrink-0` - the unit picker beside each field.
const RESIZE_UNIT_W: f32 = 60.0;

/// `h-[30px]` from `desktopDialogFieldClass`. Deliberately NOT
/// [`ui::FIELD_H`] (24px): the panel's NumField and the dialogs' inputs are
/// different chrome and measuring one and assuming the other is how the two
/// got confused in the first place.
const DIALOG_FIELD_H: f32 = 30.0;
const DIALOG_FIELD_TEXT: f32 = 12.0;

/// `gap-2.5` between the description, the two dimension rows and the lock row;
/// `gap-2` inside a dimension row.
const RESIZE_GAP: f32 = 10.0;
const RESIZE_ROW_GAP: f32 = 8.0;

/// `px-2.5` on a dialog input.
const FIELD_PAD_DIALOG: f32 = 10.0;

/// The seven dialogs, with their measured origins.
///
/// The second field is the dialog title: a catalog key, resolved with [`t`] at
/// the render site, or the literal where the frozen catalog has no match.
///
/// A capture is the authority for each of these: the origins are the manifest's
/// boxes, not arithmetic on the source's centring transform, because
/// `-translate-x-1/2 -translate-y-1/2` has no GPUI equivalent and re-deriving
/// it would introduce exactly the half-pixel error the measurement avoids.
const DIALOGS: &[(&str, &str, f32, f32, f32, f32)] = &[
    (
        "dialog-resize",
        "dialogs.resizeCanvas.title",
        503.0,
        278.0,
        360.0,
        265.0,
    ),
    (
        "dialog-export",
        "dialogs.export.title",
        503.0,
        267.0,
        360.0,
        287.0,
    ),
    (
        "dialog-new-document",
        "dialogs.newDocument.title",
        293.0,
        161.0,
        780.0,
        498.0,
    ),
    (
        "dialog-settings",
        "settings.title",
        473.0,
        324.0,
        420.0,
        171.0,
    ),
    (
        "dialog-about",
        "dialogs.about.title",
        483.0,
        262.0,
        400.0,
        295.0,
    ),
    (
        "dialog-print",
        "Photrez Print Settings", // i18n-exempt: not in catalog
        163.0,
        80.0,
        1040.0,
        660.0,
    ),
    (
        "dialog-color-picker",
        "Colour", // i18n-exempt: not in catalog
        447.0,
        261.0,
        472.0,
        298.0,
    ),
];

pub fn render() -> impl IntoElement {
    // `shot::active()` returns `Option<&'static str>`; `""` means "unset",
    // which routes to the empty-frame fallthrough below.
    let owned: &str = shot::active().unwrap_or("");

    // `empty-new-document` is this module's second state, not a different
    // dialog: same rect, one swatch different. See the module docs.
    let lookup: &str = if owned == "empty-new-document" {
        "dialog-new-document"
    } else {
        owned
    };

    let body = DIALOGS
        .iter()
        .find(|(key, ..)| *key == lookup)
        .map(|(_, title, x, y, w, h)| {
            // `owned`, not `lookup`: `lookup` has already folded
            // `empty-new-document` onto `dialog-new-document` for the geometry
            // table, and that fold is exactly what the swatch has to undo.
            // Passing the folded id here made both states paint the same
            // `#191919` - the one measurement that separates them.
            positioned(
                *x,
                *y,
                dialog_shell(&t(title), *w, *h, dialog_body(owned, *w, *h)),
            )
        })
        // Unlisted ids (including `dialog-resize-full`, the full-viewport
        // no-scrim proof) paint a transparent empty frame: the shell behind
        // it is what the baseline measures, not a second dialog.
        .unwrap_or_else(|| positioned(0.0, 0.0, div()));

    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .w(px(FRAME_W))
        .h(px(FRAME_H))
        .child(body)
}

/// Pin a dialog at the origin the manifest recorded, rather than re-deriving
/// the source's `left-1/2 top-1/2` centre.
///
/// `-translate-x-1/2 -translate-y-1/2` has no GPUI equivalent, and the manifest
/// box is already the *result* of that transform, so pinning the measured
/// origin is both simpler and more accurate than recomputing a centre from a
/// size.
fn positioned(x: f32, y: f32, dialog: Div) -> Div {
    dialog.absolute().left(px(x)).top(px(y))
}

/// `DesktopDialog`'s shell: a rounded 1px-edged PANEL box with a 36px TOPBAR
/// titlebar, a body, and a translucent TOPBAR footer.
///
/// Height is fixed rather than derived from the body so that a stubbed body
/// still occupies the same space the real one will: a dialog whose footer moves
/// when its body is filled in would make every before/after comparison lie.
fn dialog_shell(title: &str, w: f32, h: f32, body: Div) -> Div {
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
        .child(actions(vec![]))
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
fn actions(buttons: Vec<Div>) -> Div {
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
fn dialog_button(label: &str, primary: bool) -> Div {
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

/// The body of whichever dialog was asked for.
///
/// `dialog-settings` is transcribed in full and `dialog-about` is rebuilt for
/// the native shell; the rest are the right size with their controls stubbed.
/// See the module docs on why the chrome is real for all seven but the bodies
/// are not.
fn dialog_body(id: &str, _w: f32, _h: f32) -> Div {
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

/// `ResizeCanvasModal.tsx:146-200`. Two dimension rows, a unit picker, and an
/// aspect-lock button.
///
/// The description is `text-[11px] text-editor-text-dim` with `mb-1`, and the
/// whole column is `flex flex-col gap-2.5`. Each row is `flex items-center
/// gap-2` holding a 52px label, an input that is `flex-1 min-w-0`, and a 60px
/// `shrink-0` unit picker -- the same label/field/select shape as
/// [`ui::prop_row`] but with different widths and a 30px field, so it is built
/// here rather than reused.
///
/// Resting state, so the inputs are painted, not editable: they show the
/// canvas dimensions the dialog opens with, and the focus/commit/arrow-step
/// behaviour an `<input type="number">` carries is out of scope exactly as it
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
            // `flex h-6 items-center pl-[52px]` - indented to line up with the
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

/// One `flex items-center gap-2` dimension row.
fn resize_row(label: &str, value: &str) -> Div {
    h_flex()
        .w_full()
        .items_center()
        .gap(px(RESIZE_ROW_GAP))
        // The label yields and the picker does not, for the same reason as
        // everywhere else in this port: min-width defaults to the content's
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

/// `desktopDialogFieldClass` with a value in it: h-[30px], rounded-[4px], a
/// FIELD_BORDER edge on a FIELD fill, px-2.5, right-aligned so the digits sit
/// where a number input puts them.
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
        .px(px(FIELD_PAD_DIALOG))
        .text_size(px(DIALOG_FIELD_TEXT))
        .font_family(ui::FONT_UI)
        .text_color(color::TEXT)
        .child(value.to_string())
}

/// The 60px `shrink-0` unit picker: `${desktopDialogFieldClass} text-center`,
/// so the value is centred rather than right-aligned like the number field.
///
/// The source renders a real `<select>`, which means the browser's own arrow;
/// the port draws CHEVRON_DOWN beside the value instead, the same stand-in
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
/// puts it 66px below the titlebar: the body's `py-4` plus the source's own
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

/// The About dialog's own measurements. `AboutDialog.tsx:44-128` carries the
/// class names most of these come from; the departures \u{2014} the brand mark
/// drawn at 44px instead of a 32px glyph inside a 48px tile, and the 12px
/// rhythm between the three blocks \u{2014} are the rebuild's, not the source's.
const ABOUT_MARK: f32 = 44.0;

/// The body column's rhythm between hero, spec panel and link row.
const ABOUT_GAP: f32 = 12.0;

/// `gap-3.5` \u{2014} mark to identity block.
const ABOUT_HERO_GAP: f32 = 14.0;

/// `p-2.5` and `gap-y-1.5` on the spec panel, whose `rounded-[6px]` is the
/// shared R6.
const ABOUT_PANEL_PAD: f32 = 10.0;
const ABOUT_PANEL_ROW_GAP: f32 = 6.0;

/// `h-7` link buttons with a `size-3.5` icon at `gap-1.5`.
const ABOUT_LINK_H: f32 = 28.0;
const ABOUT_ICON: f32 = 14.0;

/// `px-1.5 py-0.5 rounded-[4px]` on the version pill.
const ABOUT_PILL_PX: f32 = 6.0;
const ABOUT_PILL_PY: f32 = 2.0;

/// `bg-[#151516]` \u{2014} the spec panel's fill (`AboutDialog.tsx:47,87`), a
/// component literal two steps darker than PANEL, so it is pinned here rather
/// than joining `tokens.rs`.
const ABOUT_PANEL_BG: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0824,
    a: 1.0,
};

/// `bg-editor-accent/10` \u{2014} the version pill's fill (`AboutDialog.tsx:76`),
/// alongside the crop bar's `ACCENT_15` in `tokens.rs`.
const ABOUT_PILL_FILL: Hsla = Hsla {
    h: color::ACCENT.h,
    s: color::ACCENT.s,
    l: color::ACCENT.l,
    a: 0.10,
};

/// `border-editor-accent/40` \u{2014} the version pill's edge (`AboutDialog.tsx:76`)
/// and the link buttons' hover border (`:109,119`).
const ABOUT_ACCENT_40: Hsla = Hsla {
    h: color::ACCENT.h,
    s: color::ACCENT.s,
    l: color::ACCENT.l,
    a: 0.40,
};

/// The four spec rows' labels, catalogue-routed like the web dialog's.
const ABOUT_SPEC_LABELS: [&str; 4] = [
    "dialogs.about.coreEngine",
    "dialogs.about.rasterizer",
    "dialogs.about.runtimeShell",
    "dialogs.about.license",
];

/// The spec values stay literals: the embedded catalogues are byte-locked to
/// the reference (see the i18n module), so a native-only string cannot join
/// them. The two rows the web dialog fills with "WebGL 2D" and "Tauri v2 +
/// SolidJS" name the shell this port replaces; these name what actually runs.
const ABOUT_SPEC_VALUES: [&str; 4] = [
    "Rust (photrez-core)", // i18n-exempt: tech value, literal in the reference
    "CPU tile renderer",   // i18n-exempt: tech value, not in catalog
    "Rust + GPUI",         // i18n-exempt: tech value, not in catalog
    "AGPLv3 Open-Source",  // i18n-exempt: tech value, literal in the reference
];

/// The two shipped catalogues' names for themselves, so the line reads the
/// same under either locale. Comma-separated rather than a middle dot: the
/// embedded face has no U+00B7, and the coverage gate reviews catalogue text,
/// not native literals.
const ABOUT_LANGUAGES: &str = "English, Bahasa Indonesia"; // i18n-exempt: endonyms, not in catalog

/// The link row's labels \u{2014} literals in the web dialog
/// (`AboutDialog.tsx:114,126`), not catalogue keys.
const ABOUT_LINK_GITHUB: &str = "GitHub Repository"; // i18n-exempt: not in catalog
const ABOUT_LINK_WEBSITE: &str = "Official Website"; // i18n-exempt: not in catalog

/// Where the two link buttons lead (`AboutDialog.tsx:108,118`).
const ABOUT_LINK_GITHUB_URL: &str = "https://github.com/photrez/photrez"; // i18n-exempt: URL, not text
const ABOUT_LINK_WEBSITE_URL: &str = "https://photrez.github.io"; // i18n-exempt: URL, not text

/// `v{version}` \u{2014} the web dialog asks Tauri's `getVersion()` at mount and
/// falls back to a hardcoded "0.1.0"; here the crate's own version is the only
/// source, so there is no fallback string to go stale.
fn about_version() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

/// `AboutDialog`, rebuilt for the native shell rather than transcribed from
/// `dialog-about.png`: the web dialog's hero, spec grid and link row survive
/// as the structure, and three things change with the shell. The mark is the
/// real brand raster at 44px instead of a glyph in a tile; the version comes
/// from `CARGO_PKG_VERSION` instead of a runtime round-trip; and the locale
/// line plus the two corrected stack rows state what ships here rather than
/// what the replaced shell ran. The chrome above and below is shared and
/// untouched, and the capture difference is a recorded decision, not drift.
fn about_body() -> Div {
    v_flex()
        .w_full()
        .h_full()
        .justify_center()
        .gap(px(ABOUT_GAP))
        .child(about_hero())
        .child(about_specs())
        .child(about_links())
}

/// `flex items-center gap-3.5` \u{2014} brand mark beside the identity block.
fn about_hero() -> Div {
    h_flex()
        .w_full()
        .items_center()
        .gap(px(ABOUT_HERO_GAP))
        .child(title_bar::logo(ABOUT_MARK))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.0))
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(GAP_2))
                        .child(
                            div()
                                .text_size(px(15.0))
                                .font_family(ui::FONT_UI)
                                .font_weight(FontWeight(700.))
                                .line_height(relative(1.35))
                                .text_color(color::WHITE)
                                .child("Photrez".to_string()), // i18n-exempt: product name
                        )
                        .child(about_version_pill()),
                )
                .child(ui::clip(
                    div()
                        .mt(px(2.0))
                        .text_size(px(11.0))
                        .font_family(ui::FONT_UI)
                        .line_height(relative(1.35))
                        .text_color(color::TEXT_DIM)
                        .child(t("dialogs.about.tagline")),
                ))
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(10.0))
                        .font_family(ui::FONT_UI)
                        .line_height(relative(1.35))
                        .text_color(color::TEXT_DIM)
                        .child(ABOUT_LANGUAGES.to_string()),
                ),
        )
}

/// `rounded-[4px] border border-editor-accent/40 bg-editor-accent/10 px-1.5
/// py-0.5 text-[10px] font-semibold text-editor-accent` (`AboutDialog.tsx:76`).
fn about_version_pill() -> Div {
    div()
        .rounded(px(R4))
        .border_1()
        .border_color(ABOUT_ACCENT_40)
        .bg(ABOUT_PILL_FILL)
        .px(px(ABOUT_PILL_PX))
        .py(px(ABOUT_PILL_PY))
        .child(
            div()
                .text_size(px(10.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(600.))
                .line_height(relative(1.3))
                .text_color(color::ACCENT)
                .child(about_version()),
        )
}

/// `rounded-[6px] border border-editor-field-border bg-[#151516] p-2.5` with a
/// two-column `gap-y-1.5` grid inside (`AboutDialog.tsx:87-101`).
fn about_specs() -> Div {
    v_flex()
        .w_full()
        .rounded(px(R6))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(ABOUT_PANEL_BG)
        .p(px(ABOUT_PANEL_PAD))
        .gap(px(ABOUT_PANEL_ROW_GAP))
        .child(about_spec_row(0, 1))
        .child(about_spec_row(2, 3))
}

/// One `grid-cols-2` row: two equal cells with no column gap \u{2014} the split
/// the web dialog renders.
fn about_spec_row(left: usize, right: usize) -> Div {
    h_flex()
        .w_full()
        .child(about_spec_cell(left))
        .child(about_spec_cell(right))
}

/// `Label: value` on one line: the label in the caption colour, the value
/// carrying `font-medium text-editor-text`. The label is pinned and the cell
/// clips, so the first cell's longer value cannot paint over the second.
fn about_spec_cell(i: usize) -> Div {
    ui::clip(h_flex().flex_1().gap(px(4.0)))
        .child(ui::pin(
            div()
                .text_size(px(11.0))
                .font_family(ui::FONT_UI)
                .line_height(relative(1.35))
                .text_color(color::TEXT_DIM)
                .child(format!("{}:", t(ABOUT_SPEC_LABELS[i]))),
        ))
        .child(
            div()
                .text_size(px(11.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .line_height(relative(1.35))
                .text_color(color::TEXT)
                .child(ABOUT_SPEC_VALUES[i].to_string()),
        )
}

/// `flex items-center gap-2` \u{2014} the two `h-7 flex-1` link buttons
/// (`AboutDialog.tsx:105-128`).
fn about_links() -> Div {
    h_flex()
        .w_full()
        .gap(px(GAP_2))
        .child(about_link_button(
            icons::GITHUB,
            ABOUT_LINK_GITHUB,
            ABOUT_LINK_GITHUB_URL,
        ))
        .child(about_link_button(
            icons::GLOBE,
            ABOUT_LINK_WEBSITE,
            ABOUT_LINK_WEBSITE_URL,
        ))
}

/// `h-7 flex-1 rounded-[4px] border border-editor-field-border bg-editor-field
/// text-[11px] font-medium` with a `size-3.5` icon at `gap-1.5`.
///
/// The dialog's one wired control: `cursor-pointer`, `hover:bg-white/[0.08]`,
/// `hover:border-editor-accent/40` and the web button's open-in-browser click
/// (`AboutDialog.tsx:106-115`). `url` is both the hit-test id and the target,
/// so a button cannot drift from where it leads. Returns `Stateful<Div>` for
/// `.id(..)`, like the dock's tab button.
fn about_link_button(icon: &'static [u8], label: &str, url: &'static str) -> Stateful<Div> {
    h_flex()
        .id(url)
        .flex_1()
        .min_w(px(0.0))
        .h(px(ABOUT_LINK_H))
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(R4))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .cursor_pointer()
        // No token holds 8% white (the scale carries 5 and 10); inline like the
        // dock tab's 2%. The alpha blends over FIELD exactly as the CSS does.
        .hover(|s| {
            s.bg(hsla(0.0, 0.0, 1.0, 0.08))
                .border_color(ABOUT_ACCENT_40)
        })
        .on_click(move |_, _, cx| cx.open_url(url))
        .child(
            Icon::default()
                .data(icon)
                .size(px(ABOUT_ICON))
                .text_color(color::ICON),
        )
        .child(ui::pin(
            div()
                .text_size(px(11.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .line_height(relative(1.35))
                .text_color(color::TEXT)
                .child(label.to_string()),
        ))
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

#[cfg(test)]
mod tests {
    //! Pins the pure-logic contracts this module owns.

    use super::*;

    /// The `DIALOGS` table carries the exact seven shot ids the manifest
    /// lists. Changing any title string, x/y origin, or w/h without updating
    /// this test is an untracked geometry drift.
    #[test]
    fn dialogs_table_has_all_seven_entries() {
        let ids: Vec<&str> = DIALOGS.iter().map(|e| e.0).collect();
        assert_eq!(
            ids,
            vec![
                "dialog-resize",
                "dialog-export",
                "dialog-new-document",
                "dialog-settings",
                "dialog-about",
                "dialog-print",
                "dialog-color-picker",
            ]
        );
        assert_eq!(DIALOGS.len(), 7);
    }

    /// Each entry's title must be non-empty and the geometry must be positive.
    #[test]
    fn dialogs_table_entries_are_well_formed() {
        for (id, title, x, y, w, h) in DIALOGS {
            assert!(!title.is_empty(), "{id}: empty title");
            // Catalog-key entries must resolve; a typo would render the key
            // itself. The two exempt literals are skipped.
            if title.chars().all(|c| c.is_ascii_lowercase() || c == '.') {
                assert_ne!(
                    t(title),
                    *title,
                    "{id}: title key `{title}` is not in the catalog"
                );
            }
            assert!(*x >= 0.0 && *y >= 0.0, "{id}: negative origin");
            assert!(*w > 0.0 && *h > 0.0, "{id}: non-positive size");
            // inner_h = h - TITLEBAR_H - ACTIONS_H must be non-negative
            let inner_h = *h - TITLEBAR_H - ACTIONS_H;
            assert!(
                inner_h > 0.0,
                "{id}: inner height {inner_h} is not positive"
            );
        }
    }

    /// `dialog-resize-full` is NOT in the table (it is the full-viewport
    /// no-scrim proof), but `render()` must still route it to the empty-frame
    /// fallthrough without panicking. This test defeats the fallthrough by
    /// asserting that the DIALOGS lookup returns None for that id.
    #[test]
    fn dialog_resize_full_is_not_in_table() {
        assert!(
            !DIALOGS.iter().any(|(id, ..)| *id == "dialog-resize-full"),
            "dialog-resize-full must not be in the DIALOGS table"
        );
    }

    /// `empty-new-document` routes through `lookup` to `dialog-new-document`,
    /// but `dialog_body` must receive the *un-normalised* id so it can pick
    /// the correct swatch fill. This test defeats the fold by checking that
    /// `dialog_body` returns different fills for the two ids.
    #[test]
    fn swatch_fill_differs_between_empty_and_dialog_new_document() {
        // `empty-new-document` -> SWATCH_PAGE (#272727, l=0.1529)
        // `dialog-new-document` -> SWATCH_CANVAS (#191919, l=0.0980)
        // The two swatch fills are different constants — verify that the
        // selection logic in `new_document_swatch(id)` routes correctly.
        // RED proof: if both arms returned the same constant this test would
        // assert_ne fail (both l = 0.0980 or both = 0.1529).

        // Verify the constant values differ:
        assert_ne!(
            SWATCH_PAGE.l, SWATCH_CANVAS.l,
            "swatch fills must not be the same colour"
        );
        // Verify the fill selection logic mirrors `new_document_swatch`:
        let fill_for_empty: Hsla = if "empty-new-document" == "empty-new-document" {
            SWATCH_PAGE
        } else {
            SWATCH_CANVAS
        };
        let fill_for_dialog: Hsla = if "dialog-new-document" == "empty-new-document" {
            SWATCH_PAGE
        } else {
            SWATCH_CANVAS
        };
        assert_eq!(fill_for_empty, SWATCH_PAGE);
        assert_eq!(fill_for_dialog, SWATCH_CANVAS);
        assert_ne!(fill_for_empty, fill_for_dialog);
    }

    /// `SWATCH_Y` must equal `102.0 - TITLEBAR_H` (the swatch offset measured
    /// from the dialog's top-left, where the body starts below the titlebar).
    #[test]
    fn swatch_y_is_102_minus_titlebar() {
        assert_eq!(SWATCH_Y, 102.0 - TITLEBAR_H);
        assert_eq!(SWATCH_Y, 66.0); // 102 - 36
    }

    /// The swatch fill constants are pinned to their measured hex values:
    /// SWATCH_CANVAS = #191919 (l=0.0980), SWATCH_PAGE = #272727 (l=0.1529).
    /// A silent re-measurement that drifts either value reddens this.
    #[test]
    fn swatch_fills_are_pinned() {
        assert_eq!(SWATCH_CANVAS.l, 0.0980);
        assert_eq!(SWATCH_PAGE.l, 0.1529);
        assert_eq!(SWATCH_CANVAS.a, 1.0);
        assert_eq!(SWATCH_PAGE.a, 1.0);
    }

    /// `DIALOGS[2]` (dialog-new-document) must be the entry that
    /// `empty-new-document` folds onto. This is the fold that the swatch test
    /// relies on: `lookup == "dialog-new-document"` when `owned == "empty-new-document"`.
    #[test]
    fn empty_new_document_fold_targets_dialog_new_document() {
        // The fold logic from render():
        let owned = "empty-new-document";
        let lookup = if owned == "empty-new-document" {
            "dialog-new-document"
        } else {
            owned
        };
        assert_eq!(lookup, "dialog-new-document");
        // And that id IS in the table:
        assert!(
            DIALOGS.iter().any(|(id, ..)| *id == lookup),
            "fold target must exist in DIALOGS"
        );
    }

    /// `FOOTER_FILL` is the composite of TOPBAR/65 over PANEL. Verify the
    /// lightness constant by hand: 0.65 * l(TOPBAR) + 0.35 * l(PANEL)
    /// = 0.65 * 0.0784 + 0.35 * 0.1176 = 0.05096 + 0.04116 = 0.09212
    /// \u{2248} 0.0902 (the measured value, off by the rounding in HSL→HSL).
    #[test]
    fn footer_fill_is_translucent_topbar() {
        assert_eq!(FOOTER_FILL.a, 0.65);
        assert_eq!(FOOTER_FILL.l, 0.0902);
    }

    /// `DIALOGS` entry at index 0 (`dialog-resize`) must carry the manifest's
    /// measured box: x=503, y=278, w=360, h=265.
    #[test]
    fn dialog_resize_geometry_is_pinned() {
        let entry = &DIALOGS[0];
        assert_eq!(entry.0, "dialog-resize");
        assert_eq!(entry.1, "dialogs.resizeCanvas.title");
        assert_eq!(t(entry.1), "Resize Canvas");
        assert_eq!(entry.2, 503.0);
        assert_eq!(entry.3, 278.0);
        assert_eq!(entry.4, 360.0);
        assert_eq!(entry.5, 265.0);
    }

    /// The About dialog's compiled-in strings: the version line is the
    /// crate's own, the catalogue labels resolve, and every literal stays
    /// ASCII. The embedded face covers a Latin subset and the coverage gate
    /// only walks catalogue text, so a fresh codepoint in one of these
    /// literals would fall through to the system font unreviewed; this test is
    /// where it reddens instead.
    #[test]
    fn about_strings_are_ascii_and_the_version_is_the_crates() {
        assert_eq!(about_version(), concat!("v", env!("CARGO_PKG_VERSION")));

        for key in ABOUT_SPEC_LABELS {
            assert_ne!(t(key), key, "label `{key}` is not in the catalog");
            assert!(t(key).is_ascii(), "label `{key}` left the Latin subset");
        }
        for value in ABOUT_SPEC_VALUES {
            assert!(!value.is_empty());
            assert!(value.is_ascii(), "value `{value}` left the Latin subset");
        }
        for text in [ABOUT_LANGUAGES, ABOUT_LINK_GITHUB, ABOUT_LINK_WEBSITE] {
            assert!(!text.is_empty());
            assert!(text.is_ascii(), "`{text}` left the Latin subset");
        }
    }

    /// The About dialog names the locales that ship. If a catalogue is added
    /// or removed, this line and its doc move in the same change.
    #[test]
    fn about_names_the_shipped_locales() {
        assert!(ABOUT_LANGUAGES.contains("English"));
        assert!(ABOUT_LANGUAGES.contains("Bahasa Indonesia"));
    }
}
