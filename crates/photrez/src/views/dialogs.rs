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
//!   duration-75`, `animate-dialog-fade-in` and focus trapping (Tab cycles
//!   through the dialog's own controls) are still unwired. The titlebar close,
//!   Escape for Settings/About, the backdrop's outside-click dismiss and the
//!   About link row are live: `chrome::dialog_close_button` runs each dialog's
//!   own dismissal, and `dismiss_chrome_dialog` owns the managed-focus Escape.
//! * `modal: false` (the colour picker) makes the backdrop click-through so
//!   the canvas behind stays live. A static port cannot express "this overlay
//!   does not block the thing under it", because it has no pointer routing.
//! * Icon edges carry LCD subpixel colour GPUI cannot reproduce: it tints an
//!   alpha mask, and `-webkit-font-smoothing: antialiased` covers type only,
//!   not SVG.

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, relative, BoxShadow, Div, Entity, FontWeight, Hsla, TestSupportExt};

use super::title_bar;
#[allow(unused_imports)]
use crate::components::ui;
use crate::i18n::{t, t_args};
use crate::icons;
use crate::shot;
use crate::state::{AppState, DialogId};
use crate::tokens::color;
use crate::units::DimensionUnit;

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

/// Live Settings body (`SettingsDialog.tsx`): the Interface section with the
/// language dropdown. The list expands inline because a popup deferred inside
/// a dialog overlay never receives hits (measured during the units work).
fn settings_body_live(state: &Entity<AppState>, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    use crate::i18n::{self, Locale};
    let current = i18n::locale();
    let open = state.read(cx).settings_language_open;
    let current_label = if current == Locale::Id {
        "Bahasa Indonesia"
    } else {
        "English"
    };
    let trigger = h_flex()
        .id("settings-language")
        .test_support()
        .h(px(28.0))
        .w(px(220.0))
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .px(px(8.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .text_size(px(11.5))
        .text_color(color::TEXT)
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            app.toggle_settings_language(cx);
        }))
        .child(
            h_flex()
                .gap(px(4.0))
                .child(
                    div()
                        .text_color(color::TEXT_DIM)
                        .child(format!("{}:", t("settings.language"))),
                )
                .child(div().font_weight(FontWeight(600.)).child(current_label)),
        )
        .child(
            Icon::default()
                .data(icons::CHEVRON_DOWN)
                .size(px(10.0))
                .text_color(color::TEXT_DIM),
        );
    let mut list = v_flex().w(px(220.0)).gap(px(2.0));
    for (locale, label) in [(Locale::En, "English"), (Locale::Id, "Bahasa Indonesia")] {
        list = list.child(
            div()
                .id(format!("settings-locale-{}", locale.code()))
                .test_support()
                .h(px(24.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(4.0))
                .text_size(px(11.5))
                .text_color(if locale == current {
                    color::ACCENT
                } else {
                    color::TEXT
                })
                .cursor_pointer()
                .on_click(
                    cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        app.choose_settings_language(locale, cx);
                    }),
                )
                .child(label.to_string()),
        );
    }
    v_flex()
        .gap(px(8.0))
        .w_full()
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight(700.))
                .text_color(color::TEXT_DIM)
                .child(t("settings.interface").to_uppercase()),
        )
        .child(h_flex().pl(px(8.0)).child(trigger))
        .when(open, |el| el.child(v_flex().pl(px(8.0)).child(list)))
}

