use super::*;

use crate::views::option_bar::controls::tooltip;

/// 22px — one reorder chevron.
const MOVE_BTN: f32 = 22.0;

/// 14px — the glyph inside it.
const MOVE_ICON: f32 = 14.0;

/// 4px between the chevrons and on the strip's trailing edge.
const MOVE_GAP: f32 = 4.0;

/// 24px — the adjustments indicator's box.
const ADJUST_BOX: f32 = 24.0;
/// 14px — the sliders glyph inside it.
const ADJUST_ICON: f32 = 14.0;
/// 4px — the indicator's trailing gap before the lock button.
const ADJUST_GAP: f32 = 4.0;
/// A 1.75 stroke width on the indicator's icon.
const ADJUST_STROKE: f32 = 1.75;

/// 24px tall, 8px side inset, 4px icon gap, 12px glyph, 11px label — the
/// locked-row badge the reference's `Locked` pill asks for.
const BADGE_H: f32 = 24.0;
const BADGE_PX: f32 = 8.0;
const BADGE_GAP: f32 = 4.0;
const BADGE_ICON: f32 = 12.0;
const BADGE_TEXT: f32 = 11.0;

/// 20px square with a 2px corner and a 12px glyph — the drag ghost's icon tile.
const GHOST_TILE: f32 = 20.0;
const GHOST_TILE_RADIUS: f32 = 2.0;
const GHOST_TILE_ICON: f32 = 12.0;

/// The colour of the 1px rule closing every layer row.
///
/// The reference asks `border-b border-editor-divider/10` for it, which names
/// DIVIDER at 10% over the row - a 2/255 step a capture would never notice. The
/// rule that actually reaches the screen is the app-wide border override
/// ([`color::BORDER`]), so this is rgb(51,51,51): a 21/255 step over the
/// rgb(30,30,30) panel, in a row the reference draws and the native did not.
/// Kept as a function so the tier is assertable without a window; the width is
/// the fixed 1px the reference's `border-b` gives it.
pub(super) fn row_hairline_colour() -> Hsla {
    color::BORDER
}

/// The row's own fill, in the reference's three branches: full `ROW_ACTIVE` on
/// the active row, the same colour at 70% on a row selected without being
/// active, and nothing on the rest.
///
/// The middle tier is a 5/255 step over the panel - the ratified pixel gate
/// cannot see it, which is exactly why it was never ported. It is still the
/// state the reference paints, and a multi-selection reads as one bright row
/// plus a column of dimmed ones rather than a block of identical ones.
pub(super) fn row_fill(active: bool, selected: bool) -> Hsla {
    if active {
        color::ROW_ACTIVE
    } else if selected {
        Hsla {
            a: 0.7,
            ..color::ROW_ACTIVE
        }
    } else {
        Hsla {
            a: 0.0,
            ..color::ROW_ACTIVE
        }
    }
}

/// White at 3%, the fill an unselected row takes while the pointer is on it.
///
/// The reference only asks for it in its third branch, so a selected row keeps
/// its own fill tier and never takes this one; the step it makes over the panel
/// is 7/255.
pub(super) fn row_hover_fill() -> Hsla {
    Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 0.03,
    }
}

/// The locked badge's three tiers: fill, edge, label.
///
/// The reference pill is `bg-editor-accent/5` with a `border-editor-accent/20`
/// edge. Its label asks `text-editor-accent/70`, which measures 3.33:1 on that
/// fill over the active row's `ROW_ACTIVE`, under the 4.5:1 floor for 11px
/// text, so the native label keeps full `color::ACCENT` (5.18:1 there, 6.38:1
/// over the plain panel). A function, so the tiers are assertable without a
/// window.
pub(super) fn locked_badge_colours() -> (Hsla, Hsla, Hsla) {
    (
        color::ACCENT.opacity(0.05),
        color::ACCENT.opacity(0.2),
        color::ACCENT,
    )
}

