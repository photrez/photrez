//! The window title bar, including its closed-state menu bar.
//!
//! # Scope: resting state only
//!
//! Baselines: `shell-titlebar.png` (1366x46) and `shell-menubar.png` (8 triggers).
//! Static captures omit:
//! * menu popups (their stacking order of 100, keyboard navigation)
//! * the system menu on Alt+Space
//! * the 0.1 shell's drag-region attribute and double-click-to-maximize
//! * window state (a maximized window swaps the maximize icon)
//!
//! Hover states are included as part of the component contract.
//!
//! # Three fidelity gaps
//!
//! ## 1. Letter-spacing (GPUI gap; measured)
//!
//! The menu bar's labels are 12px with wide tracking: 0.025em of
//! letter-spacing = 0.3px after every glyph, including the last. GPUI has no
//! letter-spacing field: `LineLayout`/`ShapedGlyph` and `Styled` carry none.
//!
//! Inline triggers accumulate width deficits. The reference origins are the
//! `menu-*` boxes in `tests/baseline/manifest.json` (dropdown x equals trigger
//! x); the native column below is this bar's own rendered origins.
//!
//! ```text
//! trigger    original  native    delta   glyphs
//! File         58.00      58    0.00       4
//! Edit         99.88      99   -0.88       4
//! Image       144.00     142   -2.00       5
//! Layer       201.47     198   -3.47       5
//! View        253.94     249   -4.94       4
//! Window      303.44     298   -5.44       6
//! Help        372.44     367   -5.44       4
//! Settings    421.20     415   -6.20       -
//! ```
//!
//! Ink starts 10.0px inside each trigger ([`title::MENU_PX`]); File
//! at x=58 with delta 0 rules out an earlier offset.
//!
//! Before Settings, 32 glyphs show 0.194px/glyph drift. Wide tracking adds
//! 0.3px/glyph (−9.6px total); GPUI's shaper adds +0.106px/glyph vs the previous renderer,
//! character-dependent (Window/Help level; Image/Layer differ). Net drift is
//! −0.19px, not −0.3px; restoring tracking overshoots by roughly +3px.
//!
//! A prior +9px compensation overshot (`HARD 0.127%`; target `−6px / HARD
//! 0.099%`): fractional `px()` padding rounds 10.6→11, adding ~0.8px per
//! trigger before tracking. Keep the plain 10px padding; retries must use the rendered
//! localized label (`t(MENU_LABEL_KEYS[menu])`), not fractional padding.
//!
//! The remaining −6px (`HARD 0.099%`, one hard pixel) is cosmetic and local.
//! Per-glyph error compounds in content-width rows (menus/tabs); centered title
//! and wordmark tracking shift only 0.15px.
//!
//! ## 2. Logo rendering
//!
//! `Icon` uses `paint_svg`, which keeps alpha but tints monochrome SVGs with
//! `text_color`. The Photrez mark's two linear gradients, mask and
//! `feDropShadow` would flatten; `img(ImageSource::Render(..))` uses
//! `render_single_frame` and keeps full colour.
//!
//! ## 3. Icon hover tint (uncompensated; unverified)
//!
//! As in `tool_rail`, `Icon` resolves tint before hover styles: undo/redo/window
//! icons do not brighten, only their button backgrounds. No baseline captures
//! title-bar hover.
//!
//! # Per-platform window controls
//!
//! The native title bar is hidden, so this bar owns the controls using each
//! platform's convention: AppKit traffic lights on macOS, no controls on
//! server-decorated Linux, and Windows `WindowControlArea` hit regions.
//! `window_controls::window_controls` implements the branch;
//! `title_bar::render` passes `&mut Window` so it can query decorations.

use std::sync::{Arc, OnceLock};

use gpui_kit::base::h_flex;
use gpui_kit::component::Icon;
// `StyledExt` is what supplies `.h_flex()` as a *method*. The free function
// `base::h_flex()` only builds a bare `Div`, but `div().id(..)` returns
// `Stateful<Div>` — the id is what makes the button hit-testable, and therefore
// what makes `.hover()` fire — and that type needs the trait in scope.
// (`gpui_kit::base` is `pub use ::gpui_base as base`, so the trait lives there
// alongside `h_flex`/`v_flex`.)
use gpui_kit::base::StyledExt;
use gpui_kit::prelude::*;
use gpui_kit::{
    div, img, px, Entity, FontWeight, ImageSource, RenderImage, SvgRenderer, TestSupportExt, Window,
};

