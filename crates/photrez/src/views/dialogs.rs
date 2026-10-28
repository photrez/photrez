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
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Div, Entity, Hsla, TestSupportExt};

use super::title_bar;
#[allow(unused_imports)]
use crate::components::ui;
use crate::i18n::{t, t_args};
use crate::shot;
use crate::state::{AppState, DialogId};
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

/// Live Settings body: the language preference. The frozen theme row stays a
/// deferred stub (theme switching is not authorized).
fn settings_body_live(_state: &Entity<AppState>, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    use crate::i18n::{self, Locale};
    let current = i18n::locale();
    let mut body = v_flex().p(px(16.0)).gap(px(8.0)).w_full();
    body = body.child(
        div()
            .text_size(px(11.5))
            .text_color(color::TEXT_DIM)
            .child(t("settings.language")),
    );
    for (locale, label) in [(Locale::En, "English"), (Locale::Id, "Bahasa Indonesia")] {
        body = body.child(
            div()
                .id(format!("settings-locale-{}", locale.code()))
                .test_support()
                .h(px(26.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(3.0))
                .border_1()
                .border_color(if locale == current {
                    color::ACCENT
                } else {
                    color::BORDER
                })
                .text_size(px(11.5))
                .text_color(color::TEXT)
                .cursor_pointer()
                .on_click(
                    cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        app.set_locale(locale, cx);
                    }),
                )
                .child(label.to_string()),
        );
    }
    body.child(
        div()
            .mt(px(4.0))
            .text_size(px(10.0))
            .text_color(color::TEXT_DIM)
            .child("Theme switching is not implemented.".to_string()), // i18n-exempt: native-only copy; no frozen-catalog equivalent
    )
}

/// Resize Canvas body: the two digit buffers with the active field marked.
fn resize_canvas_body(state: &Entity<AppState>, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    let (width, height, field) = state
        .read(cx)
        .resize_canvas
        .as_ref()
        .map(|session| (session.width.clone(), session.height.clone(), session.field))
        .unwrap_or_default();
    let cancel_state = state.clone();
    v_flex()
        .p(px(16.0))
        .gap(px(8.0))
        .w_full()
        .child(resize_field("resize-width", "W", &width, field == 0, 0, cx))
        .child(resize_field(
            "resize-height",
            "H",
            &height,
            field == 1,
            1,
            cx,
        ))
        .child(
            div()
                .text_size(px(10.0))
                .text_color(color::TEXT_DIM)
                .child("Tab switches field, Enter resizes, Escape cancels.".to_string()), // i18n-exempt: native-only copy; no frozen-catalog equivalent
        )
        .child(
            div()
                .mt(px(4.0))
                .flex()
                .gap(px(8.0))
                .child(
                    div()
                        .id("resize-cancel")
                        .test_support()
                        .h(px(26.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(color::BORDER)
                        .text_size(px(11.5))
                        .text_color(color::TEXT_DIM)
                        .cursor_pointer()
                        .on_click(
                            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                                cx.stop_propagation();
                                let _ = &cancel_state;
                                app.cancel_resize_canvas(cx);
                            }),
                        )
                        .child(t("common.cancel").to_string()),
                )
                .child(
                    div()
                        .id("resize-confirm")
                        .test_support()
                        .h(px(26.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .rounded(px(4.0))
                        .bg(color::ACCENT)
                        .text_size(px(11.5))
                        .text_color(color::BG)
                        .cursor_pointer()
                        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            let result = app.apply_resize_canvas(cx);
                            match result {
                                Ok(()) => app.set_status_error(None, cx),
                                Err(error) => {
                                    app.set_status_error(Some(format!("Resize: {error}")), cx)
                                }
                            }
                        }))
                        .child(t("dialogs.resizeCanvas.resize").to_string()),
                ),
        )
}

fn resize_field(
    id: &'static str,
    label: &str,
    value: &str,
    active: bool,
    field: u8,
    cx: &gpui_kit::Context<crate::app::App>,
) -> Div {
    h_flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .w(px(16.0))
                .text_size(px(11.0))
                .text_color(color::TEXT_DIM)
                .child(label.to_string()),
        )
        .child(
            div()
                .id(id)
                .test_support()
                .h(px(26.0))
                .flex_1()
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(4.0))
                .border_1()
                .border_color(if active { color::ACCENT } else { color::BORDER })
                .bg(color::FIELD)
                .text_size(px(12.0))
                .text_color(color::TEXT)
                .cursor_pointer()
                .on_click(
                    cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        app.set_resize_field(field, cx);
                    }),
                )
                .child(format!("{value}|")),
        )
}

