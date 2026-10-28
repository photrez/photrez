//! Document commands: open, save, save as, export, and print.
//!
//! The handlers live here so `app.rs` keeps construction and rendering; every
//! filesystem and platform operation still goes through the services.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use gpui_kit::{AppContext as _, Context, PathPromptOptions};

use super::{
    ensure_ptz_path, App, CloseDocument, ExportDocument, OpenDocument, PrintDocument, SaveDocument,
    SaveDocumentAs,
};
use crate::render::canvas_bridge::CanvasImage;
use crate::services::document::LoadedDocument;
use crate::services::error::ServiceError;
use crate::services::image_files::{self, ImageFormat};
use crate::services::print::{PrintCancellation, PrintError, PrintRequest, PrintSettings};

/// Paper presets shown by the Print dialog: (name, driver index, width mm,
/// height mm). The driver index matches the Windows DMPAPER values for the
/// common sizes; selecting a printer's own list is still open.
pub(crate) const PRINT_PAPERS: &[(&str, i16, f64, f64)] = &[
    ("A4", 9, 210.0, 297.0),
    ("Letter", 1, 215.9, 279.4),
    ("Legal", 5, 215.9, 355.6),
    ("A5", 11, 148.0, 210.0),
];
use crate::services::save_stream::SaveStream;
use crate::state::DialogId;

static NEXT_IMPORT_ID: AtomicU64 = AtomicU64::new(1);

