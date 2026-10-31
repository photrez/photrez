//! Option-bar slider input: click and drag map the pointer's x position over
//! the laid-out track to a value.

use gpui_kit::{Context, KeyDownEvent};

use crate::state::SliderTarget;

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

    pub(crate) fn slider_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        self.slider_key_for(SliderTarget::Tolerance, event, cx)
    }

    pub(crate) fn text_stroke_slider_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        self.slider_key_for(SliderTarget::TextStrokeWidth, event, cx)
    }

    fn slider_key_for(
        &mut self,
        target: SliderTarget,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        match target {
            SliderTarget::Tolerance => {
                let current = self.state.read(cx).fill_tolerance as f64;
                let value = match event.keystroke.key.as_str() {
                    "left" | "down" => current - 1.0,
                    "right" | "up" => current + 1.0,
                    "home" => 0.0,
                    "end" => 255.0,
                    _ => return false,
                };
                let result = self.set_fill_tolerance(value, cx);
                match result {
                    Ok(()) => self.set_status_error(None, cx),
                    Err(error) => self.set_status_error(Some(format!("Tolerance: {error}")), cx),
                }
            }
            SliderTarget::TextStrokeWidth => {
                let current = self.effective_text_stroke_width(cx);
                let value = match event.keystroke.key.as_str() {
                    "left" | "down" => current - 1.0,
                    "right" | "up" => current + 1.0,
                    "home" => 1.0,
                    "end" => 100.0,
                    _ => return false,
                };
                let result = self.set_text_stroke_width(value.clamp(1.0, 100.0), cx);
                match result {
                    Ok(()) => self.set_status_error(None, cx),
                    Err(error) => self.set_status_error(Some(format!("Text: {error}")), cx),
                }
            }
            // The oracle's range inputs take pointer input only here.
            SliderTarget::BrushMenuSize
            | SliderTarget::BrushMenuHardness
            | SliderTarget::BrushMenuStrength
            | SliderTarget::ExportQuality
            | SliderTarget::AdjustmentBrightness
            | SliderTarget::AdjustmentContrast
            | SliderTarget::AdjustmentSaturation
            | SliderTarget::PrintScale => return false,
        }
        true
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
                // `PrintInspector.tsx`: type="range" min 10 max 400.
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
        }
    }
}