/// The rename prompt shows the live buffer; Enter commits, Escape cancels.
fn rename_body(state: &Entity<AppState>, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    let content = state
        .read(cx)
        .rename_layer
        .as_ref()
        .map(|session| session.content.clone())
        .unwrap_or_default();
    let cancel_state = state.clone();
    v_flex()
        .p(px(16.0))
        .gap(px(8.0))
        .w_full()
        .child(
            div()
                .id("rename-value")
                .test_support()
                .h(px(28.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(4.0))
                .border_1()
                .border_color(color::ACCENT)
                .bg(color::FIELD)
                .text_size(px(12.0))
                .text_color(color::TEXT)
                .child(format!("{content}|")),
        )
        .child(
            div()
                .text_size(px(10.0))
                .text_color(color::TEXT_DIM)
                .child("Enter to rename, Escape to cancel.".to_string()), // i18n-exempt: native-only copy; no frozen-catalog equivalent
        )
        .child(
            div()
                .mt(px(4.0))
                .flex()
                .gap(px(8.0))
                .child(
                    div()
                        .id("rename-cancel")
                        .test_support()
                        .h(px(26.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(color::BORDER)
                        .text_size(px(11.5))
                        .text_color(color::TEXT_DIM)
                        .cursor_pointer()
                        .on_click(
                            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                                cx.stop_propagation();
                                let _ = &cancel_state;
                                app.cancel_rename_layer(cx);
                            }),
                        )
                        .child(t("common.cancel").to_string()),
                )
                .child(
                    div()
                        .id("rename-confirm")
                        .test_support()
                        .h(px(26.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .rounded(px(4.0))
                        .bg(color::ACCENT)
                        .text_size(px(11.5))
                        .text_color(color::BG)
                        .cursor_pointer()
                        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            let result = app.commit_rename_layer(cx);
                            match result {
                                Ok(()) => app.set_status_error(None, cx),
                                Err(error) => {
                                    app.set_status_error(Some(format!("Rename: {error}")), cx)
                                }
                            }
                        }))
                        .child("Rename".to_string()), // i18n-exempt: native-only copy; no frozen-catalog equivalent
                ),
        )
}

/// The startup recovery prompt: list every autosave entry and let the user
/// restore all of them as unsaved tabs or discard the store.
fn recovery_body(state: &Entity<AppState>, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    let entries = state.read(cx).recovery_entries.clone();
    let mut body = v_flex().p(px(16.0)).gap(px(6.0)).w_full();
    body = body.child(
        div()
            .text_size(px(11.5))
            .text_color(color::TEXT_DIM)
            .child("Unsaved documents found in the autosave store:".to_string()), // i18n-exempt: native-only copy; no frozen-catalog equivalent
    );
    for entry in entries {
        body = body.child(
            div()
                .h(px(24.0))
                .flex()
                .items_center()
                .text_size(px(11.5))
                .text_color(color::TEXT)
                .child(entry.name),
        );
    }
    body.child(
        div()
            .mt(px(6.0))
            .flex()
            .gap(px(8.0))
            .child(
                div()
                    .id("recovery-discard")
                    .test_support()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(color::BORDER)
                    .text_size(px(11.5))
                    .text_color(color::TEXT_DIM)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            app.discard_autosaves(cx);
                        }),
                    )
                    .child(t("common.discard").to_string()),
            )
            .child(
                div()
                    .id("recovery-restore")
                    .test_support()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .bg(color::ACCENT)
                    .text_size(px(11.5))
                    .text_color(color::BG)
                    .cursor_pointer()
                    .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        app.restore_autosaves(cx);
                    }))
                    .child("Restore".to_string()), // i18n-exempt: native-only copy; no frozen-catalog equivalent
            ),
    )
}

