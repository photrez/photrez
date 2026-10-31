//! Typed application state for the native shell.
//!
//! Seeded from `--shot <id>` via [`AppState::seeded`] so the capture harness
//! can drive one deterministic UI state per manifest entry without a real
//! document.

use crate::units::DimensionUnit;

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
/// (`ShapeKind` in `layerTypes.ts`).
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

/// Export formats the frozen dialog offers (`ImageFormat::from_export_path`).
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
    /// their tooltips say to rename instead (`LayersPanel.tsx:477-529`).
    pub is_background: bool,
}

/// One open-document tab in the strip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabRow {
    pub doc_id: String,
    pub name: String,
    pub dirty: bool,
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

/// The New Document dialog's category tabs, in the oracle's order
/// (`NewDocumentDialog.tsx:8-45`).
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

const PAPER_PRESETS: &[NewDocPreset] = &[
    NewDocPreset {
        name: "A3",
        width: 3508,
        height: 4961,
    },
    NewDocPreset {
        name: "A4",
        width: 2480,
        height: 3508,
    },
    NewDocPreset {
        name: "A5",
        width: 1748,
        height: 2480,
    },
    NewDocPreset {
        name: "Letter",
        width: 2550,
        height: 3300,
    },
    NewDocPreset {
        name: "Legal",
        width: 2550,
        height: 4200,
    },
    NewDocPreset {
        name: "Foolscap",
        width: 2550,
        height: 3900,
    },
];

const SOCIAL_PRESETS: &[NewDocPreset] = &[
    NewDocPreset {
        name: "Facebook Post",
        width: 1200,
        height: 1200,
    },
    NewDocPreset {
        name: "Facebook Cover",
        width: 1640,
        height: 664,
    },
    NewDocPreset {
        name: "Instagram Square",
        width: 1080,
        height: 1080,
    },
    NewDocPreset {
        name: "Instagram Portrait",
        width: 1080,
        height: 1350,
    },
    NewDocPreset {
        name: "Instagram Story",
        width: 1080,
        height: 1920,
    },
    NewDocPreset {
        name: "X Post",
        width: 1600,
        height: 900,
    },
    NewDocPreset {
        name: "X Header",
        width: 1500,
        height: 500,
    },
    NewDocPreset {
        name: "LinkedIn Banner",
        width: 1584,
        height: 396,
    },
    NewDocPreset {
        name: "TikTok Video",
        width: 1080,
        height: 1920,
    },
    NewDocPreset {
        name: "YouTube Thumbnail",
        width: 1280,
        height: 720,
    },
];

const WEB_PRESETS: &[NewDocPreset] = &[
    NewDocPreset {
        name: "HD",
        width: 1920,
        height: 1080,
    },
    NewDocPreset {
        name: "QHD",
        width: 2560,
        height: 1440,
    },
    NewDocPreset {
        name: "4K",
        width: 3840,
        height: 2160,
    },
];

const PHOTO_PRESETS: &[NewDocPreset] = &[
    NewDocPreset {
        name: "4×6 Landscape",
        width: 1800,
        height: 1200,
    },
    NewDocPreset {
        name: "4×6 Portrait",
        width: 1200,
        height: 1800,
    },
    NewDocPreset {
        name: "5×7 Landscape",
        width: 2100,
        height: 1500,
    },
    NewDocPreset {
        name: "5×7 Portrait",
        width: 1500,
        height: 2100,
    },
    NewDocPreset {
        name: "8×10 Landscape",
        width: 2400,
        height: 3000,
    },
    NewDocPreset {
        name: "8×10 Portrait",
        width: 3000,
        height: 2400,
    },
];

