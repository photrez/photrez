//! Pixel selection commands and the app-local selection clipboard.
//!
//! The core owns the selection rectangle/ellipse and the canonical pixels.
//! These actions only read/write through core commands and the pixel store;
//! they never keep a second document state. The clipboard is the frozen app's
//! in-memory image buffer, not the OS clipboard.

use gpui_kit::Context;
use photrez_core::protocol::{Command, CommandEnvelope, CONTRACT_VERSION};
use photrez_core::ptz::content::{LayerType, SelectionShape, SelectionState};

use super::input::next_layer_id;
use super::App;

#[derive(Clone, Debug)]
pub(crate) struct ClipboardImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct SelectAllAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct DeselectAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct InvertSelectionAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct CopySelectionAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct CutSelectionAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct PasteSelectionAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct DeleteSelectionAction;

impl App {
    pub(crate) fn select_all_action(&mut self, _: &SelectAllAction, cx: &mut Context<Self>) {
        let result = self.select_all(cx);
        self.run_selection_action(result, cx);
    }

    pub(crate) fn deselect_action(&mut self, _: &DeselectAction, cx: &mut Context<Self>) {
        let result = self.deselect(cx);
        self.run_selection_action(result, cx);
    }

    pub(crate) fn invert_selection_action(
        &mut self,
        _: &InvertSelectionAction,
        cx: &mut Context<Self>,
    ) {
        let result = self.invert_selection(cx);
        self.run_selection_action(result, cx);
    }

    pub(crate) fn copy_selection_action(
        &mut self,
        _: &CopySelectionAction,
        cx: &mut Context<Self>,
    ) {
        let result = self.copy_selection(cx);
        self.run_selection_action(result, cx);
    }

    pub(crate) fn cut_selection_action(&mut self, _: &CutSelectionAction, cx: &mut Context<Self>) {
        let result = self.cut_selection(cx);
        self.run_selection_action(result, cx);
    }

    pub(crate) fn paste_selection_action(
        &mut self,
        _: &PasteSelectionAction,
        cx: &mut Context<Self>,
    ) {
        let result = self.paste_selection(cx);
        self.run_selection_action(result, cx);
    }

    pub(crate) fn delete_selection_action(
        &mut self,
        _: &DeleteSelectionAction,
        cx: &mut Context<Self>,
    ) {
        let result = self.delete_selection(cx);
        self.run_selection_action(result, cx);
    }

