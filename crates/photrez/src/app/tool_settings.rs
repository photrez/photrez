//! Live tool settings behind the option bar's compact controls.
//!
//! The frozen option bars expose these as sliders, number fields and
//! checkboxes; the native bar keeps the captured chrome and adds a compact
//! stepper strip so every setting below is user-reachable. The values and
//! clamps match `tools/editorState.ts` defaults.

use gpui_kit::Context;
use photrez_core::protocol::{Command, TransformPatch};
use photrez_core::ptz::layer_params::BasicAdjustment;

use crate::state::ShapeTool;

use super::App;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolSetting {
    BrushSize,
    BrushHardness,
    BrushOpacity,
    BrushFlow,
    BrushSmoothing,
    EraserSize,
    EraserHardness,
    EraserOpacity,
    EraserFlow,
    EraserSmoothing,
    FillTolerance,
    ShapeRadius,
    TextSize,
    MoveX,
    MoveY,
    MoveScaleX,
    MoveScaleY,
    MoveRotation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolFlag {
    FillContiguous,
    GradientRadial,
    GradientReverse,
    ShapeFill,
    ShapeStroke,
    AutoCopyHex,
    ShapeArrowHead,
    GradientFgTransparent,
    TextBold,
    TextItalic,
}

impl App {
    pub(crate) fn adjust_tool_setting(
        &mut self,
        setting: ToolSetting,
        direction: f64,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            setting,
            ToolSetting::MoveX
                | ToolSetting::MoveY
                | ToolSetting::MoveScaleX
                | ToolSetting::MoveScaleY
                | ToolSetting::MoveRotation
        ) {
            let result = self.adjust_selected_transform(setting, direction, cx);
            match result {
                Ok(()) => self.set_status_error(None, cx),
                Err(error) => self.set_status_error(Some(format!("Transform: {error}")), cx),
            }
            return;
        }
        self.state.update(cx, |state, _| {
            let step_size = |current: f64| (current * 0.1).round().max(1.0);
            match setting {
                ToolSetting::BrushSize => {
                    let step = step_size(state.brush_size);
                    state.brush_size = (state.brush_size + direction * step).clamp(1.0, 2000.0);
                }
                ToolSetting::EraserSize => {
                    let step = step_size(state.eraser_size);
                    state.eraser_size = (state.eraser_size + direction * step).clamp(1.0, 2000.0);
                }
                ToolSetting::BrushHardness => {
                    state.brush_hardness = (state.brush_hardness + direction * 0.1).clamp(0.0, 1.0);
                }
                ToolSetting::EraserHardness => {
                    state.eraser_hardness =
                        (state.eraser_hardness + direction * 0.1).clamp(0.0, 1.0);
                }
                ToolSetting::BrushOpacity => {
                    state.brush_opacity = (state.brush_opacity + direction * 0.1).clamp(0.0, 1.0);
                }
                ToolSetting::EraserOpacity => {
                    state.eraser_opacity = (state.eraser_opacity + direction * 0.1).clamp(0.0, 1.0);
                }
                ToolSetting::BrushFlow => {
                    state.brush_flow = (state.brush_flow + direction * 0.1).clamp(0.0, 1.0);
                }
                ToolSetting::EraserFlow => {
                    state.eraser_flow = (state.eraser_flow + direction * 0.1).clamp(0.0, 1.0);
                }
                ToolSetting::BrushSmoothing => {
                    state.brush_smoothing =
                        (state.brush_smoothing + direction * 5.0).clamp(0.0, 100.0);
                }
                ToolSetting::EraserSmoothing => {
                    state.eraser_smoothing =
                        (state.eraser_smoothing + direction * 5.0).clamp(0.0, 100.0);
                }
                ToolSetting::FillTolerance => {
                    let next = state.fill_tolerance as i64 + direction as i64 * 4;
                    state.fill_tolerance = next.clamp(0, 255) as u32;
                }
                ToolSetting::ShapeRadius => {
                    state.shape_radius = (state.shape_radius + direction).clamp(0.0, 256.0);
                }
                ToolSetting::TextSize => {
                    state.text_font_size =
                        (state.text_font_size + direction * 2.0).clamp(8.0, 400.0);
                }
                ToolSetting::MoveX
                | ToolSetting::MoveY
                | ToolSetting::MoveScaleX
                | ToolSetting::MoveScaleY
                | ToolSetting::MoveRotation => {}
            }
        });
        cx.notify();
    }

    pub(crate) fn toggle_tool_flag(&mut self, flag: ToolFlag, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| match flag {
            ToolFlag::FillContiguous => state.fill_contiguous = !state.fill_contiguous,
            ToolFlag::GradientRadial => state.gradient_radial = !state.gradient_radial,
            ToolFlag::GradientReverse => state.gradient_reverse = !state.gradient_reverse,
            ToolFlag::ShapeFill => state.shape_fill = !state.shape_fill,
            ToolFlag::ShapeStroke => state.shape_stroke = !state.shape_stroke,
            ToolFlag::AutoCopyHex => state.auto_copy_hex = !state.auto_copy_hex,
            ToolFlag::ShapeArrowHead => state.shape_arrow_head = !state.shape_arrow_head,
            ToolFlag::GradientFgTransparent => {
                state.gradient_fg_transparent = !state.gradient_fg_transparent
            }
            ToolFlag::TextBold => {
                state.text_font_weight = if state.text_font_weight >= 700.0 {
                    400.0
                } else {
                    700.0
                };
            }
            ToolFlag::TextItalic => state.text_italic = !state.text_italic,
        });
        cx.notify();
    }

    pub(crate) fn cycle_shape_kind(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.shape_kind = match state.shape_kind {
                ShapeTool::Rect => ShapeTool::Ellipse,
                ShapeTool::Ellipse => ShapeTool::Line,
                ShapeTool::Line => ShapeTool::Triangle,
                ShapeTool::Triangle => ShapeTool::Star,
                ShapeTool::Star => ShapeTool::BlockArrow,
                ShapeTool::BlockArrow => ShapeTool::Heart,
                ShapeTool::Heart => ShapeTool::Diamond,
                ShapeTool::Diamond => ShapeTool::SpeechBubble,
                ShapeTool::SpeechBubble => ShapeTool::Hexagon,
                ShapeTool::Hexagon => ShapeTool::Rect,
            };
        });
        cx.notify();
    }

    /// Cycle the frozen brush presets (`brushToolState.ts`).
    pub(crate) fn cycle_brush_preset(&mut self, cx: &mut Context<Self>) {
        // (id, size, hardness, opacity, flow, smoothing, eraser_only?)
        const PRESETS: &[(&str, f64, f64, f64, f64, f64, bool)] = &[
            ("hard-round", 20.0, 1.0, 1.0, 1.0, 0.0, false),
            ("soft-round", 40.0, 0.15, 1.0, 1.0, 0.0, false),
            ("detail", 5.0, 0.8, 1.0, 1.0, 10.0, false),
            ("large-soft", 100.0, 0.0, 0.85, 0.65, 0.0, false),
            ("hard-eraser", 30.0, 1.0, 1.0, 1.0, 0.0, true),
            ("soft-eraser", 50.0, 0.15, 1.0, 0.85, 0.0, true),
        ];
        let eraser = self.state.read(cx).active_tool == crate::state::ToolId::Eraser;
        self.state.update(cx, |state, _| {
            let applicable: Vec<usize> = PRESETS
                .iter()
                .enumerate()
                .filter(|(_, preset)| preset.6 == eraser)
                .map(|(index, _)| index)
                .collect();
            if applicable.is_empty() {
                return;
            }
            let position = applicable
                .iter()
                .position(|index| *index == state.brush_preset_index)
                .map(|position| (position + 1) % applicable.len())
                .unwrap_or(0);
            let index = applicable[position];
            let (_, size, hardness, opacity, flow, smoothing, _) = PRESETS[index];
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

    /// Numeric transform edits from the Move option bar.
    pub(crate) fn adjust_selected_transform(
        &mut self,
        setting: ToolSetting,
        direction: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let mut transform = layer.transform.clone();
        match setting {
            ToolSetting::MoveX => transform.x += direction,
            ToolSetting::MoveY => transform.y += direction,
            ToolSetting::MoveScaleX => {
                transform.scale_x = (transform.scale_x + direction * 0.1).max(0.01);
            }
            ToolSetting::MoveScaleY => {
                transform.scale_y = (transform.scale_y + direction * 0.1).max(0.01);
            }
            ToolSetting::MoveRotation => {
                transform.rotation = crate::app::transform_geometry::normalize_rotation(
                    transform.rotation + direction * 15.0,
                );
            }
            _ => {}
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

    /// Flip the selected layer horizontally/vertically (option bar).
    /// Step one basic-adjustment channel of the selected layer.
    pub(crate) fn adjust_selected_adjustment(
        &mut self,
        channel: u8,
        delta: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let mut adjustment = layer.basic_adjustment.clone().unwrap_or_default();
        match channel {
            0 => adjustment.brightness = (adjustment.brightness + delta).clamp(-100.0, 100.0),
            1 => adjustment.contrast = (adjustment.contrast + delta).clamp(-100.0, 100.0),
            _ => adjustment.saturation = (adjustment.saturation + delta).clamp(-100.0, 100.0),
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

    /// Edit the selected text layer's typography (properties panel).
    pub(crate) fn adjust_selected_text(
        &mut self,
        field: u8,
        delta: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let mut data = layer
            .text_data
            .clone()
            .ok_or_else(|| "the selected layer is not text".to_string())?;
        match field {
            0 => data.font_size = (data.font_size + delta * 2.0).clamp(8.0, 400.0),
            1 => data.line_height = (data.line_height + delta * 0.1).clamp(0.5, 4.0),
            2 => data.letter_spacing = (data.letter_spacing + delta).clamp(-20.0, 100.0),
            _ => {}
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

    pub(crate) fn toggle_selected_text_bold(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let mut data = layer
            .text_data
            .clone()
            .ok_or_else(|| "the selected layer is not text".to_string())?;
        data.font_weight = if data.font_weight >= 700.0 {
            400.0
        } else {
            700.0
        };
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

    pub(crate) fn toggle_selected_text_italic(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let mut data = layer
            .text_data
            .clone()
            .ok_or_else(|| "the selected layer is not text".to_string())?;
        data.font_style = match data.font_style {
            photrez_core::ptz::layer_params::TextFontStyle::Italic => {
                photrez_core::ptz::layer_params::TextFontStyle::Normal
            }
            _ => photrez_core::ptz::layer_params::TextFontStyle::Italic,
        };
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

    pub(crate) fn cycle_selected_text_align(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        let mut data = layer
            .text_data
            .clone()
            .ok_or_else(|| "the selected layer is not text".to_string())?;
        data.align = match data.align {
            photrez_core::ptz::layer_params::TextAlign::Left => {
                photrez_core::ptz::layer_params::TextAlign::Center
            }
            photrez_core::ptz::layer_params::TextAlign::Center => {
                photrez_core::ptz::layer_params::TextAlign::Right
            }
            photrez_core::ptz::layer_params::TextAlign::Right => {
                photrez_core::ptz::layer_params::TextAlign::Left
            }
        };
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

    pub(crate) fn cycle_text_align(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.text_align = match state.text_align {
                crate::state::TextAlignTool::Left => crate::state::TextAlignTool::Center,
                crate::state::TextAlignTool::Center => crate::state::TextAlignTool::Right,
                crate::state::TextAlignTool::Right => crate::state::TextAlignTool::Left,
            };
        });
        cx.notify();
    }

    pub(crate) fn toggle_selection_shape(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.selection_shape = match state.selection_shape {
                crate::state::SelectionShape::Rect => crate::state::SelectionShape::Ellipse,
                crate::state::SelectionShape::Ellipse => crate::state::SelectionShape::Rect,
            };
        });
        cx.notify();
    }

    pub(crate) fn deselect_pixels(&mut self, cx: &mut Context<Self>) {
        let result = self.deselect(cx);
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(error), cx),
        }
    }
}
