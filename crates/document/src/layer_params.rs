// SPDX-License-Identifier: AGPL-3.0-or-later
// The nested per-layer payloads of the canonical document model: basic
// adjustments, shape params, and text data.
//
// Split out of `content.rs` to keep that file inside the size guard.
// These are the types that carry all user-typed content on a layer, so they are
// also the ones the save round-trip must prove most carefully.
//
// EVERY FIELD HERE HAS A SERDE DEFAULT, and that is load bearing rather than
// defensive. Opening a document feeds the parsed payload straight into the
// engine without normalising nested layer params, and text data is normalised
// only on the paths that write it. So a file whose layer
// predates one of these fields arrives at the save path with the field
// genuinely absent. Without a default the Rust parse fails, the save returns
// `E_VALIDATION`, and NOTHING is written - while the file itself loads and
// renders correctly, and autosave folds the failure into a status flag.

use serde::{Deserialize, Serialize};

// --- Unions used by the nested payloads ---

/// The shape `kind` union (10 values).
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

/// ShapeFill.kind union ("none" | "solid").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeFillKind {
    None,
    Solid,
}

/// TextData.fontStyle union.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextFontStyle {
    Normal,
    Italic,
}

/// TextData.align union.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// TextData.boxMode union.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextBoxMode {
    Point,
    Area,
}

/// TextStroke.align union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextStrokeAlign {
    Outside,
    Center,
    Inside,
}

// --- Structs ---

/// BasicAdjustment (brightness / contrast / saturation).
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

/// ShapeStroke: enabled / color / width.
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

/// ShapeFill: kind + color.
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

/// ShapeParams: geometry plus fill and stroke.
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

/// TextStroke: outline; width 0 = none.
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

/// TextData: all persisted text fields, incl. the nested stroke.
///
/// The defaults below are the document-format text defaults, kept identical to
/// what saved documents already carry. See the module header
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
    /// that the document format enforces on the write paths: point mode forces
    /// both to 0; area mode floors `boxWidth` at 1.
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
            // Point mode forces BOTH to 0, exactly as the write path normalises.
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
