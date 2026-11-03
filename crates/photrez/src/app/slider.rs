//! Slider input for every track host: click and drag map the pointer's x
//! position over the laid-out track to a value, and the arrow keys step that
//! value in its own units.

use gpui_kit::{Context, KeyDownEvent};

use crate::state::{SliderTarget, ToolId};

use super::brush_menu::paint_size_to_slider;
use super::App;

impl App {
    pub(crate) fn slider_pointer_down_for(
        &mut self,
        target: SliderTarget,
        x: f32,
        cx: &mut Context<Self>,
    ) {
        self.slider_dragging = true;
        self.set_slider_from_x(target, x, cx);
    }

    pub(crate) fn slider_pointer_move_for(
        &mut self,
        target: SliderTarget,
        x: f32,
        cx: &mut Context<Self>,
    ) {
        if self.slider_dragging {
            self.set_slider_from_x(target, x, cx);
        }
    }

    pub(crate) fn slider_pointer_up(&mut self) {
        self.slider_dragging = false;
    }

    /// The keyboard path for a track. Each target reads its own current value
    /// and writes through the setter the pointer path uses.
    pub(crate) fn slider_key(
        &mut self,
        target: SliderTarget,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        match target {
            SliderTarget::Tolerance => {
                let current = self.state.read(cx).fill_tolerance as f64;
                let Some(value) = Self::stepped(current, key, 0.0, 255.0) else {
                    return false;
                };
                let result = self.set_fill_tolerance(value, cx);
                self.report_slider_write("Tolerance", result, cx);
            }
            SliderTarget::TextStrokeWidth => {
                let current = self.effective_text_stroke_width(cx);
                let Some(value) = Self::stepped(current, key, 1.0, 100.0) else {
                    return false;
                };
                let result = self.set_text_stroke_width(value, cx);
                self.report_slider_write("Text", result, cx);
            }
            SliderTarget::BrushMenuSize
            | SliderTarget::BrushMenuHardness
            | SliderTarget::BrushMenuStrength => {
                // The menu's three tracks are 0..=100 positions; Size converts
                // through the same curve the pointer path uses, so a key and a
                // drag of the same position land on the same pixel width.
                let eraser = self.state.read(cx).active_tool == ToolId::Eraser;
                let current = {
                    let state = self.state.read(cx);
                    match target {
                        SliderTarget::BrushMenuSize => paint_size_to_slider(if eraser {
                            state.eraser_size
                        } else {
                            state.brush_size
                        }),
                        SliderTarget::BrushMenuHardness => {
                            100.0
                                * if eraser {
                                    state.eraser_hardness
                                } else {
                                    state.brush_hardness
                                }
                        }
                        _ => {
                            100.0
                                * if eraser {
                                    state.eraser_opacity
                                } else {
                                    state.brush_opacity
                                }
                        }
                    }
                };
                let Some(position) = Self::stepped(current, key, 0.0, 100.0) else {
                    return false;
                };
                self.brush_menu_set_from_fraction(target, position / 100.0, cx);
            }
            SliderTarget::ExportQuality => {
                let current = self.state.read(cx).export_quality as f64;
                let Some(value) = Self::stepped(current, key, 1.0, 100.0) else {
                    return false;
                };
                self.state
                    .update(cx, |state, _| state.export_quality = value.round() as u8);
                cx.notify();
            }
            SliderTarget::AdjustmentBrightness
            | SliderTarget::AdjustmentContrast
            | SliderTarget::AdjustmentSaturation => {
                let channel = match target {
                    SliderTarget::AdjustmentBrightness => 0,
                    SliderTarget::AdjustmentContrast => 1,
                    _ => 2,
                };
                let current = {
                    let state = self.state.read(cx);
                    match channel {
                        0 => state.adjustment_brightness,
                        1 => state.adjustment_contrast,
                        _ => state.adjustment_saturation,
                    }
                };
                let Some(value) = Self::stepped(current, key, -100.0, 100.0) else {
                    return false;
                };
                let result = self.set_selected_adjustment(channel, value, cx);
                self.report_slider_write("Adjust", result, cx);
            }
            SliderTarget::PrintScale => {
                let current = self.state.read(cx).print_scale_percent as f64;
                let Some(value) = Self::stepped(current, key, 10.0, 400.0) else {
                    return false;
                };
                self.state.update(cx, |state, _| {
                    state.print_scale_percent = value.round() as u32;
                    state.print_scale_to_fit = false;
                });
                cx.notify();
            }
            SliderTarget::LayerOpacity => {
                let current = self.state.read(cx).layer_opacity * 100.0;
                let Some(value) = Self::stepped(current, key, 0.0, 100.0) else {
                    return false;
                };
                let result = self.set_selected_layer_opacity(value / 100.0, cx);
                self.report_slider_write("Opacity", result, cx);
            }
        }
        true
    }

