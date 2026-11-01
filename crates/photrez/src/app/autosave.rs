//! The auto-save timer: every 60 s every dirty session is written to the
//! app-cache autosave store. The status is surfaced in
//! the footer; failure never closes or mutates a document.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::Context;

use crate::services::autosave::AutosaveStore;
use crate::services::settings;
use crate::state::{AutosaveStatus, TabRow};

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

    /// Start the 60 s autosave loop when a cache directory exists.
    pub(crate) fn start_autosave_timer(&mut self, cx: &mut Context<Self>) {
        // Tests mount the app without settings; they must not write the real
        // user cache or inherit a recovery prompt from another run.
        if self.settings.is_none() {
            return;
        }
        let Some(cache_root) = settings::user_cache_root() else {
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
    pub(crate) fn prepare_recovery_from(&mut self, cache_root: &Path, cx: &mut Context<Self>) {
        let store = AutosaveStore::new(cache_root);
        let Ok(entries) = store.list() else {
            return;
        };
        if entries.is_empty() {
            return;
        }
        let rows = entries
            .iter()
            .map(|entry| TabRow {
                doc_id: entry.doc_id().to_string(),
                name: entry.display_name().to_string(),
                dirty: true,
            })
            .collect::<Vec<_>>();
        self.state.update(cx, |state, _| {
            state.recovery_entries = rows;
            state.active_dialog = Some(crate::state::DialogId::Recovery);
        });
        cx.notify();
    }

    /// Open every recoverable autosave as a new unsaved tab, then clear the
    /// store. The recovered document has no source path, so a later Ctrl+S
    /// prompts Save As instead of overwriting the recovery file.
    pub(crate) fn restore_autosaves_from(&mut self, cache_root: &Path, cx: &mut Context<Self>) {
        let result = (|| -> Result<(), String> {
            let store = AutosaveStore::new(cache_root);
            let entries = store
                .list()
                .map_err(|error| format!("cannot read the autosave manifest: {error}"))?;
            let file_io = self.ensure_file_io()?;
            for entry in entries {
                let path = store
                    .recovery_path(&entry)
                    .map_err(|error| format!("cannot resolve the recovery file: {error}"))?;
                file_io
                    .approve_path(&path)
                    .map_err(|error| format!("cannot approve the recovery file: {error}"))?;
                let (loaded, canvas) = Self::prepare_startup_document(&path, file_io.as_ref())?;
                self.install_document(loaded, canvas, None, cx)?;
            }
            store
                .clear_all()
                .map_err(|error| format!("cannot clear the autosave store: {error}"))?;
            Ok(())
        })();
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.recovery_entries.clear();
        });
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(format!("Recovery: {error}")), cx),
        }
        self.project_tabs(cx);
        cx.notify();
    }

    /// Throw away every recoverable autosave.
    pub(crate) fn discard_autosaves_from(&mut self, cache_root: &Path, cx: &mut Context<Self>) {
        let store = AutosaveStore::new(cache_root);
        if let Err(error) = store.clear_all() {
            self.set_status_error(Some(format!("Recovery: {error}")), cx);
        }
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.recovery_entries.clear();
        });
        cx.notify();
    }

    /// Startup entry point using the real app cache directory.
    pub(crate) fn prepare_startup_recovery(&mut self, cx: &mut Context<Self>) {
        if self.settings.is_none() {
            return;
        }
        if let Some(root) = settings::user_cache_root() {
            self.prepare_recovery_from(&root, cx);
        }
    }

    pub(crate) fn restore_autosaves(&mut self, cx: &mut Context<Self>) {
        if let Some(root) = settings::user_cache_root() {
            self.restore_autosaves_from(&root, cx);
        }
    }

    pub(crate) fn discard_autosaves(&mut self, cx: &mut Context<Self>) {
        if let Some(root) = settings::user_cache_root() {
            self.discard_autosaves_from(&root, cx);
        }
    }
}
