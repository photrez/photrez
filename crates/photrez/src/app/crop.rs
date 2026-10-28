//! Crop apply/cancel over the pending frame stored in `AppState.crop_rect`.
//!
//! The drag preview is UI state; the crop itself is the core `ApplyCrop`
//! command (non-destructive: layers are recentered and the document resizes in
//! one history entry). Apply then refits the viewport and returns to Move,
//! matching `applyCropPreview`.

use gpui_kit::Context;
use photrez_core::protocol::Command;

use crate::state::ToolId;

use super::App;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CropHandle {
    Nw,
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CropDrag {
    pub handle: Option<CropHandle>,
    pub start_rect: [f64; 4],
    pub start_doc: (f64, f64),
}

/// Screen-space tolerance for grabbing a crop edge or corner.
const HANDLE_TOLERANCE_PX: f64 = 8.0;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct ApplyCropAction;

macro_rules! nudge_action {
    ($name:ident, $method:ident, $dx:expr, $dy:expr) => {
        #[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
        #[action(namespace = photrez, no_json)]
        pub(crate) struct $name;

        impl App {
            pub(crate) fn $method(&mut self, _: &$name, cx: &mut Context<Self>) {
                self.nudge_crop($dx, $dy, cx);
            }
        }
    };
}

nudge_action!(NudgeCropLeft, nudge_crop_left, -1.0, 0.0);
nudge_action!(NudgeCropRight, nudge_crop_right, 1.0, 0.0);
nudge_action!(NudgeCropUp, nudge_crop_up, 0.0, -1.0);
nudge_action!(NudgeCropDown, nudge_crop_down, 0.0, 1.0);
nudge_action!(NudgeCropLeftLarge, nudge_crop_left_large, -10.0, 0.0);
nudge_action!(NudgeCropRightLarge, nudge_crop_right_large, 10.0, 0.0);
nudge_action!(NudgeCropUpLarge, nudge_crop_up_large, 0.0, -10.0);
nudge_action!(NudgeCropDownLarge, nudge_crop_down_large, 0.0, 10.0);

impl App {
    pub(crate) fn apply_crop_action(&mut self, _: &ApplyCropAction, cx: &mut Context<Self>) {
        let result = self.apply_crop(cx);
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(format!("Crop: {error}")), cx),
        }
    }

    pub(crate) fn apply_crop(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let tool = self.state.read(cx).active_tool;
        if tool != ToolId::Crop {
            return Err("the crop tool is not active".to_string());
        }
        let rect = self
            .state
            .read(cx)
            .crop_rect
            .ok_or_else(|| "there is no crop rectangle".to_string())?;
        let (doc_w, doc_h) = self.active_document_size()?;
        let x = rect[0].round().clamp(0., (doc_w - 1.).max(0.));
        let y = rect[1].round().clamp(0., (doc_h - 1.).max(0.));
        let width = rect[2].round().max(1.).min(doc_w - x);
        let height = rect[3].round().max(1.).min(doc_h - y);
        if width < 1. || height < 1. {
            return Err("the crop rectangle is empty".to_string());
        }
        let doc_id = self.active_doc_id()?;
        self.apply_core_command(
            &doc_id,
            Command::ApplyCrop {
                x,
                y,
                width,
                height,
                rotation: None,
                target_width: None,
                target_height: None,
            },
            true,
            cx,
        )?;
        self.state.update(cx, |state, _| {
            state.crop_rect = None;
            state.crop_history.clear();
            state.crop_redo.clear();
            state.selection_drag = None;
            state.active_tool = ToolId::Move;
        });
        // The frozen apply refits the viewport and leaves no active selection.
        if let Some(document) = self.active_document.as_mut() {
            document.camera = None;
        }
        cx.notify();
        Ok(())
    }

    /// Crop-tool Ctrl+Z: restore the previous frame, keeping a redo step.
    pub(crate) fn crop_undo(&mut self, cx: &mut Context<Self>) -> bool {
        let updated = self.state.update(cx, |state, _| {
            let Some(previous) = state.crop_history.pop() else {
                return false;
            };
            if let Some(current) = state.crop_rect {
                state.crop_redo.push(current);
            }
            state.crop_rect = Some(previous);
            true
        });
        if updated {
            cx.notify();
        }
        updated
    }

    pub(crate) fn crop_redo(&mut self, cx: &mut Context<Self>) -> bool {
        let updated = self.state.update(cx, |state, _| {
            let Some(next) = state.crop_redo.pop() else {
                return false;
            };
            if let Some(current) = state.crop_rect {
                state.crop_history.push(current);
            }
            state.crop_rect = Some(next);
            true
        });
        if updated {
            cx.notify();
        }
        updated
    }

    /// Which crop handle (or the frame interior) the point hits. `zoom` maps
    /// the screen tolerance into document units.
    pub(crate) fn crop_handle_at(
        doc: (f64, f64),
        rect: [f64; 4],
        zoom: f64,
    ) -> Option<(CropHandle, bool)> {
        let tolerance = HANDLE_TOLERANCE_PX / zoom.max(0.01);
        let [x, y, w, h] = rect;
        let near_left = (doc.0 - x).abs() <= tolerance;
        let near_right = (doc.0 - (x + w)).abs() <= tolerance;
        let near_top = (doc.1 - y).abs() <= tolerance;
        let near_bottom = (doc.1 - (y + h)).abs() <= tolerance;
        let within_x = doc.0 >= x - tolerance && doc.0 <= x + w + tolerance;
        let within_y = doc.1 >= y - tolerance && doc.1 <= y + h + tolerance;
        let handle = match (near_left, near_right, near_top, near_bottom) {
            (true, _, true, _) if within_x && within_y => Some(CropHandle::Nw),
            (_, true, true, _) if within_x && within_y => Some(CropHandle::Ne),
            (true, _, _, true) if within_x && within_y => Some(CropHandle::Sw),
            (_, true, _, true) if within_x && within_y => Some(CropHandle::Se),
            (true, false, false, false) if within_y => Some(CropHandle::W),
            (false, true, false, false) if within_y => Some(CropHandle::E),
            (false, false, true, false) if within_x => Some(CropHandle::N),
            (false, false, false, true) if within_x => Some(CropHandle::S),
            _ => None,
        };
        if let Some(handle) = handle {
            return Some((handle, false));
        }
        if doc.0 > x && doc.0 < x + w && doc.1 > y && doc.1 < y + h {
            return Some((CropHandle::Nw, true));
        }
        None
    }

    /// Apply a live crop move/resize. Returns the clamped rectangle.
    pub(crate) fn crop_drag_rect(
        drag: CropDrag,
        doc: (f64, f64),
        doc_w: f64,
        doc_h: f64,
    ) -> [f64; 4] {
        let [sx, sy, sw, sh] = drag.start_rect;
        let Some(handle) = drag.handle else {
            // No handle grabbed: the whole rect follows the pointer move.
            let dx = doc.0 - drag.start_doc.0;
            let dy = doc.1 - drag.start_doc.1;
            let x = (sx + dx).clamp(0.0, (doc_w - sw).max(0.0));
            let y = (sy + dy).clamp(0.0, (doc_h - sh).max(0.0));
            return [x, y, sw, sh];
        };
        let mut left = sx;
        let mut top = sy;
        let mut right = sx + sw;
        let mut bottom = sy + sh;
        if matches!(handle, CropHandle::Nw | CropHandle::W | CropHandle::Sw) {
            left = (sx + doc.0 - drag.start_doc.0).clamp(0.0, right - 1.0);
        }
        if matches!(handle, CropHandle::Ne | CropHandle::E | CropHandle::Se) {
            right = (sx + sw + doc.0 - drag.start_doc.0).clamp(left + 1.0, doc_w);
        }
        if matches!(handle, CropHandle::Nw | CropHandle::N | CropHandle::Ne) {
            top = (sy + doc.1 - drag.start_doc.1).clamp(0.0, bottom - 1.0);
        }
        if matches!(handle, CropHandle::Sw | CropHandle::S | CropHandle::Se) {
            bottom = (sy + sh + doc.1 - drag.start_doc.1).clamp(top + 1.0, doc_h);
        }
        [left, top, right - left, bottom - top]
    }

    #[cfg(test)]
    pub(crate) fn crop_rect_for_test(&self, cx: &gpui_kit::App) -> Option<[f64; 4]> {
        self.state.read(cx).crop_rect
    }

    /// Arrow keys move the pending crop frame, clamped to the document
    /// (`cropTool.ts` step 1, Shift step 10). Other tools ignore the keys.
    fn nudge_crop(&mut self, dx: f64, dy: f64, cx: &mut Context<Self>) {
        let tool = self.state.read(cx).active_tool;
        if tool == ToolId::Move {
            let result = self.nudge_selected_layer(dx, dy, cx);
            match result {
                Ok(()) => self.set_status_error(None, cx),
                Err(error) => self.set_status_error(Some(format!("Nudge: {error}")), cx),
            }
            return;
        }
        if tool != ToolId::Crop {
            return;
        }
        let Some(rect) = self.state.read(cx).crop_rect else {
            return;
        };
        let Ok((doc_w, doc_h)) = self.active_document_size() else {
            return;
        };
        let max_x = (doc_w - rect[2]).max(0.);
        let max_y = (doc_h - rect[3]).max(0.);
        let next = [
            (rect[0] + dx).clamp(0., max_x),
            (rect[1] + dy).clamp(0., max_y),
            rect[2],
            rect[3],
        ];
        self.state.update(cx, |state, _| {
            state.crop_history.push(rect);
            state.crop_redo.clear();
            state.crop_rect = Some(next);
        });
        cx.notify();
    }
}
