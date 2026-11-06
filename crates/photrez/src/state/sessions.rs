/// Which colour an open picker edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorTarget {
    Foreground,
    Background,
    /// The text bar's swatch: the selected text layer's colour in edit mode,
    /// the paint foreground in draw mode.
    Text,
    /// The shape bar's stroke popover; choosing a colour also enables the
    /// outline.
    ShapeStroke,
    /// The shape bar's fill swatch: the selected shape's fill in edit mode,
    /// the paint foreground otherwise; choosing also forces a solid fill.
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
    /// The brush/eraser right-click menu's Size track.
    BrushMenuSize,
    BrushMenuHardness,
    BrushMenuStrength,
    /// The Export dialog's quality track.
    ExportQuality,
    /// The Adjustments panel's Bright/Contrast/Saturate tracks,
    /// -100..=100 centred.
    AdjustmentBrightness,
    AdjustmentContrast,
    AdjustmentSaturation,
    /// The Print dialog's Scale track, 10..=400 %.
    PrintScale,
    /// The Properties panel's opacity track, 0..=100 % of the layer.
    LayerOpacity,
}

impl SliderTarget {
    /// How many tracks exist, so the focus table can be built once and checked.
    pub const COUNT: usize = 11;

    /// The track's slot in `AppState::slider_foci`. Spelled out rather than
    /// derived from the enum order, so adding a variant cannot silently point a
    /// host at another track's handle.
    pub const fn index(self) -> usize {
        match self {
            SliderTarget::Tolerance => 0,
            SliderTarget::TextStrokeWidth => 1,
            SliderTarget::BrushMenuSize => 2,
            SliderTarget::BrushMenuHardness => 3,
            SliderTarget::BrushMenuStrength => 4,
            SliderTarget::ExportQuality => 5,
            SliderTarget::AdjustmentBrightness => 6,
            SliderTarget::AdjustmentContrast => 7,
            SliderTarget::AdjustmentSaturation => 8,
            SliderTarget::PrintScale => 9,
            SliderTarget::LayerOpacity => 10,
        }
    }
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
    /// focus), matching the number inputs' select-all behaviour.
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
    pub data: photrez_core::document::layer_params::TextData,
    /// Rasterized preview size in document pixels, refreshed per keystroke.
    pub preview: (u32, u32),
}

/// The autosave status the footer dot and copy read.
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

/// The transform HUD modes: a live pointer HUD during a Move/resize/
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

/// The gradient's floating DIST/ANGLE card, positioned at the drag end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientBadge {
    /// Window-space pointer position.
    pub x: f32,
    pub y: f32,
    /// Rounded document distance and one-decimal degrees.
    pub distance: f64,
    pub angle: f64,
}

/// Toast severity: picks the border colour and the ARIA role.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastSeverity {
    #[default]
    Info,
    Warn,
    Error,
}

/// One live toast: its id, message and severity.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub message: String,
    pub severity: ToastSeverity,
}
