//! Structural layer operations that need pixels, not only metadata.
//!
//! The core metadata commands create/remove layer nodes; the pixel store owns
//! the buffers. These methods composite or copy the source pixels first, seed
//! the new layer, then apply the single metadata history entry. Buffers of
//! removed layers are retained so undo can re-link them; a later cleanup can
//! drop buffers that are unreachable on both history branches.

use gpui_kit::Context;
use photrez_core::protocol::{Command, CommandEnvelope, LockKind, CONTRACT_VERSION};
use photrez_core::ptz::content::{BlendMode, LayerType, PtzDocumentContent, PtzLayer};
use photrez_core::render::compositor::{composite_layers_with_font_data, CompositedRgba};

use super::input::next_layer_id;
use super::App;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct DuplicateLayerAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct NewLayerAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct SelectAllLayersAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct MergeLayerDownAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct FlattenDocumentAction;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct StampVisibleAction;

impl App {
    pub(crate) fn duplicate_layer_action(
        &mut self,
        _: &DuplicateLayerAction,
        cx: &mut Context<Self>,
    ) {
        let result = self.duplicate_selected_layer(cx);
        self.report_layer_op(result, cx);
    }

    pub(crate) fn merge_layer_down_action(
        &mut self,
        _: &MergeLayerDownAction,
        cx: &mut Context<Self>,
    ) {
        let result = self.merge_selected_layer_down(cx);
        self.report_layer_op(result, cx);
    }

    pub(crate) fn stamp_visible_action(&mut self, _: &StampVisibleAction, cx: &mut Context<Self>) {
        let result = self.stamp_visible(cx);
        self.report_layer_op(result, cx);
    }

    pub(crate) fn new_layer_action(&mut self, _: &NewLayerAction, cx: &mut Context<Self>) {
        self.add_empty_layer(cx);
    }

    pub(crate) fn select_all_layers_action(
        &mut self,
        _: &SelectAllLayersAction,
        cx: &mut Context<Self>,
    ) {
        self.select_all_layers(cx);
    }

    /// Composite every visible layer into a new raster layer at the top while
    /// keeping the sources (`handleStampVisible`).
    pub(crate) fn stamp_visible(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let (doc_id, content) = {
            let doc_id = self.active_doc_id()?;
            let content = self.content(&doc_id)?;
            (doc_id, content)
        };
        let visible: Vec<PtzLayer> = content
            .layers
            .iter()
            .filter(|layer| layer.visible)
            .cloned()
            .collect();
        if visible.is_empty() {
            return Err("there are no visible layers to stamp".to_string());
        }
        let composite = self.composite_layers(&doc_id, &content, visible)?;
        let new_id = next_layer_id();
        self.seed_layer(
            &doc_id,
            &new_id,
            composite.width,
            composite.height,
            composite.pixels,
        )?;
        self.apply_structural(
            &doc_id,
            Command::AddLayer {
                id: new_id.clone(),
                name: "Stamp Visible".to_string(),
                width: content.width,
                height: content.height,
                index: 0,
                x: None,
                y: None,
                layer_type: Some(LayerType::Raster),
                shape_params: None,
                text_data: None,
            },
            cx,
        )?;
        self.state.update(cx, |state, _| {
            if let Some(row) = state
                .layers
                .iter()
                .find(|row| row.core_id.as_deref() == Some(new_id.as_str()))
            {
                state.selected_layer = row.id;
                state.selected_layers = vec![row.id];
            }
        });
        cx.notify();
        Ok(())
    }

    pub(crate) fn flatten_document_action(
        &mut self,
        _: &FlattenDocumentAction,
        cx: &mut Context<Self>,
    ) {
        let result = self.flatten_document(cx);
        self.report_layer_op(result, cx);
    }

