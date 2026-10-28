//! Recent project files: persisted in the flat settings store, recorded when a
//! project opens, listed at the bottom of the File menu.

use std::path::{Path, PathBuf};

use gpui_kit::Context;

use crate::services::settings::SettingsStore;

use super::App;

const RECENT_KEY: &str = "photrez.recent";
const MAX_RECENT: usize = 10;

impl App {
    /// Load the recent-project list from the settings store at startup.
    pub(crate) fn load_recent_files(&mut self, cx: &mut Context<Self>) {
        let Some(settings) = &self.settings else {
            return;
        };
        let Some(raw) = settings.read(RECENT_KEY) else {
            return;
        };
        let Ok(paths) = serde_json::from_str::<Vec<String>>(&raw) else {
            return;
        };
        self.recent_files = paths
            .into_iter()
            .map(PathBuf::from)
            .take(MAX_RECENT)
            .collect();
        self.project_recent_files(cx);
    }

    /// Put `path` at the front of the recent list and persist it.
    pub(crate) fn record_recent_file(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.recent_files.retain(|existing| existing != path);
        self.recent_files.insert(0, path.to_path_buf());
        self.recent_files.truncate(MAX_RECENT);
        if let Some(settings) = &self.settings {
            let values: Vec<String> = self
                .recent_files
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
            if let Ok(raw) = serde_json::to_string(&values) {
                settings.write(RECENT_KEY, &raw);
            }
        }
        self.project_recent_files(cx);
    }

    fn project_recent_files(&mut self, cx: &mut Context<Self>) {
        let values: Vec<String> = self
            .recent_files
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        self.state
            .update(cx, |state, _| state.recent_files = values);
    }

    pub(crate) fn open_recent_path(&mut self, raw: &str, cx: &mut Context<Self>) {
        let path = PathBuf::from(raw);
        let result = self.open_startup_path(&path, cx);
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(format!("Open: {error}")), cx),
        }
    }
}
