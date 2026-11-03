//! The Shape bar: kind options, their labels and indices, and the fill, stroke
//! and radius controls.

use super::*;

pub(super) const SHAPE_KIND_OPTIONS: [&str; 10] = [
    "Rect", "Ellipse", "Line", "Triangle", "Star", "Arrow", "Heart", "Diamond", "Bubble", "Hexagon",
]; // i18n-exempt: not in catalog

pub(super) fn shape_label(kind: ShapeTool) -> &'static str {
    match kind {
        ShapeTool::Rect => "Rect",
        ShapeTool::Ellipse => "Ellipse",
        ShapeTool::Line => "Line",
        ShapeTool::Triangle => "Triangle",
        ShapeTool::Star => "Star",
        ShapeTool::BlockArrow => "Arrow",
        ShapeTool::Heart => "Heart",
        ShapeTool::Diamond => "Diamond",
        ShapeTool::SpeechBubble => "Bubble",
        ShapeTool::Hexagon => "Hexagon",
    }
}

pub(super) fn shape_kind_index(kind: ShapeTool) -> usize {
    match kind {
        ShapeTool::Rect => 0,
        ShapeTool::Ellipse => 1,
        ShapeTool::Line => 2,
        ShapeTool::Triangle => 3,
        ShapeTool::Star => 4,
        ShapeTool::BlockArrow => 5,
        ShapeTool::Heart => 6,
        ShapeTool::Diamond => 7,
        ShapeTool::SpeechBubble => 8,
        ShapeTool::Hexagon => 9,
    }
}

/// The `Shape` variant: draw mode writes the next-shape
/// defaults, edit mode reads the selected shape layer's live parameters.
pub(super) fn shape_items(
    state: &gpui_kit::Entity<AppState>,
    row: Div,
    cx: &Context<crate::app::App>,
) -> Div {
    use photrez_core::ptz::layer_params::ShapeFillKind;

    let (
        kind,
        open,
        cursor,
        fill,
        fill_color,
        stroke,
        stroke_width,
        stroke_color,
        stroke_open,
        stroke_editing,
        stroke_panel_bounds,
        radius,
        arrow,
        editing,
        focus,
    ) = {
        let s = state.read(cx);
        let layer = s.selected_shape.as_ref();
        (
            layer.map_or(s.shape_kind, |params| {
                crate::app::tool_settings::shape_tool_for_kind(&params.kind)
            }),
            s.open_select,
            s.select_cursor,
            layer.map_or(s.shape_fill, |params| {
                params.fill.kind == ShapeFillKind::Solid
            }),
            layer
                .and_then(|params| crate::palette::parse_hex(&params.fill.color))
                .unwrap_or(s.fg_color),
            layer.map_or(s.shape_stroke, |params| params.stroke.enabled),
            layer.map_or(s.shape_stroke_width, |params| params.stroke.width),
            layer
                .and_then(|params| crate::palette::parse_hex(&params.stroke.color))
                .unwrap_or(s.shape_stroke_color),
            s.stroke_popover_open,
            s.num_field
                .as_ref()
                .filter(|field| field.id == "opt-stroke-width")
                .map(|field| field.buffer.clone()),
            s.stroke_panel_bounds.clone(),
            layer.map_or(s.shape_radius, |params| params.radius),
            layer.map_or(s.shape_arrow_head, |params| params.arrow_head),
            s.num_field
                .as_ref()
                .filter(|field| field.id == "opt-shape-radius")
                .map(|field| field.buffer.clone()),
            s.num_field_focus.clone(),
        )
    };
    row.child(tool_pill(
        icons::with_stroke(icons::RECTANGLE, STROKE_PILL),
        PILL_LABEL_SHAPE.to_string(),
    ))
    .child(divider())
    .child(tooltip(
        state,
        "opt-shape-kind",
        "Select shape", // i18n-exempt: not in catalog (accessibility label only)
        None,
        cx,
        live_select(
            "opt-shape-kind",
            Some(icons::RECTANGLE),
            None,
            if kind == ShapeTool::Rect {
                t("tools.options.shapes.rect")
            } else {
                shape_label(kind).to_string()
            },
            &SHAPE_KIND_OPTIONS,
            if open == Some("opt-shape-kind") {
                cursor
            } else {
                shape_kind_index(kind)
            },
            open == Some("opt-shape-kind"),
            state.read(cx).select_panel_bounds.clone(),
            cx,
        ),
    ))
    .child(divider())
    .child(
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(ITEM_GAP))
            .child(fill_toggle(fill, cx))
            .when(fill, |el| {
                el.child(tooltip(
                    state,
                    "opt-fill-color",
                    t("tools.options.fillColor"),
                    None,
                    cx,
                    fill_swatch(fill_color, cx),
                ))
            }),
    )
    .child(divider())
    .child(tooltip(
        state,
        "opt-shape-stroke",
        t("tools.options.strokeOptions"),
        None,
        cx,
        stroke_control(
            state,
            stroke,
            stroke_width,
            stroke_color,
            stroke_open,
            stroke_editing,
            focus.clone(),
            stroke_panel_bounds,
            cx,
        ),
    ))
    .when(kind == ShapeTool::Rect, |el| {
        el.child(divider())
            .child(radius_field(radius, editing, focus, cx))
    })
    .when(kind == ShapeTool::Line, |el| {
        el.child(divider()).child(shape_arrow_toggle(arrow, cx))
    })
}