    fn report_layer_op(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(error), cx),
        }
        cx.notify();
    }

    pub(crate) fn report_layer_op_for_view(
        &mut self,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        self.report_layer_op(result, cx);
    }

    /// Duplicate the selected layer, including its pixels for a raster layer.
    pub(crate) fn duplicate_selected_layer(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.duplicate_selected_layer_id(cx).map(|_| ())
    }

    /// Duplicate and return the new layer id, selecting the duplicate.
    pub(crate) fn duplicate_selected_layer_id(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        let (doc_id, layer) = self.selected_layer(cx)?;
        let new_id = next_layer_id();
        let seed = if matches!(layer.layer_type, LayerType::Raster) {
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
            Some((canonical.width, canonical.height, canonical.pixels.clone()))
        } else {
            None
        };
        if let Some((width, height, pixels)) = seed {
            self.seed_layer(&doc_id, &new_id, width, height, pixels)?;
        }
        self.apply_structural(
            &doc_id,
            Command::DuplicateLayer {
                id: layer.id,
                new_id: new_id.clone(),
            },
            cx,
        )?;
        self.state.update(cx, |state, _| {
            if let Some(row) = state
                .layers
                .iter()
                .find(|row| row.core_id.as_deref() == Some(new_id.as_str()))
            {
                state.selected_layer = row.id;
                state.selected_layers = vec![row.id];
            }
        });
        cx.notify();
        Ok(new_id)
    }

    /// Reset the selected layer's transform to identity at the current origin.
    pub(crate) fn reset_selected_transform(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer_for_cursor(cx)?;
        self.apply_core_command(
            &doc_id,
            Command::TransformLayer {
                id: layer.id,
                transform: photrez_core::protocol::TransformPatch {
                    x: 0.0,
                    y: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    flip_h: Some(false),
                    flip_v: Some(false),
                },
            },
            true,
            cx,
        )
    }

    /// Rasterize a parametric layer to a plain raster (`RasterizeLayer`).
    ///
    /// Native-only improvement over the frozen panel: reachable from the
    /// text-layer typography row in Properties (`prop-rasterize`), where the
    /// mounted behaviour contract pins it. The frozen panel has no such
    /// button; the improvement is recorded in the ledger.
    ///
    /// Parametric layers are metadata-only in the store, so the host renders
    /// the layer's bitmap first and hands it to `RasterizeLayer` — the
    /// command materializes it as the rasterized layer's canonical buffer
    /// (the oracle's text layers always carry their bitmap, which is why its
    /// rasterize composites).
    pub(crate) fn rasterize_selected_layer(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        use photrez_core::protocol::RasterizePixels;
        let (doc_id, layer) = self.selected_layer(cx)?;
        let rendered = match layer.layer_type {
            LayerType::Text => layer.text_data.as_ref().and_then(|data| {
                photrez_core::render::rasterize_text_preview(data, Some(crate::INTER))
            }),
            LayerType::Shape => layer
                .shape_params
                .as_ref()
                .and_then(photrez_core::render::rasterize_shape_preview),
            _ => None,
        };
        // A parametric layer whose features the rasterizer cannot render
        // (decorated/styled text outside the core subset) refuses the
        // rasterize with a typed error instead of producing a Raster layer
        // the compositor cannot paint.
        if rendered.is_none() {
            return Err("rasterize: the layer's content cannot be rendered to pixels".to_string());
        }
        let (width, height, rgba) = rendered.expect("rendered above");
        self.apply_core_command(
            &doc_id,
            Command::RasterizeLayer {
                id: layer.id,
                pixels: Some(RasterizePixels {
                    width: f64::from(width),
                    height: f64::from(height),
                    rgba,
                }),
            },
            true,
            cx,
        )
    }

    pub(crate) fn selected_layer_count(&self, cx: &Context<Self>) -> usize {
        self.state.read(cx).selected_layers.len()
    }

    /// Select every layer row (Ctrl+Alt+A / Layer menu).
    pub(crate) fn select_all_layers(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.selected_layers = state.layers.iter().map(|row| row.id).collect();
            if let Some(first) = state.layers.first() {
                state.selected_layer = first.id;
            }
        });
        cx.notify();
    }

    /// Merge every selected layer into one raster at the highest selected
    /// position (`Command::MergeSelected`), seeding the composited pixels.
    pub(crate) fn merge_selected_layers(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let (doc_id, content, selected) = {
            let (doc_id, _) = self.selected_layer(cx)?;
            let content = self.content(&doc_id)?;
            let state = self.state.read(cx);
            let ids: Vec<String> = state
                .layers
                .iter()
                .filter(|row| state.selected_layers.contains(&row.id))
                .filter_map(|row| row.core_id.clone())
                .collect();
            let selected: Vec<PtzLayer> = content
                .layers
                .iter()
                .filter(|layer| ids.contains(&layer.id))
                .cloned()
                .collect();
            (doc_id, content, selected)
        };
        if selected.len() < 2 {
            return Err("select at least two layers to merge".to_string());
        }
        let composite = self.composite_layers(&doc_id, &content, selected.clone())?;
        let new_id = next_layer_id();
        self.seed_layer(
            &doc_id,
            &new_id,
            composite.width,
            composite.height,
            composite.pixels,
        )?;
        let ids: Vec<String> = selected.into_iter().map(|layer| layer.id).collect();
        self.apply_structural(
            &doc_id,
            Command::MergeSelected {
                ids: ids.clone(),
                merged_id: new_id.clone(),
            },
            cx,
        )?;
        self.state.update(cx, |state, _| {
            if let Some(row) = state
                .layers
                .iter()
                .find(|row| row.core_id.as_deref() == Some(new_id.as_str()))
            {
                state.selected_layer = row.id;
                state.selected_layers = vec![row.id];
            }
        });
        cx.notify();
        Ok(())
    }

    /// Merge the selected layer into the one directly below it.
    pub(crate) fn merge_selected_layer_down(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_id, selected) = self.selected_layer(cx)?;
        let content = self.content(&doc_id)?;
        let index = content
            .layers
            .iter()
            .position(|layer| layer.id == selected.id)
            .ok_or_else(|| "the selected layer is missing".to_string())?;
        let below = content
            .layers
            .get(index + 1)
            .ok_or_else(|| "the selected layer has no layer below it".to_string())?
            .clone();
        let composite = self.composite_layers(&doc_id, &content, vec![selected.clone(), below])?;
        let new_id = next_layer_id();
        self.seed_layer(
            &doc_id,
            &new_id,
            composite.width,
            composite.height,
            composite.pixels,
        )?;
        self.apply_structural(
            &doc_id,
            Command::MergeDown {
                id: selected.id,
                merged_id: new_id,
            },
            cx,
        )
    }

    /// Flatten every layer into one background raster.
    pub(crate) fn flatten_document(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let doc_id = self.active_doc_id()?;
        let content = self.content(&doc_id)?;
        if content.layers.len() < 2 {
            return Err("there is nothing to flatten".to_string());
        }
        let composite = self.composite_layers(&doc_id, &content, content.layers.clone())?;
        let new_id = next_layer_id();
        self.seed_layer(
            &doc_id,
            &new_id,
            composite.width,
            composite.height,
            composite.pixels,
        )?;
        self.apply_structural(&doc_id, Command::Flatten { merged_id: new_id }, cx)
    }

    fn content(&self, doc_id: &str) -> Result<PtzDocumentContent, String> {
        self.sessions
            .get(doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .cloned()
            .ok_or_else(|| "loaded content is missing".to_string())
    }

    fn composite_layers(
        &self,
        doc_id: &str,
        content: &PtzDocumentContent,
        layers: Vec<PtzLayer>,
    ) -> Result<CompositedRgba, String> {
        let mut subset = content.clone();
        subset.layers = layers;
        let loaded = self
            .sessions
            .get(doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document();
        composite_layers_with_font_data(&subset, &loaded.registry, Some(crate::INTER))
            .map_err(|error| error.to_string())
    }

    pub(crate) fn seed_layer(
        &mut self,
        doc_id: &str,
        layer_id: &str,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    ) -> Result<(), String> {
        self.sessions
            .get_mut(doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document_mut()
            .registry
            .add_layer(doc_id, layer_id, width, height, pixels)
            .map_err(|error| format!("cannot seed the new layer: {error}"))
    }

    fn apply_structural(
        &mut self,
        doc_id: &str,
        command: Command,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let envelope = CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command,
        };
        let applied = self
            .sessions
            .get_mut(doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document_mut()
            .registry
            .apply_command(doc_id, envelope);
        if let Err(error) = applied {
            return Err(format!("{}: {}", error.code, error.message));
        }
        self.sessions
            .mark_dirty(doc_id)
            .map_err(|error| format!("dirty state failed: {error:?}"))?;
        self.project_layers(cx);
        self.refresh_active_canvas(cx)?;
        cx.notify();
        Ok(())
    }
}

/// Panel-driven layer operations: the row eye toggle and the action footer.
impl App {
    fn layer_by_core_id(&self, core_id: &str) -> Result<(String, PtzLayer), String> {
        let doc_id = self.active_doc_id()?;
        let content = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?
            .loaded_document()
            .content_snapshot()
            .ok_or_else(|| "loaded content is missing".to_string())?;
        let layer = content
            .layers
            .iter()
            .find(|layer| layer.id == core_id)
            .ok_or_else(|| "the layer is missing from the document".to_string())?
            .clone();
        Ok((doc_id, layer))
    }

    pub(crate) fn toggle_layer_visibility(&mut self, core_id: String, cx: &mut Context<Self>) {
        let result = (|| {
            let (doc_id, layer) = self.layer_by_core_id(&core_id)?;
            self.apply_core_command(
                &doc_id,
                Command::SetVisible {
                    id: layer.id,
                    visible: !layer.visible,
                },
                true,
                cx,
            )
        })();
        self.report_layer_op(result, cx);
    }

    pub(crate) fn toggle_layer_locked(&mut self, core_id: String, cx: &mut Context<Self>) {
        self.toggle_layer_lock(core_id, LockKind::Base, cx);
    }

    /// Flip one of the four lock flags on a layer (`LayersPanel.tsx:474-533`).
    pub(crate) fn toggle_layer_lock(
        &mut self,
        core_id: String,
        kind: LockKind,
        cx: &mut Context<Self>,
    ) {
        let result = (|| {
            let (doc_id, layer) = self.layer_by_core_id(&core_id)?;
            let locked = match kind {
                LockKind::Base => !layer.locked,
                LockKind::Transparency => !layer.lock_transparency.unwrap_or(false),
                LockKind::Position => !layer.lock_position.unwrap_or(false),
                LockKind::Rotation => !layer.lock_rotation.unwrap_or(false),
            };
            self.apply_core_command(
                &doc_id,
                Command::SetLocked {
                    id: layer.id,
                    kind,
                    locked,
                },
                true,
                cx,
            )
        })();
        self.report_layer_op(result, cx);
    }

    /// Add an empty transparent raster layer at the top and select it.
    pub(crate) fn add_empty_layer(&mut self, cx: &mut Context<Self>) {
        let result = (|| {
            let (doc_w, doc_h) = self.active_document_size()?;
            let width = doc_w.max(1.0) as u32;
            let height = doc_h.max(1.0) as u32;
            let doc_id = self.active_doc_id()?;
            let id = next_layer_id();
            self.seed_layer(
                &doc_id,
                &id,
                width,
                height,
                vec![0; width as usize * height as usize * 4],
            )?;
            self.apply_structural(
                &doc_id,
                Command::AddLayer {
                    id: id.clone(),
                    name: "Layer".to_string(),
                    width: doc_w,
                    height: doc_h,
                    index: 0,
                    x: None,
                    y: None,
                    layer_type: Some(LayerType::Raster),
                    shape_params: None,
                    text_data: None,
                },
                cx,
            )?;
            self.state.update(cx, |state, _| {
                if let Some(row) = state
                    .layers
                    .iter()
                    .find(|row| row.core_id.as_deref() == Some(id.as_str()))
                {
                    state.selected_layer = row.id;
                }
            });
            Ok(())
        })();
        self.report_layer_op(result, cx);
    }

    pub(crate) fn delete_selected_layer(&mut self, cx: &mut Context<Self>) {
        let result = (|| {
            let (doc_id, layer) = self.selected_layer(cx)?;
            self.apply_core_command(&doc_id, Command::DeleteLayer { id: layer.id }, true, cx)
        })();
        self.report_layer_op(result, cx);
    }

    /// Step the selected layer's opacity by `delta`, clamped to `0..=1`.
    pub(crate) fn adjust_selected_opacity(&mut self, delta: f64, cx: &mut Context<Self>) {
        let result = (|| {
            let (doc_id, layer) = self.selected_layer(cx)?;
            let opacity = (layer.opacity + delta).clamp(0.0, 1.0);
            self.apply_core_command(
                &doc_id,
                Command::SetOpacity {
                    id: layer.id,
                    opacity,
                },
                true,
                cx,
            )
        })();
        self.report_layer_op(result, cx);
    }

    /// Cycle the selected layer through the engine's blend modes.
    pub(crate) fn cycle_selected_blend(&mut self, cx: &mut Context<Self>) {
        let result = (|| {
            let (doc_id, layer) = self.selected_layer(cx)?;
            self.apply_core_command(
                &doc_id,
                Command::SetBlendMode {
                    id: layer.id,
                    mode: next_blend_mode(&layer.blend_mode),
                },
                true,
                cx,
            )
        })();
        self.report_layer_op(result, cx);
    }
}

pub(crate) fn blend_label(mode: &BlendMode) -> &'static str {
    match mode {
        BlendMode::Normal => "Normal",
        BlendMode::Multiply => "Multiply",
        BlendMode::Screen => "Screen",
        BlendMode::Overlay => "Overlay",
        BlendMode::Darken => "Darken",
        BlendMode::Lighten => "Lighten",
        BlendMode::ColorDodge => "Color Dodge",
        BlendMode::ColorBurn => "Color Burn",
        BlendMode::SoftLight => "Soft Light",
        BlendMode::HardLight => "Hard Light",
        BlendMode::Difference => "Difference",
        BlendMode::Exclusion => "Exclusion",
    }
}

fn next_blend_mode(mode: &BlendMode) -> BlendMode {
    match mode {
        BlendMode::Normal => BlendMode::Multiply,
        BlendMode::Multiply => BlendMode::Screen,
        BlendMode::Screen => BlendMode::Overlay,
        BlendMode::Overlay => BlendMode::Darken,
        BlendMode::Darken => BlendMode::Lighten,
        BlendMode::Lighten => BlendMode::ColorDodge,
        BlendMode::ColorDodge => BlendMode::ColorBurn,
        BlendMode::ColorBurn => BlendMode::SoftLight,
        BlendMode::SoftLight => BlendMode::HardLight,
        BlendMode::HardLight => BlendMode::Difference,
        BlendMode::Difference => BlendMode::Exclusion,
        BlendMode::Exclusion => BlendMode::Normal,
    }
}
