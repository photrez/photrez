//! Visual-only port of `layers/LayersPanel.tsx` and `layers/LayerItem.tsx`.
//!
//! # Geometry: 260 wide, x 1106..1366, y 84..610 (527 tall)
//!
//! This is the LayerDock column, and **527 is not 712**: the dock's second
//! column is 260px wide and the Navigator takes a fixed 185px off the bottom,
//! so the panel above it is the remainder. `manifest.json` pins both anchors —
//! `shell-navigator` is 260x185 at (1106,611) — and 611 - 84 = 527. Measured
//! the same way, `hover-layer-item` is 254x50 at (1106,165), which says two
//! separate things at once:
//!
//! * A layer row is **50px** tall (`h-[50px]`, `LayerItem.tsx:177`).
//! * A row is **254px**, not 260 — the 6px is `scrollbar-gutter: stable`
//!   (`styles.css:321`) reserving room for a scrollbar that never appears,
//!   over a `overflow-y-auto` list (`LayersPanel.tsx:643`). GPUI has no
//!   scrollbars so nothing reserves it; transcribed as `.pr(px(6.))`.
//!
//! The headers above the list are 81px, and that figure is a check rather than
//! a constant: 84 (panel top) + 12 (`pt-3`) + 26 (`h-[26px]` select) + 12
//! (`py-3`) + 16 (the `size-4` lock buttons) + 12 = 182, plus the list's
//! `border-y` = 183, and the baseline's first row starts at **165 + 46 - 46 =
//! 165**, i.e. 165 from the panel's own top of 84 → 81px of header. The
//! arithmetic lands within the line-height slack, which is as close as this
//! measurement can resolve.
//!
//! # What fills the space below the row: nothing — and a 38px footer
//!
//! The list is `flex-1 overflow-y-auto border-y border-editor-divider`
//! (`LayersPanel.tsx:643`) and its content is `<For each={layers()}>`. The
//! captured document holds one layer, so the rows stop after 50px and the
//! remaining **357px** — baseline y 215..571 — is bare `#1e1e1e`. That
//! emptiness is the source's own rather than an omission: nothing at all is
//! rendered between the last row and the footer, so the list stays `flex_1`
//! with the column stretched to the window and simply has nothing to paint.
//!
//! What the port was missing is the block *after* that run. Two full-width
//! rules and a 37px band sit at its bottom:
//!
//! ```text
//!   y 572        #333333   the list's own border-y, bottom edge
//!   y 573        #333333   the Layer Actions footer's border-t
//!   y 574..610   (37px)    the footer, five 17px icons on #1e1e1e
//!   y 611        #333333   the Navigator's border-t (right_dock.rs)
//! ```
//!
//! So the "two dividers" are a 1px pair — the list's `border-b` immediately
//! followed by the footer's `border-t`, with no gap between them — and 37px is
//! exactly `py-2.5 + size-[17px] + py-2.5` (`LayersPanel.tsx:746`). The icons
//! measure x 1122, 1159, 1196, 1233 (`px-4` then `gap-5` = 17 + 20) with
//! Delete at 1333..1349, which is `1366 - 16 - 17` under `ml-auto`; the peak
//! ink `#868686` is `text-editor-icon`. Both rules read `#333333` and not
//! `#2e2e2e`, because the unlayered `* { border-color }` rule outranks
//! `border-editor-divider` — see `tokens::color::BORDER`.
//!
//! The footer is the only thing below the list: `dragGhost`, the context menu
//! and the opacity popover are all `Show`-gated on state a resting capture
//! never has.
//!
//! # The column stretches
//!
//! `RightDock.tsx:415` gives this panel `flex min-h-0 flex-1 flex-col`, so the
//! root is `flex_1().min_h(0())` rather than a pinned height: 527 is the height
//! it measures *at the baseline window* (`611 - 84`), and pinning it left a
//! band of bare column below the Navigator's frame on any taller window. The
//! list is the flex child that absorbs the difference, which is what makes the
//! empty run grow instead of the footer moving.
//!
//! # What is a placeholder
//!
//! The layer *list* is rendered as a single row, because the port has no
//! document state and therefore one layer to show. The source maps over
//! `layers()`, which could be any length; the baseline capture happens to show
//! one active row, and that is what is reproduced. `active:bg-editor-row-active
//! ring-1 ring-inset ring-editor-accent/40` is applied — the active row is the
//! one visible, and the ring is a colour plus a 1px inset border, which GPUI
//! can draw.
//!
//! The reorder chevrons are `opacity-0 group-hover:opacity-100`
//! (`LayerItem.tsx:296`) — invisible at rest, so they are correctly absent from
//! both the baseline and this port rather than merely omitted.
//!
//! # Gaps
//!
//! * `LayerThumb` is canvas-drawn (`LayerThumb.tsx`'s `createEffect` +
//!   `drawImage`), so the row's thumbnail pixels are not reproducible; its
//!   34x34 frame, border and checkerboard are. See `properties_panel.rs` for
//!   the same treatment and why `checkerboard()` is drawn as one flat colour.
//! * The blend-mode `<select>` is a native control. Its resting chrome is
//!   drawn; its popup is not, because no baseline captures it open.
//! * `Tooltip` wrappers on all four lock buttons and the eye toggle, plus
//!   `transition-all duration-75` — interaction only.
//! * `disabled:opacity-50` / `opacity-30` / `opacity-35` are states a static
//!   render cannot distinguish from the enabled resting pixels.
//! * `eye_toggle` from the reference is not ported: nothing at rest renders
//!   it (it is not wired into any row), so it would be unreachable dead code.

