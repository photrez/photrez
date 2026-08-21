// SPDX-License-Identifier: AGPL-3.0-or-later
// History for DocumentEngine — mirrors TS CommandHistory (without ImageBitmap disposal, since Rust DocumentModel has no bitmaps)

use crate::document::DocumentModel;

pub struct History {
    undo_stack: Vec<DocumentModel>,
    redo_stack: Vec<DocumentModel>,
    max_depth: usize,
}

impl History {
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth,
        }
    }

    pub fn commit(&mut self, snapshot: DocumentModel) {
        self.undo_stack.push(snapshot);
        self.redo_stack.clear();
        if self.undo_stack.len() > self.max_depth {
            self.undo_stack.remove(0);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo(&mut self, current: DocumentModel) -> Option<DocumentModel> {
        if !self.can_undo() {
            return None;
        }
        let prev = self.undo_stack.pop().unwrap();
        self.redo_stack.push(current);
        Some(prev)
    }

    pub fn redo(&mut self, current: DocumentModel) -> Option<DocumentModel> {
        if !self.can_redo() {
            return None;
        }
        let next = self.redo_stack.pop().unwrap();
        self.undo_stack.push(current);
        Some(next)
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{DocumentModel, Layer, Transform2D};

    fn make_model(layers: usize) -> DocumentModel {
        let mut m = DocumentModel {
            id: "doc".into(),
            name: "Test".into(),
            width: 100,
            height: 100,
            layers: Vec::new(),
            active_layer_id: None,
            selection: None,
            dirty: false,
        };
        for i in 0..layers {
            m.layers.push(Layer {
                id: format!("l{}", i),
                name: format!("L{}", i),
                visible: true,
                locked: false,
                opacity: 1.0,
                blend_mode: "normal".into(),
                transform: Transform2D::default(),
                width: 100,
                height: 100,
                has_adjustments: false,
            });
        }
        m
    }

    #[test]
    fn commit_and_undo() {
        let mut h = History::new(50);
        let m1 = make_model(1);
        let m2 = make_model(2);
        h.commit(m1.clone());
        assert!(h.can_undo());
        assert_eq!(h.undo(m2.clone()), Some(m1));
        assert!(!h.can_undo());
        assert!(h.can_redo());
    }

    #[test]
    fn redo() {
        let mut h = History::new(50);
        let m1 = make_model(1);
        let m2 = make_model(2);
        h.commit(m1.clone());
        let prev = h.undo(m2.clone()).expect("undo");
        assert_eq!(prev.layers.len(), 1);
        let next = h.redo(prev.clone()).expect("redo");
        assert_eq!(next.layers.len(), 2);
    }

    #[test]
    fn max_depth() {
        let mut h = History::new(2);
        h.commit(make_model(1));
        h.commit(make_model(2));
        h.commit(make_model(3));
        assert_eq!(h.undo_count(), 2);
    }
}
