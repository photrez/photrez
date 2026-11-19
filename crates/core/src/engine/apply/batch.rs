// Multi-layer command arms: every mutation a single user gesture asks for is
// committed inside one begin_forward/finish_forward pair, so one undo step
// restores or discards the whole gesture.
//
// Validation is all-or-nothing. An empty list, a repeated id in the list, an
// id that no layer owns, a mismatch between the id and the new-id lists of
// `DuplicateLayers`, a duplicate or empty new id, or a non-finite number
// rejects with E_INVALID before any mutation and the engine is left
// unchanged. Unknown ids are never silently skipped inside a batch.
//
// Unchanged members are skipped per field, and an all-no-op batch creates no
// history entry. The delta carries an Upsert per changed layer for the
// transform and metadata arms (in envelope order), a Remove per deleted
// layer for `DeleteLayers`, and the full restated layer set for
// `DuplicateLayers` (the same shape `DuplicateLayer` emits).

use super::*;
use crate::engine::duplicate_name::next_duplicate_name;

fn e_invalid(msg: &str) -> ProtocolError {
    ProtocolError {
        code: "E_INVALID".to_string(),
        message: msg.to_string(),
    }
}

impl DocumentCore {
    /// Resolve `ids` to layer positions in envelope order, rejecting empty
    /// lists, repeated ids and unknown ids before any mutation.
    fn batch_positions(&self, ids: &[String], command: &str) -> Result<Vec<usize>, ProtocolError> {
        if ids.is_empty() {
            return Err(e_invalid(&format!("{command} needs at least one layer id")));
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut resolved = Vec::with_capacity(ids.len());
        for id in ids {
            if !seen.insert(id.as_str()) {
                return Err(e_invalid(&format!(
                    "{command} received the layer id {id} twice"
                )));
            }
            let pos = self
                .layers
                .position_by_id(id)
                .ok_or_else(|| e_invalid(&format!("{command} found no layer with id {id}")))?;
            resolved.push(pos);
        }
        Ok(resolved)
    }

    /// Dispatch the multi-layer command family to its arm.
    pub(super) fn apply_batch_arm(
        &mut self,
        command: Command,
    ) -> Result<Vec<LayerChange>, ProtocolError> {
        match command {
            Command::GroupTransformLayers { patches } => self.apply_group_transform(&patches),
            Command::SetLayersVisible { ids, visible } => {
                self.apply_shared_meta(&ids, "Set Visible", |layer| {
                    if layer.visible == visible {
                        return false;
                    }
                    layer.visible = visible;
                    true
                })
            }
            Command::SetLayersLocked { ids, kind, locked } => {
                self.apply_shared_meta(&ids, "Set Lock", |layer| {
                    let current = match kind {
                        LockKind::Base => layer.locked,
                        LockKind::Transparency => layer.lock_transparency,
                        LockKind::Position => layer.lock_position,
                        LockKind::Rotation => layer.lock_rotation,
                    };
                    if current == Some(locked) {
                        return false;
                    }
                    match kind {
                        LockKind::Base => layer.locked = Some(locked),
                        LockKind::Transparency => layer.lock_transparency = Some(locked),
                        LockKind::Position => layer.lock_position = Some(locked),
                        LockKind::Rotation => layer.lock_rotation = Some(locked),
                    }
                    true
                })
            }
            Command::SetLayersOpacity { ids, opacity } => {
                let clamped = opacity.clamp(0.0, 1.0);
                self.apply_shared_meta(&ids, "Set Opacity", |layer| {
                    if layer.opacity == clamped {
                        return false;
                    }
                    layer.opacity = clamped;
                    true
                })
            }
            Command::SetLayersBlendMode { ids, mode } => {
                let wanted = Some(mode);
                self.apply_shared_meta(&ids, "Set Blend Mode", |layer| {
                    if layer.blend_mode == wanted {
                        return false;
                    }
                    layer.blend_mode = wanted.clone();
                    true
                })
            }
            Command::DeleteLayers { ids } => self.apply_delete_layers(&ids),
            Command::DuplicateLayers { ids, new_ids } => {
                self.apply_duplicate_layers(&ids, &new_ids)
            }
            _ => unreachable!("non-batch command reached batch arm"),
        }
    }

    fn apply_group_transform(
        &mut self,
        patches: &[TransformLayerEntry],
    ) -> Result<Vec<LayerChange>, ProtocolError> {
        let ids: Vec<String> = patches.iter().map(|entry| entry.id.clone()).collect();
        self.batch_positions(&ids, "groupTransformLayers")?;
        for entry in patches {
            let patch = &entry.transform;
            if ![
                patch.x,
                patch.y,
                patch.scale_x,
                patch.scale_y,
                patch.rotation,
            ]
            .iter()
            .all(|value| value.is_finite())
            {
                return Err(e_invalid(&format!(
                    "groupTransformLayers entry for {} carries a non-finite number",
                    entry.id
                )));
            }
        }
        // Skip entries whose absolute transform already matches the layer;
        // an all-no-op gesture would otherwise fake a dirty commit.
        let changed: Vec<&TransformLayerEntry> = patches
            .iter()
            .filter(|entry| {
                self.layers
                    .position_by_id(&entry.id)
                    .map(|pos| transform_differs(self.layers.get(pos).expect("validated"), entry))
                    .unwrap_or(false)
            })
            .collect();
        if changed.is_empty() {
            return Ok(Vec::new());
        }
        let affected: Vec<String> = changed.iter().map(|entry| entry.id.clone()).collect();
        let begin = self.begin_forward("Transform Layers", &affected);
        let mut changes = Vec::with_capacity(changed.len());
        for entry in changed {
            let pos = self.layers.position_by_id(&entry.id).expect("validated");
            let mut layer = self.layers.get(pos).expect("validated").clone();
            layer.x = entry.transform.x;
            layer.y = entry.transform.y;
            layer.scale_x = entry.transform.scale_x;
            layer.scale_y = entry.transform.scale_y;
            layer.rotation = entry.transform.rotation;
            if let Some(flip_h) = entry.transform.flip_h {
                layer.flip_h = Some(flip_h);
            }
            if let Some(flip_v) = entry.transform.flip_v {
                layer.flip_v = Some(flip_v);
            }
            layer.dirty_rect = Some(Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            });
            self.layers = self.layers.replaced(pos, layer.clone());
            changes.push(LayerChange::Upsert { layer });
        }
        self.finish_forward(begin);
        Ok(changes)
    }

