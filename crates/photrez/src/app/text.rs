//! The text tool's in-place editor: typed characters live in UI state and are
//! committed to the layer's `TextData` as one core command when the editor
//! closes (Ctrl+Enter, a canvas click, or a tool switch). Escape cancels and
//! removes the temporary layer, and an empty buffer is discarded on commit.
//! The canvas key context
//! becomes `TextEdit` while the editor is open so tool letter shortcuts do not
//! fire.

use gpui_kit::{Context, KeyDownEvent};
use photrez_core::protocol::Command;
use photrez_core::ptz::layer_params::TextData;

use crate::state::{TextEditSession, ToolId};

use super::App;

impl App {
    pub(crate) fn begin_text_edit(
        &mut self,
        layer_id: String,
        doc_x: f64,
        doc_y: f64,
        data: TextData,
        cx: &mut Context<Self>,
    ) {
        let content = data.content.clone();
        self.state.update(cx, |state, _| {
            state.text_edit = Some(TextEditSession {
                layer_id,
                doc_x,
                doc_y,
                content,
                data,
                preview: (0, 0),
            });
        });
        cx.notify();
    }

    /// Append/delete text. Returns true when the key was consumed by the
    /// editor. The session contract matches the text overlay: Ctrl+Enter
    /// commits, Escape cancels, and a bare Enter inserts a newline.
    pub(crate) fn text_edit_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            let _ = self.cancel_text_edit(cx);
            return true;
        }
        if key == "enter"
            && (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
        {
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
            self.refresh_text_preview(cx);
        }
        self.state.read(cx).text_edit.is_some()
    }

    /// Commit the editor buffer to the layer as one history entry and close.
    /// An empty buffer is discarded together with the temporary layer.
    pub(crate) fn commit_text_edit(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(session) = self.state.read(cx).text_edit.clone() else {
            return Ok(());
        };
        if session.content.trim().is_empty() {
            return self.cancel_text_edit(cx);
        }
        let doc_id = self.active_doc_id()?;
        self.discard_text_preview();
        self.state.update(cx, |state, _| state.text_edit = None);
        let mut data = session.data.clone();
        data.content = session.content.clone();
        self.apply_core_command(
            &doc_id,
            Command::SetLayerParams {
                id: session.layer_id.clone(),
                shape_params: None,
                text_data: Some(data),
            },
            true,
            cx,
        )?;
        let name: String = session
            .content
            .trim()
            .lines()
            .next()
            .unwrap_or("Text")
            .to_string();
        self.apply_core_command(
            &doc_id,
            Command::Rename {
                id: session.layer_id,
                name,
            },
            true,
            cx,
        )?;
        if self.state.read(cx).active_tool == ToolId::Text {
            // stay on the text tool for the next placement
        }
        Ok(())
    }

    /// Escape / cancel: drop the session and remove the temporary layer.
    pub(crate) fn cancel_text_edit(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(session) = self.state.read(cx).text_edit.clone() else {
            return Ok(());
        };
        let doc_id = self.active_doc_id()?;
        self.discard_text_preview();
        self.state.update(cx, |state, _| state.text_edit = None);
        self.apply_core_command(
            &doc_id,
            Command::DeleteLayer {
                id: session.layer_id,
            },
            true,
            cx,
        )
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

impl App {
    /// Repaint the typing session through the document rasterizer into the
    /// live display canvas. The buffer stays in UI state, so no history entry
    /// is recorded; the patch is dropped on commit or cancel.
    pub(crate) fn refresh_text_preview(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.state.read(cx).text_edit.clone() else {
            return;
        };
        let mut data = session.data.clone();
        data.content = session.content.clone();
        let shot = if data.content.is_empty() {
            None
        } else {
            photrez_core::render::rasterize_text_preview(&data, Some(crate::INTER))
        };
        let size = shot.as_ref().map(|(w, h, _)| (*w, *h)).unwrap_or((0, 0));
        let ox = session.doc_x.round();
        let oy = session.doc_y.round();
        if let Some(active) = self.active_document.as_mut() {
            let retired = active.canvas.discard_preview_patches();
            self.retired_textures.extend(retired);
            if let Some((w, h, rgba)) = shot {
                if ox >= 0.0 && oy >= 0.0 {
                    if let Ok(base) = active.canvas.read_region_rgba(ox as u32, oy as u32, w, h) {
                        let mut blended = base.clone();
                        for (index, pixel) in rgba.chunks_exact(4).enumerate() {
                            let alpha = pixel[3] as u32;
                            let offset = index * 4;
                            for channel in 0..3 {
                                blended[offset + channel] = ((pixel[channel] as u32 * alpha
                                    + blended[offset + channel] as u32 * (255 - alpha))
                                    / 255)
                                    as u8;
                            }
                            blended[offset + 3] = 255;
                        }
                        let _ = active
                            .canvas
                            .push_scaled_patch(ox as u32, oy as u32, w, h, w, h, &blended);
                    }
                }
            }
        }
        self.state.update(cx, |state, _| {
            if let Some(session) = state.text_edit.as_mut() {
                session.preview = size;
            }
        });
        cx.notify();
    }

    /// Drop any pending typing preview patches before a commit or cancel.
    pub(crate) fn discard_text_preview(&mut self) {
        if let Some(active) = self.active_document.as_mut() {
            let retired = active.canvas.discard_preview_patches();
            self.retired_textures.extend(retired);
        }
    }
}
