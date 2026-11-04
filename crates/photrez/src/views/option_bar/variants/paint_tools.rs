//! The Eyedropper, Paint Bucket and Gradient bars.

use super::*;

/// The `Eyedropper` variant.
pub(super) fn eyedropper_items(
    state: &gpui_kit::Entity<AppState>,
    row: Div,
    cx: &Context<crate::app::App>,
) -> Div {
    let auto_copy = state.read(cx).auto_copy_hex;
    row.child(tool_pill(
        icons::with_stroke(icons::PIPETTE, STROKE_PILL),
        t(PILL_LABEL_EYEDROPPER_KEY),
    ))
    .child(divider())
    .child(
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .child(fg_swatch_square())
            .child(tooltip(
                state,
                "opt-copy-hex",
                t("tools.options.clickToCopyHex"),
                None,
                cx,
                hex_badge(),
            )),
    )
    .child(divider())
    .child(option_checkbox_action(
        "opt-auto-copy",
        &t("tools.options.autoCopyHex"),
        auto_copy,
        None,
        cx,
        |app, cx| app.toggle_tool_flag(ToolFlag::AutoCopyHex, cx),
    ))
}

// ── Paint Bucket ───────────────────────────────────────────────────────────────

/// The `PaintBucket` variant.
pub(super) fn paint_bucket_items(
    state: &gpui_kit::Entity<AppState>,
    row: Div,
    cx: &Context<crate::app::App>,
) -> Div {
    let (tolerance, contiguous, editing, focus) = {
        let s = state.read(cx);
        (
            s.fill_tolerance,
            s.fill_contiguous,
            s.num_field
                .as_ref()
                .filter(|field| field.id == "opt-tolerance")
                .map(|field| field.buffer.clone()),
            s.num_field_focus_for("opt-tolerance"),
        )
    };
    row.child(
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(10.0))
            .px(px(8.0))
            .child(tool_pill(
                icons::with_stroke(icons::PAINT_BUCKET, STROKE_PILL),
                t("tools.paintBucket"),
            ))
            .child(divider())
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.0))
                    .child(dim_span(&format!("{}:", t("tools.options.tolerance"))))
                    .child(tolerance_slider(state, cx))
                    .child(tolerance_num_field(tolerance, editing, focus, cx)),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(ITEM_GAP))
                    .children(TOLERANCE_IDS.into_iter().zip(TOLERANCE_PRESETS).map(
                        |(id, value)| {
                            let active = value
                                .parse::<u32>()
                                .map(|preset| preset == tolerance)
                                .unwrap_or(false);
                            tolerance_preset(id, value, active, cx, move |app, cx| {
                                if let Ok(preset) = value.parse::<f64>() {
                                    let _ = app.set_fill_tolerance(preset, cx);
                                }
                            })
                        },
                    )),
            )
            .child(divider())
            .child(option_checkbox_action(
                "opt-contiguous",
                &t("tools.options.contiguous"),
                contiguous,
                None,
                cx,
                |app, cx| app.toggle_tool_flag(ToolFlag::FillContiguous, cx),
            )),
    )
}

// ── Gradient ───────────────────────────────────────────────────────────────────

/// The `Gradient` variant.
pub(super) fn gradient_items(
    state: &gpui_kit::Entity<AppState>,
    row: Div,
    cx: &Context<crate::app::App>,
) -> Div {
    let (fg_color, bg_color, radial, fg_transparent, reverse, open, cursor) = {
        let s = state.read(cx);
        (
            s.fg_color,
            s.bg_color,
            s.gradient_radial,
            s.gradient_fg_transparent,
            s.gradient_reverse,
            s.open_select,
            s.select_cursor,
        )
    };
    let fg = crate::palette::hsla(fg_color);
    let bg = crate::palette::hsla(bg_color);
    let (from, to) = gradient_preview(fg_color, bg_color, fg_transparent, reverse);
    let type_active = if open == Some("opt-gradient-type") {
        cursor
    } else if radial {
        1
    } else {
        0
    };
    let preset_active = if open == Some("opt-gradient-preset") {
        cursor
    } else if fg_transparent {
        1
    } else {
        0
    };
    row.child(
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(10.0))
            .px(px(8.0))
            .child(tool_pill(
                icons::with_stroke(icons::SWATCH, STROKE_PILL),
                t(PILL_LABEL_GRADIENT_KEY),
            ))
            .child(divider())
            .child(live_select(
                "opt-gradient-type",
                None,
                Some(format!("{}: ", t("tools.options.gradientType")).as_str()),
                if radial {
                    "Radial".to_string() // i18n-exempt: not in catalog
                } else {
                    t("tools.options.linear")
                },
                &["Linear", "Radial"], // i18n-exempt: not in catalog
                type_active,
                open == Some("opt-gradient-type"),
                state.read(cx).select_panel_bounds.clone(),
                cx,
            ))
            .child(divider())
            .child(live_select(
                "opt-gradient-preset",
                None,
                Some(format!("{}: ", t("tools.options.gradientPreset")).as_str()),
                if fg_transparent {
                    "FG to transparent".to_string() // i18n-exempt: not in catalog
                } else {
                    t(GRADIENT_PRESET_KEY)
                },
                &["FG to BG", "FG to transparent"], // i18n-exempt: not in catalog
                preset_active,
                open == Some("opt-gradient-preset"),
                state.read(cx).select_panel_bounds.clone(),
                cx,
            ))
            .child(divider())
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(GAP))
                    .child(round_swatch(fg))
                    .child(gradient_strip(from, to))
                    .child(round_swatch(if fg_transparent {
                        Hsla { a: 0.0, ..bg }
                    } else {
                        bg
                    })),
            )
            .child(reverse_button(cx)),
    )
}
