use super::*;
use crate::app::tool_settings::ToolFlag;
use crate::i18n::t;

mod text;
pub(super) use text::*;

// ── Move-variant parts ────────────────────────────────────────────────────────

/// One Move-bar transform cell. With a live layer it shows the layer's own
/// number: editable for X/Y/R, read-only for the size pair. A locked layer
/// keeps the reference's disabled look, and no layer at all falls back to the
/// inert placeholder.
#[allow(clippy::too_many_arguments)] // one cell: its box, value and edit session stay positional
pub(super) fn transform_cell(
    id: &'static str,
    label: &'static str,
    width: f32,
    value: Option<f64>,
    locked: bool,
    suffix: &'static str,
    editing: Option<String>,
    focus: Option<gpui_kit::FocusHandle>,
    cx: &Context<crate::app::App>,
) -> gpui_kit::AnyElement {
    match value {
        Some(value) if !locked => transform_field(
            id,
            label,
            width,
            super::variants::selection_field_value(value),
            suffix,
            editing,
            focus,
            cx,
        )
        .into_any_element(),
        Some(value) => dimmed(editable_field_text(
            id,
            label,
            width,
            super::variants::selection_field_value(value),
            suffix,
        ))
        .into_any_element(),
        None => dimmed(editable_field(id, label, width, "0", suffix)).into_any_element(),
    }
}

/// A non-shrinking, centred row (the shared inner gap) around the live X and Y
/// fields.
pub(super) fn position_group(
    readout: Option<crate::state::LayerTransformReadout>,
    editing: impl Fn(&'static str) -> Option<String>,
    focus: Option<gpui_kit::FocusHandle>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let locked = readout.is_some_and(|readout| readout.locked);
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ITEM_GAP))
        .child(transform_cell(
            "num-x",
            "X", // i18n-exempt: technical token (field label glyph)
            FIELD_W_XY,
            readout.map(|readout| readout.x),
            locked,
            "px", // i18n-exempt: technical token (field label glyph)
            editing("num-x"),
            focus.clone(),
            cx,
        ))
        .child(transform_cell(
            "num-y",
            "Y", // i18n-exempt: technical token (field label glyph)
            FIELD_W_XY,
            readout.map(|readout| readout.y),
            locked,
            "px", // i18n-exempt: technical token (field label glyph)
            editing("num-y"),
            focus,
            cx,
        ))
}

/// A non-shrinking, centred row (the shared inner gap) around the W and H
/// fields. The reference keeps these two read-only: they are the layer's
/// scaled frame, and the size changes by dragging the frame, not by typing.
pub(super) fn size_group(readout: Option<crate::state::LayerTransformReadout>) -> impl IntoElement {
    let size = |value: Option<f64>| {
        value.map_or_else(|| "0".to_string(), |value| format!("{}", value.round()))
    };
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(ITEM_GAP))
        .child(num_field(
            "W",
            FIELD_W_WH,
            size(readout.map(|r| r.width)),
            "px",
        )) // i18n-exempt: technical token (field label glyph)
        .child(num_field(
            "H",
            FIELD_W_WH,
            size(readout.map(|r| r.height)),
            "px",
        )) // i18n-exempt: technical token (field label glyph)
}

/// The brush size slider, gated behind `PAINT_SIZE_SLIDER`.
pub(super) fn brush_size_slider(percent: f32) -> Div {
    h_flex()
        .relative()
        .flex_shrink_0()
        .w(px(BRUSH_SLIDER_W))
        .h(px(BRUSH_SLIDER_H))
        .items_center()
        .child(ui::slider(percent, None, ui::SliderKind::BrushSize, true))
}

// ── Eyedropper ─────────────────────────────────────────────────────────────────

