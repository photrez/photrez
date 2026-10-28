use super::*;

/// `flex-1 overflow-y-auto border-y border-editor-divider` — the layer stack.
///
/// The `pr` is the reserved scrollbar gutter; see the module docs on why 254
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
        // The frozen empty state: a centred layers glyph with two localized
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
                flags.get(index).copied(),
                layer.id == selected,
                selected_layers.contains(&layer.id),
                thumbs.get(index).and_then(|thumb| thumb.as_ref()),
                state,
                cx,
            )
        }))
}

/// `flex h-[50px] items-center gap-2.5 px-3.5 border-b border-editor-divider/10`
/// with the active row's `bg-editor-row-active` and its 1px inset ACCENT ring.
///
/// A `ring-1 ring-inset ring-editor-accent/40` has no GPUI equivalent — rings
/// are a box-shadow family GPUI does not expose — so the 1px inset edge is
/// drawn as a border and the colour as `ACCENT` at 40% alpha. That is the
/// closest available reading of "a faint inner outline".
///
/// The id is the A8.0-stable `layer-row-<id>` form: both the drag source and
/// the drop target are this same element, and the mounted test addresses it by
/// that string.
fn layer_row(
    layer: &LayerRow,
    flags: Option<crate::state::LayerFlags>,
    active: bool,
    selected: bool,
    thumb: Option<&std::sync::Arc<gpui_kit::RenderImage>>,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let id = layer.id;
    let name = layer.name.clone();
    let visible = flags.map(|flags| flags.visible).unwrap_or(true);
    let locked = flags.map(|flags| flags.locked).unwrap_or(false);

    let state_for_click = state.clone();
    let on_click = cx.listener(
        move |app: &mut crate::app::App,
              event: &gpui_kit::ClickEvent,
              _,
              cx: &mut Context<crate::app::App>| {
            let modifiers = event.modifiers();
            let toggle = modifiers.control || modifiers.platform;
            // Frozen handleSelectLayer: selecting another layer commits an open
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
    let on_drag =
        move |_: &u64, _: Point<gpui_kit::Pixels>, _: &mut Window, cx: &mut gpui_kit::App| {
            // The ghost is a fixed row-sized chip and does not track the pointer
            // offset; anchor it to the grab point if the drag ever needs to feel
            // attached to the cursor.
            cx.new(|_| LayerDragGhost {
                name: ghost_name.clone(),
            })
        };

    let on_drop = cx.listener(
        move |app, dragged: &u64, _, cx: &mut Context<crate::app::App>| {
            app.reorder_layer_rows(*dragged, id, cx);
        },
    );

    h_flex()
        .id(format!("layer-row-{id}"))
        .test_support()
        .w_full()
        .h(px(ROW_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(ROW_GAP))
        .px(px(PAD_X))
        .when(selected, |el| el.bg(color::ROW_ACTIVE))
        .when(active, |el| {
            el.border(px(1.0)).border_color(Hsla {
                h: color::ACCENT.h,
                s: color::ACCENT.s,
                l: color::ACCENT.l,
                a: 0.4,
            })
        })
        .cursor_pointer()
        .on_click(on_click)
        .on_drag(id, on_drag)
        .on_drop::<u64>(on_drop)
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
        .when_some(layer.core_id.clone(), |el, core_id| {
            let lock_id = core_id.clone();
            let rename_id = core_id.clone();
            el.child(
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
            )
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
                            app.toggle_layer_locked(lock_id.clone(), cx);
                        }),
                    )
                    .child(
                        Icon::default()
                            .data(if locked { icons::LOCK } else { icons::UNLOCK })
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

/// The chip GPUI paints under the pointer while a row is dragged.
///
/// `dragGhost` in the reference is the row itself floating above the list;
/// this is the same 50px row minus the thumbnail, drawn only mid-drag so the
/// resting tree stays identical to the baseline.
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
            .border(px(1.0))
            .border_color(Hsla {
                h: color::ACCENT.h,
                s: color::ACCENT.s,
                l: color::ACCENT.l,
                a: 0.4,
            })
            .child(
                div()
                    .text_size(px(ROW_TEXT))
                    .font_family(ui::FONT_UI)
                    .text_color(color::TEXT)
                    .child(self.name.clone()),
            )
    }
}

/// `LayerThumb`'s frame: 34x34, 1px border, `rounded-[3px]`, checker inside.
///
/// The canvas bitmap is a *layer preview*, which a static port cannot have —
/// so this is the empty chrome, exactly as in `properties_panel.rs`.
fn layer_thumb(thumb: Option<&std::sync::Arc<gpui_kit::RenderImage>>) -> Div {
    let mut frame = div()
        .relative()
        .w(px(THUMB))
        .h(px(THUMB))
        .rounded(px(R3))
        .border(px(1.0))
        .border_color(color::DIVIDER)
        .bg(CHECKER_MEAN)
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
