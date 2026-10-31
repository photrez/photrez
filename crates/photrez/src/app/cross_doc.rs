//! Layer drag across documents and file drops into a document.
//!
//! Mirrors the oracle's `crossDocLayerOps.ts`: a layer dragged from the Layers
//! panel lands on a tab, the panel, the canvas, or the empty workspace; a file
//! dropped on a document becomes a new raster layer: a tab or panel drop centers
//! the layer in the target document, a canvas drop centers it on the cursor,
//! and multiple files fan out by [`CASCADE_OFFSET_PX`]. ALT moves instead of
//! copying. All guards keep the same messages as the oracle so the status bar
//! and toasts read the same thing.

use std::path::PathBuf;

use gpui_kit::Context;
use photrez_core::protocol::{Command, CommandEnvelope, CONTRACT_VERSION};
use photrez_core::ptz::content::{LayerType, PtzLayer, Transform2D};

use super::input::next_layer_id;
use super::App;
use crate::state::{NewDocBackground, ToastSeverity};

/// `MAX_LAYERS` in `engine/types.ts`.
pub(crate) const MAX_LAYERS: usize = 200;
/// `CASCADE_OFFSET_PX` in `crossDocLayerOps.ts`.
pub(crate) const CASCADE_OFFSET_PX: f64 = 24.0;
/// `WorkspaceManager` maximum open documents.
pub(crate) const MAX_DOCUMENTS: usize = 16;

/// The value carried by a layer-row drag. The row id is UI state; the core id
/// is what survives a reprojection, so the payload names the canonical layer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayerDragPayload {
    pub source_doc_id: String,
    /// `None` for an unprojected demo row: only a same-document UI reorder.
    pub core_id: Option<String>,
    pub row_id: u64,
}

/// Where a layer drag landed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LayerDropTarget {
    /// Canvas drop: the layer is centered on this document-space point.
    Canvas { doc_id: String, at: (f64, f64) },
    /// Tab drop: the layer is centered in that document.
    Tab { doc_id: String },
    /// Layers-panel drop: centered in the active document.
    Panel { doc_id: String },
    /// Empty workspace: seed a new document from the layer.
    NewDocument,
}

impl App {
    /// Production entry point: run the drop and surface guard failures the way
    /// the oracle does (error toast), clearing the status bar on success.
    pub(crate) fn apply_layer_drag(
        &mut self,
        payload: &LayerDragPayload,
        target: LayerDropTarget,
        alt: bool,
        cx: &mut Context<Self>,
    ) {
        match self.layer_drag_impl(payload, target, alt, cx) {
            Ok(()) => self.set_status_error(None, cx),
            Err(message) => self.show_toast(message, ToastSeverity::Error, cx),
        }
    }

