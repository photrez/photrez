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
                if self.state.read(cx).open_select.is_some() {
                    self.select_arrow(if $dy > 0.0 { 1 } else { -1 }, cx);
                    return;
                }
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
    /// Crop bar `Delete`: discard cropped-away pixels on Apply.
    pub(crate) fn toggle_crop_delete(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.crop_delete_pixels = !state.crop_delete_pixels;
        });
        cx.notify();
    }

    /// Crop bar `Fill BG`: bake the background colour under the crop on Apply.
    pub(crate) fn toggle_crop_fill(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.crop_fill_background = !state.crop_fill_background;
        });
        cx.notify();
    }

    pub(crate) fn apply_crop_action(&mut self, _: &ApplyCropAction, cx: &mut Context<Self>) {
        if self.state.read(cx).open_select.is_some() {
            self.select_confirm(cx);
            return;
        }
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
        let (delete_pixels, fill_background) = {
            let state = self.state.read(cx);
            (
                state.crop_delete_pixels,
                if state.crop_fill_background {
                    Some(state.bg_color)
                } else {
                    None
                },
            )
        };
        #[cfg(test)]
        self.state.update(cx, |state, _| {
            state.last_crop_flags = Some((delete_pixels, fill_background));
        });
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
                delete_cropped_pixels: delete_pixels,
                fill_background,
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
        // Applying the crop refits the viewport and leaves no active selection.
        if let Some(document) = self.active_document.as_mut() {
            document.camera = None;
        }
        cx.notify();
        Ok(())
    }

    /// Crop-tool Cancel: discard the frame and its step history.
    pub(crate) fn cancel_crop(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.crop_rect = None;
            state.crop_history.clear();
            state.crop_redo.clear();
            state.selection_drag = None;
        });
        cx.notify();
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
    /// (step 1, Shift step 10). Other tools ignore the keys.
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

