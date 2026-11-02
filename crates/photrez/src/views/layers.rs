//! The Layers panel and its rows: their appearance.
//!
//! # Geometry: 260 wide, x 1106..1366, y 84..610 (527 tall)
//!
//! `manifest.json` anchors `shell-navigator` at 260x185 at (1106,611) and
//! `hover-layer-item` at 254x50 at (1106,165). The 260px LayerDock column's
//! fixed 185px Navigator leaves 527, not 712: 611 - 84 = 527.
//!
//! * A layer row is **50px** tall.
//! * A row is **254px**, not 260 — the 6px is the scroll gutter reserving room
//!   for a scrollbar that never appears
//!   over the scrollable list. GPUI has no
//!   scrollbars so nothing reserves it; transcribed as `.pr(px(6.))`.
//!
//! Header: 84 (panel top) + 12 (top padding) + 26 (select height) + 12
//!   (vertical padding)
//!   plus 16 (lock buttons) + 12 = 182; the list's top and bottom borders make
//!   183. The
//!   baseline row starts at **165 + 46 - 46 = 165**, 81px below top 84.
//!
//! # What fills the space below the row: nothing — and a 38px footer
//!
//! The scrollable list, bordered top and bottom, holds one row per layer.
//! One captured layer
//! ends after 50px; y 215..571 is **357px** of `#1e1e1e`, with no content before
//! the footer.
//!
//! ```text
//!   y 572        #333333   the list's own bottom border
//!   y 573        #333333   the Layer Actions footer's top border
//!   y 574..610   (37px)    the footer, five 17px icons on #1e1e1e
//!   y 611        #333333   the Navigator's top border (right_dock.rs)
//! ```
//!
//! The dividers are a touching 1px pair. Footer:
//! 10px + 17px + 10px = 37px; icons at
//! x 1122, 1159, 1196, 1233 (16px inset, 20px gap, so 17 + 20 apart); Delete at
//! 1333..1349 (`1366 - 16 - 17`, pushed to the trailing edge).
//! Peak ink `#868686` is the icon colour.
//! Every border here resolves to `#333333`, not the `#2e2e2e` divider value
//! (`tokens::color::BORDER`).
//!
//! The drag ghost, context menu, and opacity popover only mount while active;
//! the footer
//! is the only content below the list in the resting capture.
//!
//! # The column stretches
//!
//! The dock gives this column a flexible minimum-height cell; the column is
//! `flex_1().min_h(0())`, not a pinned height. Pinning left a bare band below
//! the Navigator on taller windows; the list absorbs the difference.
//!
//! # The list reads live state
//!
//! One row per [`AppState::layers`] entry; the baseline's single row leaves
//! resting geometry unchanged. The row whose `id` equals
//! [`AppState::selected_layer`] gets the active-row fill and a 40%-alpha accent
//! ring drawn as a 1px inset border.
//!
//! # Interactions
//!
//! `.on_click` selects; `.on_drag` starts a reorder with `rows::LayerDragGhost`
//! visible only during drag; `.on_drop::<u64>` moves the row to the target index
//! and selects it. Drops and the row's up/down chevrons both go through
//! `App::reorder_layer_rows`, which records the move in core history.
//!
//! The reorder chevrons are hidden at rest and revealed by hovering the row —
//! invisible but occupying their row slots,
//! carrying the `layers.moveLayerUp/Down`
//! tooltips and dimmed to 20% at the two ends of the stack.
//!
//! # Gaps
//!
//! * The layer thumbnail is canvas-drawn;
//!   pixels are unreproducible, but its 34x34 frame, border, and checkerboard
//!   are. `properties_panel.rs` uses the same flat-colour `checkerboard()`.
//! * Native blend-mode dropdown: resting chrome drawn, popup not captured open.
//! * The header's four lock toggles carry their tooltips, accent tint
//!   and disabled states; only the colour transition and the hover
//!   text tint are left out. The row eye toggle carries its Hide/Show Layer
//!   tooltip; the row lock exposes an accessible name instead of a
//!   tooltip, and there is no row rename button at all (renaming starts
//!   from the name), so the native pen is an addition.
//! * The 50% / 30% / 35% disabled opacities are indistinguishable
//!   from enabled resting pixels.
//! * The adjustments indicator draws from
//!   `PtzLayer::has_adjustments`; the projection reads the serialized flag and
//!   does not derive it from adjustment values itself.