/// The badge a locked row carries: the lock glyph plus the localized label, so
/// the state never reads as colour alone.
///
/// It is a status label, not a control: the row's own lock button is what
/// toggles, so this element takes no click handler and no pointer cursor.
fn locked_badge(id: u64) -> impl IntoElement {
    let (fill, edge, label) = locked_badge_colours();
    h_flex()
        .id(format!("layer-locked-badge-{id}"))
        .test_support()
        .h(px(BADGE_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(BADGE_GAP))
        .px(px(BADGE_PX))
        .rounded(px(R3))
        .border_1()
        .border_color(edge)
        .bg(fill)
        .font_family(ui::FONT_UI)
        .text_size(px(BADGE_TEXT))
        .text_color(label)
        .child(
            Icon::default()
                .data(icons::LOCK)
                .size(px(BADGE_ICON))
                .text_color(label),
        )
        .child(t("layers.locked"))
}

/// The tile the drag ghost leads with, so a floating chip still reads as the
/// layer it came from.
///
/// The design gives non-text layers a 20px dark tile carrying the layers glyph;
/// its adjustment and text variants branch on the layer's type, which the
/// projected rows do not carry.
fn ghost_tile() -> impl IntoElement {
    div()
        .id("layer-drag-ghost-tile")
        .test_support()
        .w(px(GHOST_TILE))
        .h(px(GHOST_TILE))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .rounded(px(GHOST_TILE_RADIUS))
        .border_1()
        .border_color(color::WHITE_10)
        // The reference asks `bg-black/50`; no token sits on that alpha
        // (`BLACK_40` and `BLACK_55` flank it), so the tile names it here.
        .bg(Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a: 0.5,
        })
        .child(
            Icon::default()
                .data(icons::LAYERS)
                .size(px(GHOST_TILE_ICON))
                .text_color(color::TEXT_DIM),
        )
}

/// The layer stack: a scrollable column with a 6px reserved gutter, bordered
/// top and bottom.
///
/// The right padding is the scrollbar gutter; see the module docs on why 254
/// and 260 are both correct answers here. `border_y` is [`color::BORDER`] and
/// not [`color::DIVIDER`]: the baseline reads `#333333` on both edges (y=164
/// and y=572), the `* { border-color }` rule being what reaches the screen for
/// any `border-*` utility.
pub(super) fn layer_list(
    layers: &[LayerRow],
    flags: &[crate::state::LayerFlags],
    selected_layers: &[u64],
    selected: u64,
    thumbs: &[Option<std::sync::Arc<gpui_kit::RenderImage>>],
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> Div {
    if layers.is_empty() {
        // The empty state: a centred layers glyph with two localized
        // lines, on the same bordered list box (mirrors the properties
        // panel's no-document branch).
        return v_flex()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .pr(px(6.0))
            .border_y(px(1.0))
            .border_color(color::BORDER)
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .child(
                Icon::default()
                    .data(icons::LAYERS)
                    .size(px(24.0))
                    .text_color(color::TEXT_DIM),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .font_family(ui::FONT_UI)
                    .font_weight(FontWeight(500.))
                    .text_color(color::TEXT)
                    .child(t("layers.noLayersYet")),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .font_family(ui::FONT_UI)
                    .text_color(color::TEXT_DIM)
                    .child(t("layers.addAnImage")),
            );
    }
    v_flex()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .overflow_hidden()
        .pr(px(6.0))
        .border_y(px(1.0))
        .border_color(color::BORDER)
        .children(layers.iter().enumerate().map(|(index, layer)| {
            layer_row(
                layer,
                RowView {
                    flags: flags.get(index).copied(),
                    active: layer.id == selected,
                    selected: selected_layers.contains(&layer.id),
                    up_target: index.checked_sub(1).map(|up| layers[up].id),
                    down_target: layers.get(index + 1).map(|down| down.id),
                    thumb: thumbs.get(index).and_then(|thumb| thumb.as_ref()),
                },
                state,
                cx,
            )
        }))
}

struct RowView<'a> {
    flags: Option<crate::state::LayerFlags>,
    active: bool,
    selected: bool,
    up_target: Option<u64>,
    down_target: Option<u64>,
    thumb: Option<&'a std::sync::Arc<gpui_kit::RenderImage>>,
}

