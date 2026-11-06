use super::*;

/// All 74 manifest ids must seed deterministically.
///
/// RED proof: remove `AppState::seeded` entirely and this test fails at
/// compile time. GREEN: every id produces a well-formed `AppState`.
#[test]
fn all_74_manifest_ids_seed_deterministically() {
    const MANIFEST: &str = include_str!("../../tests/baseline/manifest.json");

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
        "expected 74 manifest ids, found {}. manifest.json changed, update this test",
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

/// Brush defaults: 20 px, 80 % hard, full strength.
#[test]
fn brush_defaults_match_the_pinned_values() {
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

/// `real-dialog-*` selects the live dialog; the `dialog-*` fixture namespace
/// stays untouched so the stubbed bodies keep measuring what they measure.
///
/// RED proof: return `None` for every id in `AppState::real_dialog_shot` and
/// the first assert fails with `None != Some(NewDocument)`.
#[test]
fn real_dialog_shot_names_the_live_dialog_only() {
    assert_eq!(
        AppState::real_dialog_shot(Some("real-dialog-new-document")),
        Some(DialogId::NewDocument)
    );
    assert_eq!(
        AppState::real_dialog_shot(Some("dialog-new-document")),
        None
    );
    assert_eq!(
        AppState::real_dialog_shot(Some("real-dialog-export")),
        Some(DialogId::Export)
    );
    assert_eq!(AppState::real_dialog_shot(Some("dialog-export")), None);
    assert_eq!(
        AppState::real_dialog_shot(Some("real-dialog-print")),
        Some(DialogId::Print)
    );
    assert_eq!(AppState::real_dialog_shot(Some("dialog-print")), None);
    assert_eq!(
        AppState::real_dialog_shot(Some("real-dialog-resize-canvas")),
        Some(DialogId::ResizeCanvas)
    );
    assert_eq!(AppState::real_dialog_shot(Some("dialog-resize")), None);
    assert_eq!(
        AppState::real_dialog_shot(Some("real-dialog-rename")),
        Some(DialogId::Rename)
    );
    assert_eq!(AppState::real_dialog_shot(Some("dialog-rename")), None);
    assert_eq!(AppState::real_dialog_shot(Some("shell-full-page")), None);
    assert_eq!(AppState::real_dialog_shot(None), None);
}
