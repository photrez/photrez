//! Layer rename prompt: a focused modal that routes characters into a buffer
//! and commits `Command::Rename` on Enter.

use gpui_kit::{Context, KeyDownEvent, Window};
use photrez_core::protocol::Command;

use crate::state::RenameSession;

use super::App;

impl App {
    pub(crate) fn begin_rename_layer(
        &mut self,
        layer_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self
            .active_document
            .as_ref()
            .and_then(|document| self.sessions.get(&document.doc_id))
            .and_then(|session| session.loaded_document().content_snapshot())
            .and_then(|content| {
                content
                    .layers
                    .iter()
                    .find(|layer| layer.id == layer_id)
                    .map(|layer| layer.name.clone())
            })
        else {
            return;
        };
        self.state.update(cx, |state, _| {
            state.rename_layer = Some(RenameSession {
                layer_id,
                content: name,
            });
            state.active_dialog = Some(crate::state::DialogId::Rename);
        });
        window.focus(&self.rename_focus, cx);
        cx.notify();
    }

    pub(crate) fn cancel_rename_layer(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.rename_layer = None;
            state.active_dialog = None;
        });
        cx.notify();
    }

    pub(crate) fn rename_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let key = event.keystroke.key.as_str();
        match key {
            "escape" => {
                self.cancel_rename_layer(cx);
                true
            }
            "enter" => {
                let result = self.commit_rename_layer(cx);
                match result {
                    Ok(()) => self.set_status_error(None, cx),
                    Err(error) => self.set_status_error(Some(format!("Rename: {error}")), cx),
                }
                true
            }
            "backspace" => {
                self.state.update(cx, |state, _| {
                    if let Some(session) = state.rename_layer.as_mut() {
                        session.content.pop();
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
                if text.chars().count() == 1 && !text.chars().any(char::is_control) {
                    self.state.update(cx, |state, _| {
                        if let Some(session) = state.rename_layer.as_mut() {
                            session.content.push_str(&text);
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

    pub(crate) fn commit_rename_layer(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(session) = self.state.read(cx).rename_layer.clone() else {
            return Ok(());
        };
        let name = session.content.trim().to_string();
        if name.is_empty() {
            return Err("the layer name cannot be empty".to_string());
        }
        let doc_id = self.active_doc_id()?;
        let unchanged = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .and_then(|content| {
                content
                    .layers
                    .iter()
                    .find(|layer| layer.id == session.layer_id)
            })
            .is_some_and(|layer| layer.name == name);
        self.state.update(cx, |state, _| {
            state.rename_layer = None;
            state.active_dialog = None;
        });
        if unchanged {
            return Ok(());
        }
        self.apply_core_command(
            &doc_id,
            Command::Rename {
                id: session.layer_id,
                name,
            },
            true,
            cx,
        )
    }

    #[cfg(test)]
    pub(crate) fn rename_content_for_test(&self, cx: &gpui_kit::App) -> Option<String> {
        self.state
            .read(cx)
            .rename_layer
            .as_ref()
            .map(|session| session.content.clone())
    }
}
