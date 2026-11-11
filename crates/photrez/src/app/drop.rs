//! OS file drop: dropped projects open as tabs; dropped rasters import as
//! layers of the active document (or as new documents when none is open).
//! Paths and the drop position are held between `Entered` and `Submit`
//! because the submit event carries only the position.

use std::path::PathBuf;

use gpui_kit::{Context, Pixels, Point};

use super::App;

fn is_project(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ptz"))
}

impl App {
    pub(crate) fn stage_dropped_paths(&mut self, paths: Vec<PathBuf>, position: Point<Pixels>) {
        self.dropped_paths = paths;
        self.dropped_position = Some(position);
    }

    pub(crate) fn clear_dropped_paths(&mut self) {
        self.dropped_paths.clear();
        self.dropped_position = None;
    }

    pub(crate) fn submit_dropped_paths(&mut self, cx: &mut Context<Self>) {
        let paths = std::mem::take(&mut self.dropped_paths);
        let position = self.dropped_position.take();
        if paths.is_empty() {
            return;
        }
        let mut errors = Vec::new();
        match self.active_doc_id() {
            Ok(doc_id) => {
                let (projects, images): (Vec<_>, Vec<_>) =
                    paths.into_iter().partition(|path| is_project(path));
                let image_placement = if images.is_empty() {
                    None
                } else {
                    let (doc_w, doc_h) = self.active_document_size().unwrap_or((1.0, 1.0));
                    let doc_size = (doc_w.max(1.0) as u32, doc_h.max(1.0) as u32);
                    let over_canvas = position.and_then(|position| {
                        crate::app::canvas_doc_coords_free(
                            position,
                            &self.canvas_bounds,
                            doc_size,
                            self.active_camera(),
                        )
                    });
                    // Canvas drops center on the cursor (the user aimed there);
                    // drops elsewhere cascade from the layer origin, matching
                    // the layers-panel drop path.
                    Some(match over_canvas {
                        Some(at) => (at, true),
                        None => ((0.0, 0.0), false),
                    })
                };
                for path in projects {
                    if let Err(error) = self.open_path_file(&path, cx) {
                        errors.push(error);
                    }
                }
                if let Some((base, center)) = image_placement {
                    if let Err(error) =
                        self.drop_files_as_layers(&images, &doc_id, base, center, cx)
                    {
                        errors.push(error);
                    }
                }
            }
            Err(_) => {
                for path in paths {
                    if let Err(error) = self.open_path_file(&path, cx) {
                        errors.push(error);
                    }
                }
            }
        }
        if errors.is_empty() {
            self.set_status_error(None, cx);
        } else {
            self.set_status_error(Some(format!("Drop: {}", errors.join("; "))), cx);
        }
    }
}