use crate::assets::icons;
use crate::components::ui;
use crate::tokens::color;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Div, FontWeight, Hsla};

/// LayerDock's column width: `shell-navigator` is 260 wide at x=1106 and the
/// dock runs to the window edge.
const PANEL_W: f32 = 260.0;

/// Height above the Navigator at the baseline window: 611 - 84.
///
/// Kept as the measured figure rather than as a layout input — the root is
/// `flex_1` now (module docs, "The column stretches"), so this is what the
/// column works out to at 1366x820 and what a capture at any other height
/// grows or shrinks from.
#[expect(
    dead_code,
    reason = "documents baseline geometry; root is flex_1 and absorbs window height"
)]
const PANEL_H: f32 = 527.0;

/// `px-3.5` — both header rows' horizontal inset.
const PAD_X: f32 = 14.0;

/// `pt-3` / `py-3`.
const PAD_T: f32 = 12.0;

/// `gap-2` on the first header row, `gap-4` on the second, `gap-2.5` on a row.
const GAP_2: f32 = 8.0;
const GAP_4: f32 = 16.0;
const ROW_GAP: f32 = 10.0;

/// `h-[26px] w-[120px]` — the blend-mode select.
const SELECT_H: f32 = 26.0;
const SELECT_W: f32 = 120.0;

/// `size-4` — the lock buttons' box, holding a 15px icon.
const LOCK_BOX: f32 = 16.0;
const LOCK_ICON: f32 = 15.0;

/// `h-[50px]` — a layer row.
const ROW_H: f32 = 50.0;

/// `size-[34px]` — a row's `LayerThumb`.
const THUMB: f32 = 34.0;

/// The mean of `LayerThumb`'s two checkerboard tones, as one flat colour.
///
/// The source alternates `rgba(255,255,255,0.05)` and `rgba(255,255,255,0.1)`
/// (`LayerThumb.tsx:22-28`); GPUI's `checkerboard()` is not a `Styled` method,
/// and would take one colour anyway. See `properties_panel.rs` for the full
/// derivation of this value.
const CHECKER_MEAN: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.208,
    a: 1.0,
};

/// `rounded-[4px]` / `rounded-[3px]`.
const R4: f32 = 4.0;
const R3: f32 = 3.0;

/// `text-[11px] font-semibold` — the select's label.
const SELECT_TEXT: f32 = 11.0;

/// `text-[12px]` — "Lock:", the opacity readout.
const TEXT_12: f32 = 12.0;

/// `text-[12.5px]` — a layer row's name.
const ROW_TEXT: f32 = 12.5;

