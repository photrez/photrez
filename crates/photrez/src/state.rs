//! Typed application state for the native shell.
//!
//! Seeded from `--shot <id>` via [`AppState::seeded`] so the capture harness
//! can drive one deterministic UI state per manifest entry without a real
//! document.

use crate::units::DimensionUnit;

mod defaults;
mod new_document;
mod sessions;

pub use new_document::*;
pub use sessions::*;

// ── enums ─────────────────────────────────────────────────────────────────────

/// The ten tools the tool rail exposes (matches `option-*` suffixes in the
/// baseline manifest).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolId {
    Move,
    Selection,
    Crop,
    Eyedropper,
    Brush,
    Eraser,
    PaintBucket,
    Gradient,
    Shape,
    Text,
}

/// Which tab of the InspectorDock is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InspectorTab {
    #[default]
    Properties,
    Adjustments,
    Presets,
}

/// Which tab of the LayerDock is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LayerTab {
    #[default]
    Layers,
    History,
}

/// The selection tool's sub-variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SelectionShape {
    #[default]
    Rect,
    Ellipse,
}

/// Text alignment for newly created text layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextAlignTool {
    #[default]
    Left,
    Center,
    Right,
}

/// The ten shape variants the Shape option bar exposes
/// The shape the Move-tool transform HUD takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShapeTool {
    #[default]
    Rect,
    Ellipse,
    Line,
    Triangle,
    Star,
    BlockArrow,
    Heart,
    Diamond,
    SpeechBubble,
    Hexagon,
}

/// A top-level application menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuId {
    File,
    Edit,
    Image,
    Layer,
    View,
    Window,
    Help,
    Settings,
}

/// A shell dialog reachable through the menu command surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogId {
    NewDocument,
    Settings,
    About,
    DirtyClose,
    Recovery,
    Export,
    Print,
    Rename,
    ResizeCanvas,
}

/// Export formats the dialog offers (`ImageFormat::from_export_path`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ExportFormat {
    #[default]
    Png,
    Jpeg,
    WebP,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::WebP => "WebP",
        }
    }
}

// ── layers ────────────────────────────────────────────────────────────────────

/// One row in the Layers panel. `core_id` is the projected document layer id;
/// `None` means the row is demo/seed data with no document behind it.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerRow {
    pub id: u64,
    pub name: String,
    pub core_id: Option<String>,
}

/// Per-layer visibility/lock flags, index-aligned with [`AppState::layers`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerFlags {
    pub visible: bool,
    pub locked: bool,
    /// The layer carries a basic adjustment (brightness/contrast/saturation).
    /// Core stores it as `PtzLayer::has_adjustments`; the row shows its
    /// indicator.
    pub has_adjustments: bool,
    /// `PtzLayer::lock_transparency` / `lock_position` / `lock_rotation`;
    /// the Lock header's three partial-lock toggles read and write them.
    pub lock_transparency: bool,
    pub lock_position: bool,
    pub lock_rotation: bool,
    /// Background layers cannot be locked; the header disables its toggles and
    /// their tooltips say to rename instead.
    pub is_background: bool,
}

/// What the transform rows show for the selected layer: position, rotation, its
/// size in document pixels and the scale that produced that size. The size is
/// the scaled frame, so a layer at half scale reads half its source width.
///
/// The three lock flags decide which rows answer an edit: `locked` stops all of
/// them, `lock_position` stops X and Y, `lock_rotation` stops R. The 0.1 build
/// product guards the same three ways, per row rather than for the panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerTransformReadout {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub locked: bool,
    pub lock_position: bool,
    pub lock_rotation: bool,
}

impl LayerTransformReadout {
    /// Whether one transform row answers an edit. `axis` is the field id's
    /// last letter: `x`, `y`, `w`, `h` or `r`.
    pub fn editable(&self, axis: char) -> bool {
        if self.locked {
            return false;
        }
        match axis {
            'x' | 'y' => !self.lock_position,
            'r' => !self.lock_rotation,
            _ => true,
        }
    }
}