impl App {
    fn save_active_document(&mut self) -> Result<(), ServiceError> {
        let doc_id = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
            .ok_or_else(|| ServiceError::Conflict("no active document to save".into()))?;
        let file_io = self
            .file_io
            .clone()
            .ok_or_else(|| ServiceError::Conflict("no file boundary is open".into()))?;
        let path = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.source_path())
            .map(Path::to_path_buf)
            .ok_or_else(|| ServiceError::Conflict("the document has no save path".into()))?;
        self.sessions
            .save(&doc_id, file_io.as_ref(), &path, &SaveStream)
            .map(|_| ())
    }

    /// Called by the platform close request. `false` keeps the window open.
    pub(crate) fn should_close_window(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(doc_id) = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
        else {
            return true;
        };
        let dirty = self
            .sessions
            .get(&doc_id)
            .is_some_and(|session| session.is_dirty());
        if !dirty {
            return true;
        }
        self.close_after_dirty = true;
        self.pending_tab_close = None;
        self.state.update(cx, |state, _| {
            state.active_dialog = Some(DialogId::DirtyClose);
        });
        cx.notify();
        false
    }

    pub(crate) fn close_document_action(&mut self, _: &CloseDocument, cx: &mut Context<Self>) {
        self.close_after_dirty = false;
        self.pending_tab_close = None;
        let Some(doc_id) = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
        else {
            return;
        };
        let dirty = self
            .sessions
            .get(&doc_id)
            .is_some_and(|session| session.is_dirty());
        if dirty {
            self.state.update(cx, |state, _| {
                state.active_dialog = Some(DialogId::DirtyClose);
            });
            cx.notify();
            return;
        }
        self.close_active_document(cx);
    }

    fn close_active_document(&mut self, cx: &mut Context<Self>) {
        if self.close_after_dirty {
            self.pending_window_close = true;
        }
        self.close_after_dirty = false;
        let Some(doc_id) = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
        else {
            return;
        };
        if self.sessions.get(&doc_id).is_some() {
            let _ = self.sessions.discard(&doc_id);
        }
        if let Some(retired) = self.active_document.take() {
            self.retired_textures.extend(retired.canvas.into_textures());
        }
        self.close_after_save = false;
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.status_error = None;
            state.has_document = false;
            state.document_id = None;
            state.document_name = "Baseline".to_owned(); // i18n-exempt: demo content
        });
        cx.notify();
    }

    /// Cancel the dirty-close prompt; the document/tab stays open and dirty.
    pub(crate) fn dirty_close_cancel(&mut self, cx: &mut Context<Self>) {
        self.close_after_save = false;
        self.close_after_dirty = false;
        self.pending_tab_close = None;
        self.state.update(cx, |state, _| state.active_dialog = None);
        cx.notify();
    }

    /// Discard the document or tab without saving.
    pub(crate) fn dirty_close_discard(&mut self, cx: &mut Context<Self>) {
        self.finish_pending_close(cx);
    }

    /// Close whatever the dirty prompt was guarding: a tab when one is
    /// pending, otherwise the window's active document.
    fn finish_pending_close(&mut self, cx: &mut Context<Self>) {
        if let Some(doc_id) = self.pending_tab_close.take() {
            self.state.update(cx, |state, _| state.active_dialog = None);
            self.close_document_tab_unchecked(&doc_id, cx);
        } else {
            self.close_active_document(cx);
        }
    }

    /// Save then close. A failure keeps the document open and dirty.
    pub(crate) fn dirty_close_save(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.active_dialog = None);
        let has_source = self
            .active_document
            .as_ref()
            .and_then(|document| self.sessions.get(&document.doc_id))
            .and_then(|session| session.source_path())
            .is_some();
        if !has_source {
            self.close_after_save = true;
            self.prompt_save_as(cx);
            return;
        }
        match self.save_active_document() {
            Ok(()) => self.finish_pending_close(cx),
            Err(error) => {
                let status = format!("Save failed ({}): {error}", error.code());
                self.set_status_error(Some(status), cx);
            }
        }
    }

    pub(crate) fn save_document_action(&mut self, _: &SaveDocument, cx: &mut Context<Self>) {
        let has_source = self
            .active_document
            .as_ref()
            .and_then(|document| self.sessions.get(&document.doc_id))
            .and_then(|session| session.source_path())
            .is_some();
        if !has_source {
            self.prompt_save_as(cx);
            return;
        }
        let result = self.save_active_document();
        let error = result
            .as_ref()
            .err()
            .map(|error| format!("Save failed ({}): {error}", error.code()));
        self.set_status_error(error, cx);
    }

    pub(crate) fn save_document_as_action(&mut self, _: &SaveDocumentAs, cx: &mut Context<Self>) {
        self.prompt_save_as(cx);
    }

    fn prompt_save_as(&mut self, cx: &mut Context<Self>) {
        let Some((directory, suggested)) = self.save_as_prompt() else {
            self.set_status_error(Some("Save As failed: no active document".into()), cx);
            return;
        };
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn(async move |this, cx| {
            let selected = match receiver.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) => {
                    this.update(cx, |app, cx| app.dirty_close_cancel(cx)).ok();
                    return;
                }
                Ok(Err(error)) => {
                    let status = format!("Save As failed: {error:?}");
                    this.update(cx, |app, cx| {
                        app.close_after_save = false;
                        app.set_status_error(Some(status), cx);
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    let status = format!("Save As dialog failed: {error:?}");
                    this.update(cx, |app, cx| {
                        app.close_after_save = false;
                        app.set_status_error(Some(status), cx);
                    })
                    .ok();
                    return;
                }
            };
            let result = this.update(cx, |app, cx| app.save_active_document_to(&selected, cx));
            if let Ok(Err(error)) = result {
                this.update(cx, |app, cx| app.set_status_error(Some(error), cx))
                    .ok();
            }
        })
        .detach();
    }

    fn save_as_prompt(&self) -> Option<(PathBuf, String)> {
        self.active_document.as_ref()?;
        Some((
            self.suggested_directory(),
            format!("{}.ptz", self.suggested_stem()),
        ))
    }

    fn save_active_document_to(
        &mut self,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let path = ensure_ptz_path(path)?;
        let doc_id = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
            .ok_or_else(|| "Save As failed: no active document".to_owned())?;
        let file_io = self.ensure_file_io()?;
        file_io
            .approve_path(&path)
            .map_err(|error| format!("Save As failed ({}): {error}", error.code()))?;
        self.sessions
            .save(&doc_id, file_io.as_ref(), &path, &SaveStream)
            .map_err(|error| format!("Save As failed ({}): {error}", error.code()))?;
        self.state.update(cx, |state, _| state.status_error = None);
        cx.notify();
        if self.close_after_save {
            self.finish_pending_close(cx);
        }
        Ok(())
    }

    pub(crate) fn open_document_action(&mut self, _: &OpenDocument, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn(async move |this, cx| {
            let selected = match receiver.await {
                Ok(Ok(Some(mut paths))) => paths.pop(),
                Ok(Ok(None)) => return,
                Ok(Err(error)) => {
                    let status = format!("Open failed: {error:?}");
                    this.update(cx, |app, cx| app.set_status_error(Some(status), cx))
                        .ok();
                    return;
                }
                Err(error) => {
                    let status = format!("Open dialog failed: {error:?}");
                    this.update(cx, |app, cx| app.set_status_error(Some(status), cx))
                        .ok();
                    return;
                }
            };
            let Some(path) = selected else {
                return;
            };

            if path
                .extension()
                .and_then(OsStr::to_str)
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ptz"))
            {
                let result = this.update(cx, |app, cx| {
                    let file_io = app.ensure_file_io()?;
                    app.open_startup_document(&path, file_io.as_ref(), cx)
                });
                if let Ok(Err(error)) = result {
                    let status = format!("Open failed: {error}");
                    this.update(cx, |app, cx| app.set_status_error(Some(status), cx))
                        .ok();
                }
                return;
            }

            let file_io = match this.update(cx, |app, _| app.ensure_file_io()) {
                Ok(Ok(file_io)) => file_io,
                Ok(Err(error)) => {
                    let status = format!("Open failed: {error}");
                    this.update(cx, |app, cx| app.set_status_error(Some(status), cx))
                        .ok();
                    return;
                }
                Err(_) => return,
            };
            let decode_path = path.clone();
            let decoded = cx
                .background_spawn(async move {
                    image_files::decode_import(file_io.as_ref(), &decode_path)
                })
                .await;
            let result = match decoded {
                Ok(decoded) => this.update(cx, |app, cx| app.install_import(decoded, &path, cx)),
                Err(error) => {
                    let status = format!("Open failed ({}): {error}", error.code());
                    this.update(cx, |app, cx| app.set_status_error(Some(status), cx))
                        .ok();
                    return;
                }
            };
            if let Ok(Err(error)) = result {
                let status = format!("Open failed: {error}");
                this.update(cx, |app, cx| app.set_status_error(Some(status), cx))
                    .ok();
            }
        })
        .detach();
    }

    /// Open one project or import one raster synchronously. Used by the OS
    /// file-drop path; the picker path keeps its async decode.
    pub(crate) fn open_path_file(
        &mut self,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ptz"))
        {
            let file_io = self.ensure_file_io()?;
            return self.open_startup_document(path, file_io.as_ref(), cx);
        }
        let file_io = self.ensure_file_io()?;
        let decoded = image_files::decode_import(file_io.as_ref(), path)
            .map_err(|error| format!("Open failed ({}): {error}", error.code()))?;
        self.install_import(decoded, path, cx)
    }

    fn install_import(
        &mut self,
        decoded: image_files::DecodedImport,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .filter(|name| !name.is_empty())
            .unwrap_or("Imported image")
            .to_owned();
        let id = format!(
            "import-{}-{}",
            std::process::id(),
            NEXT_IMPORT_ID.fetch_add(1, Ordering::Relaxed)
        );
        let mut loaded =
            LoadedDocument::imported_image(id, name, decoded.width, decoded.height, decoded.rgba)
                .map_err(|error| format!("import failed: {error:?}"))?;
        let rgba = std::mem::take(&mut loaded.display_rgba);
        let canvas = CanvasImage::from_rgba(loaded.display_width, loaded.display_height, rgba)
            .map_err(|error| format!("image presentation failed: {error:?}"))?;
        self.install_document(loaded, canvas, None, cx)
    }

    pub(crate) fn export_document_action(&mut self, _: &ExportDocument, cx: &mut Context<Self>) {
        if self.active_document.is_none() {
            self.set_status_error(Some("Export failed: no active document".into()), cx);
            return;
        }
        self.state.update(cx, |state, _| {
            state.active_dialog = Some(DialogId::Export);
        });
        cx.notify();
    }

    /// The dialog's Export button: pick a path and encode with the chosen
    /// format/quality.
    pub(crate) fn export_now(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.active_dialog = None);
        let Some((directory, suggested)) = self.export_prompt(cx) else {
            self.set_status_error(Some("Export failed: no active document".into()), cx);
            return;
        };
        let quality = self.state.read(cx).export_quality;
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn(async move |this, cx| {
            let selected = match receiver.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) => return,
                Ok(Err(error)) => {
                    let status = format!("Export failed: {error:?}");
                    this.update(cx, |app, cx| app.set_status_error(Some(status), cx))
                        .ok();
                    return;
                }
                Err(error) => {
                    let status = format!("Export dialog failed: {error:?}");
                    this.update(cx, |app, cx| app.set_status_error(Some(status), cx))
                        .ok();
                    return;
                }
            };
            let prepared = this.update(cx, |app, _| app.prepare_export(&selected));
            let (width, height, rgba, format) = match prepared {
                Ok(Ok(prepared)) => prepared,
                Ok(Err(error)) => {
                    this.update(cx, |app, cx| app.set_status_error(Some(error), cx))
                        .ok();
                    return;
                }
                Err(_) => return,
            };
            let encoded = cx
                .background_spawn(async move {
                    image_files::encode_export(width, height, &rgba, format, quality)
                })
                .await;
            let bytes = match encoded {
                Ok(bytes) => bytes,
                Err(error) => {
                    let status = format!("Export failed ({}): {error}", error.code());
                    this.update(cx, |app, cx| app.set_status_error(Some(status), cx))
                        .ok();
                    return;
                }
            };
            let result = this.update(cx, |app, cx| app.write_export(&selected, &bytes, cx));
            if let Ok(Err(error)) = result {
                this.update(cx, |app, cx| app.set_status_error(Some(error), cx))
                    .ok();
            }
        })
        .detach();
    }

    fn export_prompt(&self, cx: &Context<Self>) -> Option<(PathBuf, String)> {
        self.active_document.as_ref()?;
        let extension = self.state.read(cx).export_format.extension();
        Some((
            self.suggested_directory(),
            format!("{}.{extension}", self.suggested_stem()),
        ))
    }

    fn suggested_directory(&self) -> PathBuf {
        self.active_document
            .as_ref()
            .and_then(|document| self.sessions.get(&document.doc_id))
            .and_then(|session| session.source_path())
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn suggested_stem(&self) -> String {
        self.active_document
            .as_ref()
            .and_then(|document| self.sessions.get(&document.doc_id))
            .map(|session| {
                Path::new(session.display_name())
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .filter(|stem| !stem.is_empty())
                    .unwrap_or("Untitled")
                    .to_owned()
            })
            .unwrap_or_else(|| "Untitled".to_owned())
    }

    fn prepare_export(&self, path: &Path) -> Result<(u32, u32, Vec<u8>, ImageFormat), String> {
        let format = ImageFormat::from_export_path(path)
            .map_err(|error| format!("Export failed ({}): {error}", error.code()))?;
        let doc_id = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
            .ok_or_else(|| "Export failed: no active document".to_owned())?;
        let session = self
            .sessions
            .get(&doc_id)
            .ok_or_else(|| "Export failed: the document session is not open".to_owned())?;
        let loaded = session.loaded_document();
        let content = loaded
            .content_snapshot()
            .ok_or_else(|| "Export failed: the document content is missing".to_owned())?;
        let composite = photrez_core::render::compositor::composite_layers_with_font_data(
            content,
            &loaded.registry,
            Some(crate::INTER),
        )
        .map_err(|error| format!("Export failed: document render failed: {error}"))?;
        Ok((composite.width, composite.height, composite.pixels, format))
    }

    fn write_export(
        &mut self,
        path: &Path,
        bytes: &[u8],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        image_files::write_export(path, bytes)
            .map_err(|error| format!("Export failed ({}): {error}", error.code()))?;
        self.state.update(cx, |state, _| state.status_error = None);
        cx.notify();
        Ok(())
    }

    /// Print the active document's validated canonical raster snapshot with
    /// the settings chosen in the Print dialog.
    fn print_active_document(&self, settings: &PrintSettings) -> Result<(), PrintError> {
        let doc_id = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
            .ok_or_else(|| {
                PrintError::Service(ServiceError::Conflict("no active document to print".into()))
            })?;
        let session = self.sessions.get(&doc_id).ok_or_else(|| {
            PrintError::Service(ServiceError::Conflict(
                "document session is not open".into(),
            ))
        })?;
        let loaded = session.loaded_document();
        let content = loaded.content_snapshot().ok_or_else(|| {
            PrintError::Service(ServiceError::Internal(
                "loaded document content is missing".into(),
            ))
        })?;
        let rendered = photrez_core::render::compositor::composite_layers_with_font_data(
            content,
            &loaded.registry,
            Some(crate::INTER),
        )
        .map_err(|error| {
            PrintError::Service(ServiceError::Validation(format!(
                "document render failed: {error}"
            )))
        })?;
        let request = PrintRequest {
            pixels: &rendered.pixels,
            width: rendered.width,
            height: rendered.height,
            settings,
            document_name: session.display_name(),
            output_path: None,
        };
        self.print_backend
            .print(&request, &PrintCancellation::default())
    }

    pub(crate) fn print_document_action(&mut self, _: &PrintDocument, cx: &mut Context<Self>) {
        if self.active_document.is_none() {
            self.set_status_error(
                Some("Print failed (E_CONFLICT): no active document".into()),
                cx,
            );
            return;
        }
        let printers = crate::services::print::PrintService::default()
            .list_printers()
            .unwrap_or_default();
        self.state.update(cx, |state, _| {
            state.printers = printers;
            state.print_printer = 0;
            state.active_dialog = Some(DialogId::Print);
        });
        cx.notify();
    }

    /// The Print dialog's Print button.
    pub(crate) fn print_now(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.active_dialog = None);
        let (copies, landscape, paper_index, printer) = {
            let state = self.state.read(cx);
            (
                state.print_copies,
                state.print_landscape,
                state.print_paper.min(PRINT_PAPERS.len() - 1),
                state.printers.get(state.print_printer).cloned(),
            )
        };
        let (name, index, width_mm, height_mm) = PRINT_PAPERS[paper_index];
        let mut settings = PrintSettings::default();
        settings.set_copies(copies.max(1));
        settings.set_orientation(if landscape {
            crate::services::print::Orientation::Landscape
        } else {
            crate::services::print::Orientation::Portrait
        });
        settings.set_paper(name, index, width_mm, height_mm);
        settings.set_selected_printer(printer);
        let result = self.print_active_document(&settings);
        let error = result
            .as_ref()
            .err()
            .map(|error| format!("Print failed ({}): {error}", error.code()));
        self.set_status_error(error, cx);
    }

    #[cfg(test)]
    pub(crate) fn export_to_path_for_test(
        &mut self,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (width, height, rgba, format) = self.prepare_export(path)?;
        let quality = self.state.read(cx).export_quality;
        let bytes = image_files::encode_export(width, height, &rgba, format, quality)
            .map_err(|error| format!("Export failed ({}): {error}", error.code()))?;
        self.write_export(path, &bytes, cx)
    }

    #[cfg(test)]
    pub(crate) fn save_to_path_for_test(
        &mut self,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc_id = self.active_doc_id()?;
        let file_io = self.ensure_file_io()?;
        file_io
            .approve_path(path)
            .map_err(|error| format!("Save As failed ({}): {error}", error.code()))?;
        self.sessions
            .save(&doc_id, file_io.as_ref(), path, &SaveStream)
            .map_err(|error| format!("Save As failed ({}): {error}", error.code()))?;
        self.state.update(cx, |state, _| state.status_error = None);
        self.project_tabs(cx);
        cx.notify();
        Ok(())
    }

    pub(crate) fn set_status_error(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.status_error = error);
        cx.notify();
    }
}