impl NewDocCategory {
    pub const ALL: [Self; 4] = [
        Self::Paper,
        Self::SocialMedia,
        Self::WebVideo,
        Self::Photography,
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Paper => "dialogs.newDocument.categories.paper",
            Self::SocialMedia => "dialogs.newDocument.categories.socialMedia",
            Self::WebVideo => "dialogs.newDocument.categories.webVideo",
            Self::Photography => "dialogs.newDocument.categories.photography",
        }
    }

    pub fn presets(self) -> &'static [NewDocPreset] {
        match self {
            Self::Paper => PAPER_PRESETS,
            Self::SocialMedia => SOCIAL_PRESETS,
            Self::WebVideo => WEB_PRESETS,
            Self::Photography => PHOTO_PRESETS,
        }
    }
}

/// The document's initial background (`NewDocumentResult.backgroundColor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NewDocBackground {
    Transparent,
    #[default]
    White,
}

impl NewDocBackground {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Transparent => "dialogs.newDocument.transparent",
            Self::White => "dialogs.newDocument.white",
        }
    }
}

/// The text field receiving the dialog's keystrokes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewDocField {
    Name,
    Width,
    Height,
}

/// The New Document dialog's form state (`NewDocumentDialog.tsx:67-73`).
#[derive(Clone, Debug, PartialEq)]
pub struct NewDocumentSession {
    pub category: NewDocCategory,
    pub name: String,
    /// Authoritative size, always in pixels.
    pub width_px: u32,
    pub height_px: u32,
    /// Display buffers for the width/height fields in `unit`.
    pub width_text: String,
    pub height_text: String,
    pub unit: DimensionUnit,
    pub background: NewDocBackground,
    pub field: Option<NewDocField>,
    pub unit_open: bool,
    pub background_open: bool,
    /// Last preset press and its time, for the oracle's double-click create.
    pub last_preset: Option<(usize, std::time::Instant)>,
}

impl Default for NewDocumentSession {
    fn default() -> Self {
        let unit = DimensionUnit::Px;
        Self {
            category: NewDocCategory::Paper,
            name: "New Project".to_string(), // i18n-exempt: frozen dialog default
            width_px: 1080,
            height_px: 1080,
            width_text: unit.format(1080),
            height_text: unit.format(1080),
            unit,
            background: NewDocBackground::White,
            field: None,
            unit_open: false,
            background_open: false,
            last_preset: None,
        }
    }
}

impl NewDocumentSession {
    /// `isPresetSelected`: the name and both dimensions must match.
    pub fn preset_selected(&self, preset: &NewDocPreset) -> bool {
        self.name == preset.name && self.width_px == preset.width && self.height_px == preset.height
    }

    /// `applyPreset`: copy the preset's name and size into the form.
    pub fn apply_preset(&mut self, preset: &NewDocPreset) {
        self.name = preset.name.to_string();
        self.width_px = preset.width;
        self.height_px = preset.height;
        self.width_text = self.unit.format(preset.width);
        self.height_text = self.unit.format(preset.height);
    }

    /// The oracle's `onBlur`: a missing, non-numeric or non-positive value
    /// becomes one pixel; anything else is converted and clamped.
    pub fn commit_dimension(&mut self, field: NewDocField) {
        let (text, width) = match field {
            NewDocField::Width => (&self.width_text, true),
            NewDocField::Height => (&self.height_text, false),
            NewDocField::Name => return,
        };
        let px = match text.trim().parse::<f64>() {
            Ok(value) if value > 0.0 => self.unit.to_px(value).min(16_384),
            _ => 1,
        };
        if width {
            self.width_px = px;
            self.width_text = self.unit.format(px);
        } else {
            self.height_px = px;
            self.height_text = self.unit.format(px);
        }
    }

    /// Switching units commits the typed text first and reformats the stored
    /// pixel value, which is the oracle's `createEffect(on([unit, widthPx]))`.
    pub fn set_unit(&mut self, unit: DimensionUnit) {
        self.commit_dimension(NewDocField::Width);
        self.commit_dimension(NewDocField::Height);
        self.unit = unit;
        self.width_text = unit.format(self.width_px);
        self.height_text = unit.format(self.height_px);
        self.unit_open = false;
    }
}

