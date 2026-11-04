use super::*;

/// The multi-selection branch: header card, combined bounds, align/distribute
/// and the batch actions, mirroring `PropertiesPanel`'s multi inspector.
pub(super) fn multi_layers_section(
    state: &gpui_kit::Entity<crate::state::AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let (count, names, bounds) = {
        let s = state.read(cx);
        let names: Vec<String> = s
            .layers
            .iter()
            .filter(|row| s.selected_layers.contains(&row.id))
            .map(|row| row.name.clone())
            .collect();
        (names.len(), names.join(", "), s.multi_bounds)
    };
    let bound_string = |value: f64| format!("{}", (value * 10.0).round() / 10.0);

    let mut panel = v_flex().w_full().flex_shrink_0().child(
        section_block()
            .child(section_header(
                icons::LAYERS,
                &t("properties.multipleLayers"),
            ))
            .child(
                h_flex()
                    .mt(px(8.0))
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(R4))
                    .border(px(1.0))
                    .border_color(color::DIVIDER)
                    .bg(color::FIELD)
                    .p(px(8.0))
                    .child(
                        div()
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .flex_shrink_0()
                            .rounded(px(R3))
                            .border(px(1.0))
                            .border_color(color::DIVIDER)
                            .bg(color::PANEL)
                            .child(
                                Icon::default()
                                    .data(icons::LAYERS)
                                    .size(px(16.0))
                                    .text_color(color::ACCENT),
                            ),
                    )
                    .child(
                        v_flex()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_family(ui::FONT_UI)
                                    .font_weight(FontWeight(500.))
                                    .text_color(color::TEXT)
                                    .child(crate::i18n::t_args(
                                        "properties.layersSelected",
                                        &[("count", &count.to_string())],
                                    )),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .overflow_hidden()
                                    .text_size(px(10.5))
                                    .font_family(ui::FONT_UI)
                                    .text_color(color::TEXT_DIM)
                                    .child(names),
                            ),
                    ),
            ),
    );

    if let Some((x, y, w, h)) = bounds {
        panel = panel.child(
            section_block()
                .child(section_header(icons::MOVE, &t("properties.combinedBounds")))
                .child(
                    v_flex()
                        .mt(px(8.0))
                        .gap(px(8.0))
                        .w_full()
                        .child(ui::prop_row(
                            &t("properties.position"),
                            h_flex()
                                .flex_1()
                                .gap(px(SLOT_GAP))
                                .child(ui::num_field(Some("X"), None, &bound_string(x), Some("px")))
                                .child(ui::num_field(
                                    Some("Y"),
                                    None,
                                    &bound_string(y),
                                    Some("px"),
                                )),
                        ))
                        .child(ui::prop_row(
                            &t("tools.options.size"),
                            h_flex()
                                .flex_1()
                                .gap(px(SLOT_GAP))
                                .child(ui::num_field(Some("W"), None, &bound_string(w), Some("px")))
                                .child(ui::num_field(
                                    Some("H"),
                                    None,
                                    &bound_string(h),
                                    Some("px"),
                                )),
                        )),
                ),
        );
    }

    let align_button = |id: &'static str, icon: &'static [u8], mode: u8| {
        multi_icon_button(id, icon, cx, move |app, cx| {
            app.align_selected_layers(mode, cx)
        })
    };
    panel = panel.child(
        section_block()
            .child(section_header(
                icons::GRID_3,
                &t("properties.alignToCanvas"),
            ))
            .child(
                h_flex()
                    .mt(px(8.0))
                    .gap(px(4.0))
                    .w_full()
                    .child(align_button("prop-align-left", icons::ALIGN_LEFT, 0))
                    .child(align_button("prop-align-center-h", icons::ALIGN_H, 1))
                    .child(align_button("prop-align-right", icons::ALIGN_RIGHT, 2))
                    .child(align_button("prop-align-top", icons::ALIGN_TOP, 3))
                    .child(align_button("prop-align-center-v", icons::ALIGN_V, 4))
                    .child(align_button("prop-align-bottom", icons::ALIGN_BOTTOM, 5)),
            ),
    );

    if count >= 3 {
        panel = panel.child(
            section_block()
                .child(section_header(
                    icons::SPLIT_H,
                    &t("properties.distributeSpacing"),
                ))
                .child(
                    h_flex()
                        .mt(px(8.0))
                        .gap(px(6.0))
                        .w_full()
                        .child(multi_text_button(
                            "prop-distribute-h",
                            icons::DISTRIBUTE_H,
                            &t("properties.distributeH"),
                            cx,
                            |app, cx| app.distribute_selected_layers(true, cx),
                        ))
                        .child(multi_text_button(
                            "prop-distribute-v",
                            icons::DISTRIBUTE_V,
                            &t("properties.distributeV"),
                            cx,
                            |app, cx| app.distribute_selected_layers(false, cx),
                        )),
                ),
        );
    }

    panel.child(
        section_block()
            .child(section_header(
                icons::SLIDERS,
                &t("properties.batchActions"),
            ))
            .child(
                v_flex()
                    .mt(px(8.0))
                    .gap(px(6.0))
                    .w_full()
                    .child(canvas_action_button(
                        "prop-merge-selected",
                        icons::LAYERS,
                        &t("properties.mergeSelectedLayers"),
                        ActionTone::Field,
                        cx,
                        |app, _window, cx| app.merge_selected_layers(cx),
                    ))
                    .child(
                        h_flex()
                            .gap(px(6.0))
                            .w_full()
                            .child(canvas_action_button(
                                "prop-duplicate",
                                icons::COPY,
                                &t("layers.duplicateLayer"),
                                ActionTone::Field,
                                cx,
                                |app, _window, cx| app.duplicate_selected_layer_id(cx).map(|_| ()),
                            ))
                            .child(canvas_action_button(
                                "prop-delete",
                                icons::TRASH,
                                &t("layers.deleteLayer"),
                                ActionTone::Destructive,
                                cx,
                                |app, _window, cx| {
                                    app.delete_selected_layer(cx);
                                    Ok(())
                                },
                            )),
                    ),
            ),
    )
}

