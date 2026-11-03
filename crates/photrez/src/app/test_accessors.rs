//! Accessors and staged states the mounted tests drive; the shell does not use
//! them.

use super::*;

impl App {
    #[cfg(test)]
    pub(crate) fn state_for_test(&self) -> &Entity<AppState> {
        &self.state
    }

    #[cfg(test)]
    pub(crate) fn active_document_id_for_test(&self) -> Option<String> {
        self.active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
    }

    #[cfg(test)]
    pub(crate) fn active_document_pixel_lengths_for_test(&self) -> Option<(usize, usize)> {
        let doc_id = self.active_document.as_ref()?.doc_id.as_str();
        let session = self.sessions.get(doc_id)?;
        let loaded = session.loaded_document();
        let canonical = loaded.registry.get_layer(doc_id, "layer-1")?;
        Some((canonical.pixels.len(), loaded.display_rgba.len()))
    }

    #[cfg(test)]
    pub(crate) fn mark_active_document_dirty_for_test(
        &mut self,
    ) -> Result<u64, crate::services::error::ServiceError> {
        let doc_id = self
            .active_document
            .as_ref()
            .ok_or_else(|| {
                crate::services::error::ServiceError::Conflict("no active document".into())
            })?
            .doc_id
            .clone();
        self.sessions.mark_dirty(&doc_id)
    }

    #[cfg(test)]
    pub(crate) fn set_print_backend_for_test(&mut self, backend: Arc<dyn PrintDispatch>) {
        self.print_backend = backend;
    }

    #[cfg(test)]
    pub(crate) fn active_document_save_state_for_test(&self) -> Option<(u64, u64, bool)> {
        let doc_id = self.active_document.as_ref()?.doc_id.as_str();
        let session = self.sessions.get(doc_id)?;
        Some((
            session.revision(),
            session.saved_revision(),
            session.is_dirty(),
        ))
    }

    #[cfg(test)]
    pub(crate) fn stage_startup_document_for_test(
        &self,
        path: &Path,
        file_io: &dyn FileIo,
    ) -> Result<(String, u32, u32), String> {
        Self::prepare_startup_document(path, file_io).map(|(loaded, _)| {
            (
                loaded.document_id().to_owned(),
                loaded.display_width,
                loaded.display_height,
            )
        })
    }

    #[cfg(test)]
    pub(crate) fn open_startup_document_for_test(
        &mut self,
        path: &Path,
        file_io: &dyn FileIo,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.open_startup_document(path, file_io, cx)
    }
}
