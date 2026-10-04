// SPDX-License-Identifier: AGPL-3.0-or-later
// The nested per-layer payloads of the canonical document model: basic
// adjustments, shape params, and text data.
//
// Split out of `canonical_model.rs` to keep that file inside the size guard.
// These are the types that carry all user-typed content on a layer, so they are
// also the ones the save round-trip must prove most carefully.
//
// EVERY FIELD HERE HAS A SERDE DEFAULT, and that is load bearing rather than
// defensive. The loader (`editorOpenImage.ts` `loadProjectFile`) raw-spreads a
// parsed document into the engine via `restoreSnapshot` and never normalises
// nested layer params; `normalizeTextData` runs only on the write paths
// (`document.ts` `updateTextData`, `layerOps.ts`). So a file whose layer
// predates one of these fields arrives at the save path with the field
// genuinely absent. Without a default the Rust parse fails, the save returns
// `E_VALIDATION`, and NOTHING is written - while the file itself loads and
// renders correctly, and autosave folds the failure into a status flag.

use serde::{Deserialize, Serialize};

// --- Unions used by the nested payloads ---

/// types.ts:38-48 - the shape `kind` union (10 values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeKind {
    Rect,
    Ellipse,
    Line,
    Triangle,
    Star,
    #[serde(rename = "block-arrow")]
    BlockArrow,
    Heart,
    Diamond,
    #[serde(rename = "speech-bubble")]
    SpeechBubble,
    Hexagon,
}

/// types.ts:57 - ShapeFill.kind union ("none" | "solid").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeFillKind {
    None,
    Solid,
}

/// textTypes.ts:18 - TextData.fontStyle union.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextFontStyle {
    Normal,
    Italic,
}

/// textTypes.ts:20 - TextData.align union.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// textTypes.ts:23 - TextData.boxMode union.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextBoxMode {
    Point,
    Area,
}

/// textTypes.ts:5 - TextStroke.align union.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextStrokeAlign {
    Outside,
    Center,
    Inside,
}

// --- Structs ---

/// layerAdjustments.ts:7-11 - BasicAdjustment (brightness / contrast / saturation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BasicAdjustment {
    pub brightness: f64,
    pub contrast: f64,
    pub saturation: f64,
}

impl Default for BasicAdjustment {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
        }
    }
}

/// types.ts:50-54 - ShapeStroke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShapeStroke {
    pub enabled: bool,
    pub color: String,
    pub width: f64,
}

impl Default for ShapeStroke {
    fn default() -> Self {
        Self {
            enabled: false,
            color: "#000000".to_string(),
            width: 1.0,
        }
    }
}

/// types.ts:56-59 - ShapeFill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShapeFill {
    pub kind: ShapeFillKind,
    pub color: String,
}

impl Default for ShapeFill {
    fn default() -> Self {
        Self {
            kind: ShapeFillKind::None,
            color: "#000000".to_string(),
        }
    }
}

/// types.ts:61-69 - ShapeParams.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShapeParams {
    pub kind: ShapeKind,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
    pub fill: ShapeFill,
    pub stroke: ShapeStroke,
    pub arrow_head: bool,
}

impl Default for ShapeParams {
    fn default() -> Self {
        Self {
            kind: ShapeKind::Rect,
            width: 0.0,
            height: 0.0,
            radius: 0.0,
            fill: ShapeFill::default(),
            stroke: ShapeStroke::default(),
            arrow_head: false,
        }
    }
}

/// textTypes.ts:7-11 - TextStroke (outline; width 0 = none).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextStroke {
    pub width: f64,
    pub color: String,
    pub align: Option<TextStrokeAlign>,
}

impl Default for TextStroke {
    fn default() -> Self {
        Self {
            width: 0.0,
            color: "#000000".to_string(),
            align: Some(TextStrokeAlign::Outside),
        }
    }
}