fn multi_icon_button(
    id: &'static str,
    icon: &'static [u8],
    cx: &Context<crate::app::App>,
    action: impl Fn(&mut crate::app::App, &mut Context<crate::app::App>) -> Result<(), String> + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .flex_1()
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(R3))
        .border(px(1.0))
        .border_color(color::DIVIDER)
        .bg(color::FIELD)
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                let result = action(app, cx);
                match result {
                    Ok(()) => app.set_status_error(None, cx),
                    Err(error) => app.set_status_error(Some(format!("Properties: {error}")), cx),
                }
            }),
        )
        .child(
            Icon::default()
                .data(icon)
                .size(px(14.0))
                .text_color(color::TEXT_DIM),
        )
}

fn multi_text_button(
    id: &'static str,
    icon: &'static [u8],
    label: &str,
    cx: &Context<crate::app::App>,
    action: impl Fn(&mut crate::app::App, &mut Context<crate::app::App>) -> Result<(), String> + 'static,
) -> impl IntoElement {
    h_flex()
        .id(id)
        .test_support()
        .flex_1()
        .h(px(28.0))
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(R3))
        .border(px(1.0))
        .border_color(color::DIVIDER)
        .bg(color::FIELD)
        .text_size(px(11.0))
        .font_family(ui::FONT_UI)
        .font_weight(FontWeight(500.))
        .text_color(color::TEXT_DIM)
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                let result = action(app, cx);
                match result {
                    Ok(()) => app.set_status_error(None, cx),
                    Err(error) => app.set_status_error(Some(format!("Properties: {error}")), cx),
                }
            }),
        )
        .child(
            Icon::default()
                .data(icon)
                .size(px(14.0))
                .text_color(color::TEXT_DIM),
        )
        .child(label.to_string())
}
