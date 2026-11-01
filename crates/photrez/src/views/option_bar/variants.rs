use super::*;
use crate::app::tool_settings::ToolFlag;
use crate::state::ShapeTool;
use gpui_kit::TestSupportExt;

/// `<div class="flex items-center gap-1.5">` — one variant renders inside.
pub(super) fn fade_in_row(
    state: &gpui_kit::Entity<AppState>,
    active_tool: &ToolId,
    committed_selection: Option<photrez_core::ptz::content::SelectionState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let row = h_flex().items_center().gap(px(GAP));
    let row = match active_tool {
        ToolId::Move => move_items(state, row, cx),
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
/// State assumptions: `selectedLayerId()` is true (the fields show identity
/// transform on the 800x600 background), `hoveredLayer()` is false, and
/// `moveAutoSelect()` is true.
fn move_items(state: &gpui_kit::Entity<AppState>, row: Div, cx: &Context<crate::app::App>) -> Div {
    let (snap_focus, auto_select_focus, transform_focus) = {
        let s = state.read(cx);
        (
            s.move_snap_focus.clone(),
            s.move_auto_select_focus.clone(),
            s.move_transform_focus.clone(),
        )
    };
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
    .child(position_group())
    .child(size_group())
    .child(editable_field("num-r", "R", FIELD_W_R, "0", "\u{b0}")) // i18n-exempt: technical token (field label glyph)
    .when(show_main_controls(), |el| el.child(main_controls()))
    .when(show_more_dropdown(), |el| el.child(more_dropdown_button()))
}

// ── Selection ────────────────────────────────────────────────────────────────

/// `EditableNumField`'s `Math.round(val * 100) / 100` display: two decimals
/// at most, trailing zeros trimmed.
fn selection_field_value(value: f64) -> String {
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
    committed: Option<photrez_core::ptz::content::SelectionState>,
    cx: &Context<crate::app::App>,
) -> Div {
    let (shape, open, cursor) = {
        let s = state.read(cx);
        (s.selection_shape, s.open_select, s.select_cursor)
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
    let focus = state.read(cx).num_field_focus.clone();
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
            focus.clone(),
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
    .when(show_more_dropdown(), |el| el.child(more_dropdown_button()))
}
/// Resting crop bar: `cropMode="free"` → ratio reads "Free"; both `Delete` and
/// `Fill BG` ship checked; the fill colour block is visible (`fillEnabled()`).
pub fn crop_params() -> CropParams {
    CropParams {
        pill_label: PILL_LABEL_CROP_KEY,
        ratio_prefix: "tools.options.aspectRatio",
        ratio_value: "crop.free",
        delete_id: "opt-crop-delete",
        delete_label: "Delete", // i18n-exempt: not in catalog
        delete_checked: true,
        fill_id: "opt-crop-fill",
        fill_label: "Fill BG", // i18n-exempt: not in catalog
        fill_checked: true,
        use_bg_label: "Use BG", // i18n-exempt: not in catalog
        cancel_label: "common.cancel",
        apply_label: "common.apply",
    }
}

/// The `Crop` variant — the resting 781px capture.
///
/// `cropMode` rests on `"free"`, the ratio dropdown is closed, `fillEnabled()`
/// is true, and the `@min-[880px]` inline advanced group (straighten / rotate /
/// guide / classic toggle) is hidden — so none of those paint. Element order
/// follows the reference markup fragment line-for-line:
///
/// `pill` ⟵ `Divider` ⟵ `Aspect Ratio: Free` ⟵ `Divider` ⟵ `Delete` ⟵
/// `Fill BG` ⟵ `white swatch + Use BG` ⟵ `Divider` ⟵ `…` ⟵ `Divider` ⟵
/// `Cancel` ⟵ `Apply`.
///
/// The `<Divider/>` between `MoreDropdown` (…) and `Cancel` is present in the
/// reference markup and therefore in the baseline shot - not compressed out.
pub(super) fn crop_items(
    state: &gpui_kit::Entity<AppState>,
    row: Div,
    cx: &Context<crate::app::App>,
) -> Div {
    let (
        delete_checked,
        fill_checked,
        mode,
        aspect,
        recents,
        ratio_open,
        size_w,
        size_h,
        unit,
        unit_open,
        edit,
        edit_custom,
        field,
    ) = {
        let s = state.read(cx);
        (
            s.crop_delete_pixels,
            s.crop_fill_background,
            s.crop_mode,
            s.crop_aspect,
            s.recent_crop_ratios.clone(),
            s.crop_ratio_open,
            s.crop_size_w.clone(),
            s.crop_size_h.clone(),
            s.crop_size_unit,
            s.crop_unit_open,
            s.crop_edit.clone(),
            s.crop_edit_custom,
            s.crop_size_field,
        )
    };
    let custom_active = mode == crate::crop_aspect::CropMode::Ratio
        && aspect.is_some_and(|a| {
            !crate::crop_aspect::CROP_PRESETS
                .iter()
                .any(|(p, _)| *p == a)
        });
    let (ratio_bounds, unit_bounds) = {
        let s = state.read(cx);
        (s.crop_ratio_bounds.clone(), s.crop_unit_bounds.clone())
    };
    let p = crop_params();
    row.child(tool_pill(
        icons::with_stroke(icons::CROP, STROKE_PILL),
        t(p.pill_label),
    ))
    .child(divider())
    .child(crop_ratio_control(
        &t(p.ratio_prefix),
        mode,
        aspect,
        &recents,
        ratio_open,
        ratio_bounds,
        cx,
    ))
    .when(custom_active, |row| {
        row.child(divider()).child(crop_custom_fields(
            aspect,
            edit.as_deref(),
            edit_custom,
            field,
            cx,
        ))
    })
    .when(mode == crate::crop_aspect::CropMode::Size, |row| {
        row.child(divider()).child(crop_size_fields(
            &size_w,
            &size_h,
            unit,
            unit_open,
            edit.as_deref(),
            !edit_custom,
            field,
            unit_bounds,
            cx,
        ))
    })
    .child(divider())
    .child(option_checkbox_action(
        p.delete_id,
        p.delete_label,
        delete_checked,
        None,
        cx,
        |app, cx| app.toggle_crop_delete(cx),
    ))
    .child(option_checkbox_action(
        p.fill_id,
        p.fill_label,
        fill_checked,
        None,
        cx,
        |app, cx| app.toggle_crop_fill(cx),
    ))
    .child(crop_fill_swatch_group())
    .child(divider())
    .child(more_dropdown_button())
    .child(divider())
    .child(cancel_button(cx))
    .child(apply_button(cx))
}

// ── Brush / Eraser ────────────────────────────────────────────────────────────

/// The `Brush` variant, also serving `Eraser`.
///
/// `eraser` selects the pill label, size string, slider percent and hardness.
const BRUSH_PRESET_LABELS: [&str; 4] = ["Hard Round", "Soft Round", "Detail", "Large Soft"]; // i18n-exempt: not in catalog
const ERASER_PRESET_LABELS: [&str; 6] = [
    "Hard Round",
    "Soft Round",
    "Detail",
    "Large Soft",
    "Hard Eraser",
    "Soft Eraser",
]; // i18n-exempt: not in catalog

fn brush_items(
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
fn brush_more_dropdown(
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
            .bg(color::TOOLBAR)
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

// ── Eyedropper ─────────────────────────────────────────────────────────────────

/// The `Eyedropper` variant.
fn eyedropper_items(
    state: &gpui_kit::Entity<AppState>,
    row: Div,
    cx: &Context<crate::app::App>,
) -> Div {
    let auto_copy = state.read(cx).auto_copy_hex;
    row.child(tool_pill(
        icons::with_stroke(icons::PIPETTE, STROKE_PILL),
        t(PILL_LABEL_EYEDROPPER_KEY),
    ))
    .child(divider())
    .child(
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .child(fg_swatch_square())
            .child(tooltip(
                state,
                "opt-copy-hex",
                t("tools.options.clickToCopyHex"),
                None,
                cx,
                hex_badge(),
            )),
    )
    .child(divider())
    .child(option_checkbox_action(
        "opt-auto-copy",
        &t("tools.options.autoCopyHex"),
        auto_copy,
        None,
        cx,
        |app, cx| app.toggle_tool_flag(ToolFlag::AutoCopyHex, cx),
    ))
}

// ── Paint Bucket ─�─────────────────────────────────────────────────────────────

/// The `PaintBucket` variant.
fn paint_bucket_items(
    state: &gpui_kit::Entity<AppState>,
    row: Div,
    cx: &Context<crate::app::App>,
) -> Div {
    let (tolerance, contiguous, editing, focus) = {
        let s = state.read(cx);
        (
            s.fill_tolerance,
            s.fill_contiguous,
            s.num_field
                .as_ref()
                .filter(|field| field.id == "opt-tolerance")
                .map(|field| field.buffer.clone()),
            s.num_field_focus.clone(),
        )
    };
    row.child(
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(10.0))
            .px(px(8.0))
            .child(tool_pill(
                icons::with_stroke(icons::PAINT_BUCKET, STROKE_PILL),
                t("tools.paintBucket"),
            ))
            .child(divider())
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.0))
                    .child(dim_span(&format!("{}:", t("tools.options.tolerance"))))
                    .child(tolerance_slider(state, cx))
                    .child(tolerance_num_field(tolerance, editing, focus, cx)),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(ITEM_GAP))
                    .children(TOLERANCE_IDS.into_iter().zip(TOLERANCE_PRESETS).map(
                        |(id, value)| {
                            let active = value
                                .parse::<u32>()
                                .map(|preset| preset == tolerance)
                                .unwrap_or(false);
                            tolerance_preset(id, value, active, cx, move |app, cx| {
                                if let Ok(preset) = value.parse::<f64>() {
                                    let _ = app.set_fill_tolerance(preset, cx);
                                }
                            })
                        },
                    )),
            )
            .child(divider())
            .child(option_checkbox_action(
                "opt-contiguous",
                &t("tools.options.contiguous"),
                contiguous,
                None,
                cx,
                |app, cx| app.toggle_tool_flag(ToolFlag::FillContiguous, cx),
            )),
    )
}

