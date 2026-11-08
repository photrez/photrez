//! The dialogs, rendered through the shared dialog chrome.
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
//! The design centres a dialog at the viewport midpoint. GPUI has no element
//! offset, so the centring is not recomputed here: the manifest already
//! records the *result* of that centring, and each dialog is pinned at its
//! measured origin. That
//! also sidesteps the whole class of error where this view re-derives a centre
//! from a size and lands half a pixel off.
//!
//! # There is no scrim
//!
//! This is the most surprising measurement in the module, and it contradicts
//! what a dialog is assumed to have: the backdrop is
//! The design makes the backdrop a full-viewport transparent layer. The shell
//! behind a dialog is drawn at full brightness. So this view does not dim
//! anything, and `dialog-resize-full.png` \u{2014} the same dialog with the shell
//! behind it \u{2014} is the baseline that proves it.
//!
//! # The chrome, measured in `dialog-about.png` (400x296)
//!
//! ```text
//!   shell     10px radius, 1px FIELD_BORDER edge, PANEL fill,
//!             shadow 0 12px 32px rgba(0,0,0,0.45)
//!   titlebar  36px   TOPBAR fill, 1px bottom DIVIDER border, 12px side inset
//!             brand svg 18px + 12px medium title + 20px close button
//!   body      PANEL, 16px padding, 12px text, 18px line height
//!   actions   45px   (44px min-height + 1px top border)  TOPBAR at 65%,
//!             12px side inset
//!             buttons 28px tall, 64px min-width, 6px radius, 12px side
//!             inset, 11px medium text
//! ```
//!
//! Reading the colours down a column at x=200 gives 20 (TOPBAR) for the
//! titlebar, 30 (PANEL) for the body, and 23 for the footer \u{2014} and 23 is
//! TOPBAR at 65% composited over PANEL, not a token of its own. That
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
//! * The dialog buttons' keyboard-focus outline, their 75 ms colour
//!   transition and the dialog fade-in animation are still unwired. The
//!   Rename, Resize Canvas and New Document text fields are tab stops with the
//!   shared focus band, so `Tab` moves real focus and the keystrokes follow it.
//!   Focus is still not trapped inside the dialog: `Tab` past the last field
//!   leaves for the shell behind it. The titlebar close,
//!   Escape for Settings/About, and the About link row are live:
//!   `chrome::dialog_close_button` runs each dialog's own dismissal, and
//!   `dismiss_chrome_dialog` owns the managed-focus Escape. A click on the
//!   backdrop does not dismiss the dialog.
//! * A non-modal dialog (the colour picker) makes the backdrop click-through so
//!   the canvas behind stays live. A static view cannot express "this overlay
//!   does not block the thing under it", because it has no pointer routing.
//! * Icon edges carry LCD subpixel colour GPUI cannot reproduce: it tints an
//!   alpha mask, and the font-smoothing hint covers type only, not SVG.

use gpui_kit::base::{h_flex, v_flex, FocusTrapElement};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{
    div, hsla, px, relative, AnimationExt, BoxShadow, Div, Entity, FontWeight, Hsla, TestSupportExt,
};

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

/// 36px \u{2014} the titlebar's height.
const TITLEBAR_H: f32 = 36.0;

/// 44px min-height plus the 1px top border \u{2014} the footer's measured height in
/// `dialog-about.png` (y=249..294).
const ACTIONS_H: f32 = 45.0;

/// 8px / 6px radius \u{2014} shell and buttons.
const SHELL_R: f32 = 10.0;

/// 4px / 6px radius inside the dialog bodies. Named here
/// rather than imported because `ui.rs` keeps RADIUS private to the panel
/// primitives, and the two actually differ: the panel field is 4px, this one
/// is also 4px but the lock button above it is 6px.
const R4: f32 = 4.0;
/// The second body radius: 6px against [`R4`]'s 4px, consumed by the About
/// dialog's spec panel.
const R6: f32 = 6.0;
const BTN_R: f32 = 6.0;

/// 12px on the titlebar and footer, 20px horizontal and vertical on the body.
const BAR_PX: f32 = 12.0;
const BODY_PX: f32 = 20.0;
const BODY_PY: f32 = 20.0;