/// The eyedropper's foreground colour square.
pub(super) fn fg_swatch_square() -> Div {
    let mut shadows = vec![ring_white_20()];
    shadows.extend(shadow_sm_layers());
    div()
        .size(px(ICON_LG))
        .flex_shrink_0()
        .rounded(px(RADIUS_SM))
        .border_1()
        .border_color(color::BORDER)
        .bg(FG)
        .shadow(shadows)
}

/// The "click to copy HEX" badge.
pub(super) fn hex_badge() -> impl IntoElement {
    div()
        .id("opt-copy-hex")
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(color::WHITE)
        .child(value_text("#E15A17".to_string()))
        .child(
            Icon::default()
                .data(icons::with_stroke(icons::COPY, 2.0))
                .size(px(ICON_SM))
                .text_color(ZINC),
        )
}

/// One of the five tolerance preset chips.
pub(super) fn tolerance_preset(
    id: &'static str,
    value: &'static str,
    active: bool,
    cx: &gpui_kit::Context<crate::app::App>,
    on_click: impl Fn(&mut crate::app::App, &mut gpui_kit::Context<crate::app::App>) + 'static,
) -> impl IntoElement {
    let chip = div()
        .id(id)
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .px(px(FIELD_PX))
        .font_family(ui::FONT_MONO)
        .text_size(px(TEXT_XS))
        .bg(if active { ACCENT_20 } else { FIELD_40 })
        .text_color(if active { color::WHITE } else { ZINC })
        .font_weight(if active {
            FontWeight(700.)
        } else {
            FontWeight(500.)
        })
        .cursor_pointer()
        .on_click(cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| on_click(app, cx)));

    if active {
        chip.shadow(vec![
            BoxShadow::new(px(0.), px(1.), SHADOW_XS).blur_radius(px(2.))
        ])
    } else {
        chip
    }
    .child(value)
}

/// The live tolerance field: click to edit, keystrokes commit live.
pub(super) fn tolerance_num_field(
    value: u32,
    editing: Option<String>,
    focus: Option<gpui_kit::FocusHandle>,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let active = editing.is_some();
    let text = match editing {
        Some(buffer) if buffer.is_empty() => "|".to_string(),
        Some(buffer) => format!("{buffer}|"),
        None => value.to_string(),
    };
    let mut field = div()
        .id("opt-tolerance")
        .test_support()
        .h(px(CTRL_H))
        .w(px(TOLERANCE_FIELD_W))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .justify_end()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .shadow(ui::outline_cue(active))
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
            app.begin_fill_tolerance_field(window, cx);
        }))
        .child(
            div()
                .font_family(ui::FONT_MONO)
                .text_size(px(TEXT_SM))
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(text),
        );
    if let Some(handle) = focus {
        field = field.track_focus(&handle).on_key_down(cx.listener(
            |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                app.num_field_key(event, cx);
            },
        ));
    }
    field
}

/// The tolerance slider wrapper: a live track —
/// click or drag maps the pointer's x position over the laid-out track.
pub(super) fn tolerance_slider(
    state: &gpui_kit::Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let (pct, bounds, capture, focus) = {
        let s = state.read(cx);
        (
            (s.fill_tolerance as f32 / 255.0) * 100.0,
            s.tolerance_slider_bounds.clone(),
            s.slider_capture.clone(),
            s.slider_focus.clone(),
        )
    };
    let mut visual = h_flex()
        .id("opt-tolerance-slider")
        .test_support()
        .relative()
        .flex_shrink_0()
        .w(px(TOLERANCE_SLIDER_W))
        .items_center()
        .child(ui::slider(pct, None, ui::SliderKind::Alpha, true));
    if let Some(handle) = focus.as_ref() {
        visual = visual.track_focus(handle).on_key_down(cx.listener(
            |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                app.slider_key(event, cx);
            },
        ));
    }
    super::slider_surface::BarSliderSurface::new(
        visual,
        cx.entity().downgrade(),
        crate::state::SliderTarget::Tolerance,
        bounds,
        capture,
        focus,
    )
}

// ── Gradient ───────────────────────────────────────────────────────────────────

