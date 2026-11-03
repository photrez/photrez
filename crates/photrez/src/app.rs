//! # app - the root Entity
//!
//! Owns: construction of the application's services and the state handed down
//! to `views/` (dependency injection). It is the only module allowed to know
//! the concrete wiring of `services/`.
//!
//! Must not own: canvas painting (`render/`), pointer policy (`input/`), or
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
use photrez_core::ptz::content::LayerType;

use crate::render::canvas_bridge::{
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
pub(crate) use input::canvas_doc_coords_free;
mod drop;
mod edge_scroll;
mod history;
mod input;
mod layer_marquee;
mod layers;
mod locale;
#[cfg(feature = "input-measurement")]
pub mod measurement;
mod new_document;
mod num_field;
mod recents;
mod rename;
mod resize;
mod select;
mod selection;
mod slider;
mod text;
mod toasts;
pub(crate) mod tool_settings;
mod tooltips;
pub(crate) mod transform_geometry;
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
    let doc =
        photrez_core::ptz::content::PtzDocumentContent::from_ptz_document_json(PROBE_DOCUMENT_JSON)
            .expect("probe document must parse");
    (doc.width, doc.height)
}

/// Registers Inter with GPUI's text system and reports whether it landed.
///
/// This is load-bearing rather than cosmetic: every measurement in the
/// reference captures was taken from a window rendering Inter. If this
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
    selection_rotation_drag: Option<input::SelectionRotationDrag>,
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
    last_cursor: crate::input::CursorPolicy,
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
    input: crate::input::InputState,
    /// App-local pixel clipboard (the in-memory selection buffer).
    clipboard: Option<selection::ClipboardImage>,
    /// Brush tip keyed by size/hardness/colour; see `BrushTipCache`.
    brush_tips: crate::input::BrushTipCache,
    /// Per-stroke pointer smoothing (`PaintSmoother`).
    paint_smoother: input::PaintSmoother,
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
    brush_adjust: Option<input::BrushAdjust>,
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
    pending_gesture: Option<crate::input::PendingGesture>,
    /// Held for the window's lifetime: dropping a subscription cancels it.
    _window_watch: Vec<Subscription>,
    /// Full-document composites performed for this window (G9 proof).
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
    /// The selection and crop chrome painted over the page this frame.
    ///
    /// The committed selection is document state; the drag preview is UI
    /// state. A live drag wins so the marquee follows the pointer before the
    /// core command commits.
    fn canvas_overlay(&self, cx: &Context<Self>) -> CanvasOverlay {
        use photrez_core::ptz::content::SelectionShape as CoreSelectionShape;
        let mut overlay = CanvasOverlay::default();
        let mut selection_inverted = false;
        if let Some(doc_id) = self.active_document.as_ref().map(|doc| doc.doc_id.clone()) {
            if let Some(selection) = self
                .sessions
                .get(&doc_id)
                .and_then(|session| session.loaded_document().content_snapshot())
                .and_then(|content| content.selection.clone())
            {
                overlay.selection = Some(crate::render::canvas_bridge::SelectionOverlay {
                    x: selection.x,
                    y: selection.y,
                    w: selection.width,
                    h: selection.height,
                    ellipse: matches!(selection.shape, Some(CoreSelectionShape::Ellipse)),
                    angle: selection.angle,
                    edit_mode: false,
                });
                selection_inverted = selection.inverted.unwrap_or(false);
            }
        }
        let state = self.state.read(cx);
        if let Some(drag) = state.selection_drag {
            overlay.selection = Some(crate::render::canvas_bridge::SelectionOverlay {
                x: drag[0],
                y: drag[1],
                w: drag[2],
                h: drag[3],
                ellipse: state.selection_shape == crate::state::SelectionShape::Ellipse,
                angle: 0.0,
                edit_mode: false,
            });
        }
        if let Some(rotation) = self.selection_rotation_drag.as_ref() {
            if let Some(selection) = overlay.selection.as_mut() {
                selection.angle = rotation.preview_angle;
            }
        }
        if let Some(selection) = overlay.selection.as_mut() {
            selection.edit_mode = self.selection_edit_mode
                && state.active_tool == crate::state::ToolId::Selection
                && state.selection_drag.is_none()
                && !selection_inverted;
        }
        // The rotate sprite follows the pointer angle about
        // the selection centre while a pin drag rotates, and on the pin hover
        // in edit mode.
        if state.active_tool == crate::state::ToolId::Selection {
            let mut sprite_point = None;
            if self.selection_rotation_drag.is_some() {
                sprite_point = self.hover_pos;
            } else if overlay
                .selection
                .as_ref()
                .is_some_and(|selection| selection.edit_mode)
            {
                if let (Some(position), Some(doc_size)) = (
                    self.hover_pos,
                    self.active_document
                        .as_ref()
                        .map(|document| document.canvas.dimensions()),
                ) {
                    if let Some(selection) = overlay.selection.as_ref() {
                        let zoom = self
                            .effective_camera()
                            .map(|camera| camera.zoom as f64)
                            .unwrap_or(1.0);
                        let (top_mid, normal) = crate::app::transform_geometry::selection_pin(
                            selection.x,
                            selection.y,
                            selection.w,
                            selection.h,
                            selection.angle,
                        );
                        let pin = (
                            top_mid.x + normal.x * 24.0 / zoom,
                            top_mid.y + normal.y * 24.0 / zoom,
                        );
                        if let Some((x, y)) = crate::app::input::canvas_doc_coords(
                            position,
                            &self.canvas_bounds,
                            doc_size,
                            self.active_camera(),
                        ) {
                            let distance =
                                ((x - pin.0).powi(2) + (y - pin.1).powi(2)).sqrt() * zoom;
                            if distance < 12.0 {
                                sprite_point = Some(position);
                            }
                        }
                    }
                }
            }
            if let (Some(position), Some(selection)) = (sprite_point, overlay.selection.as_ref()) {
                if let Some(doc_size) = self
                    .active_document
                    .as_ref()
                    .map(|document| document.canvas.dimensions())
                {
                    if let Some((x, y)) = crate::app::input::canvas_doc_coords(
                        position,
                        &self.canvas_bounds,
                        doc_size,
                        self.active_camera(),
                    ) {
                        let centre = crate::app::transform_geometry::Point {
                            x: selection.x + selection.w / 2.0,
                            y: selection.y + selection.h / 2.0,
                        };
                        let angle = crate::app::transform_geometry::angle_about(
                            centre,
                            crate::app::transform_geometry::Point { x, y },
                        ) + 45.0;
                        overlay.rotate_cursor =
                            Some(crate::render::canvas_bridge::RotateCursorOverlay {
                                position: [f32::from(position.x), f32::from(position.y)],
                                angle: angle as f32,
                            });
                    }
                }
            }
        }
        if let Some(crop) = state.crop_rect {
            overlay.crop = Some((crop[0], crop[1], crop[2], crop[3]));
        }
        overlay.guides = self.snap_guides.clone();
        if let Some(pending) = self
            .pending_gesture
            .as_ref()
            .filter(|gesture| gesture.tool == crate::state::ToolId::Gradient)
        {
            overlay.gradient_line = Some(GradientLine {
                start: [pending.start.0, pending.start.1],
                end: [pending.last.0, pending.last.1],
                radial: state.gradient_radial,
            });
        }
        overlay.move_marquee = self
            .move_marquee
            .as_ref()
            .and_then(|marquee| marquee.rect());
        // A cleared selection keeps row 0 as the native primary; it must not
        // paint a transform frame for a layer the user no longer selected.
        // The canvas viewport paints the frame for the Shape tool too, so
        // a shape drawn and auto-selected keeps its handles until the tool
        // changes; the frame bounds come from `layer_frame_size`, which reads
        // the shape rectangle rather than the document-sized bitmap.
        if matches!(
            state.active_tool,
            crate::state::ToolId::Move | crate::state::ToolId::Shape
        ) && state.selected_layers.contains(&state.selected_layer)
        {
            overlay.transform_controls = state.show_transform_controls;
            if state.selected_layers.len() > 1 {
                if let Some(frame) = self.group_overlay_frame(cx) {
                    overlay.transform = Some(frame);
                }
            } else if let Ok((_, layer)) = self.selected_layer_for_cursor(cx) {
                let transform = self
                    .transform_drag
                    .as_ref()
                    .filter(|drag| drag.layer_id == layer.id)
                    .map(|drag| drag.preview.clone())
                    .unwrap_or_else(|| layer.transform.clone());
                let (frame_w, frame_h) = crate::app::transform_geometry::layer_frame_size(&layer);
                let corners =
                    crate::app::transform_geometry::layer_corners(&transform, frame_w, frame_h);
                let mid = |a: crate::app::transform_geometry::Point,
                           b: crate::app::transform_geometry::Point| {
                    crate::app::transform_geometry::Point {
                        x: (a.x + b.x) / 2.0,
                        y: (a.y + b.y) / 2.0,
                    }
                };
                let handles = [
                    corners[0],
                    corners[1],
                    corners[2],
                    corners[3],
                    mid(corners[0], corners[1]),
                    mid(corners[1], corners[2]),
                    mid(corners[2], corners[3]),
                    mid(corners[3], corners[0]),
                ];
                overlay.transform = Some(TransformFrame {
                    corners: corners.map(|point| [point.x, point.y]),
                    handles: handles.map(|point| [point.x, point.y]),
                });
            }
        }
        // A non-active layer under the Move pointer gets a
        // 1 px accent outline. No drag, marquee or transform session may own the
        // pointer while it paints.
        if state.active_tool == crate::state::ToolId::Move
            && self.pending_gesture.is_none()
            && self.transform_drag.is_none()
            && self.move_marquee.is_none()
        {
            if let (Some(position), Some(_)) = (self.hover_pos, self.canvas_bounds.get()) {
                let doc_size = self
                    .active_document
                    .as_ref()
                    .map(|document| document.canvas.dimensions())
                    .or_else(|| self.demo_canvas.as_ref().map(CanvasImage::dimensions));
                if let Some(doc_size) = doc_size {
                    if let Some((x, y)) = crate::app::input::canvas_doc_coords(
                        position,
                        &self.canvas_bounds,
                        doc_size,
                        self.active_camera(),
                    ) {
                        if let Some(core_id) = self.layer_hit_at(x, y) {
                            let active_core = state
                                .layers
                                .iter()
                                .find(|row| row.id == state.selected_layer)
                                .and_then(|row| row.core_id.as_deref());
                            if active_core != Some(core_id.as_str()) {
                                if let Some(doc_id) = self
                                    .active_document
                                    .as_ref()
                                    .map(|document| document.doc_id.clone())
                                {
                                    overlay.hover_highlight = self
                                        .sessions
                                        .get(&doc_id)
                                        .and_then(|session| {
                                            session.loaded_document().content_snapshot()
                                        })
                                        .and_then(|content| {
                                            content
                                                .layers
                                                .iter()
                                                .find(|layer| layer.id == core_id)
                                                .map(|layer| {
                                                    (
                                                        layer.transform.x,
                                                        layer.transform.y,
                                                        layer.width * layer.transform.scale_x.abs(),
                                                        layer.height
                                                            * layer.transform.scale_y.abs(),
                                                    )
                                                })
                                        });
                                }
                            }
                        }
                    }
                }
            }
        }
        // Brush and eraser hide the system cursor and
        // paint the 20% alpha ring at the pointer. Alt/space take over the
        // pointer for sampling and panning, and the ring is hidden then.
        if matches!(
            state.active_tool,
            crate::state::ToolId::Brush | crate::state::ToolId::Eraser
        ) && self.pan.is_none()
            && !self.space_pressed
        {
            if let (Some(position), Some(bounds)) = (self.hover_pos, self.canvas_bounds.get()) {
                let px_x: f32 = position.x.into();
                let px_y: f32 = position.y.into();
                let origin_x = f32::from(bounds.origin.x);
                let origin_y = f32::from(bounds.origin.y);
                let width: f32 = bounds.size.width.into();
                let height: f32 = bounds.size.height.into();
                let inside = px_x >= origin_x
                    && px_y >= origin_y
                    && px_x < origin_x + width
                    && px_y < origin_y + height;
                if inside {
                    let doc_size = self
                        .active_document
                        .as_ref()
                        .map(|document| document.canvas.dimensions())
                        .or_else(|| self.demo_canvas.as_ref().map(CanvasImage::dimensions));
                    if let Some(doc_size) = doc_size {
                        if let Some(center) = crate::app::input::canvas_doc_position(
                            position,
                            &self.canvas_bounds,
                            doc_size,
                            self.active_camera(),
                        ) {
                            let is_eraser = state.active_tool == crate::state::ToolId::Eraser;
                            let (size, hardness) = if is_eraser {
                                (state.eraser_size, state.eraser_hardness)
                            } else {
                                (state.brush_size, state.brush_hardness)
                            };
                            let scale =
                                photrez_core::brush::tip::get_brush_cursor_radius_scale(hardness);
                            let base = (size / 2.0) * scale;
                            let (scale_x, scale_y, rotation) = self
                                .selected_layer_for_cursor(cx)
                                .map(|(_, layer)| {
                                    (
                                        layer.transform.scale_x,
                                        layer.transform.scale_y,
                                        layer.transform.rotation,
                                    )
                                })
                                .unwrap_or((1.0, 1.0, 0.0));
                            overlay.brush_cursor = Some(BrushCursor {
                                center: [center.0, center.1],
                                radii: [base * scale_x.abs(), base * scale_y.abs()],
                                rotation,
                                crosshair: !is_eraser,
                            });
                        }
                    }
                }
            }
        }
        overlay
    }
    #[cfg(test)]
    pub(crate) fn hover_highlight_for_test(
        &self,
        cx: &Context<Self>,
    ) -> Option<(f64, f64, f64, f64)> {
        self.canvas_overlay(cx).hover_highlight
    }
    #[cfg(test)]
    pub(crate) fn rotate_cursor_for_test(
        &self,
        cx: &Context<Self>,
    ) -> Option<crate::render::canvas_bridge::RotateCursorOverlay> {
        self.canvas_overlay(cx).rotate_cursor
    }
    #[cfg(test)]
    pub(crate) fn brush_cursor_for_test(&self, cx: &Context<Self>) -> Option<BrushCursor> {
        self.canvas_overlay(cx).brush_cursor
    }
    #[cfg(test)]
    pub(crate) fn gradient_line_for_test(&self, cx: &Context<Self>) -> Option<GradientLine> {
        self.canvas_overlay(cx).gradient_line
    }
    #[cfg(test)]
    pub(crate) fn transform_frame_for_test(&self, cx: &Context<Self>) -> Option<TransformFrame> {
        self.canvas_overlay(cx).transform
    }

    /// The cache a launch may recover from: only a settings-backed one owns the
    /// real user cache directory.
    fn startup_cache_root(settings: &Option<FileSettings>) -> Option<PathBuf> {
        settings
            .as_ref()
            .and_then(|_| services::settings::user_cache_root())
    }

    /// Creates a new `App`, seeding `AppState` from the active shot id,
    /// building the demo document's display snapshot through core, and
    /// subscribing to the window geometry it will persist.
    pub fn new(
        window: &mut Window,
        settings: Option<FileSettings>,
        open_path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let cache_root = Self::startup_cache_root(&settings);
        Self::new_inner(window, settings, open_path, None, cache_root, cx)
    }

    #[cfg(test)]
    pub(crate) fn new_with_file_io(
        window: &mut Window,
        settings: Option<FileSettings>,
        open_path: Option<PathBuf>,
        file_io: Arc<dyn FileIo>,
        cx: &mut Context<Self>,
    ) -> Self {
        let cache_root = Self::startup_cache_root(&settings);
        Self::new_inner(window, settings, open_path, Some(file_io), cache_root, cx)
    }

    /// A mount whose startup recovery reads a cache the test owns. Without this
    /// seam the explicit-open-then-recovery order only runs when the process has
    /// a real user cache directory, so no mounted test could drive it.
    #[cfg(test)]
    pub(crate) fn new_with_cache_root(
        window: &mut Window,
        settings: Option<FileSettings>,
        open_path: Option<PathBuf>,
        file_io: Arc<dyn FileIo>,
        cache_root: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_inner(
            window,
            settings,
            open_path,
            Some(file_io),
            Some(cache_root),
            cx,
        )
    }

    fn new_inner(
        window: &mut Window,
        settings: Option<FileSettings>,
        open_path: Option<PathBuf>,
        file_io: Option<Arc<dyn FileIo>>,
        cache_root: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let shot_id = crate::shot::active();
        let mut state = if shot_id.is_none() {
            AppState::empty()
        } else {
            AppState::seeded(shot_id)
        };
        // A `--shot` launch seeds the demo document without a session, so the
        // tab strip's projection from real sessions would be empty while the
        // document is on screen. The captured shell has one open document, so
        // seed its display-only tab row to match.
        if shot_id.is_some() {
            state.seed_shot_tab();
        }
        // The demo document display snapshot belongs to the demo state: an
        // empty-* launch renders no document, so its chrome (navigator
        // thumbnail, canvas area, status-bar stats) must not show one.
        let demo_canvas = if state.has_document {
            let (width, height, rgba) = services::document::blank_white_document();
            match CanvasImage::from_rgba(width, height, rgba) {
                Ok(canvas) => Some(canvas),
                Err(error) => {
                    eprintln!(
                        "photrez: the demo document was refused by the display seam: {error:?}"
                    );
                    None
                }
            }
        } else {
            None
        };

        cx.bind_keys([
            KeyBinding::new("ctrl-n", NewDocumentAction, None),
            KeyBinding::new("ctrl-shift-p", ToggleSidePanelsAction, None),
            KeyBinding::new("ctrl-s", SaveDocument, None),
            KeyBinding::new("ctrl-shift-s", SaveDocumentAs, None),
            KeyBinding::new("ctrl-o", OpenDocument, None),
            KeyBinding::new("ctrl-alt-e", ExportDocument, None),
            KeyBinding::new("ctrl-p", PrintDocument, None),
            KeyBinding::new("ctrl-w", CloseDocument, None),
        ]);
        cx.bind_keys([
            KeyBinding::new("v", input::SetCanvasTool(ToolId::Move), Some("Canvas")),
            KeyBinding::new("m", input::SetCanvasTool(ToolId::Selection), Some("Canvas")),
            KeyBinding::new("c", input::SetCanvasTool(ToolId::Crop), Some("Canvas")),
            KeyBinding::new(
                "i",
                input::SetCanvasTool(ToolId::Eyedropper),
                Some("Canvas"),
            ),
            KeyBinding::new("b", input::SetCanvasTool(ToolId::Brush), Some("Canvas")),
            KeyBinding::new("e", input::SetCanvasTool(ToolId::Eraser), Some("Canvas")),
            KeyBinding::new(
                "g",
                input::SetCanvasTool(ToolId::PaintBucket),
                Some("Canvas"),
            ),
            KeyBinding::new(
                "shift-g",
                input::SetCanvasTool(ToolId::Gradient),
                Some("Canvas"),
            ),
            KeyBinding::new("u", input::SetCanvasTool(ToolId::Shape), Some("Canvas")),
            KeyBinding::new("t", input::SetCanvasTool(ToolId::Text), Some("Canvas")),
            KeyBinding::new("shift-m", input::ToggleSelectionShape, Some("Canvas")),
            KeyBinding::new("escape", input::CancelCanvasGesture, Some("Canvas")),
            KeyBinding::new("ctrl-z", history::UndoDocument, Some("Canvas")),
            KeyBinding::new("ctrl-shift-z", history::RedoDocument, Some("Canvas")),
            KeyBinding::new("ctrl-y", history::RedoDocument, Some("Canvas")),
            KeyBinding::new("ctrl-0", viewport::FitToScreenAction, Some("Canvas")),
            KeyBinding::new("ctrl-1", viewport::ActualSizeAction, Some("Canvas")),
            KeyBinding::new(
                "ctrl-alt-0",
                viewport::ZoomToSelectionAction,
                Some("Canvas"),
            ),
            KeyBinding::new("ctrl-=", viewport::ZoomInAction, Some("Canvas")),
            KeyBinding::new("ctrl-shift-=", viewport::ZoomInAction, Some("Canvas")),
            KeyBinding::new("ctrl--", viewport::ZoomOutAction, Some("Canvas")),
            KeyBinding::new("ctrl-shift--", viewport::ZoomOutAction, Some("Canvas")),
            KeyBinding::new("enter", crop::ApplyCropAction, Some("Canvas")),
            KeyBinding::new("left", crop::NudgeCropLeft, Some("Canvas")),
            KeyBinding::new("right", crop::NudgeCropRight, Some("Canvas")),
            KeyBinding::new("up", crop::NudgeCropUp, Some("Canvas")),
            KeyBinding::new("down", crop::NudgeCropDown, Some("Canvas")),
            KeyBinding::new("shift-left", crop::NudgeCropLeftLarge, Some("Canvas")),
            KeyBinding::new("shift-right", crop::NudgeCropRightLarge, Some("Canvas")),
            KeyBinding::new("shift-up", crop::NudgeCropUpLarge, Some("Canvas")),
            KeyBinding::new("shift-down", crop::NudgeCropDownLarge, Some("Canvas")),
            KeyBinding::new("ctrl-j", layers::DuplicateLayerAction, Some("Canvas")),
            KeyBinding::new("ctrl-shift-n", NewLayerAction, Some("Canvas")),
            KeyBinding::new("ctrl-alt-a", SelectAllLayersAction, Some("Canvas")),
            KeyBinding::new("ctrl-shift-alt-e", StampVisibleAction, Some("Canvas")),
            KeyBinding::new(
                "ctrl-shift-;",
                tool_settings::ToggleSnapAction,
                Some("Canvas"),
            ),
            KeyBinding::new("ctrl-e", layers::MergeLayerDownAction, Some("Canvas")),
            KeyBinding::new(
                "ctrl-shift-e",
                layers::FlattenDocumentAction,
                Some("Canvas"),
            ),
            KeyBinding::new("ctrl-a", selection::SelectAllAction, Some("Canvas")),
            KeyBinding::new("ctrl-d", selection::DeselectAction, Some("Canvas")),
            KeyBinding::new(
                "ctrl-t",
                selection::ToggleSelectionEditModeAction,
                Some("Canvas"),
            ),
            KeyBinding::new(
                "ctrl-shift-i",
                selection::InvertSelectionAction,
                Some("Canvas"),
            ),
            KeyBinding::new("ctrl-c", selection::CopySelectionAction, Some("Canvas")),
            KeyBinding::new("ctrl-x", selection::CutSelectionAction, Some("Canvas")),
            KeyBinding::new("ctrl-v", selection::PasteSelectionAction, Some("Canvas")),
            KeyBinding::new("delete", selection::DeleteSelectionAction, Some("Canvas")),
            KeyBinding::new(
                "backspace",
                selection::DeleteSelectionAction,
                Some("Canvas"),
            ),
            KeyBinding::new("[", input::DecreaseBrushSize, Some("Canvas")),
            KeyBinding::new("]", input::IncreaseBrushSize, Some("Canvas")),
            KeyBinding::new("shift-[", input::DecreaseBrushHardness, Some("Canvas")),
            KeyBinding::new("shift-]", input::IncreaseBrushHardness, Some("Canvas")),
        ]);

        let mut window_watch = Vec::new();
        // Resizes (and any bounds notification) are throttled; the settle
        // points always write.
        window_watch.push(cx.observe_window_bounds(window, |this, window, _cx| {
            this.remember_bounds(window);
            this.persist_bounds(false);
        }));
        // Focus loss is the settle point that follows a move, which the
        // platform reports no event for: sample and write there.
        window_watch.push(cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                // Losing the window is the lost-capture signal: never commit a
                // stroke whose release may land in another window.
                this.release_canvas_pointer(window);
                this.cancel_canvas_gesture(cx);
                this.release_navigation(cx);
            }
            this.remember_bounds(window);
            this.persist_bounds(true);
        }));
        window_watch.push(cx.on_app_quit(|this, _cx| {
            // Multi-document quit: dirty sessions that are not the one the
            // close prompt handled are written to the recovery store so the
            // next launch can restore them.
            if let Some(root) = this.autosave_cache_root.clone() {
                let _ = this.autosave_dirty_sessions(&root);
            }
            this.persist_bounds(true);
            async {}
        }));
        let close_weak = cx.weak_entity();
        window.on_window_should_close(cx, move |_window, cx| {
            close_weak
                .update(cx, |app, cx| app.should_close_window(cx))
                .unwrap_or(true)
        });

        let field_focus = cx.focus_handle();
        let slider_focus = cx.focus_handle();
        let opacity_slider_focus = cx.focus_handle().tab_stop(true);
        let move_snap_focus = cx.focus_handle().tab_stop(true);
        let move_auto_select_focus = cx.focus_handle().tab_stop(true);
        let move_transform_focus = cx.focus_handle().tab_stop(true);
        let mut app = Self {
            state: cx.new(|_| {
                let mut state = state;
                state.num_field_focus = Some(field_focus);
                state.slider_focus = Some(slider_focus);
                state.opacity_slider_focus = Some(opacity_slider_focus);
                state.move_snap_focus = Some(move_snap_focus);
                state.move_auto_select_focus = Some(move_auto_select_focus);
                state.move_transform_focus = Some(move_transform_focus);
                state
            }),
            canvas_focus_handle: cx.focus_handle().tab_stop(true),
            rename_focus: cx.focus_handle(),
            resize_focus: cx.focus_handle(),
            new_document_focus: cx.focus_handle(),
            dialog_focus: cx.focus_handle(),
            pending_new_document_focus: false,
            picker_focus: cx.focus_handle(),
            demo_canvas,
            active_document: None,
            document_tabs: Vec::new(),
            tab_cameras: HashMap::new(),
            layer_thumbs: Vec::new(),
            recent_files: Vec::new(),
            dropped_paths: Vec::new(),
            dropped_position: None,
            crop_drag: None,
            move_marquee: None,
            transform_drag: None,
            selection_rotation_drag: None,
            selection_edit_mode: false,
            group_transform_drag: None,
            pending_transform_drag_update: None,
            transform_drag_frame_pending: false,
            transform_drag_frame_generation: 0,
            snap_guides: Vec::new(),
            hover_pos: None,
            last_cursor: crate::input::CursorPolicy::SystemDefault,
            edge_pointer: None,
            edge_scroll_active: false,
            edge_scroll_generation: 0,
            edge_scroll_task: None,
            live: Some(restorable_geometry(window)),
            saved: None,
            saved_at: None,
            settings,
            sessions: SessionRegistry::default(),
            file_io,
            retired_textures: Vec::new(),
            print_backend: Arc::new(PrintService::default()),
            close_after_save: false,
            close_after_dirty: false,
            pending_window_close: false,
            pending_tab_close: None,
            input: crate::input::InputState::new(),
            clipboard: None,
            brush_tips: crate::input::BrushTipCache::default(),
            paint_smoother: input::PaintSmoother::default(),
            brush_axis_lock: None,
            brush_stroke_start: None,
            brush_stroke_start_screen: None,
            alt_sampling: false,
            brush_adjust: None,
            canvas_capture: Rc::new(Cell::new(None)),
            canvas_bounds: Rc::new(Cell::new(None)),
            checker_cache: Rc::new(RefCell::new(CheckerCache::default())),
            navigator_bounds: Rc::new(Cell::new(None)),
            space_pressed: false,
            pointer_alt: false,
            pan: None,
            slider_dragging: false,
            #[cfg(feature = "input-measurement")]
            measurement: measurement::InputMeasurement::default(),
            pending_gesture: None,
            _window_watch: window_watch,
            #[cfg(test)]
            full_composites: Cell::new(0),
            #[cfg(test)]
            preview_pushes: Cell::new(0),
            autosave_task: None,
            autosave_cache_root: None,
        };
        #[cfg(feature = "input-measurement")]
        app.measurement.share_bounds(Rc::clone(&app.canvas_bounds));
        let blur = cx.on_blur(&app.canvas_focus_handle, window, |this, _window, cx| {
            this.release_navigation(cx);
        });
        app._window_watch.push(blur);
        // Only a settings-backed launch owns the real app cache; a test mount
        // injects its own root through `new_with_cache_root`.
        app.autosave_cache_root = cache_root;
        app.apply_startup_locale();
        app.start_autosave_timer(cx);
        app.load_recent_files(cx);
        // The shortcuts are window-global: they must fire wherever focus sits,
        // so they are registered on the app rather than on a focusable element.
        register_global_action::<SaveDocument>(cx, App::save_document_action);
        register_global_action::<NewDocumentAction>(cx, App::new_document_action);
        register_global_action::<ToggleSidePanelsAction>(cx, App::toggle_side_panels_action);
        register_global_action::<SaveDocumentAs>(cx, App::save_document_as_action);
        register_global_action::<OpenDocument>(cx, App::open_document_action);
        register_global_action::<ExportDocument>(cx, App::export_document_action);
        register_global_action::<PrintDocument>(cx, App::print_document_action);
        register_global_action::<CloseDocument>(cx, App::close_document_action);
        register_global_action::<input::SetCanvasTool>(cx, App::set_canvas_tool_action);
        register_global_action::<input::ToggleSelectionShape>(
            cx,
            App::toggle_selection_shape_action,
        );
        register_global_action::<input::DecreaseBrushSize>(cx, App::decrease_brush_size_action);
        register_global_action::<input::IncreaseBrushSize>(cx, App::increase_brush_size_action);
        register_global_action::<input::DecreaseBrushHardness>(
            cx,
            App::decrease_brush_hardness_action,
        );
        register_global_action::<input::IncreaseBrushHardness>(
            cx,
            App::increase_brush_hardness_action,
        );
        register_global_action::<input::CancelCanvasGesture>(cx, App::cancel_canvas_gesture_action);
        register_global_action::<history::UndoDocument>(cx, App::undo_document_action);
        register_global_action::<history::RedoDocument>(cx, App::redo_document_action);
        register_global_action::<viewport::ZoomInAction>(cx, App::zoom_in_action);
        register_global_action::<viewport::ZoomOutAction>(cx, App::zoom_out_action);
        register_global_action::<viewport::FitToScreenAction>(cx, App::fit_to_screen_action);
        register_global_action::<viewport::ActualSizeAction>(cx, App::actual_size_action);
        register_global_action::<viewport::ZoomToSelectionAction>(
            cx,
            App::zoom_to_selection_action,
        );
        register_global_action::<crop::ApplyCropAction>(cx, App::apply_crop_action);
        register_global_action::<crop::NudgeCropLeft>(cx, App::nudge_crop_left);
        register_global_action::<crop::NudgeCropRight>(cx, App::nudge_crop_right);
        register_global_action::<crop::NudgeCropUp>(cx, App::nudge_crop_up);
        register_global_action::<crop::NudgeCropDown>(cx, App::nudge_crop_down);
        register_global_action::<crop::NudgeCropLeftLarge>(cx, App::nudge_crop_left_large);
        register_global_action::<crop::NudgeCropRightLarge>(cx, App::nudge_crop_right_large);
        register_global_action::<crop::NudgeCropUpLarge>(cx, App::nudge_crop_up_large);
        register_global_action::<crop::NudgeCropDownLarge>(cx, App::nudge_crop_down_large);
        register_global_action::<layers::DuplicateLayerAction>(cx, App::duplicate_layer_action);
        register_global_action::<layers::NewLayerAction>(cx, App::new_layer_action);
        register_global_action::<layers::SelectAllLayersAction>(cx, App::select_all_layers_action);
        register_global_action::<layers::StampVisibleAction>(cx, App::stamp_visible_action);
        register_global_action::<tool_settings::ToggleSnapAction>(cx, App::toggle_snap_action);
        register_global_action::<layers::MergeLayerDownAction>(cx, App::merge_layer_down_action);
        register_global_action::<layers::FlattenDocumentAction>(cx, App::flatten_document_action);
        register_global_action::<layers::StampVisibleAction>(cx, App::stamp_visible_action);
        register_global_action::<selection::SelectAllAction>(cx, App::select_all_action);
        register_global_action::<selection::DeselectAction>(cx, App::deselect_action);
        register_global_action::<selection::ToggleSelectionEditModeAction>(
            cx,
            App::toggle_selection_edit_mode_action,
        );
        register_global_action::<selection::InvertSelectionAction>(
            cx,
            App::invert_selection_action,
        );
        register_global_action::<selection::CopySelectionAction>(cx, App::copy_selection_action);
        register_global_action::<selection::CutSelectionAction>(cx, App::cut_selection_action);
        register_global_action::<selection::PasteSelectionAction>(cx, App::paste_selection_action);
        register_global_action::<selection::DeleteSelectionAction>(
            cx,
            App::delete_selection_action,
        );
        if let Some(path) = open_path {
            let result = match app.file_io.clone() {
                Some(file_io) => app.open_startup_document(&path, file_io.as_ref(), cx),
                None => app.open_startup_path(&path, cx),
            };
            if let Err(error) = result {
                eprintln!("photrez: startup document was not opened: {error}");
            }
        }
        if shot_id.is_some() && app.state.read(cx).has_document && app.active_document.is_none() {
            app.install_shot_document(cx);
        }
        // The recovery prompt comes after the explicit open: a document the
        // user asked for by name is on screen before an offer they did not ask
        // for can cover it.
        app.prepare_startup_recovery(cx);
        app
    }

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

    /// Project the loaded core layers onto the shell's layer rows.
    ///
    /// Row ids are positions in this projection; the selection is preserved by
    /// core layer id so a reprojection after a command does not move the user
    /// to another layer.
    pub(crate) fn project_layers(&mut self, cx: &mut Context<Self>) {
        self.project_layers_with_selection(None, cx);
    }

    /// Project the layers and force `selected` as the selected core layer.
    /// Layer creation passes the new id so it becomes active, matching the
    /// editor; plain reprojection keeps the user's selection.
    pub(crate) fn project_layers_with_selection(
        &mut self,
        selected: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(doc_id) = self
            .active_document
            .as_ref()
            .map(|document| document.doc_id.clone())
        else {
            return;
        };
        let layers = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .map(|content| content.layers.clone())
            .unwrap_or_default();
        self.state.update(cx, |state, _| {
            let preserve_empty_selection = selected.is_none()
                && state.selected_layers.is_empty()
                && state.layers.iter().any(|row| {
                    row.core_id
                        .as_ref()
                        .is_some_and(|core_id| layers.iter().any(|layer| layer.id == *core_id))
                });
            let previous_selected: Vec<String> = state
                .layers
                .iter()
                .filter(|row| state.selected_layers.contains(&row.id))
                .filter_map(|row| row.core_id.clone())
                .collect();
            let selected_core = selected.map(str::to_string).or_else(|| {
                state
                    .layers
                    .iter()
                    .find(|row| row.id == state.selected_layer)
                    .and_then(|row| row.core_id.clone())
            });
            state.layers = layers
                .iter()
                .enumerate()
                .map(|(index, layer)| crate::state::LayerRow {
                    id: index as u64,
                    name: layer.name.clone(),
                    core_id: Some(layer.id.clone()),
                })
                .collect();
            state.layer_flags = layers
                .iter()
                .map(|layer| crate::state::LayerFlags {
                    visible: layer.visible,
                    locked: layer.locked,
                    has_adjustments: layer.has_adjustments.unwrap_or(false),
                    lock_transparency: layer.lock_transparency.unwrap_or(false),
                    lock_position: layer.lock_position.unwrap_or(false),
                    lock_rotation: layer.lock_rotation.unwrap_or(false),
                    is_background: layer.is_background.unwrap_or(false),
                })
                .collect();
            let selected_core_for_props = selected_core.clone();
            state.selected_layer = selected_core
                .and_then(|core_id| {
                    state
                        .layers
                        .iter()
                        .find(|row| row.core_id.as_deref() == Some(core_id.as_str()))
                        .map(|row| row.id)
                })
                .or_else(|| state.layers.first().map(|row| row.id))
                .unwrap_or(0);
            let mut mapped: Vec<u64> = if selected.is_some() {
                Vec::new()
            } else {
                state
                    .layers
                    .iter()
                    .filter(|row| {
                        row.core_id
                            .as_ref()
                            .is_some_and(|core_id| previous_selected.contains(core_id))
                    })
                    .map(|row| row.id)
                    .collect()
            };
            if !preserve_empty_selection && !mapped.contains(&state.selected_layer) {
                mapped.push(state.selected_layer);
            }
            state.selected_layers = mapped;
            state.multi_bounds = if state.selected_layers.len() > 1 {
                let mut bounds: Option<(f64, f64, f64, f64)> = None;
                for row in state
                    .layers
                    .iter()
                    .filter(|row| state.selected_layers.contains(&row.id))
                {
                    let Some(core_id) = row.core_id.as_deref() else {
                        continue;
                    };
                    let Some(layer) = layers.iter().find(|layer| layer.id == core_id) else {
                        continue;
                    };
                    let (frame_w, frame_h) = self::transform_geometry::layer_frame_size(layer);
                    let (x, y, w, h) =
                        self::transform_geometry::layer_aabb(&layer.transform, frame_w, frame_h);
                    bounds = Some(match bounds {
                        None => (x, y, w, h),
                        Some((bx, by, bw, bh)) => {
                            let min_x = bx.min(x);
                            let min_y = by.min(y);
                            let max_x = (bx + bw).max(x + w);
                            let max_y = (by + bh).max(y + h);
                            (min_x, min_y, max_x - min_x, max_y - min_y)
                        }
                    });
                }
                bounds
            } else {
                None
            };
            let projected = selected_core_for_props
                .as_deref()
                .and_then(|core_id| layers.iter().find(|layer| layer.id == core_id))
                .or_else(|| layers.first());
            if let Some(layer) = projected {
                state.layer_opacity = layer.opacity;
                state.layer_blend = crate::app::layers::blend_label(&layer.blend_mode).to_string();
                let adjustment = layer.basic_adjustment.clone().unwrap_or_default();
                state.adjustment_brightness = adjustment.brightness;
                state.adjustment_contrast = adjustment.contrast;
                state.adjustment_saturation = adjustment.saturation;
                state.active_layer_name = layer.name.clone();
                state.active_layer_width = layer.width.round() as u32;
                state.active_layer_height = layer.height.round() as u32;
                state.active_layer_has_pixels = layer.resource_id.is_some();
                state.selected_layer_type = Some(
                    match layer.layer_type {
                        LayerType::Raster => "raster",
                        LayerType::Shape => "shape",
                        LayerType::Text => "text",
                        _ => "other",
                    }
                    .to_string(),
                );
                state.selected_text_color = layer
                    .text_data
                    .as_ref()
                    .and_then(|data| crate::palette::parse_hex(&data.color));
                state.selected_shape = layer.shape_params.clone();
                if let Some(data) = layer.text_data.as_ref() {
                    state.selected_text_font_size = data.font_size;
                    state.selected_text_line_height = data.line_height;
                    state.selected_text_letter_spacing = data.letter_spacing;
                    state.selected_text_font_weight = data.font_weight;
                    state.selected_text_italic = matches!(
                        data.font_style,
                        photrez_core::ptz::layer_params::TextFontStyle::Italic
                    );
                    state.selected_text_align = match data.align {
                        photrez_core::ptz::layer_params::TextAlign::Left => {
                            crate::state::TextAlignTool::Left
                        }
                        photrez_core::ptz::layer_params::TextAlign::Center => {
                            crate::state::TextAlignTool::Center
                        }
                        photrez_core::ptz::layer_params::TextAlign::Right => {
                            crate::state::TextAlignTool::Right
                        }
                    };
                    state.selected_text_underline = data.underline.unwrap_or(false);
                    state.selected_text_strikethrough = data.strikethrough.unwrap_or(false);
                    state.selected_text_uppercase = data.uppercase.unwrap_or(false);
                    state.selected_text_stroke_width = data.stroke.width;
                    state.selected_text_stroke_align = data
                        .stroke
                        .align
                        .unwrap_or(photrez_core::ptz::layer_params::TextStrokeAlign::Outside);
                    state.selected_text_stroke_color =
                        crate::palette::parse_hex(&data.stroke.color);
                }
            }
        });
        self.refresh_layer_thumbs(&doc_id);
    }

    #[cfg(test)]
    pub(crate) fn layer_thumb_stats_for_test(&self) -> (usize, usize) {
        let with_image = self
            .layer_thumbs
            .iter()
            .filter(|thumb| thumb.is_some())
            .count();
        (self.layer_thumbs.len(), with_image)
    }

    /// The axis-aligned multi-selection frame, using live previews while a
    /// group transform drag is active.
    fn group_overlay_frame(&self, cx: &Context<Self>) -> Option<TransformFrame> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        let state = self.state.read(cx);
        let selected_ids: Vec<String> = state
            .layers
            .iter()
            .filter(|row| state.selected_layers.contains(&row.id))
            .filter_map(|row| row.core_id.clone())
            .collect();
        if selected_ids.len() < 2 {
            return None;
        }
        let previews = self
            .group_transform_drag
            .as_ref()
            .map(|drag| drag.previews.clone())
            .unwrap_or_default();
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for id in &selected_ids {
            let layer = content.layers.iter().find(|layer| &layer.id == id)?;
            if !layer.visible {
                continue;
            }
            let transform = previews
                .iter()
                .find(|(preview_id, _)| preview_id == id)
                .map(|(_, transform)| transform.clone())
                .unwrap_or_else(|| layer.transform.clone());
            let (frame_w, frame_h) = crate::app::transform_geometry::layer_frame_size(layer);
            let (x, y, w, h) =
                crate::app::transform_geometry::layer_aabb(&transform, frame_w, frame_h);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x + w);
            max_y = max_y.max(y + h);
        }
        if !min_x.is_finite() || !min_y.is_finite() || max_x <= min_x || max_y <= min_y {
            return None;
        }
        let corners = [
            [min_x, min_y],
            [max_x, min_y],
            [max_x, max_y],
            [min_x, max_y],
        ];
        let mid_x = (min_x + max_x) / 2.0;
        let mid_y = (min_y + max_y) / 2.0;
        Some(TransformFrame {
            corners,
            handles: [
                corners[0],
                corners[1],
                corners[2],
                corners[3],
                [mid_x, min_y],
                [max_x, mid_y],
                [mid_x, max_y],
                [min_x, mid_y],
            ],
        })
    }

    /// Rebuild the 34x34 row thumbnails (raster layers only; shape/text rows
    /// keep the checker tile). Sampling is O(layers x 34x34).
    fn refresh_layer_thumbs(&mut self, doc_id: &str) {
        let mut thumbs = Vec::new();
        if let Some(session) = self.sessions.get(doc_id) {
            let loaded = session.loaded_document();
            if let Some(content) = loaded.content_snapshot() {
                for layer in &content.layers {
                    let image = if matches!(layer.layer_type, LayerType::Raster) {
                        loaded
                            .registry
                            .get_layer(doc_id, &layer.id)
                            .and_then(|canonical| {
                                thumbnail_image(
                                    canonical.width,
                                    canonical.height,
                                    &canonical.pixels,
                                    34,
                                )
                            })
                    } else {
                        None
                    };
                    thumbs.push(image);
                }
            }
        }
        self.layer_thumbs = thumbs;
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
#[path = "app/tests.rs"]
mod tests;
