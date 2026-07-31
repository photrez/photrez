// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::document::Document;
use crate::history::HistoryStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MAX_OPEN_DOCUMENTS: usize = 16;

pub struct DocumentSession {
    pub id: String,
    pub document: Document,
    pub history: HistoryStore,
    pub selected_layer_id: Option<String>,
    pub dirty: bool,
    pub source_path: Option<String>,
    pub display_name: String,
    pub created_at_ms: u64,
    pub last_export_path: Option<String>,
}

impl Clone for DocumentSession {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            document: self.document.clone(),
            history: HistoryStore::new(50),
            selected_layer_id: self.selected_layer_id.clone(),
            dirty: self.dirty,
            source_path: self.source_path.clone(),
            display_name: self.display_name.clone(),
            created_at_ms: self.created_at_ms,
            last_export_path: self.last_export_path.clone(),
        }
    }
}

impl std::fmt::Debug for DocumentSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentSession")
            .field("id", &self.id)
            .field("display_name", &self.display_name)
            .field("dirty", &self.dirty)
            .finish()
    }
}

impl DocumentSession {
    pub fn new(id: String, document: Document, display_name: String) -> Self {
        Self {
            id,
            document,
            history: HistoryStore::new(50),
            selected_layer_id: None,
            dirty: false,
            source_path: None,
            display_name,
            created_at_ms: epoch_ms(),
            last_export_path: None,
        }
    }

