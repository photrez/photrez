//! The brush/eraser right-click menu.
//!
//! The menu only answers while the brush or eraser is active; this module
//! routes the same right-click through the canvas pointer path and stores the
//! anchor in `AppState::brush_menu`. The panel reads and writes the same
//! brush/eraser settings the option bar owns; nothing here belongs in core.

use gpui_kit::Context;

use crate::state::ToolId;

use super::App;

/// Maps a paint size in pixels to the menu's 0..100 slider position.
pub(crate) fn paint_size_to_slider(size: f64) -> f64 {
    let clamped = size.clamp(1.0, 2000.0);
    if clamped <= 500.0 {
        return ((clamped - 1.0) / 499.0 * 75.0).round();
    }
    let t = ((clamped - 500.0) / 1500.0).sqrt();
    (75.0 + t * 25.0).round()
}

/// Inverse of [`paint_size_to_slider`]: slider position back to pixels.
pub(crate) fn slider_to_paint_size(slider: f64) -> f64 {
    let clamped = slider.clamp(0.0, 100.0);
    if clamped <= 75.0 {
        return (1.0 + 499.0 * (clamped / 75.0)).round();
    }
    let t = (clamped - 75.0) / 25.0;
    (500.0 + 1500.0 * t * t).round()
}

impl App {
    /// The anchor is the pointer; the view clamps it to
    /// the window, leaving 220 px of width and 260 px of height clear.
    pub(crate) fn open_brush_menu(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.context_menu = None;
            state.brush_menu = Some((x, y));
        });
        cx.notify();
    }

    pub(crate) fn close_brush_menu(&mut self, cx: &mut Context<Self>) {
        if self.state.read(cx).brush_menu.is_none() {
            return;
        }
        self.state.update(cx, |state, _| state.brush_menu = None);
        cx.notify();
    }

    /// The menu writes the active tool's settings and clears the preset id,
    /// the same way the size, hardness and opacity controls do.
    pub(crate) fn brush_menu_set_from_fraction(
        &mut self,
        slot: crate::state::SliderTarget,
        fraction: f64,
        cx: &mut Context<Self>,
    ) {
        let fraction = fraction.clamp(0.0, 1.0);
        self.state.update(cx, |state, _| {
            let eraser = state.active_tool == ToolId::Eraser;
            state.brush_preset_index = usize::MAX;
            match slot {
                crate::state::SliderTarget::BrushMenuSize => {
                    let size = slider_to_paint_size(fraction * 100.0);
                    if eraser {
                        state.eraser_size = size;
                    } else {
                        state.brush_size = size;
                    }
                }
                crate::state::SliderTarget::BrushMenuHardness => {
                    if eraser {
                        state.eraser_hardness = fraction;
                    } else {
                        state.brush_hardness = fraction;
                    }
                }
                crate::state::SliderTarget::BrushMenuStrength => {
                    if eraser {
                        state.eraser_opacity = fraction;
                    } else {
                        state.brush_opacity = fraction;
                    }
                }
                _ => {}
            }
        });
        cx.notify();
    }

    /// `applyPreset`: writes all five values, records the preset id and closes
    /// the panel.
    pub(crate) fn brush_menu_apply_preset(&mut self, index: usize, cx: &mut Context<Self>) {
        let eraser = self.state.read(cx).active_tool == ToolId::Eraser;
        let Some(&(_, _, _, _, _, eraser_only)) = super::tool_settings::BRUSH_PRESETS.get(index)
        else {
            return;
        };
        if eraser_only && !eraser {
            return;
        }
        self.apply_brush_preset(index, cx);
        self.state.update(cx, |state, _| state.brush_menu = None);
        cx.notify();
    }

    /// Reset: tool defaults with flow 1, smoothing 0 and no
    /// preset, then close.
    pub(crate) fn brush_menu_reset(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.brush_preset_index = usize::MAX;
            if state.active_tool == ToolId::Eraser {
                state.eraser_size = 32.0;
                state.eraser_hardness = 1.0;
                state.eraser_opacity = 1.0;
                state.eraser_flow = 1.0;
                state.eraser_smoothing = 0.0;
            } else {
                state.brush_size = 20.0;
                state.brush_hardness = 0.8;
                state.brush_opacity = 1.0;
                state.brush_flow = 1.0;
                state.brush_smoothing = 0.0;
            }
            state.brush_menu = None;
        });
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn brush_menu_for_test(&self, cx: &gpui_kit::App) -> Option<(f32, f32)> {
        self.state.read(cx).brush_menu
    }
}
