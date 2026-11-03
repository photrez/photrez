//! The Properties panel's Transform section: the live rows for the selected
//! layer and the ratio link that decides whether a size edit moves one axis or
//! both.

use super::*;

/// Whole pixels and whole degrees: the rows carry no fractional measure.
/// The shot path launches without an engine, so it has no layer to read. These
/// are the numbers the baseline paints for it: the demo document's full-canvas
/// base layer at 0,0 in an 800x600 page, unscaled. A live document answers
/// through `App::transform_readout`, which is what the mounted tests drive.
pub(super) const REST_X: &str = "0";
pub(super) const REST_Y: &str = "0";
pub(super) const REST_W: &str = "800";
pub(super) const REST_H: &str = "600";
pub(super) const REST_R: &str = "0";
pub(super) const REST_SCALE: &str = "100";

/// The opacity readout: whole percent, as the reference prints it.
pub(super) fn opacity_text(opacity: f64) -> String {
    format!("{} %", (opacity * 100.).round())
}

/// The same measure as the track's `0..=100` fill.
pub(super) fn opacity_percent(opacity: f64) -> f32 {
    (opacity.clamp(0., 1.) * 100.).round() as f32
}

/// One row's text: the layer's own measure, or the baseline number when the shot
/// path has no engine to read.
pub(super) fn row_text(value: Option<f64>, rest: &str) -> String {
    match value {
        Some(value) => format!("{}", value.round()),
        None => rest.to_string(),
    }
}

