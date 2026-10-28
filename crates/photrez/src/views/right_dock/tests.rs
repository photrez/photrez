use super::*;

/// `tab_label` capitalises the first character of a tab id and leaves the
/// rest untouched. Empty input yields an empty string.
///
/// RED proof: remove the `to_uppercase()` call and every assertion flips.
#[test]
fn tab_label_capitalizes_first_char() {
    assert_eq!(tab_label("properties"), "Properties");
    assert_eq!(tab_label("adjustments"), "Adjustments");
    assert_eq!(tab_label("presets"), "Presets");
    assert_eq!(tab_label("layers"), "Layers");
    assert_eq!(tab_label("history"), "History");
    assert_eq!(tab_label(""), "");
}

/// `inspector_tab_id` maps each `InspectorTab` variant to the string id
/// the tab bar uses to light its button.
///
/// RED proof: swap any arm (e.g. `Adjustments` → "properties") and this
/// fails — `inspector_dock` uses this id to decide which tab is active.
#[test]
fn inspector_tab_id_maps_each_variant() {
    assert_eq!(inspector_tab_id(&InspectorTab::Properties), "properties");
    assert_eq!(inspector_tab_id(&InspectorTab::Adjustments), "adjustments");
    assert_eq!(inspector_tab_id(&InspectorTab::Presets), "presets");
}

/// `layer_tab_id` maps each `LayerTab` variant to its string id.
///
/// RED proof: swap any arm and this fails.
#[test]
fn layer_tab_id_maps_each_variant() {
    assert_eq!(layer_tab_id(&LayerTab::Layers), "layers");
    assert_eq!(layer_tab_id(&LayerTab::History), "history");
}

/// The id→label chain must agree: `tab_label(inspector_tab_id(tab))`
/// produces the display name the tab bar renders. If the two mappings
/// disagree, the lit tab's underline would sit under the wrong label.
///
/// RED proof: change `tab_label` to lowercase the rest and this fails.
#[test]
fn inspector_tab_label_round_trips() {
    assert_eq!(
        tab_label(inspector_tab_id(&InspectorTab::Properties)),
        "Properties"
    );
    assert_eq!(
        tab_label(inspector_tab_id(&InspectorTab::Adjustments)),
        "Adjustments"
    );
    assert_eq!(
        tab_label(inspector_tab_id(&InspectorTab::Presets)),
        "Presets"
    );
}

/// The id→enum direction must invert the enum→id direction: every tab id
/// the bar renders resolves back to the variant it stands for, and an
/// unknown id resolves to nothing.
///
/// This is what the click handler relies on — `write_inspector_tab` /
/// `write_layer_tab` look the clicked id up here before writing it onto
/// `AppState`.
///
/// RED proof: swap an arm of `inspector_tab_from_id` (e.g. `adjustments`
/// → `presets`) and this fails.
#[test]
fn tab_id_from_id_round_trips() {
    for tab in [
        InspectorTab::Properties,
        InspectorTab::Adjustments,
        InspectorTab::Presets,
    ] {
        assert_eq!(
            inspector_tab_from_id(inspector_tab_id(&tab)),
            Some(tab),
            "inspector tab {tab:?} must round-trip through its id"
        );
    }
    for tab in [LayerTab::Layers, LayerTab::History] {
        assert_eq!(
            layer_tab_from_id(layer_tab_id(&tab)),
            Some(tab),
            "layer tab {tab:?} must round-trip through its id"
        );
    }

    // Unknown ids are ignored by the writers, not silently mapped.
    assert_eq!(inspector_tab_from_id("nope"), None);
    assert_eq!(layer_tab_from_id("nope"), None);
    // An inspector id must not resolve as a layer tab.
    assert_eq!(layer_tab_from_id("adjustments"), None);
    assert_eq!(inspector_tab_from_id("layers"), None);
}

/// Pin the measured geometry constants from the baseline manifest.
///
/// RED proof: change any constant and this fails.
#[test]
fn geometry_constants_are_pinned() {
    assert_eq!(DOCK_W, 533.0);
    assert_eq!(DOCK_H, 750.0);
    assert_eq!(INSPECTOR_W, 272.0);
    assert_eq!(LAYER_W, 260.0);
    assert_eq!(TAB_BAR_H, 38.0);
    assert_eq!(SPLIT_BORDER, 1.0);
    assert_eq!(NAV_H, 185.0);
    assert_eq!(NAV_HEADER_H, 38.0);
    assert_eq!(UNDERLINE_H, 2.0);
    assert_eq!(TAB_PAD_X, 12.0);
    assert_eq!(TAB_TEXT, 11.5);

    // 272 + 1 + 260 = 533 — the dock's two-column split.
    assert!((INSPECTOR_W + SPLIT_BORDER + LAYER_W - DOCK_W).abs() < f32::EPSILON);

    // 38 (tab bar) + 527 (panel) + 185 (navigator) = 750 (dock height).
    assert!((TAB_BAR_H + 527.0 + NAV_H - DOCK_H).abs() < f32::EPSILON);

    // Navigator canvas: 185 - 38 (header) - 1 (border) = 146.
    assert_eq!(NAV_H - NAV_HEADER_H - SPLIT_BORDER, 146.0);
}

/// The active tab underline is 2px — matching `after:h-[2px]` in the TSX.
#[test]
fn underline_height_matches_tsx() {
    assert_eq!(UNDERLINE_H, 2.0);
}