/// 28px tall, 64px min-width \u{2014} a footer button.
const BTN_H: f32 = 28.0;
const BTN_MIN_W: f32 = 64.0;

/// 20px / 10px \u{2014} the close button and its X.
const CLOSE_BOX: f32 = 20.0;
const CLOSE_ICON: f32 = 10.0;

/// 18px \u{2014} the brand mark in the titlebar.
const BRAND: f32 = 18.0;

/// 12px title, 11px buttons, 12px text with an 18px line height for the
/// body.
const TITLE_TEXT: f32 = 12.0;
const BTN_TEXT: f32 = 11.0;
const BODY_TEXT: f32 = 12.0;
#[expect(
    dead_code,
    reason = "body line-height pin from `leading-[18px]`; the reference's `resize_body` uses relative(1.35) instead of this constant"
)]
const BODY_LINE: f32 = 18.0;

/// 8px between the brand mark and the title, and between footer buttons.
const GAP_2: f32 = 8.0;

/// The footer's fill: TOPBAR at 65% over the dialog root, held as the design's
/// own ingredient rather than a pre-blended number. TOPBAR is `#141414` (20) and
/// PANEL `#1e1e1e` (30), so the renderer's blend gives 0.65*20 + 0.35*30 = 23.5,
/// which is what the scan reads. Pre-blending it and keeping the alpha blended
/// twice, and left the footer 2/255 light.
const FOOTER_FILL: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0784,
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
/// instead, and this view picks that up through `render()`, which already routes
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

/// 52px - the label column in ResizeCanvasModal's dimension rows, and
/// the 52px left indent of the lock row under them.
const RESIZE_LABEL_W: f32 = 52.0;

/// 60px, never squeezed - the unit picker beside each field.
const RESIZE_UNIT_W: f32 = 60.0;

/// 30px \u{2014} the shared dialog field height as recorded. Deliberately NOT
/// [`ui::FIELD_H`] (24px): the panel's NumField and the dialogs' inputs are
/// different chrome and measuring one and assuming the other is how the two
/// got confused in the first place.
const DIALOG_FIELD_H: f32 = 30.0;
const DIALOG_FIELD_TEXT: f32 = 12.0;

/// 10px between the description, the two dimension rows and the lock row;
/// 8px inside a dimension row.
const RESIZE_GAP: f32 = 10.0;
const RESIZE_ROW_GAP: f32 = 8.0;

/// 10px horizontal on a dialog input.
const FIELD_PAD_DIALOG: f32 = 10.0;

/// The seven dialogs, with their measured origins.
///
/// The second field is the dialog title: a catalog key, resolved with [`t`] at
/// the render site, or the literal where the shipped catalogue has no match.
///
/// A capture is the authority for each of these: the origins are the manifest's
/// boxes, not arithmetic on the recorded centring transform, because
/// shifting a box back by half its own size has no GPUI equivalent and
/// re-deriving it would introduce exactly the half-pixel error the
/// measurement avoids.
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
                dialog_shell(&t(title), *w, *h, dialog_body(owned, *w, *h), None),
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

/// The live Settings body: the Interface section with the
/// language dropdown. The list expands inline because a popup deferred inside
/// a dialog overlay never receives hits (measured during the units work).
fn settings_body_live(state: &Entity<AppState>, cx: &gpui_kit::Context<crate::app::App>) -> Div {
    use crate::i18n::{self, Locale};
    let current = i18n::locale();
    let open = state.read(cx).open_select == Some("settings-language");
    let current_index = match current {
        Locale::Id => 1,
        Locale::Bjn => 2,
        _ => 0,
    };
    let current_label = match current {
        Locale::Id => "Bahasa Indonesia",
        Locale::Bjn => "Bahasa Banjar",
        _ => "English",
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
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.toggle_select_stay("settings-language", current_index, 3, cx);
            }),
        )
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
    let mut wrapper = div().relative().child(trigger);
    if open {
        // The panel floats over the dialog instead of expanding inline: it
        // paints late (deferred), above the dialog's own chrome, and takes
        // its clicks itself.
        let options = [
            "English".to_string(),
            "Bahasa Indonesia".to_string(),
            "Bahasa Banjar".to_string(),
        ];
        wrapper = wrapper.child(gpui_kit::deferred(
            crate::views::option_bar::controls::floating_options_panel(
                "settings-language",
                &options,
                current_index,
                28.0 + 4.0,
                0.0,
                220.0,
                cx,
            ),
        ));
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
        .child(h_flex().pl(px(8.0)).child(wrapper))
}