/// Resize Canvas body (`ResizeCanvasModal.tsx:147-217`): the description, the
/// two unit-aware fields with their own unit select, and the aspect lock
/// between them.
fn resize_canvas_body(state: &Entity<AppState>, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    let (width, height, field, unit, locked, unit_open) = {
        let read = state.read(cx);
        match read.resize_canvas.as_ref() {
            Some(session) => (
                session.width.clone(),
                session.height.clone(),
                session.field,
                session.unit,
                session.aspect_locked,
                read.resize_unit_open,
            ),
            None => (
                String::new(),
                String::new(),
                0,
                DimensionUnit::Px,
                true,
                false,
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
            cx,
        ))
}

#[allow(clippy::too_many_arguments)] // flat args mirror the oracle's field props
fn resize_field(
    id: &'static str,
    label: &str,
    value: &str,
    active: bool,
    field: u8,
    unit: DimensionUnit,
    list_open: bool,
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
        .child(
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
                .text_size(px(11.5))
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

/// `Keep proportions` (`ResizeCanvasModal.tsx:176-192`): an accent pill while
/// the aspect lock is on, a dim row while it is off.
fn resize_aspect_lock(locked: bool, cx: &gpui_kit::Context<crate::app::App>) -> Div {
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
            .border_color(if locked { color::ACCENT } else { color::BORDER })
            .text_size(px(11.0))
            .text_color(if locked {
                color::ACCENT
            } else {
                color::TEXT_DIM
            })
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

/// Export dialog: format, quality and the path prompt.
fn export_body(
    state: &Entity<AppState>,
    document_size: Option<(u32, u32)>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> Div {
    use crate::state::{ExportFormat, SliderTarget};
    use crate::views::option_bar::slider_surface::BarSliderSurface;

    let (format, quality, open, name) = {
        let s = state.read(cx);
        (
            s.export_format,
            s.export_quality,
            s.export_format_open,
            s.document_name.clone(),
        )
    };
    let size = document_size;

    let card =
        h_flex()
            .gap(px(12.0))
            .items_center()
            .p(px(10.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(color::BORDER)
            .bg(color::FIELD)
            .child(
                div()
                    .w(px(36.0))
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(color::BORDER)
                    .child(
                        Icon::default()
                            .data(icons::IMAGE)
                            .size(px(20.0))
                            .text_color(color::ACCENT),
                    ),
            )
            .child(
                v_flex()
                    .gap(px(2.0))
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight(600.0))
                            .text_color(color::TEXT)
                            .child(name),
                    )
                    .child(div().text_size(px(11.0)).text_color(color::TEXT_DIM).child(
                        match size {
                            Some((w, h)) => format!("{} \u{00d7} {} px \u{00b7} RGB 8-bit", w, h),
                            None => "No document".to_string(),
                        },
                    )),
            );

    let toggle_state = state.clone();
    let trigger = div()
        .id("export-format-trigger")
        .test_support()
        .h(px(30.0))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .px(px(10.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .cursor_pointer()
        .on_click(
            cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                toggle_state.update(cx, |state, _| {
                    state.export_format_open = !state.export_format_open;
                });
                cx.notify();
            }),
        )
        .child(
            h_flex()
                .gap(px(8.0))
                .items_center()
                .child(
                    div()
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(3.0))
                        .bg(color::CANVAS)
                        .border_1()
                        .border_color(color::BORDER)
                        .text_size(px(9.0))
                        .font_weight(FontWeight(700.0))
                        .text_color(color::TEXT_DIM)
                        .child(format.label().to_uppercase()),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight(500.0))
                        .text_color(color::TEXT)
                        .child(format.label()),
                ),
        )
        .child(
            Icon::default()
                .data(icons::CHEVRON_DOWN)
                .size(px(12.0))
                .text_color(color::TEXT_DIM),
        );

    let mut format_section = v_flex()
        .w_full()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight(700.0))
                .text_color(color::TEXT_DIM)
                .child(t("dialogs.export.format").to_uppercase()),
        )
        .child(trigger);
    if open {
        let mut list = v_flex().w_full().gap(px(2.0)).py(px(4.0));
        for candidate in [ExportFormat::Png, ExportFormat::Jpeg, ExportFormat::WebP] {
            let pick_state = state.clone();
            let current = candidate == format;
            list = list.child(
                div()
                    .id(format!("export-format-{}", candidate.extension()))
                    .test_support()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(px(3.0))
                    .when(current, |row| row.bg(hsla(0.0, 0.0, 1.0, 0.04)))
                    .text_size(px(12.0))
                    .text_color(if current { color::ACCENT } else { color::TEXT })
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            pick_state.update(cx, |state, _| {
                                state.export_format = candidate;
                                state.export_format_open = false;
                            });
                            cx.notify();
                        }),
                    )
                    .child(candidate.label())
                    .when(current, |row| {
                        row.child(
                            Icon::default()
                                .data(icons::CHEVRON_RIGHT)
                                .size(px(12.0))
                                .text_color(color::ACCENT),
                        )
                    }),
            );
        }
        format_section = format_section.child(
            v_flex()
                .w_full()
                .rounded(px(6.0))
                .border_1()
                .border_color(color::BORDER)
                .bg(color::PANEL)
                .child(list),
        );
    }
    let description = match format {
        ExportFormat::Png => t("dialogs.export.formats.pngDesc"),
        ExportFormat::Jpeg => t("dialogs.export.formats.jpegDesc"),
        ExportFormat::WebP => t("dialogs.export.formats.webpDesc"),
    };
    format_section = format_section.child(
        div()
            .text_size(px(11.0))
            .text_color(color::TEXT_DIM)
            .child(description),
    );

    let mut body = v_flex().gap(px(16.0)).w_full().child(card);
    body = body.child(format_section);

    if format != ExportFormat::Png {
        let percent = f32::from(quality.clamp(1, 100)) / 100.0;
        let track = div().w_full().h(px(14.0)).flex().items_center().child(
            div()
                .relative()
                .w_full()
                .h(px(4.0))
                .rounded(px(2.0))
                .bg(color::BORDER)
                .child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .top(px(0.0))
                        .h(px(4.0))
                        .w(relative(percent))
                        .rounded(px(2.0))
                        .bg(color::ACCENT),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(-4.0))
                        .left(relative(percent))
                        .ml(px(-6.0))
                        .w(px(12.0))
                        .h(px(12.0))
                        .rounded_full()
                        .bg(color::ACCENT),
                ),
        );
        let (bounds, capture) = {
            let s = state.read(cx);
            (
                s.export_quality_slider_bounds.clone(),
                s.slider_capture.clone(),
            )
        };
        let quality_text = match quality {
            90..=100 => t("dialogs.export.qualities.veryHigh"),
            70..=89 => t("dialogs.export.qualities.high"),
            _ => t("dialogs.export.qualities.medium"),
        };
        body = body.child(
            v_flex()
                .w_full()
                .gap(px(6.0))
                .pt(px(16.0))
                .border_t_1()
                .border_color(color::BORDER)
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight(700.0))
                                .text_color(color::TEXT_DIM)
                                .child(t("dialogs.export.quality").to_uppercase()),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight(700.0))
                                .text_color(color::TEXT)
                                .child(format!("{quality}%")),
                        ),
                )
                .child(
                    div()
                        .id("export-quality")
                        .test_support()
                        .w_full()
                        .h(px(14.0))
                        .child(BarSliderSurface::new(
                            track,
                            cx.entity().downgrade(),
                            SliderTarget::ExportQuality,
                            bounds,
                            capture,
                            None,
                        )),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(color::TEXT_DIM)
                        .child(quality_text),
                ),
        );
    }

    body
}
/// The New Document dialog's measured box: 780 CSS wide and 36 (titlebar) +
/// 460 (body incl. footer) tall, matching `w-[780px]`/`h-[460px]`.
const NEW_DOC_W: f32 = 780.0;
const NEW_DOC_H: f32 = 496.0;
const NEW_DOC_BODY_H: f32 = 460.0;
const NEW_DOC_LEFT_W: f32 = 540.0;
const NEW_DOC_RIGHT_W: f32 = 240.0;
const NEW_DOC_TABS_H: f32 = 44.0;
const NEW_DOC_FOOTER_H: f32 = 56.0;
const NEW_DOC_CARD_H: f32 = 144.0;
const NEW_DOC_CARD_GAP: f32 = 12.0;
const NEW_DOC_GRID_PAD: f32 = 20.0;
const NEW_DOC_CARD_W: f32 =
    (NEW_DOC_LEFT_W - 2.0 * NEW_DOC_GRID_PAD - 2.0 * NEW_DOC_CARD_GAP) / 3.0;

