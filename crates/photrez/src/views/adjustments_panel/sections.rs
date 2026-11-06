use super::*;

/// Section-header row: 16px icon box,
/// 15px icon, 8px gap, 12.5px medium label, and optional trailing item.
/// Call sites use stroke width 1.75; `icons::with_stroke` adjusts icons
/// pinned to a different width.
fn section_header(icon: &'static [u8], label: &str, trailing: Option<gpui_kit::AnyElement>) -> Div {
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

/// Layer thumbnail frame: 34x34, 1px border, 3px corners. This static
/// view has no preview bitmap, and the captured layer stores no
/// bitmap entry, so the empty chrome is the resting content.
///
/// The design asks for an accent border here,
/// but the global border colour resets it to `#333333` (see module docs).
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

/// The Selected Layer section.
pub(super) fn selected_layer_section(name: &str, layer_type: &str, width: u32, height: u32) -> Div {
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
                                // Line height 1.25.
                                .line_height(relative(1.25))
                                .text_color(color::TEXT)
                                .child(name.to_string()),
                        )
                        // 2px top margin, and line height 1.375.
                        .child(
                            div()
                                .mt(px(TYPE_MT))
                                .text_size(px(TYPE_TEXT))
                                .font_family(ui::FONT_UI)
                                .line_height(relative(1.375))
                                .text_color(color::TEXT_DIM)
                                // Raster layers read "Image layer"
                                // here, unlike the properties panel's
                                // "Raster layer" in the properties panel.
                                .child(format!(
                                    "{layer_type} layer \u{00b7} {width} \u{00d7} {height} px"
                                )),
                        ),
                ),
        )
}

/// Basic section: reset, three sliders, hint and no-pixels status.
pub(super) fn basic_section(
    state: &gpui_kit::Entity<crate::state::AppState>,
    cx: &Context<crate::app::App>,
) -> Div {
    let (bright, contrast, saturate, has_pixels, has_pending) = {
        let s = state.read(cx);
        (
            s.adjustment_brightness.round() as i32,
            s.adjustment_contrast.round() as i32,
            s.adjustment_saturation.round() as i32,
            s.active_layer_has_pixels,
            s.adjustment_brightness != 0.0
                || s.adjustment_contrast != 0.0
                || s.adjustment_saturation != 0.0,
        )
    };
    let mut reset = reset_button().id("adj-reset").test_support();
    if has_pending {
        reset = reset.cursor_pointer().opacity(1.0);
    }
    let reset = reset
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                if has_pending {
                    let result = app.reset_selected_adjustment(cx);
                    match result {
                        Ok(()) => app.set_status_error(None, cx),
                        Err(error) => app.set_status_error(Some(format!("Adjust: {error}")), cx),
                    }
                }
            }),
        )
        .into_any_element();
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
            Some(reset),
        ))
        .child(
            v_flex()
                .mt(px(HEADER_GAP))
                .gap(px(8.0))
                .w_full()
                .child(slider_row(
                    &t("adjustments.bright"),
                    ui::SliderKind::Brightness,
                    bright,
                    crate::state::SliderTarget::AdjustmentBrightness,
                    "adj-bright-track",
                    state,
                    cx,
                ))
                .child(slider_row(
                    &t("adjustments.contrast"),
                    ui::SliderKind::Contrast,
                    contrast,
                    crate::state::SliderTarget::AdjustmentContrast,
                    "adj-contrast-track",
                    state,
                    cx,
                ))
                .child(slider_row(
                    &t("adjustments.saturate"),
                    ui::SliderKind::Saturation,
                    saturate,
                    crate::state::SliderTarget::AdjustmentSaturation,
                    "adj-saturate-track",
                    state,
                    cx,
                ))
                .child(
                    div()
                        .mt(px(DESC_MT))
                        .text_size(px(DESC_TEXT))
                        .font_family(ui::FONT_UI)
                        // Line height 1.375.
                        .line_height(relative(1.375))
                        .text_color(color::TEXT_DIM)
                        .child(t("adjustments.dragToPreview")),
                )
                .when(!has_pixels, |col| {
                    col.child(status_hint(
                        "This layer has no pixels yet. Add image pixels before adjusting tone.", // i18n-exempt: not in catalog
                    ))
                }),
        )
}

