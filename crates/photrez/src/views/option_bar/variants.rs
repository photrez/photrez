use super::*;
use crate::app::tool_settings::ToolFlag;
use crate::state::ShapeTool;
use gpui_kit::TestSupportExt;

/// One centred row with a 6px gap — one variant renders inside.
mod brush;
mod crop;
mod paint_tools;
mod shape;
mod text;

use brush::*;
pub use crop::crop_params;
use crop::*;
use paint_tools::*;
use shape::*;
use text::*;

pub(super) fn fade_in_row(
    state: &gpui_kit::Entity<AppState>,
    active_tool: &ToolId,
    committed_selection: Option<photrez_core::document::content::SelectionState>,
    readout: Option<crate::state::LayerTransformReadout>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let row = h_flex().items_center().gap(px(GAP));
    let row = match active_tool {
        ToolId::Move => move_items(state, row, readout, cx),
        ToolId::Selection => selection_items(state, row, committed_selection, cx),
        ToolId::Crop => crop_items(state, row, cx),
        ToolId::Brush => brush_items(state, row, false, cx),
        ToolId::Eraser => brush_items(state, row, true, cx),
        ToolId::Eyedropper => eyedropper_items(state, row, cx),
        ToolId::PaintBucket => paint_bucket_items(state, row, cx),
        ToolId::Gradient => gradient_items(state, row, cx),
        ToolId::Shape => shape_items(state, row, cx),
        ToolId::Text => text_items(state, row, cx),
    };
    row.id(match active_tool {
        ToolId::Move => "option-bar-move",
        ToolId::Selection => "option-bar-selection",
        ToolId::Crop => "option-bar-crop",
        ToolId::Eyedropper => "option-bar-eyedropper",
        ToolId::Brush => "option-bar-brush",
        ToolId::Eraser => "option-bar-eraser",
        ToolId::PaintBucket => "option-bar-paint-bucket",
        ToolId::Gradient => "option-bar-gradient",
        ToolId::Shape => "option-bar-shape",
        ToolId::Text => "option-bar-text",
    })
    .test_support()
}
// ── Move ──────────────────────────────────────────────────────────────────────