    fn run_selection_action(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(error), cx),
        }
        cx.notify();
    }

    pub(crate) fn select_all(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        self.apply_selection_command(Command::SelectAll, cx)
    }

    pub(crate) fn deselect(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        self.apply_selection_command(Command::ClearSelection, cx)
    }

    pub(crate) fn invert_selection(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        self.apply_selection_command(Command::InvertSelection, cx)
    }

    fn apply_selection_command(
        &mut self,
        command: Command,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc_id = self.active_doc_id()?;
        let envelope = CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command,
        };
        self.sessions
            .get_mut(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document_mut()
            .registry
            .apply_command(&doc_id, envelope)
            .map_err(|error| format!("{}: {}", error.code, error.message))?;
        cx.notify();
        Ok(())
    }

    fn selection_state(&self) -> Result<(String, SelectionState), String> {
        let doc_id = self.active_doc_id()?;
        let content = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .ok_or_else(|| "loaded content is missing".to_string())?;
        let selection = content
            .selection
            .clone()
            .ok_or_else(|| "there is no active selection".to_string())?;
        if selection.angle.abs() > f64::EPSILON {
            return Err("rotated selection copy is not implemented yet".to_string());
        }
        Ok((doc_id, selection))
    }

    fn selection_bounds(
        &self,
        doc_id: &str,
        selection: &SelectionState,
    ) -> Result<(u32, u32, u32, u32), String> {
        let content = self
            .sessions
            .get(doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .ok_or_else(|| "loaded content is missing".to_string())?;
        let doc_w = content.width.max(1.0);
        let doc_h = content.height.max(1.0);
        let (x, y, w, h) = if selection.inverted.unwrap_or(false) {
            (0.0, 0.0, doc_w, doc_h)
        } else {
            (selection.x, selection.y, selection.width, selection.height)
        };
        let x0 = x.floor().clamp(0.0, doc_w - 1.0) as u32;
        let y0 = y.floor().clamp(0.0, doc_h - 1.0) as u32;
        let x1 = (x + w).ceil().clamp(1.0, doc_w) as u32;
        let y1 = (y + h).ceil().clamp(1.0, doc_h) as u32;
        if x1 <= x0 || y1 <= y0 {
            return Err("the selection is empty".to_string());
        }
        Ok((x0, y0, x1, y1))
    }

    fn selection_contains(selection: &SelectionState, x: f64, y: f64) -> bool {
        let inside_box = x >= selection.x
            && y >= selection.y
            && x < selection.x + selection.width
            && y < selection.y + selection.height;
        let inside_shape = match selection.shape {
            Some(SelectionShape::Ellipse) => {
                let rx = (selection.width / 2.0).max(f64::EPSILON);
                let ry = (selection.height / 2.0).max(f64::EPSILON);
                let nx = (x - (selection.x + rx)) / rx;
                let ny = (y - (selection.y + ry)) / ry;
                nx * nx + ny * ny <= 1.0
            }
            _ => true,
        };
        let inside = inside_box && inside_shape;
        if selection.inverted.unwrap_or(false) {
            !inside
        } else {
            inside
        }
    }

    /// Copy the selected pixels into the app-local clipboard.
    pub(crate) fn copy_selection(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let (doc_id, selection) = self.selection_state()?;
        let (x0, y0, x1, y1) = self.selection_bounds(&doc_id, &selection)?;
        let (_, layer) = self.selected_paint_layer(true, cx)?;
        let canonical = self
            .sessions
            .get(&doc_id)
            .and_then(|session| {
                session
                    .loaded_document()
                    .registry
                    .get_layer(&doc_id, &layer.id)
            })
            .ok_or_else(|| "the selected layer has no pixel store".to_string())?;
        let doc_w = canonical.width as usize;
        let doc_h = canonical.height as usize;
        let crop_w = (x1 - x0) as usize;
        let crop_h = (y1 - y0) as usize;
        let mut crop = vec![0u8; crop_w * crop_h * 4];
        let mut any = false;
        for row in 0..crop_h {
            for col in 0..crop_w {
                let dx = x0 as usize + col;
                let dy = y0 as usize + row;
                if dx >= doc_w || dy >= doc_h {
                    continue;
                }
                let px = dx as f64 + 0.5;
                let py = dy as f64 + 0.5;
                if !Self::selection_contains(&selection, px, py) {
                    continue;
                }
                let src = (dy * doc_w + dx) * 4;
                let dst = (row * crop_w + col) * 4;
                crop[dst..dst + 4].copy_from_slice(&canonical.pixels[src..src + 4]);
                if canonical.pixels[src + 3] != 0 {
                    any = true;
                }
            }
        }
        if !any {
            return Err("the selection contains no pixels".to_string());
        }
        let (clip_w, clip_h, pixels) = trim_transparent(crop_w, crop_h, &crop);
        self.clipboard = Some(ClipboardImage {
            width: clip_w,
            height: clip_h,
            pixels,
        });
        Ok(())
    }

    /// Cut the selected pixels: copy, then clear them in one history entry.
    pub(crate) fn cut_selection(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        self.copy_selection(cx)?;
        self.clear_selection_pixels(true, cx)
    }

    /// Delete the selected pixels in one history entry.
    pub(crate) fn delete_selection(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        self.clear_selection_pixels(false, cx)
    }

    fn clear_selection_pixels(&mut self, cut: bool, cx: &mut Context<Self>) -> Result<(), String> {
        let (doc_id, selection) = self.selection_state()?;
        let (x0, y0, x1, y1) = self.selection_bounds(&doc_id, &selection)?;
        let (_, layer) = self.selected_paint_layer(false, cx)?;
        let canonical = self
            .sessions
            .get(&doc_id)
            .and_then(|session| {
                session
                    .loaded_document()
                    .registry
                    .get_layer(&doc_id, &layer.id)
            })
            .ok_or_else(|| "the selected layer has no pixel store".to_string())?;
        let width = canonical.width as usize;
        let height = canonical.height as usize;
        let mut pixels = canonical.pixels.clone();
        let mut changed = false;
        for row in y0..y1 {
            for col in x0..x1 {
                let px = (col as usize, row as usize);
                let cx_doc = px.0 as f64 + 0.5;
                let cy_doc = px.1 as f64 + 0.5;
                if !Self::selection_contains(&selection, cx_doc, cy_doc) {
                    continue;
                }
                let index = (px.1 * width + px.0) * 4;
                if pixels[index..index + 4] != [0, 0, 0, 0] {
                    pixels[index..index + 4].copy_from_slice(&[0, 0, 0, 0]);
                    changed = true;
                }
            }
        }
        if !changed {
            return Err("the selection contains no opaque pixels".to_string());
        }
        let written = self
            .sessions
            .get_mut(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document_mut()
            .registry
            .write_region(&doc_id, &layer.id, 0, 0, width, height, pixels)
            .ok_or_else(|| "pixel region write was refused".to_string())?;
        let _ = written;
        self.sessions
            .mark_dirty(&doc_id)
            .map_err(|error| format!("dirty state failed: {error:?}"))?;
        if let Some(session) = self.sessions.get_mut(&doc_id) {
            session
                .loaded_document_mut()
                .registry
                .set_history_label(&doc_id, if cut { "Cut" } else { "Delete Selection" });
        }
        self.refresh_active_canvas(cx)?;
        Ok(())
    }

    /// Paste the clipboard as a new raster layer at the selection origin.
    pub(crate) fn paste_selection(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(clip) = self.clipboard.clone() else {
            return Err("the clipboard is empty".to_string());
        };
        let doc_id = self.active_doc_id()?;
        let (doc_w, doc_h) = self.active_document_size()?;
        let (origin_x, origin_y) = self
            .selection_state()
            .map(|(_, selection)| (selection.x, selection.y))
            .unwrap_or((0.0, 0.0));
        let max_x = (doc_w - clip.width as f64).max(0.0);
        let max_y = (doc_h - clip.height as f64).max(0.0);
        let x = origin_x.clamp(0.0, max_x);
        let y = origin_y.clamp(0.0, max_y);
        let new_id = next_layer_id();
        self.sessions
            .get_mut(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document_mut()
            .registry
            .add_layer(&doc_id, &new_id, clip.width, clip.height, clip.pixels)
            .map_err(|error| format!("cannot seed the pasted layer: {error}"))?;
        let envelope = CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::AddLayer {
                id: new_id.clone(),
                name: "Pasted Layer".to_string(),
                width: clip.width as f64,
                height: clip.height as f64,
                index: 0,
                x: Some(x),
                y: Some(y),
                layer_type: Some(LayerType::Raster),
                shape_params: None,
                text_data: None,
            },
        };
        let applied = self
            .sessions
            .get_mut(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document_mut()
            .registry
            .apply_command(&doc_id, envelope);
        if let Err(error) = applied {
            let _ = self.sessions.get_mut(&doc_id).map(|session| {
                session
                    .loaded_document_mut()
                    .registry
                    .remove_layer(&doc_id, &new_id)
            });
            return Err(format!("{}: {}", error.code, error.message));
        }
        self.sessions
            .mark_dirty(&doc_id)
            .map_err(|error| format!("dirty state failed: {error:?}"))?;
        if let Some(session) = self.sessions.get_mut(&doc_id) {
            session
                .loaded_document_mut()
                .registry
                .set_history_label(&doc_id, "Paste");
        }
        self.project_layers(cx);
        self.state.update(cx, |state, _| {
            if let Some(row) = state
                .layers
                .iter()
                .find(|row| row.core_id.as_deref() == Some(new_id.as_str()))
            {
                state.selected_layer = row.id;
            }
        });
        self.refresh_active_canvas(cx)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_core_selection_for_test(
        &mut self,
        selection: SelectionState,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.apply_selection_command(Command::SetSelection { selection }, cx)
    }

    #[cfg(test)]
    pub(crate) fn clipboard_for_test(&self) -> Option<(u32, u32, Vec<u8>)> {
        self.clipboard
            .as_ref()
            .map(|clip| (clip.width, clip.height, clip.pixels.clone()))
    }
}

/// Crop transparent padding from a straight-RGBA crop. The frozen selection
/// copy trims to the non-transparent bounds before placing the clipboard.
fn trim_transparent(width: usize, height: usize, pixels: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut found = false;
    for y in 0..height {
        for x in 0..width {
            if pixels[(y * width + x) * 4 + 3] != 0 {
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if !found {
        return (1, 1, vec![0; 4]);
    }
    let crop_w = max_x - min_x + 1;
    let crop_h = max_y - min_y + 1;
    let mut out = vec![0u8; crop_w * crop_h * 4];
    for y in 0..crop_h {
        let src = ((min_y + y) * width + min_x) * 4;
        let dst = y * crop_w * 4;
        out[dst..dst + crop_w * 4].copy_from_slice(&pixels[src..src + crop_w * 4]);
    }
    (crop_w as u32, crop_h as u32, out)
}
