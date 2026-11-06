//! Cursor-to-layer hit testing for the Move tool: which layer a document
//! point lands on, which layer the transform frame describes, and the
//! auto-select click that turns a hit into a selection.

use gpui_kit::Context;
use photrez_core::document::content::LayerType;

use crate::app::transform_geometry::{document_to_layer_local, layer_corners, Point};

use super::App;

impl App {
    /// The selected layer regardless of its type, for the transform frame.
    pub(crate) fn selected_layer_for_cursor(
        &self,
        cx: &gpui_kit::App,
    ) -> Result<(String, photrez_core::document::content::PtzLayer), String> {
        let doc_id = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
            .ok_or_else(|| "no active document".to_string())?;
        let session = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "active session is missing".to_string())?;
        let content = session
            .loaded_document()
            .content_snapshot()
            .ok_or_else(|| "loaded content is missing".to_string())?;
        let state = self.state.read(cx);
        if state.selected_layers.is_empty() && state.layers.iter().any(|row| row.core_id.is_some())
        {
            return Err("no layer is selected".to_string());
        }
        let selected = state
            .layers
            .iter()
            .find(|row| row.id == state.selected_layer)
            .and_then(|row| row.core_id.clone());
        let layer = selected
            .as_deref()
            .and_then(|core_id| content.layers.iter().find(|layer| layer.id == core_id))
            .or_else(|| content.layers.first())
            .ok_or_else(|| "the document has no layers".to_string())?
            .clone();
        Ok((doc_id, layer))
    }

    /// Topmost visible, non-background layer under a document-space point.
    /// The layer hit test: text/shape layers are hit by their
    /// whole box, raster layers by in-box alpha above 10%.
    pub(crate) fn layer_hit_at(&self, doc_x: f64, doc_y: f64) -> Option<String> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let session = self.sessions.get(&doc_id)?;
        let loaded = session.loaded_document();
        let content = loaded.content_snapshot()?;
        for layer in &content.layers {
            if !layer.visible || layer.opacity <= 0.0 || layer.is_background == Some(true) {
                continue;
            }
            if layer.width <= 0.0 || layer.height <= 0.0 {
                continue;
            }
            let (frame_w, frame_h) = crate::app::transform_geometry::layer_frame_size(layer);
            let corners = layer_corners(&layer.transform, frame_w, frame_h);
            if !point_in_polygon(doc_x, doc_y, &corners) {
                continue;
            }
            if matches!(layer.layer_type, LayerType::Text | LayerType::Shape) {
                return Some(layer.id.clone());
            }
            let local =
                document_to_layer_local(doc_x, doc_y, &layer.transform, layer.width, layer.height);
            if !local.x.is_finite() || !local.y.is_finite() || local.x < 0.0 || local.y < 0.0 {
                continue;
            }
            let (lx, ly) = (local.x.floor() as usize, local.y.floor() as usize);
            let Some(pixels) = loaded.registry.get_layer(&doc_id, &layer.id) else {
                continue;
            };
            if lx >= pixels.width as usize || ly >= pixels.height as usize {
                continue;
            }
            let index = (ly * pixels.width as usize + lx) * 4;
            let alpha = pixels.pixels.get(index + 3).copied().unwrap_or(0);
            if f32::from(alpha) / 255.0 < 0.1 {
                continue;
            }
            return Some(layer.id.clone());
        }
        None
    }

    /// Move-tool auto-select click. Returns
    /// true when the click changed the selection, in which case no transform
    /// drag starts; clicking the already-active layer keeps the native
    /// drag-to-move interaction.
    pub(crate) fn move_auto_select_click(
        &mut self,
        doc_x: f64,
        doc_y: f64,
        modify: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(core_id) = self.layer_hit_at(doc_x, doc_y) else {
            return false;
        };
        let (row_id, already_selected, already_active) = {
            let state = self.state.read(cx);
            let row_id = state
                .layers
                .iter()
                .find(|row| row.core_id.as_deref() == Some(core_id.as_str()))
                .map(|row| row.id);
            let already_selected = row_id.is_some_and(|row| state.selected_layers.contains(&row));
            (
                row_id,
                already_selected,
                row_id == Some(state.selected_layer),
            )
        };
        let Some(row_id) = row_id else {
            return false;
        };
        if modify {
            self.state.update(cx, |state, _| {
                if let Some(index) = state.selected_layers.iter().position(|id| *id == row_id) {
                    state.selected_layers.remove(index);
                } else {
                    state.selected_layers.push(row_id);
                }
                state.selected_layer = row_id;
            });
            cx.notify();
            return true;
        }
        if already_selected && already_active {
            return false;
        }
        self.state.update(cx, |state, _| {
            state.selected_layer = row_id;
            if !already_selected {
                state.selected_layers = vec![row_id];
            }
        });
        cx.notify();
        true
    }
}

/// Even-odd containment test used by the layer hit test.
fn point_in_polygon(x: f64, y: f64, corners: &[Point; 4]) -> bool {
    let mut inside = false;
    let mut j = corners.len() - 1;
    for i in 0..corners.len() {
        let (xi, yi) = (corners[i].x, corners[i].y);
        let (xj, yj) = (corners[j].x, corners[j].y);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}
