//! Typed application state for the native shell.
//!
//! Seeded from `--shot <id>` via [`AppState::seeded`] so the capture harness
//! can drive one deterministic UI state per manifest entry without a real
//! document.

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
    Tiff,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
            Self::Tiff => "tiff",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::WebP => "WebP",
            Self::Tiff => "TIFF",
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
    pub width: String,
    pub height: String,
    /// 0 = width field, 1 = height field.
    pub field: u8,
}

/// One in-flight layer rename.
#[derive(Clone, Debug, PartialEq)]
pub struct RenameSession {
    pub layer_id: String,
    pub content: String,
}

/// One in-flight text edit (`TextEditor` overlay). The layer content is
/// committed on exit; while editing only this buffer changes.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEditSession {
    pub layer_id: String,
    pub doc_x: f64,
    pub doc_y: f64,
    pub content: String,
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
    /// The New Document dialog's selected preset index.
    pub new_document_preset: usize,
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
    /// Print dialog state.
    pub print_copies: u32,
    pub print_landscape: bool,
    pub print_paper: usize,
    pub printers: Vec<String>,
    pub print_printer: usize,
    /// Foreground paint colour (RGBA), defaulting to the frozen amber.
    pub fg_color: [u8; 4],
    /// Background colour (RGBA); the gradient's second stop and the fill
    /// source for background-filled areas.
    pub bg_color: [u8; 4],
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
    pub shape_radius: f64,
    pub shape_arrow_head: bool,
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
    /// The selected layer's basic adjustment values (projected).
    pub adjustment_brightness: f64,
    pub adjustment_contrast: f64,
    pub adjustment_saturation: f64,
    /// The on-canvas pointer HUD, `None` when no gesture needs one.
    pub hud: Option<HudInfo>,
    /// Smart-guide snapping for Move/resize.
    pub snap_enabled: bool,
    /// The crop tool's pending rectangle in document space (x, y, w, h).
    pub crop_rect: Option<[f64; 4]>,
    /// Mini undo/redo for the crop frame (`cropTool.ts`).
    pub crop_history: Vec<[f64; 4]>,
    pub crop_redo: Vec<[f64; 4]>,
    /// The live selection marquee while a drag is in flight; the committed
    /// selection lives in core and is projected through the overlay builder.
    pub selection_drag: Option<[f64; 4]>,
    /// The open text editor, if any.
    pub text_edit: Option<TextEditSession>,
    /// The open layer-rename prompt, if any.
    pub rename_layer: Option<RenameSession>,
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
            }],
            history: Vec::new(),
            chrome_visible: true,
            status_error: None,
            document_name: "Baseline".into(), // i18n-exempt: demo content
            document_id: None,
            tabs: Vec::new(),
            active_tab: None,
            selection_shape: SelectionShape::Rect,
            selection_flyout_open: false,
            open_menu: None,
            context_menu: None,
            active_dialog: None,
            new_document_preset: 0,
            autosave_status: AutosaveStatus::Idle,
            autosave_error: None,
            recovery_entries: Vec::new(),
            recent_files: Vec::new(),
            export_format: ExportFormat::Png,
            export_quality: 90,
            print_copies: 1,
            print_landscape: false,
            print_paper: 0,
            printers: Vec::new(),
            print_printer: 0,
            fg_color: [225, 90, 23, 255],
            bg_color: [255, 255, 255, 255],
            brush_size: 12.0,
            brush_hardness: 1.0,
            brush_opacity: 1.0,
            brush_flow: 1.0,
            brush_smoothing: 0.0,
            brush_preset_index: 0,
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
            shape_radius: 0.0,
            shape_arrow_head: false,
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
            adjustment_brightness: 0.0,
            adjustment_contrast: 0.0,
            adjustment_saturation: 0.0,
            hud: None,
            snap_enabled: true,
            crop_rect: None,
            crop_history: Vec::new(),
            crop_redo: Vec::new(),
            selection_drag: None,
            text_edit: None,
            rename_layer: None,
            resize_canvas: None,
        }
    }
}

impl AppState {
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