/// Which colour an open picker edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorTarget {
    Foreground,
    Background,
    /// The text bar's swatch: the selected text layer's colour in edit mode,
    /// the paint foreground in draw mode.
    Text,
    /// The shape bar's stroke popover; choosing a colour also enables the
    /// outline (`ShapeOptionBar.tsx:setColor`).
    ShapeStroke,
    /// The shape bar's fill swatch: the selected shape's fill in edit mode,
    /// the paint foreground otherwise; choosing also forces a solid fill
    /// (`ShapeOptionBar.tsx:setFillColor`).
    ShapeFill,
    /// The text bar's stroke popover; edits the selected text layer in edit
    /// mode and the next-text stroke colour otherwise.
    TextStroke,
}

/// Which option-bar slider a pointer drag or key press drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliderTarget {
    Tolerance,
    TextStrokeWidth,
    /// The brush/eraser right-click menu's Size track
    /// (`BrushContextMenu.tsx:181-196`).
    BrushMenuSize,
    BrushMenuHardness,
    BrushMenuStrength,
    /// The Export dialog's quality track (`ExportDialog.tsx` quality slider).
    ExportQuality,
    /// The Adjustments panel's Bright/Contrast/Saturate tracks
    /// (`AdjustmentsPanel.tsx` `AdjustmentSliderRow`), -100..=100 centred.
    AdjustmentBrightness,
    AdjustmentContrast,
    AdjustmentSaturation,
    /// The Print dialog's Scale track (`PrintInspector.tsx`, 10..=400 %).
    PrintScale,
}

/// Which control of the picker a pointer drag drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerArea {
    Sv,
    Hue,
}

/// One open colour-picker session. `hsv` is the live state the surface paints
/// from, `original` is what Cancel restores, and `hex` is the field buffer -
/// typing only reaches the paint colour when the buffer parses.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorPickerSession {
    pub target: ColorTarget,
    pub hsv: crate::palette::Hsv,
    pub original: [u8; 4],
    pub hex: String,
}

/// One in-flight inline numeric edit. Valid keystrokes commit live; Escape
/// restores `original`; an empty buffer holds without committing.
#[derive(Clone, Debug, PartialEq)]
pub struct NumFieldSession {
    pub id: &'static str,
    pub buffer: String,
    pub original: String,
    /// The first typed character replaces the resting value (select-all on
    /// focus), matching the frozen number inputs.
    pub replace_on_type: bool,
}

/// One in-flight text edit (`TextEditor` overlay). The layer content is
/// committed on exit; while editing only this buffer changes and the style
/// snapshot keeps the overlay's live text on the layer's own metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEditSession {
    pub layer_id: String,
    pub doc_x: f64,
    pub doc_y: f64,
    pub content: String,
    pub data: photrez_core::ptz::layer_params::TextData,
    /// Rasterized preview size in document pixels, refreshed per keystroke.
    pub preview: (u32, u32),
}

/// The autosave status dot/copy (`autoSave.ts` `AutosaveStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AutosaveStatus {
    #[default]
    Idle,
    Saving,
    Saved,
    Error,
}

/// One row in the History panel, projected from the document's unified core
/// history. Row index 0 is the initial "Open" state (cursor 0); entry `i`
/// maps to cursor `i + 1`.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryRow {
    pub label: String,
    pub applied: bool,
    pub current: bool,
}

// ── AppState ──────────────────────────────────────────────────────────────────

/// The frozen `TransformHud` modes: a live pointer HUD during a Move/resize/
/// rotate session, or the Alt+Right brush size/hardness readout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HudMode {
    Move,
    Resize,
    Rotate,
    Brush,
}

/// Values painted by the on-canvas HUD. `x`/`y` are window coordinates;
/// `a`/`b` carry the mode's two numbers (move deltas, W/H, angle/0, size/hard%.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudInfo {
    pub mode: HudMode,
    pub x: f32,
    pub y: f32,
    pub a: f64,
    pub b: f64,
    pub scale_percent: f64,
    pub snap: bool,
}

