// SPDX-License-Identifier: AGPL-3.0-or-later
// Per-document replica of the persisted `.ptz` content, kept in sync with
// engine layer edits via reconciliation (not re-built from scratch each edit).
// It is a DERIVED in-memory copy, never a second authority: the engine owns
// the live layer set, this replica mirrors it.
//
// Depends on `engine::projection` (the merge step); `engine::projection` never
// depends on the replica (see `crates/core/README.md`).

use crate::document::content::{LayerType, PtzDocumentContent, PtzLayer, Transform2D};
use crate::document::layer::LayerMeta;
use crate::engine::projection::merge_into_layer;
use std::collections::HashMap;

/// Holds a full `PtzDocumentContent` copy plus tombstones for deleted layers
/// and an `incomplete` flag that marks engine-minted layers whose 14
/// persisted-only fields cannot be reconstructed from the render layer.
#[derive(Debug)]
pub struct PtzReplica {
    pub doc: PtzDocumentContent,
    // Tombstones for deleted layers; `pub(crate)` so intra-crate replica tests can
    // assert on delete/undo restore behavior.
    pub(crate) removed: HashMap<String, PtzLayer>,
    pub incomplete: bool,
}

impl PtzReplica {
    /// Wrap a freshly pushed document. Tombstones empty, `incomplete` false: a
    /// full push discards any prior delete history and unknown-layer marks.
    pub fn new(doc: PtzDocumentContent) -> Self {
        PtzReplica {
            doc,
            removed: HashMap::new(),
            incomplete: false,
        }
    }

    /// Reconcile the replica against the current engine layer set. Engine order IS
    /// persisted order, so `doc.layers` is rebuilt in a single pass over `current`:
    ///  - id in `doc.layers` -> merge render edits onto the existing layer,
    ///  - id in a tombstone -> restore it (merged with current render state),
    ///  - id in neither -> a complete typed Shape/Text layer is reconstructed;
    ///    other engine-minted layers are skipped and set `incomplete`.
    ///
    /// After the pass, any `doc.layers` entry whose id is absent from `current`
    /// (deleted) is moved to a tombstone so an undo-of-delete restores it with all
    /// fields intact. O(layers), metadata-only, no serialization.
    pub fn reconcile<'a, I: Iterator<Item = &'a LayerMeta>>(&mut self, current: I) {
        // Re-index existing persisted layers by id; matched ones are drained as they
        // are rebuilt, so whatever remains at the end is a deleted layer.
        let mut existing: HashMap<String, PtzLayer> = std::mem::take(&mut self.doc.layers)
            .into_iter()
            .map(|l| (l.id.clone(), l))
            .collect();

        let mut rebuilt: Vec<PtzLayer> = Vec::new();
        for r in current {
            if let Some(base) = existing.remove(&r.id) {
                rebuilt.push(merge_into_layer(&base, r));
            } else if let Some(tomb) = self.removed.remove(&r.id) {
                // Ids are host-minted random values; a restored tombstone reuses
                // its original id, which is benign-by-design (the restored layer is
                // the same logical layer the engine deleted). Id collision with an
                // unrelated fresh layer is negligible.
                rebuilt.push(merge_into_layer(&tomb, r));
            } else if let Some(layer) = typed_layer(r) {
                rebuilt.push(layer);
            } else {
                // Engine-minted layer: cannot reconstruct persisted-only fields.
                // Leave it out and flag the replica incomplete (sticky).
                self.incomplete = true;
                continue;
            }
        }

        // Any layer left in `existing` was removed from the engine set: move it to a
        // tombstone so a future undo can restore its full fields.
        for (id, layer) in existing {
            self.removed.insert(id, layer);
        }

        self.doc.layers = rebuilt;
    }
}

fn typed_layer(render: &LayerMeta) -> Option<PtzLayer> {
    let layer_type = render.layer_type.clone()?;
    let blend_mode = render.blend_mode.clone()?;
    let width = render.width?;
    let height = render.height?;
    if render.id.is_empty()
        || !width.is_finite()
        || !height.is_finite()
        || width < 0.0
        || height < 0.0
    {
        return None;
    }

    let (shape_params, text_data) = match layer_type {
        // Engine-minted raster layers (merge/flatten/duplicate) carry no typed
        // payload; omitting them from the replica would hide them from save and
        // the compositor.
        LayerType::Raster if render.shape_params.is_none() && render.text_data.is_none() => {
            (None, None)
        }
        LayerType::Shape if render.shape_params.is_some() && render.text_data.is_none() => {
            (render.shape_params.clone(), None)
        }
        LayerType::Text if render.text_data.is_some() && render.shape_params.is_none() => {
            (None, render.text_data.clone())
        }
        _ => return None,
    };

    Some(PtzLayer {
        id: render.id.clone(),
        name: render.name.clone(),
        layer_type,
        visible: render.visible,
        opacity: render.opacity,
        locked: render.locked.unwrap_or(false),
        is_background: render.is_background,
        lock_transparency: render.lock_transparency,
        lock_position: render.lock_position,
        lock_rotation: render.lock_rotation,
        has_adjustments: render.has_adjustments,
        basic_adjustment: render.basic_adjustment.clone(),
        resource_id: (render.resource_id != 0).then_some(render.resource_id),
        blend_mode,
        transform: Transform2D {
            x: render.x,
            y: render.y,
            scale_x: render.scale_x,
            scale_y: render.scale_y,
            rotation: render.rotation,
            flip_h: render.flip_h.unwrap_or(false),
            flip_v: render.flip_v.unwrap_or(false),
        },
        width,
        height,
        shape_params,
        text_data,
    })
}