use crate::components::ui;
use crate::i18n::t;
use crate::icons;
use crate::state::{AppState, LayerRow};
use crate::tokens::color;
use crate::views::option_bar::controls::tooltip;

use photrez_core::protocol::LockKind;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{
    div, px, AppContext, Context, Div, Entity, FontWeight, Hsla, Point, TestSupportExt, Window,
};

/// LayerDock's column width: `shell-navigator` is 260 wide at x=1106 and the
/// dock runs to the window edge.
/// The drag ghost keeps the pre-fill width; the panel itself fills its dock
/// column (298 px at 2xl) instead of pinning this value.
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

/// 14px — both header rows' horizontal inset.
const PAD_X: f32 = 14.0;

/// 12px top / 12px vertical inset.
const PAD_T: f32 = 12.0;

/// 8px on the first header row, 16px on the second, 10px on a row.
const GAP_2: f32 = 8.0;
const GAP_4: f32 = 16.0;
const ROW_GAP: f32 = 10.0;

/// 26px × 120px — the blend-mode select.
const SELECT_H: f32 = 26.0;
const SELECT_W: f32 = 120.0;

/// 16px — the lock buttons' box, holding a 15px icon.
const LOCK_BOX: f32 = 16.0;
const LOCK_ICON: f32 = 15.0;

/// 50px — a layer row.
const ROW_H: f32 = 50.0;

/// 34px — a row's `LayerThumb`.
const THUMB: f32 = 34.0;

/// The mean of `LayerThumb`'s two checkerboard tones, as one flat colour.
///
/// The original alternates `rgba(255,255,255,0.05)` and `rgba(255,255,255,0.1)`
/// in 6px cells; GPUI's `checkerboard()` is not a `Styled` method,
/// and would take one colour anyway. See `properties_panel.rs` for the full
/// derivation of this value.
const CHECKER_MEAN: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.208,
    a: 1.0,
};

/// 4px / 3px corner radii.
const R4: f32 = 4.0;
const R3: f32 = 3.0;

/// 11px semibold — the select's label.
const SELECT_TEXT: f32 = 11.0;

/// 12px — "Lock:", the opacity readout.
const TEXT_12: f32 = 12.0;

/// 12.5px — a layer row's name.
const ROW_TEXT: f32 = 12.5;

/// 20px — between the Layer Actions footer's icons.
const FOOT_GAP: f32 = 20.0;

/// 16px — the footer's horizontal inset.
const FOOT_PAD_X: f32 = 16.0;

/// 10px — the footer's vertical inset, which is what makes it 10 + 17 + 10
/// = 37px plus its 1px top border.
const FOOT_PAD_Y: f32 = 10.0;

/// 17px — each of the footer's five icons.
const FOOT_ICON: f32 = 17.0;

/// Stroke width 1.75 — explicit on all five footer icons; the `icons::`
/// constants are baked at 1.5, 1.6, 1.75 or 2 depending on where they were
/// first used, so the footer asks for this one by name.
const STROKE: f32 = 1.75;

// `trailing` marks Delete, the only button pushed to the right edge:
// measured at x 1333..1349 = 1366 - 16
// (the footer's inset) - 17, with the four on the left at 1122, 1159, 1196, 1233
// (20px between each).
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

/// The panel root: a flexible, minimum-height, clipped column on the panel
/// fill, carrying the `right-dock-layers-panel` id - trace-exempt: a local
/// test id, not a class.
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
    // The Lock header acts on the selected row's core layer, if it has one.
    let active = layers
        .iter()
        .zip(flags.iter())
        .find(|(row, _)| row.id == selected)
        .and_then(|(row, flags)| row.core_id.clone().map(|id| (id, *flags)));
    v_flex()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .overflow_hidden()
        .bg(color::PANEL)
        .child(blend_header(state, cx))
        .child(lock_header(active.as_ref(), state, cx))
        .child(layer_list(
            &layers,
            &flags,
            &selected_layers,
            selected,
            layer_thumbs,
            state,
            cx,
        ))
        .child(actions_footer(state, cx))
}

