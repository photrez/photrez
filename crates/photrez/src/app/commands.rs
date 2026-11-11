//! Document commands: open, save, save as, export, and print.
//!
//! The handlers live here so `app.rs` keeps construction and rendering; every
//! filesystem and platform operation still goes through the services.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use gpui_kit::{AppContext as _, Context, PathPromptOptions, Window};

use super::{
    ensure_ptz_path, App, CloseDocument, ExportDocument, NewDocumentAction, OpenDocument,
    PrintDocument, SaveDocument, SaveDocumentAs, ToggleSidePanelsAction,
};
use crate::display::canvas_bridge::CanvasImage;
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
        self.save_document(&doc_id)
    }

    fn save_document(&mut self, doc_id: &str) -> Result<(), ServiceError> {
        let file_io = self
            .file_io
            .clone()
            .ok_or_else(|| ServiceError::Conflict("no file boundary is open".into()))?;
        let path = self
            .sessions
            .get(doc_id)
            .and_then(|session| session.source_path())
            .map(Path::to_path_buf)
            .ok_or_else(|| ServiceError::Conflict("the document has no save path".into()))?;
        self.sessions
            .save(doc_id, file_io.as_ref(), &path, &SaveStream)
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
        self.pending_close = Some(super::CloseIntent::Window { doc_id });
        self.state.update(cx, |state, _| {
            state.active_dialog = Some(DialogId::DirtyClose);
        });
        cx.notify();
        false
    }

    pub(crate) fn close_window_from_title_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.should_close_window(cx) {
            window.remove_window();
        }
    }

    /// File > New: open the New Document dialog and close any
    /// open menu surface first.
    pub(crate) fn new_document_action(&mut self, _: &NewDocumentAction, cx: &mut Context<Self>) {
        self.open_new_document(cx);
    }

    /// View > Toggle Side Panels: hide or show the right dock. The menu
    /// row label flips between Hide/Show; the value drives `dock_open`.
    pub(crate) fn toggle_side_panels_action(
        &mut self,
        _: &ToggleSidePanelsAction,
        cx: &mut Context<Self>,
    ) {
        self.state
            .update(cx, |state, _| state.dock_open = !state.dock_open);
        cx.notify();
    }

    /// The status bar's History trigger: show the dock's History panel.
    /// This opens the dock as well, since a closed dock would show a panel
    /// the user cannot see.
    pub(crate) fn show_history_panel(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.layer_tab = crate::state::LayerTab::History;
            state.dock_open = true;
        });
        cx.notify();
    }

    pub(crate) fn close_document_action(&mut self, _: &CloseDocument, cx: &mut Context<Self>) {
        self.pending_close = None;
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
            self.pending_close = Some(super::CloseIntent::Document { doc_id });
            self.state.update(cx, |state, _| {
                state.active_dialog = Some(DialogId::DirtyClose);
            });
            cx.notify();
            return;
        }
        self.close_active_document(cx);
    }

    fn close_active_document(&mut self, cx: &mut Context<Self>) {
        let Some(doc_id) = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
        else {
            return;
        };
        self.close_document_by_id(&doc_id, cx);
    }

    fn close_document_by_id(&mut self, doc_id: &str, cx: &mut Context<Self>) {
        if !self
            .active_document
            .as_ref()
            .is_some_and(|document| document.doc_id == doc_id)
        {
            self.close_document_tab_unchecked(doc_id, cx);
            return;
        }
        if self.sessions.get(doc_id).is_some() {
            let _ = self.sessions.discard(doc_id);
        }
        if let Some(retired) = self.active_document.take() {
            self.retired_textures.extend(retired.canvas.into_textures());
        }
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.status_error = None;
            state.has_document = false;
            state.document_id = None;
            state.document_name = "Baseline".to_owned(); // i18n-exempt: demo content
                                                         // Nothing is on screen once the last document closes, so the
                                                         // panels must not keep projecting its layers or history.
            state.layers.clear();
            state.selected_layers.clear();
            state.paint_target_core_id = None;
            state.layer_flags.clear();
            state.history.clear();
        });
        self.document_tabs.retain(|open| open != doc_id);
        self.tab_cameras.remove(doc_id);
        self.project_tabs(cx);
        cx.notify();
    }

    /// Cancel the dirty-close prompt; the document/tab stays open and dirty.
    pub(crate) fn dirty_close_cancel(&mut self, cx: &mut Context<Self>) {
        self.pending_close = None;
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.clear_dialog_transient_selectors();
        });
        cx.notify();
    }

    /// Discard the document or tab without saving.
    pub(crate) fn dirty_close_discard(&mut self, cx: &mut Context<Self>) {
        self.finish_pending_close(cx);
    }

    fn finish_pending_close(&mut self, cx: &mut Context<Self>) {
        let intent = self.pending_close.take().or_else(|| {
            self.active_document
                .as_ref()
                .map(|document| super::CloseIntent::Document {
                    doc_id: document.doc_id.clone(),
                })
        });
        if let Some(intent) = intent {
            self.finish_close_intent(intent, cx);
        }
    }

    fn finish_close_intent(&mut self, intent: super::CloseIntent, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.clear_dialog_transient_selectors();
        });
        match intent {
            super::CloseIntent::Document { doc_id } => self.close_document_by_id(&doc_id, cx),
            super::CloseIntent::Tab { doc_id } => {
                self.close_document_tab_unchecked(&doc_id, cx);
            }
            super::CloseIntent::Window { doc_id } => {
                self.close_document_by_id(&doc_id, cx);
                self.pending_window_close = true;
            }
        }
    }

    /// Save then close. A failure keeps the document open and dirty.
    pub(crate) fn dirty_close_save(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.clear_dialog_transient_selectors();
        });
        let Some(doc_id) = self
            .pending_close
            .as_ref()
            .map(|intent| intent.doc_id().to_owned())
            .or_else(|| {
                self.active_document
                    .as_ref()
                    .map(|document| document.doc_id.clone())
            })
        else {
            self.set_status_error(Some("Save failed: no document to close".into()), cx);
            return;
        };
        if self.pending_close.is_none() {
            self.pending_close = Some(super::CloseIntent::Document {
                doc_id: doc_id.clone(),
            });
        }
        let has_source = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.source_path())
            .is_some();
        if !has_source {
            if !self
                .active_document
                .as_ref()
                .is_some_and(|document| document.doc_id == doc_id)
            {
                if let Err(error) = self.activate_document(&doc_id, cx) {
                    self.pending_close = None;
                    self.set_status_error(Some(format!("Save As failed: {error}")), cx);
                    return;
                }
            }
            self.prompt_save_as(cx);
            return;
        }
        match self.save_document(&doc_id) {
            Ok(()) => self.finish_pending_close(cx),
            Err(error) => {
                self.pending_close = None;
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
        let Some(doc_id) = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
        else {
            self.set_status_error(Some("Save As failed: no active document".into()), cx);
            return;
        };
        let Some((directory, suggested)) = self.save_as_prompt() else {
            self.set_status_error(Some("Save As failed: no active document".into()), cx);
            return;
        };
        let close_intent = self
            .pending_close
            .clone()
            .filter(|intent| intent.doc_id() == doc_id);
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn(async move |this, cx| {
            let selected = match receiver.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) => {
                    this.update(cx, |app, _| app.clear_close_intent(close_intent.as_ref()))
                        .ok();
                    return;
                }
                Ok(Err(error)) => {
                    let status = format!("Save As failed: {error:?}");
                    this.update(cx, |app, cx| {
                        app.clear_close_intent(close_intent.as_ref());
                        app.set_status_error(Some(status), cx);
                    })
                    .ok();
                    return;
                }
                Err(error) => {
                    let status = format!("Save As dialog failed: {error:?}");
                    this.update(cx, |app, cx| {
                        app.clear_close_intent(close_intent.as_ref());
                        app.set_status_error(Some(status), cx);
                    })
                    .ok();
                    return;
                }
            };
            let result = this.update(cx, |app, cx| {
                app.save_document_to(&doc_id, &selected, close_intent.as_ref(), cx)
            });
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

    fn clear_close_intent(&mut self, intent: Option<&super::CloseIntent>) {
        if intent.is_some_and(|intent| self.pending_close.as_ref() == Some(intent)) {
            self.pending_close = None;
        }
    }

    fn save_document_to(
        &mut self,
        doc_id: &str,
        path: &Path,
        close_intent: Option<&super::CloseIntent>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let result = (|| {
            let path = ensure_ptz_path(path)?;
            let file_io = self.ensure_file_io()?;
            file_io
                .approve_path(&path)
                .map_err(|error| format!("Save As failed ({}): {error}", error.code()))?;
            self.sessions
                .save(doc_id, file_io.as_ref(), &path, &SaveStream)
                .map_err(|error| format!("Save As failed ({}): {error}", error.code()))?;
            Ok(())
        })();
        if let Err(error) = result {
            self.clear_close_intent(close_intent);
            return Err(error);
        }
        self.state.update(cx, |state, _| state.status_error = None);
        cx.notify();
        if let Some(intent) = close_intent {
            if self.pending_close.as_ref() == Some(intent) {
                self.pending_close = None;
                self.finish_close_intent(intent.clone(), cx);
            }
        }
        Ok(())
    }

    pub(crate) fn open_document_action(&mut self, _: &OpenDocument, cx: &mut Context<Self>) {
        if self.state.read(cx).loading.is_some() {
            self.set_status_error(Some("Open: a document is already loading".into()), cx);
            return;
        }
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
            this.update(cx, |app, cx| app.open_path_async(path, cx))
                .ok();
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
        if self.state.read(cx).loading.is_some() {
            return Err("Open: a document is already loading".to_owned());
        }
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

    pub(crate) fn install_import(
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
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.clear_dialog_transient_selectors();
        });
        let Some((directory, suggested)) = self.export_prompt(cx) else {
            self.set_status_error(Some("Export failed: no active document".into()), cx);
            return;
        };
        let Some(doc_id) = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
        else {
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
            let prepared = this.update(cx, |app, _| app.prepare_export(&doc_id, &selected));
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

    fn prepare_export(
        &self,
        doc_id: &str,
        path: &Path,
    ) -> Result<(u32, u32, Vec<u8>, ImageFormat), String> {
        let format = ImageFormat::from_export_path(path)
            .map_err(|error| format!("Export failed ({}): {error}", error.code()))?;
        let session = self
            .sessions
            .get(doc_id)
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
        let printers = crate::services::print::PrintService::default().list_printers();
        self.open_print_dialog_with_printers(printers, cx);
    }

    pub(crate) fn open_print_dialog_with_printers(
        &mut self,
        printers: Result<Vec<String>, PrintError>,
        cx: &mut Context<Self>,
    ) {
        let (printers, print_error) = printer_list_result(printers);
        self.state.update(cx, |state, _| {
            state.clear_dialog_transient_selectors();
            state.printers = printers;
            state.print_error = print_error;
            state.print_printer = 0;
            state.active_dialog = Some(DialogId::Print);
        });
        self.refresh_print_papers(cx);
    }

    /// Re-list the platform printers for the Print dialog's refresh control.
    pub(crate) fn refresh_printers(&mut self, cx: &mut Context<Self>) {
        let result = crate::services::print::PrintService::default().list_printers();
        let (printers, print_error) = printer_list_result(result);
        self.state.update(cx, |state, _| {
            state.printers = printers;
            state.print_error = print_error;
            state.print_printer = 0;
        });
        self.refresh_print_papers(cx);
    }

    /// Ask the selected printer's driver which paper sizes it offers, and store
    /// the answer for the dialog's Paper Size row.
    ///
    /// Only Windows can answer. On another target the sizes stay unknown and no
    /// failure is recorded, so the row says it has no list instead of claiming
    /// the chosen paper is missing.
    pub(crate) fn refresh_print_papers(&mut self, cx: &mut Context<Self>) {
        let printer = self
            .state
            .read(cx)
            .printers
            .get(self.state.read(cx).print_printer)
            .cloned();
        #[cfg(windows)]
        let (papers, reason) = match printer {
            Some(printer) => {
                match crate::services::print::PrintService::default().paper_sizes(&printer) {
                    Ok(sizes) => (
                        Some(
                            sizes
                                .into_iter()
                                .map(|size| (size.name, size.width_mm, size.height_mm))
                                .collect(),
                        ),
                        None,
                    ),
                    Err(error) => (None, Some(error.to_string())),
                }
            }
            None => (None, None),
        };
        #[cfg(not(windows))]
        let (papers, reason) = (None, None);
        self.state.update(cx, |state, _| {
            state.print_papers = papers;
            state.print_papers_error = reason;
        });
        cx.notify();
    }

    /// The Print dialog's Print button.
    pub(crate) fn print_now(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.clear_dialog_transient_selectors();
        });
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
        let doc_id = self.active_doc_id()?;
        let (width, height, rgba, format) = self.prepare_export(&doc_id, path)?;
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

    /// Clear the active dialog ID; each caller owns any session cleanup.
    pub(crate) fn close_active_dialog(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.clear_dialog_transient_selectors();
        });
        cx.notify();
    }

    /// Escape on a dialog that has no field-specific key handler. The dialog
    /// chrome routes Escape to this dismiss action. The alert prompts
    /// (dirty close) keep their explicit decision buttons.
    pub(crate) fn dismiss_chrome_dialog(&mut self, cx: &mut Context<Self>) {
        match self.state.read(cx).active_dialog {
            Some(DialogId::Settings) => self.close_settings(cx),
            Some(DialogId::About) => self.close_active_dialog(cx),
            _ => {}
        }
    }

    pub(crate) fn set_status_error(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        if self.state.read(cx).status_error.as_ref() == error.as_ref() {
            return;
        }
        self.state.update(cx, |state, _| state.status_error = error);
        cx.notify();
    }
}

fn printer_list_result(result: Result<Vec<String>, PrintError>) -> (Vec<String>, Option<String>) {
    match result {
        Ok(printers) => (printers, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    }
}
