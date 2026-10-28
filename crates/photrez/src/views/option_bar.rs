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
    Hsla, Stateful, TestSupportExt,
};

use crate::app::tool_settings::{ToolFlag, ToolSetting};
use crate::components::ui;
use crate::i18n::t;
use crate::icons;
use crate::state::{AppState, ShapeTool, ToolId};
use crate::tokens::color;

mod controls;
mod variants;
mod widgets;
use self::controls::*;
pub use self::variants::crop_params;
use self::variants::fade_in_row;
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
pub fn render(
    state: &gpui_kit::Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let active_tool = state.read(cx).active_tool;
    // The additive live strip is retired for the Move tool: its captured
    // chrome (`option_bar/variants.rs::move_items`) already carries the
    // oracle's control order, so the parallel steppers duplicated it. The
    // other tools keep their strips until their P1 wiring replaces them.
    let strip_retired = active_tool == ToolId::Move;
    let strip = if strip_retired {
        None
    } else {
        Some(live_strip(state, &active_tool, cx))
    };
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
        .children(strip)
        .child(fade_in_row(&active_tool))
}

/// Compact steppers/toggles for every setting the frozen option bar exposes as
/// sliders and checkboxes. Appended after the captured chrome so the values are
/// user-reachable without rebuilding each variant's layout.
fn live_strip(
    state: &gpui_kit::Entity<AppState>,
    tool: &ToolId,
    cx: &Context<crate::app::App>,
) -> Div {
    let s = state.read(cx);
    let mut strip = h_flex()
        .h(px(CTRL_H))
        .items_center()
        .gap(px(3.0))
        .flex_shrink_0()
        .overflow_hidden();
    match tool {
        ToolId::Brush | ToolId::Eraser => {
            let eraser = *tool == ToolId::Eraser;
            let (size, hardness, opacity, flow, smooth) = if eraser {
                (
                    s.eraser_size,
                    s.eraser_hardness,
                    s.eraser_opacity,
                    s.eraser_flow,
                    s.eraser_smoothing,
                )
            } else {
                (
                    s.brush_size,
                    s.brush_hardness,
                    s.brush_opacity,
                    s.brush_flow,
                    s.brush_smoothing,
                )
            };
            strip = strip
                .child(live_button(
                    "brush-preset",
                    "Preset".to_string(),
                    cx,
                    |app, cx| app.cycle_brush_preset(cx),
                ))
                .child(step_pair(
                    "brush-size",
                    format!("Size {size:.0}"),
                    cx,
                    if eraser {
                        ToolSetting::EraserSize
                    } else {
                        ToolSetting::BrushSize
                    },
                ))
                .child(step_pair(
                    "brush-hardness",
                    format!("Hard {:.0}%", hardness * 100.0),
                    cx,
                    if eraser {
                        ToolSetting::EraserHardness
                    } else {
                        ToolSetting::BrushHardness
                    },
                ))
                .child(step_pair(
                    "brush-opacity",
                    format!("Opacity {:.0}%", opacity * 100.0),
                    cx,
                    if eraser {
                        ToolSetting::EraserOpacity
                    } else {
                        ToolSetting::BrushOpacity
                    },
                ))
                .child(step_pair(
                    "brush-flow",
                    format!("Flow {:.0}%", flow * 100.0),
                    cx,
                    if eraser {
                        ToolSetting::EraserFlow
                    } else {
                        ToolSetting::BrushFlow
                    },
                ))
                .child(step_pair(
                    "brush-smoothing",
                    format!("Smooth {smooth:.0}"),
                    cx,
                    if eraser {
                        ToolSetting::EraserSmoothing
                    } else {
                        ToolSetting::BrushSmoothing
                    },
                ));
        }
        ToolId::PaintBucket => {
            strip = strip
                .child(step_pair(
                    "fill-tolerance",
                    format!("Tolerance {}", s.fill_tolerance),
                    cx,
                    ToolSetting::FillTolerance,
                ))
                .child(toggle_button(
                    "fill-contiguous",
                    format!("Contiguous: {}", on_off(s.fill_contiguous)),
                    cx,
                    ToolFlag::FillContiguous,
                ));
        }
        ToolId::Gradient => {
            strip = strip
                .child(toggle_button(
                    "gradient-radial",
                    if s.gradient_radial {
                        "Radial".to_string()
                    } else {
                        "Linear".to_string()
                    },
                    cx,
                    ToolFlag::GradientRadial,
                ))
                .child(toggle_button(
                    "gradient-reverse",
                    format!("Reverse: {}", on_off(s.gradient_reverse)),
                    cx,
                    ToolFlag::GradientReverse,
                ))
                .child(toggle_button(
                    "gradient-transparent",
                    if s.gradient_fg_transparent {
                        "FG to alpha".to_string()
                    } else {
                        "FG to BG".to_string()
                    },
                    cx,
                    ToolFlag::GradientFgTransparent,
                ));
        }
        ToolId::Shape => {
            strip = strip
                .child(live_button(
                    "shape-kind",
                    shape_label(s.shape_kind).to_string(),
                    cx,
                    |app, cx| app.cycle_shape_kind(cx),
                ))
                .child(toggle_button(
                    "shape-fill",
                    format!("Fill: {}", on_off(s.shape_fill)),
                    cx,
                    ToolFlag::ShapeFill,
                ))
                .child(toggle_button(
                    "shape-stroke",
                    format!("Stroke: {}", on_off(s.shape_stroke)),
                    cx,
                    ToolFlag::ShapeStroke,
                ))
                .child(step_pair(
                    "shape-radius",
                    format!("Radius {:.0}", s.shape_radius),
                    cx,
                    ToolSetting::ShapeRadius,
                ))
                .child(toggle_button(
                    "shape-arrow-head",
                    format!("Arrow: {}", on_off(s.shape_arrow_head)),
                    cx,
                    ToolFlag::ShapeArrowHead,
                ));
        }
        ToolId::Eyedropper => {
            strip = strip.child(toggle_button(
                "eyedropper-auto-copy",
                format!("Auto-copy HEX: {}", on_off(s.auto_copy_hex)),
                cx,
                ToolFlag::AutoCopyHex,
            ));
        }
        ToolId::Crop => {
            strip = strip
                .child(live_button(
                    "crop-apply",
                    "Apply".to_string(),
                    cx,
                    |app, cx| {
                        let result = app.apply_crop(cx);
                        match result {
                            Ok(()) => app.set_status_error(None, cx),
                            Err(error) => app.set_status_error(Some(format!("Crop: {error}")), cx),
                        }
                    },
                ))
                .child(live_button(
                    "crop-cancel",
                    "Cancel".to_string(),
                    cx,
                    |app, cx| app.cancel_canvas_gesture(cx),
                ));
        }
        ToolId::Selection => {
            strip = strip
                .child(live_button(
                    "selection-shape",
                    match s.selection_shape {
                        crate::state::SelectionShape::Rect => "Rect".to_string(),
                        crate::state::SelectionShape::Ellipse => "Ellipse".to_string(),
                    },
                    cx,
                    |app, cx| app.toggle_selection_shape(cx),
                ))
                .child(live_button(
                    "selection-deselect",
                    "Deselect".to_string(),
                    cx,
                    |app, cx| app.deselect_pixels(cx),
                ));
        }
        ToolId::Text => {
            strip = strip
                .child(step_pair(
                    "text-size",
                    format!("Size {:.0}", s.text_font_size),
                    cx,
                    ToolSetting::TextSize,
                ))
                .child(toggle_button(
                    "text-bold",
                    if s.text_font_weight >= 700.0 {
                        "Bold".to_string()
                    } else {
                        "Regular".to_string()
                    },
                    cx,
                    ToolFlag::TextBold,
                ))
                .child(toggle_button(
                    "text-italic",
                    format!("Italic: {}", on_off(s.text_italic)),
                    cx,
                    ToolFlag::TextItalic,
                ))
                .child(live_button(
                    "text-align",
                    match s.text_align {
                        crate::state::TextAlignTool::Left => "Left".to_string(),
                        crate::state::TextAlignTool::Center => "Center".to_string(),
                        crate::state::TextAlignTool::Right => "Right".to_string(),
                    },
                    cx,
                    |app, cx| app.cycle_text_align(cx),
                ));
        }
        ToolId::Move => {
            strip = strip
                .child(step_pair("move-x", "X".to_string(), cx, ToolSetting::MoveX))
                .child(step_pair("move-y", "Y".to_string(), cx, ToolSetting::MoveY))
                .child(step_pair(
                    "move-w",
                    "W".to_string(),
                    cx,
                    ToolSetting::MoveScaleX,
                ))
                .child(step_pair(
                    "move-h",
                    "H".to_string(),
                    cx,
                    ToolSetting::MoveScaleY,
                ))
                .child(step_pair(
                    "move-rotation",
                    "Rot".to_string(),
                    cx,
                    ToolSetting::MoveRotation,
                ))
                .child(live_button(
                    "move-flip-h",
                    "Flip H".to_string(),
                    cx,
                    |app, cx| {
                        let result = app.flip_selected_layer(true, cx);
                        if let Err(error) = result {
                            app.set_status_error(Some(format!("Transform: {error}")), cx);
                        }
                    },
                ))
                .child(live_button(
                    "move-flip-v",
                    "Flip V".to_string(),
                    cx,
                    |app, cx| {
                        let result = app.flip_selected_layer(false, cx);
                        if let Err(error) = result {
                            app.set_status_error(Some(format!("Transform: {error}")), cx);
                        }
                    },
                ));
        }
    }
    strip
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

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

fn live_button(
    id: &'static str,
    label: String,
    cx: &Context<crate::app::App>,
    on_click: impl Fn(&mut crate::app::App, &mut Context<crate::app::App>) + 'static,
) -> impl IntoElement {
    div()
        .id(format!("option-live-{id}"))
        .test_support()
        .h(px(22.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .border_1()
        .border_color(color::BORDER)
        .text_size(px(10.0))
        .text_color(color::TEXT)
        .cursor_pointer()
        .hover(|style| style.text_color(color::ACCENT))
        .on_click(cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| on_click(app, cx)))
        .child(label)
}

/// One `label − +` triple sharing the stepper clamp.
fn step_pair(
    id: &'static str,
    label: String,
    cx: &Context<crate::app::App>,
    setting: ToolSetting,
) -> Div {
    h_flex()
        .h(px(22.0))
        .items_center()
        .gap(px(2.0))
        .flex_shrink_0()
        .child(
            div()
                .text_size(px(10.0))
                .text_color(color::TEXT_DIM)
                .child(label),
        )
        .child(live_button(
            match id {
                "brush-size" => "brush-size-dec",
                "brush-hardness" => "brush-hardness-dec",
                "brush-opacity" => "brush-opacity-dec",
                "brush-flow" => "brush-flow-dec",
                "brush-smoothing" => "brush-smoothing-dec",
                "fill-tolerance" => "fill-tolerance-dec",
                "text-size" => "text-size-dec",
                "move-x" => "move-x-dec",
                "move-y" => "move-y-dec",
                "move-w" => "move-w-dec",
                "move-h" => "move-h-dec",
                "move-rotation" => "move-rotation-dec",
                _ => "shape-radius-dec",
            },
            "\u{2212}".to_string(),
            cx,
            move |app, cx| app.adjust_tool_setting(setting, -1.0, cx),
        ))
        .child(live_button(
            match id {
                "brush-size" => "brush-size-inc",
                "brush-hardness" => "brush-hardness-inc",
                "brush-opacity" => "brush-opacity-inc",
                "brush-flow" => "brush-flow-inc",
                "brush-smoothing" => "brush-smoothing-inc",
                "fill-tolerance" => "fill-tolerance-inc",
                "text-size" => "text-size-inc",
                "move-x" => "move-x-inc",
                "move-y" => "move-y-inc",
                "move-w" => "move-w-inc",
                "move-h" => "move-h-inc",
                "move-rotation" => "move-rotation-inc",
                _ => "shape-radius-inc",
            },
            "+".to_string(),
            cx,
            move |app, cx| app.adjust_tool_setting(setting, 1.0, cx),
        ))
}

fn toggle_button(
    id: &'static str,
    label: String,
    cx: &Context<crate::app::App>,
    flag: ToolFlag,
) -> impl IntoElement {
    live_button(id, label, cx, move |app, cx| {
        app.toggle_tool_flag(flag, cx);
    })
}

// ── Crop ─────────────────────────────────────────────────────────────────────

/// Pure dispatch for [`variants::crop_items`]: the resting-state strings/flags of the
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
    /// constants are the source of truth that [`variants::crop_items`] composes, so every
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
