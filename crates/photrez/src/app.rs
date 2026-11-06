//! # app - the root Entity
//!
//! Owns: construction of the application's services and the state handed down
//! to `views/` (dependency injection). It is the only module allowed to know
//! the concrete wiring of `services/`.
//!
//! Must not own: canvas painting (`display/`), pointer policy (`input_policy/`), or
//! any document logic - document truth lives in `photrez-core` and is reached
//! in-process through the bridge below.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::{
    AppContext, Bounds, Context, Entity, IntoElement, KeyBinding, Pixels, Point, Render,
    RenderImage, Subscription, Window, WindowBounds,
};
use photrez_core::document::content::LayerType;

use crate::display::canvas_bridge::{
    thumbnail_image, BrushCursor, Camera, CanvasImage, CanvasOverlay, CheckerCache, GradientLine,
    TransformFrame,
};
use crate::services;
use crate::services::document::LoadedDocument;
use crate::services::file_io::{FileIo, LocalFileIo};
use crate::services::print::{PrintDispatch, PrintService};
use crate::services::session::SessionRegistry;
use crate::services::settings::{FileSettings, SettingsStore};
use crate::services::window_state::{self, WindowGeometry};
use crate::state::{AppState, ToolId};
use crate::views::workspace;

mod autosave;
pub(crate) mod brush_menu;
mod color_picker;
mod commands;
mod crop;
pub(crate) mod cross_doc;
mod documents;
pub(crate) use canvas_input::canvas_doc_coords_free;
mod canvas_input;
mod drop;
mod edge_scroll;
mod history;
mod layer_marquee;
mod layer_projection;
mod layers;
mod locale;
#[cfg(feature = "input-measurement")]
pub mod measurement;
mod new_document;
pub(crate) mod num_field;
mod overlay;
mod recents;
mod rename;
mod resize;
mod select;
mod selection;
mod slider;
mod startup;
mod test_accessors;
mod text;
mod toasts;
pub(crate) mod tool_settings;
mod tooltips;
mod transform_align;
mod transform_display;
pub(crate) mod transform_geometry;
mod transform_hit;
mod transform_move;
mod viewport;

pub(crate) use commands::PRINT_PAPERS;
pub(crate) use history::{RedoDocument, UndoDocument};
pub(crate) use layers::{
    DuplicateLayerAction, FlattenDocumentAction, MergeLayerDownAction, NewLayerAction,
    SelectAllLayersAction, StampVisibleAction,
};
pub(crate) use selection::{
    CopySelectionAction, CutSelectionAction, DeselectAction, InvertSelectionAction,
    PasteSelectionAction, SelectAllAction,
};
pub(crate) use viewport::{
    ActualSizeAction, FitToScreenAction, ZoomInAction, ZoomOutAction, ZoomToSelectionAction,
};

/// The Save command. Bound to `ctrl-s` in [`App::new_inner`] and handled on
/// the root element, so it works wherever the shell has focus.
#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct SaveDocument;

/// The Print command. Bound to `ctrl-p` with the same window-global handling
/// as Save.
#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct PrintDocument;

/// Open a project or import a raster image.
#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct OpenDocument;

/// Save the active document to a path chosen in the platform dialog.
#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct SaveDocumentAs;

/// Write the active composite to a flat image chosen in the platform dialog.
#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct ExportDocument;

/// Close the active document, prompting when it has unsaved changes.
#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct CloseDocument;

/// Open the New Document dialog. Bound to `ctrl-n`, the shortcut the File
/// menu advertises for the same row.
#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct NewDocumentAction;

/// View > Toggle Side Panels (`view.toggle-side-panels`), bound to
/// Ctrl+Shift+P, the same chord as its menu row.
#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct ToggleSidePanelsAction;

/// A drag-resize produces a bounds change per pixel of movement; the geometry
/// file is rewritten at most this often while that is happening. The settle
/// points (focus loss, quit) always write, so the final position is never lost.
const WRITE_INTERVAL: Duration = Duration::from_millis(500);

struct ActiveDocument {
    doc_id: String,
    canvas: CanvasImage,
    /// `None` means the view still follows the live fit (first open), exactly
    /// the first-open refit; a concrete camera survives resize and is
    /// what the session writes back into the `.ptz` viewport.
    camera: Option<Camera>,
}