/// A 50px row with a 10px child gap and 14px side inset. Its fill and its inner
/// edge follow the reference's three branches - see [`row_fill`], which is full
/// `ROW_ACTIVE` and a 1px inset `ACCENT` ring at 40% alpha on the active row,
/// the same fill at 70% and a 20% ring on a row selected without being active,
/// and no fill plus white/3% on hover for the rest.
///
/// The ring is an inset shadow rather than a border because that is how the
/// reference builds it (`ring-1 ring-inset`) and the two are not
/// interchangeable here: a border takes its pixel out of the content box, so a
/// bordered row paints its thumbnail and name 1px further in than the reference
/// row does. The row's bottom rule is a real border, because that is what the
/// reference draws there (`border-b`) and a bottom-only border leaves the
/// content 49px of 50 - the same box the reference's own layout works in.
///
/// On a ringed row the rule and the ring want the same last pixel and the rule
/// wins, so the ring reads as open at the bottom where the reference closes it
/// one row higher. Measured in the layers capture, and still the closer of the
/// two arrangements: the reference's own bottom pixel is rgb(67,57,49), a grey
/// rule with accent bleeding through it, where the bare ring would leave
/// rgb(124,84,54).
///
/// The id is the stable `layer-row-<id>` form: both the drag source and
/// the drop target are this same element, and the mounted test addresses it by
/// that string.
fn layer_row(
    layer: &LayerRow,
    view: RowView<'_>,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let RowView {
        flags,
        active,
        selected,
        up_target,
        down_target,
        thumb,
    } = view;
    let id = layer.id;
    let name = layer.name.clone();
    let visible = flags.map(|flags| flags.visible).unwrap_or(true);
    let locked = flags.map(|flags| flags.locked).unwrap_or(false);
    let background = flags.is_some_and(|flags| flags.is_background);
    let has_adjustments = flags.map(|flags| flags.has_adjustments).unwrap_or(false);

    let state_for_click = state.clone();
    let on_click = cx.listener(
        move |app: &mut crate::app::App,
              event: &gpui_kit::ClickEvent,
              _,
              cx: &mut Context<crate::app::App>| {
            let modifiers = event.modifiers();
            let toggle = modifiers.control || modifiers.platform;
            // Selecting another layer commits an open
            // text session first (click-away), so the overlay never edits a
            // layer the panel has stopped highlighting.
            let clicked_core = state_for_click
                .read(cx)
                .layers
                .iter()
                .find(|row| row.id == id)
                .and_then(|row| row.core_id.clone());
            if let Some(core_id) = clicked_core {
                let open_other = state_for_click
                    .read(cx)
                    .text_edit
                    .as_ref()
                    .is_some_and(|session| session.layer_id != core_id);
                if open_other {
                    let _ = app.commit_text_edit(cx);
                }
            }
            state_for_click.update(cx, |s, _| {
                if toggle {
                    if let Some(index) = s.selected_layers.iter().position(|row| *row == id) {
                        if s.selected_layers.len() > 1 {
                            s.selected_layers.remove(index);
                        }
                        if s.selected_layer == id {
                            s.selected_layer = *s.selected_layers.last().unwrap_or(&id);
                        }
                    } else {
                        s.selected_layers.push(id);
                        s.selected_layer = id;
                    }
                } else if modifiers.shift {
                    let ids: Vec<u64> = s.layers.iter().map(|row| row.id).collect();
                    if let Some(to) = ids.iter().position(|row| *row == id) {
                        let from = ids
                            .iter()
                            .position(|row| *row == s.selected_layer)
                            .unwrap_or(to);
                        let (start, end) = if from <= to { (from, to) } else { (to, from) };
                        s.selected_layers = ids[start..=end].to_vec();
                        s.selected_layer = id;
                    }
                } else {
                    s.selected_layers = vec![id];
                    s.selected_layer = id;
                }
            });
            cx.notify();
        },
    );

    let ghost_name = layer.name.clone();
    let drag_payload = {
        let source_doc_id = state.read(cx).document_id.clone().unwrap_or_default();
        crate::app::cross_doc::LayerDragPayload {
            source_doc_id,
            core_id: layer.core_id.clone(),
            row_id: id,
        }
    };
    let on_drag = move |_: &crate::app::cross_doc::LayerDragPayload,
                        _: Point<gpui_kit::Pixels>,
                        _: &mut Window,
                        cx: &mut gpui_kit::App| {
        // The ghost is a fixed row-sized chip and does not track the pointer
        // offset; anchor it to the grab point if the drag ever needs to feel
        // attached to the cursor.
        cx.new(|_| LayerDragGhost {
            name: ghost_name.clone(),
        })
    };

    let on_drop = cx.listener(
        move |app,
              payload: &crate::app::cross_doc::LayerDragPayload,
              window,
              cx: &mut Context<crate::app::App>| {
            app.drop_layer_on_row(payload, id, window.modifiers().alt, cx);
        },
    );

    // The inner edge, in two tiers: accent at 40% on the active row, at 20% on
    // a row that is selected without being active, and nothing on the rest.
    let accent = |alpha: f32| Hsla {
        h: color::ACCENT.h,
        s: color::ACCENT.s,
        l: color::ACCENT.l,
        a: alpha,
    };
    let mut rings = Vec::new();
    if active {
        rings.push(ui::inset_ring(accent(0.4)));
    } else if selected {
        rings.push(ui::inset_ring(accent(0.2)));
    }

    h_flex()
        .id(format!("layer-row-{id}"))
        .test_support()
        .group(format!("layer-row-{id}"))
        .w_full()
        .h(px(ROW_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(ROW_GAP))
        .px(px(PAD_X))
        .border_b_1()
        .border_color(row_hairline_colour())
        .bg(row_fill(active, selected))
        .when(!selected, |el| el.hover(|s| s.bg(row_hover_fill())))
        .shadow(rings)
        .cursor_pointer()
        .on_click(on_click)
        .on_drag(drag_payload, on_drag)
        .on_drop::<crate::app::cross_doc::LayerDragPayload>(on_drop)
        .child(layer_thumb(thumb))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(ROW_TEXT))
                .font_family(ui::FONT_UI)
                .text_color(if locked { color::TEXT_DIM } else { color::TEXT })
                .child(name),
        )
        .when(locked, |el| el.child(locked_badge(id)))
        .child(
            h_flex()
                .items_center()
                .gap(px(MOVE_GAP))
                .pr(px(MOVE_GAP))
                .opacity(0.0)
                .group_hover(format!("layer-row-{id}"), |style| style.opacity(1.0))
                .child(move_button(
                    id,
                    "up",
                    icons::CHEVRON_UP,
                    "layers.moveLayerUp",
                    up_target,
                    state,
                    cx,
                ))
                .child(move_button(
                    id,
                    "down",
                    icons::CHEVRON_DOWN,
                    "layers.moveLayerDown",
                    down_target,
                    state,
                    cx,
                )),
        )
        .when(has_adjustments, |el| {
            el.child(tooltip(
                state,
                format!("layer-adjustments-{id}"),
                t("layers.layerAdjustmentsApplied"),
                None,
                cx,
                div()
                    .id(format!("layer-adjustments-{id}"))
                    .test_support()
                    .w(px(ADJUST_BOX))
                    .h(px(ADJUST_BOX))
                    .mr(px(ADJUST_GAP))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(
                        Icon::default()
                            .data(icons::with_stroke(icons::SLIDERS, ADJUST_STROKE))
                            .size(px(ADJUST_ICON))
                            .text_color(color::ACCENT),
                    ),
            ))
        })
        .when_some(layer.core_id.clone(), |el, core_id| {
            let lock_id = core_id.clone();
            let rename_id = core_id.clone();
            el.child(tooltip(
                state,
                format!("layer-eye-{id}"),
                if visible {
                    t("layers.hideLayer")
                } else {
                    t("layers.showLayer")
                },
                None,
                cx,
                div()
                    .id(format!("layer-eye-{id}"))
                    .test_support()
                    .w(px(18.0))
                    .h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            app.toggle_layer_visibility(core_id.clone(), cx);
                        }),
                    )
                    .child(Icon::default().data(icons::EYE).size(px(14.0)).text_color(
                        if visible {
                            color::ICON
                        } else {
                            color::TEXT_DIM
                        },
                    )),
            ))
            .child(
                div()
                    .id(format!("layer-lock-{id}"))
                    .test_support()
                    .w(px(18.0))
                    .h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            if background {
                                // Background layers never toggle their lock; one
                                // only unlocks by being renamed.
                                app.show_toast(
                                    "Rename the layer to unlock",
                                    crate::state::ToastSeverity::Warn,
                                    cx,
                                );
                                return;
                            }
                            app.toggle_layer_locked(lock_id.clone(), cx);
                        }),
                    )
                    .child(
                        Icon::default()
                            .data(if locked || background {
                                icons::LOCK
                            } else {
                                icons::UNLOCK
                            })
                            .size(px(13.0))
                            .text_color(if locked { color::ICON } else { color::TEXT_DIM }),
                    ),
            )
            .child(
                div()
                    .id(format!("layer-rename-{id}"))
                    .test_support()
                    .w(px(18.0))
                    .h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |app, _: &gpui_kit::ClickEvent, window, cx| {
                            cx.stop_propagation();
                            app.begin_rename_layer(rename_id.clone(), window, cx);
                        }),
                    )
                    .child(
                        Icon::default()
                            .data(icons::PEN)
                            .size(px(12.0))
                            .text_color(color::TEXT_DIM),
                    ),
            )
        })
}

