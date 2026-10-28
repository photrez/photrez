use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::services::document::LoadedDocument;
use crate::services::error::ServiceError;
use crate::services::file_io::FileIo;
use crate::services::ptz_limits;
use crate::services::save_stream::SaveStream;

static NEXT_SESSION_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) struct DocumentSession {
    instance_id: u64,
    doc_id: String,
    display_name: String,
    source_path: Option<PathBuf>,
    revision: u64,
    saved_revision: u64,
    closed: bool,
    loaded: LoadedDocument,
}

#[derive(Debug)]
pub(crate) struct SaveReceipt {
    instance_id: u64,
    doc_id: String,
    revision: u64,
}

impl DocumentSession {
    fn new(
        loaded: LoadedDocument,
        display_name: String,
        source_path: Option<PathBuf>,
    ) -> Result<Self, ServiceError> {
        let instance_id = NEXT_SESSION_INSTANCE_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| ServiceError::Internal("Document session ID overflow".into()))?;
        Ok(Self {
            instance_id,
            doc_id: loaded.document_id().to_owned(),
            display_name,
            source_path,
            revision: 0,
            saved_revision: 0,
            closed: false,
            loaded,
        })
    }

    pub(crate) fn doc_id(&self) -> &str {
        &self.doc_id
    }

    pub(crate) fn display_name(&self) -> &str {
        &self.display_name
    }

    pub(crate) fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn saved_revision(&self) -> u64 {
        self.saved_revision
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed
    }

    pub(crate) fn loaded_document(&self) -> &LoadedDocument {
        &self.loaded
    }

    pub(crate) fn loaded_document_mut(&mut self) -> &mut LoadedDocument {
        &mut self.loaded
    }

    fn ensure_open(&self) -> Result<(), ServiceError> {
        if self.closed {
            Err(ServiceError::Conflict("Document session is closed".into()))
        } else {
            Ok(())
        }
    }

    fn mark_dirty(&mut self) -> Result<u64, ServiceError> {
        self.ensure_open()?;
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| ServiceError::Internal("Document revision overflow".into()))?;
        Ok(self.revision)
    }

    fn save(
        &mut self,
        files: &dyn FileIo,
        path: &Path,
        save_stream: &SaveStream,
    ) -> Result<SaveReceipt, ServiceError> {
        self.ensure_open()?;
        let revision = self.revision;
        self.loaded.save_to(files, path, save_stream)?;
        self.source_path = Some(path.to_path_buf());
        self.saved_revision = revision;
        Ok(SaveReceipt {
            instance_id: self.instance_id,
            doc_id: self.doc_id.clone(),
            revision,
        })
    }

    fn close_after_save(&mut self, receipt: &SaveReceipt) -> Result<(), ServiceError> {
        self.ensure_open()?;
        if receipt.instance_id != self.instance_id
            || receipt.doc_id != self.doc_id
            || receipt.revision != self.revision
            || receipt.revision != self.saved_revision
        {
            return Err(ServiceError::Conflict(
                "Save receipt does not match the current document revision".into(),
            ));
        }
        self.closed = true;
        Ok(())
    }

    fn discard(&mut self) -> Result<(), ServiceError> {
        self.ensure_open()?;
        self.closed = true;
        Ok(())
    }

    fn cancel(&self) -> Result<(), ServiceError> {
        self.ensure_open()
    }
}

#[derive(Default)]
pub(crate) struct SessionRegistry {
    sessions: HashMap<String, DocumentSession>,
}

impl SessionRegistry {
    pub(crate) fn open(
        &mut self,
        loaded: LoadedDocument,
        display_name: impl Into<String>,
        source_path: Option<PathBuf>,
    ) -> Result<(), ServiceError> {
        let doc_id = loaded.document_id().to_owned();
        ptz_limits::validate_layer_id(&doc_id)?;
        if self.sessions.contains_key(&doc_id) {
            return Err(ServiceError::Conflict(
                "Document session is already open".into(),
            ));
        }
        let session = DocumentSession::new(loaded, display_name.into(), source_path)?;
        self.sessions.insert(doc_id, session);
        Ok(())
    }

    pub(crate) fn get(&self, doc_id: &str) -> Option<&DocumentSession> {
        self.sessions.get(doc_id).filter(|session| !session.closed)
    }

    pub(crate) fn get_mut(&mut self, doc_id: &str) -> Option<&mut DocumentSession> {
        self.sessions
            .get_mut(doc_id)
            .filter(|session| !session.closed)
    }

