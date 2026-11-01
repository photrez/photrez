//! Pixel selection commands and the app-local selection clipboard.
//!
//! The core owns the selection rectangle/ellipse and the canonical pixels.
//! These actions only read/write through core commands and the pixel store;
//! they never keep a second document state. The clipboard is an in-memory
//! image buffer owned by the app, not the OS clipboard.

use gpui_kit::Context;
use photrez_core::protocol::{Command, CommandEnvelope, CONTRACT_VERSION};
use photrez_core::ptz::content::{LayerType, SelectionShape, SelectionState, Transform2D};

use super::input::next_layer_id;
use super::transform_geometry::{document_to_layer_local, rotated_rect_corners};
use super::App;

#[derive(Clone, Debug)]
pub(crate) struct ClipboardImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// A selection resolved into layer-local pixel space: the rounded box the
/// command iterates plus the float box the ellipse test uses.
#[derive(Debug)]
struct SelectionMask {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    ellipse: bool,
    inverted: bool,
}

impl SelectionMask {
    /// The pixels the command iterates: the layer box for an inverted
    /// selection (whose excluded region is the mask box), the mask box
    /// otherwise.
    fn window(&self, layer_w: u32, layer_h: u32) -> (u32, u32, u32, u32) {
        if self.inverted {
            (0, 0, layer_w, layer_h)
        } else {
            (self.x0, self.y0, self.x1, self.y1)
        }
    }

    fn covers(&self, px: u32, py: u32) -> bool {
        let inside = if self.ellipse {
            let rx = self.width / 2.0;
            let ry = self.height / 2.0;
            if rx <= 0.0 || ry <= 0.0 {
                false
            } else {
                let nx = (px as f64 - (self.left + rx)) / rx;
                let ny = (py as f64 - (self.top + ry)) / ry;
                nx * nx + ny * ny <= 1.0
            }
        } else {
            px >= self.x0 && px < self.x1 && py >= self.y0 && py < self.y1
        };
        inside != self.inverted
    }
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
pub(crate) struct ToggleSelectionEditModeAction;

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

    /// Ctrl+T: toggle the handles/rotate-pin transform
    /// mode. It needs the Selection tool and a committed selection, and it is
    /// cleared by deselect, invert, Escape and a tool switch.
    pub(crate) fn toggle_selection_edit_mode_action(
        &mut self,
        _: &ToggleSelectionEditModeAction,
        cx: &mut Context<Self>,
    ) {
        if self.state.read(cx).active_tool != crate::state::ToolId::Selection
            || !self.pixel_selection_present()
        {
            return;
        }
        self.selection_edit_mode = !self.selection_edit_mode;
        cx.notify();
    }

    /// Cut/copy enablement: a committed pixel
    /// selection and an active layer that owns pixels. Enforced at the command
    /// entry so keyboard shortcuts cannot bypass the menus' disabled rows.
    fn copy_cut_enabled(&self, cx: &Context<Self>) -> bool {
        self.pixel_selection_present() && self.active_layer_has_bitmap(cx)
    }

    pub(crate) fn copy_selection_action(
        &mut self,
        _: &CopySelectionAction,
        cx: &mut Context<Self>,
    ) {
        if !self.copy_cut_enabled(cx) {
            return;
        }
        let result = self.copy_selection(cx);
        self.run_selection_action(result, cx);
    }

    pub(crate) fn cut_selection_action(&mut self, _: &CutSelectionAction, cx: &mut Context<Self>) {
        if !self.copy_cut_enabled(cx) {
            return;
        }
        let result = self.cut_selection(cx);
        self.run_selection_action(result, cx);
    }

    pub(crate) fn paste_selection_action(
        &mut self,
        _: &PasteSelectionAction,
        cx: &mut Context<Self>,
    ) {
        if !self.selection_clipboard_present() {
            return;
        }
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
        self.selection_edit_mode = false;
        self.apply_selection_command(Command::ClearSelection, cx)
    }