impl App {
    /// Crop bar ratio trigger: the list expands inline, like the language list
    /// (a deferred popup inside a canvas overlay never receives hits).
    pub(crate) fn toggle_crop_ratio(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.crop_ratio_open = !state.crop_ratio_open;
        });
        cx.notify();
    }

    /// `handleFreeClick`: the frame stops following a ratio; its rect stays.
    pub(crate) fn crop_choose_free(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.crop_mode = crate::crop_aspect::CropMode::Free;
            state.crop_aspect = None;
            state.crop_ratio_open = false;
        });
        cx.notify();
    }

    /// A preset, a recent, or `Lock Current Shape`: refit the pending frame to
    /// this aspect inside the document and remember it in the recents.
    pub(crate) fn crop_choose_ratio(&mut self, aspect: (u32, u32), cx: &mut Context<Self>) {
        if aspect.0 == 0 || aspect.1 == 0 {
            return;
        }
        let fitted = self.active_document_size().ok().map(|(doc_w, doc_h)| {
            crate::crop_aspect::fit_crop_rect_to_aspect(aspect, doc_w, doc_h, 0.0)
        });
        self.state.update(cx, |state, _| {
            state.crop_mode = crate::crop_aspect::CropMode::Ratio;
            state.crop_aspect = Some(aspect);
            state.crop_ratio_open = false;
            push_recent(&mut state.recent_crop_ratios, aspect);
            if let Some(rect) = fitted {
                if let Some(previous) = state.crop_rect.replace(rect) {
                    state.crop_history.push(previous);
                }
                state.crop_redo.clear();
            }
        });
        cx.notify();
    }

    /// `handleSizeModeClick`: the Size fields drive a pixel target.
    pub(crate) fn crop_choose_size(&mut self, cx: &mut Context<Self>) {
        let (target, unit) = {
            let state = self.state.read(cx);
            (
                state.crop_size_target.unwrap_or((800, 600)),
                state.crop_size_unit,
            )
        };
        let fitted = self.active_document_size().ok().map(|(doc_w, doc_h)| {
            crate::crop_aspect::fit_crop_rect_to_aspect(target, doc_w, doc_h, 0.0)
        });
        self.state.update(cx, |state, _| {
            state.crop_mode = crate::crop_aspect::CropMode::Size;
            state.crop_size_target = Some(target);
            state.crop_size_w = format_unit(target.0, unit);
            state.crop_size_h = format_unit(target.1, unit);
            state.crop_ratio_open = false;
            if let Some(rect) = fitted {
                if let Some(previous) = state.crop_rect.replace(rect) {
                    state.crop_history.push(previous);
                }
                state.crop_redo.clear();
            }
        });
        cx.notify();
    }

    /// `onLockShape`: keep the pending frame's own proportions as a custom ratio.
    pub(crate) fn crop_lock_shape(&mut self, cx: &mut Context<Self>) {
        let (rect, mode) = {
            let state = self.state.read(cx);
            (state.crop_rect, state.crop_mode)
        };
        if mode != crate::crop_aspect::CropMode::Free {
            return;
        }
        let Some(rect) = rect else { return };
        let w = rect[2].round() as u32;
        let h = rect[3].round() as u32;
        if w == 0 || h == 0 {
            return;
        }
        self.crop_choose_ratio((w, h), cx);
    }

    /// The Size units re-format the display buffers from the pixel target; the
    /// pixel value stays authoritative.
    pub(crate) fn crop_set_size_unit(
        &mut self,
        unit: crate::crop_aspect::CropSizeUnit,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| {
            state.crop_size_unit = unit;
            if let Some((w, h)) = state.crop_size_target {
                state.crop_size_w = format_unit(w, unit);
                state.crop_size_h = format_unit(h, unit);
            }
        });
        cx.notify();
    }

    /// Commit one Size field: `fromUnit` at 300 PPI, then refit the frame.
    pub(crate) fn crop_set_size_value(&mut self, field: usize, text: &str, cx: &mut Context<Self>) {
        let parsed: f64 = match text.trim().parse() {
            Ok(value) => value,
            Err(_) => return,
        };
        if !parsed.is_finite() || parsed <= 0.0 {
            return;
        }
        let (unit, mut target) = {
            let state = self.state.read(cx);
            (
                state.crop_size_unit,
                state.crop_size_target.unwrap_or((800, 600)),
            )
        };
        let px = unit.to_pixels(parsed).round().max(1.0) as u32;
        if field == 0 {
            target.0 = px;
        } else {
            target.1 = px;
        }
        let fitted = self.active_document_size().ok().map(|(doc_w, doc_h)| {
            crate::crop_aspect::fit_crop_rect_to_aspect(target, doc_w, doc_h, 0.0)
        });
        self.state.update(cx, |state, _| {
            state.crop_size_target = Some(target);
            state.crop_size_field = field.min(1);
            if field == 0 {
                state.crop_size_w = text.trim().to_string();
            } else {
                state.crop_size_h = text.trim().to_string();
            }
            if let Some(rect) = fitted {
                if let Some(previous) = state.crop_rect.replace(rect) {
                    state.crop_history.push(previous);
                }
                state.crop_redo.clear();
            }
        });
        cx.notify();
    }

    /// `handleSwap` in Size mode: exchange the two target dimensions.
    pub(crate) fn crop_swap_size(&mut self, cx: &mut Context<Self>) {
        let (mut target, w_text, h_text) = {
            let state = self.state.read(cx);
            (
                state.crop_size_target.unwrap_or((800, 600)),
                state.crop_size_w.clone(),
                state.crop_size_h.clone(),
            )
        };
        target = (target.1, target.0);
        let fitted = self.active_document_size().ok().map(|(doc_w, doc_h)| {
            crate::crop_aspect::fit_crop_rect_to_aspect(target, doc_w, doc_h, 0.0)
        });
        self.state.update(cx, |state, _| {
            state.crop_size_target = Some(target);
            state.crop_size_w = h_text;
            state.crop_size_h = w_text;
            if let Some(rect) = fitted {
                if let Some(previous) = state.crop_rect.replace(rect) {
                    state.crop_history.push(previous);
                }
                state.crop_redo.clear();
            }
        });
        cx.notify();
    }
}