/// Centred on one row with an 8px gap, 14px side inset and 12px top inset —
/// the blend select and the opacity readout, pushed to the right by `.ml_auto()`.
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

/// The native select control's resting chrome: 26px tall and 120px wide, a
/// 4px radius, a 1px FIELD_BORDER edge on a FIELD fill, 8px side inset and
/// 11px semibold text, with a chevron standing in for the control's own arrow.
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

/// One header lock toggle: which flag it flips, its glyph, and whether the
/// panel disables it.
struct LockToggle {
    id: &'static str,
    svg: &'static [u8],
    kind: LockKind,
    locked: bool,
    /// The disabled flag: the base toggle is off for a missing
    /// or background layer; the three partial locks also need an unlocked base.
    disabled: bool,
    /// The 30% disabled opacity — only the three partial toggles carry it.
    dimmed: bool,
    text: String,
}

/// The four toggles in source order: base, transparency, position, rotation.
///
/// `None` means no active layer, which disables every toggle just as a missing
/// active layer does.
fn lock_toggles(flags: Option<crate::state::LayerFlags>) -> Vec<LockToggle> {
    let background = flags.is_some_and(|flags| flags.is_background);
    let base_locked = flags.is_some_and(|flags| flags.locked);
    let toggle = |id: &'static str,
                  svg: &'static [u8],
                  kind: LockKind,
                  locked: bool,
                  blocked: bool,
                  dimmed: bool,
                  locked_key: &str,
                  unlocked_key: &str| {
        LockToggle {
            id,
            svg,
            kind,
            locked,
            disabled: flags.is_none() || background || blocked,
            dimmed: dimmed && (flags.is_none() || background || blocked),
            text: if background {
                t("layers.renameToUnlock")
            } else if locked {
                t(locked_key)
            } else {
                t(unlocked_key)
            },
        }
    };
    vec![
        toggle(
            "base",
            if base_locked {
                icons::LOCK
            } else {
                icons::UNLOCK
            },
            LockKind::Base,
            base_locked,
            false,
            false,
            "layers.unlockLayer",
            "layers.lockLayer",
        ),
        toggle(
            "transparency",
            icons::PAINT_BUCKET,
            LockKind::Transparency,
            flags.is_some_and(|flags| flags.lock_transparency),
            base_locked,
            true,
            "layers.unlockTransparency",
            "layers.lockTransparency",
        ),
        toggle(
            "position",
            icons::MAXIMIZE,
            LockKind::Position,
            flags.is_some_and(|flags| flags.lock_position),
            base_locked,
            true,
            "layers.unlockPosition",
            "layers.lockPosition",
        ),
        toggle(
            "rotation",
            icons::ROTATE,
            LockKind::Rotation,
            flags.is_some_and(|flags| flags.lock_rotation),
            base_locked,
            true,
            "layers.unlockRotation",
            "layers.lockRotation",
        ),
    ]
}

/// Centred on one row with a 16px gap, 14px side and 12px vertical inset —
/// the "Lock:" label and four 16px icon buttons.
///
/// Each button carries its `Tooltip`; the locked variants tint ACCENT and the
/// three partial locks dim while the base lock is on. The background layer
/// cannot be locked at all — its tooltips say to rename it first.
fn lock_header(
    active: Option<&(String, crate::state::LayerFlags)>,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> Div {
    let flags = active.map(|(_, flags)| *flags);
    let toggles = lock_toggles(flags);
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
                .children(toggles.into_iter().map(|spec| {
                    let core_id = active.map(|(id, _)| id.clone());
                    lock_button(spec, core_id, state, cx).into_any_element()
                })),
        )
}