    fn apply_shared_meta(
        &mut self,
        ids: &[String],
        label: &str,
        mut apply: impl FnMut(&mut LayerMeta) -> bool,
    ) -> Result<Vec<LayerChange>, ProtocolError> {
        self.batch_positions(ids, label)?;
        // Discover the changed subset on cloned values first; only then are
        // layers replaced and history written.
        let changed: Vec<usize> = ids
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let pos = self.layers.position_by_id(id)?;
                let mut probe = self.layers.get(pos)?.clone();
                apply(&mut probe).then_some(index)
            })
            .collect();
        if changed.is_empty() {
            return Ok(Vec::new());
        }
        let affected: Vec<String> = changed.iter().map(|&index| ids[index].clone()).collect();
        let begin = self.begin_forward(label, &affected);
        let mut changes = Vec::with_capacity(changed.len());
        for &index in &changed {
            let id = &ids[index];
            let pos = self.layers.position_by_id(id).expect("validated");
            let mut layer = self.layers.get(pos).expect("validated").clone();
            apply(&mut layer);
            layer.dirty_rect = Some(Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            });
            self.layers = self.layers.replaced(pos, layer.clone());
            changes.push(LayerChange::Upsert { layer });
        }
        self.finish_forward(begin);
        Ok(changes)
    }

    fn apply_delete_layers(&mut self, ids: &[String]) -> Result<Vec<LayerChange>, ProtocolError> {
        self.batch_positions(ids, "deleteLayers")?;
        // Capture every id and its resource before any removal; emit the
        // delta in envelope order. Removals run bottom-to-top so the working
        // indices stay valid.
        let captured: Vec<(String, u32)> = ids
            .iter()
            .map(|id| {
                let pos = self.layers.position_by_id(id).expect("validated");
                let layer = self.layers.get(pos).expect("validated");
                (id.clone(), layer.resource_id)
            })
            .collect();
        let mut order: Vec<usize> = (0..ids.len()).collect();
        order.sort_unstable_by_key(|&index| {
            std::cmp::Reverse(self.layers.position_by_id(&ids[index]).expect("validated"))
        });
        let begin = self.begin_forward("Delete Layer", ids);
        for &index in &order {
            let pos = self.layers.position_by_id(&ids[index]).expect("validated");
            self.layers = self.layers.removed(pos);
        }
        self.finish_forward(begin);
        Ok(captured
            .into_iter()
            .map(|(id, resource_id)| LayerChange::Remove { id, resource_id })
            .collect())
    }

    fn apply_duplicate_layers(
        &mut self,
        ids: &[String],
        new_ids: &[String],
    ) -> Result<Vec<LayerChange>, ProtocolError> {
        if ids.len() != new_ids.len() {
            return Err(e_invalid("duplicateLayers needs one new id per source id"));
        }
        // Every structural mistake is rejected before the first mutation.
        self.batch_positions(ids, "duplicateLayers")?;
        let mut seen = std::collections::BTreeSet::new();
        for (index, new_id) in new_ids.iter().enumerate() {
            if new_id.is_empty() {
                return Err(e_invalid(&format!(
                    "duplicateLayers needs a non-empty new id at index {index}"
                )));
            }
            if !seen.insert(new_id.as_str()) {
                return Err(e_invalid(&format!(
                    "duplicateLayers received the new id {new_id} twice"
                )));
            }
            if self.layers.position_by_id(new_id).is_some() {
                return Err(e_invalid(&format!(
                    "duplicateLayers new id already present: {new_id}"
                )));
            }
        }
        let begin = self.begin_forward("Duplicate Layer", new_ids);
        // Process the sources from the top of the stack downward; a clone sits
        // at and above its source, so earlier insertions only shift the
        // indices this loop re-reads afterward.
        let mut order: Vec<usize> = (0..ids.len()).collect();
        order.sort_unstable_by_key(|&index| {
            std::cmp::Reverse(self.layers.position_by_id(&ids[index]).expect("validated"))
        });
        for index in order {
            let src_id = &ids[index];
            let new_id = &new_ids[index];
            let pos = self.layers.position_by_id(src_id).expect("validated");
            let src = self.layers.get(pos).expect("validated").clone();
            let names: Vec<&str> = self.layers.0.iter().map(|l| l.name.as_str()).collect();
            let name = next_duplicate_name(&names, &src.name);
            let rid = self.next_resource;
            self.next_resource += 1;
            let mut clone = src;
            clone.id = new_id.clone();
            clone.name = name;
            clone.resource_id = rid;
            clone.locked = Some(false);
            clone.is_background = None;
            clone.lock_position = None;
            clone.lock_rotation = None;
            clone.lock_transparency = None;
            self.layers = self.layers.insert_at(clone, pos);
        }
        self.finish_forward(begin);
        Ok(self
            .layers
            .0
            .iter()
            .map(|layer| LayerChange::Upsert {
                layer: layer.as_ref().clone(),
            })
            .collect())
    }
}

fn transform_differs(layer: &LayerMeta, entry: &TransformLayerEntry) -> bool {
    let patch = &entry.transform;
    if layer.x != patch.x
        || layer.y != patch.y
        || layer.scale_x != patch.scale_x
        || layer.scale_y != patch.scale_y
        || layer.rotation != patch.rotation
    {
        return true;
    }
    if let Some(flip_h) = patch.flip_h
        && layer.flip_h.unwrap_or(false) != flip_h
    {
        return true;
    }
    if let Some(flip_v) = patch.flip_v
        && layer.flip_v.unwrap_or(false) != flip_v
    {
        return true;
    }
    false
}
