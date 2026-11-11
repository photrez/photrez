//! Read-only accessors the mounted tests drive, for state the shell does not
//! otherwise expose.

use super::*;

impl App {
    #[cfg(test)]
    pub(crate) fn full_composites_for_test(&self) -> usize {
        self.full_composites.get()
    }

    #[cfg(test)]
    pub(crate) fn preview_pushes_for_test(&self) -> usize {
        self.preview_pushes.get()
    }

    #[cfg(test)]
    pub(crate) fn region_refreshes_for_test(&self) -> usize {
        self.region_refreshes.get()
    }

    #[cfg(test)]
    pub(crate) fn projection_pixel_copies_for_test(&self) -> usize {
        self.projection_pixel_copies.get()
    }

    #[cfg(test)]
    pub(crate) fn canvas_stale_for_test(&self) -> Option<bool> {
        self.active_document
            .as_ref()
            .map(|active| active.canvas_stale)
    }

    #[cfg(test)]
    pub(crate) fn projection_rect_for_test(&self) -> Option<(u32, u32, u32, u32)> {
        self.active_document
            .as_ref()?
            .canvas
            .projection_rect_for_test()
    }

    #[cfg(test)]
    pub(crate) fn projection_image_size_for_test(&self) -> Option<(u32, u32)> {
        self.active_document
            .as_ref()?
            .canvas
            .projection_image_size_for_test()
    }

    #[cfg(test)]
    pub(crate) fn projection_pixel_for_test(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        self.active_document
            .as_ref()?
            .canvas
            .projection_pixel_for_test(x, y)
    }

    #[cfg(test)]
    pub(crate) fn last_shape_kind_for_test(&self) -> Option<ShapeKind> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        content.layers.iter().find_map(|layer| {
            layer
                .shape_params
                .as_ref()
                .map(|params| params.kind.clone())
        })
    }

    #[cfg(test)]
    pub(crate) fn last_shape_stroke_for_test(&self) -> Option<(bool, String, f64)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        content.layers.iter().find_map(|layer| {
            layer.shape_params.as_ref().map(|params| {
                (
                    params.stroke.enabled,
                    params.stroke.color.clone(),
                    params.stroke.width,
                )
            })
        })
    }

    #[cfg(test)]
    pub(crate) fn active_composite_pixel_for_test(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let loaded = self.sessions.get(&doc_id)?.loaded_document();
        let content = loaded.content_snapshot()?;
        let composite = photrez_core::render::compositor::composite_layers_with_font_data(
            content,
            &loaded.registry,
            Some(crate::INTER),
        )
        .ok()?;
        if x >= composite.width || y >= composite.height {
            return None;
        }
        let index = (y as usize * composite.width as usize + x as usize) * 4;
        composite.pixels[index..index + 4].try_into().ok()
    }

    #[cfg(test)]
    pub(crate) fn active_document_dims_for_test(&self) -> Option<(u32, u32)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        Some((content.width as u32, content.height as u32))
    }

    #[cfg(test)]
    pub(crate) fn selection_drag_for_test(&self, cx: &gpui_kit::App) -> Option<[f64; 4]> {
        self.state.read(cx).selection_drag
    }

    #[cfg(test)]
    pub(crate) fn input_phase_for_test(&self) -> crate::input_policy::GesturePhase {
        self.input.phase()
    }

    #[cfg(test)]
    pub(crate) fn active_core_pixels_for_test(&self) -> Option<(u32, u32, Vec<u8>)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let loaded = self.sessions.get(&doc_id)?.loaded_document();
        let content = loaded.content_snapshot()?;
        let layer = content
            .layers
            .iter()
            .find(|layer| matches!(layer.layer_type, LayerType::Raster))?;
        let canonical = loaded.registry.get_layer(&doc_id, &layer.id)?;
        Some((canonical.width, canonical.height, canonical.pixels.clone()))
    }

    #[cfg(test)]
    pub(crate) fn active_core_layer_ids_for_test(&self) -> Vec<String> {
        let Some(doc_id) = self.active_document.as_ref().map(|d| d.doc_id.clone()) else {
            return Vec::new();
        };
        self.sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .map(|content| {
                content
                    .layers
                    .iter()
                    .map(|layer| layer.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
    #[cfg(test)]
    pub(crate) fn active_core_layer_origin_for_test(
        &self,
        layer_type: LayerType,
    ) -> Option<(f64, f64)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        content
            .layers
            .iter()
            .find(|layer| layer.layer_type == layer_type)
            .map(|layer| (layer.transform.x, layer.transform.y))
    }

    #[cfg(test)]
    pub(crate) fn core_layer_origins_for_test(&self, layer_type: LayerType) -> Vec<(f64, f64)> {
        let Some(doc_id) = self.active_document.as_ref().map(|d| d.doc_id.clone()) else {
            return Vec::new();
        };
        self.sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .map(|content| {
                content
                    .layers
                    .iter()
                    .filter(|layer| layer.layer_type == layer_type)
                    .map(|layer| (layer.transform.x, layer.transform.y))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn core_layer_pixels_for_test(&self, layer_id: &str) -> Option<(u32, u32, Vec<u8>)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let layer = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .registry
            .get_layer(&doc_id, layer_id)?;
        Some((layer.width, layer.height, layer.pixels.clone()))
    }

    #[cfg(test)]
    pub(crate) fn active_core_layer_count_for_test(&self) -> Option<usize> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let loaded = self.sessions.get(&doc_id)?.loaded_document();
        Some(loaded.content_snapshot()?.layers.len())
    }

    #[cfg(test)]
    pub(crate) fn active_core_layer_types_for_test(&self) -> Option<Vec<LayerType>> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let loaded = self.sessions.get(&doc_id)?.loaded_document();
        Some(
            loaded
                .content_snapshot()?
                .layers
                .iter()
                .map(|layer| layer.layer_type.clone())
                .collect(),
        )
    }

    #[cfg(test)]
    pub(crate) fn active_core_selection_for_test(&self) -> Option<SelectionState> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let loaded = self.sessions.get(&doc_id)?.loaded_document();
        loaded.content_snapshot()?.selection.clone()
    }

    #[cfg(test)]
    pub(crate) fn active_core_transform_for_test(&self) -> Option<(f64, f64)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let loaded = self.sessions.get(&doc_id)?.loaded_document();
        let content = loaded.content_snapshot()?;
        let layer = content
            .layers
            .iter()
            .find(|layer| matches!(layer.layer_type, LayerType::Raster))?;
        Some((layer.transform.x, layer.transform.y))
    }

    #[cfg(test)]
    pub(crate) fn active_core_history_version_for_test(&self) -> Option<u64> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        self.sessions
            .get(&doc_id)?
            .loaded_document()
            .registry
            .get_history_version(&doc_id)
    }

    #[cfg(test)]
    pub(crate) fn active_core_history_cursor_for_test(&self) -> Option<usize> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        self.sessions
            .get(&doc_id)?
            .loaded_document()
            .registry
            .get_history_cursor(&doc_id)
    }
}