/// One reorder chevron: hover-revealed by the row's group, and disabled with
/// The indicator sits at 20% opacity when the row already sits at that end of
/// the stack.
///
/// The disabled button keeps its paint but gets no handler, which is what
/// disabling does in the row as well.
fn move_button(
    row_id: u64,
    direction: &'static str,
    icon: &'static [u8],
    tooltip_key: &'static str,
    target: Option<u64>,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let button = div()
        .id(format!("layer-{direction}-{row_id}"))
        .test_support()
        .w(px(MOVE_BTN))
        .h(px(MOVE_BTN))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .rounded(px(R3))
        .child(
            Icon::default()
                .data(icon)
                .size(px(MOVE_ICON))
                .text_color(color::ICON),
        );
    let button = match target {
        Some(target) => button
            .cursor_pointer()
            .hover(|style| style.bg(color::WHITE_10))
            .on_click(
                cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.reorder_layer_rows(row_id, target, cx);
                }),
            ),
        None => button.opacity(0.2),
    };
    tooltip(
        state,
        format!("layer-{direction}-{row_id}"),
        t(tooltip_key),
        None,
        cx,
        button,
    )
}

/// The chip GPUI paints under the pointer while a row is dragged.
///
/// `dragGhost` in the reference is the row itself floating above the list;
/// this is the same 50px row minus the layer bitmap, drawn only mid-drag so the
/// resting tree stays identical to the baseline. [`ghost_tile`] stands in for
/// the thumbnail so the chip still names a layer.
struct LayerDragGhost {
    name: String,
}