fn new_doc_field_box(active: bool) -> Div {
    div()
        .h(px(28.0))
        .w_full()
        .flex()
        .items_center()
        .rounded(px(R4))
        .border_1()
        .border_color(if active {
            color::ACCENT
        } else {
            color::FIELD_BORDER
        })
        .bg(color::FIELD)
        .px(px(8.0))
        .text_size(px(12.0))
        .text_color(color::TEXT)
}

fn new_doc_label(key: &str) -> Div {
    div()
        .text_size(px(12.0))
        .font_weight(FontWeight(500.))
        .text_color(color::TEXT_DIM)
        .child(t(key).to_string())
}

/// `getOrientationBadge`: L/P/SQ, the card's only icon-sized mark.
fn new_doc_orientation(width: u32, height: u32) -> &'static str {
    match width.cmp(&height) {
        std::cmp::Ordering::Greater => "L",
        std::cmp::Ordering::Less => "P",
        std::cmp::Ordering::Equal => "SQ",
    }
}

/// `getCanvasStyle`: the preview keeps the preset's aspect ratio inside an
/// 80 px box with a 20 px floor.
fn new_doc_preview_size(width: u32, height: u32) -> (f32, f32) {
    let max_dim = 80.0;
    let scale = max_dim / width.max(height) as f32;
    (
        (width as f32 * scale).round().max(20.0),
        (height as f32 * scale).round().max(20.0),
    )
}