use crate::components::ui;
use crate::i18n::t;
use crate::icons;
use crate::state::{AppState, MenuId};
use crate::tokens::{color, title};

/// The eight top-level menus, in the order the 0.1 menu bar lists them,
/// as catalog keys resolved with [`t`] at the trigger.
const MENUS: &[&str] = &[
    "menus.file",
    "menus.edit",
    "menus.image",
    "menus.layer",
    "menus.view",
    "menus.window",
    "menus.help",
    "menus.settings",
];

/// The logo raster. Filled by [`init`] before any window opens, so [`logo`]
/// can rely on it being present.
static LOGO_IMAGE: OnceLock<Arc<RenderImage>> = OnceLock::new();

/// Rasterises the brand mark once before the first frame. GPUI's
/// `SMOOTH_SVG_SCALE_FACTOR` (2) turns the 30x30 logo into a 60x60 raster,
/// matching `paint_svg`'s 2x icon supersampling.
///
/// Panics with the underlying `usvg` error rather than rendering nothing: a
/// silently missing logo would show up as a confusing diff, and if usvg cannot
/// handle this SVG the project needs a different strategy for it.
#[expect(
    clippy::panic,
    reason = "logo raster failure is startup-fatal; a silently missing logo is a defect"
)]
pub fn init(svg: &SvgRenderer) {
    let image = svg
        .render_single_frame(icons::LOGO, 1.0)
        .unwrap_or_else(|e| {
            panic!(
                "the Photrez logo SVG failed to rasterise: {e}\n\
                 gradient/mask/filter support is the likely culprit"
            )
        });
    let _ = LOGO_IMAGE.set(image);
}

/// The bar: a positioned 46px row, never squeezed, children centred; 1px
/// bottom border, topbar fill, text unselectable, content clipped to its box.
///
/// `window` reaches `right_group` so it can query client-side decorations on
/// Linux without forking the signature by platform.
pub fn render(
    window: &mut Window,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    h_flex()
        .relative()
        .h(px(title::HEADER_H))
        .flex_shrink_0()
        // `h_flex()` already sets `items_center`; the explicit call documents
        // the cross-axis alignment unless a child opts into `.self_stretch()`.
        .items_center()
        .border_b_1()
        // The divider token names #2e2e2e, but the global border override
        // outranks it and paints #333333 — see `color::BORDER`,
        // which records the four baselines proving it.
        .border_color(color::BORDER)
        .bg(color::TOPBAR)
        .overflow_hidden()
        // The whole bar is Inter. GPUI's theme default is `.SystemUIFont`,
        // which resolves to Segoe UI here — a different font with different
        // advances, which would move every trigger in the menu bar.
        .font_family(ui::FONT_UI)
        .child(left_group(state, cx))
        .child(center_group(state, cx))
        .child(right_group(window, state, cx))
}

/// The logo cell plus the menu bar: one row, centred, 8px apart, stretched to
/// the bar's height, allowed to shrink below its content.
fn left_group(state: &Entity<AppState>, cx: &Context<crate::app::App>) -> impl IntoElement {
    h_flex()
        .min_w(px(0.))
        .items_center()
        .gap(px(title::LEFT_GAP))
        .self_stretch()
        .child(logo_cell())
        .child(menu_bar(state, cx))
}

/// The logo cell: one full-height, un-squeezed row, centred, 12px left and
/// 8px right padding. Being full-height is why the wrapper is stretched rather
/// than centred: it is
/// what gives the logo cell the bar's full 45px content height to centre in.
fn logo_cell() -> impl IntoElement {
    // On macOS the traffic lights own the top-left corner, so the logo starts
    // after them instead of at the 12px inset. See `title::LOGO_PAD_L_MACOS` for why
    // and for the fact that the clearance is an estimate.
    let pad_left = if cfg!(target_os = "macos") {
        title::LOGO_PAD_L_MACOS
    } else {
        title::LOGO_PAD_L
    };

    h_flex()
        .self_stretch()
        .flex_shrink_0()
        .items_center()
        .pl(px(pad_left))
        .pr(px(title::LOGO_PAD_R))
        .child(logo(title::LOGO))
}

