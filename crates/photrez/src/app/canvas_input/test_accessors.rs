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
}