pub(super) fn section(
    state: &gpui_kit::Entity<crate::state::AppState>,
    readout: Option<crate::state::LayerTransformReadout>,
    cx: &Context<crate::app::App>,
) -> Div {
    // The layer's lock flags decide which hint, if any, sits above the rows.
    let status = {
        let s = state.read(cx);
        s.layer_flags
            .get(s.selected_layer as usize)
            .copied()
            .and_then(|flags| {
                if flags.locked {
                    Some((
                        "properties-status-layer-locked",
                        t("properties.layerLockedTransform"),
                    ))
                } else if flags.lock_position && flags.lock_rotation {
                    Some(("properties-status-pos-rot", t("properties.posRotLocked")))
                } else if flags.lock_position {
                    Some(("properties-status-pos", t("properties.posLocked")))
                } else if flags.lock_rotation {
                    Some(("properties-status-rot", t("properties.rotLocked")))
                } else {
                    None
                }
            })
    };
    let (session, focus, constrain, opacity, slider_bounds, slider_capture) = {
        let s = state.read(cx);
        (
            s.num_field.clone(),
            s.num_field_focus.clone(),
            s.constrain_ratio,
            s.layer_opacity,
            s.opacity_slider_bounds.clone(),
            s.slider_capture.clone(),
        )
    };
    let editing = |id: &'static str| {
        session
            .as_ref()
            .filter(|field| field.id == id)
            .map(|field| field.buffer.clone())
    };
    let open = crate::app::App::begin_transform_field;
    v_flex()
        .w_full()
        .flex_shrink_0()
        .px(px(PAD_X))
        .py(px(PAD_Y))
        .border_b(px(1.0))
        .border_color(color::DIVIDER)
        .child(section_header(icons::MOVE, &t("properties.transform")))
        .when(status.is_some(), move |el| match status {
            Some((id, message)) => el.child(status_hint(id, message)),
            None => el,
        })
        .child(
            v_flex().mt(px(HEADER_GAP)).gap(px(8.0)).w_full().children([
                // Position — X / Y, each taking half the slot.
                ui::prop_row(
                    &t("properties.position"),
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(ui::live_num_field(
                            "prop-x",
                            Some("X"), // i18n-exempt: technical token (field label glyph)
                            &row_text(readout.map(|layer| layer.x), REST_X),
                            Some("px"), // i18n-exempt: technical token (unit)
                            editing("prop-x").as_deref(),
                            focus.clone(),
                            open,
                            cx,
                        ))
                        .child(ui::live_num_field(
                            "prop-y",
                            Some("Y"), // i18n-exempt: technical token (field label glyph)
                            &row_text(readout.map(|layer| layer.y), REST_Y),
                            Some("px"), // i18n-exempt: technical token (unit)
                            editing("prop-y").as_deref(),
                            focus.clone(),
                            open,
                            cx,
                        )),
                )
                .into_any_element(),
                // Size — W / H plus the constrain-proportions toggle. The two
                // fields commit a scale, and the link decides whether the other
                // axis follows them.
                ui::prop_row(
                    &t("tools.options.size"),
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(ui::live_num_field(
                            "prop-w",
                            Some("W"), // i18n-exempt: technical token (field label glyph)
                            &row_text(readout.map(|layer| layer.width), REST_W),
                            Some("px"), // i18n-exempt: technical token (unit)
                            editing("prop-w").as_deref(),
                            focus.clone(),
                            open,
                            cx,
                        ))
                        .child(ui::live_num_field(
                            "prop-h",
                            Some("H"), // i18n-exempt: technical token (field label glyph)
                            &row_text(readout.map(|layer| layer.height), REST_H),
                            Some("px"), // i18n-exempt: technical token (unit)
                            editing("prop-h").as_deref(),
                            focus.clone(),
                            open,
                            cx,
                        ))
                        .child(constrain_toggle(constrain, cx)),
                )
                .into_any_element(),
                // Rotation — one field, "deg" rather than "px".
                ui::prop_row(
                    &t("properties.rotation"),
                    ui::live_num_field(
                        "prop-r",
                        Some("R"), // i18n-exempt: technical token (field label glyph)
                        &row_text(readout.map(|layer| layer.rotation), REST_R),
                        Some("deg"), // i18n-exempt: technical token (unit)
                        editing("prop-r").as_deref(),
                        focus,
                        open,
                        cx,
                    ),
                )
                .into_any_element(),
                // Scale — read-only `NumField`s, percent.
                ui::prop_row(
                    &t("properties.scale"),
                    h_flex()
                        .flex_1()
                        .gap(px(SLOT_GAP))
                        .child(ui::num_field(
                            Some("X"), // i18n-exempt: technical token (field label glyph)
                            None,
                            &row_text(readout.map(|layer| layer.scale_x * 100.), REST_SCALE),
                            Some("%"), // i18n-exempt: technical token (unit)
                        ))
                        .child(ui::num_field(
                            Some("Y"), // i18n-exempt: technical token (field label glyph)
                            None,
                            &row_text(readout.map(|layer| layer.scale_y * 100.), REST_SCALE),
                            Some("%"), // i18n-exempt: technical token (unit)
                        )),
                )
                .into_any_element(),
                // Opacity — the slider plus a right-aligned 44px readout, both
                // driven by the selected layer's own opacity.
                ui::prop_row(
                    &t("tools.options.opacity"),
                    h_flex()
                        .flex_1()
                        .items_center()
                        .gap(px(10.0))
                        // `h_flex` + `items_center`, not a bare block box: the
                        // wrapper is what centres the 4px track in its 24px row.
                        .child(
                            crate::views::option_bar::slider_surface::BarSliderSurface::new(
                                h_flex()
                                    .id("prop-opacity-track")
                                    .test_support()
                                    .flex_1()
                                    .h(px(24.0))
                                    .items_center()
                                    .aria_label(t("tools.options.opacity"))
                                    .child(ui::slider(
                                        opacity_percent(opacity),
                                        None,
                                        ui::SliderKind::Alpha,
                                        false,
                                    )),
                                cx.entity().downgrade(),
                                crate::state::SliderTarget::LayerOpacity,
                                slider_bounds,
                                slider_capture,
                                None,
                            ),
                        )
                        .child(
                            div()
                                .id("prop-opacity-value")
                                .test_support()
                                .w(px(44.0))
                                .flex_shrink_0()
                                .text_right()
                                .text_size(px(12.0))
                                .font_family(ui::FONT_UI)
                                .text_color(color::TEXT)
                                .child(opacity_text(opacity)),
                        ),
                )
                .into_any_element(),
                // Actions — three labelled buttons, wired to the transform
                // handlers (the additive strip that used to carry them is
                // gone; the inspector puts these here). Labels are catalog
                // keys, resolved from the `properties.*` entries: properties.flipH /
                // properties.flipV / common.reset.
                ui::prop_row(
                    &t("properties.actions"),
                    h_flex().flex_1().gap(px(6.0)).children(
                        ACTIONS
                            .iter()
                            .zip(ACTION_HANDLERS)
                            .map(|(&(svg, key), (id, action))| {
                                wired_action_button(svg, id, &t(key), cx, *action)
                            }),
                    ),
                )
                .into_any_element(),
                // Quick — icon-only buttons, so the labels live in `Tooltip`
                // wrappers that a static view cannot show.
                ui::prop_row(
                    "Quick", // i18n-exempt: not in catalog (the previous build's `t` resolves canvasProps.quickActions = "Quick Actions", not byte-identical)
                    h_flex()
                        .flex_1()
                        .gap(px(4.0))
                        .children(QUICK_ICONS.iter().map(|&svg| icon_button(svg))),
                )
                .into_any_element(),
            ]),
        )
}

/// 26px tall, stretches with its row: 4px corners, 1px border, 8px inset, 11px text.
///
/// Three of these share a 186px slot, 58px each. The chrome is therefore kept
/// at 28px (12px icon, 4px gap, 6px padding) so the widest label, "Flip H" at
/// 10.5px, still has ~2px of slack: the toolkit measures a label's min-content
/// as its whole string, so a heavier chrome makes the buttons sit at their
/// intrinsic widths and pushes the last one past the row's edge ("Reset" used
/// to clip to "Res").
/// The constrain-proportions link: a 26px square, accent with the `link` glyph
/// while the axes are held together and dim with `unlink` while they are free.
/// It writes the same state the Move bar's ratio button uses.
fn constrain_toggle(on: bool, cx: &Context<crate::app::App>) -> impl IntoElement {
    div()
        .id("prop-constrain-ratio")
        .test_support()
        .w(px(BTN))
        .h(px(BTN))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .cursor_pointer()
        .aria_label("Constrain proportions") // i18n-exempt: the reference hardcodes this accessible name
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            app.toggle_constrain_ratio(cx);
        }))
        .child(
            Icon::default()
                .data(if on { icons::LINK } else { icons::UNLINK })
                .size(px(14.0))
                .text_color(if on { color::ACCENT } else { color::TEXT_DIM }),
        )
}
