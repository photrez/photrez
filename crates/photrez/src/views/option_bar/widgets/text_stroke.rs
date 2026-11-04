use super::*;

/// The text stroke popover's 240px width.
const TEXT_STROKE_POPOVER_W: f32 = 240.0;

/// The text bar's stroke pill; clicking toggles the
/// stroke popover, not the outline.
pub(super) fn text_stroke_pill(
    width: f64,
    color_value: [u8; 4],
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let active = width > 0.0;
    div()
        .id("opt-text-stroke")
        .test_support()
        .h(px(CTRL_H))
        .w(px(STROKE_PILL_W))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_between()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(if active { color::ACCENT } else { color::BORDER })
        .bg(if active { ACCENT_20 } else { color::FIELD })
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(if active {
            FontWeight(600.)
        } else {
            FontWeight(500.)
        })
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
            app.toggle_text_stroke_popover(window, cx);
        }))
        .child(ui::clip(
            h_flex()
                .items_center()
                .gap(px(GAP))
                .child(
                    div()
                        .size(px(DOT))
                        .flex_shrink_0()
                        .rounded_full()
                        .border_1()
                        .border_color(color::BORDER)
                        .bg(if active {
                            crate::palette::hsla(color_value)
                        } else {
                            STROKE_DOT
                        }),
                )
                .child(div().text_color(ZINC).child("Stroke:")) // i18n-exempt: not in catalog (catalog has "Stroke" without colon)
                .child(
                    div()
                        .w(px(STROKE_VAL_W))
                        .flex_none()
                        .font_family(ui::FONT_MONO)
                        .text_color(if active { color::WHITE } else { ZINC })
                        .child(if active {
                            format!("{width:.0}px")
                        } else {
                            t("common.off")
                        }),
                ),
        ))
        .child(ui::pin(div().child(chevron_175())))
}

/// The text bar's stroke pill plus, when open, its popover.
// Field/slider focus and bounds remain distinct for their separate interactions.
#[allow(clippy::too_many_arguments)]
pub(in super::super) fn text_stroke_control(
    width: f64,
    color_value: [u8; 4],
    align: photrez_core::ptz::layer_params::TextStrokeAlign,
    open: bool,
    editing: Option<String>,
    field_focus: Option<gpui_kit::FocusHandle>,
    slider_focus: Option<gpui_kit::FocusHandle>,
    panel_bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    slider_bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    capture: std::rc::Rc<std::cell::Cell<Option<gpui_kit::HitboxId>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let mut wrapper =
        div()
            .relative()
            .flex_shrink_0()
            .child(text_stroke_pill(width, color_value, cx));
    if open {
        wrapper = wrapper.child(gpui_kit::deferred(
            super::select_surface::SelectPanelSurface::new(
                text_stroke_popover(
                    width,
                    color_value,
                    align,
                    editing,
                    field_focus,
                    slider_focus,
                    slider_bounds,
                    capture,
                    cx,
                ),
                panel_bounds,
            ),
        ));
    }
    wrapper
}