    /// One key on a `min..=max` track, in that track's own units: arrows step
    /// one unit and Home/End go to the ends, as a range input does.
    fn stepped(current: f64, key: &str, min: f64, max: f64) -> Option<f64> {
        match key {
            "left" | "down" => Some((current - 1.0).clamp(min, max)),
            "right" | "up" => Some((current + 1.0).clamp(min, max)),
            "home" => Some(min),
            "end" => Some(max),
            _ => None,
        }
    }

    /// A rejected track write becomes a status line; an accepted one clears the
    /// previous warning.
    fn report_slider_write(
        &mut self,
        label: &str,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(format!("{label}: {error}")), cx),
        }
    }

    fn set_slider_from_x(&mut self, target: SliderTarget, x: f32, cx: &mut Context<Self>) {
        let bounds = {
            let state = self.state.read(cx);
            match target {
                SliderTarget::Tolerance => state.tolerance_slider_bounds.get(),
                SliderTarget::TextStrokeWidth => state.text_stroke_slider_bounds.get(),
                SliderTarget::BrushMenuSize => state.brush_menu_size_bounds.get(),
                SliderTarget::BrushMenuHardness => state.brush_menu_hardness_bounds.get(),
                SliderTarget::BrushMenuStrength => state.brush_menu_strength_bounds.get(),
                SliderTarget::ExportQuality => state.export_quality_slider_bounds.get(),
                SliderTarget::AdjustmentBrightness => state.adjustment_slider_bounds[0].get(),
                SliderTarget::AdjustmentContrast => state.adjustment_slider_bounds[1].get(),
                SliderTarget::AdjustmentSaturation => state.adjustment_slider_bounds[2].get(),
                SliderTarget::PrintScale => state.print_scale_slider_bounds.get(),
                SliderTarget::LayerOpacity => state.opacity_slider_bounds.get(),
            }
        };
        let Some(bounds) = bounds else {
            return;
        };
        let width = f32::from(bounds.size.width).max(1.0);
        let fraction = ((x - f32::from(bounds.origin.x)) / width).clamp(0.0, 1.0) as f64;
        match target {
            SliderTarget::Tolerance => {
                let result = self.set_fill_tolerance((fraction * 255.0).round(), cx);
                match result {
                    Ok(()) => self.set_status_error(None, cx),
                    Err(error) => self.set_status_error(Some(format!("Tolerance: {error}")), cx),
                }
            }
            SliderTarget::TextStrokeWidth => {
                let result = self.set_text_stroke_width(1.0 + fraction * 99.0, cx);
                match result {
                    Ok(()) => self.set_status_error(None, cx),
                    Err(error) => self.set_status_error(Some(format!("Text: {error}")), cx),
                }
            }
            SliderTarget::BrushMenuSize
            | SliderTarget::BrushMenuHardness
            | SliderTarget::BrushMenuStrength => {
                self.brush_menu_set_from_fraction(target, fraction, cx);
            }
            SliderTarget::ExportQuality => {
                let quality = (1.0 + fraction * 99.0).round().clamp(1.0, 100.0) as u8;
                self.state
                    .update(cx, |state, _| state.export_quality = quality);
                cx.notify();
            }
            SliderTarget::PrintScale => {
                // The print scale field: min 10, max 400.
                let pct = (10.0 + fraction * 390.0).round().clamp(10.0, 400.0) as u32;
                self.state.update(cx, |state, _| {
                    state.print_scale_percent = pct;
                    state.print_scale_to_fit = false;
                });
                cx.notify();
            }
            SliderTarget::AdjustmentBrightness
            | SliderTarget::AdjustmentContrast
            | SliderTarget::AdjustmentSaturation => {
                // Centre-origin track: 50% is 0, the ends are -100/+100.
                let channel = match target {
                    SliderTarget::AdjustmentBrightness => 0,
                    SliderTarget::AdjustmentContrast => 1,
                    _ => 2,
                };
                let value = (fraction * 200.0 - 100.0).round();
                let result = self.set_selected_adjustment(channel, value, cx);
                match result {
                    Ok(()) => self.set_status_error(None, cx),
                    Err(error) => self.set_status_error(Some(format!("Adjust: {error}")), cx),
                }
            }
            SliderTarget::LayerOpacity => {
                let result = self.set_selected_layer_opacity(fraction, cx);
                match result {
                    Ok(()) => self.set_status_error(None, cx),
                    Err(error) => self.set_status_error(Some(format!("Opacity: {error}")), cx),
                }
            }
        }
    }
}
