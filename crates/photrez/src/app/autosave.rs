//! The auto-save timer: every 60 s every dirty session is written to the
//! app-cache autosave store. The status is surfaced in
//! the footer; failure never closes or mutates a document.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::Context;

use crate::services::autosave::AutosaveStore;
use crate::services::error::ServiceError;
use crate::services::file_io::FileIo;
use crate::services::save_stream::SaveStream;
use crate::state::{AutosaveStatus, RecoveryEntry};

use super::App;

const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(60);

impl App {
    /// Write every dirty session now and project the outcome into the footer.
    pub(crate) fn autosave_now(&mut self, cache_root: &Path, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.autosave_status = AutosaveStatus::Saving;
            state.autosave_error = None;
        });
        cx.notify();
        match self.autosave_dirty_sessions(cache_root) {
            Ok(_) => {
                self.state.update(cx, |state, _| {
                    state.autosave_status = AutosaveStatus::Saved;
                });
            }
            Err(error) => {
                self.state.update(cx, |state, _| {
                    state.autosave_status = AutosaveStatus::Error;
                    state.autosave_error = Some(error.to_string());
                });
            }
        }
        cx.notify();
    }

    /// Write every dirty session to the app-cache autosave store and return how
    /// many archives were written.
    ///
    /// `cache_root` is injected so a test never writes the real user cache. Each
    /// session is written on its own: one document whose snapshot fails must not
    /// stop the other dirty sessions from being snapshotted, and no session's
    /// dirty flag is cleared here.
    pub(crate) fn autosave_dirty_sessions(
        &mut self,
        cache_root: &Path,
    ) -> Result<usize, ServiceError> {
        let file_io = self.ensure_file_io().map_err(ServiceError::Internal)?;
        let store = AutosaveStore::new(cache_root);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        let mut saved = 0;
        let mut first_error = None;
        for session in self.sessions.dirty_sessions() {
            match store.autosave(
                session.doc_id(),
                session,
                file_io.as_ref(),
                &SaveStream,
                now_ms,
            ) {
                Ok(true) => saved += 1,
                Ok(false) => {}
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(saved),
        }
    }

    /// Start the 60 s autosave loop when an app-cache directory exists.
    pub(crate) fn start_autosave_timer(&mut self, cx: &mut Context<Self>) {
        // Tests mount the app without a cache root; they must not write the
        // real user cache or inherit a recovery prompt from another run.
        let Some(cache_root) = self.autosave_cache_root.clone() else {
            return;
        };
        // Opportunistic 7-day cleanup for stale autosaves (best effort).
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        let _ = AutosaveStore::new(&cache_root).cleanup_stale(now_ms);
        let task = cx.spawn(async move |weak, cx| loop {
            cx.background_executor().timer(AUTOSAVE_INTERVAL).await;
            let root: PathBuf = cache_root.clone();
            if weak
                .update(cx, |app, cx| app.autosave_now(&root, cx))
                .is_err()
            {
                break;
            }
        });
        self.autosave_task = Some(task);
    }

    /// Offer recoverable autosaves at startup. A malformed/empty manifest
    /// shows nothing; reading it never blocks opening a requested document.
    /// An archive whose document is already on screen, in a state the file was
    /// written at or after, is not offered: it cannot hold work the user has
    /// not got in front of them. An archive that is newer, or whose age cannot
    /// be established, is offered and marked as the document already on screen,
    /// so the choice reads against that document rather than as a new tab.
    pub(crate) fn prepare_recovery_from(&mut self, cache_root: &Path, cx: &mut Context<Self>) {
        let store = AutosaveStore::new(cache_root);
        let Ok(entries) = store.list() else {
            return;
        };
        if entries.is_empty() {
            return;
        }
        let mut rows = Vec::new();
        for entry in entries.iter() {
            if self.holds_state_of(entry) {
                continue;
            }
            rows.push(RecoveryEntry {
                doc_id: entry.doc_id().to_string(),
                name: entry.display_name().to_string(),
                already_open: self.sessions.get(entry.doc_id()).is_some(),
            });
        }
        if rows.is_empty() {
            return;
        }
        self.state.update(cx, |state, _| {
            state.recovery_entries = rows;
            state.active_dialog = Some(crate::state::DialogId::Recovery);
        });
        cx.notify();
    }

    /// True when an open session's own file already carries this archive's state
    /// or something newer. Identity is the open session's document ID plus that
    /// session's file path, which the boundary canonicalises before it reads it,
    /// never the display name. Recency is that file's last-modified time against
    /// the archive's write time. Anything unreadable, an archive with no recorded
    /// time, a document with no file behind it, or a timestamp the OS will not
    /// give, stays offered: a missing signal must not be read as stale, because
    /// the archive can be the only copy of unsaved work.
    fn holds_state_of(&mut self, entry: &crate::services::autosave::AutosaveEntry) -> bool {
        let Some(saved_at) = entry.unix_ms() else {
            return false;
        };
        let Some(source_path) = self
            .sessions
            .get(entry.doc_id())
            .and_then(|session| session.source_path())
            .map(|path| path.to_path_buf())
        else {
            return false;
        };
        let Ok(files) = self.ensure_file_io() else {
            return false;
        };
        let Ok(modified_ms) = files.modified_ms(&source_path) else {
            return false;
        };
        saved_at <= modified_ms
    }

    /// Open every recoverable autosave as a new unsaved tab and drop the ones
    /// that opened. The recovered document has no source path, so a later
    /// Ctrl+S prompts Save As instead of overwriting the recovery file.
    ///
    /// Each entry is restored on its own: an archive that cannot be read
    /// keeps the other archives restorable and keeps its own file in the store
    /// so the next start offers it again.
    pub(crate) fn restore_autosaves_from(&mut self, cache_root: &Path, cx: &mut Context<Self>) {
        let store = AutosaveStore::new(cache_root);
        let outcome = match (store.list(), self.ensure_file_io()) {
            (Ok(entries), Ok(file_io)) => {
                let total = entries.len();
                let mut restored = 0;
                let mut failures: Vec<String> = Vec::new();
                for entry in entries {
                    let name = entry.display_name().to_owned();
                    match self.restore_entry(&store, entry, file_io.as_ref(), cx) {
                        Ok(()) => restored += 1,
                        Err(reason) => failures.push(format!("{name}: {reason}")),
                    }
                }
                (total, restored, failures)
            }
            (Ok(entries), Err(error)) => (entries.len(), 0, vec![error]),
            (Err(error), _) => (
                0,
                0,
                vec![format!("cannot read the autosave manifest: {error}")],
            ),
        };
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.recovery_entries.clear();
        });
        let (total, restored, failures) = outcome;
        match failures.is_empty() {
            true => self.set_status_error(None, cx),
            false => {
                let summary = if restored == 0 {
                    format!("Recovery: {}", failures.join("; "))
                } else {
                    format!(
                        "Recovery: {restored} of {total} restored; {}",
                        failures.join("; ")
                    )
                };
                self.set_status_error(Some(summary), cx);
            }
        }
        self.project_tabs(cx);
        cx.notify();
    }

    /// Restore one entry and remove only that entry's archive once it is open.
    fn restore_entry(
        &mut self,
        store: &AutosaveStore,
        entry: crate::services::autosave::AutosaveEntry,
        file_io: &dyn FileIo,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let path = store
            .recovery_path(&entry)
            .map_err(|error| format!("cannot resolve the recovery file: {error}"))?;
        file_io
            .approve_path(&path)
            .map_err(|error| format!("cannot approve the recovery file: {error}"))?;
        let (loaded, canvas) = Self::prepare_startup_document(&path, file_io)?;
        self.install_document(loaded, canvas, None, cx)
            .map_err(|error| format!("cannot open the recovered document: {error}"))?;
        self.sessions
            .mark_dirty(entry.doc_id())
            .map_err(|error| format!("cannot mark the recovered document unsaved: {error}"))?;
        store
            .clear_one(entry.doc_id())
            .map(|_| ())
            .map_err(|error| format!("opened, but its autosave file stayed: {error}"))
    }

    /// Decline the offer without touching the store: every autosave is still
    /// there for the next start, and the 7-day cleanup bounds how long.
    pub(crate) fn decline_autosaves(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.recovery_entries.clear();
        });
        cx.notify();
    }

    /// Throw away the recoverable autosaves this prompt actually listed.
    ///
    /// A suppressed archive stays on disk. `holds_state_of` hides a row only when
    /// an open session provably holds the same or newer state, and the user never
    /// sees that row, so no choice of theirs can cover deleting it. Clearing the
    /// whole store here would destroy the copy that suppression left behind.
    pub(crate) fn discard_autosaves_from(&mut self, cache_root: &Path, cx: &mut Context<Self>) {
        let store = AutosaveStore::new(cache_root);
        let listed: Vec<String> = self
            .state
            .read(cx)
            .recovery_entries
            .iter()
            .map(|entry| entry.doc_id.clone())
            .collect();
        let mut failed = Vec::new();
        for doc_id in &listed {
            if let Err(error) = store.clear_one(doc_id) {
                failed.push(format!("{doc_id}: {error}"));
            }
        }
        if !failed.is_empty() {
            self.set_status_error(Some(format!("Recovery: {}", failed.join("; "))), cx);
        }
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.recovery_entries.clear();
        });
        cx.notify();
    }

    /// Startup entry point using the resolved app cache directory.
    pub(crate) fn prepare_startup_recovery(&mut self, cx: &mut Context<Self>) {
        if let Some(root) = self.autosave_cache_root.clone() {
            self.prepare_recovery_from(&root, cx);
        }
    }

    pub(crate) fn restore_autosaves(&mut self, cx: &mut Context<Self>) {
        if let Some(root) = self.autosave_cache_root.clone() {
            self.restore_autosaves_from(&root, cx);
        }
    }

    pub(crate) fn discard_autosaves(&mut self, cx: &mut Context<Self>) {
        if let Some(root) = self.autosave_cache_root.clone() {
            self.discard_autosaves_from(&root, cx);
        }
    }

    /// Point the store at a directory the test owns, so a mounted prompt
    /// exercises the real buttons without touching the user cache.
    #[cfg(test)]
    pub(crate) fn use_autosave_cache_root_for_test(&mut self, root: PathBuf) {
        self.autosave_cache_root = Some(root);
    }
}
