//! Visual-only port of `layers/LayersPanel.tsx` and `layers/LayerItem.tsx`.
//!
//! # Geometry: 260 wide, x 1106..1366, y 84..610 (527 tall)
//!
//! `manifest.json` anchors `shell-navigator` at 260x185 at (1106,611) and
//! `hover-layer-item` at 254x50 at (1106,165). The 260px LayerDock column's
//! fixed 185px Navigator leaves 527, not 712: 611 - 84 = 527.
//!
//! * A layer row is **50px** tall (`h-[50px]`, `LayerItem.tsx:177`).
//! * A row is **254px**, not 260 — the 6px is `scrollbar-gutter: stable`
//!   (`styles.css:321`) reserving room for a scrollbar that never appears,
//!   over a `overflow-y-auto` list (`LayersPanel.tsx:643`). GPUI has no
//!   scrollbars so nothing reserves it; transcribed as `.pr(px(6.))`.
//!
//! Header: 84 (panel top) + 12 (`pt-3`) + 26 (`h-[26px]` select) + 12 (`py-3`)
//!   plus 16 (`size-4` lock buttons) + 12 = 182; list `border-y` makes 183. The
//!   baseline row starts at **165 + 46 - 46 = 165**, 81px below top 84.
//!
//! # What fills the space below the row: nothing — and a 38px footer
//!
//! `flex-1 overflow-y-auto border-y border-editor-divider`
//! (`LayersPanel.tsx:643`) holds `<For each={layers()}>`. One captured layer
//! ends after 50px; y 215..571 is **357px** of `#1e1e1e`, with no content before
//! the footer.
//!
//! ```text
//!   y 572        #333333   the list's own border-y, bottom edge
//!   y 573        #333333   the Layer Actions footer's border-t
//!   y 574..610   (37px)    the footer, five 17px icons on #1e1e1e
//!   y 611        #333333   the Navigator's border-t (right_dock.rs)
//! ```
//!
//! The dividers are a touching 1px pair (`border-b`, `border-t`). Footer:
//! `py-2.5 + size-[17px] + py-2.5` = 37px (`LayersPanel.tsx:746`); icons at
//! x 1122, 1159, 1196, 1233 (`px-4`, `gap-5` = 17 + 20); Delete at 1333..1349
//! (`1366 - 16 - 17`, `ml-auto`). Peak ink `#868686` is `text-editor-icon`.
//! Unlayered `* { border-color }` outranks `border-editor-divider`: both rules
//! read `#333333`, not `#2e2e2e` (`tokens::color::BORDER`).
//!
//! `dragGhost`, context menu, and opacity popover are `Show`-gated; the footer
//! is the only content below the list in the resting capture.
//!
//! # The column stretches
//!
//! `RightDock.tsx:415` assigns `flex min-h-0 flex-1 flex-col`; the column is
//! `flex_1().min_h(0())`, not a pinned height. Pinning left a bare band below
//! the Navigator on taller windows; the list absorbs the difference.
//!
//! # The list reads live state
//!
//! One row per [`AppState::layers`] entry; the baseline's single row leaves
//! resting geometry unchanged. The row whose `id` equals
//! [`AppState::selected_layer`] gets `active:bg-editor-row-active ring-1
//! ring-inset ring-editor-accent/40` (40% alpha, a drawable 1px inset border).
//!
//! # Interactions
//!
//! `.on_click` selects; `.on_drag` starts a reorder with [`rows::LayerDragGhost`]
//! visible only during drag; `.on_drop::<u64>` moves the row to the target index
//! and selects it. Reordering is UI state only: no history or document mutation,
//! not undoable yet.
//!
//! The reorder chevrons are `opacity-0 group-hover:opacity-100`
//! (`LayerItem.tsx:296`), invisible at rest and absent from the baseline and port.
//!
//! # Gaps
//!
//! * `LayerThumb` is canvas-drawn (`LayerThumb.tsx`: `createEffect` + `drawImage`);
//!   pixels are unreproducible, but its 34x34 frame, border, and checkerboard
//!   are. `properties_panel.rs` uses the same flat-colour `checkerboard()`.
//! * Native blend-mode `<select>`: resting chrome drawn, popup not captured open.
//! * `Tooltip` wrappers on four lock buttons and the eye toggle, plus
//!   `transition-all duration-75`, are interaction-only.
//! * `disabled:opacity-50` / `opacity-30` / `opacity-35` are indistinguishable
//!   from enabled resting pixels.
//! * Reference `eye_toggle` is unported and unreachable; no row wires it.

use crate::components::ui;
use crate::i18n::t;
use crate::icons;
use crate::state::{AppState, LayerRow};
use crate::tokens::color;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{
    div, px, AppContext, Context, Div, Entity, FontWeight, Hsla, Point, TestSupportExt, Window,
};

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

