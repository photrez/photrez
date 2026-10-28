use super::*;

/// `SectionHeader` row (`layers/SectionHeader.tsx`): 16px icon box,
/// 15px icon, 8px gap, 12.5px medium label, and optional trailing item.
/// Call sites use stroke width 1.75; `icons::with_stroke` adjusts icons
/// pinned to a different width.
fn section_header(icon: &'static [u8], label: &str, trailing: Option<Div>) -> Div {
    let mut row = h_flex().items_center().justify_between().child(
        h_flex()
            .items_center()
            .gap(px(ROW_GAP))
            .child(
                div()
                    .w(px(HDR_ICON_BOX))
                    .h(px(HDR_ICON_BOX))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(
                        Icon::default()
                            .data(icon)
                            .size(px(HDR_ICON))
                            .text_color(color::TEXT_DIM),
                    ),
            )
            .child(
                div()
                    .text_size(px(HDR_TEXT))
                    .font_family(ui::FONT_UI)
                    .font_weight(FontWeight(500.))
                    .text_color(color::TEXT)
                    .child(label.to_string()),
            ),
    );
    if let Some(t) = trailing {
        row = row.child(t);
    }
    row
}

/// `LayerThumb` frame: 34x34, 1px border, `rounded-[3px]`. This static
/// port has no preview bitmap, and the captured layer has no
/// `imageBitmap`, so the empty chrome is the resting content.
///
/// Source requests `border-editor-accent` (`AdjustmentsPanel.tsx:431`),
/// but the unlayered `*` rule resets it to `#333333` (see module docs).
fn layer_thumb() -> Div {
    div()
        .w(px(THUMB))
        .h(px(THUMB))
        .rounded(px(R3))
        .border_1()
        .border_color(color::BORDER)
        .bg(CHECKER_MEAN)
        .flex_shrink_0()
}

/// Selected Layer section (`AdjustmentsPanel.tsx:386-401`).
pub(super) fn selected_layer_section() -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .border_b(px(1.0))
        .border_color(color::BORDER)
        .child(section_header(
            icons::with_stroke(icons::LAYERS, 1.75),
            &t("adjustments.selectedLayer"),
            None,
        ))
        .child(
            div()
                .mt(px(HEADER_GAP))
                .flex()
                .items_center()
                .gap(px(ROW_GAP))
                .rounded(px(R4))
                .border_1()
                .border_color(color::BORDER)
                .bg(color::FIELD)
                .p(px(BOX_PAD))
                .child(layer_thumb())
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .text_size(px(NAME_TEXT))
                                .font_family(ui::FONT_UI)
                                .font_weight(FontWeight(500.))
                                // `leading-tight`.
                                .line_height(relative(1.25))
                                .text_color(color::TEXT)
                                // Measured in the baseline, not the
                                // placeholder "Layer 1" the properties
                                // port uses: this capture's layer is
                                // named "Background".
                                .child("Background".to_string()), // i18n-exempt: demo content <layer name, captured frame>
                        )
                        // `mt-0.5` = 2px, and `leading-snug`.
                        .child(
                            div()
                                .mt(px(TYPE_MT))
                                .text_size(px(TYPE_TEXT))
                                .font_family(ui::FONT_UI)
                                .line_height(relative(1.375))
                                .text_color(color::TEXT_DIM)
                                // Raster layers read "Image layer"
                                // here, unlike the properties panel's
                                // "Raster layer" (`AdjustmentsPanel.tsx:399`).
                                .child("Image layer \u{00b7} 100 \u{00d7} 100 px".to_string()), // i18n-exempt: demo content <layer type+size, captured frame>
                        ),
                ),
        )
}

/// Basic section: reset, three sliders, hint and no-pixels status
/// (`AdjustmentsPanel.tsx:403-456`).
pub(super) fn basic_section() -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .border_b(px(1.0))
        .border_color(color::BORDER)
        .child(section_header(
            icons::with_stroke(icons::SUN, 1.75),
            &t("adjustments.basic"),
            Some(reset_button()),
        ))
        .child(
            v_flex()
                .mt(px(HEADER_GAP))
                .gap(px(8.0))
                .w_full()
                .child(slider_row(
                    &t("adjustments.bright"),
                    ui::SliderKind::Brightness,
                ))
                .child(slider_row(
                    &t("adjustments.contrast"),
                    ui::SliderKind::Contrast,
                ))
                .child(slider_row(
                    &t("adjustments.saturate"),
                    ui::SliderKind::Saturation,
                ))
                .child(
                    div()
                        .mt(px(DESC_MT))
                        .text_size(px(DESC_TEXT))
                        .font_family(ui::FONT_UI)
                        // `leading-snug`.
                        .line_height(relative(1.375))
                        .text_color(color::TEXT_DIM)
                        .child(t("adjustments.dragToPreview")),
                )
                .child(status_hint(
                    "This layer has no pixels yet. Add image pixels before adjusting tone.", // i18n-exempt: not in catalog
                )),
        )
}