// ── Gradient ───────────────────────────────────────────────────────────────────

/// The `Gradient` variant.
fn gradient_items(
    state: &gpui_kit::Entity<AppState>,
    row: Div,
    cx: &Context<crate::app::App>,
) -> Div {
    let (radial, fg_transparent, open, cursor) = {
        let s = state.read(cx);
        (
            s.gradient_radial,
            s.gradient_fg_transparent,
            s.open_select,
            s.select_cursor,
        )
    };
    let type_active = if open == Some("opt-gradient-type") {
        cursor
    } else if radial {
        1
    } else {
        0
    };
    let preset_active = if open == Some("opt-gradient-preset") {
        cursor
    } else if fg_transparent {
        1
    } else {
        0
    };
    row.child(
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(10.0))
            .px(px(8.0))
            .child(tool_pill(
                icons::with_stroke(icons::SWATCH, STROKE_PILL),
                t(PILL_LABEL_GRADIENT_KEY),
            ))
            .child(divider())
            .child(live_select(
                "opt-gradient-type",
                None,
                Some(format!("{}: ", t("tools.options.gradientType")).as_str()),
                if radial {
                    "Radial".to_string() // i18n-exempt: not in catalog
                } else {
                    t("tools.options.linear")
                },
                &["Linear", "Radial"], // i18n-exempt: not in catalog
                type_active,
                open == Some("opt-gradient-type"),
                state.read(cx).select_panel_bounds.clone(),
                cx,
            ))
            .child(divider())
            .child(live_select(
                "opt-gradient-preset",
                None,
                Some(format!("{}: ", t("tools.options.gradientPreset")).as_str()),
                if fg_transparent {
                    "FG to transparent".to_string() // i18n-exempt: not in catalog
                } else {
                    t(GRADIENT_PRESET_KEY)
                },
                &["FG to BG", "FG to transparent"], // i18n-exempt: not in catalog
                preset_active,
                open == Some("opt-gradient-preset"),
                state.read(cx).select_panel_bounds.clone(),
                cx,
            ))
            .child(divider())
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(GAP))
                    .child(round_swatch(FG))
                    .child(gradient_strip())
                    .child(round_swatch(color::WHITE)),
            )
            .child(reverse_button(cx)),
    )
}

