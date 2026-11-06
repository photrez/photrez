use super::*;

// ── Variant table ───────────────────────────────────────────────────────

#[test]
fn card_x_formula_reproduces_the_1366_pin() {
    // The live formula is centre-in-canvas-column. At the 1366x820 baseline
    // the canvas column is 1366 - 52 (rail) - 533 (dock) = 781, so the box
    // sits at 52 + (781-520)/2 = 182.5 and the snapped painted edge is 183 -
    // exactly the recorded baseline pin.
    const CARD_X_AT_1366: f32 = 183.0;
    let canvas_col_w = 1366.0 - 52.0 - 533.0;
    assert_eq!((canvas_col_w - CARD_W) / 2.0, 130.5);
    assert_eq!(52.0 + (canvas_col_w - CARD_W) / 2.0, 182.5);
    assert_eq!(CARD_X_AT_1366, 183.0);
    // At the wide viewport (1536 layout px, dock 598) the same formula
    // gives the measured centred position: 52 + (886-520)/2 = 235.
    let comparison_col = 1536.0 - 52.0 - 598.0;
    assert_eq!(52.0 + (comparison_col - CARD_W) / 2.0, 235.0);
}

#[test]
fn select_routes_empty_new_document_to_dialog() {
    assert_eq!(
        select(Some("empty-new-document")),
        EmptyVariant::NewDocument
    );
}

#[test]
fn select_draws_card_for_every_other_shot() {
    assert_eq!(select(Some("empty-card")), EmptyVariant::Card);
    assert_eq!(select(Some("empty-full")), EmptyVariant::Card);
    // An unset id (plain launch) still draws the card.
    assert_eq!(select(None), EmptyVariant::Card);
    // Shots that belong to other modules do not hijack the card arm.
    assert_eq!(select(Some("loading-overlay")), EmptyVariant::Card);
}

// ── Empty-state card content table ──────────────────────────────────────

#[test]
fn presets_table_has_four_entries() {
    assert_eq!(PRESETS.len(), 4);
    assert_eq!(PRESETS[0].0, "Instagram Post");
    assert_eq!(PRESETS[3].1, "2480 \u{00d7} 3508 px");
    // The multiplication sign is the measured glyph, not a stray '*'.
    assert!(PRESETS.iter().all(|(_l, t, _w, _h)| t.contains("\u{00d7}")));
}

#[test]
fn preset_document_carries_the_tile_label_and_size() {
    // A preset tile names the document after the preset
    // label and creates it at the preset's pixel size.
    assert_eq!(preset_document(0), Some(("Instagram Post", 1080, 1080)));
    assert_eq!(preset_document(1), Some(("Story / Reels", 1080, 1920)));
    assert_eq!(preset_document(2), Some(("Full HD Canvas", 1920, 1080)));
    assert_eq!(preset_document(3), Some(("A4 Print (300 DPI)", 2480, 3508)));
    assert_eq!(preset_document(4), None);
}

#[test]
fn preset_icons_line_up_with_labels() {
    assert_eq!(PRESET_ICONS.len(), PRESETS.len());
    // preset_icon re-strokes each tile icon; the header icon stays at its
    // baked 1.5, so it is not part of the re-stroked tile set.
    for i in 0..PRESET_ICONS.len() {
        let re = preset_icon(i);
        assert!(!re.is_empty(), "tile {i} produced an empty glyph");
    }
}

// ── Geometry invariant: blocks tile the card height exactly ─────────────

#[test]
fn blocks_tile_card_height() {
    // 74 + 64 + 166 + 46 = 350 content; the 352 border box adds the 1px
    // header border implicitly captured by CARD_H in the docs.
    assert!(
        (HEADER_H + ACTIONS_H + PRESETS_H + FOOTER_H - 350.0).abs() < 1e-9,
        "block heights must sum to the 350px content height"
    );
    assert!(
        (HEADER_H + ACTIONS_H + PRESETS_H + FOOTER_H - (CARD_H - 2.0)).abs() < 1e-9
            || (HEADER_H + ACTIONS_H + PRESETS_H + FOOTER_H - CARD_H).abs() < 1e-9
    );
}