    /// Testable core of [`App::apply_layer_drag`]. Returns the message for a
    /// guard rejection; the caller decides between a toast and the status bar.
    pub(crate) fn layer_drag_impl(
        &mut self,
        payload: &LayerDragPayload,
        target: LayerDropTarget,
        alt: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(core_id) = payload.core_id.as_deref() else {
            return Err("Layer was deleted. Drop cancelled.".to_string());
        };
        let Some(source_layer) = self.doc_layer(&payload.source_doc_id, core_id) else {
            return Err(if self.sessions.get(&payload.source_doc_id).is_none() {
                "Source document was closed. Drop cancelled.".to_string()
            } else {
                "Layer was deleted. Drop cancelled.".to_string()
            });
        };

        let target = match target {
            LayerDropTarget::NewDocument => {
                if self.document_tabs.len() >= MAX_DOCUMENTS {
                    return Err("Workspace full: close a document first".to_string());
                }
                let width = source_layer.width.max(1.0).round() as u32;
                let height = source_layer.height.max(1.0).round() as u32;
                let name = if source_layer.name.is_empty() {
                    "Image".to_string()
                } else {
                    source_layer.name.clone()
                };
                self.create_new_named_document(
                    name,
                    width,
                    height,
                    NewDocBackground::Transparent,
                    cx,
                )?;
                let doc_id = self.active_doc_id()?;
                LayerDropTarget::Tab { doc_id }
            }
            other => other,
        };

        let (target_doc_id, at) = match &target {
            LayerDropTarget::Canvas { doc_id, at } => (doc_id.clone(), Some(*at)),
            LayerDropTarget::Tab { doc_id } | LayerDropTarget::Panel { doc_id } => {
                (doc_id.clone(), None)
            }
            LayerDropTarget::NewDocument => unreachable!("resolved above"),
        };

        if target_doc_id == payload.source_doc_id {
            // No insertion hint reached this path, so the oracle moves the
            // layer to the end of its own stack (never a silent no-op drop).
            let count = self.doc_layer_count(&target_doc_id)?;
            let source_index = self
                .doc_layers(&target_doc_id)?
                .iter()
                .position(|layer| layer.id == core_id)
                .ok_or_else(|| "Layer was deleted. Drop cancelled.".to_string())?;
            if count == 0 || source_index + 1 == count {
                return Ok(());
            }
            return self.apply_doc_command(
                &target_doc_id,
                Command::Reorder {
                    id: core_id.to_string(),
                    to: count - 1,
                },
                cx,
            );
        }

        if alt && self.doc_layer_count(&payload.source_doc_id)? <= 1 {
            return Err("Cannot move the last layer from a document. Drop cancelled.".to_string());
        }
        if self.doc_layer_count(&target_doc_id)? >= MAX_LAYERS {
            return Err(format!("Target document reached max {MAX_LAYERS} layers"));
        }
        let (doc_w, doc_h) = self.doc_size(&target_doc_id)?;
        let position = match at {
            Some(at) => (
                at.0 - source_layer.width / 2.0,
                at.1 - source_layer.height / 2.0,
            ),
            None => (
                ((doc_w - source_layer.width) / 2.0).max(0.0),
                ((doc_h - source_layer.height) / 2.0).max(0.0),
            ),
        };

        let new_id = next_layer_id();
        if matches!(source_layer.layer_type, LayerType::Raster) {
            let pixels = self
                .sessions
                .get(&payload.source_doc_id)
                .and_then(|session| {
                    session
                        .loaded_document()
                        .registry
                        .get_layer(&payload.source_doc_id, core_id)
                })
                .map(|layer| (layer.width, layer.height, layer.pixels.clone()))
                .ok_or_else(|| "the source layer has no pixel store".to_string())?;
            self.seed_layer(&target_doc_id, &new_id, pixels.0, pixels.1, pixels.2)?;
        }
        let transform = Transform2D {
            x: position.0,
            y: position.1,
            ..source_layer.transform.clone()
        };
        self.apply_doc_command(
            &target_doc_id,
            Command::AddLayerWithState {
                id: new_id.clone(),
                name: source_layer.name.clone(),
                width: source_layer.width,
                height: source_layer.height,
                index: 0,
                transform,
                opacity: source_layer.opacity,
                blend_mode: source_layer.blend_mode.clone(),
                visible: source_layer.visible,
                locked: source_layer.locked,
                layer_type: Some(source_layer.layer_type.clone()),
                shape_params: source_layer.shape_params.clone(),
                text_data: source_layer.text_data.clone(),
            },
            cx,
        )?;
        if alt {
            self.apply_doc_command(
                &payload.source_doc_id,
                Command::DeleteLayer {
                    id: core_id.to_string(),
                },
                cx,
            )?;
        }
        Ok(())
    }

    /// Layers-panel row drop: a same-document drag reorders to that exact row;
    /// a cross-document drag copies/moves into the active document.
    pub(crate) fn drop_layer_on_row(
        &mut self,
        payload: &LayerDragPayload,
        to_row: u64,
        alt: bool,
        cx: &mut Context<Self>,
    ) {
        let active = self.state.read(cx).document_id.clone();
        let same_document =
            active.is_none() || active.as_deref() == Some(payload.source_doc_id.as_str());
        if same_document {
            let from_row = payload
                .core_id
                .as_ref()
                .and_then(|core_id| {
                    self.state
                        .read(cx)
                        .layers
                        .iter()
                        .find(|row| row.core_id.as_deref() == Some(core_id.as_str()))
                        .map(|row| row.id)
                })
                .or(Some(payload.row_id));
            if let Some(from_row) = from_row {
                self.reorder_layer_rows(from_row, to_row, cx);
            }
        } else if let Some(active) = active {
            if payload.core_id.is_some() {
                self.apply_layer_drag(payload, LayerDropTarget::Panel { doc_id: active }, alt, cx);
            }
        }
    }