impl Render for LayerDragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w(px(PANEL_W - 6.0))
            .h(px(ROW_H))
            .items_center()
            .gap(px(ROW_GAP))
            .px(px(PAD_X))
            .bg(color::ROW_ACTIVE)
            // The reference's dragged row asks a dashed accent border and an inset
            // accent ring; the border colour is overridden to grey by the global rule,
            // so only the ring carries the accent here.
            .border_b_1()
            .border_color(row_hairline_colour())
            .shadow(vec![ui::inset_ring(color::ACCENT.opacity(0.6))])
            .child(ghost_tile())
            .child(
                div()
                    .text_size(px(ROW_TEXT))
                    .font_family(ui::FONT_UI)
                    .text_color(color::TEXT)
                    .child(self.name.clone()),
            )
    }
}

/// The layer thumbnail's frame: 34x34, 1px border, 3px corners, checker inside.
///
/// The canvas bitmap is a *layer preview*, which a static view cannot have —
/// so this is the empty chrome, exactly as in `properties_panel.rs`.
fn layer_thumb(thumb: Option<&std::sync::Arc<gpui_kit::RenderImage>>) -> Div {
    // A row without a core layer is the seeded display-only Background, whose
    // document image is the white 800x600 canvas; that is the swatch the
    // capture paints. Anything with pixels keeps the checkerboard frame.
    let mut frame = div()
        .relative()
        .w(px(THUMB))
        .h(px(THUMB))
        .rounded(px(R3))
        .border(px(1.0))
        .border_color(color::DIVIDER)
        .bg(if thumb.is_some() {
            CHECKER_MEAN
        } else {
            color::WHITE
        })
        .flex_shrink_0();
    if let Some(image) = thumb.cloned() {
        frame = frame.child(
            gpui_kit::canvas(
                move |bounds, _window, _cx| bounds,
                move |bounds, _paint_bounds, window, _cx| {
                    let _ = window.paint_image(
                        bounds,
                        bounds,
                        gpui_kit::Corners::default(),
                        image.clone(),
                        0,
                        false,
                    );
                },
            )
            .absolute()
            .inset(px(1.0)),
        );
    }
    frame
}
