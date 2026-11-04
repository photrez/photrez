//! The Text bar: weight labels and the option controls that reach the text layer.

use super::*;

/// The `Text` variant, draw mode.
pub(super) const TEXT_WEIGHT_LABELS: [&str; 7] = [
    "Thin", "Light", "Regular", "Medium", "SemiBold", "Bold", "Black",
]; // i18n-exempt: not in catalog

pub(super) fn text_weight_index(weight: f64) -> usize {
    crate::app::tool_settings::TEXT_WEIGHTS
        .iter()
        .position(|candidate| (candidate - weight).abs() < 1.0)
        .unwrap_or(2)
}

pub(super) fn text_items(
    state: &gpui_kit::Entity<AppState>,
    row: Div,
    cx: &Context<crate::app::App>,
) -> Div {
    let (
        text_layer,
        size,
        editing,
        weight,
        italic,
        underline,
        strikethrough,
        uppercase,
        align,
        open,
        cursor,
        stroke_width,
        stroke_color,
        stroke_align,
        stroke_open,
        stroke_editing,
        stroke_panel_bounds,
        stroke_slider_bounds,
        stroke_capture,
        stroke_slider_focus,
    ) = {
        let s = state.read(cx);
        let text_layer = s.selected_layer_type.as_deref() == Some("text");
        (
            text_layer,
            if text_layer {
                s.selected_text_font_size
            } else {
                s.text_font_size
            },
            s.num_field
                .as_ref()
                .filter(|field| field.id == "opt-text-size")
                .map(|field| field.buffer.clone()),
            if text_layer {
                s.selected_text_font_weight
            } else {
                s.text_font_weight
            },
            if text_layer {
                s.selected_text_italic
            } else {
                s.text_italic
            },
            if text_layer {
                s.selected_text_underline
            } else {
                s.text_underline
            },
            if text_layer {
                s.selected_text_strikethrough
            } else {
                s.text_strikethrough
            },
            if text_layer {
                s.selected_text_uppercase
            } else {
                s.text_uppercase
            },
            if text_layer {
                s.selected_text_align
            } else {
                s.text_align
            },
            s.open_select,
            s.select_cursor,
            if text_layer {
                s.selected_text_stroke_width
            } else {
                s.text_stroke_width
            },
            if text_layer {
                s.selected_text_stroke_color.unwrap_or(s.text_stroke_color)
            } else {
                s.text_stroke_color
            },
            if text_layer {
                s.selected_text_stroke_align
            } else {
                s.text_stroke_align
            },
            s.text_stroke_popover_open,
            s.num_field
                .as_ref()
                .filter(|field| field.id == "opt-text-stroke-width")
                .map(|field| field.buffer.clone()),
            s.text_stroke_panel_bounds.clone(),
            s.text_stroke_slider_bounds.clone(),
            s.slider_capture.clone(),
            s.slider_focus_for(crate::state::SliderTarget::TextStrokeWidth),
        )
    };
    row.child(tool_pill(
        icons::with_stroke(icons::TYPE, STROKE_PILL),
        t(PILL_LABEL_TEXT_KEY),
    ))
    .child(divider())
    .child(font_family_button())
    .child(divider())
    .child(font_size_field(
        size,
        editing,
        state.read(cx).num_field_focus_for("opt-text-size"),
        cx,
    ))
    .child(tooltip(
        state,
        "opt-weight",
        t("tools.options.fontWeight"),
        None,
        cx,
        live_select(
            "opt-weight",
            None,
            None,
            TEXT_WEIGHT_LABELS[text_weight_index(weight)].to_string(),
            &TEXT_WEIGHT_LABELS,
            if open == Some("opt-weight") {
                cursor
            } else {
                text_weight_index(weight)
            },
            open == Some("opt-weight"),
            state.read(cx).select_panel_bounds.clone(),
            cx,
        ),
    ))
    .child(tooltip(
        state,
        "opt-italic",
        t("tools.options.italic"),
        None,
        cx,
        text_toggle_action(
            "opt-italic",
            div()
                .font_family("Georgia")
                .italic()
                .font_weight(FontWeight(700.))
                .text_size(px(TEXT_MD))
                .child("I"), // i18n-exempt: glyph or symbol
            italic,
            cx,
            |app, cx| {
                let _ = app.toggle_text_italic(cx);
            },
        ),
    ))
    .child(tooltip(
        state,
        "opt-underline",
        t("tools.options.underline"),
        None,
        cx,
        text_toggle_action(
            "opt-underline",
            div()
                .underline()
                .font_weight(FontWeight(700.))
                .text_size(px(TEXT_MD))
                .child("U"), // i18n-exempt: glyph or symbol
            underline,
            cx,
            |app, cx| {
                let _ = app.toggle_text_underline(cx);
            },
        ),
    ))
    .child(tooltip(
        state,
        "opt-strikethrough",
        t("tools.options.strikethrough"),
        None,
        cx,
        text_toggle_action(
            "opt-strikethrough",
            div()
                .line_through()
                .font_weight(FontWeight(700.))
                .text_size(px(TEXT_MD))
                .child("S"), // i18n-exempt: glyph or symbol
            strikethrough,
            cx,
            |app, cx| {
                let _ = app.toggle_text_strikethrough(cx);
            },
        ),
    ))
    .child(tooltip(
        state,
        "opt-uppercase",
        t("tools.options.uppercase"),
        None,
        cx,
        text_toggle_action(
            "opt-uppercase",
            div()
                .font_weight(FontWeight(700.))
                .text_size(px(TEXT_XS))
                .child("TT"), // i18n-exempt: glyph or symbol
            uppercase,
            cx,
            |app, cx| {
                let _ = app.toggle_text_uppercase(cx);
            },
        ),
    ))
    .child(divider())
    .child(
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(ICON_GAP))
            .child(tooltip(
                state,
                "opt-align-left",
                t("properties.alignLeft"),
                None,
                cx,
                text_icon_action(
                    "opt-align-left",
                    icons::ALIGN_TEXT_LEFT,
                    align == crate::state::TextAlignTool::Left,
                    cx,
                    |app, cx| {
                        let _ = app.set_text_align(crate::state::TextAlignTool::Left, cx);
                    },
                ),
            ))
            .child(tooltip(
                state,
                "opt-align-center",
                t("properties.alignCenter"),
                None,
                cx,
                text_icon_action(
                    "opt-align-center",
                    icons::ALIGN_TEXT_CENTER,
                    align == crate::state::TextAlignTool::Center,
                    cx,
                    |app, cx| {
                        let _ = app.set_text_align(crate::state::TextAlignTool::Center, cx);
                    },
                ),
            ))
            .child(tooltip(
                state,
                "opt-align-right",
                t("properties.alignRight"),
                None,
                cx,
                text_icon_action(
                    "opt-align-right",
                    icons::ALIGN_TEXT_RIGHT,
                    align == crate::state::TextAlignTool::Right,
                    cx,
                    |app, cx| {
                        let _ = app.set_text_align(crate::state::TextAlignTool::Right, cx);
                    },
                ),
            )),
    )
    .child(divider())
    .child(tooltip(
        state,
        "opt-box-mode",
        t("tools.options.boxModeDesc"),
        None,
        cx,
        box_mode_control(state, cx),
    ))
    .child(tooltip(
        state,
        "opt-text-color",
        t("tools.options.textColor"),
        None,
        cx,
        text_color_swatch(state, cx),
    ))
    .child(divider())
    .child(tooltip(
        state,
        "opt-text-stroke",
        t("tools.options.strokeOptions"),
        None,
        cx,
        text_stroke_control(
            stroke_width,
            stroke_color,
            stroke_align,
            stroke_open,
            stroke_editing,
            state.read(cx).num_field_focus_for("opt-text-stroke-width"),
            stroke_slider_focus,
            stroke_panel_bounds,
            stroke_slider_bounds,
            stroke_capture,
            cx,
        ),
    ))
    .when(text_layer, |el| {
        el.child(divider()).child(text_toggle_action(
            "opt-text-rasterize",
            div()
                .font_weight(FontWeight(600.))
                .text_size(px(TEXT_XS))
                .child("Raster"), // i18n-exempt: not in catalog
            false,
            cx,
            |app, cx| {
                let result = app.rasterize_selected_layer(cx);
                app.report_layer_op_for_view(result, cx);
            },
        ))
    })
}