pub const LOCK_BUTTONS: &[&[u8]] = &[
    icons::UNLOCK,
    icons::PAINT_BUCKET,
    icons::MAXIMIZE,
    icons::ROTATE,
];

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
pub fn render(
    state: &Entity<AppState>,
    layer_thumbs: &[Option<std::sync::Arc<gpui_kit::RenderImage>>],
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    // One read per frame; the row list is cloned so the handlers below can
    // mutate the same entity while this tree is built.
    let (layers, selected, selected_layers, flags) = {
        let s = state.read(cx);
        (
            s.layers.clone(),
            s.selected_layer,
            s.selected_layers.clone(),
            s.layer_flags.clone(),
        )
    };
    v_flex()
        .w(px(PANEL_W))
        .flex_1()
        .min_h(px(0.0))
        .overflow_hidden()
        .bg(color::PANEL)
        .child(blend_header(state, cx))
        .child(lock_header())
        .child(layer_list(
            &layers,
            &flags,
            &selected_layers,
            selected,
            layer_thumbs,
            state,
            cx,
        ))
        .child(actions_footer(cx))
}

/// `flex items-center gap-2 px-3.5 pt-3` — the blend select and the opacity
/// readout, pushed to the right by `ml-auto`.
fn blend_header(state: &Entity<AppState>, cx: &Context<crate::app::App>) -> Div {
    let (blend, opacity) = {
        let s = state.read(cx);
        (s.layer_blend.clone(), s.layer_opacity)
    };
    h_flex()
        .w_full()
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP_2))
        .px(px(PAD_X))
        .pt(px(PAD_T))
        .child(blend_select(blend, cx))
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
                        .child(t("tools.options.opacity")),
                )
                .child(opacity_button("layer-opacity-dec", "-", cx, -0.1))
                .child(
                    div()
                        .id("layer-opacity-value")
                        .test_support()
                        .text_size(px(TEXT_12))
                        .font_family(ui::FONT_UI)
                        .font_weight(FontWeight(500.))
                        .text_color(color::TEXT)
                        .child(format!("{:.0}%", opacity * 100.0)),
                )
                .child(opacity_button("layer-opacity-inc", "+", cx, 0.1)),
        )
}

fn opacity_button(
    id: &'static str,
    label: &'static str,
    cx: &Context<crate::app::App>,
    delta: f64,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .w(px(16.0))
        .h(px(16.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.0))
        .border_1()
        .border_color(color::BORDER)
        .text_size(px(10.0))
        .text_color(color::TEXT_DIM)
        .cursor_pointer()
        .hover(|style| style.text_color(color::TEXT))
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                app.adjust_selected_opacity(delta, cx);
            }),
        )
        .child(label)
}

/// The native `<select>`'s resting chrome: `h-[26px] w-[120px] rounded-[4px]
/// border border-editor-field-border bg-editor-field px-2 text-[11px]
/// font-semibold`, with a chevron standing in for the control's own arrow.
fn blend_select(label: String, cx: &Context<crate::app::App>) -> impl IntoElement {
    h_flex()
        .id("layer-blend-cycle")
        .test_support()
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
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            app.cycle_selected_blend(cx);
        }))
        // Same split as the status bar: the value is the text that gives way,
        // the chevron is the mark that stays whole.
        .child(ui::clip(
            div()
                .text_size(px(SELECT_TEXT))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(600.))
                .text_color(color::TEXT)
                .child(label),
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
                .child(t("layers.lock")),
        )
        .child(
            h_flex()
                .items_center()
                .gap(px(GAP_4))
                .children(LOCK_BUTTONS.iter().map(|&svg| lock_button(svg))),
        )
}

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

/// `flex shrink-0 items-center gap-5 border-t border-editor-divider
/// bg-editor-panel px-4 py-2.5 text-editor-icon` — the Layer Actions footer
/// (`LayersPanel.tsx:746-796`).
///
/// Tooltips, hover tint and `disabled:opacity-30` are interaction-only; resting
/// captures show all five glyphs at measured `#868686`.
fn actions_footer(cx: &Context<crate::app::App>) -> Div {
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
        .children(FOOTER_ICONS.iter().enumerate().map(|(index, fi)| {
            let button = footer_icon(fi.svg)
                .id(format!("layer-action-{index}"))
                .test_support()
                .cursor_pointer()
                .on_click(cx.listener(
                    move |app: &mut crate::app::App, _: &gpui_kit::ClickEvent, _window, cx| {
                        match index {
                            0 => app.add_empty_layer(cx),
                            1 => {
                                let result = app.duplicate_selected_layer(cx);
                                app.report_layer_op_for_view(result, cx);
                            }
                            2 => {
                                let result = if app.selected_layer_count(cx) > 1 {
                                    app.merge_selected_layers(cx)
                                } else {
                                    app.merge_selected_layer_down(cx)
                                };
                                app.report_layer_op_for_view(result, cx);
                            }
                            3 => {
                                let result = app.flatten_document(cx);
                                app.report_layer_op_for_view(result, cx);
                            }
                            _ => app.delete_selected_layer(cx),
                        }
                    },
                ));
            if fi.trailing {
                div().ml_auto().child(button).into_any_element()
            } else {
                button.into_any_element()
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

#[cfg(test)]
mod tests {
    use super::*;

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

mod rows;
use rows::layer_list;