/// Print dialog: paper, orientation, copies and the Print action.
fn print_body(state: &Entity<AppState>, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    use crate::app::PRINT_PAPERS;
    let (paper, copies, landscape, printers, selected_printer) = {
        let s = state.read(cx);
        (
            s.print_paper,
            s.print_copies,
            s.print_landscape,
            s.printers.clone(),
            s.print_printer,
        )
    };
    let mut body = v_flex().p(px(16.0)).gap(px(6.0)).w_full();
    if printers.is_empty() {
        body = body.child(
            div()
                .text_size(px(11.0))
                .text_color(color::TEXT_DIM)
                .child("Printer: system default".to_string()), // i18n-exempt: native-only copy; no frozen-catalog equivalent
        );
    } else {
        for (index, name) in printers.iter().enumerate() {
            let state = state.clone();
            body = body.child(
                div()
                    .id(format!("print-printer-{index}"))
                    .test_support()
                    .h(px(22.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(3.0))
                    .border_1()
                    .border_color(if index == selected_printer {
                        color::ACCENT
                    } else {
                        color::BORDER
                    })
                    .text_size(px(11.0))
                    .text_color(color::TEXT)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            state.update(cx, |state, _| state.print_printer = index);
                            cx.notify();
                        }),
                    )
                    .child(name.clone()),
            );
        }
    }
    for (index, (name, ..)) in PRINT_PAPERS.iter().enumerate() {
        let state = state.clone();
        body = body.child(
            div()
                .id(format!("print-paper-{index}"))
                .test_support()
                .h(px(24.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(3.0))
                .border_1()
                .border_color(if index == paper {
                    color::ACCENT
                } else {
                    color::BORDER
                })
                .text_size(px(11.5))
                .text_color(color::TEXT)
                .cursor_pointer()
                .on_click(
                    cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        state.update(cx, |state, _| state.print_paper = index);
                        cx.notify();
                    }),
                )
                .child((*name).to_string()),
        );
    }
    let orientation_state = state.clone();
    body = body
        .child(
            div()
                .id("print-orientation")
                .test_support()
                .h(px(26.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(3.0))
                .border_1()
                .border_color(color::BORDER)
                .text_size(px(11.5))
                .text_color(color::TEXT)
                .cursor_pointer()
                .on_click(
                    cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        orientation_state.update(cx, |state, _| {
                            state.print_landscape = !state.print_landscape;
                        });
                        cx.notify();
                    }),
                )
                .child(if landscape {
                    "Landscape".to_string()
                } else {
                    "Portrait".to_string()
                }),
        )
        .child(
            div().flex().items_center().gap(px(6.0)).child(
                div()
                    .text_size(px(11.5))
                    .text_color(color::TEXT_DIM)
                    .child(format!("Copies: {copies}")),
            ),
        );
    let copies_dec = state.clone();
    let copies_inc = state.clone();
    body = body.child(
        div()
            .flex()
            .gap(px(6.0))
            .child(
                div()
                    .id("print-copies-dec")
                    .test_support()
                    .h(px(24.0))
                    .w(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(3.0))
                    .border_1()
                    .border_color(color::BORDER)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            copies_dec.update(cx, |state, _| {
                                state.print_copies = state.print_copies.saturating_sub(1).max(1);
                            });
                            cx.notify();
                        }),
                    )
                    .child("-"), // i18n-exempt: glyph or symbol
            )
            .child(
                div()
                    .id("print-copies-inc")
                    .test_support()
                    .h(px(24.0))
                    .w(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(3.0))
                    .border_1()
                    .border_color(color::BORDER)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            copies_inc.update(cx, |state, _| {
                                state.print_copies = state.print_copies.saturating_add(1).min(99);
                            });
                            cx.notify();
                        }),
                    )
                    .child("+"), // i18n-exempt: glyph or symbol
            ),
    );
    let cancel_state = state.clone();
    body.child(
        div()
            .mt(px(6.0))
            .flex()
            .gap(px(8.0))
            .child(
                div()
                    .id("print-cancel")
                    .test_support()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(color::BORDER)
                    .text_size(px(11.5))
                    .text_color(color::TEXT_DIM)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            cancel_state.update(cx, |state, _| state.active_dialog = None);
                            cx.notify();
                        }),
                    )
                    .child(t("common.cancel").to_string()),
            )
            .child(
                div()
                    .id("print-confirm")
                    .test_support()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .bg(color::ACCENT)
                    .text_size(px(11.5))
                    .text_color(color::BG)
                    .cursor_pointer()
                    .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        app.print_now(cx);
                    }))
                    .child(t("dialogs.print.printBtn").to_string()),
            ),
    )
}

