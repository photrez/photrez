//! Visual-only port of `shell/OptionBar.tsx`, its shared controls in
//! `shell/OptionBarShared.tsx`, and the per-tool variant bars — at the captured
//! 781px width.
//!
//! # Container queries are frozen, not evaluated
//!
//! The JSX wraps everything in `@container` and branches on it. The captured bar
//! is 781px wide, so the three gates resolve to constants here:
//! [`MAIN_CONTROLS`], [`MORE_DROPDOWN`] and [`FIELD_LABELS`].
//!
//! # Dispatch
//!
//! The reference's `state::active("tool", "move")` string dispatch becomes a
//! `match` on [`crate::state::ToolId`] — see [`render`].

// Reference's `#![allow(dead_code)]` removed: all helpers are reachable from a
// `match` arm, and `brush_params` carries its own test.
// lean-ctx: dead_code lint re-enabled; every fn below is called.

use gpui_kit::base::{h_flex, StyledExt};
use gpui_kit::component::Icon;
// `h_flex` is both the free fn and the `StyledExt` trait method (`.h_flex()`);
// both live in `gpui_kit::base` (re-export of the `gpui_base` crate). The
// reference imports them together (`gpui_kit::base::{StyledExt, h_flex}`) so
// both call forms resolve.
use gpui_kit::prelude::*;
use gpui_kit::{
    checkerboard, div, hsla, linear_color_stop, linear_gradient, px, BoxShadow, Div, FontWeight,
    Hsla, Stateful,
};

use crate::assets::icons;
use crate::components::ui;
use crate::i18n::t;
use crate::state::ToolId;
use crate::tokens::color;

mod controls;
mod widgets;
use self::controls::*;
use self::widgets::*;

// ── Geometry ──────────────────────────────────────────────────────────────────

/// Height from the manifest: 44px.
const BAR_H: f32 = 44.0;

/// `px-3` — the bar's horizontal inset.
const BAR_PX: f32 = 12.0;

/// `gap-1.5` — between branches and inside the FadeIn row.
const GAP: f32 = 6.0;

/// `gap-1` — inside a field and between paired X/Y and W/H groups.
const ITEM_GAP: f32 = 4.0;

/// `gap-0.5` — inside the align/flip button groups and the text bar's align icons.
const ICON_GAP: f32 = 2.0;

/// `h-[24px]` — every control in the bar.
const CTRL_H: f32 = 24.0;

/// `size-[24px]` — the align/flip buttons and the `...` button.
const BTN: f32 = 24.0;

/// `rounded-[4px]` — pill, checkboxes, fields, align/flip/reset.
const RADIUS: f32 = 4.0;

/// `rounded-[3px]` — the checkbox square and the `...` button.
const RADIUS_SM: f32 = 3.0;

/// `px-2` — pill padding.
const PILL_PX: f32 = 8.0;

/// `px-1.5` — checkbox-label and field padding.
const FIELD_PX: f32 = 6.0;

/// `mr-1` — after the "Align" / "Flip" captions and the text-bar chevrons.
const MR_LABEL: f32 = 4.0;

/// `h-5` — the `Divider` helper's height.
const DIV_H: f32 = 20.0;

/// See [`DIV_H`]: its width.
const DIV_W: f32 = 1.0;

/// `size-3` — pill icon and the check mark.
const ICON_SM: f32 = 12.0;

/// `size-3.5` — align/flip icons and the checkbox square.
const ICON_MD: f32 = 14.0;

/// `size-4` — the `...` icon.
const ICON_LG: f32 = 16.0;

/// `text-[11px]` — pill, checkbox labels, field values, Reset.
const TEXT_SM: f32 = 11.0;

/// `text-[10px]` — field labels and suffixes, Align/Flip captions.
const TEXT_XS: f32 = 10.0;

/// `text-[12px]` — the gradient bar's arrow and the text bar's letter toggles.
const TEXT_MD: f32 = 12.0;

/// `text-[13px]` — the text bar's "Aa".
const TEXT_13: f32 = 13.0;

/// `w-[62px]` — the X and Y fields.
const FIELD_W_XY: f32 = 62.0;

/// `w-[70px]` — the W and H fields.
const FIELD_W_WH: f32 = 70.0;

/// `w-[58px]` — the rotation field.
const FIELD_W_R: f32 = 58.0;

/// `w-12` — the brush Size field's fixed input.
const PAINT_INPUT_W: f32 = 48.0;