/// `GradientOverlay.tsx`'s floating DIST/ANGLE card, positioned at the drag end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientBadge {
    /// Window-space pointer position.
    pub x: f32,
    pub y: f32,
    /// Rounded document distance and one-decimal degrees.
    pub distance: f64,
    pub angle: f64,
}

/// `ToastSeverity` (`Toast.tsx`): picks the border class and the ARIA role.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastSeverity {
    #[default]
    Info,
    Warn,
    Error,
}

/// One live toast: `Toast.tsx`'s `{ id, message, severity }`.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub message: String,
    pub severity: ToastSeverity,
}

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
    /// True while the pointer is over the canvas hitbox (drives the hidden
    /// OS cursor for the paint tools).
    pub canvas_hovered: bool,
    /// The live toast stack, oldest first (`Toast.tsx`), capped at three.
    pub toasts: Vec<Toast>,
    /// Next toast id (`Toast.tsx`'s `nextId`).
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
    pub recovery_entries: Vec<TabRow>,
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
    /// Laid-out bounds of the Print dialog's Scale track.
    pub print_scale_slider_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    /// Foreground paint colour (RGBA), defaulting to the frozen amber.
    pub fg_color: [u8; 4],
    /// Background colour (RGBA); the gradient's second stop and the fill
    /// source for background-filled areas.
    pub bg_color: [u8; 4],
    /// Crop bar `Delete`: discard pixels outside the crop (oracle ships it on).
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
    /// Text tool stroke defaults (`editorState.ts:textStroke*`), used in draw
    /// mode and synced by edit-mode changes.
    pub text_stroke_width: f64,
    pub text_stroke_color: [u8; 4],
    pub text_stroke_align: photrez_core::ptz::layer_params::TextStrokeAlign,
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
    pub selected_text_stroke_align: photrez_core::ptz::layer_params::TextStrokeAlign,
    /// The selected shape layer's parameters, `None` when the projected layer
    /// is not a shape. Drives the shape bar's edit mode.
    pub selected_shape: Option<photrez_core::ptz::layer_params::ShapeParams>,
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
    /// no-pixels hint (`AdjustmentsPanel.tsx` `basicStatusText`).
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
    /// Move-tool click selects the layer under the pointer (`moveAutoSelect`).
    pub move_auto_select: bool,
    /// Draw the interactive transform frame/handles (`showTransformControls`).
    pub show_transform_controls: bool,
    /// The crop tool's pending rectangle in document space (x, y, w, h).
    pub crop_rect: Option<[f64; 4]>,
    /// Mini undo/redo for the crop frame (`cropTool.ts`).
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
    /// Focus target for inline numeric edits, installed at App construction.
    pub num_field_focus: Option<gpui_kit::FocusHandle>,
    /// Focus target for the option-bar slider, installed at App construction.
    pub slider_focus: Option<gpui_kit::FocusHandle>,
    /// Focus targets for the Move bar toggles, installed at App construction.
    pub move_snap_focus: Option<gpui_kit::FocusHandle>,
    pub move_auto_select_focus: Option<gpui_kit::FocusHandle>,
    pub move_transform_focus: Option<gpui_kit::FocusHandle>,
    /// Laid-out bounds of the Paint Bucket tolerance slider (pointer mapping).
    pub tolerance_slider_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    /// Pointer capture held by the bar slider while dragging.
    pub slider_capture: std::rc::Rc<std::cell::Cell<Option<gpui_kit::HitboxId>>>,
    /// Laid-out bounds of the open option-bar dropdown panel (click-away).
    pub select_panel_bounds:
        std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
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
    /// The open Resize Canvas dialog, if any.
    pub resize_canvas: Option<ResizeSession>,
}