    /// Add decoded files as raster layers to `doc_id`. Every file must decode
    /// before the first layer is created, so a corrupt file leaves the document
    /// untouched.
    pub(crate) fn drop_files_as_layers(
        &mut self,
        paths: &[PathBuf],
        doc_id: &str,
        base: (f64, f64),
        center: bool,
        cx: &mut Context<Self>,
    ) -> Result<usize, String> {
        if paths.is_empty() {
            return Ok(0);
        }
        if self.doc_layer_count(doc_id)? + paths.len() > MAX_LAYERS {
            return Err(format!(
                "Adding {} files would exceed max {} layers",
                paths.len(),
                MAX_LAYERS
            ));
        }
        let file_io = self.ensure_file_io()?;
        let mut decoded = Vec::with_capacity(paths.len());
        for path in paths {
            let name = path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .filter(|name| !name.is_empty())
                .unwrap_or("Imported")
                .to_string();
            let image = crate::services::image_files::decode_import(file_io.as_ref(), path)
                .map_err(|error| format!("Failed to load {name}: {}", error.code()))?;
            decoded.push((name, image));
        }
        let mut added = 0;
        for (index, (name, image)) in decoded.into_iter().enumerate() {
            let mut x = base.0 + index as f64 * CASCADE_OFFSET_PX;
            let mut y = base.1 + index as f64 * CASCADE_OFFSET_PX;
            if center {
                x -= image.width as f64 / 2.0;
                y -= image.height as f64 / 2.0;
            }
            let id = next_layer_id();
            self.seed_layer(doc_id, &id, image.width, image.height, image.rgba)?;
            self.apply_doc_command(
                doc_id,
                Command::AddLayer {
                    id,
                    name,
                    width: image.width as f64,
                    height: image.height as f64,
                    index: 0,
                    x: Some(x),
                    y: Some(y),
                    layer_type: Some(LayerType::Raster),
                    shape_params: None,
                    text_data: None,
                },
                cx,
            )?;
            added += 1;
        }
        Ok(added)
    }

    /// Apply one command to `doc_id`, refreshing the panels and display only
    /// when that document is active. Unlike [`App::apply_core_command`] this is
    /// safe for a tab that is not the active document.
    pub(crate) fn apply_doc_command(
        &mut self,
        doc_id: &str,
        command: Command,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let added_layer = match &command {
            Command::AddLayer { id, .. } | Command::AddLayerWithState { id, .. } => {
                Some(id.clone())
            }
            _ => None,
        };
        let envelope = CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command,
        };
        self.sessions
            .get_mut(doc_id)
            .ok_or_else(|| "the document is not open".to_string())?
            .loaded_document_mut()
            .registry
            .apply_command(doc_id, envelope)
            .map_err(|error| format!("{}: {}", error.code, error.message))?;
        self.sessions
            .mark_dirty(doc_id)
            .map_err(|error| format!("dirty state failed: {error:?}"))?;
        let active = self
            .active_document
            .as_ref()
            .is_some_and(|document| document.doc_id == doc_id);
        if active {
            self.project_layers_with_selection(added_layer.as_deref(), cx);
            self.refresh_active_canvas(cx)?;
        } else {
            self.project_tabs(cx);
        }
        cx.notify();
        Ok(())
    }

    fn doc_layers(&self, doc_id: &str) -> Result<Vec<PtzLayer>, String> {
        self.sessions
            .get(doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .map(|content| content.layers.clone())
            .ok_or_else(|| "the document is not open".to_string())
    }

    fn doc_layer(&self, doc_id: &str, core_id: &str) -> Option<PtzLayer> {
        self.doc_layers(doc_id)
            .ok()?
            .into_iter()
            .find(|layer| layer.id == core_id)
    }

    pub(crate) fn doc_layer_count(&self, doc_id: &str) -> Result<usize, String> {
        Ok(self.doc_layers(doc_id)?.len())
    }

    fn doc_size(&self, doc_id: &str) -> Result<(f64, f64), String> {
        self.sessions
            .get(doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .map(|content| (content.width, content.height))
            .ok_or_else(|| "the document is not open".to_string())
    }
}