// ── Shape ──────────────────────────────────────────────────────────────────────

const SHAPE_KIND_OPTIONS: [&str; 10] = [
    "Rect", "Ellipse", "Line", "Triangle", "Star", "Arrow", "Heart", "Diamond", "Bubble", "Hexagon",
]; // i18n-exempt: not in catalog

fn shape_label(kind: ShapeTool) -> &'static str {
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

fn shape_kind_index(kind: ShapeTool) -> usize {
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
fn shape_items(state: &gpui_kit::Entity<AppState>, row: Div, cx: &Context<crate::app::App>) -> Div {
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

// ── Text ───────────────────────────────────────────────────────────────────────

/// The `Text` variant, draw mode.
const TEXT_WEIGHT_LABELS: [&str; 7] = [
    "Thin", "Light", "Regular", "Medium", "SemiBold", "Bold", "Black",
]; // i18n-exempt: not in catalog

fn text_weight_index(weight: f64) -> usize {
    crate::app::tool_settings::TEXT_WEIGHTS
        .iter()
        .position(|candidate| (candidate - weight).abs() < 1.0)
        .unwrap_or(2)
}

fn text_items(state: &gpui_kit::Entity<AppState>, row: Div, cx: &Context<crate::app::App>) -> Div {
    let (
        text_layer,
        size,
        editing,
        focus,
        weight,
        italic,
        underline,
        strikethrough,
        uppercase,
        align,
        open,
        cursor,
        stroke_width,
        stroke_color,
        stroke_align,
        stroke_open,
        stroke_editing,
        stroke_panel_bounds,
        stroke_slider_bounds,
        stroke_capture,
        stroke_slider_focus,
    ) = {
        let s = state.read(cx);
        let text_layer = s.selected_layer_type.as_deref() == Some("text");
        (
            text_layer,
            if text_layer {
                s.selected_text_font_size
            } else {
                s.text_font_size
            },
            s.num_field
                .as_ref()
                .filter(|field| field.id == "opt-text-size")
                .map(|field| field.buffer.clone()),
            s.num_field_focus.clone(),
            if text_layer {
                s.selected_text_font_weight
            } else {
                s.text_font_weight
            },
            if text_layer {
                s.selected_text_italic
            } else {
                s.text_italic
            },
            if text_layer {
                s.selected_text_underline
            } else {
                s.text_underline
            },
            if text_layer {
                s.selected_text_strikethrough
            } else {
                s.text_strikethrough
            },
            if text_layer {
                s.selected_text_uppercase
            } else {
                s.text_uppercase
            },
            if text_layer {
                s.selected_text_align
            } else {
                s.text_align
            },
            s.open_select,
            s.select_cursor,
            if text_layer {
                s.selected_text_stroke_width
            } else {
                s.text_stroke_width
            },
            if text_layer {
                s.selected_text_stroke_color.unwrap_or(s.text_stroke_color)
            } else {
                s.text_stroke_color
            },
            if text_layer {
                s.selected_text_stroke_align
            } else {
                s.text_stroke_align
            },
            s.text_stroke_popover_open,
            s.num_field
                .as_ref()
                .filter(|field| field.id == "opt-text-stroke-width")
                .map(|field| field.buffer.clone()),
            s.text_stroke_panel_bounds.clone(),
            s.text_stroke_slider_bounds.clone(),
            s.slider_capture.clone(),
            s.slider_focus.clone(),
        )
    };
    row.child(tool_pill(
        icons::with_stroke(icons::TYPE, STROKE_PILL),
        t(PILL_LABEL_TEXT_KEY),
    ))
    .child(divider())
    .child(font_family_button())
    .child(divider())
    .child(font_size_field(size, editing, focus.clone(), cx))
    .child(tooltip(
        state,
        "opt-weight",
        t("tools.options.fontWeight"),
        None,
        cx,
        live_select(
            "opt-weight",
            None,
            None,
            TEXT_WEIGHT_LABELS[text_weight_index(weight)].to_string(),
            &TEXT_WEIGHT_LABELS,
            if open == Some("opt-weight") {
                cursor
            } else {
                text_weight_index(weight)
            },
            open == Some("opt-weight"),
            state.read(cx).select_panel_bounds.clone(),
            cx,
        ),
    ))
    .child(tooltip(
        state,
        "opt-italic",
        t("tools.options.italic"),
        None,
        cx,
        text_toggle_action(
            "opt-italic",
            div()
                .font_family("Georgia")
                .italic()
                .font_weight(FontWeight(700.))
                .text_size(px(TEXT_MD))
                .child("I"), // i18n-exempt: glyph or symbol
            italic,
            cx,
            |app, cx| {
                let _ = app.toggle_text_italic(cx);
            },
        ),
    ))
    .child(tooltip(
        state,
        "opt-underline",
        t("tools.options.underline"),
        None,
        cx,
        text_toggle_action(
            "opt-underline",
            div()
                .underline()
                .font_weight(FontWeight(700.))
                .text_size(px(TEXT_MD))
                .child("U"), // i18n-exempt: glyph or symbol
            underline,
            cx,
            |app, cx| {
                let _ = app.toggle_text_underline(cx);
            },
        ),
    ))
    .child(tooltip(
        state,
        "opt-strikethrough",
        t("tools.options.strikethrough"),
        None,
        cx,
        text_toggle_action(
            "opt-strikethrough",
            div()
                .line_through()
                .font_weight(FontWeight(700.))
                .text_size(px(TEXT_MD))
                .child("S"), // i18n-exempt: glyph or symbol
            strikethrough,
            cx,
            |app, cx| {
                let _ = app.toggle_text_strikethrough(cx);
            },
        ),
    ))
    .child(tooltip(
        state,
        "opt-uppercase",
        t("tools.options.uppercase"),
        None,
        cx,
        text_toggle_action(
            "opt-uppercase",
            div()
                .font_weight(FontWeight(700.))
                .text_size(px(TEXT_XS))
                .child("TT"), // i18n-exempt: glyph or symbol
            uppercase,
            cx,
            |app, cx| {
                let _ = app.toggle_text_uppercase(cx);
            },
        ),
    ))
    .child(divider())
    .child(
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(ICON_GAP))
            .child(tooltip(
                state,
                "opt-align-left",
                t("properties.alignLeft"),
                None,
                cx,
                text_icon_action(
                    "opt-align-left",
                    icons::ALIGN_TEXT_LEFT,
                    align == crate::state::TextAlignTool::Left,
                    cx,
                    |app, cx| {
                        let _ = app.set_text_align(crate::state::TextAlignTool::Left, cx);
                    },
                ),
            ))
            .child(tooltip(
                state,
                "opt-align-center",
                t("properties.alignCenter"),
                None,
                cx,
                text_icon_action(
                    "opt-align-center",
                    icons::ALIGN_TEXT_CENTER,
                    align == crate::state::TextAlignTool::Center,
                    cx,
                    |app, cx| {
                        let _ = app.set_text_align(crate::state::TextAlignTool::Center, cx);
                    },
                ),
            ))
            .child(tooltip(
                state,
                "opt-align-right",
                t("properties.alignRight"),
                None,
                cx,
                text_icon_action(
                    "opt-align-right",
                    icons::ALIGN_TEXT_RIGHT,
                    align == crate::state::TextAlignTool::Right,
                    cx,
                    |app, cx| {
                        let _ = app.set_text_align(crate::state::TextAlignTool::Right, cx);
                    },
                ),
            )),
    )
    .child(divider())
    .child(box_mode_button())
    .child(tooltip(
        state,
        "opt-text-color",
        t("tools.options.textColor"),
        None,
        cx,
        text_color_swatch(state, cx),
    ))
    .child(divider())
    .child(tooltip(
        state,
        "opt-text-stroke",
        t("tools.options.strokeOptions"),
        None,
        cx,
        text_stroke_control(
            stroke_width,
            stroke_color,
            stroke_align,
            stroke_open,
            stroke_editing,
            focus,
            stroke_slider_focus,
            stroke_panel_bounds,
            stroke_slider_bounds,
            stroke_capture,
            cx,
        ),
    ))
    .when(text_layer, |el| {
        el.child(divider()).child(text_toggle_action(
            "opt-text-rasterize",
            div()
                .font_weight(FontWeight(600.))
                .text_size(px(TEXT_XS))
                .child("Raster"), // i18n-exempt: not in catalog
            false,
            cx,
            |app, cx| {
                let _ = app.rasterize_selected_layer(cx);
            },
        ))
    })
}

/// The `Aspect Ratio` button plus its inline list: lock the current
/// shape, the three recents, Free / Custom / Size, then the preset list.
fn crop_ratio_control(
    prefix: &str,
    mode: crate::crop_aspect::CropMode,
    aspect: Option<(u32, u32)>,
    recents: &[(u32, u32)],
    open: bool,
    _bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    use crate::crop_aspect::ratio_label;

    let trigger = h_flex()
        .id("crop-ratio")
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .font_weight(FontWeight(500.))
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
            cx.stop_propagation();
            app.toggle_crop_ratio(cx);
        }))
        .child(
            h_flex()
                .child(div().text_color(ZINC).child(format!("{prefix}: ")))
                .child(
                    div()
                        .font_weight(FontWeight(600.))
                        .text_color(color::WHITE)
                        .child(ratio_label(mode, aspect)),
                )
                .child(
                    Icon::default()
                        .data(icons::with_stroke(icons::CHEVRON_DOWN, 1.75))
                        .size(px(ICON_SM))
                        .text_color(ZINC),
                ),
        );

    let panel = crop_ratio_list(mode, aspect, recents, cx);

    div()
        .relative()
        .flex_shrink_0()
        .child(trigger)
        .when(open, |el| {
            el.child(gpui_kit::deferred(
                div()
                    .absolute()
                    .top(px(CTRL_H + 6.0))
                    .left(px(0.0))
                    .child(panel),
            ))
        })
}

