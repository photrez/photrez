//! The Brush and Eraser bars: preset labels, the size and hardness controls, and
//! the More dropdown.

use super::*;

/// The `Brush` variant, also serving `Eraser`.
///
/// `eraser` selects the pill label, size string, slider percent and hardness.
pub(super) const BRUSH_PRESET_LABELS: [&str; 4] =
    ["Hard Round", "Soft Round", "Detail", "Large Soft"]; // i18n-exempt: not in catalog
pub(super) const ERASER_PRESET_LABELS: [&str; 6] = [
    "Hard Round",
    "Soft Round",
    "Detail",
    "Large Soft",
    "Hard Eraser",
    "Soft Eraser",
]; // i18n-exempt: not in catalog

pub(super) fn brush_items(
    state: &gpui_kit::Entity<AppState>,
    row: Div,
    eraser: bool,
    cx: &Context<crate::app::App>,
) -> Div {
    let (size, hardness, opacity, flow, smoothing, preset, open, cursor) = {
        let s = state.read(cx);
        if eraser {
            (
                s.eraser_size,
                s.eraser_hardness,
                s.eraser_opacity,
                s.eraser_flow,
                s.eraser_smoothing,
                s.brush_preset_index,
                s.open_select,
                s.select_cursor,
            )
        } else {
            (
                s.brush_size,
                s.brush_hardness,
                s.brush_opacity,
                s.brush_flow,
                s.brush_smoothing,
                s.brush_preset_index,
                s.open_select,
                s.select_cursor,
            )
        }
    };
    let labels: &'static [&'static str] = if eraser {
        &ERASER_PRESET_LABELS
    } else {
        &BRUSH_PRESET_LABELS
    };
    let preset_active = if preset < labels.len() {
        preset
    } else {
        usize::MAX
    };
    let preset_name = labels.get(preset).copied().unwrap_or("Custom"); // i18n-exempt: not in catalog
    let editing = |id: &'static str| {
        state
            .read(cx)
            .num_field
            .as_ref()
            .filter(|field| field.id == id)
            .map(|field| field.buffer.clone())
    };
    let focus = state.read(cx).num_field_focus.clone();
    let p = brush_params(eraser);
    row.child(tool_pill(
        icons::with_stroke(p.icon, STROKE_PILL),
        t(p.label),
    ))
    .child(divider())
    .child(brush_field(
        "opt-brush-size",
        &t("tools.options.size"),
        PAINT_INPUT_W,
        format!("{size:.0}"),
        "px",
        editing("opt-brush-size"),
        focus.clone(),
        cx,
    ))
    .when(show_paint_size_slider(), |el| {
        el.child(brush_size_slider(p.slider_pct))
    })
    .child(divider())
    .child(brush_field(
        "opt-brush-hardness",
        &t("tools.options.hard"),
        PAINT_INPUT_W_SM,
        format!("{:.0}", hardness * 100.0),
        "%",
        editing("opt-brush-hardness"),
        focus.clone(),
        cx,
    ))
    .child(brush_field(
        "opt-brush-opacity",
        &t("tools.options.strength"),
        PAINT_INPUT_W_SM,
        format!("{:.0}", opacity * 100.0),
        "%",
        editing("opt-brush-opacity"),
        focus.clone(),
        cx,
    ))
    .when(show_main_controls(), |el| {
        el.child(divider())
            .child(brush_field(
                "opt-brush-flow",
                &t("tools.options.flow"),
                PAINT_INPUT_W_SM,
                format!("{:.0}", flow * 100.0),
                "%",
                editing("opt-brush-flow"),
                focus.clone(),
                cx,
            ))
            .child(brush_field(
                "opt-brush-smoothing",
                &t("tools.options.smooth"),
                PAINT_INPUT_W_SM,
                format!("{smoothing:.0}"),
                "%",
                editing("opt-brush-smoothing"),
                focus.clone(),
                cx,
            ))
            .child(tooltip(
                state,
                "opt-brush-preset",
                t("tools.options.brushPresets"),
                None,
                cx,
                live_select(
                    "opt-brush-preset",
                    None,
                    None,
                    preset_name.to_string(),
                    labels,
                    if open == Some("opt-brush-preset") {
                        cursor
                    } else {
                        preset_active
                    },
                    open == Some("opt-brush-preset"),
                    state.read(cx).select_panel_bounds.clone(),
                    cx,
                ),
            ))
            .when(eraser, |el| {
                el.child(hard100_button("opt-brush-hard100", cx))
            })
    })
    .when(show_more_dropdown(), |el| {
        el.child(brush_more_dropdown(
            state,
            eraser,
            flow,
            smoothing,
            preset_name,
            labels,
            preset_active,
            focus,
            cx,
        ))
    })
}

