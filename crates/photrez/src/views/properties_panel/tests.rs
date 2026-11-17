use super::*;

#[test]
fn selected_layer_subtitle_pins_the_shape() {
    // The live Selected Layer card's subtitle carries the projected layer's
    // kind and source size; the properties and adjustments panels share this
    // shape.
    assert_eq!(
        selected_layer_subtitle("Image", 800, 600),
        "Image layer \u{b7} 800 \u{d7} 600 px"
    );
    assert_eq!(
        selected_layer_subtitle("Shape", 1080, 1080),
        "Shape layer \u{b7} 1080 \u{d7} 1080 px"
    );
    assert_eq!(
        selected_layer_subtitle("Text", 200, 48),
        "Text layer \u{b7} 200 \u{d7} 48 px"
    );
}

#[test]
fn opacity_rows_print_the_layer_not_a_specimen() {
    assert_eq!(transform::opacity_text(1.0), "100 %");
    assert_eq!(transform::opacity_text(0.6), "60 %");
    assert_eq!(transform::opacity_text(0.336), "34 %");
    assert_eq!(transform::opacity_percent(0.6), 60.);
    assert_eq!(transform::opacity_percent(1.4), 100.);
}

#[test]
fn transform_rows_read_the_layer_or_fall_back_to_the_baseline() {
    // The live branch is what a mounted document drives; the fallback is what
    // the engine-less shot path paints, and it keeps the baseline's own numbers.
    assert_eq!(transform::row_text(Some(2.4), transform::REST_W), "2");
    assert_eq!(transform::row_text(Some(39.6), transform::REST_X), "40");
    assert_eq!(transform::row_text(None, transform::REST_W), "800");
    assert_eq!(transform::row_text(None, transform::REST_H), "600");
    assert_eq!(transform::row_text(None, transform::REST_SCALE), "100");
}

#[test]
fn action_table_matches_the_pinned_rows() {
    assert_eq!(ACTIONS.len(), 3);
    assert_eq!(ACTIONS[0].0, icons::FLIP_H);
    assert_eq!(t(ACTIONS[0].1), "Flip H");
    assert_eq!(ACTIONS[1].0, icons::FLIP_V);
    assert_eq!(t(ACTIONS[1].1), "Flip V");
    assert_eq!(ACTIONS[2].0, icons::ROTATE_CCW);
    assert_eq!(t(ACTIONS[2].1), "Reset");
}

#[test]
fn quick_actions_match_the_pinned_rows_and_test_ids() {
    assert_eq!(QUICK_ACTIONS.len(), 5);
    assert_eq!(QUICK_ACTIONS[0].0, "prop-quick-center-h");
    assert_eq!(QUICK_ACTIONS[0].1, icons::ALIGN_H);
    assert_eq!(t(QUICK_ACTIONS[0].2), "Center horizontally on canvas");
    assert_eq!(QUICK_ACTIONS[1].0, "prop-quick-center-v");
    assert_eq!(QUICK_ACTIONS[1].1, icons::ALIGN_V);
    assert_eq!(t(QUICK_ACTIONS[1].2), "Center vertically on canvas");
    assert_eq!(QUICK_ACTIONS[2].0, "prop-quick-fit-canvas");
    assert_eq!(QUICK_ACTIONS[2].1, icons::MAXIMIZE);
    assert_eq!(t(QUICK_ACTIONS[2].2), "Fit to canvas (scale + center)");
    assert_eq!(QUICK_ACTIONS[3].0, "prop-quick-rotate-ccw");
    assert_eq!(QUICK_ACTIONS[3].1, icons::ROTATE_CCW);
    assert_eq!(t(QUICK_ACTIONS[3].2), "Rotate 90\u{b0} counterclockwise");
    assert_eq!(QUICK_ACTIONS[4].0, "prop-quick-rotate-cw");
    assert_eq!(QUICK_ACTIONS[4].1, icons::ROTATE_CW);
    assert_eq!(t(QUICK_ACTIONS[4].2), "Rotate 90\u{b0} clockwise");
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