pub(super) fn crop_ratio_list(
    mode: crate::crop_aspect::CropMode,
    aspect: Option<(u32, u32)>,
    recents: &[(u32, u32)],
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    use crate::crop_aspect::{CropMode, CROP_PRESETS};
    let custom_active = mode == CropMode::Ratio
        && aspect.is_some_and(|a| !CROP_PRESETS.iter().any(|(p, _)| *p == a));
    let mut panel = gpui_kit::base::v_flex()
        .id("crop-ratio-list")
        .test_support()
        .w(px(160.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::PANEL)
        .py(px(4.0))
        .max_h(px(300.0))
        .overflow_y_scroll()
        .child(crop_ratio_action_row(
            "crop-ratio-lock",
            Some(icons::LOCK),
            "Lock Current Shape",
            false,
            mode == CropMode::Free,
            cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_lock_shape(cx);
            }),
        ))
        .child(crop_ratio_divider());
    if !recents.is_empty() {
        panel = panel.child(crop_ratio_caption(&t("common.recents")));
        for (index, recent) in recents.iter().enumerate() {
            let value = *recent;
            panel = panel.child(crop_ratio_action_row(
                &format!("crop-ratio-recent-{index}"),
                None,
                &format!("{}:{}", value.0, value.1),
                false,
                true,
                cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    app.crop_choose_ratio(value, cx);
                }),
            ));
        }
        panel = panel.child(crop_ratio_divider());
    }
    panel = panel
        .child(crop_ratio_action_row(
            "crop-ratio-free",
            None,
            "Free",
            mode == CropMode::Free,
            true,
            cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_choose_free(cx);
            }),
        ))
        .child(crop_ratio_action_row(
            "crop-ratio-custom",
            None,
            "Custom...",
            custom_active,
            true,
            cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_choose_custom(cx);
            }),
        ))
        .child(crop_ratio_action_row(
            "crop-ratio-size",
            None,
            "Size",
            mode == CropMode::Size,
            true,
            cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_choose_size(cx);
            }),
        ))
        .child(crop_ratio_divider())
        .child(crop_ratio_caption("Presets")); // i18n-exempt: fixed section caption, not in catalog
    for (index, (preset, label)) in CROP_PRESETS.iter().enumerate() {
        let value = *preset;
        panel = panel.child(crop_ratio_preset_row(
            &format!("crop-ratio-preset-{index}"),
            label,
            mode == CropMode::Ratio && aspect == Some(value),
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_choose_ratio(value, cx);
            }),
        ));
    }
    panel
}