/// The `.ptz` `document.json` payload the bridge probe parses below: a real
/// payload of the production reader's shape, kept tiny. The dimensions the
/// reader yields from it are what the window displays.
#[allow(dead_code)] // only reachable from the bridge test until the shell wires it into a view
const PROBE_DOCUMENT_JSON: &str = r#"{
  "id": "bridge-probe",
  "name": "Bridge Probe",
  "width": 1748,
  "height": 1240,
  "layers": []
}"#;

/// Dimensions of the probe document, produced by [`photrez_core`]'s
/// production `.ptz` JSON reader - not echoed from a constant here. This is
/// the in-process wiring proof: the number on screen comes from core.
#[allow(clippy::expect_used)]
// the probe payload is a compile-time constant; a parse failure is a defect, not a runtime condition
#[allow(dead_code)] // only reachable from the bridge test until the shell wires it into a view
pub fn probe_document_size() -> (f64, f64) {
    let doc = photrez_core::document::content::PtzDocumentContent::from_ptz_document_json(
        PROBE_DOCUMENT_JSON,
    )
    .expect("probe document must parse");
    (doc.width, doc.height)
}

/// Registers Inter with GPUI's text system and reports whether it landed.
///
/// This is load-bearing rather than cosmetic: every measurement in the
/// 0.1 captures was taken from a window rendering Inter. If this
/// silently falls back to a system font, glyph advances differ and diffs fail
/// for reasons that have nothing to do with the layout being measured.
pub fn load_fonts(cx: &gpui_kit::App) {
    match cx
        .text_system()
        .add_fonts(vec![Cow::Borrowed(crate::INTER)])
    {
        Err(error) => eprintln!("photrez: FAILED to register Inter: {error:#}"),
        Ok(()) => {
            let names = cx.text_system().all_font_names();
            if names.iter().any(|name| name.trim() == "Inter") {
                eprintln!(
                    "photrez: Inter registered ({} families visible)",
                    names.len()
                );
            } else {
                eprintln!(
                    "photrez: WARNING - Inter not found after registration. \
                     DirectWrite sees {names:?}"
                );
            }
        }
    }
}