    /// Register `loaded` in place of the session with the same document ID.
    ///
    /// The replacement is constructed before the old session is removed, so a
    /// validation failure leaves the open session untouched. A dirty session is
    /// never replaced silently.
    pub(crate) fn replace(
        &mut self,
        loaded: LoadedDocument,
        display_name: impl Into<String>,
        source_path: Option<PathBuf>,
    ) -> Result<(), ServiceError> {
        let doc_id = loaded.document_id().to_owned();
        ptz_limits::validate_layer_id(&doc_id)?;
        if self
            .sessions
            .get(&doc_id)
            .is_some_and(|session| session.is_dirty())
        {
            return Err(ServiceError::Conflict(
                "Document has unsaved changes".into(),
            ));
        }
        let session = DocumentSession::new(loaded, display_name.into(), source_path)?;
        self.sessions.insert(doc_id, session);
        Ok(())
    }

    pub(crate) fn mark_dirty(&mut self, doc_id: &str) -> Result<u64, ServiceError> {
        self.sessions
            .get_mut(doc_id)
            .ok_or_else(|| ServiceError::Conflict("Document session is not open".into()))?
            .mark_dirty()
    }

    pub(crate) fn save(
        &mut self,
        doc_id: &str,
        files: &dyn FileIo,
        path: &Path,
        save_stream: &SaveStream,
    ) -> Result<SaveReceipt, ServiceError> {
        self.sessions
            .get_mut(doc_id)
            .ok_or_else(|| ServiceError::Conflict("Document session is not open".into()))?
            .save(files, path, save_stream)
    }

    pub(crate) fn close_after_save(
        &mut self,
        doc_id: &str,
        receipt: &SaveReceipt,
    ) -> Result<(), ServiceError> {
        let session = self
            .sessions
            .get_mut(doc_id)
            .ok_or_else(|| ServiceError::Conflict("Document session is not open".into()))?;
        session.close_after_save(receipt)?;
        self.sessions.remove(doc_id);
        Ok(())
    }

    pub(crate) fn discard(&mut self, doc_id: &str) -> Result<(), ServiceError> {
        let session = self
            .sessions
            .get_mut(doc_id)
            .ok_or_else(|| ServiceError::Conflict("Document session is not open".into()))?;
        session.discard()?;
        self.sessions.remove(doc_id);
        Ok(())
    }

    pub(crate) fn cancel(&self, doc_id: &str) -> Result<(), ServiceError> {
        self.sessions
            .get(doc_id)
            .ok_or_else(|| ServiceError::Conflict("Document session is not open".into()))?
            .cancel()
    }