/// The two endpoints the preview strip must paint, taken from the live swatch
/// colours rather than any constant. The order matches the fill path in
/// `app/input.rs`: the FG-to-transparent preset wins, then `reverse` swaps the
/// pair; otherwise the strip runs foreground to background.
pub(super) fn gradient_preview(
    fg: [u8; 4],
    bg: [u8; 4],
    fg_transparent: bool,
    reverse: bool,
) -> (Hsla, Hsla) {
    let fg = crate::palette::hsla(fg);
    let bg = crate::palette::hsla(bg);
    if fg_transparent {
        (fg, Hsla { a: 0.0, ..fg })
    } else if reverse {
        (bg, fg)
    } else {
        (fg, bg)
    }
}

/// The gradient preview strip, painted between the two endpoints the tool will
/// actually fill with. Callers derive `from`/`to` from live state with
/// [`gradient_preview`].
pub(super) fn gradient_strip(from: Hsla, to: Hsla) -> Div {
    let mut shadows = vec![
        crate::components::ui::inset_well(),
        BoxShadow::new(px(0.), px(0.), color::WHITE_10).spread_radius(px(1.)),
    ];
    shadows.extend(shadow_sm_layers());
    div()
        .relative()
        .flex_shrink_0()
        .h(px(STRIP_H))
        .w(px(STRIP_W))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .overflow_hidden()
        .bg(STRIP_BG)
        .shadow(shadows)
        .child(
            div()
                .absolute()
                .inset(px(0.))
                .bg(checkerboard(STRIP_CHECK, 8.)),
        )
        .child(div().absolute().inset(px(0.)).bg(linear_gradient(
            90.0,
            linear_color_stop(from, 0.0),
            linear_color_stop(to, 1.0),
        )))
}

/// A round gradient-end swatch.
pub(super) fn round_swatch(fill: Hsla) -> Div {
    let mut shadows = vec![ring_white_20()];
    shadows.extend(shadow_sm_layers());
    div()
        .size(px(ICON_MD))
        .flex_shrink_0()
        .rounded_full()
        .border_1()
        .border_color(color::BORDER)
        .bg(fill)
        .shadow(shadows)
}

/// The gradient bar's Reverse button.
pub(super) fn reverse_button(cx: &gpui_kit::Context<crate::app::App>) -> impl IntoElement {
    div()
        .id("opt-reverse")
        .test_support()
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            app.toggle_tool_flag(ToolFlag::GradientReverse, cx);
        }))
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(FIELD_40)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(ZINC)
        .child(div().text_size(px(TEXT_MD)).child("\u{21c4}")) // i18n-exempt: glyph or symbol
        .child(div().child(t("tools.options.reverse")))
}

// ── Shape ──────────────────────────────────────────────────────────────────────

/// The shape bar's Fill toggle, accent while
/// filled and field-coloured while off.
pub(super) fn fill_toggle(enabled: bool, cx: &Context<crate::app::App>) -> impl IntoElement {
    div()
        .id("opt-fill")
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(if enabled { ACCENT_20 } else { color::FIELD })
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(if enabled {
            FontWeight(600.)
        } else {
            FontWeight(500.)
        })
        .text_color(if enabled { color::WHITE } else { ZINC })
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            app.toggle_shape_fill(cx);
        }))
        .child(t("tools.fill"))
}

/// The shape bar's 22px fill colour swatch: the selected shape's fill in edit
/// mode, the paint foreground while drawing the next shape. Clicking opens the
/// picker on [`crate::state::ColorTarget::ShapeFill`].
pub(super) fn fill_swatch(color_value: [u8; 4], cx: &Context<crate::app::App>) -> impl IntoElement {
    div()
        .id("opt-fill-color")
        .test_support()
        .size(px(SWATCH_FILL))
        .flex_shrink_0()
        .rounded(px(RADIUS_SM))
        .border_1()
        .border_color(color::BORDER)
        .bg(crate::palette::hsla(color_value))
        .shadow(vec![ring_white_20()])
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
            app.open_color_picker(crate::state::ColorTarget::ShapeFill, window, cx);
        }))
}

