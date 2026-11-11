//! Asynchronous document opening behind the live loading overlay.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{AppContext, Context};

use super::App;
use crate::display::canvas_bridge::CanvasImage;
use crate::services::document::{load_document_candidate_with_progress, LoadedDocument};
use crate::services::file_io::FileIo;

const PROGRESS_POLL: Duration = Duration::from_millis(50);

impl App {
    /// Open a project or import a raster without blocking the window. Project
    /// loads report per-layer progress; a raster decode stays indeterminate
    /// until it returns. Startup and OS-drop opens stay synchronous.
    pub(crate) fn open_path_async(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.state.read(cx).loading.is_some() {
            self.set_status_error(Some("Open: a document is already loading".into()), cx);
            return;
        }
        let file_io = match self.ensure_file_io() {
            Ok(file_io) => file_io,
            Err(error) => {
                self.set_status_error(Some(format!("Open failed: {error}")), cx);
                return;
            }
        };
        let generation = self.begin_loading(cx);
        if is_project_path(&path) {
            let progress = Arc::new(AtomicU32::new(0));
            let worker_progress = Arc::clone(&progress);
            let project_path = path.clone();
            cx.spawn(async move |this, cx| {
                let prepared = cx
                    .background_spawn(async move {
                        prepare_project(&project_path, file_io.as_ref(), &worker_progress)
                    })
                    .await;
                let result = this.update(cx, |app, cx| match prepared {
                    Ok((loaded, canvas)) => app.install_document(loaded, canvas, Some(path), cx),
                    Err(error) => Err(error),
                });
                let _ = this.update(cx, |app, cx| {
                    app.end_loading(cx);
                    match result {
                        Ok(Ok(())) => app.set_status_error(None, cx),
                        Ok(Err(error)) => {
                            app.set_status_error(Some(format!("Open failed: {error}")), cx)
                        }
                        Err(_) => {}
                    }
                });
            })
            .detach();
            watch_open_progress(generation, progress, cx);
        } else {
            let decode_path = path.clone();
            cx.spawn(async move |this, cx| {
                let decoded = cx
                    .background_spawn(async move {
                        crate::services::image_files::decode_import(file_io.as_ref(), &decode_path)
                    })
                    .await;
                let result = this.update(cx, |app, cx| match decoded {
                    Ok(decoded) => app.install_import(decoded, &path, cx),
                    Err(error) => Err(format!("Open failed ({}): {error}", error.code())),
                });
                let _ = this.update(cx, |app, cx| {
                    app.end_loading(cx);
                    match result {
                        Ok(Ok(())) => app.set_status_error(None, cx),
                        Ok(Err(error)) => app.set_status_error(Some(error), cx),
                        Err(_) => {}
                    }
                });
            })
            .detach();
        }
    }
}

/// Poll the background loader's fraction. The loader cannot touch App state
/// from its thread.
fn watch_open_progress(generation: u64, progress: Arc<AtomicU32>, cx: &mut Context<App>) {
    cx.spawn(async move |this, cx| loop {
        cx.background_executor().timer(PROGRESS_POLL).await;
        let stop = this
            .update(cx, |app, cx| {
                let loading = app.state.read(cx).loading;
                if app.loading_generation != generation || loading.is_none() {
                    return true;
                }
                let fraction = progress.load(Ordering::Relaxed) as f32 / 1000.0;
                if fraction > 0.0 {
                    app.set_loading_progress(fraction, cx);
                }
                false
            })
            .unwrap_or(true);
        if stop {
            break;
        }
    })
    .detach();
}

fn prepare_project(
    path: &Path,
    file_io: &dyn FileIo,
    progress: &AtomicU32,
) -> Result<(LoadedDocument, CanvasImage), String> {
    if !super::is_existing_ptz_file(path) {
        return Err("startup path must be an existing regular .ptz file".to_owned());
    }
    file_io
        .approve_path(path)
        .map_err(|error| format!("path approval failed: {error:?}"))?;
    let mut loaded = load_document_candidate_with_progress(file_io, path, &mut |fraction| {
        let scaled = (fraction.clamp(0.0, 1.0) * 1000.0).round() as u32;
        progress.store(scaled, Ordering::Relaxed);
    })
    .map_err(|error| format!("document load failed: {error:?}"))?;
    let rgba = std::mem::take(&mut loaded.display_rgba);
    let canvas = CanvasImage::from_rgba(loaded.display_width, loaded.display_height, rgba)
        .map_err(|error| format!("document presentation failed: {error:?}"))?;
    Ok((loaded, canvas))
}

fn is_project_path(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ptz"))
}