/// The more-dropdown: the narrow bar folds the overflow
/// group into this button. The panel keeps Flow, Smoothing, the preset list
/// and the eraser's full-strength helper reachable below 880 px.
// Explicit option-bar values keep the dropdown builder aligned with its controls.
#[allow(clippy::too_many_arguments)]
pub(super) fn brush_more_dropdown(
    state: &gpui_kit::Entity<AppState>,
    eraser: bool,
    flow: f64,
    smoothing: f64,
    preset_name: &str,
    labels: &'static [&'static str],
    preset_active: usize,
    focus: Option<gpui_kit::FocusHandle>,
    cx: &Context<crate::app::App>,
) -> Div {
    let (open, preset_open, cursor) = {
        let s = state.read(cx);
        (
            s.more_open,
            s.open_select == Some("opt-brush-preset"),
            s.select_cursor,
        )
    };
    let editing = |id: &'static str| {
        state
            .read(cx)
            .num_field
            .as_ref()
            .filter(|field| field.id == id)
            .map(|field| field.buffer.clone())
    };
    // The press may land on the icon inside the button, so the clickable id
    // sits on the wrapper and the visual button is its child.
    let trigger =
        div()
            .id("opt-more")
            .test_support()
            .flex_shrink_0()
            .cursor_pointer()
            .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, window, cx| {
                app.toggle_more_menu(window, cx)
            }))
            .child(more_dropdown_button());
    let mut wrapper = div().relative().flex_shrink_0().child(trigger);
    if open {
        let mut panel = div()
            .id("opt-more-panel")
            .test_support()
            .absolute()
            .top(px(CTRL_H + 4.0))
            .left(px(0.0))
            .w(px(200.0))
            .flex()
            .flex_col()
            .gap(px(GAP))
            .rounded(px(RADIUS))
            .border_1()
            .border_color(color::BORDER)
            .bg(color::BAR_POPOVER)
            .p(px(8.0))
            .occlude()
            .child(brush_field(
                "opt-brush-flow",
                &t("tools.options.flow"),
                PAINT_INPUT_W_SM,
                format!("{:.0}", flow * 100.0),
                "%",
                editing("opt-brush-flow"),
                focus.clone(),
                cx,
            ))
            .child(brush_field(
                "opt-brush-smoothing",
                &t("tools.options.smooth"),
                PAINT_INPUT_W_SM,
                format!("{smoothing:.0}"),
                "%",
                editing("opt-brush-smoothing"),
                focus,
                cx,
            ))
            .child(tooltip(
                state,
                "opt-brush-preset",
                t("tools.options.brushPresets"),
                None,
                cx,
                live_select(
                    "opt-brush-preset",
                    None,
                    None,
                    preset_name.to_string(),
                    labels,
                    if preset_open { cursor } else { preset_active },
                    preset_open,
                    state.read(cx).select_panel_bounds.clone(),
                    cx,
                ),
            ));
        if eraser {
            panel = panel.child(hard100_button("opt-more-hard100", cx));
        }
        wrapper = wrapper.child(panel);
    }
    wrapper
}