    pub(crate) fn dirty_sessions(&self) -> Vec<&DocumentSession> {
        let mut dirty = self
            .sessions
            .values()
            .filter(|session| !session.closed && session.is_dirty())
            .collect::<Vec<_>>();
        dirty.sort_by(|left, right| left.doc_id.cmp(&right.doc_id));
        dirty
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::services::document::load_document_candidate;
    use crate::services::error::ServiceError;
    use crate::services::file_io::{FileIo, LocalFileIo};
    use crate::services::save_stream::SaveStream;
    use crate::services::session::SessionRegistry;

    static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);
    const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/bounded-raster.ptz");

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            for _ in 0..32 {
                let id = NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir()
                    .join(format!("photrez-session-{}-{id}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => panic!("create test directory: {error}"),
                }
            }
            panic!("could not allocate a unique test directory");
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn session_fixture() -> (TestDirectory, LocalFileIo, SessionRegistry, PathBuf) {
        let directory = TestDirectory::new();
        let files = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("file service opens");
        let source_path = directory.path().join("source.ptz");
        fs::write(&source_path, FIXTURE).expect("fixture is copied");
        files
            .approve_path(&source_path)
            .expect("fixture is approved");
        let loaded = load_document_candidate(&files, &source_path).expect("fixture loads");
        let mut registry = SessionRegistry::default();
        registry
            .open(loaded, "Fixture", Some(source_path.clone()))
            .expect("session opens");
        (directory, files, registry, source_path)
    }

    #[test]
    fn dirty_revision_is_tracked_and_enumerated() {
        let (_directory, _files, mut registry, _) = session_fixture();
        assert_eq!(registry.get("parity-doc").unwrap().revision(), 0);
        assert!(!registry.get("parity-doc").unwrap().is_dirty());

        assert_eq!(registry.mark_dirty("parity-doc").unwrap(), 1);
        assert_eq!(registry.mark_dirty("parity-doc").unwrap(), 2);
        let session = registry.get("parity-doc").unwrap();
        assert_eq!(session.revision(), 2);
        assert_eq!(session.saved_revision(), 0);
        assert!(session.is_dirty());
        assert_eq!(
            registry
                .dirty_sessions()
                .iter()
                .map(|session| session.doc_id())
                .collect::<Vec<_>>(),
            ["parity-doc"]
        );
    }

    #[test]
    fn successful_save_updates_path_after_archive_finalizes() {
        let (directory, files, mut registry, _) = session_fixture();
        registry.mark_dirty("parity-doc").unwrap();
        let target = directory.path().join("saved.ptz");
        let receipt = registry
            .save("parity-doc", &files, &target, &SaveStream)
            .expect("save completes");

        let file = fs::File::open(&target).expect("final archive exists");
        let mut archive = zip::ZipArchive::new(file).expect("final archive is complete");
        assert!(archive.by_name("document.json").is_ok());
        let session = registry.get("parity-doc").unwrap();
        assert_eq!(session.source_path(), Some(target.as_path()));
        assert_eq!(session.revision(), 1);
        assert_eq!(session.saved_revision(), 1);
        assert!(!session.is_dirty());
        registry
            .close_after_save("parity-doc", &receipt)
            .expect("matching receipt closes session");
        assert!(registry.get("parity-doc").is_none());
    }

    #[test]
    fn failed_save_preserves_path_dirty_and_open_state() {
        let (directory, files, mut registry, source_path) = session_fixture();
        registry.mark_dirty("parity-doc").unwrap();
        let target = directory.path().join("rejected.png");
        let error = registry
            .save("parity-doc", &files, &target, &SaveStream)
            .expect_err("non-project target is rejected");
        assert_eq!(error.code(), "E_VALIDATION");
        assert!(!target.exists());

        let session = registry.get("parity-doc").unwrap();
        assert_eq!(session.source_path(), Some(source_path.as_path()));
        assert_eq!(session.revision(), 1);
        assert_eq!(session.saved_revision(), 0);
        assert!(session.is_dirty());
        assert!(!session.is_closed());
    }

    #[test]
    fn stale_receipt_after_edit_and_cancel_does_not_close() {
        let (directory, files, mut registry, _) = session_fixture();
        let target = directory.path().join("saved.ptz");
        let receipt = registry
            .save("parity-doc", &files, &target, &SaveStream)
            .expect("save completes");
        registry.mark_dirty("parity-doc").unwrap();

        assert!(registry.close_after_save("parity-doc", &receipt).is_err());
        registry
            .cancel("parity-doc")
            .expect("cancel keeps session open");
        let session = registry.get("parity-doc").unwrap();
        assert!(session.is_dirty());
        assert!(!session.is_closed());
        assert!(registry.close_after_save("parity-doc", &receipt).is_err());
        assert!(registry.get("parity-doc").is_some());
    }

    #[test]
    fn replace_refuses_dirty_and_swaps_clean_session() {
        let (directory, files, mut registry, source_path) = session_fixture();
        registry.mark_dirty("parity-doc").unwrap();

        let replacement = load_document_candidate(&files, &source_path).expect("document reloads");
        let error = registry
            .replace(replacement, "Replacement", Some(source_path.clone()))
            .expect_err("a dirty session is not replaced silently");
        assert!(matches!(error, ServiceError::Conflict(_)));
        assert!(registry.get("parity-doc").unwrap().is_dirty());
        assert_eq!(registry.dirty_sessions().len(), 1);

        let target = directory.path().join("saved.ptz");
        registry
            .save("parity-doc", &files, &target, &SaveStream)
            .expect("save clears dirty");
        let replacement = load_document_candidate(&files, &source_path).expect("document reloads");
        registry
            .replace(replacement, "Replacement", Some(source_path.clone()))
            .expect("a clean session is replaced");
        let session = registry.get("parity-doc").unwrap();
        assert_eq!(session.display_name(), "Replacement");
        assert_eq!(session.revision(), 0);
        assert_eq!(session.saved_revision(), 0);
        assert!(!session.is_dirty());
    }

    #[test]
    fn discard_closes_while_cancel_keeps_dirty_session_open() {
        let (_directory, _files, mut registry, _) = session_fixture();
        registry.mark_dirty("parity-doc").unwrap();
        registry
            .cancel("parity-doc")
            .expect("cancel is non-destructive");
        assert!(registry.get("parity-doc").unwrap().is_dirty());
        registry
            .discard("parity-doc")
            .expect("discard closes session");
        assert!(registry.get("parity-doc").is_none());
        assert!(registry.dirty_sessions().is_empty());
    }

    #[test]
    fn receipt_cannot_close_reopened_session_with_same_document_id() {
        let (directory, files, mut registry, source_path) = session_fixture();
        let target = directory.path().join("saved.ptz");
        let receipt = registry
            .save("parity-doc", &files, &target, &SaveStream)
            .expect("save completes");
        registry
            .discard("parity-doc")
            .expect("first session closes");

        let replacement = load_document_candidate(&files, &source_path).expect("document reloads");
        registry
            .open(replacement, "Fixture", Some(source_path))
            .expect("replacement session opens");
        assert!(registry.close_after_save("parity-doc", &receipt).is_err());
        assert!(!registry.get("parity-doc").unwrap().is_closed());
    }
}