/// Root entity — the window's single GPUI root.
///
/// Holds the `AppState` that drives every region of the shell. At this stage
/// `AppState` is seeded entirely from `shot::active()`, so a `--shot` launch
/// renders the exact baseline state; `probe_document_size()` is kept as an
/// in-process wiring proof that `photrez-core`'s JSON reader works.
pub struct App {
    state: Entity<AppState>,
    canvas_focus_handle: gpui_kit::FocusHandle,
    /// Focus target for the layer-rename prompt.
    rename_focus: gpui_kit::FocusHandle,
    /// Focus target for the Resize Canvas prompt.
    resize_focus: gpui_kit::FocusHandle,
    /// Focus target for the New Document dialog's form keys.
    new_document_focus: gpui_kit::FocusHandle,
    /// Focus target for the generic dialog chrome (Escape dismisses a dialog
    /// that has no field of its own to focus).
    pub(crate) dialog_focus: gpui_kit::FocusHandle,
    /// The focus to give back when a dialog closes. Twelve paths set
    /// `active_dialog` to none and none is a choke point, so the transition is
    /// watched in render rather than in each of them.
    shell_focus_before_dialog: Option<gpui_kit::FocusHandle>,
    dialog_was_open: bool,
    /// A New Document dialog opened by a shortcut; the next render focuses it
    /// because a window-global action has no `Window` to focus with.
    pending_new_document_focus: bool,
    /// Focus target for the colour picker's hex field and keys.
    pub(crate) picker_focus: gpui_kit::FocusHandle,
    /// The display snapshot of the demo document, built once at startup
    /// through `photrez-core` (`services::document`). The seam only converts
    /// and paints it; document truth stays in core. `None` is defined
    /// behaviour: the canvas area falls back to its flat colour.
    demo_canvas: Option<CanvasImage>,
    active_document: Option<ActiveDocument>,
    /// Open-document tab order and per-document camera (the viewport is
    /// session state, restored when the tab becomes active again).
    document_tabs: Vec<String>,
    tab_cameras: HashMap<String, Option<Camera>>,
    /// Layer thumbnails aligned with the projected layer rows.
    layer_thumbs: Vec<Option<Arc<RenderImage>>>,
    /// Recently opened project paths, newest first.
    recent_files: Vec<PathBuf>,
    /// Paths from the current OS file drag, between Entered and Submit.
    dropped_paths: Vec<PathBuf>,
    /// Window position of the current OS file drag, so a drop onto the canvas
    /// can center the imported layers on the cursor.
    dropped_position: Option<Point<Pixels>>,
    /// The live crop frame move/resize drag.
    crop_drag: Option<crop::CropDrag>,
    /// The live Move-tool empty-canvas layer marquee.
    move_marquee: Option<layer_marquee::MoveMarquee>,
    /// The live Move-tool transform drag.
    transform_drag: Option<transform_move::TransformDrag>,
    /// The live Selection-tool rotation of the committed selection.
    selection_rotation_drag: Option<canvas_input::SelectionRotationDrag>,
    /// Selection transform mode (Ctrl+T): paints the eight
    /// resize handles and the rotate pin, and arms the pin drag.
    selection_edit_mode: bool,
    group_transform_drag: Option<transform_move::GroupTransformDrag>,
    pending_transform_drag_update: Option<transform_move::TransformDragUpdate>,
    transform_drag_frame_pending: bool,
    transform_drag_frame_generation: u64,
    /// Active smart-guide lines while a transform snaps.
    snap_guides: Vec<(bool, f64)>,
    /// Last hovered canvas position (window coords) for crop cursors.
    hover_pos: Option<Point<Pixels>>,
    last_cursor: crate::input_policy::CursorPolicy,
    /// Edge auto-scroll state: holding the pointer near a canvas edge pans
    /// the view while a drag is in flight.
    edge_pointer: Option<Point<Pixels>>,
    edge_scroll_active: bool,
    edge_scroll_generation: u64,
    edge_scroll_task: Option<gpui_kit::Task<()>>,
    /// The window geometry seen on the last frame. `render` is the one place
    /// handed a live window every frame, so it is where the geometry is
    /// sampled; the observers below only decide when it is written.
    live: Option<WindowGeometry>,
    /// The record already on disk, and when it was written: unchanged geometry
    /// is never rewritten and a drag-resize is throttled by [`WRITE_INTERVAL`].
    saved: Option<WindowGeometry>,
    saved_at: Option<Instant>,
    /// Where the record goes. `None` (no config directory) disables persistence
    /// without disabling the window.
    settings: Option<FileSettings>,
    /// The open document lives here, not in the view state: the session owns
    /// the loaded core registry and its save/dirty lifecycle.
    sessions: SessionRegistry,
    /// The file boundary that opened the active document. Save writes through
    /// the same boundary, so an unapproved path cannot become a save target.
    file_io: Option<Arc<dyn FileIo>>,
    /// Display textures retired by a document swap. `render` is the next place
    /// a window exists, so it releases them there.
    retired_textures: Vec<Arc<RenderImage>>,
    /// The print boundary. Production uses the native print service; tests
    /// inject a recorder so the caller is provable without a printer.
    print_backend: Arc<dyn PrintDispatch>,
    /// A Save As started from the dirty-close prompt closes the document after
    /// the chosen path is written.
    close_after_save: bool,
    /// The dirty prompt was opened by a window-close request, so a successful
    /// save/discard also closes the window.
    close_after_dirty: bool,
    /// Set by `close_active_document`; the next render removes the window.
    pending_window_close: bool,
    /// The tab whose dirty-close prompt is open, if the prompt came from a tab
    /// close rather than a window close.
    pending_tab_close: Option<String>,
    /// In-flight canvas gesture state (brush chain; later tools join here).
    input: crate::input_policy::InputState,
    /// App-local pixel clipboard (the in-memory selection buffer).
    clipboard: Option<selection::ClipboardImage>,
    /// Brush tip keyed by size/hardness/colour; see `BrushTipCache`.
    brush_tips: crate::input_policy::BrushTipCache,
    /// Per-stroke pointer smoothing (`PaintSmoother`).
    paint_smoother: canvas_input::PaintSmoother,
    /// Shift-held brush drag: `Some(true)` locks to the horizontal axis,
    /// `Some(false)` to the vertical one, `None` before the 5px threshold.
    brush_axis_lock: Option<bool>,
    /// The stroke start in document and screen space, for the Shift axis lock.
    brush_stroke_start: Option<(f64, f64)>,
    brush_stroke_start_screen: Option<(f32, f32)>,
    /// Alt is held over a brush/eraser drag: sample the foreground colour
    /// instead of painting.
    alt_sampling: bool,
    /// Alt+Right brush adjustment drag: the size/hardness at pointer-down.
    brush_adjust: Option<canvas_input::BrushAdjust>,
    /// The canvas hitbox that currently owns the captured pointer. Shared with
    /// the custom pointer surface, whose hitbox id changes every frame.
    canvas_capture: Rc<Cell<Option<gpui_kit::HitboxId>>>,
    /// The laid-out canvas bounds, published by the display seam and read by
    /// the camera actions and pointer mapping.
    canvas_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Screen-space checkerboard reused across frames; rebuilt on resize only.
    checker_cache: Rc<RefCell<CheckerCache>>,
    /// The laid-out Navigator thumbnail bounds, published by its paint pass.
    navigator_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Space is held over the canvas: the next drag pans instead of using the
    /// active tool (`usePanNavigation.isSpacePressed`).
    space_pressed: bool,
    /// Alt as the pointer last reported it, so a Move drag can show the clone
    /// cursor without a modifier event of its own.
    pointer_alt: bool,
    /// The live space/middle-button pan drag.
    pan: Option<viewport::PanDrag>,
    /// An option-bar slider drag is in flight.
    slider_dragging: bool,
    #[cfg(feature = "input-measurement")]
    measurement: measurement::InputMeasurement,
    /// A non-brush pointer drag waiting for pointer-up.
    pending_gesture: Option<crate::input_policy::PendingGesture>,
    /// Held for the window's lifetime: dropping a subscription cancels it.
    _window_watch: Vec<Subscription>,
    /// Full-document composites performed for this window.
    #[cfg(test)]
    full_composites: Cell<usize>,
    /// Accepted brush previews for this window (multi-layer proof).
    #[cfg(test)]
    preview_pushes: Cell<usize>,
    /// The 60 s auto-save loop; dropping the task cancels it.
    autosave_task: Option<gpui_kit::Task<()>>,
    /// The app-cache directory holding the autosave store, resolved once so
    /// the timer, the recovery prompt and the quit flush all use one answer.
    autosave_cache_root: Option<PathBuf>,
}