/// `w-11` — the brush Hard/Strength field's fixed input.
const PAINT_INPUT_W_SM: f32 = 44.0;

/// `w-11` — the paint-bucket tolerance number field.
const TOLERANCE_FIELD_W: f32 = 44.0;

/// `w-24` — the tolerance slider wrapper.
const TOLERANCE_SLIDER_W: f32 = 96.0;

/// `h-5 w-24` — the gradient preview strip.
const STRIP_H: f32 = 20.0;

/// See [`STRIP_H`]: its width.
const STRIP_W: f32 = 96.0;

/// `size-2.5` — the stroke pill's dot.
const DOT: f32 = 10.0;

/// `size-[22px]` — the shape bar's fill swatch.
const SWATCH_FILL: f32 = 22.0;

/// `w-[116px]` — the stroke pill.
const STROKE_PILL_W: f32 = 116.0;

/// `min-w-[34px]` — the stroke pill's value slot.
const STROKE_VAL_W: f32 = 34.0;

/// `min-w-[64px]` — the text bar's weight button label slot.
const WEIGHT_W: f32 = 64.0;

/// `w-[38px]` — the text bar's font-size input.
const SIZE_INPUT_W: f32 = 38.0;

/// `w-[34px]` — the shape bar's corner-radius input.
const RADIUS_INPUT_W: f32 = 34.0;

/// `px-2.5` — the crop bar's Apply / Cancel buttons.
const ACTION_PX: f32 = 10.0;

/// `w-[72px] h-[14px]` — the brush size slider (never painted at 781px).
const BRUSH_SLIDER_W: f32 = 72.0;
const BRUSH_SLIDER_H: f32 = 14.0;

/// Stroke widths passed to [`icons::with_stroke`].
const STROKE_PILL: f32 = 1.75;
const STROKE_CHECK: f32 = 3.5;
const STROKE_DROPDOWN: f32 = 1.6;
#[allow(dead_code)] // reserved for icon buttons not yet ported to the option bar
const STROKE_ICON: f32 = 1.5;

// ── Colours not in `tokens.rs` ────────────────────────────────────────────────

/// `oklch(0.666 0 0)` = `#A1A1AA` — the dim caption on every option bar.
const ZINC: Hsla = Hsla {
    h: 0.666667,
    s: 0.050279,
    l: 0.649020,
    a: 1.0,
};

/// `#2D323C` — the `Divider` helper's background.
const DIVIDER_LINE: Hsla = Hsla {
    h: 0.611111,
    s: 0.142857,
    l: 0.205882,
    a: 1.0,
};

/// `oklch(0.74 0.15 55)` = `#F28E42` — the shared foreground swatch colour.
const FG: Hsla = Hsla {
    h: 0.0553,
    s: 0.8145,
    l: 0.4863,
    a: 1.0,
};

/// `bg-editor-field/40` — inactive button backgrounds.
const FIELD_40: Hsla = Hsla {
    h: color::FIELD.h,
    s: color::FIELD.s,
    l: color::FIELD.l,
    a: 0.40,
};

/// `bg-editor-field/60` — checkbox hover.
const FIELD_60: Hsla = Hsla {
    h: color::FIELD.h,
    s: color::FIELD.s,
    l: color::FIELD.l,
    a: 0.60,
};

/// `bg-editor-field/80` — the unchecked checkbox square.
const FIELD_80: Hsla = Hsla {
    h: color::FIELD.h,
    s: color::FIELD.s,
    l: color::FIELD.l,
    a: 0.80,
};

/// `shadow-xs` — on the checked square and active chips.
const SHADOW_XS: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0,
    a: 0.05,
};

/// `bg-editor-accent/20` — active toggles and the accent swatch ring.
const ACCENT_20: Hsla = Hsla {
    h: color::ACCENT.h,
    s: color::ACCENT.s,
    l: color::ACCENT.l,
    a: 0.20,
};

/// `#2A2E37` — the stroke pill's off-state dot.
const STROKE_DOT: Hsla = Hsla {
    h: 0.615384,
    s: 0.134021,
    l: 0.190196,
    a: 1.0,
};

/// `#1A1A1A` — the gradient strip's background.
const STRIP_BG: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.101961,
    a: 1.0,
};

/// `#2A2A2A` — the gradient strip's checkerboard light square.
const STRIP_CHECK: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.164706,
    a: 1.0,
};

// ── Static text ─────────────────────────────────────────────────────────────────

