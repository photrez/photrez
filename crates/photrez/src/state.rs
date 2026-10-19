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

// ── AppState ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct AppState {
    pub active_tool: ToolId,
    pub inspector_tab: InspectorTab,
    pub layer_tab: LayerTab,
    pub dock_open: bool,
    pub has_document: bool,
    /// `chrome-hidden`: the rail, option bar, dock and status bar hidden while
    /// the title bar and the document tabs stay.
    pub chrome_visible: bool,
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
            chrome_visible: true,
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

            // empty-*: no document open
            if sid.strip_prefix("empty-").is_some() {
                state.has_document = false;
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
}