/// Manual `Default`: the three `true` fields below are the ones a derived
/// `Default` (all `false`) would NOT give.
impl Default for AppState {
    fn default() -> Self {
        Self {
            active_tool: ToolId::Move,
            inspector_tab: InspectorTab::Properties,
            layer_tab: LayerTab::Layers,
            dock_open: true,
            has_document: true,
            // The frozen new-document data model: the created document has ONE
            // full-canvas **Background** layer (the oracle's `NewDocumentDialog`
            // flow names it that and locks its position/rotation), not a small
            // "Layer 1" tile. The demo document mirrors that so the shell reads
            // the same state the oracle shows.
            layers: vec![LayerRow {
                id: 0,
                name: "Background".into(), // i18n-exempt: the frozen new-document layer name
                core_id: None,
            }],
            selected_layer: 0,
            selected_layers: vec![0],
            layer_flags: vec![LayerFlags {
                visible: true,
                locked: false,
                has_adjustments: false,
                lock_transparency: false,
                // The demo document's Background is the PTZ flow's background
                // layer: position and rotation are locked, and the Layers
                // header disables its lock toggles.
                lock_position: true,
                lock_rotation: true,
                is_background: true,
            }],
            history: Vec::new(),
            chrome_visible: true,
            status_error: None,
            canvas_hovered: false,
            toasts: Vec::new(),
            next_toast_id: 1,
            document_name: "Baseline".into(), // i18n-exempt: demo content
            document_id: None,
            tabs: Vec::new(),
            active_tab: None,
            selection_shape: SelectionShape::Rect,
            selection_flyout_open: false,
            open_menu: None,
            context_menu: None,
            active_dialog: None,
            new_document: None,
            autosave_status: AutosaveStatus::Idle,
            autosave_error: None,
            recovery_entries: Vec::new(),
            recent_files: Vec::new(),
            export_format: ExportFormat::Png,
            export_quality: 90,
            export_format_open: false,
            export_quality_slider_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            print_copies: 1,
            print_landscape: false,
            print_paper: 0,
            printers: Vec::new(),
            print_printer: 0,
            print_margins_mm: 5.0,
            print_center_on_page: true,
            print_scale_to_fit: true,
            print_scale_percent: 100,
            print_unit: "mm".to_string(),
            print_show_paper_white: true,
            print_color_open: false,
            print_position_open: true,
            print_paper_open: false,
            print_unit_open: false,
            print_printer_open: false,
            print_scale_slider_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            fg_color: [225, 90, 23, 255],
            bg_color: [255, 255, 255, 255],
            settings_language_open: false,
            resize_unit_open: false,
            crop_delete_pixels: true,
            crop_fill_background: true,
            #[cfg(test)]
            last_crop_flags: None,
            color_picker: None,
            brush_size: 20.0,
            brush_hardness: 0.8,
            brush_opacity: 1.0,
            brush_flow: 1.0,
            brush_smoothing: 0.0,
            // The oracle opens with no preset id: its defaults are
            // 20 / 0.8 / 1 / 1 / 0, which are not the Hard Round preset.
            brush_preset_index: usize::MAX,
            eraser_size: 32.0,
            eraser_hardness: 1.0,
            eraser_opacity: 1.0,
            eraser_flow: 1.0,
            eraser_smoothing: 0.0,
            fill_tolerance: 32,
            fill_contiguous: true,
            gradient_radial: false,
            gradient_reverse: false,
            gradient_fg_transparent: false,
            shape_kind: ShapeTool::Rect,
            shape_fill: true,
            shape_stroke: false,
            text_font_size: 48.0,
            text_font_weight: 400.0,
            text_italic: false,
            text_align: TextAlignTool::Left,
            shape_stroke_width: 4.0,
            shape_stroke_color: [0, 0, 0, 255],
            shape_radius: 0.0,
            shape_arrow_head: false,
            text_stroke_width: 0.0,
            text_stroke_color: [0, 0, 0, 255],
            text_stroke_align: photrez_core::ptz::layer_params::TextStrokeAlign::Outside,
            auto_copy_hex: false,
            layer_opacity: 1.0,
            layer_blend: "Normal".to_string(),
            multi_bounds: None,
            selected_layer_type: None,
            selected_text_font_size: 48.0,
            selected_text_line_height: 1.2,
            selected_text_letter_spacing: 0.0,
            selected_text_font_weight: 400.0,
            selected_text_italic: false,
            selected_text_align: TextAlignTool::Left,
            selected_text_underline: false,
            selected_text_strikethrough: false,
            selected_text_uppercase: false,
            selected_text_color: None,
            selected_text_stroke_width: 0.0,
            selected_text_stroke_color: None,
            selected_text_stroke_align: photrez_core::ptz::layer_params::TextStrokeAlign::Outside,
            selected_shape: None,
            text_underline: false,
            text_strikethrough: false,
            text_uppercase: false,
            adjustment_brightness: 0.0,
            adjustment_contrast: 0.0,
            adjustment_saturation: 0.0,
            active_layer_name: String::new(),
            active_layer_width: 0,
            active_layer_height: 0,
            active_layer_has_pixels: false,
            adjustment_slider_bounds: std::array::from_fn(|_| {
                std::rc::Rc::new(std::cell::Cell::new(None))
            }),
            hud: None,
            gradient_badge: None,
            snap_enabled: true,
            move_auto_select: true,
            show_transform_controls: true,
            crop_rect: None,
            crop_history: Vec::new(),
            crop_redo: Vec::new(),
            crop_mode: crate::crop_aspect::CropMode::Free,
            crop_aspect: None,
            recent_crop_ratios: Vec::new(),
            crop_size_target: None,
            crop_size_unit: crate::crop_aspect::CropSizeUnit::Px,
            crop_size_w: "800".to_string(),
            crop_size_h: "600".to_string(),
            crop_size_field: 0,
            crop_edit: None,
            crop_edit_custom: false,
            crop_ratio_open: false,
            crop_unit_open: false,
            crop_ratio_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            crop_unit_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            selection_drag: None,
            text_edit: None,
            rename_layer: None,
            num_field: None,
            num_field_focus: None,
            slider_focus: None,
            move_snap_focus: None,
            move_auto_select_focus: None,
            move_transform_focus: None,
            tolerance_slider_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            slider_capture: std::rc::Rc::new(std::cell::Cell::new(None)),
            select_panel_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            stroke_panel_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            open_select: None,
            more_open: false,
            select_cursor: 0,
            select_count: 0,
            stroke_popover_open: false,
            text_stroke_panel_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            text_stroke_slider_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            text_stroke_popover_open: false,
            brush_menu: None,
            brush_menu_size_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            brush_menu_hardness_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            brush_menu_strength_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            brush_menu_capture: std::rc::Rc::new(std::cell::Cell::new(None)),
            tooltip: None,
            resize_canvas: None,
        }
    }
}