// The popover consumes each text-stroke control's state and interaction target directly.
#[allow(clippy::too_many_arguments)]
fn text_stroke_popover(
    width: f64,
    color_value: [u8; 4],
    align: photrez_core::ptz::layer_params::TextStrokeAlign,
    editing: Option<String>,
    field_focus: Option<gpui_kit::FocusHandle>,
    slider_focus: Option<gpui_kit::FocusHandle>,
    slider_bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    capture: std::rc::Rc<std::cell::Cell<Option<gpui_kit::HitboxId>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let enabled = width > 0.0;
    let switch = div()
        .id("opt-text-stroke-toggle")
        .test_support()
        .h(px(16.0))
        .w(px(28.0))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .rounded_full()
        .bg(if enabled {
            color::ACCENT
        } else {
            crate::palette::hsla([0x2D, 0x32, 0x3C, 0xFF])
        })
        .px(px(2.0))
        .when(enabled, |el| el.justify_end())
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            app.toggle_text_stroke(cx);
        }))
        .child(div().size(px(12.0)).rounded_full().bg(color::WHITE));

    let width_row = h_flex()
        .items_center()
        .justify_between()
        .gap(px(GAP))
        .child(dim_span(&t("common.width")))
        .child(
            h_flex()
                .items_center()
                .gap(px(GAP))
                .child(text_stroke_stepper("opt-text-stroke-dec", "-", -1.0, cx))
                .child(text_stroke_width_field(width, editing, field_focus, cx))
                .child(text_stroke_stepper("opt-text-stroke-inc", "+", 1.0, cx)),
        );

    let position_row = div()
        .flex()
        .flex_col()
        .gap(px(GAP))
        .child(dim_span(&t("properties.position")))
        .child(
            h_flex()
                .gap(px(2.0))
                .p(px(2.0))
                .rounded(px(RADIUS_SM))
                .border_1()
                .border_color(color::BORDER)
                .bg(crate::palette::hsla([0x17, 0x19, 0x1E, 0xFF]))
                .shadow(vec![crate::components::ui::inset_well()])
                .child(text_stroke_position_segment(
                    "opt-text-stroke-outside",
                    t("tools.options.strokeOutside"),
                    photrez_core::ptz::layer_params::TextStrokeAlign::Outside,
                    align == photrez_core::ptz::layer_params::TextStrokeAlign::Outside,
                    cx,
                ))
                .child(text_stroke_position_segment(
                    "opt-text-stroke-center",
                    t("tools.options.strokeCenter"),
                    photrez_core::ptz::layer_params::TextStrokeAlign::Center,
                    align == photrez_core::ptz::layer_params::TextStrokeAlign::Center,
                    cx,
                ))
                .child(text_stroke_position_segment(
                    "opt-text-stroke-inside",
                    t("tools.options.strokeInside"),
                    photrez_core::ptz::layer_params::TextStrokeAlign::Inside,
                    align == photrez_core::ptz::layer_params::TextStrokeAlign::Inside,
                    cx,
                )),
        );

    div()
        .id("opt-text-stroke-popover")
        .test_support()
        .absolute()
        .top(px(CTRL_H + 6.0))
        .right(px(0.0))
        .w(px(TEXT_STROKE_POPOVER_W))
        .flex()
        .flex_col()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::BAR_POPOVER)
        .p(px(PILL_PX))
        .occlude()
        .shadow(vec![
            BoxShadow::new(px(0.), px(6.), SHADOW_XS).blur_radius(px(18.))
        ])
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .gap(px(GAP))
                .pb(px(GAP))
                .border_b_1()
                .border_color(color::BORDER)
                .child(
                    div()
                        .flex_1()
                        .text_size(px(TEXT_SM))
                        .font_weight(FontWeight(600.))
                        .text_color(color::WHITE)
                        .child(t("tools.options.strokeOutline")),
                )
                .child(switch),
        )
        .child(
            div()
                .id("opt-text-stroke-body")
                .relative()
                .flex()
                .flex_col()
                .gap(px(GAP))
                .when(!enabled, |el| el.opacity(0.4))
                .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                    if app.effective_text_stroke_width(cx) == 0.0 {
                        app.toggle_text_stroke(cx);
                    }
                }))
                .child(width_row)
                .child(text_stroke_width_slider(
                    width,
                    slider_focus,
                    slider_bounds,
                    capture,
                    cx,
                ))
                .child(text_stroke_color_row(color_value, cx))
                .child(position_row),
        )
}

fn text_stroke_stepper(
    id: &'static str,
    label: &'static str,
    delta: f64,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .size(px(20.0))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_center()
        .rounded(px(3.0))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(700.))
        .text_color(color::WHITE)
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                if app.effective_text_stroke_width(cx) > 0.0 {
                    app.step_text_stroke(delta, cx);
                } else {
                    app.toggle_text_stroke(cx);
                }
            }),
        )
        .child(label)
}