/// The stroke pill's off-state dot.
pub(super) fn stroke_dot() -> Div {
    div()
        .size(px(DOT))
        .flex_shrink_0()
        .rounded_full()
        .border_1()
        .border_color(color::BORDER)
        .bg(STROKE_DOT)
}

/// The shape bar's stroke pill; clicking toggles the stroke popover, not the
/// outline (stroke popover open state).
pub(super) fn shape_stroke_pill(
    enabled: bool,
    width: f64,
    open: bool,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let active = enabled && width > 0.0;
    div()
        .id("opt-shape-stroke")
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
        .border_color(if active {
            color::ACCENT
        } else if open {
            HOVER_BORDER_LOCAL
        } else {
            color::BORDER
        })
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
            app.toggle_stroke_popover(window, cx);
        }))
        .child(ui::clip(
            h_flex()
                .items_center()
                .gap(px(GAP))
                .child(stroke_dot())
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

/// The stroke pill plus, when open, its popover.
///
/// The popover is 224px wide with a header checkbox, a colour row and a
/// width row; when the outline is off the body is dimmed and a click on it
/// enables the outline as the dimmed wrapper itself does.
// Explicit stroke state and hit-test bounds keep popover construction readable.
#[allow(clippy::too_many_arguments)]
pub(super) fn stroke_control(
    state: &gpui_kit::Entity<AppState>,
    enabled: bool,
    width: f64,
    color_value: [u8; 4],
    open: bool,
    editing: Option<String>,
    focus: Option<gpui_kit::FocusHandle>,
    panel_bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let mut wrapper = div()
        .relative()
        .flex_shrink_0()
        .child(shape_stroke_pill(enabled, width, open, cx));
    if open {
        wrapper = wrapper.child(gpui_kit::deferred(
            super::select_surface::SelectPanelSurface::new(
                stroke_popover(state, enabled, width, color_value, editing, focus, cx),
                panel_bounds,
            ),
        ));
    }
    wrapper
}

fn stroke_popover(
    state: &gpui_kit::Entity<AppState>,
    enabled: bool,
    width: f64,
    color_value: [u8; 4],
    editing: Option<String>,
    focus: Option<gpui_kit::FocusHandle>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let on_off = if enabled {
        t("common.on")
    } else {
        t("common.off")
    };
    let body = div()
        .flex()
        .flex_col()
        .gap(px(GAP))
        .when(!enabled, |el| el.opacity(0.4))
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .gap(px(GAP))
                .child(dim_span(&t("common.color")))
                .child(stroke_color_button(state, color_value, enabled, cx)),
        )
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .gap(px(GAP))
                .child(dim_span(&t("common.width")))
                .child(stroke_width_field(state, width, editing, focus, cx)),
        );
    div()
        .id("opt-stroke-popover")
        .test_support()
        .absolute()
        .top(px(CTRL_H + 6.0))
        .left(px(0.0))
        .w(px(STROKE_POPOVER_W))
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
                .child(option_checkbox_action(
                    "opt-stroke-enable",
                    &on_off,
                    enabled,
                    None,
                    cx,
                    |app, cx| app.toggle_shape_stroke(cx),
                )),
        )
        .child(body)
}

