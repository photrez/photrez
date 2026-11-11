//! Multi-document tabs: projection, activation and per-tab close.
//!
//! Document truth stays in `SessionRegistry`; this module only decides which
//! session is on screen. The per-document camera is session state: switching
//! away parks it in `tab_cameras`, switching back restores it.

use gpui_kit::Context;

use crate::display::canvas_bridge::CanvasImage;
use crate::services::document::LoadedDocument;
use crate::state::TabRow;

use super::{App, CloseIntent};

impl App {
    /// Rebuild the tab strip from the open sessions, in tab order.
    pub(crate) fn project_tabs(&mut self, cx: &mut Context<Self>) {
        let active = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone());
        let tabs: Vec<TabRow> = self
            .document_tabs
            .iter()
            .filter_map(|doc_id| {
                let session = self.sessions.get(doc_id)?;
                Some(TabRow {
                    doc_id: doc_id.clone(),
                    name: session.display_name().to_string(),
                    dirty: session.is_dirty(),
                })
            })
            .collect();
        self.state.update(cx, |state, _| {
            state.tabs = tabs;
            state.active_tab = active;
        });
    }

    fn composite_for(&self, doc_id: &str) -> Result<CanvasImage, String> {
        let composite = {
            let loaded = self
                .sessions
                .get(doc_id)
                .ok_or_else(|| "active session is missing".to_string())?
                .loaded_document();
            let content = loaded
                .content_snapshot()
                .ok_or_else(|| "loaded content is missing".to_string())?;
            photrez_core::render::compositor::composite_layers_with_font_data(
                content,
                &loaded.registry,
                Some(crate::INTER),
            )
            .map_err(|error| error.to_string())?
        };
        CanvasImage::from_rgba(composite.width, composite.height, composite.pixels)
            .map_err(|error| format!("{error:?}"))
    }

    /// Put an already-open document on screen, parking the current camera.
    pub(crate) fn activate_document(
        &mut self,
        doc_id: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self
            .active_document
            .as_ref()
            .is_some_and(|document| document.doc_id == doc_id)
        {
            return Ok(());
        }
        if self.sessions.get(doc_id).is_none() {
            return Err("that document is not open".to_string());
        }
        let canvas = self.composite_for(doc_id)?;
        self.cancel_canvas_gesture(cx);
        if self.state.read(cx).text_edit.is_some() {
            let _ = self.commit_text_edit(cx);
        }
        self.cancel_color_picker_for_document_change(cx);
        let camera = self.tab_cameras.remove(doc_id).flatten();
        if let Some(previous) = self.active_document.as_ref() {
            self.tab_cameras
                .insert(previous.doc_id.clone(), previous.camera);
        }
        let display_name = self
            .sessions
            .get(doc_id)
            .map(|session| session.display_name().to_string())
            .unwrap_or_else(|| doc_id.to_string());
        let retired = self.active_document.replace(super::ActiveDocument {
            doc_id: doc_id.to_string(),
            canvas,
            canvas_stale: false,
            camera,
        });
        if let Some(retired) = retired {
            self.retired_textures.extend(retired.canvas.into_textures());
        }
        self.state.update(cx, |state, _| {
            state.document_name = display_name;
            state.document_id = Some(doc_id.to_string());
            state.has_document = true;
            state.status_error = None;
        });
        self.reset_transient_for_active_document(cx);
        self.project_layers(cx);
        self.project_history(cx);
        self.project_tabs(cx);
        cx.notify();
        Ok(())
    }

    /// A document change invalidates interactive state that is addressed in
    /// document coordinates. Crop is re-seeded for the new page, and a stale
    /// frame or undo stack never crosses documents.
    pub(crate) fn reset_transient_for_active_document(&mut self, cx: &mut Context<Self>) {
        self.crop_drag = None;
        self.selection_edit_mode = false;
        let Some((width, height)) = self
            .active_document
            .as_ref()
            .map(|document| document.canvas.dimensions())
        else {
            return;
        };
        let crop_active = self.state.read(cx).active_tool == crate::state::ToolId::Crop;
        self.state.update(cx, |state, _| {
            state.crop_rect =
                crop_active.then_some([0.0, 0.0, f64::from(width), f64::from(height)]);
            state.crop_history.clear();
            state.crop_redo.clear();
            state.selection_drag = None;
            state.hud = None;
        });
    }

    /// Close one tab. A dirty tab opens the same save/discard prompt as a
    /// dirty document close; the prompt's discard/save actions close this tab.
    pub(crate) fn close_document_tab(&mut self, doc_id: &str, cx: &mut Context<Self>) -> bool {
        let Some(session) = self.sessions.get(doc_id) else {
            return false;
        };
        if session.is_dirty() {
            if self
                .active_document
                .as_ref()
                .is_some_and(|document| document.doc_id != doc_id)
            {
                if let Err(error) = self.activate_document(doc_id, cx) {
                    self.set_status_error(Some(format!("Tab: {error}")), cx);
                    return false;
                }
            }
            self.pending_close = Some(CloseIntent::Tab {
                doc_id: doc_id.to_string(),
            });
            self.state.update(cx, |state, _| {
                state.active_dialog = Some(crate::state::DialogId::DirtyClose);
            });
            cx.notify();
            return false;
        }
        self.close_document_tab_unchecked(doc_id, cx)
    }

    /// Close a tab without a dirty check; the save/discard prompt already made
    /// the decision.
    pub(crate) fn close_document_tab_unchecked(
        &mut self,
        doc_id: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.document_tabs.iter().position(|tab| tab == doc_id) else {
            return false;
        };
        if self.sessions.get(doc_id).is_none() {
            return false;
        }
        if self.sessions.discard(doc_id).is_err() {
            return false;
        }
        self.document_tabs.remove(index);
        self.tab_cameras.remove(doc_id);
        if self
            .active_document
            .as_ref()
            .is_some_and(|document| document.doc_id == doc_id)
        {
            if let Some(retired) = self.active_document.take() {
                self.retired_textures.extend(retired.canvas.into_textures());
            }
            let next = self
                .document_tabs
                .get(index.min(self.document_tabs.len().saturating_sub(1)))
                .cloned();
            if let Some(next) = next {
                if let Err(error) = self.activate_document(&next, cx) {
                    self.set_status_error(Some(format!("Tab: {error}")), cx);
                }
            } else {
                self.state.update(cx, |state, _| {
                    state.has_document = false;
                    state.document_id = None;
                    state.document_name.clear();
                    state.layers.clear();
                    state.layer_flags.clear();
                    state.history.clear();
                });
            }
        }
        self.project_tabs(cx);
        cx.notify();
        true
    }

    /// Reorder one tab by document id; the active document does not change.
    pub(crate) fn move_document_tab(
        &mut self,
        from: &str,
        to: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if from == to {
            return false;
        }
        let (Some(from_index), Some(to_index)) = (
            self.document_tabs.iter().position(|tab| tab == from),
            self.document_tabs.iter().position(|tab| tab == to),
        ) else {
            return false;
        };
        let moved = self.document_tabs.remove(from_index);
        self.document_tabs.insert(to_index, moved);
        self.project_tabs(cx);
        cx.notify();
        true
    }

    #[cfg(test)]
    pub(crate) fn open_tab_ids_for_test(&self) -> Vec<String> {
        self.document_tabs.clone()
    }

    /// Test-fixture convenience: a white "Untitled" document in a new tab.
    #[cfg(test)]
    pub(crate) fn create_document(
        &mut self,
        width: u32,
        height: u32,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.create_new_named_document(
            "Untitled".to_string(),
            width,
            height,
            crate::state::NewDocBackground::White,
            cx,
        )
    }

    /// The New Document dialog's Create action: one Background layer named
    /// exactly as the form says, filled white or left transparent.
    /// The document is marked dirty so closing it
    /// prompts for a save path.
    pub(crate) fn create_new_named_document(
        &mut self,
        name: String,
        width: u32,
        height: u32,
        background: crate::state::NewDocBackground,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
            return Err("document dimensions must be 1..=16384".to_string());
        }
        let id = format!("native-doc-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let fill = match background {
            crate::state::NewDocBackground::White => 255u8,
            crate::state::NewDocBackground::Transparent => 0u8,
        };
        let rgba = vec![fill; width as usize * height as usize * 4];
        let loaded =
            LoadedDocument::seeded_background(id.clone(), name, width, height, rgba.clone())
                .map_err(|error| format!("cannot create the document: {error}"))?;
        let canvas =
            CanvasImage::from_rgba(width, height, rgba).map_err(|error| format!("{error:?}"))?;
        self.install_document(loaded, canvas, None, cx)?;
        self.sessions
            .mark_dirty(&id)
            .map_err(|error| format!("dirty state failed: {error:?}"))?;
        self.project_tabs(cx);
        self.state.update(cx, |state, _| state.active_dialog = None);
        cx.notify();
        Ok(())
    }

    /// A Quick Preset on the empty workspace card: a blank white document
    /// named after the preset and sized from the same table the card draws.
    pub(crate) fn create_preset_document(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((name, width, height)) = crate::views::empty_state::preset_document(index) else {
            return;
        };
        if let Err(error) = self.create_new_named_document(
            name.to_string(),
            width,
            height,
            crate::state::NewDocBackground::White,
            cx,
        ) {
            self.state
                .update(cx, |state, _| state.status_error = Some(error));
            cx.notify();
        }
    }

    /// A `--shot` launch renders the seeded shell without a session. Install
    /// the same one-layer Background document a New Document creates, so the
    /// Layers row, its eye/lock controls and the Properties banner read core
    /// state exactly as the mounted panels read it. The document starts clean.
    pub(crate) fn install_shot_document(&mut self, cx: &mut Context<Self>) {
        let (width, height, rgba) = crate::services::document::blank_white_document();
        let name = self.state.read(cx).document_name.clone();
        let Ok(loaded) = LoadedDocument::seeded_background(
            "shot-document".to_owned(),
            name,
            width,
            height,
            rgba,
        ) else {
            return;
        };
        let Ok(canvas) = CanvasImage::from_rgba(
            width,
            height,
            vec![255; width as usize * height as usize * 4],
        ) else {
            return;
        };
        if let Err(error) = self.install_document(loaded, canvas, None, cx) {
            eprintln!("photrez: the shot document was not installed: {error}");
        }
    }

    #[cfg(test)]
    pub(crate) fn active_tab_id_for_test(&self) -> Option<String> {
        self.active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
    }
}