impl App {
    /// Start editing one of the Size or Custom fields (click on the field).
    pub(crate) fn crop_begin_edit(&mut self, field: usize, custom: bool, cx: &mut Context<Self>) {
        // The source's EditableNumField selects its text on focus, so the first
        // digit replaces the shown value; the buffer therefore starts empty.
        let _ = (field, custom);
        self.state.update(cx, |state, _| {
            state.crop_edit = Some(String::new());
            state.crop_edit_custom = custom;
            state.crop_size_field = field.min(1);
        });
        cx.notify();
    }

    /// Keys while a Size/Custom buffer is open: digits, one decimal point for
    /// physical units, Backspace, Enter to commit, Escape to cancel.
    pub(crate) fn crop_edit_key(
        &mut self,
        event: &gpui_kit::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        let (unit, custom) = {
            let state = self.state.read(cx);
            (state.crop_size_unit, state.crop_edit_custom)
        };
        let Some(mut buffer) = self.state.read(cx).crop_edit.clone() else {
            return false;
        };
        match key {
            "enter" => {
                self.state.update(cx, |state, _| state.crop_edit = None);
                cx.notify();
                true
            }
            "escape" => {
                self.state.update(cx, |state, _| state.crop_edit = None);
                cx.notify();
                true
            }
            "backspace" => {
                buffer.pop();
                self.state
                    .update(cx, |state, _| state.crop_edit = Some(buffer));
                cx.notify();
                true
            }
            "tab" => {
                let next = 1 - self.state.read(cx).crop_size_field.min(1);
                self.crop_begin_edit(next, custom, cx);
                true
            }
            _ => {
                let text = event
                    .keystroke
                    .key_char
                    .clone()
                    .unwrap_or_else(|| event.keystroke.key.clone());
                let decimal = text == "." && (custom || unit.accepts_decimal());
                if !text.chars().all(|c| c.is_ascii_digit()) && !decimal {
                    return true;
                }
                if decimal && buffer.contains('.') {
                    return true;
                }
                buffer.push_str(&text);
                self.state
                    .update(cx, |state, _| state.crop_edit = Some(buffer.clone()));
                let field = self.state.read(cx).crop_size_field;
                if custom {
                    self.crop_set_custom_value(field, &buffer, cx);
                } else {
                    self.crop_set_size_value(field, &buffer, cx);
                }
                cx.notify();
                true
            }
        }
    }

    /// Commit one Custom W/H value: it becomes the ratio and the frame refits.
    pub(crate) fn crop_set_custom_value(
        &mut self,
        field: usize,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let Ok(value) = text.trim().parse::<u32>() else {
            return;
        };
        if value == 0 {
            return;
        }
        let current = self.state.read(cx).crop_aspect.unwrap_or((1, 1));
        let aspect = if field == 0 {
            (value, current.1)
        } else {
            (current.0, value)
        };
        self.crop_choose_ratio(aspect, cx);
    }
}

impl App {
    /// `Custom...` row: stay in Ratio mode and open the W:H field pair.
    pub(crate) fn crop_choose_custom(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.crop_mode = crate::crop_aspect::CropMode::Ratio;
            state.crop_aspect = state.crop_aspect.or(Some((1, 1)));
            state.crop_ratio_open = false;
            state.crop_edit = Some(
                state
                    .crop_aspect
                    .map(|(w, _)| w.to_string())
                    .unwrap_or_else(|| "1".to_string()),
            );
            state.crop_edit_custom = true;
            state.crop_size_field = 0;
        });
        cx.notify();
    }

    pub(crate) fn toggle_crop_unit(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.crop_unit_open = !state.crop_unit_open;
        });
        cx.notify();
    }
}

/// `pushRecentRatio`: newest first, de-duplicated, at most three.
fn push_recent(recents: &mut Vec<(u32, u32)>, aspect: (u32, u32)) {
    recents.retain(|existing| *existing != aspect);
    recents.insert(0, aspect);
    recents.truncate(3);
}

fn format_unit(px: u32, unit: crate::crop_aspect::CropSizeUnit) -> String {
    let value = unit.to_unit(f64::from(px));
    if unit.accepts_decimal() {
        format!("{value:.2}")
    } else {
        format!("{}", value.round() as i64)
    }
}