/// The popover's colour button: swatch plus uppercase hex, opening the picker
/// on [`crate::state::ColorTarget::ShapeStroke`]. While the outline is off a
/// click enables it, since the dimmed body wrapper ignores pointer events.
fn stroke_color_button(
    state: &gpui_kit::Entity<AppState>,
    color_value: [u8; 4],
    enabled: bool,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let state = state.clone();
    let swatch = if enabled {
        crate::palette::hsla(color_value)
    } else {
        crate::palette::hsla([0x2A, 0x2E, 0x37, 0xFF])
    };
    div()
        .id("opt-stroke-color")
        .test_support()
        .h_flex()
        .h(px(CTRL_H))
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, window, cx| {
                if state.read(cx).shape_stroke {
                    app.open_color_picker(crate::state::ColorTarget::ShapeStroke, window, cx);
                } else {
                    app.toggle_shape_stroke(cx);
                }
            }),
        )
        .child(
            div()
                .size(px(12.0))
                .flex_shrink_0()
                .rounded_full()
                .border_1()
                .border_color(color::BORDER)
                .bg(swatch),
        )
        .child(
            div()
                .font_family(ui::FONT_MONO)
                .text_size(px(TEXT_SM))
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(crate::palette::to_hex(color_value).to_uppercase()),
        )
}

/// The popover's outline width, edited inline like the corner-radius field.
fn stroke_width_field(
    state: &gpui_kit::Entity<AppState>,
    width: f64,
    editing: Option<String>,
    focus: Option<gpui_kit::FocusHandle>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let state = state.clone();
    let active = editing.is_some();
    let text = match editing {
        Some(buffer) if buffer.is_empty() => "|".to_string(),
        Some(buffer) => format!("{buffer}|"),
        None => format!("{width:.0}"),
    };
    let mut field = h_flex()
        .id("opt-stroke-width")
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .shadow(ui::ring_cue(active))
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, window, cx| {
                if state.read(cx).shape_stroke {
                    app.begin_stroke_width_field(window, cx);
                } else {
                    app.toggle_shape_stroke(cx);
                }
            }),
        )
        .child(
            div()
                .w(px(RADIUS_INPUT_W))
                .flex_none()
                .text_center()
                .font_family(ui::FONT_MONO)
                .text_size(px(TEXT_SM))
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(text),
        )
        .child(dim_span("px")); // i18n-exempt: technical token (unit)
    if let Some(handle) = focus {
        field = field.track_focus(&handle).on_key_down(cx.listener(
            |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                app.num_field_key(event, cx);
            },
        ));
    }
    field
}

/// The text bar's stroke pill; clicking toggles the
/// stroke popover, not the outline.
pub(super) fn text_stroke_pill(
    width: f64,
    color_value: [u8; 4],
    open: bool,
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
        .border_color(if active {
            color::ACCENT
        } else if open {
            HOVER_BORDER_LOCAL
        } else {
            color::BORDER
        })
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
pub(super) fn text_stroke_control(
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
            .child(text_stroke_pill(width, color_value, open, cx));
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
            |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                app.num_field_key(event, cx);
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
    let mut visual = h_flex()
        .id("opt-text-stroke-slider")
        .test_support()
        .relative()
        .w_full()
        .items_center()
        .child(ui::slider(pct, None, ui::SliderKind::Default, true));
    if let Some(handle) = focus.as_ref() {
        visual = visual.track_focus(handle).on_key_down(cx.listener(
            |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                app.text_stroke_slider_key(event, cx);
            },
        ));
    }
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

/// The line shape's arrow-head toggle.
pub(super) fn shape_arrow_toggle(enabled: bool, cx: &Context<crate::app::App>) -> impl IntoElement {
    div()
        .id("opt-shape-arrow")
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(if enabled { ACCENT_20 } else { color::FIELD })
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(if enabled {
            FontWeight(600.)
        } else {
            FontWeight(500.)
        })
        .text_color(if enabled { color::WHITE } else { ZINC })
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            app.toggle_tool_flag(crate::app::tool_settings::ToolFlag::ShapeArrowHead, cx);
        }))
        .child(t("tools.options.shapes.arrow"))
}