/// Route an action to this window's `App` entity from anywhere in the window.
///
/// GPUI's element-level action listeners only reach the focused node's
/// ancestry, so a shortcut with no focused control needs an app-level handler.
/// The weak entity keeps a dismissed window's handler inert.
fn register_global_action<A: gpui_kit::Action>(
    cx: &mut Context<App>,
    handler: fn(&mut App, &A, &mut Context<App>),
) {
    let weak_app = cx.weak_entity();
    gpui_kit::App::on_action(cx, move |action: &A, app_cx: &mut gpui_kit::App| {
        if let Some(app) = weak_app.upgrade() {
            app.update(app_cx, |app, cx| handler(app, action, cx));
        }
    });
}

impl App {
    fn remember_bounds(&mut self, window: &Window) {
        self.live = Some(restorable_geometry(window));
    }

    /// Writes the last seen geometry when it differs from the record already on
    /// disk. `settled` marks the end of a gesture (focus loss, quit), where the
    /// throttle must not skip the final position.
    fn persist_bounds(&mut self, settled: bool) {
        let Some(geometry) = self.live else {
            return;
        };
        if self.saved == Some(geometry) {
            return;
        }
        if !settled
            && self
                .saved_at
                .is_some_and(|at| at.elapsed() < WRITE_INTERVAL)
        {
            return;
        }
        if let Some(settings) = &self.settings {
            settings.write(window_state::KEY, &window_state::encode(geometry));
        }
        self.saved = Some(geometry);
        self.saved_at = Some(Instant::now());
    }

    fn open_startup_path(&mut self, path: &Path, cx: &mut Context<Self>) -> Result<(), String> {
        if !is_existing_ptz_file(path) {
            return Err("startup path must be an existing regular .ptz file".to_owned());
        }
        let file_io = self.ensure_file_io()?;
        self.open_startup_document(path, file_io.as_ref(), cx)
    }

