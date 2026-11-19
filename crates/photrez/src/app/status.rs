//! The status sentence: the tool hint, unless a live interaction overrides it.
//!
//! The reference footer shows the tool's hint at rest, but swaps it for a
//! paint-block reason, a transform-session hint or the live gradient vector
//! while one of those is in flight. This module owns that choice so the
//! status bar view stays a pure renderer.

use crate::i18n::{t, t_args};
use crate::state::ToolId;

use super::App;

/// The catalog key for a tool's resting hint.
pub(crate) fn tip_key(tool: ToolId) -> &'static str {
    match tool {
        ToolId::Move => "status.toolTips.move",
        ToolId::Selection => "status.toolTips.selection",
        ToolId::Crop => "status.toolTips.crop",
        ToolId::Eyedropper => "status.toolTips.eyedropper",
        ToolId::Brush => "status.toolTips.brush",
        ToolId::Eraser => "status.toolTips.eraser",
        ToolId::PaintBucket => "status.toolTips.paintBucket",
        ToolId::Gradient => "status.toolTips.gradient",
        ToolId::Shape => "status.toolTips.shape",
        ToolId::Text => "status.toolTips.text",
    }
}

impl App {
    pub(crate) fn status_tip(&self, app: &gpui_kit::App) -> String {
        let state = self.state.read(app);
        let tool = state.active_tool;

        if let Some(gesture) = self
            .pending_gesture
            .as_ref()
            .filter(|gesture| gesture.tool == ToolId::Gradient)
        {
            let dx = gesture.last.0 - gesture.start.0;
            let dy = gesture.last.1 - gesture.start.1;
            let distance = dx.hypot(dy).round() as i64;
            let angle = ((dy.atan2(dx) * 180.0 / std::f64::consts::PI + 360.0) % 360.0) * 10.0;
            let angle = (angle.round() / 10.0).to_string();
            let distance = distance.to_string();
            return t_args(
                "status.gradientVector",
                &[("distance", &distance), ("angle", &angle)],
            );
        }
        if self.transform_drag.is_some() || self.group_transform_drag.is_some() {
            return t("status.toolTips.transforming");
        }

        if matches!(tool, ToolId::Brush | ToolId::Eraser) {
            let state = self.state.read(app);
            let flags = state.layer_flags.get(state.selected_layer as usize);
            let Some(flags) = flags else {
                return t("status.toolTips.noActiveLayer");
            };
            if flags.locked {
                return t("status.toolTips.layerLocked");
            }
            if !flags.visible {
                return t("status.toolTips.layerHidden");
            }
            if tool == ToolId::Eraser && flags.lock_transparency {
                return t("status.toolTips.transparencyProtected");
            }
        }

        t(tip_key(tool))
    }
}