/// One open-document tab in the strip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabRow {
    pub doc_id: String,
    pub name: String,
    pub dirty: bool,
}

/// One archive in the startup recovery prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryEntry {
    pub doc_id: String,
    pub name: String,
    /// An open session already holds this document. The prompt names it, because
    /// restoring then acts on the document on screen instead of adding a tab.
    pub already_open: bool,
}

/// The Resize Canvas dialog's editable dimensions.
#[derive(Clone, Debug, PartialEq)]
pub struct ResizeSession {
    /// Canonical sizes in pixels; the text buffers below are display only.
    pub width_px: u32,
    pub height_px: u32,
    pub width: String,
    pub height: String,
    /// 0 = width field, 1 = height field.
    pub field: u8,
    pub unit: DimensionUnit,
    /// The document aspect ratio captured when the dialog opened.
    pub aspect: f64,
    pub aspect_locked: bool,
    /// The field the user last typed into; the aspect lock derives the other.
    pub edited: Option<u8>,
}

/// One in-flight layer rename.
#[derive(Clone, Debug, PartialEq)]
pub struct RenameSession {
    pub layer_id: String,
    pub content: String,
}

/// The New Document dialog's category tabs, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NewDocCategory {
    #[default]
    Paper,
    SocialMedia,
    WebVideo,
    Photography,
}

/// One preset card in the New Document dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewDocPreset {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
}

