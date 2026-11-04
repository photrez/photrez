use super::*;

/// The stroke popover's 224px width.
const STROKE_POPOVER_W: f32 = 224.0;

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
/// outline (stroke popover open state). The reference styles the pill identically open and closed, so
/// the edge does not track `open`.
pub(super) fn shape_stroke_pill(
    enabled: bool,
    width: f64,
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
pub(in super::super) fn stroke_control(
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
        .child(shape_stroke_pill(enabled, width, cx));
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

/// Opens the outline-width edit, or turns the outline on when the shape has
/// none. The click and a keystroke on the focused row take this same path.
fn begin_stroke_width(
    state: &gpui_kit::Entity<AppState>,
    app: &mut crate::app::App,
    window: &mut gpui_kit::Window,
    cx: &mut Context<crate::app::App>,
) {
    if state.read(cx).shape_stroke {
        app.begin_stroke_width_field(window, cx);
    } else {
        app.toggle_shape_stroke(cx);
    }
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
    let key_state = state.clone();
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
                begin_stroke_width(&state, app, window, cx);
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
        field = ui::tabbable_focus(field, &handle).on_key_down(cx.listener(
            move |app, event: &gpui_kit::KeyDownEvent, window, cx| {
                app.num_field_key_in_row(
                    "opt-stroke-width",
                    event,
                    window,
                    |app, window, cx| {
                        begin_stroke_width(&key_state, app, window, cx);
                    },
                    cx,
                );
            },
        ));
    }
    field
}