fn crop_ratio_divider() -> impl IntoElement {
    div().w_full().h(px(1.0)).my(px(4.0)).bg(color::DIVIDER)
}

fn crop_ratio_caption(label: &str) -> impl IntoElement {
    div()
        .px(px(12.0))
        .py(px(2.0))
        .text_size(px(9.0))
        .font_weight(FontWeight(700.))
        .text_color(color::TEXT_DIM)
        .child(label.to_uppercase())
}

#[allow(clippy::too_many_arguments)] // id, icon, label, tone, gate and listener are one row record
fn crop_ratio_action_row(
    id: &str,
    icon: Option<&'static [u8]>,
    label: &str,
    active: bool,
    enabled: bool,
    listener: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
) -> impl IntoElement {
    let mut row = h_flex()
        .id(id.to_string())
        .test_support()
        .w_full()
        .items_center()
        .gap(px(GAP))
        .px(px(12.0))
        .py(px(6.0))
        .text_size(px(TEXT_SM))
        .text_color(if active { color::WHITE } else { ZINC })
        .font_weight(if active {
            FontWeight(600.)
        } else {
            FontWeight(400.)
        })
        .when(active, |el| {
            el.border_l(px(2.0))
                .border_color(color::ACCENT)
                .bg(color::ACCENT.opacity(0.2))
        });
    if let Some(icon) = icon {
        row = row.child(
            Icon::default()
                .data(icon)
                .size(px(ICON_SM))
                .text_color(if active { color::WHITE } else { ZINC }),
        );
    }
    row = row.child(div().child(label.to_string()));
    if enabled {
        row.cursor_pointer()
            .hover(|style| style.bg(color::WHITE_10))
            .on_click(listener)
    } else {
        row.opacity(0.4)
    }
}