    fn open_startup_document(
        &mut self,
        path: &Path,
        file_io: &dyn FileIo,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (loaded, canvas) = Self::prepare_startup_document(path, file_io)?;
        self.install_document(loaded, canvas, Some(path.to_path_buf()), cx)
    }

    /// Put a prepared document on screen, replacing the current one only when
    /// it has no unsaved changes. A replacement's textures are retired and
    /// released by the next frame.
    fn install_document(
        &mut self,
        loaded: LoadedDocument,
        canvas: CanvasImage,
        source_path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc_id = loaded.document_id().to_owned();
        let display_name = loaded.document_snapshot().name.clone();
        let display_name_for_tab = display_name.clone();
        let active_id = self.active_document.as_ref().map(|doc| doc.doc_id.clone());
        let recent_path = source_path.clone();

        if active_id.as_deref() == Some(doc_id.as_str()) {
            self.sessions
                .replace(loaded, display_name, source_path)
                .map_err(|error| format!("session replacement failed: {error:?}"))?;
        } else {
            self.sessions
                .open(loaded, display_name, source_path)
                .map_err(|error| format!("session registration failed: {error:?}"))?;
            if let Some(previous) = active_id.as_deref() {
                let camera = self
                    .active_document
                    .as_ref()
                    .and_then(|document| document.camera);
                self.tab_cameras.insert(previous.to_string(), camera);
            }
            if !self.document_tabs.iter().any(|tab| tab == &doc_id) {
                self.document_tabs.push(doc_id.clone());
            }
        }

        let camera = if active_id.as_deref() == Some(doc_id.as_str()) {
            self.active_document
                .as_ref()
                .and_then(|document| document.camera)
        } else {
            self.tab_cameras.remove(&doc_id).flatten()
        };
        let retired = self.active_document.replace(ActiveDocument {
            doc_id: doc_id.clone(),
            canvas,
            camera,
        });
        if let Some(retired) = retired {
            self.retired_textures.extend(retired.canvas.into_textures());
        }
        self.state.update(cx, |state, _| {
            state.status_error = None;
            state.document_name = display_name_for_tab.clone();
            state.document_id = Some(doc_id.clone());
            state.has_document = true;
        });
        if let Some(path) = recent_path.as_deref() {
            self.record_recent_file(path, cx);
        }
        self.project_layers(cx);
        self.project_history(cx);
        self.project_tabs(cx);
        cx.notify();
        Ok(())
    }

    /// The file boundary is created on first use, so an App launched without a
    /// project can still import and save.
    fn ensure_file_io(&mut self) -> Result<Arc<dyn FileIo>, String> {
        if let Some(file_io) = &self.file_io {
            return Ok(Arc::clone(file_io));
        }
        let settings = self
            .settings
            .as_ref()
            .ok_or_else(|| "settings path is unavailable for path approval".to_owned())?;
        let cache_root = services::settings::user_cache_root()
            .ok_or_else(|| "user cache directory is unavailable".to_owned())?;
        let file_io: Arc<dyn FileIo> = Arc::new(
            LocalFileIo::new(cache_root, settings.trusted_paths_path())
                .map_err(|error| format!("cannot open path approvals: {error:?}"))?,
        );
        self.file_io = Some(Arc::clone(&file_io));
        Ok(file_io)
    }

    fn prepare_startup_document(
        path: &Path,
        file_io: &dyn FileIo,
    ) -> Result<(LoadedDocument, CanvasImage), String> {
        if !is_existing_ptz_file(path) {
            return Err("startup path must be an existing regular .ptz file".to_owned());
        }
        file_io
            .approve_path(path)
            .map_err(|error| format!("path approval failed: {error:?}"))?;
        let mut loaded = services::document::load_document_candidate(file_io, path)
            .map_err(|error| format!("document load failed: {error:?}"))?;
        let rgba = std::mem::take(&mut loaded.display_rgba);
        let canvas = CanvasImage::from_rgba(loaded.display_width, loaded.display_height, rgba)
            .map_err(|error| format!("document presentation failed: {error:?}"))?;
        Ok((loaded, canvas))
    }
}

fn is_existing_ptz_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ptz"))
        && std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