/// `gap-5` — between the Layer Actions footer's icons.
const FOOT_GAP: f32 = 20.0;

/// `px-4` — the footer's horizontal inset.
const FOOT_PAD_X: f32 = 16.0;

/// `py-2.5` — the footer's vertical inset, which is what makes it 10 + 17 + 10
/// = 37px plus its `border-t`.
const FOOT_PAD_Y: f32 = 10.0;

/// `size-[17px]` — each of the footer's five icons (`LayersPanel.tsx:753`).
const FOOT_ICON: f32 = 17.0;

/// `strokeWidth={1.75}` — explicit on all five footer icons; the `icons::`
/// constants are baked at 1.5, 1.6, 1.75 or 2 depending on where they were
/// first used, so the footer asks for this one by name.
const STROKE: f32 = 1.75;

/// The single placeholder layer's name (no document state — module docs,
/// "What is a placeholder").
pub const LAYER_NAME: &str = "Layer 1";

// --- Lock-header icon table (4 buttons, in order) ---
pub const LOCK_BUTTONS: &[&[u8]] = &[
    icons::UNLOCK,
    icons::PAINT_BUCKET,
    icons::MAXIMIZE,
    icons::ROTATE,
];

// --- Footer icon table ---
// `trailing` marks Delete, the only button the source pushes right with
// `ml-auto` (`LayersPanel.tsx:790`): measured at x 1333..1349 = 1366 - 16
// (`px-4`) - 17, with the four on the left at 1122, 1159, 1196, 1233
// (`gap-5` = 17 + 20 between each).
pub struct FooterIcon {
    pub svg: &'static [u8],
    pub trailing: bool,
}

pub const FOOTER_ICONS: &[FooterIcon] = &[
    FooterIcon {
        svg: icons::PLUS,
        trailing: false,
    },
    FooterIcon {
        svg: icons::COPY,
        trailing: false,
    },
    FooterIcon {
        svg: icons::CHEVRON_DOWN,
        trailing: false,
    },
    FooterIcon {
        svg: icons::SQUARE_DASHED,
        trailing: false,
    },
    FooterIcon {
        svg: icons::TRASH,
        trailing: true,
    },
];

/// `<div id="right-dock-layers-panel" class="flex min-h-0 flex-1 flex-col
/// overflow-hidden bg-editor-panel">` (`LayersPanel.tsx:415`).
///
/// The root is `flex_1().min_h(0())`, not a pinned height: it owns whatever
/// the column has between the 38px tab bar and the Navigator, and the list
/// inside it is the part that grows. See the module docs for why the run
/// below the single row is empty and what the pair of rules at its foot is.
pub fn render() -> impl IntoElement {
    v_flex()
        .w(px(PANEL_W))
        .flex_1()
        .min_h(px(0.0))
        .overflow_hidden()
        .bg(color::PANEL)
        .child(blend_header())
        .child(lock_header())
        .child(layer_list())
        .child(actions_footer())
}

/// `flex items-center gap-2 px-3.5 pt-3` — the blend select and the opacity
/// readout, pushed to the right by `ml-auto`.
fn blend_header() -> Div {
    h_flex()
        .w_full()
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP_2))
        .px(px(PAD_X))
        .pt(px(PAD_T))
        .child(blend_select())
        .child(
            h_flex()
                .ml_auto()
                .items_center()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(TEXT_12))
                        .font_family(ui::FONT_UI)
                        .text_color(color::TEXT_DIM)
                        .child("Opacity".to_string()),
                )
                .child(
                    div()
                        .text_size(px(TEXT_12))
                        .font_family(ui::FONT_UI)
                        .font_weight(FontWeight(500.))
                        .text_color(color::TEXT)
                        .child("100%".to_string()),
                )
                .child(
                    Icon::default()
                        .data(icons::CHEVRON_DOWN)
                        .size(px(14.0))
                        .text_color(color::ICON),
                ),
        )
}