fn text_stroke_width_field(
    width: f64,
    editing: Option<String>,
    focus: Option<gpui_kit::FocusHandle>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let active = editing.is_some();
    let text = match editing {
        Some(buffer) if buffer.is_empty() => "|".to_string(),
        Some(buffer) => format!("{buffer}|"),
        None => format!("{width:.0}"),
    };
    let mut field = h_flex()
        .id("opt-text-stroke-width")
        .test_support()
        .h(px(20.0))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(3.0))
        .border_1()
        .border_color(color::BORDER)
        .shadow(ui::outline_cue(active))
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
            if app.effective_text_stroke_width(cx) > 0.0 {
                app.begin_text_stroke_width_field(window, cx);
            } else {
                app.toggle_text_stroke(cx);
            }
        }))
        .child(
            div()
                .w(px(40.0))
                .text_center()
                .font_family(ui::FONT_MONO)
                .text_size(px(TEXT_SM))
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(text),
        );
    if let Some(handle) = focus {
        field = field.track_focus(&handle).on_key_down(cx.listener(
            |app, event: &gpui_kit::KeyDownEvent, window, cx| {
                app.num_field_key_in_row(
                    "opt-text-stroke-width",
                    event,
                    window,
                    move |app, window, cx| {
                        if app.effective_text_stroke_width(cx) > 0.0 {
                            app.begin_text_stroke_width_field(window, cx);
                        } else {
                            app.toggle_text_stroke(cx);
                        }
                    },
                    cx,
                );
            },
        ));
    }
    field
}

fn text_stroke_width_slider(
    width: f64,
    focus: Option<gpui_kit::FocusHandle>,
    slider_bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    capture: std::rc::Rc<std::cell::Cell<Option<gpui_kit::HitboxId>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let pct = width.clamp(0.0, 100.0) as f32;
    let visual = h_flex()
        .id("opt-text-stroke-slider")
        .test_support()
        .relative()
        .w_full()
        .items_center()
        .child(ui::slider(pct, None, ui::SliderKind::Default, true));
    let visual = super::slider_surface::track_focus_and_keys(
        visual,
        &focus,
        crate::state::SliderTarget::TextStrokeWidth,
        cx,
    );
    super::slider_surface::BarSliderSurface::new(
        visual,
        cx.entity().downgrade(),
        crate::state::SliderTarget::TextStrokeWidth,
        slider_bounds,
        capture,
        focus,
    )
}

fn text_stroke_color_row(color_value: [u8; 4], cx: &Context<crate::app::App>) -> impl IntoElement {
    let hex = crate::palette::to_hex(color_value).to_uppercase();
    h_flex()
        .items_center()
        .justify_between()
        .gap(px(GAP))
        .child(dim_span(&t("common.color")))
        .child(
            div()
                .id("opt-text-stroke-color")
                .test_support()
                .h(px(CTRL_H))
                .h_flex()
                .items_center()
                .gap(px(GAP))
                .rounded(px(RADIUS))
                .border_1()
                .border_color(color::BORDER)
                .bg(color::FIELD)
                .px(px(FIELD_PX))
                .cursor_pointer()
                .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
                    if app.effective_text_stroke_width(cx) > 0.0 {
                        app.open_color_picker(crate::state::ColorTarget::TextStroke, window, cx);
                    } else {
                        app.toggle_text_stroke(cx);
                    }
                }))
                .child(
                    div()
                        .font_family(ui::FONT_MONO)
                        .text_size(px(TEXT_SM))
                        .font_weight(FontWeight(600.))
                        .text_color(color::WHITE)
                        .child(hex),
                )
                .child(
                    div()
                        .size(px(12.0))
                        .flex_shrink_0()
                        .rounded(px(3.0))
                        .border_1()
                        .border_color(color::BORDER)
                        .bg(crate::palette::hsla(color_value)),
                ),
        )
}

fn text_stroke_position_segment(
    id: &'static str,
    label: String,
    value: photrez_core::ptz::layer_params::TextStrokeAlign,
    active: bool,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .flex_1()
        .h_flex()
        .items_center()
        .justify_center()
        .rounded(px(3.0))
        .py(px(4.0))
        .text_size(px(TEXT_XS))
        .font_weight(if active {
            FontWeight(700.)
        } else {
            FontWeight(600.)
        })
        .text_color(if active { color::WHITE } else { ZINC })
        .when(active, |el| el.bg(color::ACCENT))
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                if app.effective_text_stroke_width(cx) > 0.0 {
                    app.set_text_stroke_align(value, cx);
                } else {
                    app.toggle_text_stroke(cx);
                }
            }),
        )
        .child(label)
}