/// The logo: a 30px square, never squeezed, full colour from the 512x512
/// artwork - see the module docs for why this is `img` and not `Icon`.
///
/// `size` is the box requested by the bar or About dialog; both share the same
/// raster and gradient/mask/filter artwork.
#[expect(clippy::expect_used)] // init() is the parent's wiring responsibility; a missing raster is a programmer error, not a runtime one
pub(crate) fn logo(size: f32) -> impl IntoElement {
    let image = LOGO_IMAGE
        .get()
        .expect("title_bar::init() must run before the window opens");

    img(ImageSource::Render(image.clone()))
        .size(px(size))
        .flex_shrink_0()
}

/// The menu bar: one full-height row, centred, 2px between triggers.
///
/// The reference hides this bar below its medium breakpoint and shows it as a
/// row from there up: the baseline is 1366px wide, past the 768px breakpoint.
fn menu_bar(state: &Entity<AppState>, cx: &Context<crate::app::App>) -> impl IntoElement {
    h_flex()
        .self_stretch()
        .items_center()
        .gap(px(title::MENU_GAP))
        .children(
            MENUS
                .iter()
                .copied()
                .map(|key| menu_trigger(key, state, cx)),
        )
}

/// One menu trigger: a positioned, full-height, centred row wrapping
/// the button itself.
///
/// `key` is the catalog key: it names the element id and resolves the visible
/// label, so the id stays locale-independent while the text follows the locale.
///
/// The button is not given a popup — see the module docs for why the open
/// state is out of scope.
fn menu_trigger(
    key: &'static str,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let menu = menu_id(key);
    let state_for_click = state.clone();
    let on_click = cx.listener(move |_, _, _, cx: &mut Context<crate::app::App>| {
        let Some(menu) = menu else {
            return;
        };
        state_for_click.update(cx, |state, _| {
            state.open_menu = if state.open_menu == Some(menu) {
                None
            } else {
                Some(menu)
            };
        });
        cx.notify();
    });
    h_flex().relative().self_stretch().items_center().child(
        div()
            .id(key)
            .test_support()
            .on_click(on_click)
            .h_flex()
            .h(px(title::MENU_H))
            .items_center()
            .justify_center()
            .rounded(px(title::RADIUS))
            .px(px(pad_x(key)))
            .text_size(px(title::MENU_TEXT))
            // Medium weight.
            .font_weight(FontWeight(500.))
            // Text at 90%.
            .text_color(color::TEXT_90)
            // On hover: white at 4.5% behind it, full-strength text.
            .hover(|s| s.bg(color::WHITE_45).text_color(color::TEXT))
            .child(t(key)),
    )
}

fn menu_id(key: &str) -> Option<MenuId> {
    match key {
        "menus.file" => Some(MenuId::File),
        "menus.edit" => Some(MenuId::Edit),
        "menus.image" => Some(MenuId::Image),
        "menus.layer" => Some(MenuId::Layer),
        "menus.view" => Some(MenuId::View),
        "menus.window" => Some(MenuId::Window),
        "menus.help" => Some(MenuId::Help),
        "menus.settings" => Some(MenuId::Settings),
        _ => None,
    }
}

/// A plain 10px inset; tracking compensation is omitted because the gap is
/// character-dependent (see the module docs).
fn pad_x(_label: &str) -> f32 {
    title::MENU_PX
}

/// The centre cell: a full-height centred row that takes the spare width above a 16px floor.
///
/// The 0.1 shell's drag-region attribute and its double-click handler have no
/// native equivalent here; the cell is still a plain centred child.
fn center_group(state: &Entity<AppState>, cx: &Context<crate::app::App>) -> impl IntoElement {
    let document_name = state.read(cx).document_name.clone();
    h_flex()
        .min_w(px(title::CENTER_MIN_W))
        .flex_1()
        .items_center()
        .justify_center()
        .self_stretch()
        .child(
            h_flex()
                .items_center()
                .gap(px(title::CENTER_GAP))
                // The 90% opacity applies to the row, not each span.
                .opacity(0.9)
                .child(
                    div()
                        .text_size(px(title::MENU_TEXT))
                        // Semibold.
                        .font_weight(FontWeight(600.))
                        .text_color(color::TEXT)
                        // The tight tracking here remains uncompensated; this centered
                        // row limits the difference to a 0.15px offset.
                        .child(document_name),
                )
                .child(
                    div()
                        .text_size(px(title::SEP_TEXT))
                        .text_color(color::TEXT_DIM_60)
                        .child("\u{b7}"), // i18n-exempt: glyph or symbol
                )
                .child(
                    div()
                        .text_size(px(title::WORDMARK_TEXT))
                        .font_weight(FontWeight(500.))
                        // Dim text at 80% composites to ~3.2:1 over
                        // the topbar; the wordmark uses full dim strength.
                        // The separator stays at 60% as decorative punctuation.
                        .text_color(color::TEXT_DIM)
                        .child("photrez"), // i18n-exempt: product name
                ),
        )
}