fn lock_button(
    spec: LockToggle,
    core_id: Option<String>,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let mut button = div()
        .id(format!("layer-lock-header-{}", spec.id))
        .test_support()
        .w(px(LOCK_BOX))
        .h(px(LOCK_BOX))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .child(
            Icon::default()
                .data(spec.svg)
                .size(px(LOCK_ICON))
                .text_color(if spec.locked {
                    color::ACCENT
                } else {
                    color::TEXT_DIM
                }),
        );
    if spec.disabled {
        if spec.dimmed {
            button = button.opacity(0.3);
        }
    } else {
        let kind = spec.kind;
        button = button.cursor_pointer().on_click(cx.listener(
            move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                if let Some(core_id) = core_id.clone() {
                    app.toggle_layer_lock(core_id, kind.clone(), cx);
                }
            },
        ));
    }
    tooltip(
        state,
        format!("layer-lock-header-{}", spec.id),
        spec.text,
        None,
        cx,
        button,
    )
}

/// The Layer Actions footer: a top-bordered, non-growing row on the panel fill,
/// 16px inset, 20px between icons, 10px vertical padding, in the icon colour.
///
/// Each button carries its tooltip and a 30% disabled
/// gate: duplicate and merge-down need an active layer, merge-down also needs
/// one beneath it, flatten needs more than one layer, and delete refuses the
/// last layer and the Background. Resting captures show all five glyphs at
/// measured `#868686`.
fn actions_footer(state: &Entity<AppState>, cx: &Context<crate::app::App>) -> Div {
    let (layers, selected, flags) = {
        let s = state.read(cx);
        (s.layers.clone(), s.selected_layer, s.layer_flags.clone())
    };
    let position = layers.iter().position(|row| row.id == selected);
    let background = position
        .and_then(|index| flags.get(index).copied())
        .is_some_and(|flags| flags.is_background);
    let has_active = position.is_some();
    let at_bottom = position.is_none_or(|index| index + 1 >= layers.len());
    let single = layers.len() <= 1;
    let disabled = [
        false,
        !has_active,
        !has_active || at_bottom,
        single,
        single || background,
    ];
    let texts = [
        t("layers.newLayer"),
        t("layers.duplicateLayer"),
        t("layers.mergeDown"),
        t("layers.flattenAll"),
        if background {
            t("layers.cannotDeleteBackground")
        } else {
            t("layers.deleteLayer")
        },
    ];

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
            let mut button = footer_icon(fi.svg)
                .id(format!("layer-action-{index}"))
                .test_support();
            if disabled[index] {
                button = button.opacity(0.3);
            } else {
                button = button.cursor_pointer().on_click(cx.listener(
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
            }
            let tagged = tooltip(
                state,
                format!("layer-action-{index}"),
                texts[index].clone(),
                None,
                cx,
                button,
            );
            if fi.trailing {
                div().ml_auto().child(tagged).into_any_element()
            } else {
                tagged.into_any_element()
            }
        }))
}