/// Slider row: 58px label,
/// slider, and 40px value box.
///
/// Not [`ui::prop_row`]: this label is 58px of 12px medium dim text;
/// `PropRow` uses 50px of 11px `#A1A1AA`.
fn slider_row(
    label: &str,
    kind: ui::SliderKind,
    value: i32,
    target: crate::state::SliderTarget,
    id: &'static str,
    state: &gpui_kit::Entity<crate::state::AppState>,
    cx: &Context<crate::app::App>,
) -> Div {
    use crate::views::option_bar::slider_surface::BarSliderSurface;
    let (capture, bounds, focus) = {
        let s = state.read(cx);
        // The same channel numbering the drag path and core command use.
        let channel = match target {
            crate::state::SliderTarget::AdjustmentBrightness => 0,
            crate::state::SliderTarget::AdjustmentContrast => 1,
            _ => 2,
        };
        (
            s.slider_capture.clone(),
            s.adjustment_slider_bounds[channel].clone(),
            s.slider_focus_for(target),
        )
    };
    // Centre-origin: 0 sits at 50 %, the ends are -100 / +100.
    let percent = ((f64::from(value) + 100.0) / 2.0) as f32;
    let track = div()
        .id(id)
        .test_support()
        .flex_1()
        .h(px(SLIDER_WRAP_H))
        .aria_label(label.to_string());
    let track =
        crate::views::option_bar::slider_surface::track_focus_and_keys(track, &focus, target, cx);
    let track = track.child(BarSliderSurface::new(
        // Centre-origin: no fill, the centre tick and
        // thumb; accent is unused by these kinds.
        ui::slider(percent, Some(0.0), kind, true),
        cx.entity().downgrade(),
        target,
        bounds,
        capture,
        focus,
    ));
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
                .child(track)
                .child(value_box(&format_display_value(value))),
        )
}

/// Slider readout: a 22x40px box, right-aligned, 11px tabular numerals,
/// 3px corners, a field-border edge on the field fill, 6px inset.
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

/// Basic reset button: a centred 20px square, 3px corners, dim text,
/// 40% opacity and no pointer events once disabled.
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

/// The no-pixels status hint; the message
/// wraps to two lines, so the icon aligns to the first
/// (2px top margin).
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
                // Line height 1.375.
                .line_height(relative(1.375))
                .text_color(color::TEXT_DIM)
                .child(message.to_string()),
        )
}

/// A collapsed section row:
/// 42px row, 16px icon box, 15px icon, 8px gap, 12.5px regular label,
/// and 16px chevron. Closed sections omit descriptions and use
/// `chevron-right` rather than the expanded `chevron-up`.
/// A not-yet-built adjustment group. Collapsed it is the 42px header row;
/// expanded it adds the 0.1 build's dashed In Development card, which is the
/// only place the panel says these tools are unfinished.
pub(super) fn coming_soon_section(
    id: &'static str,
    icon: &'static [u8],
    icon_colour: Hsla,
    label: &str,
    open: bool,
    cx: &Context<crate::app::App>,
) -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .border_b(px(1.0))
        .border_color(color::BORDER)
        .child(
            div()
                .id(id)
                .test_support()
                .h(px(COLLAPSE_H))
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .px(px(COLLAPSE_PX))
                .cursor_pointer()
                .on_click(
                    cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                        app.toggle_adjustment_section(id, cx);
                    }),
                )
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
                .child(
                    Icon::default()
                        .data(if open {
                            icons::CHEVRON_UP
                        } else {
                            // `icons::CHEVRON_RIGHT` is pinned to stroke width 1.75.
                            icons::CHEVRON_RIGHT
                        })
                        .size(px(CHEVRON))
                        .text_color(color::TEXT_DIM),
                ),
        )
        .when(open, |section| section.child(in_development_card(id)))
}

/// The card the 0.1 build expands under every unfinished adjustment group: a
/// dashed well holding the In Development eyebrow and one description line.
/// `bg-white/[0.015]` is the well's fill; the runtime expresses no
/// letter-spacing, so the uppercase eyebrow runs untracked.
fn in_development_card(id: &'static str) -> impl IntoElement {
    const CARD_FILL: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 0.015,
    };
    const EYEBROW_OPACITY: f32 = 0.7;
    v_flex()
        .id(format!("adj-card-{id}"))
        .test_support()
        .w_full()
        .flex_shrink_0()
        .px(px(16.0))
        .pt(px(8.0))
        .pb(px(20.0))
        .child(
            v_flex()
                .w_full()
                .gap(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_dashed()
                .border_color(color::FIELD_BORDER)
                .bg(CARD_FILL)
                .p(px(12.0))
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            Icon::default()
                                .data(icons::SPARKLES)
                                .size(px(12.0))
                                .text_color(color::TEXT_DIM.opacity(EYEBROW_OPACITY)),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .font_family(ui::FONT_UI)
                                .font_weight(FontWeight(700.))
                                .text_color(color::TEXT_DIM)
                                .child(t("adjustments.inDevelopment").to_uppercase()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_family(ui::FONT_UI)
                        .line_height(relative(1.625))
                        .text_color(color::TEXT_DIM)
                        .child(t("adjustments.defaultInDevDesc")),
                ),
        )
}