// ── AppState ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct AppState {
    pub active_tool: ToolId,
    pub inspector_tab: InspectorTab,
    pub layer_tab: LayerTab,
    pub dock_open: bool,
    pub has_document: bool,
    /// Layer rows shown in the Layers panel, top of the stack first.
    pub layers: Vec<LayerRow>,
    /// Visibility/lock flags, index-aligned with `layers`.
    pub layer_flags: Vec<LayerFlags>,
    /// `id` of the primary selected row.
    pub selected_layer: u64,
    /// All selected row ids (Ctrl-click adds/removes; the primary is last).
    pub selected_layers: Vec<u64>,
    /// History rows projected from the active document's core stream.
    pub history: Vec<HistoryRow>,
    /// `chrome-hidden`: the rail, option bar, dock and status bar hidden while
    /// the title bar and the document tabs stay.
    pub chrome_visible: bool,
    /// The last save failure, shown in the status bar until a save succeeds.
    /// `None` is the resting state the baseline captures.
    pub status_error: Option<String>,
    /// The live toast stack, oldest first, capped at three.
    pub toasts: Vec<Toast>,
    /// Next toast id.
    pub next_toast_id: u64,
    /// The active document's display name and id, projected for the tab strip.
    pub document_name: String,
    pub document_id: Option<String>,
    /// Projected open-document tabs and the active one.
    pub tabs: Vec<TabRow>,
    pub active_tab: Option<String>,
    /// The selection tool's chosen sub-variant.
    pub selection_shape: SelectionShape,
    /// The right-click sub-variant popover is open.
    pub selection_flyout_open: bool,
    /// The open top-level menu, if any.
    pub open_menu: Option<MenuId>,
    /// The canvas context menu's window position, if open.
    pub context_menu: Option<(f32, f32)>,
    /// The mounted shell dialog, if any.
    pub active_dialog: Option<DialogId>,
    /// The New Document dialog's form; `None` until the dialog opens.
    pub new_document: Option<NewDocumentSession>,
    /// Auto-save status shown in the footer (`Saving...` / `Saved` / error).
    pub autosave_status: AutosaveStatus,
    pub autosave_error: Option<String>,
    /// Recoverable autosave documents offered at startup.
    pub recovery_entries: Vec<RecoveryEntry>,
    /// Recent project paths, newest first (File menu).
    pub recent_files: Vec<String>,
    /// Export dialog state.
    pub export_format: ExportFormat,
    pub export_quality: u8,
    /// The Export dialog's format list is open.
    pub export_format_open: bool,
    /// Laid-out bounds of the Export dialog's quality slider.
    pub export_quality_slider_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    /// Print dialog state.
    pub print_copies: u32,
    pub print_landscape: bool,
    pub print_paper: usize,
    pub printers: Vec<String>,
    /// Why the printer list could not be read. The dialog reports the failure
    /// instead of showing an empty list, because an unreadable spool and no
    /// installed printers look identical otherwise.
    pub print_error: Option<String>,
    pub print_printer: usize,
    /// Print layout options driven by the PrintInspector.
    pub print_margins_mm: f64,
    pub print_center_on_page: bool,
    pub print_scale_to_fit: bool,
    pub print_scale_percent: u32,
    pub print_unit: String,
    pub print_show_paper_white: bool,
    pub print_color_open: bool,
    pub print_position_open: bool,
    pub print_paper_open: bool,
    pub print_unit_open: bool,
    pub print_printer_open: bool,
    /// Each paper size the selected printer's driver reports, as its name with
    /// its millimetre width and height. `None` means no enumeration has run,
    /// which on macOS and Linux is also the permanent answer, so the dialog
    /// makes no listing claim from it.
    pub print_papers: Option<Vec<(String, f64, f64)>>,
    /// Why the driver refused to list its paper sizes. The dialog prints the
    /// reason instead of showing an empty list, as the printer row does.
    pub print_papers_error: Option<String>,
    /// The dialog's printed Width and Height at 100 % scale, measured in the
    /// unit the dialog shows. A typed size edit divides by this basis to get
    /// the scale percentage, so the edit needs no inverse unit conversion.
    pub print_size_basis: Option<(f64, f64)>,
    /// Laid-out bounds of the Print dialog's Scale track.
    pub print_scale_slider_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    /// Foreground paint colour (RGBA), defaulting to the brand amber.
    pub fg_color: [u8; 4],
    /// Background colour (RGBA); the gradient's second stop and the fill
    /// source for background-filled areas.
    pub bg_color: [u8; 4],
    /// Crop bar `Delete`: discard pixels outside the crop (on by default).
    /// The Settings dialog's language list is expanded (`LanguageSwitcher`).
    pub settings_language_open: bool,
    /// The Resize Canvas unit list is expanded.
    pub resize_unit_open: bool,
    pub crop_delete_pixels: bool,
    /// Crop bar `Fill BG`: bake the background colour under the crop.
    pub crop_fill_background: bool,
    /// Test seam: the crop flags actually sent to core on the last Apply.
    #[cfg(test)]
    pub last_crop_flags: Option<(bool, Option<[u8; 4]>)>,
    /// The open colour picker, if any.
    pub color_picker: Option<ColorPickerSession>,
    /// Brush/eraser diameter in document pixels.
    pub brush_size: f64,
    /// Brush edge hardness in `0..=1`.
    pub brush_hardness: f64,
    /// Brush stroke alpha multiplier (`opacity * flow`).
    pub brush_opacity: f64,
    pub brush_flow: f64,
    /// Smoothing strength in `0..=70` (`PaintSmoother` window).
    pub brush_smoothing: f64,
    /// Index into the brush preset cycle for the active paint tool.
    pub brush_preset_index: usize,
    /// The eraser keeps its own size/hardness/opacity/flow/smoothing.
    pub eraser_size: f64,
    pub eraser_hardness: f64,
    pub eraser_opacity: f64,
    pub eraser_flow: f64,
    pub eraser_smoothing: f64,
    /// Paint bucket options (`fillTolerance` default 32, contiguous on).
    pub fill_tolerance: u32,
    pub fill_contiguous: bool,
    /// Gradient type and direction.
    pub gradient_radial: bool,
    pub gradient_reverse: bool,
    pub gradient_fg_transparent: bool,
    /// Text tool defaults used when a text layer is created.
    pub text_font_size: f64,
    pub text_font_weight: f64,
    pub text_italic: bool,
    pub text_align: TextAlignTool,
    /// Shape tool options.
    pub shape_kind: ShapeTool,
    pub shape_fill: bool,
    pub shape_stroke: bool,
    pub shape_stroke_width: f64,
    pub shape_stroke_color: [u8; 4],
    pub shape_radius: f64,
    pub shape_arrow_head: bool,
    /// Text tool stroke defaults, used in draw
    /// mode and synced by edit-mode changes.
    pub text_stroke_width: f64,
    pub text_stroke_color: [u8; 4],
    pub text_stroke_align: photrez_core::document::layer_params::TextStrokeAlign,
    /// The selected layer's opacity and blend label, projected with the rows.
    pub layer_opacity: f64,
    pub layer_blend: String,
    /// Eyedropper auto-copies the sampled HEX to the OS clipboard.
    pub auto_copy_hex: bool,
    /// Union AABB `(x, y, w, h)` of a multi-layer selection, projected with the
    /// rows; `None` for zero or one selected layer.
    pub multi_bounds: Option<(f64, f64, f64, f64)>,
    /// The selected layer's type and text parameters (projected).
    pub selected_layer_type: Option<String>,
    pub selected_text_font_size: f64,
    pub selected_text_line_height: f64,
    pub selected_text_letter_spacing: f64,
    pub selected_text_font_weight: f64,
    pub selected_text_italic: bool,
    pub selected_text_align: crate::state::TextAlignTool,
    pub selected_text_underline: bool,
    pub selected_text_strikethrough: bool,
    pub selected_text_uppercase: bool,
    /// The selected text layer's colour as RGBA, `None` when the projected
    /// layer carries no text data.
    pub selected_text_color: Option<[u8; 4]>,
    /// The selected text layer's stroke, projected like the other text
    /// parameters; `None` colour when the projected layer has no text data.
    pub selected_text_stroke_width: f64,
    pub selected_text_stroke_color: Option<[u8; 4]>,
    pub selected_text_stroke_align: photrez_core::document::layer_params::TextStrokeAlign,
    /// The selected shape layer's parameters, `None` when the projected layer
    /// is not a shape. Drives the shape bar's edit mode.
    pub selected_shape: Option<photrez_core::document::layer_params::ShapeParams>,
    pub text_underline: bool,
    pub text_strikethrough: bool,
    pub text_uppercase: bool,
    /// The selected layer's basic adjustment values (projected).
    pub adjustment_brightness: f64,
    pub adjustment_contrast: f64,
    pub adjustment_saturation: f64,
    /// The selected layer's name and pixel size, for the Adjustments panel's
    /// Selected Layer card.
    pub active_layer_name: String,
    pub active_layer_width: u32,
    pub active_layer_height: u32,
    /// Whether the selected layer owns a pixel store; drives the panel's
    /// no-pixels hint.
    pub active_layer_has_pixels: bool,
    /// Laid-out bounds of the Adjustments panel's Bright/Contrast/Saturate
    /// tracks, in that order.
    pub adjustment_slider_bounds:
        [std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>; 3],
    /// The on-canvas pointer HUD, `None` when no gesture needs one.
    pub hud: Option<HudInfo>,
    /// The gradient drag readout (distance/angle) while the guide line paints.
    pub gradient_badge: Option<GradientBadge>,
    /// Smart-guide snapping for Move/resize.
    pub snap_enabled: bool,
    /// Move-tool click selects the layer under the pointer.
    pub move_auto_select: bool,
    /// Draw the interactive transform frame/handles.
    pub show_transform_controls: bool,
    /// Keep the two size axes at one ratio. Shared by the Move bar's ratio
    /// toggle and the Properties panel's link button, which drive the same
    /// state in the 0.1 build; it starts on.
    pub constrain_ratio: bool,
    /// Which not-yet-built adjustment groups are expanded, by section id. The
    /// 0.1 build opens each one independently and shows its In Development card.
    pub open_adjustment_sections: Vec<String>,
    /// The crop tool's pending rectangle in document space (x, y, w, h).
    pub crop_rect: Option<[f64; 4]>,
    /// Mini undo/redo for the crop frame.
    pub crop_history: Vec<[f64; 4]>,
    pub crop_redo: Vec<[f64; 4]>,
    /// Crop bar mode (`cropMode`): the frame follows the drag, a ratio, or a
    /// physical size target.
    pub crop_mode: crate::crop_aspect::CropMode,
    /// The active ratio when `crop_mode` is `Ratio`.
    pub crop_aspect: Option<(u32, u32)>,
    /// The three most recent ratios (`recentRatios`), newest first.
    pub recent_crop_ratios: Vec<(u32, u32)>,
    /// The pixel target the Size mode writes (`cropSizeTarget`).
    pub crop_size_target: Option<(u32, u32)>,
    pub crop_size_unit: crate::crop_aspect::CropSizeUnit,
    /// The display buffers for the Size fields, kept in the chosen unit so a
    /// round-trip does not drift (the source keeps `sizeWVal`/`sizeHVal`).
    pub crop_size_w: String,
    pub crop_size_h: String,
    pub crop_size_field: usize,
    /// The field buffer while a Size/Custom value is being typed.
    pub crop_edit: Option<String>,
    /// `true` while the buffer belongs to the Custom W:H pair, not the Size pair.
    pub crop_edit_custom: bool,
    /// The ratio dropdown panel.
    pub crop_ratio_open: bool,
    /// The Size unit list panel.
    pub crop_unit_open: bool,
    /// Window bounds of the ratio trigger and the unit trigger, so their lists
    /// can paint in the overlay layer (a list inside the fixed-height bar would
    /// be centred on the row and land over the tab strip).
    pub crop_ratio_bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    pub crop_unit_bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    /// The live selection marquee while a drag is in flight; the committed
    /// selection lives in core and is projected through the overlay builder.
    pub selection_drag: Option<[f64; 4]>,
    /// The open text editor, if any.
    pub text_edit: Option<TextEditSession>,
    /// The open layer-rename prompt, if any.
    pub rename_layer: Option<RenameSession>,
    /// The open inline numeric edit, if any.
    pub num_field: Option<NumFieldSession>,
    /// One tabbable focus handle per inline numeric field, in the order of
    /// `NUM_FIELD_IDS`, and installed at App construction.
    pub num_field_foci: Vec<gpui_kit::FocusHandle>,
    /// One tabbable focus handle per slider track, indexed by
    /// `SliderTarget::index` and installed at App construction.
    pub slider_foci: Vec<gpui_kit::FocusHandle>,
    /// Focus targets for the Move bar toggles, installed at App construction.
    pub move_snap_focus: Option<gpui_kit::FocusHandle>,
    pub move_auto_select_focus: Option<gpui_kit::FocusHandle>,
    pub move_transform_focus: Option<gpui_kit::FocusHandle>,
    /// One tabbable focus handle per New Document text field, indexed by
    /// `new_document_field_index` and installed at App construction.
    pub new_document_field_foci: Vec<gpui_kit::FocusHandle>,
    /// One tabbable focus handle per Resize Canvas size field, indexed by the
    /// session's `field` value (0 width, 1 height).
    pub resize_field_foci: Vec<gpui_kit::FocusHandle>,
    /// Tabbable focus handle for the Rename prompt's text field.
    pub rename_field_focus: Option<gpui_kit::FocusHandle>,
    /// The handle that holds the open dialog's subtree in the keyboard cycle,
    /// so Tab cannot reach the shell behind a modal.
    pub dialog_trap_focus: Option<gpui_kit::FocusHandle>,
    /// Laid-out bounds of the Paint Bucket tolerance slider (pointer mapping).
    pub tolerance_slider_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    /// Laid-out bounds of the Properties panel's opacity track (pointer mapping).
    pub opacity_slider_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    /// Pointer capture held by the bar slider while dragging.
    pub slider_capture: std::rc::Rc<std::cell::Cell<Option<gpui_kit::HitboxId>>>,
    /// Laid-out bounds of the open option-bar dropdown panel (click-away).
    pub select_panel_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    /// Laid-out bounds of the overflow panel itself (click-away). Held apart
    /// from the dropdown cell because a preset list opens inside the overflow
    /// and would otherwise leave a stale rect behind to judge the panel with.
    pub more_panel_bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    /// The option-bar dropdown currently open, if any.
    pub open_select: Option<&'static str>,
    /// The option bar's overflow (`MoreDropdown`) panel, independent of
    /// `open_select` so a preset list can open inside it.
    pub more_open: bool,
    pub select_cursor: usize,
    pub select_count: usize,
    /// Laid-out bounds of the shape bar's stroke popover (click-away).
    pub stroke_panel_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    /// The shape bar's stroke popover is open.
    pub stroke_popover_open: bool,
    /// Laid-out bounds of the text bar's stroke popover (click-away).
    pub text_stroke_panel_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    /// Laid-out bounds of the text stroke width slider.
    pub text_stroke_slider_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    /// The text bar's stroke popover is open.
    pub text_stroke_popover_open: bool,
    /// The brush/eraser right-click menu's anchor in window pixels, if open.
    pub brush_menu: Option<(f32, f32)>,
    pub brush_menu_size_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    pub brush_menu_hardness_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    pub brush_menu_strength_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    /// Pointer capture held by a brush-menu slider while dragging.
    pub brush_menu_capture: std::rc::Rc<std::cell::Cell<Option<gpui_kit::HitboxId>>>,
    /// Control whose tooltip is showing, if any. A `String` because layer-row
    /// tooltips carry the row's numeric id, which no static key can encode.
    pub tooltip: Option<String>,
    /// Tool-rail button under the pointer, if any. The rail resolves its icon
    /// tint from this because `Icon` reads its own colour field at build time.
    pub hovered_tool: Option<ToolId>,
    /// The open Resize Canvas dialog, if any.
    pub resize_canvas: Option<ResizeSession>,
}