/// Export dialog: format, quality and the path prompt.
fn export_body(state: &Entity<AppState>, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    use crate::state::ExportFormat;
    let (format, quality) = {
        let s = state.read(cx);
        (s.export_format, s.export_quality)
    };
    let mut body = v_flex().p(px(16.0)).gap(px(8.0)).w_full();
    let mut formats = v_flex().gap(px(4.0));
    for candidate in [
        ExportFormat::Png,
        ExportFormat::Jpeg,
        ExportFormat::WebP,
        ExportFormat::Tiff,
    ] {
        let state = state.clone();
        formats = formats.child(
            div()
                .id(format!("export-format-{}", candidate.extension()))
                .test_support()
                .h(px(24.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(3.0))
                .border_1()
                .border_color(if candidate == format {
                    color::ACCENT
                } else {
                    color::BORDER
                })
                .text_size(px(11.5))
                .text_color(color::TEXT)
                .cursor_pointer()
                .on_click(
                    cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        state.update(cx, |state, _| state.export_format = candidate);
                        cx.notify();
                    }),
                )
                .child(candidate.label().to_string()),
        );
    }
    body = body.child(formats).child(
        div().flex().items_center().gap(px(6.0)).child(
            div()
                .text_size(px(11.5))
                .text_color(color::TEXT_DIM)
                .child(format!("Quality: {quality}")),
        ),
    );
    let quality_dec = state.clone();
    let quality_inc = state.clone();
    body = body.child(
        div()
            .flex()
            .gap(px(6.0))
            .child(
                div()
                    .id("export-quality-dec")
                    .test_support()
                    .h(px(24.0))
                    .w(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(3.0))
                    .border_1()
                    .border_color(color::BORDER)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            quality_dec.update(cx, |state, _| {
                                state.export_quality =
                                    state.export_quality.saturating_sub(5).max(1);
                            });
                            cx.notify();
                        }),
                    )
                    .child("-"), // i18n-exempt: glyph or symbol
            )
            .child(
                div()
                    .id("export-quality-inc")
                    .test_support()
                    .h(px(24.0))
                    .w(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(3.0))
                    .border_1()
                    .border_color(color::BORDER)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            quality_inc.update(cx, |state, _| {
                                state.export_quality =
                                    state.export_quality.saturating_add(5).min(100);
                            });
                            cx.notify();
                        }),
                    )
                    .child("+"), // i18n-exempt: glyph or symbol
            ),
    );
    let cancel_state = state.clone();
    body.child(
        div()
            .mt(px(6.0))
            .flex()
            .gap(px(8.0))
            .child(
                div()
                    .id("export-cancel")
                    .test_support()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(color::BORDER)
                    .text_size(px(11.5))
                    .text_color(color::TEXT_DIM)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            cancel_state.update(cx, |state, _| state.active_dialog = None);
                            cx.notify();
                        }),
                    )
                    .child(t("common.cancel").to_string()),
            )
            .child(
                div()
                    .id("export-confirm")
                    .test_support()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .bg(color::ACCENT)
                    .text_size(px(11.5))
                    .text_color(color::BG)
                    .cursor_pointer()
                    .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        app.export_now(cx);
                    }))
                    .child("Export...".to_string()), // i18n-exempt: native-only copy; no frozen-catalog equivalent
            ),
    )
}

const NEW_DOC_PRESETS: &[(&str, u32, u32)] = &[
    ("800 x 600", 800, 600),
    ("1920 x 1080", 1920, 1080),
    ("1080 x 1080", 1080, 1080),
    ("3840 x 2160", 3840, 2160),
];

