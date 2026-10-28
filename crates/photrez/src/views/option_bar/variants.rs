use super::*;
use gpui_kit::TestSupportExt;

/// `<div class="flex items-center gap-1.5">` — one variant renders inside.
pub(super) fn fade_in_row(active_tool: &ToolId) -> impl IntoElement {
    let row = h_flex().items_center().gap(px(GAP));
    let row = match active_tool {
        ToolId::Move => move_items(row),
        ToolId::Selection => selection_items(row),
        ToolId::Crop => crop_items(row),
        ToolId::Brush => brush_items(row, false),
        ToolId::Eraser => brush_items(row, true),
        ToolId::Eyedropper => eyedropper_items(row),
        ToolId::PaintBucket => paint_bucket_items(row),
        ToolId::Gradient => gradient_items(row),
        ToolId::Shape => shape_items(row),
        ToolId::Text => text_items(row),
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

/// The `Move` variant (`MoveOptionBar.tsx:214-451`).
///
/// State assumptions: `selectedLayerId()` is true (the fields show identity
/// transform on the 800x600 background), `hoveredLayer()` is false, and
/// `moveAutoSelect()` is true.
fn move_items(row: Div) -> Div {
    row.child(tool_pill(
        icons::with_stroke(icons::CURSOR, STROKE_PILL),
        t(PILL_LABEL_MOVE_KEY),
    ))
    .child(divider())
    .child(option_checkbox(
        "opt-auto-select",
        &t("tools.options.autoSelect"),
    ))
    .child(option_checkbox("opt-snap", &t("tools.options.snap")))
    .child(divider())
    .child(option_checkbox("opt-transform", &t("properties.transform")))
    .child(divider())
    .child(position_group())
    .child(size_group())
    .child(editable_field("num-r", "R", FIELD_W_R, "0", "\u{b0}")) // i18n-exempt: technical token (field label glyph)
    .when(MAIN_CONTROLS, |el| el.child(main_controls()))
    .when(MORE_DROPDOWN, |el| el.child(more_dropdown_button()))
}

// ── Selection ────────────────────────────────────────────────────────────────

/// The `Selection` variant (`SelectionOptionBar.tsx`).
///
/// State assumptions: `selectionShape()` is "Rectangle Select", no marquee
/// exists, so every numeric field is dimmed to "0".
fn selection_items(row: Div) -> Div {
    row.child(tool_pill(
        icons::with_stroke(icons::SQUARE_DASHED, STROKE_PILL),
        t("tools.rectSelect"),
    ))
    .child(divider())
    .child(select_dropdown(
        Some(icons::SQUARE_DASHED),
        None,
        t("tools.rectSelect"),
    ))
    .child(divider())
    .child(select_dropdown(
        None,
        Some(format!("{}: ", t("tools.options.style")).as_str()),
        t("tools.options.normal"),
    ))
    .child(divider())
    .when(GROUP_LABELS_960, |el| {
        el.child(caps_label(&t("properties.position")))
    })
    .child(field_pair(
        dimmed(editable_field("sel-x", "X", FIELD_W_XY, "0", "px")), // i18n-exempt: technical token (field label glyph)
        dimmed(editable_field("sel-y", "Y", FIELD_W_XY, "0", "px")), // i18n-exempt: technical token (field label glyph)
    ))
    .child(divider())
    .when(GROUP_LABELS_960, |el| {
        el.child(caps_label(&t("tools.options.size")))
    })
    .child(field_pair(
        dimmed(editable_field("sel-w", "W", FIELD_W_XY, "0", "px")), // i18n-exempt: technical token (field label glyph)
        dimmed(editable_field("sel-h", "H", FIELD_W_XY, "0", "px")), // i18n-exempt: technical token (field label glyph)
    ))
    .child(divider())
    .when(GROUP_LABELS_960, |el| {
        el.child(caps_label(&t("properties.rotation")))
    })
    .child(dimmed(editable_field(
        "sel-r", "R", FIELD_W_R, "0", "\u{b0}",
    )))
    .when(MORE_DROPDOWN, |el| el.child(more_dropdown_button()))
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

/// The `Crop` variant — the resting 781px capture of `CropOptionBar.tsx:432-668`.
///
/// `cropMode` rests on `"free"`, the ratio dropdown is closed, `fillEnabled()`
/// is true, and the `@min-[880px]` inline advanced group (straighten / rotate /
/// guide / classic toggle) is hidden — so none of those paint. Element order
/// follows the TSX fragment line-for-line:
///
/// `pill` ⟵ `Divider` ⟵ `Aspect Ratio: Free` ⟵ `Divider` ⟵ `Delete` ⟵
/// `Fill BG` ⟵ `white swatch + Use BG` ⟵ `Divider` ⟵ `…` ⟵ `Divider` ⟵
/// `Cancel` ⟵ `Apply`.
///
/// The `<Divider/>` between `MoreDropdown` (…) and `Cancel` is present in the
/// TSX (line 640) and therefore in the baseline shot - it is not compressed out.
pub(super) fn crop_items(row: Div) -> Div {
    let p = crop_params();
    row.child(tool_pill(
        icons::with_stroke(icons::CROP, STROKE_PILL),
        t(p.pill_label),
    ))
    .child(divider())
    .child(select_dropdown(
        None,
        Some(format!("{}: ", t(p.ratio_prefix)).as_str()),
        t(p.ratio_value),
    ))
    .child(divider())
    .child(option_checkbox_state(
        p.delete_id,
        p.delete_label,
        p.delete_checked,
    ))
    .child(option_checkbox_state(
        p.fill_id,
        p.fill_label,
        p.fill_checked,
    ))
    .child(crop_fill_swatch_group())
    .child(divider())
    .child(more_dropdown_button())
    .child(divider())
    .child(cancel_button())
    .child(apply_button())
}

// ── Brush / Eraser ────────────────────────────────────────────────────────────

/// The `Brush` variant (`BrushOptionBar.tsx`), also serving `Eraser`.
///
/// `eraser` selects the pill label, size string, slider percent and hardness.
fn brush_items(row: Div, eraser: bool) -> Div {
    let p = brush_params(eraser);
    row.child(tool_pill(
        icons::with_stroke(p.icon, STROKE_PILL),
        t(p.label),
    ))
    .child(divider())
    .child(paint_field(
        "paint-size",
        &t("tools.options.size"),
        PAINT_INPUT_W,
        p.size,
        "px",
    ))
    .when(PAINT_SIZE_SLIDER, |el| {
        el.child(brush_size_slider(p.slider_pct))
    })
    .child(divider())
    .child(paint_field(
        "paint-hardness",
        &t("tools.options.hard"),
        PAINT_INPUT_W_SM,
        p.hardness,
        "%",
    ))
    .child(paint_field(
        "paint-opacity",
        &t("tools.options.strength"),
        PAINT_INPUT_W_SM,
        "100",
        "%",
    ))
}

// ── Eyedropper ─────────────────────────────────────────────────────────────────

/// The `Eyedropper` variant (`EyedropperOptionBar.tsx`).
fn eyedropper_items(row: Div) -> Div {
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
            .child(hex_badge()),
    )
    .child(divider())
    .child(option_checkbox_state(
        "opt-auto-copy",
        &t("tools.options.autoCopyHex"),
        false,
    ))
}

// ── Paint Bucket ─�─────────────────────────────────────────────────────────────

/// The `PaintBucket` variant (`PaintBucketOptionBar.tsx`).
fn paint_bucket_items(row: Div) -> Div {
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
                    .child(tolerance_slider())
                    .child(right_num_field("opt-tolerance", "32")),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(ITEM_GAP))
                    .children(
                        TOLERANCE_IDS
                            .into_iter()
                            .zip(TOLERANCE_PRESETS)
                            .map(|(id, value)| tolerance_preset(id, value, value == "32")),
                    ),
            )
            .child(divider())
            .child(option_checkbox_state(
                "opt-contiguous",
                &t("tools.options.contiguous"),
                true,
            )),
    )
}

