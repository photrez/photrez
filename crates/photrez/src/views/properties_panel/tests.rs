use super::*;

#[test]
fn static_layer_values_pinned() {
    // The pins carry the new-document data model as measured from a created
    // 800x600 document (one full-canvas
    // "Background" image layer at 0,0 with locked position/rotation) — not
    // the older "Layer 1 · 100x100" seed.
    // ASCII escapes must render to the expected characters.
    assert_eq!(LAYER_NAME, "Background");
    assert!(LAYER_SUBTITLE.contains('\u{b7}'));
    assert!(LAYER_SUBTITLE.contains('\u{d7}'));
    assert_eq!(LAYER_SUBTITLE, "Image layer \u{b7} 800 \u{d7} 600 px");
    assert_eq!(POS_X, "0");
    assert_eq!(POS_Y, "0");
    assert_eq!(SIZE_W, "800");
    assert_eq!(SIZE_H, "600");
    assert_eq!(ROTATION, "0");
    assert_eq!(SCALE_X, "100");
    assert_eq!(SCALE_Y, "100");
    assert_eq!(OPACITY, "100 %");
}

#[test]
fn action_table_matches_reference() {
    assert_eq!(ACTIONS.len(), 3);
    assert_eq!(ACTIONS[0].0, icons::FLIP_H);
    assert_eq!(t(ACTIONS[0].1), "Flip H");
    assert_eq!(ACTIONS[1].0, icons::FLIP_V);
    assert_eq!(t(ACTIONS[1].1), "Flip V");
    assert_eq!(ACTIONS[2].0, icons::ROTATE_CCW);
    assert_eq!(t(ACTIONS[2].1), "Reset");
}

#[test]
fn quick_icons_match_reference() {
    assert_eq!(QUICK_ICONS.len(), 4);
    assert_eq!(QUICK_ICONS[0], icons::ALIGN_H);
    assert_eq!(QUICK_ICONS[1], icons::ALIGN_V);
    assert_eq!(QUICK_ICONS[2], icons::SWAP);
    assert_eq!(QUICK_ICONS[3], icons::BOX);
}

/// A canvas action row lifts to its own border tier on hover, and the delete
/// row keeps the red pair the design gives it: 10% at rest, 20% hovered, with
/// the lighter red label and glyph.
#[test]
fn canvas_action_rows_carry_their_hover_and_label_tiers() {
    let (fill, hover, text, icon) = ActionTone::Field.colours();
    assert_eq!(fill, color::FIELD);
    assert_eq!(
        hover,
        color::FIELD_BORDER,
        "a field row hovers to its own border tier"
    );
    assert_eq!(text, color::TEXT);
    assert_eq!(icon, color::TEXT_DIM);

    let (fill, hover, text, icon) = ActionTone::Destructive.colours();
    let panel = crate::tokens::hsla_to_rgb(color::PANEL);
    let over = |tint: gpui_kit::Hsla| {
        let rgb = crate::tokens::hsla_to_rgb(tint);
        let mix =
            |t: u8, base: u8| (t as f32 * tint.a + base as f32 * (1.0 - tint.a)).round() as u8;
        (
            mix(rgb.0, panel.0),
            mix(rgb.1, panel.1),
            mix(rgb.2, panel.2),
        )
    };
    assert_eq!(
        over(fill),
        (52, 31, 32),
        "the resting delete row is red at 10% over the panel"
    );
    assert_eq!(
        over(hover),
        (74, 33, 35),
        "the hovered delete row is red at 20% over the panel"
    );
    assert_eq!(crate::tokens::hsla_to_rgb(text), (248, 113, 113));
    assert_eq!(icon, text, "the trash glyph takes the label's red");
}