/// The oracle's preset card: a white page preview over the name, an
/// orientation badge and the pixel dimensions.
fn new_doc_preset_card(
    index: usize,
    preset: crate::state::NewDocPreset,
    selected: bool,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let (preview_w, preview_h) = new_doc_preview_size(preset.width, preset.height);
    div()
        .id(format!("new-doc-preset-{index}"))
        .test_support()
        .w(px(NEW_DOC_CARD_W))
        .h(px(NEW_DOC_CARD_H))
        .flex_shrink_0()
        .flex()
        .flex_col()
        .items_center()
        .p(px(12.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(if selected {
            color::ACCENT
        } else {
            color::DIVIDER
        })
        .bg(if selected {
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 1.0,
                a: 0.08,
            }
        } else {
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 0.15,
            }
        })
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.click_new_document_preset(index, cx);
            }),
        )
        .child(
            div()
                .flex_1()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(preview_w))
                        .h(px(preview_h))
                        .rounded(px(2.0))
                        .border_1()
                        .border_color(Hsla {
                            h: 0.0,
                            s: 0.0,
                            l: 0.0,
                            a: if selected { 0.4 } else { 0.2 },
                        })
                        .bg(Hsla {
                            h: 0.0,
                            s: 0.0,
                            l: 1.0,
                            a: if selected { 1.0 } else { 0.9 },
                        }),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .mt(px(8.0))
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight(500.))
                        .text_color(color::TEXT)
                        .child(preset.name.to_string()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .rounded(px(2.0))
                        .bg(Hsla {
                            h: 0.0,
                            s: 0.0,
                            l: 1.0,
                            a: 0.10,
                        })
                        .px(px(4.0))
                        .py(px(1.0))
                        .text_size(px(9.0))
                        .font_weight(FontWeight(500.))
                        .text_color(color::TEXT_DIM)
                        .child(new_doc_orientation(preset.width, preset.height).to_string()),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .mt(px(2.0))
                .text_size(px(11.0))
                .text_color(color::TEXT_DIM)
                .child(format!("{} × {}", preset.width, preset.height)),
        )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NewDocSelectKind {
    Unit,
    Background,
}