fn crop_ratio_preset_row(
    id: &str,
    label: &str,
    active: bool,
    listener: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
) -> impl IntoElement {
    h_flex()
        .id(id.to_string())
        .test_support()
        .w_full()
        .items_center()
        .px(px(12.0))
        .py(px(6.0))
        .text_size(px(TEXT_SM))
        .text_color(if active { color::ACCENT } else { color::TEXT })
        .font_weight(if active {
            FontWeight(500.)
        } else {
            FontWeight(400.)
        })
        .cursor_pointer()
        .hover(|style| style.bg(color::FIELD.opacity(0.6)))
        .on_click(listener)
        .child(label.to_string())
}

/// `CropCustomInputs`: two integer W/H fields (the custom ratio pair).
fn crop_custom_fields(
    aspect: Option<(u32, u32)>,
    edit: Option<&str>,
    editing_custom: bool,
    field: usize,
    cx: &Context<crate::app::App>,
) -> Div {
    let (w, h) = aspect.unwrap_or((1, 1));
    let w_text = if editing_custom && edit.is_some() && field == 0 {
        edit.unwrap_or("").to_string()
    } else {
        w.to_string()
    };
    let h_text = if editing_custom && edit.is_some() && field == 1 {
        edit.unwrap_or("").to_string()
    } else {
        h.to_string()
    };
    h_flex()
        .flex_shrink_0()
        .gap(px(GAP))
        .child(crop_edit_field(
            "crop-custom-w",
            "W",
            &w_text,
            "",
            cx,
            0,
            true,
        ))
        .child(crop_edit_field(
            "crop-custom-h",
            "H",
            &h_text,
            "",
            cx,
            1,
            true,
        ))
}