/// The native `<select>`'s resting chrome: `h-[26px] w-[120px] rounded-[4px]
/// border border-editor-field-border bg-editor-field px-2 text-[11px]
/// font-semibold`, with a chevron standing in for the control's own arrow.
fn blend_select() -> Div {
    h_flex()
        .w(px(SELECT_W))
        .h(px(SELECT_H))
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .rounded(px(R4))
        .border(px(1.0))
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .px(px(GAP_2))
        // Same split as the status bar: the value is the text that gives way,
        // the chevron is the mark that stays whole.
        .child(ui::clip(
            div()
                .text_size(px(SELECT_TEXT))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(600.))
                .text_color(color::TEXT)
                .child("Normal".to_string()),
        ))
        .child(ui::pin(
            Icon::default()
                .data(icons::CHEVRON_DOWN)
                .size(px(12.0))
                .text_color(color::FIELD_CAPTION),
        ))
}

/// `flex items-center gap-4 px-3.5 py-3` — the "Lock:" label and four
/// `size-4` icon buttons.
///
/// All four carry a `Tooltip` wrapper, which a static port cannot show, so the
/// icons stand alone. Their resting colour is `text-editor-text-dim`; the
/// locked variants tint ACCENT, and none of them is captured in the baseline's
/// resting state, so they are drawn unlocked.
fn lock_header() -> Div {
    h_flex()
        .w_full()
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP_4))
        .px(px(PAD_X))
        .py(px(PAD_T))
        .child(
            div()
                .text_size(px(TEXT_12))
                .font_family(ui::FONT_UI)
                .text_color(color::TEXT_DIM)
                .child("Lock:".to_string()),
        )
        .child(
            h_flex()
                .items_center()
                .gap(px(GAP_4))
                .children(LOCK_BUTTONS.iter().map(|&svg| lock_button(svg))),
        )
}

/// A `size-4` centring box holding a 15px icon at stroke 1.75.
fn lock_button(icon: &'static [u8]) -> Div {
    div()
        .w(px(LOCK_BOX))
        .h(px(LOCK_BOX))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .child(
            Icon::default()
                .data(icon)
                .size(px(LOCK_ICON))
                .text_color(color::TEXT_DIM),
        )
}

/// `flex-1 overflow-y-auto border-y border-editor-divider` — the layer stack.
///
/// The `pr` is the reserved scrollbar gutter; see the module docs on why 254
/// and 260 are both correct answers here. `border_y` is [`color::BORDER`] and
/// not [`color::DIVIDER`]: the baseline reads `#333333` on both edges (y=164
/// and y=572), the `* { border-color }` rule being what reaches the screen for
/// any `border-*` utility.
fn layer_list() -> Div {
    v_flex()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .overflow_hidden()
        .pr(px(6.0))
        .border_y(px(1.0))
        .border_color(color::BORDER)
        .child(layer_row())
}

/// `flex h-[50px] items-center gap-2.5 px-3.5 border-b border-editor-divider/10`
/// with the active row's `bg-editor-row-active` and its 1px inset ACCENT ring.
///
/// A `ring-1 ring-inset ring-editor-accent/40` has no GPUI equivalent — rings
/// are a box-shadow family GPUI does not expose — so the 1px inset edge is
/// drawn as a border and the colour as `ACCENT` at 40% alpha. That is the
/// closest available reading of "a faint inner outline".
fn layer_row() -> Div {
    h_flex()
        .w_full()
        .h(px(ROW_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(ROW_GAP))
        .px(px(PAD_X))
        .bg(color::ROW_ACTIVE)
        .border(px(1.0))
        .border_color(Hsla {
            h: color::ACCENT.h,
            s: color::ACCENT.s,
            l: color::ACCENT.l,
            a: 0.4,
        })
        .child(layer_thumb())
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(ROW_TEXT))
                .font_family(ui::FONT_UI)
                .text_color(color::TEXT)
                .child(LAYER_NAME.to_string()),
        )
}

