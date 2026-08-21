// SPDX-License-Identifier: AGPL-3.0-or-later
// DocumentEngine SSOT — Rust owns layer graph + selection + history (vertical slice: add/select/undo)

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use crate::history::History;
use crate::selection::SelectionState;

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
        Self { x: 0.0, y: 0.0, scale_x: 1.0, scale_y: 1.0, rotation: 0.0, flip_h: false, flip_v: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f64,
    pub blend_mode: String,
    pub transform: Transform2D,
    pub width: u32,
    pub height: u32,
    pub has_adjustments: bool,
}

impl Layer {
    fn new(id: String, name: String, width: u32, height: u32) -> Self {
        Self {
            id, name, visible: true, locked: false, opacity: 1.0,
            blend_mode: "normal".to_string(), transform: Transform2D::default(),
            width, height, has_adjustments: false,
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
        let model = DocumentModel { id, name, width, height, layers: Vec::new(), active_layer_id: None, selection: None, dirty: false };
        Self { model, history: History::new(50) }
    }

    pub fn add_layer(&mut self, layer_id: String, name: String, width: u32, height: u32) {
        let layer = Layer::new(layer_id.clone(), name, width, height);
        self.model.layers.push(layer);
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
        if let Some(pos) = self.model.layers.iter().position(|l| l.id == layer_id) {
            self.model.layers.remove(pos);
            if self.model.active_layer_id.as_deref() == Some(&layer_id) {
                self.model.active_layer_id = self.model.layers.first().map(|l| l.id.clone());
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

    pub fn layer_count(&self) -> usize {
        self.model.layers.len()
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

    pub fn has_undo(&self) -> bool { self.history.can_undo() }
    pub fn has_redo(&self) -> bool { self.history.can_redo() }
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
}