/// Render a production dialog over the live shell.
///
/// The overlay is transparent (the dialog has no scrim) and ignores a
/// click outside; the framed shell is centred in the window at its measured
/// size.
// Distinct focus handles keep each dialog's keyboard target explicit.
#[allow(clippy::too_many_arguments)]
pub fn render_dialog(
    state: &Entity<AppState>,
    dialog: DialogId,
    document_size: Option<(u32, u32)>,
    rename_focus: &gpui_kit::FocusHandle,
    resize_focus: &gpui_kit::FocusHandle,
    new_document_focus: &gpui_kit::FocusHandle,
    dialog_focus: &gpui_kit::FocusHandle,
    window: &mut gpui_kit::Window,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let framed = match dialog {
        DialogId::DirtyClose => dirty_close_shell(state, dialog, cx).into_any_element(),
        DialogId::NewDocument => div()
            .id("dialog-new-document")
            .test_support()
            .child(new_document_shell(state, dialog, cx))
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
                    Some((cx, dialog)),
                ))
                .into_any_element()
        }
        DialogId::Rename => div()
            .id("dialog-rename")
            .test_support()
            .child(dialog_shell_live(
                "Rename layer",
                360.0,
                180.0,
                rename_body(state, cx),
                dialog_close_button(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.cancel_rename_layer(cx);
                })),
                Some((cx, dialog)),
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
                    Some((cx, dialog)),
                ))
                .into_any_element()
        }
        DialogId::Recovery => {
            let close = chrome::dialog_close_button(cx.listener(
                |app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.close_active_dialog(cx);
                },
            ));
            div()
                .id("dialog-recovery")
                .test_support()
                .child(chrome::dialog_shell_auto_actions(
                    &t("dialogs.recovery.title"),
                    380.0,
                    recovery_body(state, cx),
                    recovery_actions(cx),
                    close,
                    Some((cx, dialog)),
                ))
                .into_any_element()
        }
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
                    Some((cx, dialog)),
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
                    Some((cx, dialog)),
                ))
                .into_any_element()
        }
    };
    // The recorded centring rule: the live
    // dialog is centred in the window, which is what the 1366x820 shot's
    // measured origin (293,161) is the result of.
    let viewport = window.viewport_size();
    let (limit_x, limit_y) = drag::offset_limit(viewport);
    let (offset_x, offset_y) = drag::offset(dialog);
    let offset_x = offset_x.clamp(-limit_x, limit_x);
    let offset_y = offset_y.clamp(-limit_y, limit_y);
    div()
        .id("dialog-overlay")
        .test_support()
        .absolute()
        .inset(px(0.0))
        .flex()
        .items_center()
        .justify_center()
        // The modal curtain: a press anywhere on the overlay is the modal's
        // business and stops here, so no surface under the dialog - canvas,
        // navigator, dock - starts a gesture behind it. The dialog's own
        // controls and the titlebar drag run first as inner handlers.
        .on_mouse_down(
            gpui_kit::MouseButton::Left,
            cx.listener(|_, _, _, cx| cx.stop_propagation()),
        )
        .when(dialog == DialogId::Rename, |overlay| {
            overlay.track_focus(rename_focus).on_key_down(cx.listener(
                |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                    app.rename_key(event, cx);
                },
            ))
        })
        .when(dialog == DialogId::ResizeCanvas, |overlay| {
            overlay.track_focus(resize_focus).on_key_down(cx.listener(
                |app, event: &gpui_kit::KeyDownEvent, window, cx| {
                    app.resize_key(event, window, cx);
                },
            ))
        })
        .when(dialog == DialogId::NewDocument, |overlay| {
            overlay
                .track_focus(new_document_focus)
                .on_key_down(
                    cx.listener(|app, event: &gpui_kit::KeyDownEvent, window, cx| {
                        app.new_document_key(event, window, cx);
                    }),
                )
        })
        // Settings and About have no field of their own; the recorded build still
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
        // the user's decision. Dialogs elsewhere follow the same rule, so
        // dismissal is only through their own controls.
        .on_click(|_, _, _| {})
        .on_mouse_move(
            cx.listener(|_app, event: &gpui_kit::MouseMoveEvent, window, cx| {
                if event.pressed_button != Some(gpui_kit::MouseButton::Left) && drag::end_drag() {
                    cx.notify();
                    return;
                }
                if drag::drag_move(event.position, window.viewport_size()) {
                    cx.notify();
                }
            }),
        )
        .capture_any_mouse_up(
            cx.listener(|_app, _event: &gpui_kit::MouseUpEvent, _window, cx| {
                if drag::end_drag() {
                    cx.notify();
                }
            }),
        )
        // A press on the dialog's own surface (padding, body text, a slider)
        // must not bubble out to the backdrop. Every control inside already
        // stops propagation; this covers the rest.
        // The trap host keeps Tab inside the panel: the recorded cycle wrapped
        // at its ends, and here focus cannot leave at all.
        .child(
            div()
                .id("dialog-surface")
                .test_support()
                .left(px(offset_x))
                .top(px(offset_y))
                .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                .child(match state.read(cx).dialog_trap_focus.clone() {
                    Some(trap) => div()
                        .child(framed)
                        .focus_trap("dialog-trap", &trap)
                        .into_any_element(),
                    None => framed.into_any_element(),
                }),
        )
        // The design fades the dialog panel in over 200ms. The backdrop this
        // group paints is transparent, so fading the group reaches the panel and
        // nothing else.
        .with_animation(
            "dialog-fade-in",
            gpui_kit::Animation::new(std::time::Duration::from_millis(200))
                .with_easing(gpui_kit::ease_out_quint()),
            |overlay, progress| overlay.opacity(progress),
        )
        .into_any_element()
}

