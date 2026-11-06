//! Projecting the core document into shell state: the layer rows, the selection
//! they carry, the group overlay frame and the thumbnail refresh.

use super::*;

impl App {
    /// Project the loaded core layers onto the shell's layer rows.
    ///
    /// Row ids are positions in this projection; the selection is preserved by
    /// core layer id so a reprojection after a command does not move the user
    /// to another layer.
    pub(crate) fn project_layers(&mut self, cx: &mut Context<Self>) {
        self.project_layers_with_selection(None, cx);
    }

    /// Project the layers and force `selected` as the selected core layer.
    /// Layer creation passes the new id so it becomes active, matching the
    /// editor; plain reprojection keeps the user's selection.
    pub(crate) fn project_layers_with_selection(
        &mut self,
        selected: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(doc_id) = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
        else {
            return;
        };
        let layers = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .map(|content| content.layers.clone())
            .unwrap_or_default();
        self.state.update(cx, |state, _| {
            let preserve_empty_selection = selected.is_none()
                && state.selected_layers.is_empty()
                && state.layers.iter().any(|row| {
                    row.core_id
                        .as_ref()
                        .is_some_and(|core_id| layers.iter().any(|layer| layer.id == *core_id))
                });
            let previous_selected: Vec<String> = state
                .layers
                .iter()
                .filter(|row| state.selected_layers.contains(&row.id))
                .filter_map(|row| row.core_id.clone())
                .collect();
            let selected_core = selected.map(str::to_string).or_else(|| {
                state
                    .layers
                    .iter()
                    .find(|row| row.id == state.selected_layer)
                    .and_then(|row| row.core_id.clone())
            });
            state.layers = layers
                .iter()
                .enumerate()
                .map(|(index, layer)| crate::state::LayerRow {
                    id: index as u64,
                    name: layer.name.clone(),
                    core_id: Some(layer.id.clone()),
                })
                .collect();
            state.layer_flags = layers
                .iter()
                .map(|layer| crate::state::LayerFlags {
                    visible: layer.visible,
                    locked: layer.locked,
                    has_adjustments: layer.has_adjustments.unwrap_or(false),
                    lock_transparency: layer.lock_transparency.unwrap_or(false),
                    lock_position: layer.lock_position.unwrap_or(false),
                    lock_rotation: layer.lock_rotation.unwrap_or(false),
                    is_background: layer.is_background.unwrap_or(false),
                })
                .collect();
            let selected_core_for_props = selected_core.clone();
            state.selected_layer = selected_core
                .and_then(|core_id| {
                    state
                        .layers
                        .iter()
                        .find(|row| row.core_id.as_deref() == Some(core_id.as_str()))
                        .map(|row| row.id)
                })
                .or_else(|| state.layers.first().map(|row| row.id))
                .unwrap_or(0);
            let mut mapped: Vec<u64> = if selected.is_some() {
                Vec::new()
            } else {
                state
                    .layers
                    .iter()
                    .filter(|row| {
                        row.core_id
                            .as_ref()
                            .is_some_and(|core_id| previous_selected.contains(core_id))
                    })
                    .map(|row| row.id)
                    .collect()
            };
            if !preserve_empty_selection && !mapped.contains(&state.selected_layer) {
                mapped.push(state.selected_layer);
            }
            state.selected_layers = mapped;
            state.multi_bounds = if state.selected_layers.len() > 1 {
                let mut bounds: Option<(f64, f64, f64, f64)> = None;
                for row in state
                    .layers
                    .iter()
                    .filter(|row| state.selected_layers.contains(&row.id))
                {
                    let Some(core_id) = row.core_id.as_deref() else {
                        continue;
                    };
                    let Some(layer) = layers.iter().find(|layer| layer.id == core_id) else {
                        continue;
                    };
                    let (frame_w, frame_h) = self::transform_geometry::layer_frame_size(layer);
                    let (x, y, w, h) =
                        self::transform_geometry::layer_aabb(&layer.transform, frame_w, frame_h);
                    bounds = Some(match bounds {
                        None => (x, y, w, h),
                        Some((bx, by, bw, bh)) => {
                            let min_x = bx.min(x);
                            let min_y = by.min(y);
                            let max_x = (bx + bw).max(x + w);
                            let max_y = (by + bh).max(y + h);
                            (min_x, min_y, max_x - min_x, max_y - min_y)
                        }
                    });
                }
                bounds
            } else {
                None
            };
            let projected = selected_core_for_props
                .as_deref()
                .and_then(|core_id| layers.iter().find(|layer| layer.id == core_id))
                .or_else(|| layers.first());
            if let Some(layer) = projected {
                state.layer_opacity = layer.opacity;
                state.layer_blend = crate::app::layers::blend_label(&layer.blend_mode).to_string();
                let adjustment = layer.basic_adjustment.clone().unwrap_or_default();
                state.adjustment_brightness = adjustment.brightness;
                state.adjustment_contrast = adjustment.contrast;
                state.adjustment_saturation = adjustment.saturation;
                state.active_layer_name = layer.name.clone();
                state.active_layer_width = layer.width.round() as u32;
                state.active_layer_height = layer.height.round() as u32;
                state.active_layer_has_pixels = layer.resource_id.is_some();
                state.selected_layer_type = Some(
                    match layer.layer_type {
                        LayerType::Raster => "raster",
                        LayerType::Shape => "shape",
                        LayerType::Text => "text",
                        _ => "other",
                    }
                    .to_string(),
                );
                state.selected_text_color = layer
                    .text_data
                    .as_ref()
                    .and_then(|data| crate::palette::parse_hex(&data.color));
                state.selected_shape = layer.shape_params.clone();
                if let Some(data) = layer.text_data.as_ref() {
                    state.selected_text_font_size = data.font_size;
                    state.selected_text_line_height = data.line_height;
                    state.selected_text_letter_spacing = data.letter_spacing;
                    state.selected_text_font_weight = data.font_weight;
                    state.selected_text_italic = matches!(
                        data.font_style,
                        photrez_core::document::layer_params::TextFontStyle::Italic
                    );
                    state.selected_text_align = match data.align {
                        photrez_core::document::layer_params::TextAlign::Left => {
                            crate::state::TextAlignTool::Left
                        }
                        photrez_core::document::layer_params::TextAlign::Center => {
                            crate::state::TextAlignTool::Center
                        }
                        photrez_core::document::layer_params::TextAlign::Right => {
                            crate::state::TextAlignTool::Right
                        }
                    };
                    state.selected_text_underline = data.underline.unwrap_or(false);
                    state.selected_text_strikethrough = data.strikethrough.unwrap_or(false);
                    state.selected_text_uppercase = data.uppercase.unwrap_or(false);
                    state.selected_text_stroke_width = data.stroke.width;
                    state.selected_text_stroke_align = data
                        .stroke
                        .align
                        .unwrap_or(photrez_core::document::layer_params::TextStrokeAlign::Outside);
                    state.selected_text_stroke_color =
                        crate::palette::parse_hex(&data.stroke.color);
                }
            }
        });
        self.refresh_layer_thumbs(&doc_id);
    }