/// The right-hand group: a shrinkable centred row of undo/redo and window controls.
///
/// The side-panel toggle is hidden at large widths: it exists below
/// 1024px and is absent from the 1366px baseline. The branch is decided from
/// the live window width so the 74-shot reference comparison is unchanged.
fn right_group(
    window: &mut Window,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let dock_open = state.read(cx).dock_open;
    h_flex()
        .min_w(px(0.))
        .items_center()
        .child(undo_group(dock_open, window, state, cx))
        .child(window_controls(window))
}

/// The undo/redo group: a clipped centred row, 2px between buttons, 12px right margin, icon colour.
///
/// At the 1366px baseline the panel toggle is hidden (it only shows below the
/// 1024px breakpoint). The 81px group
/// (28 + 2 + 28 + 2 + 4 + 1 + 4 + 12) puts undo at x=1153 (1366 - 81 - 3*44),
/// matching `manifest.json`.
fn undo_group(
    dock_open: bool,
    window: &Window,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    h_flex()
        .mr(px(title::UNDO_MR))
        .items_center()
        .gap(px(title::UNDO_GAP))
        .overflow_hidden()
        .text_color(color::ICON)
        .child(icon_button("undo", icons::UNDO))
        .child(icon_button("redo", icons::REDO))
        .child(rule())
        .when(window.bounds().size.width < px(1024.0), |row| {
            row.child(panel_toggle(dock_open, state, cx))
        })
}

/// A 28px square with 4px corners and a white-at-4.5% hover fill, shown below 1024px.
fn panel_toggle(
    dock_open: bool,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let state = state.clone();
    div()
        .id("panel-toggle")
        .test_support()
        .h_flex()
        .size(px(title::UNDO_BTN))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(title::RADIUS))
        .hover(|s| s.bg(color::WHITE_45).text_color(color::TEXT))
        .on_click(cx.listener(move |_app, _event, _window, cx| {
            state.update(cx, |state, _| state.dock_open = !state.dock_open);
            cx.notify();
        }))
        .child(
            Icon::default()
                .data(if dock_open {
                    icons::PANEL_RIGHT_CLOSE
                } else {
                    icons::PANEL_RIGHT_OPEN
                })
                .size(px(title::ICON_16))
                .text_color(color::ICON),
        )
}

/// An undo/redo button: a 28px square with 4px corners, centred on one row,
/// that gains white at 4.5% behind it and the plain text colour on hover.
fn icon_button(id: &'static str, icon: &'static [u8]) -> impl IntoElement {
    div()
        .id(id)
        .h_flex()
        .size(px(title::UNDO_BTN))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(title::RADIUS))
        .hover(|s| s.bg(color::WHITE_45).text_color(color::TEXT))
        .child(
            Icon::default()
                .data(icon)
                .size(px(title::ICON_16))
                // The tint is decided here rather than inherited, because
                // `Icon` reads `window.text_style()` while the tree is built —
                // before hover refinement. Same constraint as `tool_rail`.
                .text_color(color::ICON),
        )
}

/// The rule between redo and the controls: 1px wide, 14px tall, 4px side margin, divider fill.
fn rule() -> impl IntoElement {
    div()
        .mx(px(title::RULE_MX))
        .h(px(title::RULE_H))
        .w(px(1.))
        .bg(color::DIVIDER)
}

mod window_controls;
use window_controls::window_controls;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_x_returns_menu_px_for_any_label() {
        assert_eq!(pad_x("File"), title::MENU_PX);
        assert_eq!(pad_x("Settings"), title::MENU_PX);
        assert_eq!(pad_x(""), title::MENU_PX);
    }

    #[test]
    fn menus_array_is_reference_order() {
        assert_eq!(
            MENUS,
            &[
                "menus.file",
                "menus.edit",
                "menus.image",
                "menus.layer",
                "menus.view",
                "menus.window",
                "menus.help",
                "menus.settings",
            ]
        );
        assert_eq!(t(MENUS[0]), "File");
        assert_eq!(t(MENUS[7]), "Settings");
    }
}
