// SPDX-License-Identifier: AGPL-3.0-or-later
// The typed canonical Rust document model: layers, transforms, text, shape
// params, and adjustments.
//
// CONSUMED BY PRODUCTION through `ptz_document.rs`, which wraps these types in
// the on-disk `.ptz` `document.json` payload. So the document CONTENT is on
// the save path, while the transport/session envelope (header, viewport,
// active layer, per-layer bitmap keys) lives in `ptz_document.rs` — those are
// editor state, which the content model deliberately excludes.
//
// The v4 writer `to_ptz_document_json` below is fenced to `#[cfg(test)]`: the
// production writer is `PtzDocument::to_json`. See the note on that function.
//
// Every field/enum is mirrored from the ACTUAL TypeScript source of truth:
//   - apps/desktop/src/engine/types.ts        (LayerNode, Transform2D, BlendMode,
//                                              ShapeKind, ShapeParams, SelectionState,
//                                              DocumentModel, ViewportState, LayerId)
//   - apps/desktop/src/engine/layerAdjustments.ts (BasicAdjustment)
//   - apps/desktop/src/engine/textTypes.ts    (TextData, TextStroke, unions)
//
// Wire format: camelCase (matches the existing Rust protocol convention used by
// command.rs / model.rs / document_core.rs, and matches types.ts field names
// verbatim so .ptz stays compatible). Enum string values are the EXACT TS strings.
//
// EXCLUDED FIELDS (deliberate, with reason):
//   - LayerNode.bitmapEpoch (types.ts:105): transient in-memory freshness epoch,
//     explicitly "not persisted to .ptz" per the source comment. Not document content.
//   - LayerNode.imageBitmap / baseImageBitmap (types.ts:85,92): live pixel
//     references (ImageBitmap | null). Pixels are NEVER stored in the model.
//     Replaced by `resource_id: Option<ResourceId>` (a token handle mirroring
//     RenderLayer.resource_id in model.rs:23).
//   - DocumentModel.activeLayerId (types.ts:136): editor session state, not
//     document CONTENT. It IS persisted -- `PtzDocument` in `ptz_document.rs`
//     carries it on the file envelope, and the loader restores it.
//   - DocumentModel.viewport (types.ts:138): same split. Persisted on the
//     envelope, not on the content model.
//   - DocumentModel.dirty (types.ts:140): save-state flag, likewise persisted on
//     the envelope and cleared by the loader after restore.
// So these three are excluded from the CONTENT model and included in the FILE —
// which is why the exclusion list and the `.ptz` payload are not the same set.

use crate::command::ResourceId;
use serde::{Deserialize, Serialize};

// The nested per-layer payloads (basic adjustment, shape params, text data) and
// the unions they use live in `canonical_layer_params`, re-exported here so the
// canonical model's imports stay in one place.
pub use crate::canonical_layer_params::{
    BasicAdjustment, ShapeFill, ShapeFillKind, ShapeKind, ShapeParams, ShapeStroke, TextAlign,
    TextBoxMode, TextData, TextFontStyle, TextStroke, TextStrokeAlign,
};

// --- Small string unions (mirrored from types.ts / textTypes.ts) ---

/// types.ts:75 - the layer `type` union (5 values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayerType {
    Raster,
    Adjustment,
    Group,
    Shape,
    Text,
}

/// types.ts:12-24 - the 12 blend modes, exact TS string values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    #[serde(rename = "color-dodge")]
    ColorDodge,
    #[serde(rename = "color-burn")]
    ColorBurn,
    #[serde(rename = "soft-light")]
    SoftLight,
    #[serde(rename = "hard-light")]
    HardLight,
    Difference,
    Exclusion,
}

/// types.ts:116 - SelectionState.shape union.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelectionShape {
    Rect,
    Ellipse,
}

/// types.ts:27-35 - Transform2D (includes flipH / flipV).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transform2D {
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    pub flip_h: bool,
    pub flip_v: bool,
}