/// The `Move` variant.
///
/// The transform cells read the layer the canvas is holding, so the numbers
/// describe the same layer the frame, the align group and Reset act on.
/// State assumptions for the rest of the bar: nothing is hovered, and
/// move-auto-select is on.
fn move_items(
    state: &gpui_kit::Entity<AppState>,
    row: Div,
    readout: Option<crate::state::LayerTransformReadout>,
    cx: &Context<crate::app::App>,
) -> Div {
    let (snap_focus, auto_select_focus, transform_focus, more_open) = {
        let s = state.read(cx);
        (
            s.move_snap_focus.clone(),
            s.move_auto_select_focus.clone(),
            s.move_transform_focus.clone(),
            s.more_open,
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
    let num_focus = |id: &'static str| state.read(cx).num_field_focus_for(id);
    let locked = readout.is_some_and(|readout| readout.locked);
    row.child(tool_pill(
        icons::with_stroke(icons::CURSOR, STROKE_PILL),
        t(PILL_LABEL_MOVE_KEY),
    ))
    .child(divider())
    .child(tooltip(
        state,
        "opt-auto-select",
        t("tools.options.autoSelectTip"),
        None,
        cx,
        option_checkbox_action(
            "opt-auto-select",
            &t("tools.options.autoSelect"),
            state.read(cx).move_auto_select,
            auto_select_focus,
            cx,
            |app, cx| app.toggle_move_auto_select(cx),
        ),
    ))
    .child(tooltip(
        state,
        "opt-snap",
        t("tools.options.snapTip"),
        None,
        cx,
        option_checkbox_action(
            "opt-snap",
            &t("tools.options.snap"),
            state.read(cx).snap_enabled,
            snap_focus,
            cx,
            |app, cx| app.toggle_snap(cx),
        ),
    ))
    .child(divider())
    .child(tooltip(
        state,
        "opt-transform",
        t("tools.options.transformControlsTip"),
        None,
        cx,
        option_checkbox_action(
            "opt-transform",
            &t("properties.transform"),
            state.read(cx).show_transform_controls,
            transform_focus,
            cx,
            |app, cx| app.toggle_transform_controls(cx),
        ),
    ))
    .child(divider())
    .child(position_group(readout, &editing, &num_focus, cx))
    .child(size_group(readout))
    .child(transform_cell(
        "num-r",
        "R", // i18n-exempt: technical token (field label glyph)
        FIELD_W_R,
        readout.map(|readout| readout.rotation),
        locked,
        "\u{b0}", // i18n-exempt: technical token (field label glyph)
        editing("num-r"),
        num_focus("num-r"),
        cx,
    ))
    .when(show_main_controls(), |el| {
        el.child(main_controls(locked, cx))
    })
    .when(show_more_dropdown(), |el| {
        el.child(move_more_dropdown(state, more_open, locked, cx))
    })
}

/// The Move bar's `MoreDropdown` body: the Align and Flip groups and
/// Reset Transform, the controls the 0.1 build folds below 880 px. A locked
/// selection keeps the design's inert look: dimmed sections, clickable nothing.
fn move_more_dropdown(
    state: &gpui_kit::Entity<AppState>,
    open: bool,
    locked: bool,
    cx: &Context<crate::app::App>,
) -> Div {
    let align_section = overflow_section(
        overflow_caption(&t("properties.align")),
        overflow_group_row()
            .child(overflow_icon_button(
                "opt-move-align-left",
                icons::ALIGN_LEFT,
                !locked,
                cx,
                align_clicked(0),
            ))
            .child(overflow_icon_button(
                "opt-move-align-center-h",
                icons::ALIGN_H,
                !locked,
                cx,
                align_clicked(1),
            ))
            .child(overflow_icon_button(
                "opt-move-align-right",
                icons::ALIGN_RIGHT,
                !locked,
                cx,
                align_clicked(2),
            ))
            .child(overflow_icon_button(
                "opt-move-align-top",
                icons::ALIGN_TOP,
                !locked,
                cx,
                align_clicked(3),
            ))
            .child(overflow_icon_button(
                "opt-move-align-center-v",
                icons::ALIGN_V,
                !locked,
                cx,
                align_clicked(4),
            ))
            .child(overflow_icon_button(
                "opt-move-align-bottom",
                icons::ALIGN_BOTTOM,
                !locked,
                cx,
                align_clicked(5),
            )),
        locked,
    );
    let flip_section = overflow_section(
        overflow_caption(&t("properties.flip")),
        overflow_group_row()
            .child(overflow_icon_button(
                "opt-move-flip-h",
                icons::FLIP_H,
                !locked,
                cx,
                flip_clicked(true),
            ))
            .child(overflow_icon_button(
                "opt-move-flip-v",
                icons::FLIP_V,
                !locked,
                cx,
                flip_clicked(false),
            )),
        locked,
    );
    let body = gpui_kit::base::v_flex()
        .gap(px(ITEM_GAP))
        .child(align_section)
        .child(flip_section)
        .child(overflow_divider())
        .child(overflow_action_button(
            "opt-move-reset",
            t("properties.resetTransform"),
            None,
            !locked,
            true,
            cx,
            reset_transform_clicked,
        ));
    more_dropdown_overflow(open, state, cx, body)
}

/// One labelled overflow group: the 0.1 build's caption-plus-row section,
/// dimmed as a whole while the selection is locked.
fn overflow_section(caption: Div, body: impl IntoElement, locked: bool) -> Div {
    let section = gpui_kit::base::v_flex()
        .gap(px(ITEM_GAP))
        .child(caption)
        .child(body);
    if locked {
        section.opacity(0.3)
    } else {
        section
    }
}

// ── Selection ────────────────────────────────────────────────────────────────

/// `EditableNumField`'s `Math.round(val * 100) / 100` display: two decimals
/// at most, trailing zeros trimmed.
pub(super) fn selection_field_value(value: f64) -> String {
    let rounded = format!("{value:.2}");
    let trimmed = rounded.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}

/// The `Selection` variant.
///
/// Without a committed selection every numeric field is in its disabled
/// state (opacity 40 %, inert, reading "0"). With one, the fields read the
/// selection and their commits rewrite that dimension through core history.
fn selection_items(
    state: &gpui_kit::Entity<AppState>,
    row: Div,
    committed: Option<photrez_core::document::content::SelectionState>,
    cx: &Context<crate::app::App>,
) -> Div {
    let (shape, open, cursor, more_open) = {
        let s = state.read(cx);
        (
            s.selection_shape,
            s.open_select,
            s.select_cursor,
            s.more_open,
        )
    };
    let elliptical = shape == crate::state::SelectionShape::Ellipse;
    let editing = |id: &'static str| {
        state
            .read(cx)
            .num_field
            .as_ref()
            .filter(|field| field.id == id)
            .map(|field| field.buffer.clone())
    };
    let focus = |id: &'static str| state.read(cx).num_field_focus_for(id);
    let field = |id: &'static str,
                 label: &'static str,
                 width: f32,
                 value: f64,
                 suffix: &'static str,
                 cx: &Context<crate::app::App>| {
        selection_field(
            id,
            label,
            width,
            selection_field_value(value),
            suffix,
            editing(id),
            focus(id),
            cx,
        )
    };
    row.child(tool_pill(
        icons::with_stroke(
            if elliptical {
                icons::CIRCLE_DASHED
            } else {
                icons::SQUARE_DASHED
            },
            STROKE_PILL,
        ),
        t("tools.rectSelect"),
    ))
    .child(divider())
    .child(live_select(
        "opt-select-shape",
        Some(if elliptical {
            icons::CIRCLE_DASHED
        } else {
            icons::SQUARE_DASHED
        }),
        None,
        if elliptical {
            t("tools.ellipseSelect")
        } else {
            t("tools.rectSelect")
        },
        &["Rectangular", "Elliptical"], // i18n-exempt: not in catalog
        if open == Some("opt-select-shape") {
            cursor
        } else if elliptical {
            1
        } else {
            0
        },
        open == Some("opt-select-shape"),
        state.read(cx).select_panel_bounds.clone(),
        cx,
    ))
    .child(divider())
    .child(select_dropdown(
        None,
        Some(format!("{}: ", t("tools.options.style")).as_str()),
        t("tools.options.normal"),
    ))
    .child(divider())
    .when(show_group_labels_960(), |el| {
        el.child(caps_label(&t("properties.position")))
    })
    .when_some(committed.as_ref(), |el, selection| {
        el.child(field_pair(
            field("sel-x", "X", FIELD_W_XY, selection.x, "px", cx),
            field("sel-y", "Y", FIELD_W_XY, selection.y, "px", cx),
        ))
    })
    .when(committed.is_none(), |el| {
        el.child(field_pair(
            dimmed(editable_field("sel-x", "X", FIELD_W_XY, "0", "px")), // i18n-exempt: technical token (field label glyph)
            dimmed(editable_field("sel-y", "Y", FIELD_W_XY, "0", "px")), // i18n-exempt: technical token (field label glyph)
        ))
    })
    .child(divider())
    .when(show_group_labels_960(), |el| {
        el.child(caps_label(&t("tools.options.size")))
    })
    .when_some(committed.as_ref(), |el, selection| {
        el.child(field_pair(
            field("sel-w", "W", FIELD_W_XY, selection.width, "px", cx),
            field("sel-h", "H", FIELD_W_XY, selection.height, "px", cx),
        ))
    })
    .when(committed.is_none(), |el| {
        el.child(field_pair(
            dimmed(editable_field("sel-w", "W", FIELD_W_XY, "0", "px")), // i18n-exempt: technical token (field label glyph)
            dimmed(editable_field("sel-h", "H", FIELD_W_XY, "0", "px")), // i18n-exempt: technical token (field label glyph)
        ))
    })
    .child(divider())
    .when(show_group_labels_960(), |el| {
        el.child(caps_label(&t("properties.rotation")))
    })
    .when_some(committed.as_ref(), |el, selection| {
        el.child(field(
            "sel-r",
            "R",
            FIELD_W_R,
            selection.angle,
            "\u{b0}",
            cx,
        ))
    })
    .when(committed.is_none(), |el| {
        el.child(dimmed(editable_field(
            "sel-r", "R", FIELD_W_R, "0", "\u{b0}",
        ))) // i18n-exempt: technical token (field label glyph)
    })
    .when(show_more_dropdown(), |el| {
        el.child(selection_more_dropdown(
            state,
            more_open,
            committed.is_some(),
            cx,
        ))
    })
}

/// The Selection bar's `MoreDropdown`: the clipboard grid the 0.1 build folds
/// at narrow widths. Without a committed
/// selection every row keeps the inert state that build set with
/// `pointer-events-none`. Its first row, the Transform toggle, is absent here: the
/// native edit mode is only reachable through `ToggleSelectionEditModeAction`,
/// whose type stays inside the private `app::selection` module, so a bar click
/// has no entry point yet.
fn selection_more_dropdown(
    state: &gpui_kit::Entity<AppState>,
    open: bool,
    has_selection: bool,
    cx: &Context<crate::app::App>,
) -> Div {
    use crate::app::{
        CopySelectionAction, CutSelectionAction, DeselectAction, InvertSelectionAction,
        PasteSelectionAction,
    };
    let grid = gpui_kit::base::v_flex()
        .gap(px(ITEM_GAP))
        .child(
            h_flex()
                .gap(px(ITEM_GAP))
                .child(overflow_action_button(
                    "opt-sel-cut",
                    t("menus.items.cut"),
                    Some(icons::SLICE),
                    has_selection,
                    true,
                    cx,
                    |app, cx| app.cut_selection_action(&CutSelectionAction, cx),
                ))
                .child(overflow_action_button(
                    "opt-sel-copy",
                    t("menus.items.copy"),
                    Some(icons::COPY),
                    has_selection,
                    true,
                    cx,
                    |app, cx| app.copy_selection_action(&CopySelectionAction, cx),
                )),
        )
        .child(
            h_flex()
                .gap(px(ITEM_GAP))
                .child(overflow_action_button(
                    "opt-sel-paste",
                    t("menus.items.paste"),
                    Some(icons::SQUARE_DASHED),
                    has_selection,
                    true,
                    cx,
                    |app, cx| app.paste_selection_action(&PasteSelectionAction, cx),
                ))
                .child(overflow_action_button(
                    "opt-sel-invert",
                    t("menus.items.invertSelection"),
                    Some(icons::FLIP_H),
                    has_selection,
                    true,
                    cx,
                    |app, cx| app.invert_selection_action(&InvertSelectionAction, cx),
                )),
        )
        .child(
            h_flex()
                .gap(px(ITEM_GAP))
                .child(overflow_action_button(
                    "opt-sel-delete",
                    t("layers.deleteLayer"),
                    Some(icons::TRASH),
                    has_selection,
                    true,
                    cx,
                    |app, cx| {
                        // Same body as `App::delete_selection_action`, whose
                        // action type is not reachable from the views tree.
                        // `set_status_error` notifies like the action wrapper.
                        match app.delete_selection(cx) {
                            Ok(()) => app.set_status_error(None, cx),
                            Err(error) => app.set_status_error(Some(error), cx),
                        }
                    },
                ))
                .child(overflow_action_button(
                    "opt-sel-deselect",
                    t("menus.items.deselect"),
                    Some(icons::X),
                    has_selection,
                    true,
                    cx,
                    |app, cx| app.deselect_action(&DeselectAction, cx),
                )),
        );
    let body = gpui_kit::base::v_flex()
        .gap(px(ITEM_GAP))
        .child(overflow_caption(&t("tools.options.moreOptions")))
        .child(grid);
    more_dropdown_overflow(open, state, cx, body)
}

// ── Brush / Eraser ────────────────────────────────────────────────────────────

// ── Eyedropper ─────────────────────────────────────────────────────────────────

// ── Shape ──────────────────────────────────────────────────────────────────────

// ── Text ───────────────────────────────────────────────────────────────────────