// ── Gradient ───────────────────────────────────────────────────────────────────

/// The `Gradient` variant (`GradientOptionBar.tsx`).
fn gradient_items(row: Div) -> Div {
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
            .child(select_dropdown(
                None,
                Some(format!("{}: ", t("tools.options.gradientType")).as_str()),
                t("tools.options.linear"),
            ))
            .child(divider())
            .child(select_dropdown(
                None,
                Some(format!("{}: ", t("tools.options.gradientPreset")).as_str()),
                t(GRADIENT_PRESET_KEY),
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
            .child(reverse_button()),
    )
}

// ── Shape ──────────────────────────────────────────────────────────────────────

/// The `Shape` variant (`ShapeOptionBar.tsx`), draw mode.
fn shape_items(row: Div) -> Div {
    row.child(tool_pill(
        icons::with_stroke(icons::RECTANGLE, STROKE_PILL),
        PILL_LABEL_SHAPE.to_string(),
    ))
    .child(divider())
    .child(select_dropdown(
        Some(icons::RECTANGLE),
        None,
        t("tools.options.shapes.rect"),
    ))
    .child(divider())
    .child(
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(ITEM_GAP))
            .child(fill_toggle())
            .child(fill_swatch()),
    )
    .child(divider())
    .child(stroke_pill_off("opt-shape-stroke"))
    .child(divider())
    .child(radius_field())
}

// ── Text ───────────────────────────────────────────────────────────────────────

/// The `Text` variant (`TextOptionBar.tsx`), draw mode.
fn text_items(row: Div) -> Div {
    row.child(tool_pill(
        icons::with_stroke(icons::TYPE, STROKE_PILL),
        t(PILL_LABEL_TEXT_KEY),
    ))
    .child(divider())
    .child(font_family_button())
    .child(divider())
    .child(font_size_field())
    .child(weight_button())
    .child(text_toggle_button(
        "opt-italic",
        div()
            .font_family("Georgia")
            .italic()
            .font_weight(FontWeight(700.))
            .text_size(px(TEXT_MD))
            .child("I"), // i18n-exempt: glyph or symbol
    ))
    .child(text_toggle_button(
        "opt-underline",
        div()
            .underline()
            .font_weight(FontWeight(700.))
            .text_size(px(TEXT_MD))
            .child("U"), // i18n-exempt: glyph or symbol
    ))
    .child(text_toggle_button(
        "opt-strikethrough",
        div()
            .line_through()
            .font_weight(FontWeight(700.))
            .text_size(px(TEXT_MD))
            .child("S"), // i18n-exempt: glyph or symbol
    ))
    .child(text_toggle_button(
        "opt-uppercase",
        div()
            .font_weight(FontWeight(700.))
            .text_size(px(TEXT_XS))
            .child("TT"), // i18n-exempt: glyph or symbol
    ))
    .child(divider())
    .child(
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(ICON_GAP))
            .child(text_icon_button(
                "opt-align-left",
                icons::ALIGN_TEXT_LEFT,
                true,
            ))
            .child(text_icon_button(
                "opt-align-center",
                icons::ALIGN_TEXT_CENTER,
                false,
            ))
            .child(text_icon_button(
                "opt-align-right",
                icons::ALIGN_TEXT_RIGHT,
                false,
            )),
    )
    .child(divider())
    .child(box_mode_button())
    .child(text_color_swatch())
    .child(divider())
    .child(stroke_pill_off("opt-text-stroke"))
}
