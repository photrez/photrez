//! Multi-selection layout commands: align to the canvas and distribute along one
//! axis. Both build a list of edits and commit them through one
//! `TransformLayer` command per moved layer.

use gpui_kit::Context;
use photrez_core::protocol::{Command, TransformPatch};
use photrez_core::ptz::content::Transform2D;

use super::transform_move::LayerBounds;
use super::App;

impl App {
    /// Align the selected layers to the canvas (Properties multi-selection).
    /// `mode`: 0 left, 1 centre horizontally, 2 right, 3 top, 4 centre
    /// vertically, 5 bottom. Locked / position-locked / background members
    /// never move, mirroring `alignTransformEdits`.
    pub(crate) fn align_selected_layers(
        &mut self,
        mode: u8,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc_id = self.active_doc_id()?;
        let (doc_w, doc_h) = self.active_document_size()?;
        let layers = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .map(|content| content.layers.clone())
            .ok_or_else(|| "loaded content is missing".to_string())?;
        let ids = self.selected_core_ids(cx);
        let mut edits = Vec::new();
        for id in ids {
            let Some(layer) = layers.iter().find(|layer| layer.id == id) else {
                continue;
            };
            if layer.locked
                || layer.lock_position == Some(true)
                || layer.is_background == Some(true)
            {
                continue;
            }
            // Align keeps the document-sized layer box; shape-rect alignment is
            // a separate, not-yet-implemented behaviour.
            let layer_w = (layer.width * layer.transform.scale_x).round();
            let layer_h = (layer.height * layer.transform.scale_y).round();
            let (mut x, mut y) = (layer.transform.x, layer.transform.y);
            match mode {
                0 => x = 0.0,
                1 => x = ((doc_w - layer_w) / 2.0).round(),
                2 => x = doc_w - layer_w,
                3 => y = 0.0,
                4 => y = ((doc_h - layer_h) / 2.0).round(),
                5 => y = doc_h - layer_h,
                _ => {}
            }
            if x != layer.transform.x || y != layer.transform.y {
                edits.push((layer.id.clone(), x, y, layer.transform.clone()));
            }
        }
        self.apply_transform_edits(&doc_id, edits, cx)
    }

    /// Evenly distribute the selected layers' spacing along one axis. Keeps
    /// the outermost members fixed.
    pub(crate) fn distribute_selected_layers(
        &mut self,
        horizontal: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc_id = self.active_doc_id()?;
        let layers = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .map(|content| content.layers.clone())
            .ok_or_else(|| "loaded content is missing".to_string())?;
        let mut members: Vec<(String, Transform2D, LayerBounds)> = Vec::new();
        for id in self.selected_core_ids(cx) {
            let Some(layer) = layers.iter().find(|layer| layer.id == id) else {
                continue;
            };
            if layer.locked
                || layer.lock_position == Some(true)
                || layer.is_background == Some(true)
            {
                continue;
            }
            let aabb = crate::app::transform_geometry::layer_aabb(
                &layer.transform,
                layer.width,
                layer.height,
            );
            members.push((layer.id.clone(), layer.transform.clone(), aabb));
        }
        if members.len() < 3 {
            return Ok(());
        }
        members.sort_by(|a, b| {
            if horizontal {
                a.2 .0.total_cmp(&b.2 .0)
            } else {
                a.2 .1.total_cmp(&b.2 .1)
            }
        });
        let first = members[0].2;
        let last = members[members.len() - 1].2;
        let total_span = if horizontal {
            last.0 + last.2 - first.0
        } else {
            last.1 + last.3 - first.1
        };
        let total_extent: f64 = members
            .iter()
            .map(|(_, _, aabb)| if horizontal { aabb.2 } else { aabb.3 })
            .sum();
        let gap = (total_span - total_extent) / (members.len() as f64 - 1.0);
        let mut current = if horizontal { first.0 } else { first.1 };
        let mut edits = Vec::new();
        for (id, start, aabb) in members {
            let delta = if horizontal {
                (current - aabb.0).round()
            } else {
                (current - aabb.1).round()
            };
            current += (if horizontal { aabb.2 } else { aabb.3 }) + gap;
            if delta == 0.0 {
                continue;
            }
            let (x, y) = if horizontal {
                (start.x + delta, start.y)
            } else {
                (start.x, start.y + delta)
            };
            edits.push((id, x, y, start));
        }
        self.apply_transform_edits(&doc_id, edits, cx)
    }

    fn selected_core_ids(&self, cx: &Context<Self>) -> Vec<String> {
        let state = self.state.read(cx);
        state
            .layers
            .iter()
            .filter(|row| state.selected_layers.contains(&row.id))
            .filter_map(|row| row.core_id.clone())
            .collect()
    }

    fn apply_transform_edits(
        &mut self,
        doc_id: &str,
        edits: Vec<(String, f64, f64, Transform2D)>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        for (id, x, y, start) in edits {
            self.apply_core_command(
                doc_id,
                Command::TransformLayer {
                    id,
                    transform: TransformPatch {
                        x,
                        y,
                        scale_x: start.scale_x,
                        scale_y: start.scale_y,
                        rotation: start.rotation,
                        flip_h: Some(start.flip_h),
                        flip_v: Some(start.flip_v),
                    },
                },
                true,
                cx,
            )?;
        }
        Ok(())
    }
}