    pub fn with_source_path(mut self, path: String) -> Self {
        self.source_path = Some(path);
        self
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    pub fn select_layer(&mut self, layer_id: Option<String>) {
        self.selected_layer_id = layer_id;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub documents: Vec<DocumentTabSummary>,
    pub active_document_id: Option<String>,
    pub active_document: Option<DocumentSnapshot>,
    pub limits: WorkspaceLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentTabSummary {
    pub id: String,
    pub display_name: String,
    pub is_dirty: bool,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSnapshot {
    pub id: String,
    pub display_name: String,
    pub width: u32,
    pub height: u32,
    pub layers: Vec<crate::layers::Layer>,
    pub selected_layer_id: Option<String>,
    pub selection: Option<crate::selection::SelectionRect>,
    pub dirty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceLimits {
    pub max_open_documents: usize,
    pub open_documents: usize,
}

pub struct WorkspaceState {
    sessions: Vec<DocumentSession>,
    active_document_id: Option<String>,
    max_open_documents: usize,
    /// Track duplicate display names for disambiguation
    name_counts: HashMap<String, usize>,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            active_document_id: None,
            max_open_documents: MAX_OPEN_DOCUMENTS,
            name_counts: HashMap::new(),
        }
    }

    pub fn max_open_documents(&self) -> usize {
        self.max_open_documents
    }

    pub fn open_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_full(&self) -> bool {
        self.sessions.len() >= self.max_open_documents
    }

    pub fn active_document_id(&self) -> Option<&str> {
        self.active_document_id.as_deref()
    }

    pub fn get_active_session(&self) -> Option<&DocumentSession> {
        let id = self.active_document_id.as_deref()?;
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn get_active_session_mut(&mut self) -> Option<&mut DocumentSession> {
        let id = self.active_document_id.clone()?;
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn get_session(&self, id: &str) -> Option<&DocumentSession> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn get_session_mut(&mut self, id: &str) -> Option<&mut DocumentSession> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn sessions(&self) -> &[DocumentSession] {
        &self.sessions
    }

    /// Generate a unique display name, disambiguating duplicates with (2), (3), etc.
    fn unique_display_name(&mut self, base_name: &str) -> String {
        let count = self.name_counts.entry(base_name.to_string()).or_insert(0);
        *count += 1;
        if *count == 1 {
            base_name.to_string()
        } else {
            format!("{} ({})", base_name, count)
        }
    }

    /// Reset name counts (called when all documents are closed)
    fn reset_name_counts(&mut self) {
        self.name_counts.clear();
    }

    /// Add a new document session. Returns the session id.
    /// The session becomes the active document.
    pub fn add_document(&mut self, mut session: DocumentSession) -> Result<String, String> {
        if self.is_full() {
            return Err("E_RESOURCE_LIMIT".to_string());
        }

        // Deduplicate display name
        let unique_name = self.unique_display_name(&session.display_name);
        session.display_name = unique_name;

        let id = session.id.clone();
        self.sessions.push(session);
        self.active_document_id = Some(id.clone());
        Ok(id)
    }

    /// Remove a document session by id.
    /// Returns true if the session was found and removed.
    pub fn remove_document(&mut self, id: &str) -> bool {
        let pos = self.sessions.iter().position(|s| s.id == id);
        if let Some(pos) = pos {
            self.sessions.remove(pos);

            // If the removed document was active, activate the nearest remaining tab
            if self.active_document_id.as_deref() == Some(id) {
                if self.sessions.is_empty() {
                    self.active_document_id = None;
                    self.reset_name_counts();
                } else {
                    // Activate nearest tab (same index or last)
                    let new_idx = pos.min(self.sessions.len() - 1);
                    self.active_document_id = Some(self.sessions[new_idx].id.clone());
                }
            }
            true
        } else {
            false
        }
    }

    /// Switch active document by id.
    pub fn switch_document(&mut self, id: &str) -> Result<(), String> {
        if self.sessions.iter().any(|s| s.id == id) {
            self.active_document_id = Some(id.to_string());
            Ok(())
        } else {
            Err("E_NOT_FOUND".to_string())
        }
    }

    /// Create a workspace snapshot for the frontend.
    pub fn snapshot(&self) -> WorkspaceSnapshot {
        let documents: Vec<DocumentTabSummary> = self
            .sessions
            .iter()
            .map(|s| DocumentTabSummary {
                id: s.id.clone(),
                display_name: s.display_name.clone(),
                is_dirty: s.dirty,
                width: s.document.width,
                height: s.document.height,
            })
            .collect();

        let active_document = self.get_active_session().map(|s| DocumentSnapshot {
            id: s.id.clone(),
            display_name: s.display_name.clone(),
            width: s.document.width,
            height: s.document.height,
            layers: s.document.layers.clone(),
            selected_layer_id: s.selected_layer_id.clone(),
            selection: s.document.selection,
            dirty: s.dirty,
        });

        WorkspaceSnapshot {
            documents,
            active_document_id: self.active_document_id.clone(),
            active_document,
            limits: WorkspaceLimits {
                max_open_documents: self.max_open_documents,
                open_documents: self.sessions.len(),
            },
        }
    }

    /// Check if any document is dirty.
    pub fn has_dirty_documents(&self) -> bool {
        self.sessions.iter().any(|s| s.dirty)
    }

    /// Get all dirty document ids.
    pub fn dirty_document_ids(&self) -> Vec<String> {
        self.sessions
            .iter()
            .filter(|s| s.dirty)
            .map(|s| s.id.clone())
            .collect()
    }
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self::new()
    }
}

fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_doc(id: &str) -> Document {
        Document::new(id.to_string(), 100, 100)
    }

    fn make_test_session(id: &str, name: &str) -> DocumentSession {
        let doc = make_test_doc(id);
        DocumentSession::new(id.to_string(), doc, name.to_string())
    }

    #[test]
    fn test_empty_workspace() {
        let ws = WorkspaceState::new();
        assert_eq!(ws.open_count(), 0);
        assert!(ws.active_document_id().is_none());
        assert!(!ws.is_full());
    }

    #[test]
    fn test_add_single_document() {
        let mut ws = WorkspaceState::new();
        let session = make_test_session("doc-1", "image.png");
        let id = ws.add_document(session).unwrap();
        assert_eq!(id, "doc-1");
        assert_eq!(ws.open_count(), 1);
        assert_eq!(ws.active_document_id(), Some("doc-1"));
    }

    #[test]
    fn test_add_multiple_documents() {
        let mut ws = WorkspaceState::new();
        ws.add_document(make_test_session("doc-1", "a.png"))
            .unwrap();
        ws.add_document(make_test_session("doc-2", "b.png"))
            .unwrap();
        ws.add_document(make_test_session("doc-3", "c.png"))
            .unwrap();
        assert_eq!(ws.open_count(), 3);
        assert_eq!(ws.active_document_id(), Some("doc-3"));
    }

    #[test]
    fn test_max_documents_enforced() {
        let mut ws = WorkspaceState::new();
        for i in 0..16 {
            ws.add_document(make_test_session(
                &format!("doc-{}", i),
                &format!("img-{}.png", i),
            ))
            .unwrap();
        }
        assert!(ws.is_full());
        let result = ws.add_document(make_test_session("doc-overflow", "overflow.png"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "E_RESOURCE_LIMIT");
    }

    #[test]
    fn test_remove_active_document_activates_nearest() {
        let mut ws = WorkspaceState::new();
        ws.add_document(make_test_session("doc-1", "a.png"))
            .unwrap();
        ws.add_document(make_test_session("doc-2", "b.png"))
            .unwrap();
        ws.add_document(make_test_session("doc-3", "c.png"))
            .unwrap();
        // active is doc-3 (last added)
        assert_eq!(ws.active_document_id(), Some("doc-3"));

        ws.remove_document("doc-3");
        // nearest is doc-2 (index 2 was removed, now len=2, min(2, 1)=1 â†’ doc-2)
        assert_eq!(ws.active_document_id(), Some("doc-2"));
        assert_eq!(ws.open_count(), 2);
    }

    #[test]
    fn test_remove_last_document() {
        let mut ws = WorkspaceState::new();
        ws.add_document(make_test_session("doc-1", "a.png"))
            .unwrap();
        ws.remove_document("doc-1");
        assert_eq!(ws.open_count(), 0);
        assert!(ws.active_document_id().is_none());
    }

    #[test]
    fn test_switch_document() {
        let mut ws = WorkspaceState::new();
        ws.add_document(make_test_session("doc-1", "a.png"))
            .unwrap();
        ws.add_document(make_test_session("doc-2", "b.png"))
            .unwrap();
        assert_eq!(ws.active_document_id(), Some("doc-2"));

        ws.switch_document("doc-1").unwrap();
        assert_eq!(ws.active_document_id(), Some("doc-1"));
    }

    #[test]
    fn test_switch_nonexistent_document() {
        let mut ws = WorkspaceState::new();
        let result = ws.switch_document("does-not-exist");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "E_NOT_FOUND");
    }

    #[test]
    fn test_duplicate_display_name_disambiguation() {
        let mut ws = WorkspaceState::new();
        ws.add_document(make_test_session("doc-1", "photo.png"))
            .unwrap();
        ws.add_document(make_test_session("doc-2", "photo.png"))
            .unwrap();
        ws.add_document(make_test_session("doc-3", "photo.png"))
            .unwrap();

        let names: Vec<String> = ws
            .sessions()
            .iter()
            .map(|s| s.display_name.clone())
            .collect();
        assert_eq!(names[0], "photo.png");
        assert_eq!(names[1], "photo.png (2)");
        assert_eq!(names[2], "photo.png (3)");
    }

    #[test]
    fn test_has_dirty_documents() {
        let mut ws = WorkspaceState::new();
        assert!(!ws.has_dirty_documents());

        let mut session = make_test_session("doc-1", "a.png");
        session.mark_dirty();
        ws.add_document(session).unwrap();
        assert!(ws.has_dirty_documents());
    }

    #[test]
    fn test_snapshot_empty() {
        let ws = WorkspaceState::new();
        let snap = ws.snapshot();
        assert!(snap.documents.is_empty());
        assert!(snap.active_document_id.is_none());
        assert!(snap.active_document.is_none());
        assert_eq!(snap.limits.open_documents, 0);
        assert_eq!(snap.limits.max_open_documents, 16);
    }

    #[test]
    fn test_snapshot_with_documents() {
        let mut ws = WorkspaceState::new();
        ws.add_document(make_test_session("doc-1", "a.png"))
            .unwrap();
        ws.add_document(make_test_session("doc-2", "b.png"))
            .unwrap();

        let snap = ws.snapshot();
        assert_eq!(snap.documents.len(), 2);
        assert_eq!(snap.active_document_id, Some("doc-2".to_string()));
        assert!(snap.active_document.is_some());
        assert_eq!(snap.limits.open_documents, 2);
    }

    #[test]
    fn test_document_session_mark_dirty_clean() {
        let mut session = make_test_session("doc-1", "test.png");
        assert!(!session.dirty);
        session.mark_dirty();
        assert!(session.dirty);
        session.mark_clean();
        assert!(!session.dirty);
    }

    #[test]
    fn test_remove_nonexistent_document() {
        let mut ws = WorkspaceState::new();
        assert!(!ws.remove_document("does-not-exist"));
    }

    #[test]
    fn test_remove_inactive_document() {
        let mut ws = WorkspaceState::new();
        ws.add_document(make_test_session("doc-1", "a.png"))
            .unwrap();
        ws.add_document(make_test_session("doc-2", "b.png"))
            .unwrap();
        // active is doc-2
        ws.remove_document("doc-1");
        // active should still be doc-2
        assert_eq!(ws.active_document_id(), Some("doc-2"));
    }

    #[test]
    fn test_dirty_document_ids() {
        let mut ws = WorkspaceState::new();
        let mut s1 = make_test_session("doc-1", "a.png");
        s1.mark_dirty();
        ws.add_document(s1).unwrap();
        ws.add_document(make_test_session("doc-2", "b.png"))
            .unwrap();

        let dirty = ws.dirty_document_ids();
        assert_eq!(dirty, vec!["doc-1".to_string()]);
    }
}
