// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::ptz::layer_params::*;

/// A layer payload written before most of its fields existed must parse.
/// This is the case that hard-failed the save before these defaults existed.
#[test]
fn a_partial_text_data_parses_and_takes_defaults() {
    let json = r##"{"content":"legacy","fontFamily":"Arial","fontSize":36,
            "fontWeight":400,"fontStyle":"normal","color":"#000000","align":"left",
            "lineHeight":1.2}"##;
    let td: TextData = serde_json::from_str(json).expect("partial TextData must parse");
    assert_eq!(td.content, "legacy");
    assert_eq!(td.font_size, 36.0);
    assert_eq!(td.letter_spacing, 0.0);
    assert_eq!(td.box_mode, TextBoxMode::Point);
    assert_eq!(td.uppercase, Some(false));
}

#[test]
fn an_empty_text_data_is_all_defaults() {
    let td: TextData = serde_json::from_str("{}").expect("empty TextData");
    assert_eq!(td, TextData::default());
    assert_eq!(td.font_size, 48.0);
    assert_eq!(td.font_weight, 400.0);
}

#[test]
fn a_partial_shape_params_parses_and_takes_defaults() {
    let sp: ShapeParams =
        serde_json::from_str(r#"{"kind":"rect"}"#).expect("partial ShapeParams must parse");
    assert_eq!(sp.kind, ShapeKind::Rect);
    assert_eq!(sp.width, 0.0);
    assert_eq!(sp.fill.kind, ShapeFillKind::None);
    assert!(!sp.stroke.enabled);
}

#[test]
fn a_partial_basic_adjustment_parses_and_takes_defaults() {
    let adj: BasicAdjustment =
        serde_json::from_str(r#"{"brightness":5.0}"#).expect("partial BasicAdjustment");
    assert_eq!(adj.brightness, 5.0);
    assert_eq!(adj.contrast, 0.0);
    assert_eq!(adj.saturation, 0.0);
}

/// A present-but-INVALID enum inside a defaulted container must still be
/// rejected. `#[serde(default)]` only covers an ABSENT field; a field
/// present with a value outside the union must not silently downgrade to
/// the default, or a corrupt file would be rewritten as if it were valid.
#[test]
fn a_present_but_invalid_enum_is_still_rejected() {
    let bad_box_mode = r##"{"content":"x","boxMode":"sideways"}"##;
    assert!(
        serde_json::from_str::<TextData>(bad_box_mode).is_err(),
        "an unknown boxMode must not silently default to point"
    );

    let bad_stroke_align = r##"{"content":"x","stroke":{"align":"diagonal"}}"##;
    assert!(
        serde_json::from_str::<TextData>(bad_stroke_align).is_err(),
        "an unknown stroke.align must not silently default to outside"
    );

    let bad_fill_kind = r##"{"kind":"rect","fill":{"kind":"gradient"}}"##;
    assert!(
        serde_json::from_str::<ShapeParams>(bad_fill_kind).is_err(),
        "an unknown fill.kind must not silently default to none"
    );

    let bad_shape_kind = r##"{"kind":"hexagram"}"##;
    assert!(
        serde_json::from_str::<ShapeParams>(bad_shape_kind).is_err(),
        "an unknown shape kind must not silently default to rect"
    );

    let bad_font_style = r##"{"content":"x","fontStyle":"oblique"}"##;
    assert!(
        serde_json::from_str::<TextData>(bad_font_style).is_err(),
        "an unknown fontStyle must not silently default to normal"
    );
}

/// DELIBERATE leniency, recorded so it is not mistaken for an oversight: a
/// present-but-EMPTY `shapeParams: {}` now parses to all-defaults instead
/// of failing. That is accepted because `shapeParams` is `Option`, so `{}`
/// and an absent key are the same intent ("a shape layer with default
/// geometry"), and a hand-edited or third-party file carrying `{}` should
/// load and re-save rather than become unsaveable. The genuinely dangerous
/// case -- an invalid VALUE -- is still rejected, above.
#[test]
fn an_empty_shape_params_is_accepted_as_all_defaults() {
    let sp: ShapeParams = serde_json::from_str("{}").expect("empty ShapeParams");
    assert_eq!(sp, ShapeParams::default());
    assert_eq!(sp.kind, ShapeKind::Rect);
}

/// `boxWidth`/`boxHeight` follow `boxMode`, matching `normalizeTextData` in
/// `textTypes.ts`: point mode forces both to 0, area mode floors `boxWidth`
/// at 1. Without this a legacy area-mode text layer with no `boxWidth` would
/// be written as 0 and then mutated to 1 on the next load, so the value would
/// change across one save/reload cycle for no user action.
#[test]
fn box_dimensions_follow_box_mode_like_the_typescript_normaliser() {
    // Absent boxWidth in AREA mode floors to 1, not 0.
    let area = r##"{"content":"x","boxMode":"area"}"##;
    let td: TextData = serde_json::from_str(area).expect("area-mode TextData");
    assert_eq!(td.box_mode, TextBoxMode::Area);
    assert_eq!(td.box_width, 1.0, "area mode floors boxWidth at 1");
    assert_eq!(td.box_height, 0.0);

    // POINT mode forces both to 0 even if the file carried values.
    let point = r##"{"content":"x","boxMode":"point","boxWidth":320,"boxHeight":44}"##;
    let td: TextData = serde_json::from_str(point).expect("point-mode TextData");
    assert_eq!(td.box_width, 0.0, "point mode forces boxWidth to 0");
    assert_eq!(td.box_height, 0.0);

    // An explicit area-mode width is preserved, not floored away.
    let sized = r##"{"content":"x","boxMode":"area","boxWidth":320,"boxHeight":44}"##;
    let td: TextData = serde_json::from_str(sized).expect("sized area TextData");
    assert_eq!(td.box_width, 320.0);
    assert_eq!(td.box_height, 44.0);
}

/// Defaults must not mask a genuinely wrong type: that still has to fail,
/// or a corrupt file would be written back as if it were fine.
#[test]
fn a_wrong_typed_field_is_still_rejected() {
    let bad = r##"{"content":123,"fontFamily":"Arial","fontSize":36,"fontWeight":400,
            "fontStyle":"normal","color":"#000000","align":"left","lineHeight":1.2}"##;
    assert!(
        serde_json::from_str::<TextData>(bad).is_err(),
        "a wrong-typed TextData field must not be silently defaulted"
    );
}

/// A defaulted payload must re-serialize with every key present, so the
/// NEXT save of the same file is not lossy.
#[test]
fn a_defaulted_payload_round_trips_with_every_key() {
    let v = serde_json::to_value(TextData::default()).unwrap();
    assert_eq!(
        v.as_object().unwrap().len(),
        16,
        "TextData emits all 16 fields"
    );
}