/// A select-style control: the field chrome plus the current label and a
/// chevron, opening a small option list below it.
fn new_doc_select(
    state: &Entity<AppState>,
    id: &'static str,
    label: String,
    open: bool,
    options: Vec<(&'static str, String, bool)>,
    center: bool,
    kind: NewDocSelectKind,
    cx: &gpui_kit::Context<crate::app::App>,
) -> Div {
    let toggle_state = state.clone();
    let mut field = new_doc_field_box(open)
        .id(id)
        .test_support()
        .cursor_pointer();
    if center {
        field = field.justify_center();
    }
    field = field.on_click(
        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            toggle_state.update(cx, |state, _| {
                if let Some(session) = state.new_document.as_mut() {
                    match kind {
                        NewDocSelectKind::Unit => {
                            session.unit_open = !session.unit_open;
                            session.background_open = false;
                        }
                        NewDocSelectKind::Background => {
                            session.background_open = !session.background_open;
                            session.unit_open = false;
                        }
                    }
                }
            });
            cx.notify();
        }),
    );
    field = field.child(
        div()
            .flex_1()
            .text_size(px(12.0))
            .text_color(color::TEXT)
            .child(label.to_string()),
    );
    field = field.child(
        Icon::default()
            .data(icons::CHEVRON_DOWN)
            .size(px(10.0))
            .text_color(color::ICON),
    );
    let mut wrapper = div().relative().w_full().child(field);
    if open {
        let mut panel = v_flex()
            .id(format!("{id}-panel"))
            .absolute()
            .top(px(30.0))
            .left(px(0.0))
            .w_full()
            .p(px(4.0))
            .gap(px(2.0))
            .rounded(px(4.0))
            .border_1()
            .border_color(color::FIELD_BORDER)
            .bg(color::PANEL)
            .shadow(vec![BoxShadow::new(
                px(0.0),
                px(8.0),
                hsla(0.0, 0.0, 0.0, 0.5),
            )
            .blur_radius(px(20.0))]);
        for (value, text, selected) in options {
            let pick_state = state.clone();
            panel = panel.child(
                div()
                    .id(format!("{id}-{value}"))
                    .test_support()
                    .h(px(24.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(3.0))
                    .bg(if selected {
                        Hsla {
                            h: 0.0,
                            s: 0.0,
                            l: 1.0,
                            a: 0.08,
                        }
                    } else {
                        color::PANEL
                    })
                    .text_size(px(11.5))
                    .text_color(if selected { color::ACCENT } else { color::TEXT })
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |_app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            pick_state.update(cx, |state, _| {
                                if let Some(session) = state.new_document.as_mut() {
                                    match kind {
                                        NewDocSelectKind::Unit => {
                                            let unit = crate::units::DimensionUnit::ALL
                                                .iter()
                                                .copied()
                                                .find(|unit| unit.label() == value)
                                                .unwrap_or_default();
                                            session.set_unit(unit);
                                        }
                                        NewDocSelectKind::Background => {
                                            session.background = if value == "transparent" {
                                                crate::state::NewDocBackground::Transparent
                                            } else {
                                                crate::state::NewDocBackground::White
                                            };
                                            session.background_open = false;
                                        }
                                    }
                                }
                            });
                            cx.notify();
                        }),
                    )
                    .child(text),
            );
        }
        wrapper = wrapper.child(panel);
    }
    wrapper
}