/// `AdjustmentSliderRow` (`AdjustmentsPanel.tsx:490-533`): 58px label,
/// slider, and 40px value box.
///
/// Not [`ui::prop_row`]: this label is 58px, `text-[12px] font-medium`
/// `text-editor-text-dim`; `PropRow` uses 50px of 11px `text-[#A1A1AA]`.
fn slider_row(label: &str, kind: ui::SliderKind) -> Div {
    h_flex()
        .min_h(px(SLIDER_ROW_H))
        .items_center()
        .gap(px(ROW_GAP))
        .child(
            div()
                .w(px(SLIDER_LABEL_W))
                .flex_none()
                // Clip captions to the measured 58px label width.
                .overflow_hidden()
                .text_size(px(SLIDER_LABEL))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .text_color(color::TEXT_DIM)
                .child(label.to_string()),
        )
        .child(
            h_flex()
                .flex_1()
                .items_center()
                .gap(px(ROW_GAP))
                // The source's invisible range input needs this wrapper;
                // it centers the 4px slider in 18px.
                .child(
                    h_flex()
                        .flex_1()
                        .h(px(SLIDER_WRAP_H))
                        .items_center()
                        // At rest, centre-origin sliders use 50% and 0:
                        // no fill, only the centre tick and thumb; accent
                        // is unused by these kinds.
                        .child(ui::slider(50.0, Some(0.0), kind, true)),
                )
                .child(value_box(&format_display_value(0))),
        )
}

/// Slider readout: `flex h-[22px] w-[40px] items-center justify-end
/// rounded-[3px] border border-editor-field-border bg-editor-field
/// px-1.5 text-right text-[11px] tabular-nums text-editor-text`
/// (`AdjustmentsPanel.tsx:527`).
///
/// Not [`ui::num_field`]: this is 22px, caption-free, Inter, radius 3,
/// and right-aligned; that field is 24px, with a 10px caption and
/// mono semibold value.
fn value_box(value: &str) -> Div {
    h_flex()
        .w(px(VALUE_W))
        .h(px(VALUE_H))
        .flex_none()
        .items_center()
        .justify_end()
        .rounded(px(R3))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(VALUE_PAD_X))
        // Clip signed values: the 22px box fits resting "0", not "+8" or "-12".
        .overflow_hidden()
        .child(
            div()
                .text_size(px(VALUE_TEXT))
                .font_family(ui::FONT_UI)
                .text_right()
                .text_color(color::TEXT)
                .child(value.to_string()),
        )
}

/// Basic reset button classes (`AdjustmentsPanel.tsx:409-417`): `flex
/// size-5 items-center justify-center rounded-[3px] text-editor-text-dim
/// disabled:pointer-events-none disabled:opacity-40`.
fn reset_button() -> Div {
    div()
        .w(px(RESET))
        .h(px(RESET))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(R3))
        .flex_shrink_0()
        .opacity(0.4)
        .child(
            // `icons::X` is already pinned to stroke width 1.75.
            Icon::default()
                .data(icons::X)
                .size(px(RESET_ICON))
                .text_color(color::TEXT_DIM),
        )
}

/// No-pixels `StatusHint` (`AdjustmentsPanel.tsx:477-488`); the message
/// wraps to two lines, so `items-start` keeps the icon on the first
/// (`mt-0.5` = 2px).
fn status_hint(message: &str) -> Div {
    h_flex()
        .items_start()
        .gap(px(HINT_GAP))
        .rounded(px(R4))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(HINT_PAD_X))
        .py(px(HINT_PAD_Y))
        .child(
            // `icons::SLIDERS` is already pinned to stroke width 1.75.
            Icon::default()
                .data(icons::SLIDERS)
                .size(px(HINT_ICON))
                .text_color(color::TEXT_DIM)
                .mt(px(TYPE_MT))
                .flex_shrink_0(),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .text_size(px(HINT_TEXT))
                .font_family(ui::FONT_UI)
                // `leading-snug`.
                .line_height(relative(1.375))
                .text_color(color::TEXT_DIM)
                .child(message.to_string()),
        )
}

/// Collapsed `CollapsibleSection` (`AdjustmentsPanel.tsx:535-581`):
/// 42px row, 16px icon box, 15px icon, 8px gap, 12.5px regular label,
/// and 16px chevron. Closed sections omit descriptions and use
/// `chevron-right` rather than the expanded `chevron-up`.
pub(super) fn coming_soon_section(icon: &'static [u8], icon_colour: Hsla, label: &str) -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .border_b(px(1.0))
        .border_color(color::BORDER)
        .child(
            div()
                .h(px(COLLAPSE_H))
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .px(px(COLLAPSE_PX))
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(COLLAPSE_GAP))
                        .child(
                            div()
                                .w(px(COLLAPSE_ICON_BOX))
                                .h(px(COLLAPSE_ICON_BOX))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    // Call sites pass 1.75; adjust icons
                                    // pinned to a different stroke width.
                                    Icon::default()
                                        .data(icon)
                                        .size(px(COLLAPSE_ICON))
                                        .text_color(icon_colour),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(COLLAPSE_TEXT))
                                .font_family(ui::FONT_UI)
                                .text_color(color::TEXT)
                                .child(label.to_string()),
                        ),
                )
                // `icons::CHEVRON_RIGHT` is pinned to stroke width 1.75.
                .child(
                    Icon::default()
                        .data(icons::CHEVRON_RIGHT)
                        .size(px(CHEVRON))
                        .text_color(color::TEXT_DIM),
                ),
        )
}