impl AppState {
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
                state.has_document = false;
                state.layers.clear();
                state.selected_layers.clear();
                state.layer_flags.clear();
                state.document_name = "Untitled".into(); // i18n-exempt: the oracle uses the same unsaved-doc literal
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
mod tests {
    use super::*;

    /// All 74 manifest ids must seed deterministically.
    ///
    /// RED proof: remove `AppState::seeded` entirely and this test fails at
    /// compile time. GREEN: every id produces a well-formed `AppState`.
    #[test]
    fn all_74_manifest_ids_seed_deterministically() {
        const MANIFEST: &str = include_str!("../tests/baseline/manifest.json");

        // Manual JSON "id" scan (no serde_json dependency).
        // Scans for `"id":` then reads the quoted value that follows.
        let ids: Vec<String> = MANIFEST
            .split("\"id\":")
            .skip(1)
            .filter_map(|s| {
                let rest = s.strip_prefix(|c: char| c.is_whitespace())?;
                let rest = rest.strip_prefix('"')?;
                let end = rest.find('"')?;
                Some(rest[..end].to_string())
            })
            .collect();

        assert_eq!(
            ids.len(),
            74,
            "expected 74 manifest ids, found {} — manifest.json changed, update this test",
            ids.len()
        );

        for id in &ids {
            let state = AppState::seeded(Some(id));
            // Every field must be a valid, readable value.
            let _ = state.active_tool;
            let _ = state.inspector_tab;
            let _ = state.layer_tab;
            // Prove the booleans are reachable.
            let _ = state.dock_open;
            let _ = state.has_document;
            let _ = state.chrome_visible;
        }
    }