/// textTypes.ts:13-30 - TextData (all fields, incl. nested stroke).
///
/// Defaults mirror `DEFAULT_TEXT_DATA` in `textTypes.ts`. See the module header
/// for why that is required rather than merely prudent.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextData {
    pub content: String,
    pub font_family: String,
    pub font_size: f64,
    pub font_weight: f64,
    pub font_style: TextFontStyle,
    pub color: String,
    pub align: TextAlign,
    pub line_height: f64,
    pub letter_spacing: f64,
    pub box_mode: TextBoxMode,
    pub box_width: f64,
    pub box_height: f64,
    pub stroke: TextStroke,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
    pub uppercase: Option<bool>,
}

impl Default for TextData {
    fn default() -> Self {
        Self {
            content: String::new(),
            font_family: "Arial".to_string(),
            font_size: 48.0,
            font_weight: 400.0,
            font_style: TextFontStyle::Normal,
            color: "#000000".to_string(),
            align: TextAlign::Left,
            line_height: 1.2,
            letter_spacing: 0.0,
            box_mode: TextBoxMode::Point,
            box_width: 0.0,
            box_height: 0.0,
            stroke: TextStroke::default(),
            underline: Some(false),
            strikethrough: Some(false),
            uppercase: Some(false),
        }
    }
}

impl<'de> Deserialize<'de> for TextData {
    /// Derive the struct, then apply the `boxMode`-conditional box dimensions
    /// that `normalizeTextData` (textTypes.ts) applies on the TypeScript write
    /// paths: point mode forces both to 0; area mode floors `boxWidth` at 1.
    ///
    /// A container `Default` cannot express this, because the rule depends on a
    /// field that may itself come from the file. Without it, a legacy
    /// area-mode text layer with no `boxWidth` would be written as 0 and then
    /// read back as 1, silently changing the user's text box on one
    /// save/reload cycle with no edit.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", default)]
        struct Raw {
            content: String,
            font_family: String,
            font_size: f64,
            font_weight: f64,
            font_style: TextFontStyle,
            color: String,
            align: TextAlign,
            line_height: f64,
            letter_spacing: f64,
            box_mode: TextBoxMode,
            /// `Option` so an ABSENT boxWidth can be told apart from an explicit
            /// 0, which is what lets area mode floor it at 1.
            box_width: Option<f64>,
            box_height: Option<f64>,
            stroke: TextStroke,
            underline: Option<bool>,
            strikethrough: Option<bool>,
            uppercase: Option<bool>,
        }

        // Mirrors `TextData::default()`, so a file predating any field still
        // parses. This is the whole point: the loader never normalises
        // textData, so absent fields are a normal input, not an error.
        impl Default for Raw {
            fn default() -> Self {
                let d = TextData::default();
                Self {
                    content: d.content,
                    font_family: d.font_family,
                    font_size: d.font_size,
                    font_weight: d.font_weight,
                    font_style: d.font_style,
                    color: d.color,
                    align: d.align,
                    line_height: d.line_height,
                    letter_spacing: d.letter_spacing,
                    box_mode: d.box_mode,
                    box_width: None,
                    box_height: None,
                    stroke: d.stroke,
                    underline: d.underline,
                    strikethrough: d.strikethrough,
                    uppercase: d.uppercase,
                }
            }
        }

        let raw = Raw::deserialize(d)?;
        let base = TextData::default();
        let (box_width, box_height) = match raw.box_mode {
            // Point mode forces BOTH to 0, exactly as normalizeTextData does.
            TextBoxMode::Point => (0.0, 0.0),
            // Area mode floors boxWidth at 1 and clamps boxHeight at 0.
            TextBoxMode::Area => (
                raw.box_width.unwrap_or(1.0).max(1.0),
                raw.box_height.unwrap_or(0.0).max(0.0),
            ),
        };
        Ok(TextData {
            content: raw.content,
            font_family: raw.font_family,
            font_size: raw.font_size,
            font_weight: raw.font_weight,
            font_style: raw.font_style,
            color: raw.color,
            align: raw.align,
            line_height: raw.line_height,
            letter_spacing: raw.letter_spacing,
            box_mode: raw.box_mode,
            box_width,
            box_height,
            stroke: raw.stroke,
            underline: raw.underline.or(base.underline),
            strikethrough: raw.strikethrough.or(base.strikethrough),
            uppercase: raw.uppercase.or(base.uppercase),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