/// Save As writes projects only; flat images go through Export.
fn ensure_ptz_path(path: &Path) -> Result<PathBuf, String> {
    match path.extension().and_then(OsStr::to_str) {
        Some(extension) if extension.eq_ignore_ascii_case("ptz") => Ok(path.to_path_buf()),
        Some(_) => Err("Save As failed: choose a .ptz path; use Export for flat images".to_owned()),
        None => Ok(path.with_extension("ptz")),
    }
}

/// The window's restorable geometry: a normal window's bounds, or a maximized
/// window's restore bounds plus its state. `WindowBounds::Maximized` carries
/// the restore rect by contract, so a maximized window is never remembered as
/// a normal window the size of the screen.
///
/// The shell exposes no fullscreen command; a fullscreen window's restore rect
/// is the honest record of where it was before.
fn restorable_geometry(window: &Window) -> WindowGeometry {
    match window.window_bounds() {
        WindowBounds::Windowed(bounds) => WindowGeometry::of(bounds, false),
        WindowBounds::Maximized(bounds) => WindowGeometry::of(bounds, true),
        WindowBounds::Fullscreen(bounds) => WindowGeometry::of(bounds, false),
    }
}

impl Render for App {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.pending_window_close) {
            window.remove_window();
        }
        // A dialog that has just closed gives focus back to whatever held it
        // before. The saved handle is refreshed only while no dialog is up, so
        // on the frame a dialog disappears it still names the pre-dialog element.
        let dialog_open = self.state.read(cx).active_dialog.is_some();
        if self.dialog_was_open && !dialog_open {
            if let Some(handle) = self.shell_focus_before_dialog.take() {
                handle.focus(window, cx);
            }
        }
        self.dialog_was_open = dialog_open;
        if !dialog_open {
            self.shell_focus_before_dialog = window.focused(cx);
        }
        #[cfg(feature = "input-measurement")]
        let render_identity = self.active_document.as_ref().and_then(|document| {
            let version = self
                .sessions
                .get(&document.doc_id)?
                .loaded_document()
                .registry
                .get_history_version(&document.doc_id)?;
            Some((document.doc_id.clone(), version))
        });
        #[cfg(feature = "input-measurement")]
        if let Some(marker) = self.measurement.rendered(
            Instant::now(),
            render_identity
                .as_ref()
                .map(|(doc_id, version)| (doc_id.as_str(), *version)),
        ) {
            cx.emit(marker);
        }
        // The geometry is sampled here because this is the only place the root
        // is handed a live window every frame.
        self.remember_bounds(window);
        if std::mem::take(&mut self.pending_new_document_focus) {
            window.focus(&self.new_document_focus, cx);
        }
        for texture in self.retired_textures.drain(..) {
            let _ = window.drop_image(texture);
        }
        // Delegate to the workspace renderer.
        let cursor = self.canvas_cursor_policy(cx);
        let camera = self.active_camera();
        let overlay = self.canvas_overlay(cx);
        let has_pixel_selection = self.pixel_selection_present();
        let has_copyable_selection = has_pixel_selection && self.active_layer_has_bitmap(cx);
        let has_selection_clipboard = self.selection_clipboard_present();
        let can_undo = self.can_undo_document(cx);
        let can_redo = self.can_redo_document(cx);
        let committed_selection = self.committed_selection();
        let transform_readout = self.transform_readout(cx);
        // The demo canvas paints only while a document is on screen; after
        // the last tab closes the workspace is empty, so no canvas may leak
        // into the navigator, status bar or canvas slot.
        let has_document = self.state.read(cx).has_document;
        let canvas = self
            .active_document
            .as_ref()
            .map(|document| &document.canvas)
            .or(if has_document {
                self.demo_canvas.as_ref()
            } else {
                None
            });
        workspace::render(
            &self.state,
            window,
            canvas,
            camera,
            overlay,
            Rc::clone(&self.canvas_bounds),
            Rc::clone(&self.checker_cache),
            Rc::clone(&self.navigator_bounds),
            &self.layer_thumbs,
            &self.canvas_focus_handle,
            &self.rename_focus,
            &self.resize_focus,
            &self.new_document_focus,
            &self.dialog_focus,
            &self.picker_focus,
            Rc::clone(&self.canvas_capture),
            cursor,
            has_pixel_selection,
            has_copyable_selection,
            has_selection_clipboard,
            can_undo,
            can_redo,
            committed_selection,
            transform_readout,
            cx,
        )
    }
}

#[cfg(test)]
mod tests;