/// The shape bar's corner-radius field. Clicking starts an inline edit; valid
/// keystrokes commit live, Enter ends, Escape restores the value.
pub(super) fn radius_field(
    radius: f64,
    editing: Option<String>,
    focus: Option<gpui_kit::FocusHandle>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let active = editing.is_some();
    let text = match editing {
        Some(buffer) if buffer.is_empty() => "|".to_string(),
        Some(buffer) => format!("{buffer}|"),
        None => format!("{radius:.0}"),
    };
    let mut field = h_flex()
        .id("opt-shape-radius")
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .shadow(ui::ring_cue(active))
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
            app.begin_shape_radius_field(window, cx);
        }))
        .child(dim_span(&t("properties.cornerRadius")))
        .child(
            div()
                .w(px(RADIUS_INPUT_W))
                .flex_none()
                .text_center()
                .font_family(ui::FONT_MONO)
                .text_size(px(TEXT_SM))
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(text),
        )
        .child(dim_span("px")); // i18n-exempt: technical token (unit)
    if let Some(handle) = focus {
        field = field.track_focus(&handle).on_key_down(cx.listener(
            |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                app.num_field_key(event, cx);
            },
        ));
    }
    field
}

/// The selection bar's X/Y/W/H/R field. Enabled
/// only while a pixel selection is committed; clicking starts an inline edit
/// whose commit rewrites that one dimension through core history.
// Display and edit inputs stay explicit to preserve per-field selection behavior.
#[allow(clippy::too_many_arguments)]
/// Opens one of the selection bar's five fields on click.
pub(super) fn selection_field(
    id: &'static str,
    label: &str,
    width: f32,
    value: String,
    suffix: &'static str,
    editing: Option<String>,
    focus: Option<gpui_kit::FocusHandle>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    numeric_field(
        id,
        label,
        width,
        value,
        suffix,
        editing,
        focus,
        crate::app::App::begin_selection_field,
        cx,
    )
}

/// One of the Move bar's editable transform fields (X, Y, R).
#[allow(clippy::too_many_arguments)] // the field box, its value and its edit session stay positional
pub(super) fn transform_field(
    id: &'static str,
    label: &str,
    width: f32,
    value: String,
    suffix: &'static str,
    editing: Option<String>,
    focus: Option<gpui_kit::FocusHandle>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    numeric_field(
        id,
        label,
        width,
        value,
        suffix,
        editing,
        focus,
        crate::app::App::begin_transform_field,
        cx,
    )
}

/// The shared live numeric field: click opens the edit session `open` seeds,
/// the ring cue marks the active one, and the caret run stands in for the
/// buffer while typing.
#[allow(clippy::too_many_arguments)] // geometry, value, session and opener of one box
pub(super) fn numeric_field(
    id: &'static str,
    label: &str,
    width: f32,
    value: String,
    suffix: &'static str,
    editing: Option<String>,
    focus: Option<gpui_kit::FocusHandle>,
    open: fn(
        &mut crate::app::App,
        &'static str,
        &mut gpui_kit::Window,
        &mut gpui_kit::Context<crate::app::App>,
    ),
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let active = editing.is_some();
    let text = match editing {
        Some(buffer) if buffer.is_empty() => "|".to_string(),
        Some(buffer) => format!("{buffer}|"),
        None => value,
    };
    let mut field = h_flex()
        .id(id)
        .test_support()
        .h(px(CTRL_H))
        .w(px(width))
        .flex_shrink_0()
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .shadow(ui::ring_cue(active))
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .overflow_hidden()
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, window, cx| {
                open(app, id, window, cx);
            }),
        )
        .child(dim_span(label))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .whitespace_nowrap()
                .font_family(ui::FONT_MONO)
                .text_size(px(TEXT_SM))
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(text),
        )
        .child(dim_span(suffix));
    if let Some(handle) = focus {
        field = field.track_focus(&handle).on_key_down(cx.listener(
            |app, event: &gpui_kit::KeyDownEvent, _window, cx| {
                app.num_field_key(event, cx);
            },
        ));
    }
    field
}

// ── Shared hover colour ───────────────────────────────────────────────────────