/// types.ts:109-119 - SelectionState.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionState {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    pub shape: Option<SelectionShape>,
    pub inverted: Option<bool>,
}

/// types.ts:72-106 - CanonicalLayer. Every LayerNode field, TYPED.
/// Pixels are NOT stored: raster pixel references (imageBitmap /
/// baseImageBitmap) are represented by `resource_id` (a token handle, mirroring
/// RenderLayer.resource_id). `bitmapEpoch` is intentionally excluded (transient).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalLayer {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub layer_type: LayerType,
    pub visible: bool,
    pub opacity: f64,
    pub locked: bool,
    #[serde(default)]
    pub is_background: Option<bool>,
    #[serde(default)]
    pub lock_transparency: Option<bool>,
    #[serde(default)]
    pub lock_position: Option<bool>,
    #[serde(default)]
    pub lock_rotation: Option<bool>,
    #[serde(default)]
    pub has_adjustments: Option<bool>,
    #[serde(default)]
    pub basic_adjustment: Option<BasicAdjustment>,
    /// Token handle to the layer's pixel resource. Mirrors RenderLayer.resource_id.
    /// None for non-raster layers. Replaces imageBitmap / baseImageBitmap.
    /// Absent in v3 saved files (which carry no resource tokens); deserializes to None.
    #[serde(default)]
    pub resource_id: Option<ResourceId>,
    pub blend_mode: BlendMode,
    pub transform: Transform2D,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub shape_params: Option<ShapeParams>,
    #[serde(default)]
    pub text_data: Option<TextData>,
}

/// types.ts:130-140 - CanonicalDocument.
/// EXCLUDES activeLayerId + viewport (UI/session state) and dirty
/// (save-state flag). Includes selection per the brief.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalDocument {
    pub id: String,
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub layers: Vec<CanonicalLayer>,
    #[serde(default)]
    pub selection: Option<SelectionState>,
}