    pub(crate) fn invert_selection(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        self.selection_edit_mode = false;
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
        Ok((doc_id, selection))
    }

    /// The selection bar's X/Y/W/H/R fields submit one `SetSelection` that
    /// replaces a single dimension of the committed selection. Width and
    /// height ignore non-positive input, the same `n > 0` guard the fields
    /// use, and
    /// the rewritten selection takes the tool's current marquee shape. The
    /// committed state itself is read through `App::committed_selection`.
    pub(crate) fn set_selection_field(
        &mut self,
        id: &'static str,
        value: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (_, selection) = self.selection_state()?;
        if !value.is_finite() {
            return Ok(());
        }
        let shape = match self.state.read(cx).selection_shape {
            crate::state::SelectionShape::Rect => SelectionShape::Rect,
            crate::state::SelectionShape::Ellipse => SelectionShape::Ellipse,
        };
        let (x, y, width, height) = match id {
            "sel-x" => (value, selection.y, selection.width, selection.height),
            "sel-y" => (selection.x, value, selection.width, selection.height),
            "sel-w" => {
                if value <= 0.0 {
                    return Ok(());
                }
                (selection.x, selection.y, value, selection.height)
            }
            "sel-h" => {
                if value <= 0.0 {
                    return Ok(());
                }
                (selection.x, selection.y, selection.width, value)
            }
            "sel-r" => (selection.x, selection.y, selection.width, selection.height),
            _ => return Ok(()),
        };
        let angle = if id == "sel-r" {
            value
        } else {
            selection.angle
        };
        let next = SelectionState {
            x,
            y,
            width,
            height,
            angle,
            shape: Some(shape),
            inverted: None,
        };
        self.apply_selection_command(Command::SetSelection { selection: next }, cx)
    }

    #[cfg(test)]
    pub(crate) fn selection_rotation_preview_for_test(&self) -> Option<f64> {
        self.selection_rotation_drag
            .as_ref()
            .map(|drag| drag.preview_angle)
    }

    #[cfg(test)]
    pub(crate) fn core_selection_for_test(&self) -> Option<SelectionState> {
        self.committed_selection()
    }

    #[cfg(test)]
    pub(crate) fn selection_edit_mode_for_test(&self) -> bool {
        self.selection_edit_mode
    }

    /// Resolve a document-space selection onto the layer-local pixel grid the
    /// pixel commands write to.
    ///
    /// A rotated selection addresses the axis-aligned bounding box of its
    /// rotated corners (the selection clears the box, not the rotated
    /// quad); an ellipse marquee addresses the ellipse inscribed in the same
    /// box; an inverted selection addresses the layer minus that box. The
    /// mapping runs through the layer transform because the selection is
    /// stored in document space while the bitmap is layer-local.
    fn selection_mask(
        transform: &Transform2D,
        layer_w: f64,
        layer_h: f64,
        width: u32,
        height: u32,
        selection: &SelectionState,
    ) -> Result<SelectionMask, String> {
        let doc_w = width.max(1) as f64;
        let doc_h = height.max(1) as f64;
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let corners = rotated_rect_corners(
            selection.x,
            selection.y,
            selection.width,
            selection.height,
            selection.angle,
        );
        for corner in corners {
            let local = document_to_layer_local(corner.x, corner.y, transform, layer_w, layer_h);
            min_x = min_x.min(local.x);
            min_y = min_y.min(local.y);
            max_x = max_x.max(local.x);
            max_y = max_y.max(local.y);
        }
        if !(min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite())
            || max_x <= 0.0
            || max_y <= 0.0
            || min_x >= doc_w
            || min_y >= doc_h
        {
            return Err("the selection is empty".to_string());
        }
        let x0 = min_x.floor().clamp(0.0, doc_w - 1.0) as u32;
        let y0 = min_y.floor().clamp(0.0, doc_h - 1.0) as u32;
        let x1 = max_x.ceil().clamp(1.0, doc_w) as u32;
        let y1 = max_y.ceil().clamp(1.0, doc_h) as u32;
        if x1 <= x0 || y1 <= y0 {
            return Err("the selection is empty".to_string());
        }
        Ok(SelectionMask {
            x0,
            y0,
            x1,
            y1,
            left: min_x,
            top: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
            ellipse: matches!(selection.shape, Some(SelectionShape::Ellipse)),
            inverted: selection.inverted.unwrap_or(false),
        })
    }

