//! Resize Canvas dialog: unit-aware digit fields with Tab switching, an aspect
//! lock, and Enter applying the core `ResizeCanvas` command (document size
//! only). Escape closes an open unit list first, then cancels.

use gpui_kit::{Context, KeyDownEvent, Window};
use photrez_core::protocol::Command;

use crate::render::canvas_bridge::MAX_CANVAS_DIM;
use crate::state::{ResizeSession, ToastSeverity};
use crate::units::DimensionUnit;

use super::App;

impl App {
    pub(crate) fn begin_resize_canvas(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (width, height) = self.active_document_size()?;
        let width_px = width as u32;
        let height_px = height as u32;
        self.state.update(cx, |state, _| {
            state.resize_unit_open = false;
            state.resize_canvas = Some(ResizeSession {
                width_px,
                height_px,
                width: DimensionUnit::Px.format(width_px),
                height: DimensionUnit::Px.format(height_px),
                field: 0,
                unit: DimensionUnit::Px,
                aspect: f64::from(width_px) / f64::from(height_px.max(1)),
                aspect_locked: true,
                edited: None,
            });
            state.open_select = None;
            state.active_dialog = Some(crate::state::DialogId::ResizeCanvas);
        });
        window.focus(&self.resize_focus, cx);
        cx.notify();
        Ok(())
    }