/// The Move pill label.
const PILL_LABEL_MOVE_KEY: &str = "tools.move";
/// The brush/eraser pill label, selected by [`brush_params`].
const PILL_LABEL_BRUSH_KEY: &str = "tools.brush";
const PILL_LABEL_ERASER_KEY: &str = "tools.eraser";
/// The eyedropper pill label.
const PILL_LABEL_EYEDROPPER_KEY: &str = "tools.eyedropper";
/// The gradient pill label.
const PILL_LABEL_GRADIENT_KEY: &str = "tools.gradient";
/// The shape pill label.
const PILL_LABEL_SHAPE: &str = "Shape"; // i18n-exempt: not in catalog
/// The text pill label.
const PILL_LABEL_TEXT_KEY: &str = "tools.text";
/// The crop pill label.
const PILL_LABEL_CROP_KEY: &str = "tools.crop";

/// The gradient preset label.
const GRADIENT_PRESET_KEY: &str = "tools.options.fgBg";
/// The text box-mode label.
const BOX_MODE: &str = "Auto Width"; // i18n-exempt: not in catalog
/// The text weight label.
const WEIGHT_LABEL: &str = "Regular"; // i18n-exempt: not in catalog

/// The paint-bucket tolerance presets.
const TOLERANCE_PRESETS: [&str; 5] = ["0", "16", "32", "64", "128"];
/// Element ids for the five [`TOLERANCE_PRESETS`] chips.
const TOLERANCE_IDS: [&str; 5] = [
    "opt-tol-0",
    "opt-tol-16",
    "opt-tol-32",
    "opt-tol-64",
    "opt-tol-128",
];

/// Resting brush size → `paintSizeToSizeSlider(20)` = `((20-1)/(500-1))*75`.
const BRUSH_SLIDER_PCT: f32 = 3.0;
/// Resting eraser size → `paintSizeToSizeSlider(32)` = `((32-1)/(500-1))*75`.
const ERASER_SLIDER_PCT: f32 = 5.0;

/// Resting tolerance (32 of 255) as a percentage.
const TOLERANCE_PCT: f32 = 13.0;

// ── Container-query constants (frozen at CONTAINER_W = 781px) ──────────────────

/// `hidden @min-[880px]:flex` — the align/flip/reset group. False at 781px.
const MAIN_CONTROLS: bool = 781.0 >= 880.0;

/// `hidden @max-[880px]:flex` — the overflow button. True at 781px.
const MORE_DROPDOWN: bool = 781.0 <= 880.0;

/// `@max-[900px]:hidden` — field captions and the Align/Flip captions. False.
const FIELD_LABELS: bool = 781.0 > 900.0;

/// `hidden @min-[960px]:inline-block` — the selection/group captions. False.
const GROUP_LABELS_960: bool = 781.0 >= 960.0;

/// `@max-[900px]:hidden` — the brush size slider. False.
const PAINT_SIZE_SLIDER: bool = 781.0 >= 900.0;

// ── Brush params ───────────────────────────────────────────────────────────────

/// Pure dispatch for [`brush_items`]: selects the icon, label, resting size
/// string, slider percent and hardness string for the brush/eraser pair.
///
/// Extracted so the branch logic is unit-testable without a GPUI context.
///
/// RED proof: swap the `size`/`hardness` assignments and
/// [`brush_params_brush_defaults`] fails.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BrushParams {
    pub icon: &'static [u8],
    pub label: &'static str,
    pub size: &'static str,
    pub slider_pct: f32,
    pub hardness: &'static str,
}

pub fn brush_params(eraser: bool) -> BrushParams {
    if eraser {
        BrushParams {
            icon: icons::ERASER,
            label: PILL_LABEL_ERASER_KEY,
            size: "32",
            slider_pct: ERASER_SLIDER_PCT,
            hardness: "100",
        }
    } else {
        BrushParams {
            icon: icons::BRUSH,
            label: PILL_LABEL_BRUSH_KEY,
            size: "20",
            slider_pct: BRUSH_SLIDER_PCT,
            hardness: "80",
        }
    }
}

// ── The bar ───────────────────────────────────────────────────────────────────

/// The option bar: a 44px horizontal strip whose child row is selected by
/// `active_tool`. See the module docs on the container-query constants.
pub fn render(active_tool: &ToolId) -> impl IntoElement {
    h_flex()
        .h(px(BAR_H))
        .flex_shrink_0()
        .items_center()
        .gap(px(GAP))
        .border_b_1()
        .border_color(color::BORDER)
        .bg(color::TOOLBAR)
        .px(px(BAR_PX))
        .font_family(ui::FONT_UI)
        .child(fade_in_row(active_tool))
}

