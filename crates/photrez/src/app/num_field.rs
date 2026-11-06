//! Inline numeric edits: a focused field commits valid keystrokes live,
//! Escape restores the value the edit started from, and Enter ends the edit.

use gpui_kit::{Context, KeyDownEvent, Window};

use crate::state::NumFieldSession;

use super::App;

/// Every inline numeric field the app opens. A row whose id is absent here gets no focus
/// handle, so Tab cannot reach it; adding a field means adding its id here as well.
/// Traversal order comes from the view tree, not from this list.
pub(crate) const NUM_FIELD_IDS: [&str; 26] = [
    "opt-text-size",
    "opt-text-stroke-width",
    "opt-tolerance",
    "opt-shape-radius",
    "opt-stroke-width",
    "opt-brush-size",
    "opt-brush-hardness",
    "opt-brush-opacity",
    "opt-brush-flow",
    "opt-brush-smoothing",
    "sel-x",
    "sel-y",
    "sel-w",
    "sel-h",
    "sel-r",
    "num-x",
    "num-y",
    "num-r",
    "prop-x",
    "prop-y",
    "prop-r",
    "prop-w",
    "prop-h",
    "print-width",
    "print-height",
    "print-copies",
];

impl App {
    pub(crate) fn begin_num_field(
        &mut self,
        id: &'static str,
        value: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let original = format!("{value}");
        self.state.update(cx, |state, _| {
            state.num_field = Some(NumFieldSession {
                id,
                buffer: original.clone(),
                original,
                replace_on_type: true,
            });
        });
        if let Some(handle) = self.state.read(cx).num_field_focus_for(id) {
            window.focus(&handle, cx);
        }
        cx.notify();
    }

    /// Whether the open edit belongs to this row. A keystroke that arrives on a
    /// focused row edits that row's session and no other.
    pub(crate) fn num_field_row_is_editing(&self, id: &'static str, cx: &gpui_kit::App) -> bool {
        self.state
            .read(cx)
            .num_field
            .as_ref()
            .is_some_and(|field| field.id == id)
    }

    /// Ends the open edit keeping what the user typed, the path Enter takes.
    /// An empty buffer restores the value the edit started from.
    pub(crate) fn commit_num_field(&mut self, cx: &mut Context<Self>) {
        let empty = self
            .state
            .read(cx)
            .num_field
            .as_ref()
            .map(|field| field.buffer.trim().is_empty())
            .unwrap_or(true);
        if empty {
            self.cancel_num_field(cx);
            return;
        }
        self.state.update(cx, |state, _| state.num_field = None);
        cx.notify();
    }

    /// Feeds a keystroke to one focused numeric row. Tab moves focus before the
    /// keystroke is delivered, so the row the user left commits its typed value
    /// and this row opens through `open`, the same path its click takes. That
    /// makes Tab-then-type behave like click-then-type, seed replacement
    /// included. Escape, Enter and Backspace stay with the open session, so
    /// their behaviour does not move.
    pub(crate) fn num_field_key_in_row<F>(
        &mut self,
        id: &'static str,
        event: &KeyDownEvent,
        window: &mut Window,
        open: F,
        cx: &mut Context<Self>,
    ) where
        F: FnOnce(&mut Self, &mut Window, &mut Context<Self>),
    {
        if !self.num_field_row_is_editing(id, cx) && numeric_key(event).is_some() {
            self.commit_num_field(cx);
            open(self, window, cx);
        }
        self.num_field_key(event, cx);
    }