/// The dirty-close prompt: Save, Discard, or Cancel.
///
/// The document-close confirm offers Discard/Cancel only; the
/// native prompt adds Save because the save path must stay reachable
/// from the close decision. Geometry follows the settings dialog's measured
/// frame (420x171 at 473,324).
fn dirty_close_shell(
    state: &Entity<AppState>,
    dialog: DialogId,
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
    let close = dialog_close_button(cx.listener(|app, _, _, cx| {
        cx.stop_propagation();
        app.dirty_close_cancel(cx);
    }));
    div()
        .id("dialog-dirty-close")
        .test_support()
        .child(dialog_shell_with_actions_live(
            &t("dialogs.unsavedChanges.title"),
            420.0,
            171.0,
            body,
            vec![discard, cancel, save],
            close,
            Some((cx, dialog)),
        ))
}

/// Pin a dialog at the origin the manifest recorded, rather than re-deriving
/// the recorded centre-in-the-viewport placement.
///
/// Shifting a box back by half its own size has no GPUI equivalent, and the
/// manifest box is already the *result* of that transform, so pinning the
/// measured origin is both simpler and more accurate than recomputing a centre
/// from a size.
fn positioned(x: f32, y: f32, dialog: Div) -> Div {
    dialog.absolute().left(px(x)).top(px(y))
}

mod about;
mod chrome;
mod content;
mod drag;
mod export;
mod new_document;
mod print;
mod recovery;
mod rename;
mod resize;
#[cfg(test)]
mod tests;

use about::about_body;
#[cfg(test)]
use about::{
    about_version, ABOUT_LINK_GITHUB, ABOUT_LINK_WEBSITE, ABOUT_SPEC_LABELS, ABOUT_SPEC_VALUES,
};
use chrome::{
    dialog_action_button, dialog_close_button, dialog_shell, dialog_shell_live,
    dialog_shell_with_actions_live,
};
use content::dialog_body;
use export::export_body;
use new_document::new_document_shell;
use recovery::{recovery_actions, recovery_body};
use rename::rename_body;
use resize::resize_canvas_body;
#[cfg(test)]
use resize::resize_pill_colours;