    /// Copy the selected pixels into the app-local clipboard.
    pub(crate) fn copy_selection(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let (doc_id, selection) = self.selection_state()?;
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
        let mask = Self::selection_mask(
            &layer.transform,
            layer.width,
            layer.height,
            canonical.width,
            canonical.height,
            &selection,
        )?;
        let doc_w = canonical.width as usize;
        let doc_h = canonical.height as usize;
        let (x0, y0, x1, y1) = mask.window(canonical.width, canonical.height);
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
                if !mask.covers(dx as u32, dy as u32) {
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
        // Copying trims transparent edges for a plain marquee only:
        // an inverted copy is the whole layer minus the marquee, and an
        // ellipse copy keeps its box so the mask survives the paste.
        let (clip_w, clip_h, pixels) = if mask.inverted || mask.ellipse {
            (crop_w as u32, crop_h as u32, crop)
        } else {
            trim_transparent(crop_w, crop_h, &crop)
        };
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
        let mask = Self::selection_mask(
            &layer.transform,
            layer.width,
            layer.height,
            canonical.width,
            canonical.height,
            &selection,
        )?;
        let width = canonical.width as usize;
        let height = canonical.height as usize;
        let (x0, y0, x1, y1) = mask.window(canonical.width, canonical.height);
        let mut pixels = canonical.pixels.clone();
        let mut changed = false;
        for row in y0..y1 {
            for col in x0..x1 {
                if !mask.covers(col, row) {
                    continue;
                }
                let index = (row as usize * width + col as usize) * 4;
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

    /// Whether the active document carries a committed pixel selection.
    /// The canvas context menu enables its copy entries on exactly this test.
    pub(crate) fn pixel_selection_present(&self) -> bool {
        self.active_document
            .as_ref()
            .and_then(|doc| self.sessions.get(&doc.doc_id))
            .and_then(|session| session.loaded_document().content_snapshot())
            .is_some_and(|content| content.selection.is_some())
    }

    /// Whether the app-local selection clipboard holds an image, the state
    /// the reference build's clipboard check exposed.
    pub(crate) fn selection_clipboard_present(&self) -> bool {
        self.clipboard.is_some()
    }

    /// Whether the active layer owns a pixel store (`edit.cut`/`edit.copy`
    /// also need the layer's own bitmap).
    pub(crate) fn active_layer_has_bitmap(&self, cx: &Context<Self>) -> bool {
        let Some(doc_id) = self.active_document.as_ref().map(|doc| doc.doc_id.clone()) else {
            return false;
        };
        let Some(session) = self.sessions.get(&doc_id) else {
            return false;
        };
        let state = self.state.read(cx);
        let Some(core_id) = state
            .layers
            .iter()
            .find(|row| row.id == state.selected_layer)
            .and_then(|row| row.core_id.clone())
        else {
            return false;
        };
        session
            .loaded_document()
            .registry
            .get_layer(&doc_id, &core_id)
            .is_some()
    }

    #[cfg(test)]
    pub(crate) fn clipboard_for_test(&self) -> Option<(u32, u32, Vec<u8>)> {
        self.clipboard
            .as_ref()
            .map(|clip| (clip.width, clip.height, clip.pixels.clone()))
    }
}

/// Crop transparent padding from a straight-RGBA crop. The selection
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

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> Transform2D {
        Transform2D {
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        }
    }

    fn selection(x: f64, y: f64, width: f64, height: f64, angle: f64) -> SelectionState {
        SelectionState {
            x,
            y,
            width,
            height,
            angle,
            shape: Some(SelectionShape::Rect),
            inverted: None,
        }
    }

    #[test]
    fn an_identity_layer_keeps_the_raw_selection_rect() {
        let mask = App::selection_mask(
            &identity(),
            32.0,
            32.0,
            32,
            32,
            &selection(1.0, 2.0, 4.0, 3.0, 0.0),
        )
        .expect("the selection lies inside the layer");
        assert_eq!((mask.x0, mask.y0, mask.x1, mask.y1), (1, 2, 5, 5));
        assert!(mask.covers(4, 4));
        assert!(!mask.covers(5, 4));
    }

    #[test]
    fn a_rotated_selection_addresses_the_box_of_its_rotated_corners() {
        let mask = App::selection_mask(
            &identity(),
            32.0,
            32.0,
            32,
            32,
            &selection(8.0, 8.0, 4.0, 4.0, 45.0),
        )
        .expect("the rotated selection lies inside the layer");
        // A 4x4 square has a 2.828 half-diagonal, so the box spans 7.17..12.83.
        assert_eq!((mask.x0, mask.y0, mask.x1, mask.y1), (7, 7, 13, 13));
        assert!(
            mask.covers(7, 10),
            "a plain marquee clears its whole box, not the rotated quad"
        );
        assert!(!mask.covers(6, 10));
    }

    #[test]
    fn a_moved_layer_maps_the_selection_into_layer_local_space() {
        let transform = Transform2D {
            x: 2.0,
            y: -1.0,
            ..identity()
        };
        let mask = App::selection_mask(
            &transform,
            32.0,
            32.0,
            32,
            32,
            &selection(8.0, 8.0, 4.0, 4.0, 0.0),
        )
        .expect("the selection lies inside the layer");
        assert_eq!((mask.x0, mask.y0, mask.x1, mask.y1), (6, 9, 10, 13));
    }

    #[test]
    fn a_scaled_layer_counts_layer_pixels_not_document_pixels() {
        let transform = Transform2D {
            scale_x: 2.0,
            scale_y: 2.0,
            ..identity()
        };
        let mask = App::selection_mask(
            &transform,
            32.0,
            32.0,
            32,
            32,
            &selection(8.0, 8.0, 8.0, 8.0, 0.0),
        )
        .expect("the selection lies inside the layer");
        assert_eq!((mask.x0, mask.y0, mask.x1, mask.y1), (4, 4, 8, 8));
    }

    #[test]
    fn an_ellipse_marquee_addresses_the_inscribed_ellipse() {
        let mut ellipse = selection(4.0, 4.0, 4.0, 4.0, 0.0);
        ellipse.shape = Some(SelectionShape::Ellipse);
        let mask = App::selection_mask(&identity(), 32.0, 32.0, 32, 32, &ellipse)
            .expect("the selection lies inside the layer");
        assert!(mask.covers(6, 6), "the centre is inside the ellipse");
        assert!(!mask.covers(4, 4), "the box corner is outside the ellipse");
        assert!(
            mask.covers(4, 6),
            "the left edge midpoint is on the ellipse"
        );
    }

    #[test]
    fn an_inverted_selection_addresses_the_layer_minus_the_box() {
        let mut inverted = selection(4.0, 4.0, 4.0, 4.0, 0.0);
        inverted.inverted = Some(true);
        let mask = App::selection_mask(&identity(), 8.0, 8.0, 8, 8, &inverted)
            .expect("the selection lies inside the layer");
        assert_eq!(mask.window(8, 8), (0, 0, 8, 8));
        assert!(mask.covers(0, 0));
        assert!(!mask.covers(5, 5));
    }

    #[test]
    fn a_selection_wholly_outside_the_layer_is_empty() {
        let error = App::selection_mask(
            &identity(),
            8.0,
            8.0,
            8,
            8,
            &selection(20.0, 0.0, 4.0, 4.0, 0.0),
        )
        .expect_err("a selection beyond the layer has no pixels");
        assert!(error.contains("empty"), "{error}");
    }
}