/// The New Document dialog body: preset sizes plus Create/Cancel. The frozen
/// dialog's free-form width/height fields are represented by presets until the
/// shared text-input surface lands.
fn new_document_body(state: &Entity<AppState>, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    let selected = state
        .read(cx)
        .new_document_preset
        .min(NEW_DOC_PRESETS.len() - 1);
    let mut body = v_flex().p(px(16.0)).gap(px(8.0)).w_full();
    for (index, (label, _, _)) in NEW_DOC_PRESETS.iter().enumerate() {
        let state = state.clone();
        body = body.child(
            div()
                .id(format!("new-doc-preset-{index}"))
                .test_support()
                .h(px(28.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .rounded(px(4.0))
                .border_1()
                .border_color(if index == selected {
                    color::ACCENT
                } else {
                    color::BORDER
                })
                .text_size(px(11.5))
                .text_color(color::TEXT)
                .cursor_pointer()
                .on_click(
                    cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        state.update(cx, |state, _| state.new_document_preset = index);
                        cx.notify();
                    }),
                )
                .child(label.to_string()),
        );
    }
    let state_for_cancel = state.clone();
    body.child(
        div()
            .mt(px(4.0))
            .flex()
            .gap(px(8.0))
            .child(
                div()
                    .id("new-doc-cancel")
                    .test_support()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(color::BORDER)
                    .text_size(px(11.5))
                    .text_color(color::TEXT_DIM)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            state_for_cancel.update(cx, |state, _| state.active_dialog = None);
                            cx.notify();
                        }),
                    )
                    .child(t("common.cancel").to_string()),
            )
            .child(
                div()
                    .id("new-doc-create")
                    .test_support()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .bg(color::ACCENT)
                    .text_size(px(11.5))
                    .text_color(color::BG)
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            let (_, width, height) = NEW_DOC_PRESETS[selected];
                            let result = app.create_document(width, height, cx);
                            if let Err(error) = result {
                                app.set_status_error(Some(format!("New: {error}")), cx);
                            }
                        }),
                    )
                    .child(t("dialogs.newDocument.create").to_string()),
            ),
    )
}

/// Render a production dialog over the live shell.
///
/// The overlay is transparent (the frozen dialog has no scrim) and closes on a
/// click anywhere; the framed shell keeps the measured origin and size.
pub fn render_dialog(
    state: &Entity<AppState>,
    dialog: DialogId,
    rename_focus: &gpui_kit::FocusHandle,
    resize_focus: &gpui_kit::FocusHandle,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let state_for_close = state.clone();
    let close = cx.listener(
        move |app, _, _, cx: &mut gpui_kit::Context<crate::app::App>| {
            if dialog == DialogId::DirtyClose {
                app.dirty_close_cancel(cx);
            } else {
                state_for_close.update(cx, |state, _| state.active_dialog = None);
                cx.notify();
            }
        },
    );
    let framed = match dialog {
        DialogId::DirtyClose => dirty_close_shell(state, cx).into_any_element(),
        DialogId::NewDocument => {
            let Some((_, title, x, y, w, h)) =
                DIALOGS.iter().find(|(id, ..)| *id == "dialog-new-document")
            else {
                return div().into_any_element();
            };
            div()
                .id("dialog-new-document")
                .test_support()
                .absolute()
                .left(px(*x))
                .top(px(*y))
                .child(dialog_shell(
                    &t(title),
                    *w,
                    *h,
                    new_document_body(state, cx),
                ))
                .into_any_element()
        }
        DialogId::ResizeCanvas => div()
            .id("dialog-resize-canvas")
            .test_support()
            .absolute()
            .left(px(493.0))
            .top(px(260.0))
            .child(dialog_shell(
                "Resize Canvas",
                380.0,
                220.0,
                resize_canvas_body(state, cx),
            ))
            .into_any_element(),
        DialogId::Rename => div()
            .id("dialog-rename")
            .test_support()
            .absolute()
            .left(px(493.0))
            .top(px(300.0))
            .child(dialog_shell(
                "Rename layer",
                360.0,
                180.0,
                rename_body(state, cx),
            ))
            .into_any_element(),
        DialogId::Print => div()
            .id("dialog-print")
            .test_support()
            .absolute()
            .left(px(493.0))
            .top(px(260.0))
            .child(dialog_shell("Print", 380.0, 480.0, print_body(state, cx)))
            .into_any_element(),
        DialogId::Export => div()
            .id("dialog-export")
            .test_support()
            .absolute()
            .left(px(493.0))
            .top(px(260.0))
            .child(dialog_shell("Export", 380.0, 380.0, export_body(state, cx)))
            .into_any_element(),
        DialogId::Recovery => div()
            .id("dialog-recovery")
            .test_support()
            .absolute()
            .left(px(493.0))
            .top(px(260.0))
            .child(dialog_shell(
                "Recover unsaved work", // i18n-exempt: recovery dialog copy
                380.0,
                300.0,
                recovery_body(state, cx),
            ))
            .into_any_element(),
        DialogId::Settings => {
            let Some((_, title, x, y, w, _h)) =
                DIALOGS.iter().find(|(id, ..)| *id == "dialog-settings")
            else {
                return div().into_any_element();
            };
            div()
                .id("dialog-settings")
                .test_support()
                .absolute()
                .left(px(*x))
                .top(px(*y))
                .child(dialog_shell(
                    &t(title),
                    *w,
                    260.0,
                    settings_body_live(state, cx),
                ))
                .into_any_element()
        }
        DialogId::About => {
            let key = match dialog {
                DialogId::About => "dialog-about",
                DialogId::NewDocument
                | DialogId::Settings
                | DialogId::DirtyClose
                | DialogId::Recovery
                | DialogId::Export
                | DialogId::Print
                | DialogId::Rename
                | DialogId::ResizeCanvas => unreachable!("handled above"),
            };
            let Some((_, title, x, y, w, h)) = DIALOGS.iter().find(|(id, ..)| *id == key) else {
                return div().into_any_element();
            };
            div()
                .id(key)
                .test_support()
                .absolute()
                .left(px(*x))
                .top(px(*y))
                .child(dialog_shell(&t(title), *w, *h, dialog_body(key, *w, *h)))
                .into_any_element()
        }
    };
    div()
        .id("dialog-overlay")
        .test_support()
        .absolute()
        .inset(px(0.0))
        .when(dialog == DialogId::Rename, |overlay| {
            overlay.track_focus(rename_focus).on_key_down(cx.listener(
                |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                    app.rename_key(event, cx);
                },
            ))
        })
        .when(dialog == DialogId::ResizeCanvas, |overlay| {
            overlay.track_focus(resize_focus).on_key_down(cx.listener(
                |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                    app.resize_key(event, cx);
                },
            ))
        })
        // The dirty-close prompt is modal: a click outside must not discard
        // the user's decision. Other dialogs keep click-anywhere dismissal.
        .when(dialog == DialogId::DirtyClose, |overlay| {
            overlay.on_click(|_, _, _| {})
        })
        .when(dialog != DialogId::DirtyClose, |overlay| {
            overlay.on_click(close)
        })
        .child(framed)
        .into_any_element()
}

