//! OS file drop onto the canvas: dropped projects open as tabs, dropped
//! rasters import. Paths are held between `Entered` and `Submit` because the
//! submit event carries only the position.

use std::path::PathBuf;

use gpui_kit::Context;

use super::App;

impl App {
    pub(crate) fn stage_dropped_paths(&mut self, paths: Vec<PathBuf>) {
        self.dropped_paths = paths;
    }

    pub(crate) fn clear_dropped_paths(&mut self) {
        self.dropped_paths.clear();
    }

    pub(crate) fn submit_dropped_paths(&mut self, cx: &mut Context<Self>) {
        let paths = std::mem::take(&mut self.dropped_paths);
        if paths.is_empty() {
            return;
        }
        let mut errors = Vec::new();
        for path in paths {
            if let Err(error) = self.open_path_file(&path, cx) {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            self.set_status_error(None, cx);
        } else {
            self.set_status_error(Some(format!("Drop: {}", errors.join("; "))), cx);
        }
    }
}