/// `flex shrink-0 items-center gap-5 border-t border-editor-divider
/// bg-editor-panel px-4 py-2.5 text-editor-icon` — the Layer Actions footer
/// (`LayersPanel.tsx:746-796`).
///
/// It is what turns the two rules at the foot of the empty run into a pair:
/// its `border-t` paints the row directly under the list's `border-y`, so
/// y 572 and y 573 are both `#333333` with nothing between them, and its own
/// height is fixed by the source rather than by anything in the port — `py-2.5`
/// (10) + `size-[17px]` + `py-2.5` (10) = 37, plus the border, i.e. the 38px
/// that ends the panel at y=610 with the Navigator's `border-t` at 611.
///
/// Every button is `Tooltip`-wrapped and carries a `hover:` tint — interaction
/// a resting capture cannot show — and the `disabled:opacity-30` states are not
/// distinguishable from the enabled resting pixels here, so all five glyphs
/// are drawn at full [`color::ICON`] (`text-editor-icon`, measured `#868686`).
fn actions_footer() -> Div {
    h_flex()
        .w_full()
        .flex_shrink_0()
        .items_center()
        .gap(px(FOOT_GAP))
        .px(px(FOOT_PAD_X))
        .py(px(FOOT_PAD_Y))
        .border_t(px(1.0))
        .border_color(color::BORDER)
        .bg(color::PANEL)
        .children(FOOTER_ICONS.iter().map(|fi| {
            if fi.trailing {
                div().ml_auto().child(footer_icon(fi.svg))
            } else {
                footer_icon(fi.svg)
            }
        }))
}

/// One footer glyph: the source's `<button>` has no dimensions of its own, so
/// it is exactly the `size-[17px]` of the `Icon` inside it.
///
/// `strokeWidth={1.75}` is explicit on all five (`LayersPanel.tsx:753` etc.),
/// and the `icons::` constants are baked at whatever the source used where
/// they were first introduced, so this asks [`icons::with_stroke`] rather than
/// assuming the constant already agrees — the same call `history_panel` makes.
fn footer_icon(icon: &'static [u8]) -> Div {
    div()
        .w(px(FOOT_ICON))
        .h(px(FOOT_ICON))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .child(
            Icon::default()
                .data(icons::with_stroke(icon, STROKE))
                .size(px(FOOT_ICON))
                .text_color(color::ICON),
        )
}

/// `LayerThumb`'s frame: 34x34, 1px border, `rounded-[3px]`, checker inside.
///
/// The canvas bitmap is a *layer preview*, which a static port cannot have —
/// so this is the empty chrome, exactly as in `properties_panel.rs`.
fn layer_thumb() -> Div {
    div()
        .w(px(THUMB))
        .h(px(THUMB))
        .rounded(px(R3))
        .border(px(1.0))
        .border_color(color::DIVIDER)
        .bg(CHECKER_MEAN)
        .flex_shrink_0()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_row_name_pinned() {
        assert_eq!(LAYER_NAME, "Layer 1");
    }

    #[test]
    fn footer_icons_match_reference() {
        assert_eq!(FOOTER_ICONS.len(), 5);
        assert_eq!(FOOTER_ICONS[0].svg, icons::PLUS);
        assert!(!FOOTER_ICONS[0].trailing);
        assert_eq!(FOOTER_ICONS[1].svg, icons::COPY);
        assert!(!FOOTER_ICONS[1].trailing);
        assert_eq!(FOOTER_ICONS[2].svg, icons::CHEVRON_DOWN);
        assert!(!FOOTER_ICONS[2].trailing);
        assert_eq!(FOOTER_ICONS[3].svg, icons::SQUARE_DASHED);
        assert!(!FOOTER_ICONS[3].trailing);
        assert_eq!(FOOTER_ICONS[4].svg, icons::TRASH);
        assert!(FOOTER_ICONS[4].trailing);
    }

    #[test]
    fn lock_buttons_match_reference() {
        assert_eq!(LOCK_BUTTONS.len(), 4);
        assert_eq!(LOCK_BUTTONS[0], icons::UNLOCK);
        assert_eq!(LOCK_BUTTONS[1], icons::PAINT_BUCKET);
        assert_eq!(LOCK_BUTTONS[2], icons::MAXIMIZE);
        assert_eq!(LOCK_BUTTONS[3], icons::ROTATE);
    }
}
