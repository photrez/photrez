// SPDX-License-Identifier: AGPL-3.0-or-later
// DocumentEngine SSOT — Rust owns layer graph + selection + history (vertical slice: add/select/undo)

use crate::history::History;
use crate::selection::SelectionState;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Transform2D {
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub layer_type: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f64,
    pub is_background: Option<bool>,
    pub lock_transparency: Option<bool>,
    pub lock_position: Option<bool>,
    pub lock_rotation: Option<bool>,
    pub has_adjustments: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basic_adjustment: Option<serde_json::Value>,
    pub blend_mode: String,
    pub transform: Transform2D,
    pub width: u32,
    pub height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape_params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_data: Option<serde_json::Value>,
}

impl Layer {
    pub(crate) fn new(id: String, name: String, width: u32, height: u32) -> Self {
        Self {
            id,
            name,
            layer_type: "raster".to_string(),
            visible: true,
            locked: false,
            opacity: 1.0,
            is_background: None,
            lock_transparency: None,
            lock_position: None,
            lock_rotation: None,
            has_adjustments: false,
            basic_adjustment: None,
            blend_mode: "normal".to_string(),
            transform: Transform2D::default(),
            width,
            height,
            shape_params: None,
            text_data: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentModel {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    pub active_layer_id: Option<String>,
    pub selection: Option<SelectionState>,
    pub dirty: bool,
}

#[wasm_bindgen]
pub struct DocumentEngine {
    model: DocumentModel,
    history: History,
}

#[wasm_bindgen]
impl DocumentEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(id: String, name: String, width: u32, height: u32) -> Self {
        let model = DocumentModel {
            id,
            name,
            width,
            height,
            layers: Vec::new(),
            active_layer_id: None,
            selection: None,
            dirty: false,
        };
        Self {
            model,
            history: History::new(50),
        }
    }

    pub fn add_layer(&mut self, layer_id: String, name: String, width: u32, height: u32) {
        let layer = Layer::new(layer_id.clone(), name, width, height);
        // Parity with TS applyAddLayer: insert directly ABOVE the active layer,
        // else at the front (top) of the stack.
        let active_index = self
            .model
            .active_layer_id
            .as_ref()
            .and_then(|id| self.model.layers.iter().position(|l| &l.id == id));
        match active_index {
            Some(i) => self.model.layers.insert(i, layer),
            None => self.model.layers.insert(0, layer),
        }
        self.model.active_layer_id = Some(layer_id);
        self.model.dirty = true;
    }

    pub fn get_layers_json(&self) -> String {
        serde_json::to_string(&self.model.layers).unwrap_or_else(|_| "[]".to_string())
    }

    pub fn get_active_layer_id(&self) -> Option<String> {
        self.model.active_layer_id.clone()
    }

    pub fn set_active_layer(&mut self, layer_id: String) -> bool {
        if self.model.layers.iter().any(|l| l.id == layer_id) {
            self.model.active_layer_id = Some(layer_id);
            return true;
        }
        false
    }

    pub fn delete_layer(&mut self, layer_id: String) -> bool {
        // Parity with TS applyDeleteLayer: background layer and last remaining
        // layer cannot be deleted.
        if let Some(l) = self.model.layers.iter().find(|l| l.id == layer_id) {
            if l.is_background == Some(true) {
                return false;
            }
        }
        if self.model.layers.len() <= 1 {
            return false;
        }
        if let Some(pos) = self.model.layers.iter().position(|l| l.id == layer_id) {
            self.model.layers.remove(pos);
            if self.model.active_layer_id.as_deref() == Some(&layer_id) {
                let next_idx = pos.min(self.model.layers.len().saturating_sub(1));
                self.model.active_layer_id = self.model.layers.get(next_idx).map(|l| l.id.clone());
            }
            self.model.dirty = true;
            return true;
        }
        false
    }

    pub fn duplicate_layer(&mut self, layer_id: String) -> Option<String> {
        if let Some(pos) = self.model.layers.iter().position(|l| l.id == layer_id) {
            let orig = self.model.layers[pos].clone();
            let mut new_id = format!("{}-copy", orig.id);
            let mut counter = 1;
            while self.model.layers.iter().any(|l| l.id == new_id) {
                new_id = format!("{}-copy-{}", orig.id, counter);
                counter += 1;
            }
            let mut new_layer = orig.clone();
            new_layer.id = new_id.clone();
            new_layer.name = format!("{} copy", orig.name);
            self.model.layers.insert(pos + 1, new_layer);
            self.model.active_layer_id = Some(new_id.clone());
            self.model.dirty = true;
            return Some(new_id);
        }
        None
    }

    pub fn reorder_layer(&mut self, from_index: usize, to_index: usize) -> bool {
        if from_index >= self.model.layers.len() || to_index >= self.model.layers.len() {
            return false;
        }
        if from_index == to_index {
            return true;
        }
        // Parity with TS applyReorderLayer: the Background is pinned to the
        // bottom — it can never be reordered, and nothing may sit below it.
        if self.model.layers[from_index].is_background == Some(true) {
            return false;
        }
        let mut updated = self.model.layers.clone();
        let moved = updated.remove(from_index);
        updated.insert(to_index, moved);
        // Invariant: re-seat the Background at the bottom if the move pushed
        // it off the bottom (a layer beneath the opaque Background would be
        // unreachable / hidden).
        if let Some(bg_idx) = updated.iter().position(|l| l.is_background == Some(true)) {
            if bg_idx != updated.len() - 1 {
                let bg = updated.remove(bg_idx);
                updated.push(bg);
            }
        }
        self.model.layers = updated;
        self.model.dirty = true;
        true
    }

    pub fn layer_count(&self) -> usize {
        self.model.layers.len()
    }

    /// Merge a layer down into the one below it (graph op — pixel composite is
    /// done TS-side; caller attaches the bitmap to `merged_id` after sync).
    /// Parity with TS applyMergeDown: merged node inherits bottom's blendMode,
    /// locked = top||bottom, inserted at the pair's stack position.
    pub fn merge_down(
        &mut self,
        id: String,
        merged_id: String,
        name: String,
        locked: bool,
    ) -> bool {
        let idx = match self.model.layers.iter().position(|l| l.id == id) {
            Some(i) => i,
            None => return false,
        };
        if idx >= self.model.layers.len() - 1 {
            return false;
        }
        let bottom = self.model.layers[idx + 1].clone();
        let w = self.model.width;
        let h = self.model.height;
        let mut m = Layer::new(merged_id.clone(), name, w, h);
        m.locked = locked;
        m.blend_mode = bottom.blend_mode.clone();
        let removed: Vec<Layer> = self.model.layers.drain(idx..idx + 2).collect();
        drop(removed);
        self.model.layers.insert(idx, m);
        self.model.active_layer_id = Some(merged_id);
        self.model.dirty = true;
        true
    }

    /// Merge multiple arbitrary layers into one raster node at the highest
    /// stack position of the selection. Parity with TS applyMergeSelectedLayers.
    pub fn merge_selected(
        &mut self,
        ids: Vec<String>,
        merged_id: String,
        name: String,
        locked: bool,
    ) -> bool {
        if ids.len() < 2 {
            return false;
        }
        let selected_count = self
            .model
            .layers
            .iter()
            .filter(|l| ids.contains(&l.id))
            .count();
        if selected_count < 2 {
            return false;
        }
        let w = self.model.width;
        let h = self.model.height;
        let mut updated: Vec<Layer> =
            Vec::with_capacity(self.model.layers.len() - selected_count + 1);
        let mut inserted = false;
        for l in std::mem::take(&mut self.model.layers) {
            if ids.contains(&l.id) {
                if !inserted {
                    let mut m = Layer::new(merged_id.clone(), name.clone(), w, h);
                    m.locked = locked;
                    updated.push(m);
                    inserted = true;
                }
            } else {
                updated.push(l);
            }
        }
        self.model.layers = updated;
        self.model.active_layer_id = Some(merged_id);
        self.model.dirty = true;
        true
    }

    /// Flatten all layers into a single Background node. Parity with TS
    /// applyFlattenLayers (isBackground + position/rotation locks).
    pub fn flatten(&mut self, merged_id: String, name: String, locked: bool) -> bool {
        if self.model.layers.len() <= 1 {
            return false;
        }
        let w = self.model.width;
        let h = self.model.height;
        let mut m = Layer::new(merged_id.clone(), name, w, h);
        m.locked = locked;
        m.is_background = Some(true);
        m.lock_position = Some(true);
        m.lock_rotation = Some(true);
        self.model.layers.clear();
        self.model.layers.push(m.clone());
        self.model.active_layer_id = Some(m.id);
        self.model.dirty = true;
        true
    }

    pub fn snapshot_json(&self) -> String {
        serde_json::to_string(&self.model).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn restore_snapshot(&mut self, json: &str) -> bool {
        if let Ok(model) = serde_json::from_str::<DocumentModel>(json) {
            self.model = model;
            return true;
        }
        false
    }

    pub fn commit_snapshot(&mut self) {
        let snap = self.model.clone();
        self.history.commit(snap);
    }

    pub fn undo(&mut self) -> bool {
        let cur = self.model.clone();
        if let Some(prev) = self.history.undo(cur) {
            self.model = prev;
            return true;
        }
        false
    }

    pub fn redo(&mut self) -> bool {
        let cur = self.model.clone();
        if let Some(next) = self.history.redo(cur) {
            self.model = next;
            return true;
        }
        false
    }

    pub fn has_undo(&self) -> bool {
        self.history.can_undo()
    }
    pub fn has_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn set_selection(
        &mut self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        angle: f64,
        shape: Option<String>,
        inverted: Option<bool>,
    ) {
        self.model.selection = Some(SelectionState {
            x,
            y,
            width,
            height,
            angle,
            shape,
            inverted,
        });
        self.model.dirty = true;
    }

    pub fn clear_selection(&mut self) {
        self.model.selection = None;
        self.model.dirty = true;
    }

    /// Mark a layer as the Background (bottommost, position/rotation locked).
    /// Used by document open / flatten to flag the bottom layer.
    pub fn set_layer_background(&mut self, layer_id: String) -> bool {
        if let Some(l) = self.model.layers.iter_mut().find(|l| l.id == layer_id) {
            l.is_background = Some(true);
            l.lock_position = Some(true);
            l.lock_rotation = Some(true);
            self.model.dirty = true;
            return true;
        }
        false
    }

    pub fn get_selection_json(&self) -> String {
        serde_json::to_string(&self.model.selection).unwrap_or_else(|_| "null".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_has_no_layers() {
        let e = DocumentEngine::new("doc1".into(), "Test".into(), 800, 600);
        assert_eq!(e.layer_count(), 0);
        assert_eq!(e.get_active_layer_id(), None);
    }

    #[test]
    fn add_layer_sets_active() {
        let mut e = DocumentEngine::new("d".into(), "n".into(), 100, 100);
        e.add_layer("l1".into(), "Layer 1".into(), 100, 100);
        assert_eq!(e.layer_count(), 1);
        assert_eq!(e.get_active_layer_id(), Some("l1".into()));
    }

    #[test]
    fn set_active_layer() {
        let mut e = DocumentEngine::new("d".into(), "n".into(), 100, 100);
        e.add_layer("l1".into(), "A".into(), 100, 100);
        e.add_layer("l2".into(), "B".into(), 100, 100);
        assert!(e.set_active_layer("l1".into()));
        assert_eq!(e.get_active_layer_id(), Some("l1".into()));
        assert!(!e.set_active_layer("nope".into()));
    }

    #[test]
    fn snapshot_undo_redo() {
        let mut e = DocumentEngine::new("d".into(), "n".into(), 100, 100);
        e.add_layer("l1".into(), "A".into(), 100, 100);
        e.commit_snapshot();
        e.add_layer("l2".into(), "B".into(), 100, 100);
        assert_eq!(e.layer_count(), 2);
        assert!(e.undo());
        assert_eq!(e.layer_count(), 1);
        assert!(e.redo());
        assert_eq!(e.layer_count(), 2);
    }

    #[test]
    fn snapshot_json_roundtrip() {
        let mut e = DocumentEngine::new("d".into(), "n".into(), 100, 100);
        e.add_layer("l1".into(), "A".into(), 100, 100);
        let json = e.snapshot_json();
        let mut e2 = DocumentEngine::new("x".into(), "y".into(), 1, 1);
        assert!(e2.restore_snapshot(&json));
        assert_eq!(e2.layer_count(), 1);
        assert_eq!(e2.get_active_layer_id(), Some("l1".into()));
    }

    #[test]
    fn delete_layer() {
        let mut e = DocumentEngine::new("d".into(), "n".into(), 100, 100);
        e.add_layer("l1".into(), "A".into(), 100, 100);
        e.add_layer("l2".into(), "B".into(), 100, 100);
        assert!(e.delete_layer("l1".into()));
        assert_eq!(e.layer_count(), 1);
        assert_eq!(e.get_active_layer_id(), Some("l2".into()));
        assert!(!e.delete_layer("nope".into()));
    }

    #[test]
    fn duplicate_layer() {
        let mut e = DocumentEngine::new("d".into(), "n".into(), 100, 100);
        e.add_layer("l1".into(), "A".into(), 100, 100);
        let new_id = e.duplicate_layer("l1".into()).expect("duplicate");
        assert_eq!(e.layer_count(), 2);
        assert_eq!(e.get_active_layer_id(), Some(new_id));
    }

    #[test]
    fn reorder_layer() {
        let mut e = DocumentEngine::new("d".into(), "n".into(), 100, 100);
        e.add_layer("l1".into(), "A".into(), 100, 100);
        e.add_layer("l2".into(), "B".into(), 100, 100);
        e.add_layer("l3".into(), "C".into(), 100, 100);
        assert!(e.reorder_layer(0, 2));
        assert_eq!(e.layer_count(), 3);
        assert!(!e.reorder_layer(10, 0));
    }

    #[test]
    fn reorder_pins_background_to_bottom() {
        let mut e = DocumentEngine::new("d".into(), "n".into(), 100, 100);
        // Simulate a Background: bottom layer flagged isBackground.
        e.add_layer("bg".into(), "Background".into(), 100, 100);
        e.set_layer_background("bg".into());
        e.add_layer("l1".into(), "A".into(), 100, 100);
        e.add_layer("l2".into(), "B".into(), 100, 100);
        // Order now (top→bottom): l2, l1, bg → bg at index 2 (bottom).
        // Moving bg is forbidden.
        assert!(!e.reorder_layer(2, 0));
        // Moving l1 to index 2 (below bg) re-seats bg back to the bottom.
        assert!(e.reorder_layer(1, 2));
        let json = e.get_layers_json();
        assert!(
            json.rfind("\"bg\"").unwrap() > json.rfind("\"l1\"").unwrap(),
            "background must remain bottommost after reorder"
        );
    }

    #[test]
    fn merge_down() {
        let mut e = DocumentEngine::new("d".into(), "n".into(), 100, 100);
        // addLayer inserts ABOVE active → stack (top→bottom): [upper, lower]
        e.add_layer("lower".into(), "L".into(), 100, 100);
        e.add_layer("upper".into(), "U".into(), 100, 100);
        assert!(e.merge_down("upper".into(), "m1".into(), "U + L".into(), false));
        assert_eq!(e.layer_count(), 1);
        assert_eq!(e.get_active_layer_id(), Some("m1".into()));
        // Cannot merge the bottom-most layer (nothing below it).
        assert!(!e.merge_down("m1".into(), "m2".into(), "x".into(), false));
    }

    #[test]
    fn merge_selected() {
        let mut e = DocumentEngine::new("d".into(), "n".into(), 100, 100);
        e.add_layer("a".into(), "A".into(), 100, 100);
        e.add_layer("b".into(), "B".into(), 100, 100);
        e.add_layer("c".into(), "C".into(), 100, 100);
        assert!(e.merge_selected(
            vec!["a".into(), "c".into()],
            "m".into(),
            "merged".into(),
            false
        ));
        assert_eq!(e.layer_count(), 2);
        assert_eq!(e.get_active_layer_id(), Some("m".into()));
        // Fewer than 2 matching ids is a no-op.
        assert!(!e.merge_selected(vec!["b".into()], "m2".into(), "x".into(), false));
    }

    #[test]
    fn flatten_all() {
        let mut e = DocumentEngine::new("d".into(), "n".into(), 100, 100);
        e.add_layer("a".into(), "A".into(), 100, 100);
        e.add_layer("b".into(), "B".into(), 100, 100);
        assert!(e.flatten("flat".into(), "Background".into(), false));
        assert_eq!(e.layer_count(), 1);
        assert_eq!(e.get_active_layer_id(), Some("flat".into()));
        let json = e.get_layers_json();
        assert!(json.contains("\"isBackground\":true"));
        // Single layer cannot flatten.
        assert!(!e.flatten("f2".into(), "x".into(), false));
    }

    #[test]
    fn selection() {
        let mut e = DocumentEngine::new("d".into(), "n".into(), 100, 100);
        assert_eq!(e.get_selection_json(), "null");
        e.set_selection(
            10.0,
            20.0,
            100.0,
            50.0,
            0.0,
            Some("rect".into()),
            Some(false),
        );
        assert!(e.get_selection_json().contains("10"));
        e.clear_selection();
        assert_eq!(e.get_selection_json(), "null");
    }
}