/// One footer glyph: the source `<button>` has no size (trace-exempt: that
/// shape is why), it is exactly the 17px of the icon inside it.
///
/// A 1.75 stroke width is explicit on all five,
/// and the `icons::` constants are baked at whatever width they were first
/// introduced with, so this asks [`icons::with_stroke`] rather than
/// assuming the constant already agrees — the same call `history` makes.
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

    /// Each row closes with a 1px rule, and the rule is the colour that
    /// actually paints rather than the `border-editor-divider/10` the
    /// reference's class names: the app-wide border override wins, so this is
    /// rgb(51,51,51) over the rgb(30,30,30) panel, not DIVIDER at 10%.
    /// RED proof by mutation: point [`rows::row_hairline_colour`] at
    /// `color::DIVIDER` and this fails by name; the surface capture is what
    /// shows the row closing without any rule at all.
    #[test]
    fn layer_row_hairline_is_the_global_border_colour() {
        assert_eq!(
            rows::row_hairline_colour(),
            color::BORDER,
            "the row rule is the global border grey, not divider/10"
        );
    }

    /// The row fill has three branches and the middle one is a tier, not a
    /// second colour: a row selected without being active takes the same
    /// `ROW_ACTIVE` at 70%, and an unselected row takes no fill at all so its
    /// hover can. RED proof by mutation: drop the `else if selected` branch and
    /// the alpha, and this fails at its named middle assertion while the active
    /// and resting assertions stay green.
    #[test]
    fn layer_row_fill_follows_the_three_reference_branches() {
        assert_eq!(
            rows::row_fill(true, true),
            color::ROW_ACTIVE,
            "the active row keeps the full row fill"
        );
        let dimmed = rows::row_fill(false, true);
        assert_eq!(
            dimmed.a, 0.7,
            "a selected-but-inactive row takes the 70% tier"
        );
        assert_eq!(
            dimmed.l,
            color::ROW_ACTIVE.l,
            "the tier dims the same colour, it is not a second grey"
        );
        assert_eq!(
            rows::row_fill(false, false).a,
            0.0,
            "an unselected row draws no fill of its own"
        );
    }

    /// The hover tier is white at 3%, and it is colourless - a hue would show
    /// through on the panel and the reference asks plain white.
    /// RED proof by mutation: raise the alpha or give it a saturation and this
    /// fails at the named assertion.
    #[test]
    fn layer_row_hover_is_three_percent_white() {
        let hover = rows::row_hover_fill();
        assert_eq!(hover.a, 0.03, "the row's hover fill is white at 3%");
        assert_eq!((hover.h, hover.s, hover.l), (0.0, 0.0, 1.0));
    }

    fn flags() -> crate::state::LayerFlags {
        crate::state::LayerFlags {
            visible: true,
            locked: false,
            has_adjustments: false,
            lock_transparency: false,
            lock_position: false,
            lock_rotation: false,
            is_background: false,
        }
    }

    #[test]
    fn lock_toggles_match_the_reference_order_and_glyphs() {
        let toggles = lock_toggles(Some(flags()));
        let glyphs: Vec<&[u8]> = toggles.iter().map(|toggle| toggle.svg).collect();
        assert_eq!(
            glyphs,
            vec![
                icons::UNLOCK,
                icons::PAINT_BUCKET,
                icons::MAXIMIZE,
                icons::ROTATE
            ]
        );
        let ids: Vec<&str> = toggles.iter().map(|toggle| toggle.id).collect();
        assert_eq!(ids, vec!["base", "transparency", "position", "rotation"]);
        assert!(toggles.iter().all(|toggle| !toggle.disabled));
        assert!(toggles.iter().all(|toggle| !toggle.locked));
        assert_eq!(toggles[0].text, t("layers.lockLayer"));
        assert_eq!(toggles[1].text, t("layers.lockTransparency"));
        assert_eq!(toggles[2].text, t("layers.lockPosition"));
        assert_eq!(toggles[3].text, t("layers.lockRotation"));
    }

    #[test]
    fn lock_toggles_follow_the_lock_state_and_disable_partials() {
        let unlocked = lock_toggles(Some(crate::state::LayerFlags {
            lock_transparency: true,
            ..flags()
        }));
        assert_eq!(unlocked[1].text, t("layers.unlockTransparency"));
        assert!(unlocked[1].locked);
        assert!(!unlocked[1].disabled);

        let locked = lock_toggles(Some(crate::state::LayerFlags {
            locked: true,
            ..flags()
        }));
        assert_eq!(locked[0].svg, icons::LOCK);
        assert_eq!(locked[0].text, t("layers.unlockLayer"));
        for toggle in &locked[1..] {
            assert!(toggle.disabled, "a base lock disables the partial toggles");
            assert!(toggle.dimmed, "the partial toggles dim while disabled");
        }

        let background = lock_toggles(Some(crate::state::LayerFlags {
            is_background: true,
            ..flags()
        }));
        assert!(background.iter().all(|toggle| toggle.disabled));
        assert!(background
            .iter()
            .all(|toggle| toggle.text == t("layers.renameToUnlock")));

        let absent = lock_toggles(None);
        assert!(absent.iter().all(|toggle| toggle.disabled));
        assert_eq!(absent[0].text, t("layers.lockLayer"));
    }
}

mod rows;
use rows::layer_list;
