//! The text tool's in-place editor: typed characters live in UI state and are
//! committed to the layer's `TextData` as one core command when the editor
//! closes (Escape or a canvas click). The canvas key context becomes
//! `TextEdit` while the editor is open so tool letter shortcuts do not fire.

use gpui_kit::{Context, KeyDownEvent};
use photrez_core::protocol::Command;

use crate::state::{TextEditSession, ToolId};

use super::App;

impl App {
    pub(crate) fn begin_text_edit(
        &mut self,
        layer_id: String,
        doc_x: f64,
        doc_y: f64,
        content: String,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| {
            state.text_edit = Some(TextEditSession {
                layer_id,
                doc_x,
                doc_y,
                content,
            });
        });
        cx.notify();
    }

    /// Append/delete text. Returns true when the key was consumed by the
    /// editor (Escape commits and closes).
    pub(crate) fn text_edit_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            let _ = self.commit_text_edit(cx);
            return true;
        }
        let mut changed = false;
        self.state.update(cx, |state, _| {
            let Some(session) = state.text_edit.as_mut() else {
                return;
            };
            match key {
                "backspace" => {
                    session.content.pop();
                    changed = true;
                }
                "enter" => {
                    session.content.push('\n');
                    changed = true;
                }
                "space" => {
                    session.content.push(' ');
                    changed = true;
                }
                _ => {
                    let text = event
                        .keystroke
                        .key_char
                        .clone()
                        .unwrap_or_else(|| event.keystroke.key.clone());
                    if text.chars().count() == 1 && !text.chars().any(char::is_control) {
                        session.content.push_str(&text);
                        changed = true;
                    }
                }
            }
        });
        if changed {
            cx.notify();
        }
        self.state.read(cx).text_edit.is_some()
    }

    /// Commit the editor buffer to the layer as one history entry and close.
    pub(crate) fn commit_text_edit(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(session) = self.state.read(cx).text_edit.clone() else {
            return Ok(());
        };
        let doc_id = self.active_doc_id()?;
        let mut data = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .and_then(|content| {
                content
                    .layers
                    .iter()
                    .find(|layer| layer.id == session.layer_id)
                    .and_then(|layer| layer.text_data.clone())
            })
            .ok_or_else(|| "the text layer is missing".to_string())?;
        data.content = session.content.clone();
        self.state.update(cx, |state, _| state.text_edit = None);
        self.apply_core_command(
            &doc_id,
            Command::SetLayerParams {
                id: session.layer_id,
                shape_params: None,
                text_data: Some(data),
            },
            true,
            cx,
        )?;
        if self.state.read(cx).active_tool == ToolId::Text {
            // stay on the text tool for the next placement
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn text_edit_content_for_test(&self, cx: &gpui_kit::App) -> Option<String> {
        self.state
            .read(cx)
            .text_edit
            .as_ref()
            .map(|session| session.content.clone())
    }

    #[cfg(test)]
    pub(crate) fn text_layer_id_for_test(&self) -> Option<String> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        content
            .layers
            .iter()
            .find(|layer| layer.text_data.is_some())
            .map(|layer| layer.id.clone())
    }

    #[cfg(test)]
    pub(crate) fn text_layer_data_for_test(
        &self,
    ) -> Option<photrez_core::ptz::layer_params::TextData> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        content
            .layers
            .iter()
            .find_map(|layer| layer.text_data.clone())
    }

    #[cfg(test)]
    pub(crate) fn text_layer_content_for_test(&self, layer_id: &str) -> Option<String> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        content
            .layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .and_then(|layer| layer.text_data.as_ref())
            .map(|data| data.content.clone())
    }
}