/// `#4B515D` — the hover border for editable fields and the overflow button.
pub(super) const HOVER_BORDER_LOCAL: Hsla = Hsla {
    h: 0.611111,
    s: 0.107143,
    l: 0.329412,
    a: 1.0,
};

/// The stroke popover's 224px width.
const STROKE_POPOVER_W: f32 = 224.0;

/// The text stroke popover's 240px width.
const TEXT_STROKE_POPOVER_W: f32 = 240.0;

// ── Group labels ───────────────────────────────────────────────────────────────

/// The selection bar's caps captions ("Position", "Size", "Rotation").
pub(super) fn caps_label(text: &str) -> Div {
    div()
        .mr(px(MR_LABEL))
        .text_size(px(TEXT_XS))
        .font_weight(FontWeight(700.))
        .text_color(color::TEXT_DIM)
        .child(text.to_string())
}

// ── Crop ───────────────────────────────────────────────────────────────────────

/// The crop fill controls: the white colour swatch + "Use BG"
/// button, shown when `fill_enabled` is true. At the capture's rest state the
/// fill source resolves to the white background default (the crop's
/// resolved fill colour), so the swatch paints [`color::WHITE`].
///
/// The swatch is a 22x18 rect with a 2px corner radius, not
/// the circular `round_swatch` - it is flat (no ring) to match the baseline.
pub(super) fn crop_fill_swatch_group() -> Stateful<Div> {
    div()
        .id("opt-crop-use-bg")
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .gap(px(ITEM_GAP))
        .rounded(px(RADIUS_SM))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(FIELD_PX))
        .child(
            div()
                .h(px(18.0))
                .w(px(22.0))
                .flex_shrink_0()
                .rounded(px(RADIUS_SM))
                .border_1()
                .border_color(color::BORDER)
                .bg(color::WHITE),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(TEXT_XS))
                .text_color(ZINC)
                .font_weight(FontWeight(500.))
                .child("Use BG"), // i18n-exempt: not in catalog
        )
}

/// `t("common.apply", "Apply")` — accent action:
/// 24px tall, 10px padding, 4px radius, bold white text and a subtle shadow, with the fill at
/// 15%. Live via the Crop bar, so [`ACTION_PX`] is consumed (the dead-code
/// allow on it was dropped in mod.rs).
///
/// The fill: the resting `option-crop` baseline blends the button interior to
/// `#3a2b20` (58, 43, 32) over the `#1b1b1b` bar - [`color::ACCENT_15`], not
/// the solid accent fill that the transform variant's own markup spells
/// out. No baseline captures the transform bar's Apply, so the measured crop
/// value is what paints here.
pub(super) fn apply_button(cx: &gpui_kit::Context<crate::app::App>) -> impl IntoElement {
    div()
        .id("opt-apply")
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::ACCENT_15)
        .px(px(ACTION_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(700.))
        .text_color(color::WHITE)
        .cursor_pointer()
        .shadow(vec![
            BoxShadow::new(px(0.), px(1.), SHADOW_XS).blur_radius(px(2.))
        ])
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            let result = app.apply_crop(cx);
            match result {
                Ok(()) => app.set_status_error(None, cx),
                Err(error) => app.set_status_error(Some(format!("Crop: {error}")), cx),
            }
        }))
        .child(t("common.apply"))
}

/// `t("common.cancel", "Cancel")` — secondary
/// action. The markup asks for a field background and a `#A1A1AA` label, but the
/// resting `option-crop` baseline paints the border-only ghost: the interior
/// stays the bar's `#1b1b1b` (27) where the field background would read 37. The
/// border and the label paint; the fill does not.
pub(super) fn cancel_button(cx: &gpui_kit::Context<crate::app::App>) -> impl IntoElement {
    div()
        .id("opt-cancel")
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .h_flex()
        .items_center()
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .px(px(ACTION_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(600.))
        .text_color(ZINC)
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            app.cancel_crop(cx);
        }))
        .child(t("common.cancel"))
}
