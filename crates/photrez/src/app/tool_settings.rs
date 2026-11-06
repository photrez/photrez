//! Live tool settings behind the option bar's compact controls.
//!
//! The option bars expose these as sliders, number fields and
//! checkboxes; the native bar keeps the captured chrome and adds a compact
//! stepper strip so every setting below is user-reachable. The values and
//! clamps match the tool defaults the bars are built on.

use gpui_kit::Context;
use photrez_core::document::layer_params::{
    BasicAdjustment, ShapeFillKind, ShapeKind, ShapeParams,
};
use photrez_core::protocol::{Command, TransformPatch};

use super::App;

/// The weight dropdown's values, ordered smallest to largest.
pub(crate) const TEXT_WEIGHTS: [f64; 7] = [100.0, 300.0, 400.0, 500.0, 600.0, 700.0, 900.0];

/// View → Snap (`Shift+Ctrl+;`), the shortcut the View menu advertises.
#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct ToggleSnapAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrushField {
    Size,
    Hardness,
    Opacity,
    Flow,
    Smoothing,
}

/// `(size, hardness, opacity, flow, smoothing, eraser_only?)`, the
/// six built-in brush presets.
pub(crate) const BRUSH_PRESETS: [(f64, f64, f64, f64, f64, bool); 6] = [
    (20.0, 1.0, 1.0, 1.0, 0.0, false),
    (40.0, 0.15, 1.0, 1.0, 0.0, false),
    (5.0, 0.8, 1.0, 1.0, 10.0, false),
    (100.0, 0.0, 0.85, 0.65, 0.0, false),
    (30.0, 1.0, 1.0, 1.0, 0.0, true),
    (50.0, 0.15, 1.0, 0.85, 0.0, true),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolFlag {
    FillContiguous,
    GradientReverse,
    AutoCopyHex,
    ShapeArrowHead,
}

/// The core shape enum a bar preset maps to, for write-through edits.
pub(crate) fn shape_kind_for_tool(tool: crate::state::ShapeTool) -> ShapeKind {
    use crate::state::ShapeTool;
    match tool {
        ShapeTool::Rect => ShapeKind::Rect,
        ShapeTool::Ellipse => ShapeKind::Ellipse,
        ShapeTool::Line => ShapeKind::Line,
        ShapeTool::Triangle => ShapeKind::Triangle,
        ShapeTool::Star => ShapeKind::Star,
        ShapeTool::BlockArrow => ShapeKind::BlockArrow,
        ShapeTool::Heart => ShapeKind::Heart,
        ShapeTool::Diamond => ShapeKind::Diamond,
        ShapeTool::SpeechBubble => ShapeKind::SpeechBubble,
        ShapeTool::Hexagon => ShapeKind::Hexagon,
    }
}

/// The bar preset a selected layer's kind shows, for the projected read.
pub(crate) fn shape_tool_for_kind(kind: &ShapeKind) -> crate::state::ShapeTool {
    use crate::state::ShapeTool;
    match kind {
        ShapeKind::Rect => ShapeTool::Rect,
        ShapeKind::Ellipse => ShapeTool::Ellipse,
        ShapeKind::Line => ShapeTool::Line,
        ShapeKind::Triangle => ShapeTool::Triangle,
        ShapeKind::Star => ShapeTool::Star,
        ShapeKind::BlockArrow => ShapeTool::BlockArrow,
        ShapeKind::Heart => ShapeTool::Heart,
        ShapeKind::Diamond => ShapeTool::Diamond,
        ShapeKind::SpeechBubble => ShapeTool::SpeechBubble,
        ShapeKind::Hexagon => ShapeTool::Hexagon,
    }
}

impl App {
    pub(crate) fn toggle_tool_flag(&mut self, flag: ToolFlag, cx: &mut Context<Self>) {
        let mut arrow = None;
        self.state.update(cx, |state, _| match flag {
            ToolFlag::FillContiguous => state.fill_contiguous = !state.fill_contiguous,
            ToolFlag::GradientReverse => state.gradient_reverse = !state.gradient_reverse,
            ToolFlag::AutoCopyHex => state.auto_copy_hex = !state.auto_copy_hex,
            ToolFlag::ShapeArrowHead => {
                state.shape_arrow_head = !state.shape_arrow_head;
                arrow = Some(state.shape_arrow_head);
            }
        });
        if let Some(value) = arrow {
            self.apply_shape_edit(cx, |params| params.arrow_head = value);
        }
        cx.notify();
    }

    /// Absolute tolerance edit from the Paint Bucket bar (0..255).
    pub(crate) fn set_fill_tolerance(
        &mut self,
        value: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let value = value.round().clamp(0.0, 255.0) as u32;
        self.state
            .update(cx, |state, _| state.fill_tolerance = value);
        cx.notify();
        Ok(())
    }

    /// The shape bar's edit mode: a shape layer is selected, so the
    /// controls read and write that layer instead of the next-shape defaults.
    fn is_shape_layer_selected(&self, cx: &Context<Self>) -> bool {
        self.state.read(cx).selected_shape.is_some()
    }

    /// Edits the selected shape layer after syncing the next-shape defaults,
    /// which is the shape bar's edit-mode contract. Repeating an edit at its
    /// current value is a no-op so pointer ticks never stack history entries
    /// (the bar compares the params before writing).
    pub(crate) fn edit_selected_shape(
        &mut self,
        cx: &mut Context<Self>,
        mutate: impl FnOnce(&mut ShapeParams),
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let mut params = layer
            .shape_params
            .clone()
            .ok_or_else(|| "the selected layer is not a shape".to_string())?;
        let before = params.clone();
        mutate(&mut params);
        self.sync_shape_defaults(&params, cx);
        if params == before {
            return Ok(());
        }
        self.apply_core_command(
            &doc_id,
            Command::SetLayerParams {
                id: layer.id,
                shape_params: Some(params),
                text_data: None,
            },
            true,
            cx,
        )
    }

    /// Routes a bar edit to the selected shape layer when one is selected; in
    /// draw mode the caller has already updated the next-shape defaults.
    fn apply_shape_edit(&mut self, cx: &mut Context<Self>, mutate: impl FnOnce(&mut ShapeParams)) {
        if !self.is_shape_layer_selected(cx) {
            return;
        }
        if let Err(error) = self.edit_selected_shape(cx, mutate) {
            self.set_status_error(Some(format!("Shape: {error}")), cx);
        }
    }

    fn sync_shape_defaults(&mut self, params: &ShapeParams, cx: &mut Context<Self>) {
        let color = crate::palette::parse_hex(&params.stroke.color);
        self.state.update(cx, |state, _| {
            state.shape_kind = shape_tool_for_kind(&params.kind);
            state.shape_fill = params.fill.kind == ShapeFillKind::Solid;
            state.shape_stroke = params.stroke.enabled;
            state.shape_stroke_width = params.stroke.width;
            if let Some(color) = color {
                state.shape_stroke_color = color;
            }
            state.shape_radius = params.radius;
            state.shape_arrow_head = params.arrow_head;
        });
    }

    /// Fill toggle: the bar refuses to turn
    /// fill off while stroke is already off, so a shape never becomes
    /// invisible and still hittable.
    pub(crate) fn toggle_shape_fill(&mut self, cx: &mut Context<Self>) {
        let (fill, stroke) = self.effective_shape_paint(cx);
        if fill && !stroke {
            return;
        }
        let next = !fill;
        self.state.update(cx, |state, _| state.shape_fill = next);
        self.apply_shape_edit(cx, |params| {
            params.fill.kind = if next {
                ShapeFillKind::Solid
            } else {
                ShapeFillKind::None
            };
        });
        cx.notify();
    }

    /// Stroke toggle, same last-paint guard.
    pub(crate) fn toggle_shape_stroke(&mut self, cx: &mut Context<Self>) {
        let (fill, stroke) = self.effective_shape_paint(cx);
        if stroke && !fill {
            return;
        }
        let next = !stroke;
        self.state.update(cx, |state, _| state.shape_stroke = next);
        self.apply_shape_edit(cx, |params| params.stroke.enabled = next);
        cx.notify();
    }

    /// `(fill, stroke)` as the bar shows them: the selected layer's values in
    /// edit mode, the next-shape defaults otherwise.
    fn effective_shape_paint(&self, cx: &Context<Self>) -> (bool, bool) {
        let state = self.state.read(cx);
        match state.selected_shape.as_ref() {
            Some(params) => (
                params.fill.kind == ShapeFillKind::Solid,
                params.stroke.enabled,
            ),
            None => (state.shape_fill, state.shape_stroke),
        }
    }

    /// Absolute corner radius from the bar's field, clamped 0..100.
    pub(crate) fn set_shape_radius(
        &mut self,
        value: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let value = value.clamp(0.0, 100.0);
        self.state.update(cx, |state, _| state.shape_radius = value);
        self.apply_shape_edit(cx, |params| params.radius = value);
        cx.notify();
        Ok(())
    }

    /// Outline width from the stroke popover, whose input is `min 1`,
    /// `max 200`.
    pub(crate) fn set_shape_stroke_width(
        &mut self,
        value: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let value = value.clamp(1.0, 200.0);
        self.state
            .update(cx, |state, _| state.shape_stroke_width = value);
        self.apply_shape_edit(cx, |params| params.stroke.width = value);
        cx.notify();
        Ok(())
    }

    /// A stroke-colour pick in edit mode writes the layer as well and keeps
    /// the outline on when the colour is chosen.
    pub(crate) fn set_shape_stroke_color(&mut self, color: [u8; 4], cx: &mut Context<Self>) {
        let hex = crate::palette::to_hex(color);
        self.state.update(cx, |state, _| {
            state.shape_stroke_color = color;
            state.shape_stroke = true;
        });
        self.apply_shape_edit(cx, |params| {
            params.stroke.color = hex;
            params.stroke.enabled = true;
        });
        cx.notify();
    }

    /// The shape bar's fill swatch in edit mode:
    /// the paint foreground follows and the chosen colour forces a solid fill.
    pub(crate) fn set_shape_fill_color(&mut self, color: [u8; 4], cx: &mut Context<Self>) {
        let hex = crate::palette::to_hex(color);
        self.state.update(cx, |state, _| {
            state.fg_color = color;
            state.shape_fill = true;
        });
        self.apply_shape_edit(cx, |params| {
            params.fill.color = hex;
            params.fill.kind = ShapeFillKind::Solid;
        });
        cx.notify();
    }

    /// The stroke pill toggles the popover, not the outline.
    pub(crate) fn toggle_stroke_popover(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) {
        let open = self.state.read(cx).stroke_popover_open;
        self.state
            .update(cx, |state, _| state.stroke_popover_open = !open);
        if !open {
            // Escape is bound in the Canvas context, so keep it active.
            window.focus(&self.canvas_focus_handle, cx);
        }
        cx.notify();
    }

    pub(crate) fn close_stroke_popover(&mut self, cx: &mut Context<Self>) {
        if !self.state.read(cx).stroke_popover_open {
            return;
        }
        self.state
            .update(cx, |state, _| state.stroke_popover_open = false);
        cx.notify();
    }

    /// Cycle the built-in brush presets. Applies one brush preset by its
    /// global index. Presets marked
    /// eraser-only are ignored while the brush is active, matching the
    /// filtered preset list the bar shows.
    pub(crate) fn apply_brush_preset(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(&(size, hardness, opacity, flow, smoothing, eraser_only)) =
            BRUSH_PRESETS.get(index)
        else {
            return;
        };
        let eraser = self.state.read(cx).active_tool == crate::state::ToolId::Eraser;
        if eraser_only && !eraser {
            return;
        }
        self.state.update(cx, |state, _| {
            state.brush_preset_index = index;
            if eraser {
                state.eraser_size = size;
                state.eraser_hardness = hardness;
                state.eraser_opacity = opacity;
                state.eraser_flow = flow;
                state.eraser_smoothing = smoothing;
            } else {
                state.brush_size = size;
                state.brush_hardness = hardness;
                state.brush_opacity = opacity;
                state.brush_flow = flow;
                state.brush_smoothing = smoothing;
            }
        });
        cx.notify();
    }

    /// Flip the selected layer horizontally/vertically (option bar).
    /// Set one basic-adjustment channel to an absolute value (the
    /// Adjustments panel's sliders), clamped to the -100..=100 range those
    /// inputs use.
    pub(crate) fn set_selected_adjustment(
        &mut self,
        channel: u8,
        value: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let mut adjustment = layer.basic_adjustment.clone().unwrap_or_default();
        let value = value.clamp(-100.0, 100.0);
        match channel {
            0 => adjustment.brightness = value,
            1 => adjustment.contrast = value,
            _ => adjustment.saturation = value,
        }
        self.apply_core_command(
            &doc_id,
            Command::SetAdjustment {
                id: layer.id,
                adjustment: Some(BasicAdjustment {
                    brightness: adjustment.brightness,
                    contrast: adjustment.contrast,
                    saturation: adjustment.saturation,
                }),
            },
            true,
            cx,
        )
    }

    pub(crate) fn reset_selected_adjustment(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        self.apply_core_command(
            &doc_id,
            Command::SetAdjustment {
                id: layer.id,
                adjustment: None,
            },
            true,
            cx,
        )
    }

    pub(crate) fn toggle_snap(&mut self, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, _| state.snap_enabled = !state.snap_enabled);
        cx.notify();
    }

    pub(crate) fn toggle_snap_action(&mut self, _: &ToggleSnapAction, cx: &mut Context<Self>) {
        self.toggle_snap(cx);
    }

    pub(crate) fn toggle_move_auto_select(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.move_auto_select = !state.move_auto_select
        });
        cx.notify();
    }

    pub(crate) fn toggle_transform_controls(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.show_transform_controls = !state.show_transform_controls
        });
        cx.notify();
    }

    /// Hold the two size axes at one ratio. The Move bar's ratio button and the
    /// Properties panel's link share this state.
    pub(crate) fn toggle_constrain_ratio(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.constrain_ratio = !state.constrain_ratio
        });
        cx.notify();
    }

    /// Opens or closes one not-yet-built adjustment group. Groups are
    /// independent, so opening Curves leaves HSL as it was.
    pub(crate) fn toggle_adjustment_section(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            let index = state
                .open_adjustment_sections
                .iter()
                .position(|open| open == id);
            match index {
                Some(index) => {
                    state.open_adjustment_sections.remove(index);
                }
                None => state.open_adjustment_sections.push(id.to_string()),
            }
        });
        cx.notify();
    }

    pub(crate) fn flip_selected_layer(
        &mut self,
        horizontal: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let mut transform = layer.transform.clone();
        if horizontal {
            transform.flip_h = !transform.flip_h;
        } else {
            transform.flip_v = !transform.flip_v;
        }
        self.apply_core_command(
            &doc_id,
            Command::TransformLayer {
                id: layer.id.clone(),
                transform: TransformPatch {
                    x: transform.x,
                    y: transform.y,
                    scale_x: transform.scale_x,
                    scale_y: transform.scale_y,
                    rotation: transform.rotation,
                    flip_h: Some(transform.flip_h),
                    flip_v: Some(transform.flip_v),
                },
            },
            true,
            cx,
        )
    }

    /// The text bar's edit mode: a text layer is selected.
    fn is_text_layer_selected(&self, cx: &Context<Self>) -> bool {
        self.state.read(cx).selected_layer_type.as_deref() == Some("text")
    }

    /// Edits the selected text layer and syncs the next-text defaults, which
    /// is the text bar's edit-mode contract.
    pub(crate) fn edit_selected_text(
        &mut self,
        cx: &mut Context<Self>,
        mutate: impl FnOnce(&mut photrez_core::document::layer_params::TextData),
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let mut data = layer
            .text_data
            .clone()
            .ok_or_else(|| "the selected layer is not text".to_string())?;
        let before = data.clone();
        mutate(&mut data);
        self.sync_text_defaults(&data, cx);
        // Repeating an edit at its current value is a no-op so pointer ticks
        // never stack history entries.
        if data == before {
            return Ok(());
        }
        self.apply_core_command(
            &doc_id,
            Command::SetLayerParams {
                id: layer.id,
                shape_params: None,
                text_data: Some(data),
            },
            true,
            cx,
        )
    }

    fn sync_text_defaults(
        &mut self,
        data: &photrez_core::document::layer_params::TextData,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| {
            state.text_font_size = data.font_size;
            state.text_font_weight = data.font_weight;
            state.text_italic =
                data.font_style == photrez_core::document::layer_params::TextFontStyle::Italic;
            state.text_align = match data.align {
                photrez_core::document::layer_params::TextAlign::Left => {
                    crate::state::TextAlignTool::Left
                }
                photrez_core::document::layer_params::TextAlign::Center => {
                    crate::state::TextAlignTool::Center
                }
                photrez_core::document::layer_params::TextAlign::Right => {
                    crate::state::TextAlignTool::Right
                }
            };
            state.text_underline = data.underline.unwrap_or(false);
            state.text_strikethrough = data.strikethrough.unwrap_or(false);
            state.text_uppercase = data.uppercase.unwrap_or(false);
            state.text_stroke_width = data.stroke.width;
            state.text_stroke_align = data
                .stroke
                .align
                .unwrap_or(photrez_core::document::layer_params::TextStrokeAlign::Outside);
            if let Some(color) = crate::palette::parse_hex(&data.stroke.color) {
                state.text_stroke_color = color;
            }
        });
    }

    /// The eraser bar's `Hard 100` helper: hardness, strength and flow to
    /// 100 %, preset cleared to Custom, as the brush bar's own helper does.
    pub(crate) fn eraser_full_strength(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.eraser_hardness = 1.0;
            state.eraser_opacity = 1.0;
            state.eraser_flow = 1.0;
            state.brush_preset_index = usize::MAX;
        });
        cx.notify();
    }

    /// Absolute edits from the brush bar's number fields. Percent fields
    /// arrive as 0..100 and are stored as 0..1, the same convention the bar
    /// uses.
    pub(crate) fn set_brush_value(
        &mut self,
        field: BrushField,
        value: f64,
        cx: &mut Context<Self>,
    ) {
        let eraser = self.state.read(cx).active_tool == crate::state::ToolId::Eraser;
        self.state.update(cx, |state, _| {
            state.brush_preset_index = usize::MAX;
            match (field, eraser) {
                (BrushField::Size, false) => state.brush_size = value.clamp(1.0, 2000.0),
                (BrushField::Size, true) => state.eraser_size = value.clamp(1.0, 2000.0),
                (BrushField::Hardness, false) => {
                    state.brush_hardness = (value / 100.0).clamp(0.0, 1.0)
                }
                (BrushField::Hardness, true) => {
                    state.eraser_hardness = (value / 100.0).clamp(0.0, 1.0)
                }
                (BrushField::Opacity, false) => {
                    state.brush_opacity = (value / 100.0).clamp(0.0, 1.0)
                }
                (BrushField::Opacity, true) => {
                    state.eraser_opacity = (value / 100.0).clamp(0.0, 1.0)
                }
                (BrushField::Flow, false) => state.brush_flow = (value / 100.0).clamp(0.0, 1.0),
                (BrushField::Flow, true) => state.eraser_flow = (value / 100.0).clamp(0.0, 1.0),
                (BrushField::Smoothing, false) => state.brush_smoothing = value.clamp(0.0, 100.0),
                (BrushField::Smoothing, true) => state.eraser_smoothing = value.clamp(0.0, 100.0),
            }
        });
        cx.notify();
    }

    /// Absolute size edit from the inline field, clamped 1..2000.
    pub(crate) fn set_text_font_size(
        &mut self,
        value: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let value = value.clamp(1.0, 2000.0);
        if !self.is_text_layer_selected(cx) {
            self.state
                .update(cx, |state, _| state.text_font_size = value);
            cx.notify();
            return Ok(());
        }
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let mut data = layer
            .text_data
            .clone()
            .ok_or_else(|| "the selected layer is not text".to_string())?;
        data.font_size = value;
        self.state
            .update(cx, |state, _| state.text_font_size = value);
        self.apply_core_command(
            &doc_id,
            Command::SetLayerParams {
                id: layer.id,
                shape_params: None,
                text_data: Some(data),
            },
            true,
            cx,
        )
    }

    pub(crate) fn set_text_font_weight(
        &mut self,
        value: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let value = value.clamp(100.0, 900.0);
        if !self.is_text_layer_selected(cx) {
            self.state
                .update(cx, |state, _| state.text_font_weight = value);
            cx.notify();
            return Ok(());
        }
        self.edit_selected_text(cx, |data| data.font_weight = value)
    }

    pub(crate) fn toggle_text_italic(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if !self.is_text_layer_selected(cx) {
            self.state
                .update(cx, |state, _| state.text_italic = !state.text_italic);
            cx.notify();
            return Ok(());
        }
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let mut data = layer
            .text_data
            .clone()
            .ok_or_else(|| "the selected layer is not text".to_string())?;
        data.font_style = match data.font_style {
            photrez_core::document::layer_params::TextFontStyle::Italic => {
                photrez_core::document::layer_params::TextFontStyle::Normal
            }
            _ => photrez_core::document::layer_params::TextFontStyle::Italic,
        };
        self.state.update(cx, |state, _| {
            state.text_italic =
                data.font_style == photrez_core::document::layer_params::TextFontStyle::Italic;
        });
        self.apply_core_command(
            &doc_id,
            Command::SetLayerParams {
                id: layer.id,
                shape_params: None,
                text_data: Some(data),
            },
            true,
            cx,
        )
    }

    pub(crate) fn set_text_align(
        &mut self,
        align: crate::state::TextAlignTool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.is_text_layer_selected(cx) {
            self.state.update(cx, |state, _| state.text_align = align);
            cx.notify();
            return Ok(());
        }
        self.edit_selected_text(cx, |data| {
            data.align = match align {
                crate::state::TextAlignTool::Left => {
                    photrez_core::document::layer_params::TextAlign::Left
                }
                crate::state::TextAlignTool::Center => {
                    photrez_core::document::layer_params::TextAlign::Center
                }
                crate::state::TextAlignTool::Right => {
                    photrez_core::document::layer_params::TextAlign::Right
                }
            };
        })
    }

    pub(crate) fn toggle_text_underline(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if !self.is_text_layer_selected(cx) {
            self.state
                .update(cx, |state, _| state.text_underline = !state.text_underline);
            cx.notify();
            return Ok(());
        }
        self.edit_selected_text(cx, |data| {
            data.underline = Some(!data.underline.unwrap_or(false));
        })
    }

    pub(crate) fn toggle_text_strikethrough(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.is_text_layer_selected(cx) {
            self.state.update(cx, |state, _| {
                state.text_strikethrough = !state.text_strikethrough;
            });
            cx.notify();
            return Ok(());
        }
        self.edit_selected_text(cx, |data| {
            data.strikethrough = Some(!data.strikethrough.unwrap_or(false));
        })
    }

    pub(crate) fn toggle_text_uppercase(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if !self.is_text_layer_selected(cx) {
            self.state
                .update(cx, |state, _| state.text_uppercase = !state.text_uppercase);
            cx.notify();
            return Ok(());
        }
        self.edit_selected_text(cx, |data| {
            data.uppercase = Some(!data.uppercase.unwrap_or(false));
        })
    }

    /// The stroke width the bar shows: the selected text layer's value in edit
    /// mode, the next-text default otherwise.
    pub(crate) fn effective_text_stroke_width(&self, cx: &Context<Self>) -> f64 {
        let state = self.state.read(cx);
        if self.is_text_layer_selected(cx) {
            state.selected_text_stroke_width
        } else {
            state.text_stroke_width
        }
    }

    /// Stroke width from the popover's slider and toggle, clamped 0..100.
    pub(crate) fn set_text_stroke_width(
        &mut self,
        value: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let value = value.clamp(0.0, 100.0);
        if !self.is_text_layer_selected(cx) {
            self.state
                .update(cx, |state, _| state.text_stroke_width = value);
            cx.notify();
            return Ok(());
        }
        self.edit_selected_text(cx, |data| data.stroke.width = value)
    }

    /// The number input clamps 1..100: clearing the box must not disable the
    /// stroke, and the switch is the explicit off control.
    pub(crate) fn set_text_stroke_width_field(
        &mut self,
        value: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.set_text_stroke_width(value.clamp(1.0, 100.0), cx)
    }

    /// The stepper never disables the stroke; 1 is its floor.
    pub(crate) fn step_text_stroke(&mut self, delta: f64, cx: &mut Context<Self>) {
        let current = self.effective_text_stroke_width(cx);
        let _ = self.set_text_stroke_width((current + delta).clamp(1.0, 100.0), cx);
        cx.notify();
    }

    pub(crate) fn toggle_text_stroke(&mut self, cx: &mut Context<Self>) {
        let next = if self.effective_text_stroke_width(cx) > 0.0 {
            0.0
        } else {
            4.0
        };
        let _ = self.set_text_stroke_width(next, cx);
        cx.notify();
    }

    pub(crate) fn set_text_stroke_color(&mut self, color: [u8; 4], cx: &mut Context<Self>) {
        let hex = crate::palette::to_hex(color);
        if !self.is_text_layer_selected(cx) {
            self.state
                .update(cx, |state, _| state.text_stroke_color = color);
            cx.notify();
            return;
        }
        if let Err(error) = self.edit_selected_text(cx, |data| data.stroke.color = hex) {
            self.set_status_error(Some(format!("Text: {error}")), cx);
        }
    }

    pub(crate) fn set_text_stroke_align(
        &mut self,
        align: photrez_core::document::layer_params::TextStrokeAlign,
        cx: &mut Context<Self>,
    ) {
        if !self.is_text_layer_selected(cx) {
            self.state
                .update(cx, |state, _| state.text_stroke_align = align);
            cx.notify();
            return;
        }
        if let Err(error) = self.edit_selected_text(cx, |data| data.stroke.align = Some(align)) {
            self.set_status_error(Some(format!("Text: {error}")), cx);
        }
    }

    /// The text stroke pill toggles the popover, not the outline.
    pub(crate) fn toggle_text_stroke_popover(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) {
        let open = self.state.read(cx).text_stroke_popover_open;
        self.state
            .update(cx, |state, _| state.text_stroke_popover_open = !open);
        if !open {
            window.focus(&self.canvas_focus_handle, cx);
        }
        cx.notify();
    }

    pub(crate) fn close_text_stroke_popover(&mut self, cx: &mut Context<Self>) {
        if !self.state.read(cx).text_stroke_popover_open {
            return;
        }
        self.state
            .update(cx, |state, _| state.text_stroke_popover_open = false);
        cx.notify();
    }
}