/// The Size fields: W/H with the unit as suffix, a swap button and the unit
/// list (expanded inline, like the language list in Settings).
// Separate field and geometry inputs mirror the two rendered size fields.
#[allow(clippy::too_many_arguments)]
fn crop_size_fields(
    w: &str,
    h: &str,
    unit: crate::crop_aspect::CropSizeUnit,
    unit_open: bool,
    edit: Option<&str>,
    editing_size: bool,
    field: usize,
    _bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let w_text = if editing_size && edit.is_some() && field == 0 {
        edit.unwrap_or("").to_string()
    } else {
        w.to_string()
    };
    let h_text = if editing_size && edit.is_some() && field == 1 {
        edit.unwrap_or("").to_string()
    } else {
        h.to_string()
    };
    let suffix = unit.label();
    div()
        .relative()
        .flex_shrink_0()
        .child(
            h_flex()
                .gap(px(GAP))
                .child(crop_edit_field(
                    "crop-size-w",
                    "W",
                    &w_text,
                    suffix,
                    cx,
                    0,
                    false,
                ))
                .child(
                    div()
                        .id("crop-size-swap")
                        .test_support()
                        .h(px(CTRL_H))
                        .w(px(CTRL_H))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(RADIUS))
                        .cursor_pointer()
                        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            app.crop_swap_size(cx);
                        }))
                        .child(
                            Icon::default()
                                .data(icons::SWAP)
                                .size(px(12.0))
                                .text_color(color::ICON),
                        ),
                )
                .child(crop_edit_field(
                    "crop-size-h",
                    "H",
                    &h_text,
                    suffix,
                    cx,
                    1,
                    false,
                ))
                .child(
                    h_flex()
                        .id("crop-size-unit")
                        .test_support()
                        .h(px(CTRL_H))
                        .flex_shrink_0()
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
                        .cursor_pointer()
                        .on_click(cx.listener(|app, _: &gpui_kit::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            app.toggle_crop_unit(cx);
                        }))
                        .child(unit.label().to_string()),
                ),
        )
        .when(unit_open, |el| {
            el.child(gpui_kit::deferred(
                div()
                    .absolute()
                    .top(px(CTRL_H + 32.0))
                    .left(px(0.0))
                    .child(crop_unit_list(unit, cx)),
            ))
        })
}