impl AppState {
    /// The handle a track takes so it can be tabbed to and stepped with arrows.
    pub fn slider_focus_for(&self, target: SliderTarget) -> Option<gpui_kit::FocusHandle> {
        self.slider_foci.get(target.index()).cloned()
    }

    /// How many New Document text fields exist, so the focus table is built
    /// once and the tab order is checkable.
    pub const NEW_DOCUMENT_FIELD_COUNT: usize = 3;
    /// How many Resize Canvas size fields exist.
    pub const RESIZE_FIELD_COUNT: usize = 2;

    /// A field's slot in `AppState::new_document_field_foci`, spelled out
    /// rather than read from the enum order, so adding a variant cannot
    /// silently point a field at another field's handle.
    pub fn new_document_field_index(field: NewDocField) -> usize {
        match field {
            NewDocField::Name => 0,
            NewDocField::Width => 1,
            NewDocField::Height => 2,
        }
    }

    /// The handle a New Document field takes so Tab can reach it.
    pub fn new_document_field_focus(&self, field: NewDocField) -> Option<gpui_kit::FocusHandle> {
        self.new_document_field_foci
            .get(Self::new_document_field_index(field))
            .cloned()
    }

    /// The handle a Resize Canvas size field takes so Tab can reach it.
    pub fn resize_field_focus(&self, index: u8) -> Option<gpui_kit::FocusHandle> {
        self.resize_field_foci.get(usize::from(index)).cloned()
    }