    #[cfg(test)]
    pub(crate) fn layer_thumb_stats_for_test(&self) -> (usize, usize) {
        let with_image = self
            .layer_thumbs
            .iter()
            .filter(|thumb| thumb.is_some())
            .count();
        (self.layer_thumbs.len(), with_image)
    }

    /// The axis-aligned multi-selection frame, using live previews while a
    /// group transform drag is active.
    pub(super) fn group_overlay_frame(&self, cx: &Context<Self>) -> Option<TransformFrame> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        let state = self.state.read(cx);
        let selected_ids: Vec<String> = state
            .layers
            .iter()
            .filter(|row| state.selected_layers.contains(&row.id))
            .filter_map(|row| row.core_id.clone())
            .collect();
        if selected_ids.len() < 2 {
            return None;
        }
        let previews = self
            .group_transform_drag
            .as_ref()
            .map(|drag| drag.previews.clone())
            .unwrap_or_default();
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for id in &selected_ids {
            let layer = content.layers.iter().find(|layer| &layer.id == id)?;
            if !layer.visible {
                continue;
            }
            let transform = previews
                .iter()
                .find(|(preview_id, _)| preview_id == id)
                .map(|(_, transform)| transform.clone())
                .unwrap_or_else(|| layer.transform.clone());
            let (frame_w, frame_h) = crate::app::transform_geometry::layer_frame_size(layer);
            let (x, y, w, h) =
                crate::app::transform_geometry::layer_aabb(&transform, frame_w, frame_h);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x + w);
            max_y = max_y.max(y + h);
        }
        if !min_x.is_finite() || !min_y.is_finite() || max_x <= min_x || max_y <= min_y {
            return None;
        }
        let corners = [
            [min_x, min_y],
            [max_x, min_y],
            [max_x, max_y],
            [min_x, max_y],
        ];
        let mid_x = (min_x + max_x) / 2.0;
        let mid_y = (min_y + max_y) / 2.0;
        Some(TransformFrame {
            corners,
            handles: [
                corners[0],
                corners[1],
                corners[2],
                corners[3],
                [mid_x, min_y],
                [max_x, mid_y],
                [mid_x, max_y],
                [min_x, mid_y],
            ],
        })
    }

    /// Rebuild the 34x34 row thumbnails (raster layers only; shape/text rows
    /// keep the checker tile). Sampling is O(layers x 34x34).
    pub(super) fn refresh_layer_thumbs(&mut self, doc_id: &str) {
        let mut thumbs = Vec::new();
        if let Some(session) = self.sessions.get(doc_id) {
            let loaded = session.loaded_document();
            if let Some(content) = loaded.content_snapshot() {
                for layer in &content.layers {
                    let image = if matches!(layer.layer_type, LayerType::Raster) {
                        loaded
                            .registry
                            .get_layer(doc_id, &layer.id)
                            .and_then(|canonical| {
                                thumbnail_image(
                                    canonical.width,
                                    canonical.height,
                                    &canonical.pixels,
                                    34,
                                )
                            })
                    } else {
                        None
                    };
                    thumbs.push(image);
                }
            }
        }
        self.layer_thumbs = thumbs;
    }
}