/// The dirty-close prompt: Save, Discard, or Cancel.
///
/// The frozen app's document-close confirm offers Discard/Cancel only; the
/// native prompt adds Save because G17 requires the save path to be reachable
/// from the close decision. Geometry follows the settings dialog's measured
/// frame (420x171 at 473,324).
fn dirty_close_shell(
    state: &Entity<AppState>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let name = state.read(cx).document_name.clone();
    let message = t_args("dialogs.unsavedChanges.message", &[("name", name.as_str())]);
    let body = v_flex().w_full().h_full().child(
        div()
            .text_size(px(12.0))
            .font_family(ui::FONT_UI)
            .line_height(px(18.0))
            .text_color(color::TEXT)
            .child(message),
    );
    let discard = dialog_action_button(
        &t("common.discard"),
        false,
        "dirty-close-discard",
        cx.listener(|app, _, _, cx| app.dirty_close_discard(cx)),
    )
    .into_any_element();
    let cancel = dialog_action_button(
        &t("common.cancel"),
        false,
        "dirty-close-cancel",
        cx.listener(|app, _, _, cx| app.dirty_close_cancel(cx)),
    )
    .into_any_element();
    let save = dialog_action_button(
        &t("menus.items.save"),
        true,
        "dirty-close-save",
        cx.listener(|app, _, _, cx| app.dirty_close_save(cx)),
    )
    .into_any_element();
    div()
        .id("dialog-dirty-close")
        .test_support()
        .absolute()
        .left(px(473.0))
        .top(px(324.0))
        .child(dialog_shell_with_actions(
            &t("dialogs.unsavedChanges.title"),
            420.0,
            171.0,
            body,
            vec![discard, cancel, save],
        ))
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

mod about;
mod chrome;
mod content;
#[cfg(test)]
mod tests;

use about::about_body;
#[cfg(test)]
use about::{
    about_version, ABOUT_LANGUAGES, ABOUT_LINK_GITHUB, ABOUT_LINK_WEBSITE, ABOUT_SPEC_LABELS,
    ABOUT_SPEC_VALUES,
};
use chrome::{dialog_action_button, dialog_shell, dialog_shell_with_actions};
use content::dialog_body;