    /// The handle one inline numeric field takes so Tab can reach it. The id is
    /// spelled out rather than read off a shared handle, so renaming a field
    /// cannot point it at another field's handle. Traversal order comes from
    /// the view tree, not from the id list. The list lives beside the edits
    /// that own those ids, in `crate::app::num_field`.
    pub fn num_field_focus_for(&self, id: &str) -> Option<gpui_kit::FocusHandle> {
        crate::app::num_field::NUM_FIELD_IDS
            .iter()
            .position(|entry| *entry == id)
            .and_then(|index| self.num_field_foci.get(index))
            .cloned()
    }

    /// A `--shot` launch seeds the demo document without a session, so the tab
    /// strip's projection from real sessions would be empty while the document
    /// is on screen. Seed the display-only tab row that the captured shell
    /// shows (one open, active document).
    pub(crate) fn seed_shot_tab(&mut self) {
        if !self.has_document {
            return;
        }
        let id = "shot-document".to_string();
        self.tabs = vec![TabRow {
            doc_id: id.clone(),
            name: self.document_name.clone(),
            dirty: false,
        }];
        self.active_tab = Some(id);
    }

    /// The product dialog a capture asks to open itself, or `None`.
    ///
    /// Ids beginning `dialog-` belong to the capture fixture in
    /// `views/dialogs.rs`, whose bodies are stubbed on purpose. `real-dialog-*`
    /// names the live view instead, so the body gets pixel evidence of its own.
    pub fn real_dialog_shot(id: Option<&str>) -> Option<DialogId> {
        match id? {
            "real-dialog-new-document" => Some(DialogId::NewDocument),
            "real-dialog-export" => Some(DialogId::Export),
            "real-dialog-print" => Some(DialogId::Print),
            "real-dialog-resize-canvas" => Some(DialogId::ResizeCanvas),
            "real-dialog-rename" => Some(DialogId::Rename),
            _ => None,
        }
    }

