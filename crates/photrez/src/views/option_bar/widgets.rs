use super::*;
use crate::app::tool_settings::ToolFlag;
use crate::i18n::t;

mod stroke;
mod text;
mod text_stroke;

pub(super) use stroke::*;
pub(super) use text::*;
pub(super) use text_stroke::*;

// ── Move-variant parts ────────────────────────────────────────────────────────

/// One Move-bar transform cell. With a live layer it shows the layer's own
/// number: editable for X/Y/R, read-only for the size pair. A locked layer
/// keeps the disabled look, and no layer at all falls back to the inert
/// placeholder.
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
    focus_for: impl Fn(&'static str) -> Option<gpui_kit::FocusHandle>,
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
            focus_for("num-x"),
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
            focus_for("num-y"),
            cx,
        ))
}

/// A non-shrinking, centred row (the shared inner gap) around the W and H
/// fields. This cell takes no typed size, so a typed size belongs to the
/// Properties rows.
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
        field = ui::tabbable_focus(field, &handle).on_key_down(cx.listener(
            |app, event: &gpui_kit::KeyDownEvent, window, cx| {
                app.num_field_key_in_row(
                    "opt-tolerance",
                    event,
                    window,
                    move |app, window, cx| {
                        app.begin_fill_tolerance_field(window, cx);
                    },
                    cx,
                );
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
            s.slider_focus_for(crate::state::SliderTarget::Tolerance),
        )
    };
    let visual = h_flex()
        .id("opt-tolerance-slider")
        .test_support()
        .relative()
        .flex_shrink_0()
        .w(px(TOLERANCE_SLIDER_W))
        .items_center()
        .child(ui::slider(pct, None, ui::SliderKind::Alpha, true));
    let visual = super::slider_surface::track_focus_and_keys(
        visual,
        &focus,
        crate::state::SliderTarget::Tolerance,
        cx,
    );
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
        field = ui::tabbable_focus(field, &handle).on_key_down(cx.listener(
            |app, event: &gpui_kit::KeyDownEvent, window, cx| {
                app.num_field_key_in_row(
                    "opt-shape-radius",
                    event,
                    window,
                    move |app, window, cx| {
                        app.begin_shape_radius_field(window, cx);
                    },
                    cx,
                );
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
        field = ui::tabbable_focus(field, &handle).on_key_down(cx.listener(
            move |app, event: &gpui_kit::KeyDownEvent, window, cx| {
                app.num_field_key_in_row(
                    id,
                    event,
                    window,
                    move |app, window, cx| {
                        open(app, id, window, cx);
                    },
                    cx,
                );
            },
        ));
    }
    field
}

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
/// The fill: the resting Crop bar blends the button interior to
/// `#3a2b20` (58, 43, 32) over the `#1b1b1b` bar - [`color::ACCENT_15`], not
/// the solid accent fill. No measurement covers the transform bar's Apply, so
/// the measured Crop value is what paints here.
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
/// action. The resting `option-crop` baseline paints the border-only ghost:
/// the interior stays the bar's `#1b1b1b` (27) where the field background
/// would read 37. The border and the `#A1A1AA` label paint; the fill does not.
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