    pub(crate) fn begin_text_size_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let size = {
            let state = self.state.read(cx);
            if state.selected_layer_type.as_deref() == Some("text") {
                state.selected_text_font_size
            } else {
                state.text_font_size
            }
        };
        self.begin_num_field("opt-text-size", size, window, cx);
    }

    pub(crate) fn begin_fill_tolerance_field(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tolerance = self.state.read(cx).fill_tolerance as f64;
        self.begin_num_field("opt-tolerance", tolerance, window, cx);
    }

    pub(crate) fn begin_shape_radius_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let radius = self.state.read(cx).shape_radius;
        self.begin_num_field("opt-shape-radius", radius, window, cx);
    }

    pub(crate) fn begin_stroke_width_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let width = self.state.read(cx).shape_stroke_width;
        self.begin_num_field("opt-stroke-width", width, window, cx);
    }

    /// Opens one of the selection bar's five fields.
    /// The fields only exist while a pixel
    /// selection is committed, so the seed value reads the committed state.
    pub(crate) fn begin_selection_field(
        &mut self,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.committed_selection() else {
            return;
        };
        let value = match id {
            "sel-x" => selection.x,
            "sel-y" => selection.y,
            "sel-w" => selection.width,
            "sel-h" => selection.height,
            "sel-r" => selection.angle,
            _ => return,
        };
        self.begin_num_field(id, value, window, cx);
    }

    /// Opens one of the editable transform fields: the Move bar's (`num-*`) or
    /// the Properties panel's (`prop-*`). A row whose lock flag is set keeps
    /// its disabled state, so its click does nothing.
    pub(crate) fn begin_transform_field(
        &mut self,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(readout) = self.transform_readout(cx) else {
            return;
        };
        let value = match id {
            "num-x" | "prop-x" => readout.x,
            "num-y" | "prop-y" => readout.y,
            "num-r" | "prop-r" => readout.rotation,
            "prop-w" => readout.width,
            "prop-h" => readout.height,
            _ => return,
        };
        if !readout.editable(id.chars().next_back().unwrap_or(' ')) {
            return;
        }
        self.begin_num_field(id, value, window, cx);
    }

    pub(crate) fn begin_text_stroke_width_field(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let width = self.effective_text_stroke_width(cx);
        let width = if width > 0.0 { width } else { 4.0 };
        self.begin_num_field("opt-text-stroke-width", width, window, cx);
    }

    /// Opens one of the brush bar's five numeric fields. Percent fields seed
    /// the buffer with 0..100, matching what the field shows.
    pub(crate) fn begin_brush_field(
        &mut self,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let eraser = self.state.read(cx).active_tool == crate::state::ToolId::Eraser;
        let state = self.state.read(cx);
        let value = match (id, eraser) {
            ("opt-brush-size", false) => state.brush_size,
            ("opt-brush-size", true) => state.eraser_size,
            ("opt-brush-hardness", false) => state.brush_hardness * 100.0,
            ("opt-brush-hardness", true) => state.eraser_hardness * 100.0,
            ("opt-brush-opacity", false) => state.brush_opacity * 100.0,
            ("opt-brush-opacity", true) => state.eraser_opacity * 100.0,
            ("opt-brush-flow", false) => state.brush_flow * 100.0,
            ("opt-brush-flow", true) => state.eraser_flow * 100.0,
            ("opt-brush-smoothing", false) => state.brush_smoothing,
            ("opt-brush-smoothing", true) => state.eraser_smoothing,
            _ => return,
        };
        self.begin_num_field(id, value, window, cx);
    }

    /// Opens the Print dialog's Width or Height row. The view supplies the pair
    /// of sizes the row shows at 100 % scale in its own unit, so the commit
    /// divides by that basis and never converts units a second time.
    pub(crate) fn begin_print_size_field(
        &mut self,
        id: &'static str,
        basis: (f64, f64),
        value: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| {
            state.print_size_basis = Some(basis);
        });
        self.begin_num_field(id, value, window, cx);
    }

    /// Writes a typed printed size back as the scale percentage the dialog
    /// already drives. A size is a scale question here, not a second paper
    /// authority, so the row keeps one owner.
    fn set_print_size(&mut self, width_axis: bool, value: f64, cx: &mut Context<Self>) {
        let basis = match (width_axis, self.state.read(cx).print_size_basis) {
            (_, None) => return,
            (true, Some((basis, _))) => basis,
            (false, Some((_, basis))) => basis,
        };
        if basis <= 0.0 || !basis.is_finite() {
            return;
        }
        let percent = (value / basis * 100.0).round().clamp(10.0, 400.0) as u32;
        self.state.update(cx, |state, _| {
            state.print_scale_to_fit = false;
            state.print_scale_percent = percent;
        });
    }

    pub(crate) fn cancel_num_field(&mut self, cx: &mut Context<Self>) {
        let Some((id, original)) = self
            .state
            .read(cx)
            .num_field
            .as_ref()
            .map(|field| (field.id, field.original.clone()))
        else {
            return;
        };
        self.state.update(cx, |state, _| state.num_field = None);
        self.apply_num_field_value(id, &original, cx);
        cx.notify();
    }

    pub(crate) fn num_field_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        match event.keystroke.key.as_str() {
            "escape" => {
                self.cancel_num_field(cx);
                true
            }
            "enter" => {
                self.commit_num_field(cx);
                true
            }
            "backspace" => {
                self.state.update(cx, |state, _| {
                    if let Some(field) = state.num_field.as_mut() {
                        field.replace_on_type = false;
                        field.buffer.pop();
                    }
                });
                self.apply_num_field_buffer(cx);
                true
            }
            _ => {
                let Some(text) = numeric_key(event) else {
                    return false;
                };
                self.state.update(cx, |state, _| {
                    if let Some(field) = state.num_field.as_mut() {
                        if field.replace_on_type {
                            field.buffer.clear();
                            field.replace_on_type = false;
                        }
                        field.buffer.push_str(&text);
                    }
                });
                self.apply_num_field_buffer(cx);
                true
            }
        }
    }

    fn apply_num_field_buffer(&mut self, cx: &mut Context<Self>) {
        let Some((id, raw)) = self
            .state
            .read(cx)
            .num_field
            .as_ref()
            .map(|field| (field.id, field.buffer.clone()))
        else {
            return;
        };
        self.apply_num_field_value(id, &raw, cx);
    }

    fn apply_num_field_value(&mut self, id: &'static str, raw: &str, cx: &mut Context<Self>) {
        let Ok(value) = raw.trim().parse::<f64>() else {
            cx.notify();
            return;
        };
        if !value.is_finite() {
            return;
        }
        let result = match id {
            "opt-text-size" => self.set_text_font_size(value, cx),
            "opt-tolerance" => self.set_fill_tolerance(value, cx),
            "opt-shape-radius" => self.set_shape_radius(value, cx),
            "opt-stroke-width" => self.set_shape_stroke_width(value, cx),
            "opt-text-stroke-width" => self.set_text_stroke_width_field(value, cx),
            "sel-x" | "sel-y" | "sel-w" | "sel-h" | "sel-r" => {
                self.set_selection_field(id, value, cx)
            }
            "num-x" | "num-y" | "num-r" | "prop-x" | "prop-y" | "prop-w" | "prop-h" | "prop-r" => {
                self.set_transform_field(id, value, cx)
            }
            "print-copies" => {
                let copies = value.round().clamp(1.0, 99.0) as u32;
                self.state
                    .update(cx, |state, _| state.print_copies = copies);
                Ok(())
            }
            "print-width" => {
                self.set_print_size(true, value, cx);
                Ok(())
            }
            "print-height" => {
                self.set_print_size(false, value, cx);
                Ok(())
            }
            "opt-brush-size" => {
                self.set_brush_value(crate::app::tool_settings::BrushField::Size, value, cx);
                Ok(())
            }
            "opt-brush-hardness" => {
                self.set_brush_value(crate::app::tool_settings::BrushField::Hardness, value, cx);
                Ok(())
            }
            "opt-brush-opacity" => {
                self.set_brush_value(crate::app::tool_settings::BrushField::Opacity, value, cx);
                Ok(())
            }
            "opt-brush-flow" => {
                self.set_brush_value(crate::app::tool_settings::BrushField::Flow, value, cx);
                Ok(())
            }
            "opt-brush-smoothing" => {
                self.set_brush_value(crate::app::tool_settings::BrushField::Smoothing, value, cx);
                Ok(())
            }
            _ => Ok(()),
        };
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(format!("Text: {error}")), cx),
        }
        cx.notify();
    }
}

/// The character an inline numeric field accepts, or `None` for a keystroke
/// that only moves focus or ends the edit. One classifier keeps the row-adopt
/// test and the buffer test from drifting apart.
fn numeric_key(event: &KeyDownEvent) -> Option<String> {
    let text = event
        .keystroke
        .key_char
        .clone()
        .unwrap_or_else(|| event.keystroke.key.clone());
    let numeric =
        text.chars().count() == 1 && text.chars().all(|c| c.is_ascii_digit() || ".-".contains(c));
    numeric.then_some(text)
}