impl CanonicalDocument {
    /// Parse a v3 `document.json` (photrez-ptz) into the canonical model.
    ///
    /// Real saved files carry session/transport fields that are not part of the
    /// canonical document content (`format`, `version`, `activeLayerId`,
    /// `viewport`, `dirty`) and per-layer pixel references (`imageBitmap`,
    /// `baseImageBitmap`). The deserializer ignores these via its default
    /// field behavior. Optional layer/document fields that are absent from the
    /// file deserialize to `None`. Malformed JSON returns `Err` (never panics).
    pub fn from_ptz_document_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    /// Serialize the canonical document to a v4 `.ptz` `document.json` payload.
    ///
    /// NOT THE PRODUCTION WRITER. The production writer is `PtzDocument::to_json`
    /// in `ptz_document.rs`. This one is fenced to `#[cfg(test)]` on purpose: it
    /// emits `version: 4`, and the loader in `editorOpenImage.ts` shows a
    /// "saved by a newer Photrez version" toast for any `version > 3`. It also
    /// cannot carry `viewport` or `activeLayerId`, which `CanonicalDocument`
    /// deliberately excludes as editor state, so a file written by it would
    /// silently reset the viewport and active layer on open. Wiring this into a
    /// save path would ship both. Kept compiling under test only so the shape it
    /// describes stays exercised; see the pending-deletion note in
    /// `ptz_document.rs`.
    ///
    /// RESOURCE HANDLE STRIPPING: a `ResourceId` is a session-scoped token handle.
    /// Persisting it would be a lie the next session cannot honor (the handle is
    /// minted fresh per session), so every layer's `resource_id` is normalized to
    /// `None` before serialization.
    #[cfg(test)]
    pub fn to_ptz_document_json(&self) -> Result<String, String> {
        let mut normalized = self.clone();
        for layer in normalized.layers.iter_mut() {
            layer.resource_id = None;
        }
        let mut value = serde_json::to_value(&normalized).map_err(|e| e.to_string())?;
        if let serde_json::Value::Object(ref mut map) = value {
            map.insert(
                "format".to_string(),
                serde_json::Value::String("photrez-ptz".to_string()),
            );
            map.insert(
                "version".to_string(),
                serde_json::Value::Number(serde_json::Number::from(4u64)),
            );
        }
        serde_json::to_string(&value).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn assert_enum_json<T: Serialize>(value: &T, expected: &str) {
        let json = serde_json::to_value(value).expect("enum serializes");
        assert_eq!(
            json,
            Value::String(expected.to_string()),
            "enum variant did not serialize to exact TS string"
        );
    }

    #[test]
    fn blend_mode_round_trips_to_exact_ts_strings() {
        assert_enum_json(&BlendMode::Normal, "normal");
        assert_enum_json(&BlendMode::Multiply, "multiply");
        assert_enum_json(&BlendMode::Screen, "screen");
        assert_enum_json(&BlendMode::Overlay, "overlay");
        assert_enum_json(&BlendMode::Darken, "darken");
        assert_enum_json(&BlendMode::Lighten, "lighten");
        assert_enum_json(&BlendMode::ColorDodge, "color-dodge");
        assert_enum_json(&BlendMode::ColorBurn, "color-burn");
        assert_enum_json(&BlendMode::SoftLight, "soft-light");
        assert_enum_json(&BlendMode::HardLight, "hard-light");
        assert_enum_json(&BlendMode::Difference, "difference");
        assert_enum_json(&BlendMode::Exclusion, "exclusion");

        // Deserialize back, proving the string is the canonical wire value.
        for s in [
            "normal",
            "multiply",
            "screen",
            "overlay",
            "darken",
            "lighten",
            "color-dodge",
            "color-burn",
            "soft-light",
            "hard-light",
            "difference",
            "exclusion",
        ] {
            let v: BlendMode = serde_json::from_str(&format!("\"{}\"", s)).unwrap();
            assert_eq!(
                serde_json::to_value(&v).unwrap(),
                Value::String(s.to_string())
            );
        }
    }

    #[test]
    fn layer_type_round_trips_to_exact_ts_strings() {
        assert_enum_json(&LayerType::Raster, "raster");
        assert_enum_json(&LayerType::Adjustment, "adjustment");
        assert_enum_json(&LayerType::Group, "group");
        assert_enum_json(&LayerType::Shape, "shape");
        assert_enum_json(&LayerType::Text, "text");
    }

    #[test]
    fn shape_kind_round_trips_to_exact_ts_strings() {
        assert_enum_json(&ShapeKind::Rect, "rect");
        assert_enum_json(&ShapeKind::Ellipse, "ellipse");
        assert_enum_json(&ShapeKind::Line, "line");
        assert_enum_json(&ShapeKind::Triangle, "triangle");
        assert_enum_json(&ShapeKind::Star, "star");
        assert_enum_json(&ShapeKind::BlockArrow, "block-arrow");
        assert_enum_json(&ShapeKind::Heart, "heart");
        assert_enum_json(&ShapeKind::Diamond, "diamond");
        assert_enum_json(&ShapeKind::SpeechBubble, "speech-bubble");
        assert_enum_json(&ShapeKind::Hexagon, "hexagon");
    }

    fn sample_transform() -> Transform2D {
        Transform2D {
            x: 10.0,
            y: 20.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 15.0,
            flip_h: false,
            flip_v: true,
        }
    }

    fn raster_layer() -> CanonicalLayer {
        CanonicalLayer {
            id: "layer-raster".to_string(),
            name: "Background".to_string(),
            layer_type: LayerType::Raster,
            visible: true,
            opacity: 1.0,
            locked: false,
            is_background: Some(true),
            lock_transparency: Some(false),
            lock_position: None,
            lock_rotation: None,
            has_adjustments: Some(true),
            basic_adjustment: Some(BasicAdjustment {
                brightness: 5.0,
                contrast: -10.0,
                saturation: 20.0,
            }),
            resource_id: Some(1),
            blend_mode: BlendMode::Normal,
            transform: sample_transform(),
            width: 800.0,
            height: 600.0,
            shape_params: None,
            text_data: None,
        }
    }

    fn shape_layer() -> CanonicalLayer {
        CanonicalLayer {
            id: "layer-shape".to_string(),
            name: "Star 1".to_string(),
            layer_type: LayerType::Shape,
            visible: true,
            opacity: 0.8,
            locked: false,
            is_background: None,
            lock_transparency: None,
            lock_position: None,
            lock_rotation: None,
            has_adjustments: None,
            basic_adjustment: None,
            resource_id: None,
            blend_mode: BlendMode::Multiply,
            transform: sample_transform(),
            width: 200.0,
            height: 200.0,
            shape_params: Some(ShapeParams {
                kind: ShapeKind::Star,
                width: 200.0,
                height: 200.0,
                radius: 12.0,
                fill: ShapeFill {
                    kind: ShapeFillKind::Solid,
                    color: "#E15A17".to_string(),
                },
                stroke: ShapeStroke {
                    enabled: true,
                    color: "#000000".to_string(),
                    width: 2.0,
                },
                arrow_head: false,
            }),
            text_data: None,
        }
    }

    fn text_layer() -> CanonicalLayer {
        CanonicalLayer {
            id: "layer-text".to_string(),
            name: "Title".to_string(),
            layer_type: LayerType::Text,
            visible: true,
            opacity: 1.0,
            locked: false,
            is_background: None,
            lock_transparency: None,
            lock_position: None,
            lock_rotation: None,
            has_adjustments: None,
            basic_adjustment: None,
            resource_id: None,
            blend_mode: BlendMode::Normal,
            transform: sample_transform(),
            width: 400.0,
            height: 80.0,
            shape_params: None,
            text_data: Some(TextData {
                content: "Hello".to_string(),
                font_family: "Arial".to_string(),
                font_size: 48.0,
                font_weight: 700.0,
                font_style: TextFontStyle::Italic,
                color: "#000000".to_string(),
                align: TextAlign::Center,
                line_height: 1.2,
                letter_spacing: 0.0,
                box_mode: TextBoxMode::Area,
                box_width: 400.0,
                box_height: 80.0,
                stroke: TextStroke {
                    width: 3.0,
                    color: "#FF0000".to_string(),
                    align: Some(TextStrokeAlign::Outside),
                },
                underline: Some(false),
                strikethrough: Some(false),
                uppercase: Some(true),
            }),
        }
    }

    #[test]
    fn document_round_trip_preserves_all_layers() {
        let doc = CanonicalDocument {
            id: "doc-1".to_string(),
            name: "Sample".to_string(),
            width: 800.0,
            height: 600.0,
            layers: vec![raster_layer(), shape_layer(), text_layer()],
            selection: Some(SelectionState {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 50.0,
                angle: 0.0,
                shape: Some(SelectionShape::Rect),
                inverted: Some(false),
            }),
        };

        let json = serde_json::to_string(&doc).expect("serialize");
        let back: CanonicalDocument = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(doc, back, "round-trip must be value-identical");

        // Field-name compatibility check: JSON uses camelCase matching types.ts.
        let v: Value = serde_json::from_str(&json).unwrap();
        let obj = v.as_object().unwrap();
        assert!(
            !obj.contains_key("activeLayerId"),
            "activeLayerId must NOT be present"
        );
        assert!(
            !obj.contains_key("viewport"),
            "viewport must NOT be present"
        );
        assert!(!obj.contains_key("dirty"), "dirty must NOT be present");
        assert!(obj.contains_key("layers"));
        assert!(obj.contains_key("selection"));
    }

    /// Completeness guard: every LayerNode field from types.ts (lines 73-105)
    /// has a counterpart in CanonicalLayer. We fully populate one layer and
    /// assert the exact serialized key set, so no field is silently dropped.
    #[test]
    fn canonical_layer_is_superset_of_ts_layernode() {
        let layer = CanonicalLayer {
            id: "x".to_string(),
            name: "x".to_string(),
            layer_type: LayerType::Raster,
            visible: true,
            opacity: 1.0,
            locked: false,
            is_background: Some(true),
            lock_transparency: Some(false),
            lock_position: Some(false),
            lock_rotation: Some(false),
            has_adjustments: Some(true),
            basic_adjustment: Some(BasicAdjustment {
                brightness: 0.0,
                contrast: 0.0,
                saturation: 0.0,
            }),
            resource_id: Some(0),
            blend_mode: BlendMode::Normal,
            transform: sample_transform(),
            width: 1.0,
            height: 1.0,
            shape_params: Some(ShapeParams {
                kind: ShapeKind::Rect,
                width: 1.0,
                height: 1.0,
                radius: 0.0,
                fill: ShapeFill {
                    kind: ShapeFillKind::None,
                    color: "#000000".to_string(),
                },
                stroke: ShapeStroke {
                    enabled: false,
                    color: "#000000".to_string(),
                    width: 1.0,
                },
                arrow_head: false,
            }),
            text_data: Some(TextData {
                content: "".to_string(),
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
                stroke: TextStroke {
                    width: 0.0,
                    color: "#000000".to_string(),
                    align: Some(TextStrokeAlign::Outside),
                },
                underline: Some(false),
                strikethrough: Some(false),
                uppercase: Some(false),
            }),
        };

        let v: Value = serde_json::to_value(&layer).unwrap();
        let got: std::collections::BTreeSet<String> =
            v.as_object().unwrap().keys().cloned().collect();

        // Expected keys = every LayerNode field (types.ts:73-105) except the
        // excluded bitmapEpoch/imageBitmap/baseImageBitmap (those collapse into
        // `resourceId`). camelCase per the wire convention.
        let expected: std::collections::BTreeSet<String> = [
            "id",
            "name",
            "type",
            "visible",
            "opacity",
            "locked",
            "isBackground",
            "lockTransparency",
            "lockPosition",
            "lockRotation",
            "hasAdjustments",
            "basicAdjustment",
            "resourceId",
            "blendMode",
            "transform",
            "width",
            "height",
            "shapeParams",
            "textData",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        assert_eq!(
            got, expected,
            "CanonicalLayer key set must equal LayerNode superset"
        );
    }
}

#[cfg(test)]
mod ptz_reader_tests {
    use super::*;
    // Shared with `ptz_document.rs` and the desktop command tests via
    // `crate::ptz_fixtures`, so there is ONE definition of this dump.
    use crate::ptz_fixtures::REAL_DUMPED_MODEL;

    #[test]
    fn parses_real_dumped_model() {
        let doc = CanonicalDocument::from_ptz_document_json(REAL_DUMPED_MODEL)
            .expect("real dumped model must parse without error");

        assert_eq!(doc.layers.len(), 2, "real doc has 2 layers");

        let painted = doc
            .layers
            .iter()
            .find(|l| l.name == "Painted")
            .expect("Painted layer present");
        assert_eq!(painted.layer_type, LayerType::Raster);
        assert_eq!(painted.blend_mode, BlendMode::Multiply);
        assert_eq!(painted.opacity, 0.75);
        assert!(painted.locked);
        assert_eq!(painted.lock_transparency, Some(true));
        assert_eq!(painted.transform.scale_x, 1.0);
        assert_eq!(painted.resource_id, None, "v3 file has no resource token");

        let background = doc
            .layers
            .iter()
            .find(|l| l.name == "Background")
            .expect("Background layer present");
        assert_eq!(background.is_background, Some(true));
        assert_eq!(background.lock_position, Some(true));
        assert_eq!(background.lock_rotation, Some(true));

        let sel = doc.selection.as_ref().expect("selection parsed");
        assert_eq!(sel.shape, Some(SelectionShape::Rect));
        assert_eq!(sel.inverted, None);
    }

    #[test]
    fn tolerates_missing_optional_fields() {
        let json = r#"{"id":"d","name":"n","width":10,"height":10,"layers":[{"id":"l","name":"L","type":"raster","visible":true,"opacity":1,"locked":false,"blendMode":"normal","transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},"width":10,"height":10}]}"#;
        let doc = CanonicalDocument::from_ptz_document_json(json).expect("minimal doc must parse");
        assert_eq!(doc.layers.len(), 1);
        let l = &doc.layers[0];
        assert_eq!(l.id, "l");
        assert_eq!(l.is_background, None);
        assert_eq!(l.lock_transparency, None);
        assert_eq!(l.lock_position, None);
        assert_eq!(l.lock_rotation, None);
        assert_eq!(l.has_adjustments, None);
        assert_eq!(l.basic_adjustment, None);
        assert_eq!(l.resource_id, None);
        assert_eq!(l.shape_params, None);
        assert_eq!(l.text_data, None);
        assert_eq!(doc.selection, None);
    }

    #[test]
    fn rejects_malformed_json() {
        let res = CanonicalDocument::from_ptz_document_json("{ not json");
        assert!(res.is_err(), "malformed JSON must return Err, not panic");
    }

    #[test]
    fn ignores_unknown_fields() {
        let json = r#"{"id":"d","name":"n","width":10,"height":10,"bogusTop":123,"layers":[{"id":"l","name":"L","type":"raster","visible":true,"opacity":1,"locked":false,"blendMode":"normal","transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},"width":10,"height":10,"bogusLayer":"x"}]}"#;
        let doc = CanonicalDocument::from_ptz_document_json(json)
            .expect("unknown top-level and layer keys must be ignored");
        assert_eq!(doc.layers.len(), 1);
    }
}

/// Write-side tests for `to_ptz_document_json` (v4 `.ptz` `document.json`).
#[cfg(test)]
mod write_tests {
    use super::*;

    fn sample_transform() -> Transform2D {
        Transform2D {
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        }
    }

    fn raster() -> CanonicalLayer {
        CanonicalLayer {
            id: "w-raster".to_string(),
            name: "Bg".to_string(),
            layer_type: LayerType::Raster,
            visible: true,
            opacity: 1.0,
            locked: false,
            is_background: Some(true),
            lock_transparency: None,
            lock_position: None,
            lock_rotation: None,
            has_adjustments: None,
            basic_adjustment: None,
            resource_id: None,
            blend_mode: BlendMode::Normal,
            transform: sample_transform(),
            width: 800.0,
            height: 600.0,
            shape_params: None,
            text_data: None,
        }
    }

    fn shape() -> CanonicalLayer {
        CanonicalLayer {
            id: "w-shape".to_string(),
            name: "Star".to_string(),
            layer_type: LayerType::Shape,
            visible: true,
            opacity: 0.9,
            locked: false,
            is_background: None,
            lock_transparency: None,
            lock_position: None,
            lock_rotation: None,
            has_adjustments: None,
            basic_adjustment: None,
            resource_id: None,
            blend_mode: BlendMode::Multiply,
            transform: Transform2D {
                x: 1.0,
                y: 2.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
            },
            width: 100.0,
            height: 100.0,
            shape_params: Some(ShapeParams {
                kind: ShapeKind::Star,
                width: 100.0,
                height: 100.0,
                radius: 5.0,
                fill: ShapeFill {
                    kind: ShapeFillKind::Solid,
                    color: "#ffffff".to_string(),
                },
                stroke: ShapeStroke {
                    enabled: false,
                    color: "#000000".to_string(),
                    width: 1.0,
                },
                arrow_head: false,
            }),
            text_data: None,
        }
    }

    fn text() -> CanonicalLayer {
        CanonicalLayer {
            id: "w-text".to_string(),
            name: "Title".to_string(),
            layer_type: LayerType::Text,
            visible: true,
            opacity: 1.0,
            locked: false,
            is_background: None,
            lock_transparency: None,
            lock_position: None,
            lock_rotation: None,
            has_adjustments: None,
            basic_adjustment: None,
            resource_id: None,
            blend_mode: BlendMode::Normal,
            transform: sample_transform(),
            width: 300.0,
            height: 40.0,
            shape_params: None,
            text_data: Some(TextData {
                content: "Hi".to_string(),
                font_family: "Arial".to_string(),
                font_size: 24.0,
                font_weight: 400.0,
                font_style: TextFontStyle::Normal,
                color: "#000000".to_string(),
                align: TextAlign::Left,
                line_height: 1.2,
                letter_spacing: 0.0,
                box_mode: TextBoxMode::Point,
                box_width: 0.0,
                box_height: 0.0,
                stroke: TextStroke {
                    width: 0.0,
                    color: "#000000".to_string(),
                    align: None,
                },
                // `Some(false)`, not `None`: reading normalises an absent OR an
                // explicit `null` to `false`, matching `normalizeTextData` in
                // textTypes.ts. A `None` here could not survive a round trip.
                underline: Some(false),
                strikethrough: Some(false),
                uppercase: Some(false),
            }),
        }
    }

    fn doc3() -> CanonicalDocument {
        CanonicalDocument {
            id: "doc-w".to_string(),
            name: "Write".to_string(),
            width: 800.0,
            height: 600.0,
            layers: vec![raster(), shape(), text()],
            selection: Some(SelectionState {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
                angle: 5.0,
                shape: Some(SelectionShape::Ellipse),
                inverted: Some(false),
            }),
        }
    }

    #[test]
    fn to_ptz_round_trips_three_layer_doc() {
        let doc = doc3();
        let json = doc.to_ptz_document_json().expect("v4 serialize");
        let parsed = CanonicalDocument::from_ptz_document_json(&json).expect("v4 parse");
        assert_eq!(doc, parsed, "to_ptz -> from_ptz must be value-identical");
    }

    #[test]
    fn to_ptz_strips_resource_handles() {
        let mut doc = doc3();
        doc.layers[0].resource_id = Some(9);
        let json = doc.to_ptz_document_json().expect("v4 serialize");

        // The written JSON must not carry a session-scoped resource handle.
        let v: serde_json::Value = serde_json::from_str(&json).expect("written JSON is valid");
        let layers = v["layers"].as_array().expect("has layers");
        assert_eq!(
            layers[0]["resourceId"],
            serde_json::Value::Null,
            "resource handle must be null in output"
        );

        // The reader recovers the layer with resource_id None, rest intact.
        let parsed = CanonicalDocument::from_ptz_document_json(&json).expect("v4 parse");
        assert_eq!(parsed.layers[0].resource_id, None);
        assert_eq!(parsed.layers[0].id, doc.layers[0].id);
        assert_eq!(parsed.layers[0].name, doc.layers[0].name);
        assert_eq!(parsed.layers[0].width, doc.layers[0].width);
        assert_eq!(parsed.layers[1].resource_id, None);
    }

    #[test]
    fn to_ptz_emits_v4_and_is_reader_compatible() {
        let doc = doc3();
        let json = doc.to_ptz_document_json().expect("v4 serialize");
        let v: serde_json::Value = serde_json::from_str(&json).expect("written JSON is valid");
        assert_eq!(
            v["format"],
            serde_json::Value::String("photrez-ptz".to_string())
        );
        assert_eq!(
            v["version"],
            serde_json::Value::Number(serde_json::Number::from(4u64))
        );
        assert_eq!(v["layers"].as_array().map(|l| l.len()).unwrap_or(0), 3);

        // The existing reader (which ignores format/version) must accept it.
        let parsed = CanonicalDocument::from_ptz_document_json(&json);
        assert!(
            parsed.is_ok(),
            "v4 output must be readable by from_ptz_document_json"
        );
    }
}