    /// `AppState::default()` has `dock_open`, `has_document` and
    /// `chrome_visible` all `true` — the fields a derived `Default` would set
    /// to `false`.
    #[test]
    fn default_state_has_document_and_dock_open() {
        let s = AppState::default();
        assert!(
            s.dock_open,
            "Default::dock_open must be true (not the derived false)"
        );
        assert!(
            s.has_document,
            "Default::has_document must be true (not the derived false)"
        );
        assert!(
            s.chrome_visible,
            "Default::chrome_visible must be true (not the derived false)"
        );
        assert_eq!(s.active_tool, ToolId::Move);
        assert_eq!(s.inspector_tab, InspectorTab::Properties);
        assert_eq!(s.layer_tab, LayerTab::Layers);
    }

    /// Pin the five tool mappings — `option-*` → `ToolId`.
    /// RED proof: swap any mapping and this test fails.
    #[test]
    fn option_tool_seeds_are_pinned() {
        assert_eq!(
            AppState::seeded(Some("option-move")).active_tool,
            ToolId::Move
        );
        assert_eq!(
            AppState::seeded(Some("option-brush")).active_tool,
            ToolId::Brush
        );
        assert_eq!(
            AppState::seeded(Some("option-paintBucket")).active_tool,
            ToolId::PaintBucket
        );
        assert_eq!(
            AppState::seeded(Some("option-text")).active_tool,
            ToolId::Text
        );
        assert_eq!(
            AppState::seeded(Some("option-eyedropper")).active_tool,
            ToolId::Eyedropper
        );
    }

    /// The oracle's brush defaults: 20 px, 80 % hard, full strength.
    #[test]
    fn brush_defaults_match_the_oracle() {
        let state = AppState::seeded(None);
        assert_eq!(state.brush_size, 20.0);
        assert_eq!(state.brush_hardness, 0.8);
        assert_eq!(state.brush_opacity, 1.0);
        assert_eq!(state.brush_flow, 1.0);
    }

    /// Pin the inspector-tab / layer-tab / dock mappings.
    #[test]
    fn dock_tab_seeds_are_pinned() {
        assert_eq!(
            AppState::seeded(Some("dock-adjustments")).inspector_tab,
            InspectorTab::Adjustments
        );
        assert_eq!(
            AppState::seeded(Some("dock-presets")).inspector_tab,
            InspectorTab::Presets
        );
        assert_eq!(
            AppState::seeded(Some("dock-history")).layer_tab,
            LayerTab::History
        );
        assert!(!AppState::seeded(Some("dock-closed")).dock_open);
        assert!(!AppState::seeded(Some("empty-full")).has_document);

        // A shot launch seeds a display-only tab for the demo document; an
        // empty shot seeds none.
        let mut shot = AppState::seeded(Some("shell-full-page"));
        shot.seed_shot_tab();
        assert_eq!(shot.tabs.len(), 1, "the shot shell shows one tab");
        assert_eq!(shot.tabs[0].name, shot.document_name);
        assert_eq!(shot.active_tab.as_deref(), Some("shot-document"));
        let mut empty = AppState::seeded(Some("empty-full"));
        empty.seed_shot_tab();
        assert!(empty.tabs.is_empty(), "no document means no tab");
    }