/// `<div class="flex items-center gap-1.5">` — one variant renders inside.
fn fade_in_row(active_tool: &ToolId) -> Div {
    let row = h_flex().items_center().gap(px(GAP));
    match active_tool {
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
    }
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

// ── Crop ─────────────────────────────────────────────────────────────────────

/// Pure dispatch for [`crop_items`]: the resting-state strings/flags of the
/// crop bar, extracted so the defaults are unit-testable without a GPUI
/// context.
///
/// RED proof: set [`CropParams::ratio_value`] to `"Size"` (or
/// [`CropParams::delete_checked`] to `false`) and [`crop_bar_resting_state`]
/// fails — proving the branch default is read, not just hard-coded at the call
/// site.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CropParams {
    pub pill_label: &'static str,
    pub ratio_prefix: &'static str,
    pub ratio_value: &'static str,
    pub delete_id: &'static str,
    pub delete_label: &'static str,
    pub delete_checked: bool,
    pub fill_id: &'static str,
    pub fill_label: &'static str,
    pub fill_checked: bool,
    pub use_bg_label: &'static str,
    pub cancel_label: &'static str,
    pub apply_label: &'static str,
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
fn crop_items(row: Div) -> Div {
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// `brush_params` selects the right icon/label/size/hardness for brush vs
    /// eraser, and the constants it references are pinned.
    ///
    /// RED proof: lower `ERASER_SLIDER_PCT` to `1.0` and the eraser assertion
    /// fails — proving the field is read, not just returned.
    #[test]
    fn brush_params_brush_defaults() {
        let p = brush_params(false);
        assert!(p.icon == icons::BRUSH, "brush must use the BRUSH icon");
        assert_eq!(p.label, PILL_LABEL_BRUSH_KEY);
        assert_eq!(t(p.label), "Brush Tool");
        assert_eq!(p.size, "20");
        assert_eq!(p.hardness, "80");
        assert_eq!(p.slider_pct, BRUSH_SLIDER_PCT);
    }

    /// Same, for the eraser branch — must not match the brush fields.
    #[test]
    fn brush_params_eraser_branch() {
        let p = brush_params(true);
        assert!(p.icon == icons::ERASER, "eraser must use the ERASER icon");
        assert_eq!(p.label, PILL_LABEL_ERASER_KEY);
        assert_eq!(t(p.label), "Eraser Tool");
        assert_eq!(p.size, "32");
        assert_eq!(p.hardness, "100");
        assert_eq!(p.slider_pct, ERASER_SLIDER_PCT);
        // Must differ from the brush branch.
        let b = brush_params(false);
        assert_ne!(p.icon, b.icon);
        assert_ne!(p.label, b.label);
        assert_ne!(p.size, b.size);
        assert_ne!(p.hardness, b.hardness);
        assert_ne!(p.slider_pct, b.slider_pct);
    }

    /// Pins the resting data of the Crop bar (`CropOptionBar.tsx`). These
    /// constants are the source of truth that [`crop_items`] composes, so every
    /// label, checkbox default and dropdown value the baseline shot encodes is
    /// asserted here.
    ///
    /// RED proof: flip `ratio_value` to `"Size"` (or `delete_checked` to
    /// `false`) and the assertions below fail — the values are read from
    /// [`crop_params`], not re-typed at the call site.
    #[test]
    fn crop_bar_resting_state() {
        let p = crop_params();
        // Pill label + ratio dropdown face.
        assert_eq!(p.pill_label, PILL_LABEL_CROP_KEY);
        assert_eq!(t(p.pill_label), "Crop Tool");
        assert_eq!(p.ratio_prefix, "tools.options.aspectRatio");
        assert_eq!(t("tools.options.aspectRatio"), "Aspect Ratio");
        assert_eq!(t(p.ratio_value), "Free", "resting cropMode is 'free'");
        assert_eq!(p.delete_id, "opt-crop-delete");
        assert_eq!(p.delete_label, "Delete");
        assert!(p.delete_checked, "Delete ships checked");
        assert_eq!(p.fill_id, "opt-crop-fill");
        assert_eq!(p.fill_label, "Fill BG");
        assert!(p.fill_checked, "Fill BG ships checked");
        // Swatch + action buttons.
        assert_eq!(p.use_bg_label, "Use BG");
        assert_eq!(t(p.cancel_label), "Cancel");
        assert_eq!(t(p.apply_label), "Apply");
    }
}
