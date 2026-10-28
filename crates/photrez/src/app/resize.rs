//! Resize Canvas dialog: digit fields with Tab switching, Enter applies the
//! core `ResizeCanvas` command (document size only), Escape cancels.

use gpui_kit::{Context, KeyDownEvent, Window};
use photrez_core::protocol::Command;

use crate::state::ResizeSession;

use super::App;

impl App {
    pub(crate) fn begin_resize_canvas(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (width, height) = self.active_document_size()?;
        self.state.update(cx, |state, _| {
            state.resize_canvas = Some(ResizeSession {
                width: format!("{}", width as u32),
                height: format!("{}", height as u32),
                field: 0,
            });
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

    pub(crate) fn cancel_resize_canvas(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.resize_canvas = None;
            state.active_dialog = None;
        });
        cx.notify();
    }

    pub(crate) fn resize_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let key = event.keystroke.key.as_str();
        match key {
            "escape" => {
                self.cancel_resize_canvas(cx);
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
            "tab" => {
                self.state.update(cx, |state, _| {
                    if let Some(session) = state.resize_canvas.as_mut() {
                        session.field = 1 - session.field.min(1);
                    }
                });
                cx.notify();
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
                if text.chars().count() == 1 && text.chars().any(|c| c.is_ascii_digit()) {
                    self.state.update(cx, |state, _| {
                        if let Some(session) = state.resize_canvas.as_mut() {
                            let field = if session.field == 0 {
                                &mut session.width
                            } else {
                                &mut session.height
                            };
                            if field.len() < 5 {
                                field.push_str(&text);
                            }
                        }
                    });
                    cx.notify();
                    true
                } else {
                    false
                }
            }
        }
    }

    pub(crate) fn apply_resize_canvas(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(session) = self.state.read(cx).resize_canvas.clone() else {
            return Ok(());
        };
        let width: u32 = session
            .width
            .parse()
            .map_err(|_| "the width is not a valid number".to_string())?;
        let height: u32 = session
            .height
            .parse()
            .map_err(|_| "the height is not a valid number".to_string())?;
        if width == 0 || height == 0 {
            return Err("the dimensions must be positive".to_string());
        }
        if width > 16_384 || height > 16_384 {
            return Err("the dimensions must be 16384 or less".to_string());
        }
        let doc_id = self.active_doc_id()?;
        self.state.update(cx, |state, _| {
            state.resize_canvas = None;
            state.active_dialog = None;
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