    /// Build the state that matches a manifest shot id.
    ///
    /// `None` and every id that is not recognised returns the default state.
    pub fn seeded(id: Option<&str>) -> Self {
        let mut state = Self::default();

        if let Some(sid) = id {
            // option-*: sets active_tool
            if let Some(suffix) = sid.strip_prefix("option-") {
                state.active_tool = match suffix {
                    "move" => ToolId::Move,
                    "selection" => ToolId::Selection,
                    "crop" => ToolId::Crop,
                    "eyedropper" => ToolId::Eyedropper,
                    "brush" => ToolId::Brush,
                    "eraser" => ToolId::Eraser,
                    "paintBucket" => ToolId::PaintBucket,
                    "gradient" => ToolId::Gradient,
                    "shape" => ToolId::Shape,
                    "text" => ToolId::Text,
                    _ => state.active_tool, // unknown: keep default
                };
            }

            // dock-properties / dock-adjustments / dock-presets → inspector tab
            if sid == "dock-properties" {
                state.inspector_tab = InspectorTab::Properties;
            }
            if sid == "dock-adjustments" {
                state.inspector_tab = InspectorTab::Adjustments;
            }
            if sid == "dock-presets" {
                state.inspector_tab = InspectorTab::Presets;
            }

            // dock-history → layer tab
            if sid == "dock-history" {
                state.layer_tab = LayerTab::History;
            }

            // dock-closed → dock hidden
            if sid == "dock-closed" {
                state.dock_open = false;
            }

            // empty-*: no document open — the demo content must go with it, or
            // the chrome (title, layer rows, thumbnails) would still describe
            // the demo document the empty state does not have.
            if sid.strip_prefix("empty-").is_some() {
                state = Self::empty();
            }

            // chrome-hidden: rail, option bar, dock and status bar hidden
            if sid == "chrome-hidden" {
                state.chrome_visible = false;
            }

            // rail-active-brush: the brush carries the active indicator
            if sid == "rail-active-brush" {
                state.active_tool = ToolId::Brush;
            }
        }

        state
    }

    /// The fresh-launch state: no document, and none of the demo layer
    /// projection that [`Default::default`] carries for the capture fixtures.
    pub(crate) fn empty() -> Self {
        Self {
            has_document: false,
            layers: Vec::new(),
            selected_layers: Vec::new(),
            layer_flags: Vec::new(),
            document_name: "Untitled".into(), // i18n-exempt: unsaved-document label
            ..Default::default()
        }
    }

    /// Move the row `from_id` so it sits at `to_id`'s current index.
    ///
    /// Both ids must exist; unknown ids leave the order untouched and return
    /// `false`. `from_id == to_id` is a no-op success. Moving 0 onto 2 in
    /// `[0, 1, 2]` yields `[1, 2, 0]`.
    pub fn move_layer(&mut self, from_id: u64, to_id: u64) -> bool {
        let Some(from_idx) = self.layers.iter().position(|l| l.id == from_id) else {
            return false;
        };
        let Some(to_idx) = self.layers.iter().position(|l| l.id == to_id) else {
            return false;
        };
        if from_idx == to_idx {
            return true;
        }
        let row = self.layers.remove(from_idx);
        self.layers.insert(to_idx, row);
        true
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