    pub(crate) fn set_resize_field(&mut self, field: u8, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            if let Some(session) = state.resize_canvas.as_mut() {
                session.field = field.min(1);
            }
        });
        cx.notify();
    }

    /// Points the session and the keyboard at one size field, so a pointer
    /// click and a Tab leave the same state behind.
    pub(crate) fn focus_resize_field(
        &mut self,
        field: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = field.min(1);
        self.set_resize_field(field, cx);
        if let Some(handle) = self.state.read(cx).resize_field_focus(field) {
            window.focus(&handle, cx);
        }
    }

    /// The size field the keyboard points at, or `None` when focus is outside
    /// the dialog's fields.
    fn focused_resize_field(&self, window: &Window, cx: &gpui_kit::App) -> Option<u8> {
        let state = self.state.read(cx);
        (0..crate::state::AppState::RESIZE_FIELD_COUNT as u8).find(|index| {
            state
                .resize_field_focus(*index)
                .is_some_and(|handle| handle.is_focused(window))
        })
    }

    /// Expands/collapses the unit list.
    pub(crate) fn toggle_resize_unit(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.resize_unit_open = !state.resize_unit_open;
        });
        cx.notify();
    }

    /// Re-displays both fields in `unit`, keeping the pixel values authoritative.
    pub(crate) fn set_resize_unit(&mut self, unit: DimensionUnit, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            if let Some(session) = state.resize_canvas.as_mut() {
                if session.unit == unit {
                    return;
                }
                session.width_px =
                    text_to_px(&session.width, session.unit).unwrap_or(session.width_px);
                session.height_px =
                    text_to_px(&session.height, session.unit).unwrap_or(session.height_px);
                session.unit = unit;
                session.width = unit.format(session.width_px);
                session.height = unit.format(session.height_px);
            }
            state.resize_unit_open = false;
        });
        cx.notify();
    }

    pub(crate) fn toggle_resize_aspect_lock(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            if let Some(session) = state.resize_canvas.as_mut() {
                session.aspect_locked = !session.aspect_locked;
            }
        });
        cx.notify();
    }

    pub(crate) fn cancel_resize_canvas(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.resize_canvas = None;
            state.active_dialog = None;
            state.open_select = None;
            state.resize_unit_open = false;
        });
        cx.notify();
    }

    pub(crate) fn resize_key(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        // Tab is a real focus move, applied by the platform before this handler
        // runs, so the session follows the field that ended up focused.
        if let Some(focused) = self.focused_resize_field(window, cx) {
            let current = self.state.read(cx).resize_canvas.as_ref().map(|s| s.field);
            if current != Some(focused) {
                self.set_resize_field(focused, cx);
            }
        }
        match key {
            "escape" => {
                if self.state.read(cx).resize_unit_open {
                    self.state
                        .update(cx, |state, _| state.resize_unit_open = false);
                    cx.notify();
                } else {
                    self.cancel_resize_canvas(cx);
                }
                true
            }
            "enter" => {
                let result = self.apply_resize_canvas(cx);
                match result {
                    Ok(()) => self.set_status_error(None, cx),
                    Err(error) => self.set_status_error(Some(format!("Resize: {error}")), cx),
                }
                true
            }
            "backspace" => {
                self.state.update(cx, |state, _| {
                    if let Some(session) = state.resize_canvas.as_mut() {
                        if session.field == 0 {
                            session.width.pop();
                        } else {
                            session.height.pop();
                        }
                        session.edited = Some(session.field);
                    }
                });
                cx.notify();
                true
            }
            _ => {
                let text = event
                    .keystroke
                    .key_char
                    .clone()
                    .unwrap_or_else(|| event.keystroke.key.clone());
                let decimal = text == ".";
                let accepted = text.chars().count() == 1
                    && (text.chars().all(|c| c.is_ascii_digit())
                        || (decimal
                            && self
                                .state
                                .read(cx)
                                .resize_canvas
                                .as_ref()
                                .is_some_and(|session| session.unit.accepts_decimal())));
                if !accepted {
                    return false;
                }
                self.state.update(cx, |state, _| {
                    if let Some(session) = state.resize_canvas.as_mut() {
                        let field = if session.field == 0 {
                            &mut session.width
                        } else {
                            &mut session.height
                        };
                        let limit = if session.unit.accepts_decimal() { 8 } else { 5 };
                        if field.len() < limit {
                            field.push_str(&text);
                        }
                        session.edited = Some(session.field);
                    }
                });
                cx.notify();
                true
            }
        }
    }

    pub(crate) fn apply_resize_canvas(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(session) = self.state.read(cx).resize_canvas.clone() else {
            return Ok(());
        };
        let mut width = text_to_px(&session.width, session.unit).unwrap_or(session.width_px);
        let mut height = text_to_px(&session.height, session.unit).unwrap_or(session.height_px);
        let mut clamped = width > MAX_CANVAS_DIM || height > MAX_CANVAS_DIM;
        width = width.min(MAX_CANVAS_DIM);
        height = height.min(MAX_CANVAS_DIM);
        if session.aspect_locked {
            // The field the user last typed into drives the linked dimension,
            // like a per-field commit.
            let linked = if session.edited == Some(1) {
                (f64::from(height) * session.aspect).round().max(1.0) as u32
            } else {
                (f64::from(width) / session.aspect).round().max(1.0) as u32
            };
            clamped |= linked > MAX_CANVAS_DIM;
            if session.edited == Some(1) {
                width = linked.clamp(1, MAX_CANVAS_DIM);
            } else {
                height = linked.clamp(1, MAX_CANVAS_DIM);
            }
        }
        if width == 0 || height == 0 {
            return Err("the dimensions must be positive".to_string());
        }
        if clamped {
            self.show_toast(
                format!("Maximum dimension is {MAX_CANVAS_DIM}px"),
                ToastSeverity::Warn,
                cx,
            );
        }
        let doc_id = self.active_doc_id()?;
        self.state.update(cx, |state, _| {
            state.resize_canvas = None;
            state.active_dialog = None;
            state.open_select = None;
            state.resize_unit_open = false;
        });
        self.apply_core_command(
            &doc_id,
            Command::ResizeCanvas {
                width: f64::from(width),
                height: f64::from(height),
            },
            true,
            cx,
        )?;
        if let Some(document) = self.active_document.as_mut() {
            document.camera = None;
        }
        cx.notify();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn resize_fields_for_test(&self, cx: &gpui_kit::App) -> Option<(String, String)> {
        self.state
            .read(cx)
            .resize_canvas
            .as_ref()
            .map(|session| (session.width.clone(), session.height.clone()))
    }
}

/// Parses a field buffer in the session's current unit to whole pixels.
fn text_to_px(text: &str, unit: DimensionUnit) -> Option<u32> {
    let value: f64 = text.trim().parse().ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    Some(unit.to_px(value))
}