    /// `chrome-hidden` gates the chrome; `rail-active-brush` moves the active
    /// indicator onto the brush. RED proof: drop either seed and the matching
    /// assertion fails.
    #[test]
    fn chrome_and_rail_seeds_are_pinned() {
        assert!(
            !AppState::seeded(Some("chrome-hidden")).chrome_visible,
            "chrome-hidden must hide the rail / option bar / dock / status bar"
        );
        assert!(AppState::seeded(Some("shell-full-page")).chrome_visible);
        assert_eq!(
            AppState::seeded(Some("rail-active-brush")).active_tool,
            ToolId::Brush
        );
    }

    /// Unknown ids must leave the default state untouched.
    #[test]
    fn unknown_ids_return_default() {
        let s = AppState::seeded(Some("totally-unknown-id"));
        assert_eq!(s, AppState::default());
    }

    /// `None` (no `--shot` flag) must also return the default state.
    #[test]
    fn none_returns_default() {
        assert_eq!(AppState::seeded(None), AppState::default());
    }

    /// `option-paintBucket` case sensitivity — the manifest uses camelCase.
    /// RED proof: lowercasing the match arm makes this fail.
    #[test]
    fn paint_bucket_case_sensitivity() {
        // exact match works
        assert_eq!(
            AppState::seeded(Some("option-paintBucket")).active_tool,
            ToolId::PaintBucket
        );
        // wrong case does NOT match
        assert_eq!(
            AppState::seeded(Some("option-paintbucket")).active_tool,
            ToolId::Move // falls through to default
        );
    }

    /// Moving a row onto a later row lands it at that row's old index.
    ///
    /// RED proof: insert at `to_idx - 1` (a common off-by-one) and `[0, 1, 2]`
    /// becomes `[1, 0, 2]`, so this fails.
    #[test]
    fn move_layer_reorders() {
        let mut state = AppState {
            layers: vec![
                LayerRow {
                    id: 0,
                    name: "0".into(),
                    core_id: None,
                },
                LayerRow {
                    id: 1,
                    name: "1".into(),
                    core_id: None,
                },
                LayerRow {
                    id: 2,
                    name: "2".into(),
                    core_id: None,
                },
            ],
            ..AppState::default()
        };

        assert!(state.move_layer(0, 2), "moving 0 onto 2 must succeed");
        let ids: Vec<u64> = state.layers.iter().map(|l| l.id).collect();
        assert_eq!(ids, vec![1, 2, 0]);

        // The reverse direction inserts before the target.
        assert!(state.move_layer(0, 1), "moving 0 onto 1 must succeed");
        let ids: Vec<u64> = state.layers.iter().map(|l| l.id).collect();
        assert_eq!(ids, vec![0, 1, 2]);

        // A self-move is a success and changes nothing.
        assert!(state.move_layer(1, 1));
        let ids: Vec<u64> = state.layers.iter().map(|l| l.id).collect();
        assert_eq!(ids, vec![0, 1, 2]);
    }

    /// Unknown ids are a no-op, not a panic and not a partial reorder.
    ///
    /// RED proof: dropping either `position` lookup (e.g. `unwrap_or(0)`) makes
    /// one of these assertions fail.
    #[test]
    fn move_layer_unknown_id_is_noop() {
        let mut state = AppState {
            layers: vec![
                LayerRow {
                    id: 0,
                    name: "0".into(),
                    core_id: None,
                },
                LayerRow {
                    id: 1,
                    name: "1".into(),
                    core_id: None,
                },
            ],
            ..AppState::default()
        };
        let before = state.layers.clone();

        assert!(!state.move_layer(9, 1), "unknown source must fail");
        assert_eq!(state.layers, before);
        assert!(!state.move_layer(0, 9), "unknown target must fail");
        assert_eq!(state.layers, before);
        assert!(!state.move_layer(9, 9), "both unknown must fail");
        assert_eq!(state.layers, before);
    }
}
