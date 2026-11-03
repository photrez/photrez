//! Rotating a live selection: capture, update, finish and cancel.

use super::*;

impl App {
    /// Start a selection rotation when the press lands on the rotate pin.
    pub(super) fn selection_rotate_start(
        &mut self,
        bounds: &Rc<Cell<Option<Bounds<Pixels>>>>,
        doc_size: (u32, u32),
        point: (f64, f64),
        cx: &mut Context<Self>,
    ) -> bool {
        if self.state.read(cx).active_tool != ToolId::Selection || !self.selection_edit_mode {
            return false;
        }
        let Some(selection) = self.committed_selection() else {
            return false;
        };
        if selection.inverted.unwrap_or(false) {
            return false;
        }
        let scale = display_scale(bounds, doc_size, self.active_camera());
        if !scale.is_finite() || scale <= 0.0 {
            return false;
        }
        let (top_mid, normal) = crate::app::transform_geometry::selection_pin(
            selection.x,
            selection.y,
            selection.width,
            selection.height,
            selection.angle,
        );
        let pin = (
            top_mid.x + normal.x * 24.0 / scale,
            top_mid.y + normal.y * 24.0 / scale,
        );
        let (dx, dy) = (point.0 - pin.0, point.1 - pin.1);
        if (dx * dx + dy * dy).sqrt() > 12.0 / scale {
            return false;
        }
        let center = crate::app::transform_geometry::Point {
            x: selection.x + selection.width / 2.0,
            y: selection.y + selection.height / 2.0,
        };
        let reference = crate::app::transform_geometry::angle_about(
            center,
            crate::app::transform_geometry::Point {
                x: point.0,
                y: point.1,
            },
        );
        self.selection_rotation_drag = Some(SelectionRotationDrag {
            selection: selection.clone(),
            start_angle: selection.angle,
            reference_angle: reference,
            preview_angle: selection.angle,
        });
        cx.notify();
        true
    }

    pub(super) fn selection_rotate_update(
        &mut self,
        point: (f64, f64),
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(mut drag) = self.selection_rotation_drag.clone() else {
            return;
        };
        let center = crate::app::transform_geometry::Point {
            x: drag.selection.x + drag.selection.width / 2.0,
            y: drag.selection.y + drag.selection.height / 2.0,
        };
        let current = crate::app::transform_geometry::angle_about(
            center,
            crate::app::transform_geometry::Point {
                x: point.0,
                y: point.1,
            },
        );
        let reference = drag.reference_angle;
        let mut angle = drag.start_angle + (current - reference);
        if shift {
            angle = (angle / 15.0).round() * 15.0;
        }
        drag.preview_angle = crate::app::transform_geometry::normalize_rotation(angle);
        self.selection_rotation_drag = Some(drag);
        cx.notify();
    }

    pub(super) fn selection_rotate_finish(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.selection_rotation_drag.take() else {
            return;
        };
        let selection = SelectionState {
            angle: drag.preview_angle,
            ..drag.selection.clone()
        };
        let result = self.active_doc_id().and_then(|doc_id| {
            self.apply_core_command(&doc_id, Command::SetSelection { selection }, false, cx)
        });
        if let Err(error) = result {
            self.set_status_error(Some(error), cx);
        }
        cx.notify();
    }

    pub(super) fn cancel_selection_rotation(&mut self, cx: &mut Context<Self>) {
        if self.selection_rotation_drag.take().is_some() {
            cx.notify();
        }
    }
}