fn crop_edit_field(
    id: &str,
    label: &str,
    value: &str,
    suffix: &str,
    cx: &Context<crate::app::App>,
    field: usize,
    custom: bool,
) -> impl IntoElement {
    let mut el = h_flex()
        .id(id.to_string())
        .test_support()
        .h(px(CTRL_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::FIELD)
        .px(px(PILL_PX))
        .text_size(px(TEXT_SM))
        .cursor_pointer()
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_begin_edit(field, custom, cx);
            }),
        )
        .child(div().text_color(ZINC).child(label.to_string()))
        .child(
            div()
                .font_weight(FontWeight(600.))
                .text_color(color::WHITE)
                .child(value.to_string()),
        );
    if !suffix.is_empty() {
        el = el.child(div().text_color(ZINC).child(suffix.to_string()));
    }
    el
}

pub(super) fn crop_unit_list(
    unit: crate::crop_aspect::CropSizeUnit,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let mut list = gpui_kit::base::v_flex()
        .id("crop-size-unit-list")
        .test_support()
        .w(px(64.0))
        .rounded(px(RADIUS))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::PANEL)
        .py(px(4.0));
    for candidate in crate::crop_aspect::CropSizeUnit::ALL {
        list = list.child(crop_ratio_preset_row(
            &format!("crop-size-unit-{}", candidate.label()),
            candidate.label(),
            candidate == unit,
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                cx.stop_propagation();
                app.crop_set_size_unit(candidate, cx);
            }),
        ));
    }
    list
}