/// The live New Document dialog (`NewDocumentDialog.tsx`): a tabbed preset
/// grid on the left and the name/size/unit/background form on the right,
/// with the dialog's own 56 px footer.
fn new_document_body(state: &Entity<AppState>, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    use crate::state::{NewDocCategory, NewDocField};

    let session = state.read(cx).new_document.clone().unwrap_or_default();

    // Category tabs: `h-[44px]`, equal cells, active gets a 2 px underline.
    let mut tabs = h_flex()
        .w_full()
        .h(px(NEW_DOC_TABS_H))
        .flex_shrink_0()
        .border_b_1()
        .border_color(color::DIVIDER);
    for (index, category) in NewDocCategory::ALL.iter().enumerate() {
        let active = session.category == *category;
        let category = *category;
        tabs = tabs.child(
            div()
                .id(format!("new-doc-tab-{index}"))
                .test_support()
                .flex_1()
                .h_full()
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .px(px(12.0))
                .text_size(px(12.0))
                .font_weight(FontWeight(500.))
                .text_color(if active { color::TEXT } else { color::TEXT_DIM })
                .cursor_pointer()
                .on_click(
                    cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        app.select_new_doc_category(category, cx);
                    }),
                )
                .when(active, |tab| {
                    tab.child(
                        div()
                            .absolute()
                            .bottom(px(0.0))
                            .left(px(0.0))
                            .right(px(0.0))
                            .h(px(2.0))
                            .bg(color::ACCENT),
                    )
                })
                .child(t(category.label_key()).to_string()),
        );
    }

    // The 3-column grid is laid out in rows so every card keeps its measured
    // size; a wrapping flex would re-flow the oracle's fixed 3 columns.
    let presets = session.category.presets();
    let mut grid = v_flex().w_full().gap(px(NEW_DOC_CARD_GAP));
    let mut card_index = 0;
    for row in presets.chunks(3) {
        let mut row_el = h_flex().w_full().gap(px(NEW_DOC_CARD_GAP));
        for preset in row {
            row_el = row_el.child(new_doc_preset_card(
                card_index,
                *preset,
                session.preset_selected(preset),
                cx,
            ));
            card_index += 1;
        }
        grid = grid.child(row_el);
    }

    let left = v_flex()
        .w(px(NEW_DOC_LEFT_W))
        .flex_shrink_0()
        .h_full()
        .border_r_1()
        .border_color(color::DIVIDER)
        .bg(color::PANEL)
        .child(tabs)
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .overflow_hidden()
                .p(px(NEW_DOC_GRID_PAD))
                .child(grid),
        );

    // Right panel: Name, Width/Height/Unit, Background.
    let name_field = new_doc_field_box(session.field == Some(NewDocField::Name))
        .id("new-doc-name")
        .test_support()
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
            cx.stop_propagation();
            app.focus_new_doc_field(NewDocField::Name, window, cx);
        }))
        .child(format!("{}|", session.name));

    let width_field = new_doc_field_box(session.field == Some(NewDocField::Width))
        .id("new-doc-width")
        .test_support()
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
            cx.stop_propagation();
            app.focus_new_doc_field(NewDocField::Width, window, cx);
        }))
        .child(format!("{}|", session.width_text));

    let height_field = new_doc_field_box(session.field == Some(NewDocField::Height))
        .id("new-doc-height")
        .test_support()
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
            cx.stop_propagation();
            app.focus_new_doc_field(NewDocField::Height, window, cx);
        }))
        .child(format!("{}|", session.height_text));

    let unit_select = new_doc_select(
        state,
        "new-doc-unit",
        session.unit.label().to_string(),
        session.unit_open,
        DimensionUnit::ALL
            .iter()
            .map(|unit| {
                (
                    unit.label(),
                    unit.label().to_string(),
                    *unit == session.unit,
                )
            })
            .collect(),
        true,
        NewDocSelectKind::Unit,
        cx,
    );

    let unit_label = new_doc_label("dialogs.newDocument.unit");
    let background_select = new_doc_select(
        state,
        "new-doc-background",
        t(session.background.label_key()).to_string(),
        session.background_open,
        vec![
            (
                "transparent",
                t("dialogs.newDocument.transparent").to_string(),
                session.background == crate::state::NewDocBackground::Transparent,
            ),
            (
                "white",
                t("dialogs.newDocument.white").to_string(),
                session.background == crate::state::NewDocBackground::White,
            ),
        ],
        false,
        NewDocSelectKind::Background,
        cx,
    );

    let right = v_flex()
        .w(px(NEW_DOC_RIGHT_W))
        .flex_shrink_0()
        .h_full()
        .p(px(16.0))
        .gap(px(16.0))
        .bg(Hsla {
            h: color::TOPBAR.h,
            s: color::TOPBAR.s,
            l: color::TOPBAR.l,
            a: 0.30,
        })
        .child(
            v_flex()
                .gap(px(6.0))
                .child(new_doc_label("dialogs.newDocument.name"))
                .child(name_field),
        )
        .child(
            h_flex()
                .gap(px(8.0))
                .items_start()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .gap(px(6.0))
                        .child(new_doc_label("dialogs.newDocument.width"))
                        .child(width_field),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .gap(px(6.0))
                        .child(new_doc_label("dialogs.newDocument.height"))
                        .child(height_field),
                )
                .child(
                    v_flex()
                        .w(px(60.0))
                        .flex_shrink_0()
                        .gap(px(6.0))
                        .child(unit_label)
                        .child(unit_select),
                ),
        )
        .child(
            v_flex()
                .gap(px(6.0))
                .child(new_doc_label("dialogs.newDocument.background"))
                .child(background_select),
        );

    let footer = h_flex()
        .w_full()
        .h(px(NEW_DOC_FOOTER_H))
        .flex_shrink_0()
        .items_center()
        .justify_end()
        .gap(px(12.0))
        .px(px(20.0))
        .border_t_1()
        .border_color(color::DIVIDER)
        .bg(color::PANEL)
        .child(
            new_doc_button(&t("common.cancel"), false)
                .id("new-doc-cancel")
                .test_support()
                .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.close_new_document(cx);
                })),
        )
        .child(
            new_doc_button(&t("dialogs.newDocument.create"), true)
                .id("new-doc-create")
                .test_support()
                .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.create_new_document(cx);
                })),
        );

    v_flex()
        .w_full()
        .h(px(NEW_DOC_BODY_H))
        .flex_shrink_0()
        .child(
            h_flex()
                .w_full()
                .flex_1()
                .min_h(px(0.0))
                .child(left)
                .child(right),
        )
        .child(footer)
}

/// `DesktopDialogButton class="w-24 h-8"`: 96x32, 6 px radius, 11 px medium.
fn new_doc_button(label: &str, primary: bool) -> Div {
    div()
        .w(px(96.0))
        .h(px(32.0))
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
        .text_size(px(BTN_TEXT))
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
        .cursor_pointer()
        .child(label.to_string())
}

/// The measured shell for the live New Document dialog: the shared titlebar
/// plus the dialog's own 460 px body (its footer is part of that body).
fn new_document_shell(
    state: &Entity<AppState>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let stop = cx.listener(|_app, _: &gpui_kit::ClickEvent, _window, cx| {
        cx.stop_propagation();
    });
    v_flex()
        .id("new-doc-shell")
        .test_support()
        .w(px(NEW_DOC_W))
        .h(px(NEW_DOC_H))
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
        .on_click(stop)
        .child(chrome::titlebar(&t("dialogs.newDocument.title")))
        .child(new_document_body(state, cx))
}

/// Render a production dialog over the live shell.
///
/// The overlay is transparent (the frozen dialog has no scrim) and closes on a
/// click anywhere; the framed shell is centred in the window at its measured
/// size.
pub fn render_dialog(
    state: &Entity<AppState>,
    dialog: DialogId,
    document_size: Option<(u32, u32)>,
    rename_focus: &gpui_kit::FocusHandle,
    resize_focus: &gpui_kit::FocusHandle,
    new_document_focus: &gpui_kit::FocusHandle,
    dialog_focus: &gpui_kit::FocusHandle,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let state_for_close = state.clone();
    let close = cx.listener(
        move |app, _, _, cx: &mut gpui_kit::Context<crate::app::App>| {
            if dialog == DialogId::NewDocument && app.dismiss_new_document_panel(cx) {
                return;
            }
            if dialog == DialogId::DirtyClose {
                app.dirty_close_cancel(cx);
            } else {
                state_for_close.update(cx, |state, _| {
                    state.active_dialog = None;
                    state.new_document = None;
                });
                cx.notify();
            }
        },
    );
    let framed = match dialog {
        DialogId::DirtyClose => dirty_close_shell(state, cx).into_any_element(),
        DialogId::NewDocument => div()
            .id("dialog-new-document")
            .test_support()
            .child(new_document_shell(state, cx))
            .into_any_element(),
        DialogId::ResizeCanvas => {
            let close = chrome::dialog_close_button(cx.listener(
                |app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.cancel_resize_canvas(cx);
                },
            ));
            div()
                .id("dialog-resize-canvas")
                .test_support()
                .child(chrome::dialog_shell_auto_actions(
                    "Resize Canvas",
                    360.0,
                    resize_canvas_body(state, cx),
                    vec![
                        dialog_action_button(
                            &t("common.cancel"),
                            false,
                            "resize-cancel",
                            cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                                cx.stop_propagation();
                                app.cancel_resize_canvas(cx);
                            }),
                        )
                        .into_any_element(),
                        dialog_action_button(
                            &t("dialogs.resizeCanvas.resize"),
                            true,
                            "resize-confirm",
                            cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                                cx.stop_propagation();
                                match app.apply_resize_canvas(cx) {
                                    Ok(()) => app.set_status_error(None, cx),
                                    Err(error) => {
                                        app.set_status_error(Some(format!("Resize: {error}")), cx)
                                    }
                                }
                            }),
                        )
                        .into_any_element(),
                    ],
                    close,
                ))
                .into_any_element()
        }
        DialogId::Rename => div()
            .id("dialog-rename")
            .test_support()
            .child(dialog_shell(
                "Rename layer",
                360.0,
                180.0,
                rename_body(state, cx),
            ))
            .into_any_element(),
        DialogId::Print => print::render(state, document_size, cx),
        DialogId::Export => {
            let cancel = dialog_action_button(
                &t("common.cancel"),
                false,
                "export-cancel",
                cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.close_active_dialog(cx);
                }),
            )
            .into_any_element();
            let confirm = dialog_action_button(
                &t("dialogs.export.exportBtn"),
                true,
                "export-confirm",
                cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.export_now(cx);
                }),
            )
            .into_any_element();
            let close = chrome::dialog_close_button(cx.listener(
                |app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.close_active_dialog(cx);
                },
            ));
            div()
                .id("dialog-export")
                .test_support()
                .child(chrome::dialog_shell_auto_actions(
                    &t("dialogs.export.title"),
                    360.0,
                    export_body(state, document_size, cx),
                    vec![cancel, confirm],
                    close,
                ))
                .into_any_element()
        }
        DialogId::Recovery => div()
            .id("dialog-recovery")
            .test_support()
            .child(dialog_shell(
                "Recover unsaved work", // i18n-exempt: recovery dialog copy
                380.0,
                300.0,
                recovery_body(state, cx),
            ))
            .into_any_element(),
        DialogId::Settings => {
            let Some((_, title, _, _, w, _h)) =
                DIALOGS.iter().find(|(id, ..)| *id == "dialog-settings")
            else {
                return div().into_any_element();
            };
            let _ = w;
            let ok = dialog_action_button(
                &t("common.ok"),
                true,
                "settings-ok",
                cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.close_settings(cx);
                }),
            )
            .into_any_element();
            let close = chrome::dialog_close_button(cx.listener(
                |app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.close_settings(cx);
                },
            ));
            div()
                .id("dialog-settings")
                .test_support()
                .child(chrome::dialog_shell_auto_actions(
                    &t(title),
                    420.0,
                    settings_body_live(state, cx),
                    vec![ok],
                    close,
                ))
                .into_any_element()
        }
        DialogId::About => {
            let close = dialog_action_button(
                &t("common.close"),
                true,
                "about-close",
                cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.close_active_dialog(cx);
                    cx.notify();
                }),
            )
            .into_any_element();
            let dismiss = chrome::dialog_close_button(cx.listener(
                |app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.close_active_dialog(cx);
                },
            ));
            div()
                .id("dialog-about")
                .test_support()
                .child(chrome::dialog_shell_auto_actions(
                    &t("dialogs.about.title"),
                    400.0,
                    dialog_body("dialog-about", 400.0, 0.0),
                    vec![close],
                    dismiss,
                ))
                .into_any_element()
        }
    };
    // `fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2`: the live
    // dialog is centred in the window, which is what the 1366x820 shot's
    // measured origin (293,161) is the result of.
    div()
        .id("dialog-overlay")
        .test_support()
        .absolute()
        .inset(px(0.0))
        .flex()
        .items_center()
        .justify_center()
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
        .when(dialog == DialogId::NewDocument, |overlay| {
            overlay
                .track_focus(new_document_focus)
                .on_key_down(
                    cx.listener(|app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                        app.new_document_key(event, cx);
                    }),
                )
        })
        // Settings and About have no field of their own; the source still
        // routes Escape through `DesktopDialog`'s managed key handler.
        .when(
            matches!(dialog, DialogId::Settings | DialogId::About),
            |overlay| {
                overlay.track_focus(dialog_focus).on_key_down(cx.listener(
                    |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                        if event.keystroke.key == "escape" {
                            app.dismiss_chrome_dialog(cx);
                        }
                    },
                ))
            },
        )
        // The dirty-close prompt is modal: a click outside must not discard
        // the user's decision. Other dialogs keep click-anywhere dismissal.
        .when(dialog == DialogId::DirtyClose, |overlay| {
            overlay.on_click(|_, _, _| {})
        })
        .when(dialog != DialogId::DirtyClose, |overlay| {
            overlay.on_click(close)
        })
        // The backdrop owns dismissal; a press on the dialog's own surface
        // (padding, body text, a slider) must not bubble out to it. Every
        // control inside already stops propagation; this covers the rest.
        .child(
            div()
                .id("dialog-surface")
                .test_support()
                .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                .child(framed),
        )
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
mod print;
#[cfg(test)]
mod tests;

use about::about_body;
#[cfg(test)]
use about::{
    about_version, ABOUT_LINK_GITHUB, ABOUT_LINK_WEBSITE, ABOUT_SPEC_LABELS, ABOUT_SPEC_VALUES,
};
use chrome::{dialog_action_button, dialog_shell, dialog_shell_with_actions};
use content::dialog_body;
